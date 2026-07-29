//! Native Host session authority for the cataloged control families.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use saccade_protocol::{
    ActionPayload, ActionReceipt, ActionRequest, ControlError, ControlRequest, ControlResponse,
    HostGrant, LocalAddress, NativeEnvelope, ObservationSnapshot, PreparedAction, HOST_PROTOCOL,
    SESSION_CAPABILITY_SCHEME,
};
use serde_json::{json, Value};

use crate::native_messaging;
use crate::platform_input::PlatformInput;
use crate::profile::Profile;
use crate::{ClosedLoopEngine, ClosedLoopError, NativeInput, ObservationSource};

const EXTENSION_TIMEOUT: Duration = Duration::from_secs(10);
const POST_ACTION_TIMEOUT: Duration = Duration::from_secs(2);
const POST_ACTION_QUIET_WINDOW: Duration = Duration::from_millis(300);
const SELECT_POST_ACTION_QUIET_WINDOW: Duration = Duration::from_millis(750);

pub trait ExtensionOutbound: Send + Sync {
    fn send(&self, message: &NativeEnvelope) -> Result<()>;
}

pub struct StdoutOutbound {
    lock: Mutex<()>,
}

impl StdoutOutbound {
    pub fn new() -> Self {
        Self {
            lock: Mutex::new(()),
        }
    }
}

impl Default for StdoutOutbound {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensionOutbound for StdoutOutbound {
    fn send(&self, message: &NativeEnvelope) -> Result<()> {
        let _guard = self.lock.lock().map_err(lock_error)?;
        native_messaging::write_message(&mut std::io::stdout().lock(), message)?;
        Ok(())
    }
}

pub struct NativeHostSession {
    capability: String,
    runtime_dir: PathBuf,
    endpoint: Mutex<Option<LocalAddress>>,
    browser_instance_id: Mutex<Option<String>>,
    extension_connected: AtomicBool,
    observations: Mutex<BTreeMap<String, ObservationSnapshot>>,
    observation_changed: Condvar,
    pending: Mutex<BTreeMap<u64, mpsc::Sender<Value>>>,
    next_request_id: AtomicU64,
    outbound: Arc<dyn ExtensionOutbound>,
    profile: Profile,
    engine: Mutex<ClosedLoopEngine>,
    native: Mutex<Box<dyn NativeInput>>,
}

impl NativeHostSession {
    pub fn new(runtime_dir: PathBuf) -> Result<Self> {
        let profile = Profile::load(&runtime_dir)?;
        Self::with_adapters_and_profile(
            runtime_dir,
            Arc::new(StdoutOutbound::new()),
            Box::new(PlatformInput),
            profile,
        )
    }

    pub fn with_adapters(
        runtime_dir: PathBuf,
        outbound: Arc<dyn ExtensionOutbound>,
        native: Box<dyn NativeInput>,
    ) -> Result<Self> {
        Self::with_adapters_and_profile(runtime_dir, outbound, native, Profile::default())
    }

    pub fn with_adapters_and_profile(
        runtime_dir: PathBuf,
        outbound: Arc<dyn ExtensionOutbound>,
        native: Box<dyn NativeInput>,
        profile: Profile,
    ) -> Result<Self> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes)
            .map_err(|error| anyhow!("failed to create host capability: {error}"))?;
        fs::create_dir_all(&runtime_dir)?;
        #[cfg(unix)]
        fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700))?;
        Ok(Self {
            capability: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes),
            runtime_dir,
            endpoint: Mutex::new(None),
            browser_instance_id: Mutex::new(None),
            extension_connected: AtomicBool::new(false),
            observations: Mutex::new(BTreeMap::new()),
            observation_changed: Condvar::new(),
            pending: Mutex::new(BTreeMap::new()),
            next_request_id: AtomicU64::new(1),
            outbound,
            profile,
            engine: Mutex::new(ClosedLoopEngine::builtin()?),
            native: Mutex::new(native),
        })
    }

    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    pub fn install_endpoint(&self, address: LocalAddress) -> Result<()> {
        *self.endpoint.lock().map_err(lock_error)? = Some(address);
        self.write_grant()
    }

    pub fn handle_native(&self, message: NativeEnvelope) -> Result<()> {
        if message.protocol != HOST_PROTOCOL {
            bail!("extension used the wrong host protocol");
        }
        match message.kind.as_str() {
            "hello" => self.handle_hello(message.payload),
            "observation" => self.handle_observation(message.payload),
            _ if message.request_id.is_some() => self.handle_extension_response(message),
            other => bail!("extension sent unsupported message kind {other}"),
        }
    }

    pub fn mark_extension_disconnected(&self) {
        self.extension_connected.store(false, Ordering::Release);
        self.observation_changed.notify_all();
    }

    pub fn handle_control(&self, request: ControlRequest) -> ControlResponse {
        if request.capability != self.capability {
            return control_error(
                request.id,
                "PERMISSION_DENIED",
                "invalid session capability",
            );
        }
        match self.dispatch_control(&request.method, request.params) {
            Ok(value) => ControlResponse {
                id: request.id,
                ok: true,
                result: Some(value),
                error: None,
            },
            Err(error) => control_error(request.id, "REQUEST_REJECTED", &error.to_string()),
        }
    }

    fn dispatch_control(&self, method: &str, params: Value) -> Result<Value> {
        match method {
            "system.capabilities" => Ok(json!({
                "schema":"saccade.capabilities/4",
                "observation_schema":saccade_protocol::OBSERVATION_SCHEMA,
                "host_protocol":HOST_PROTOCOL,
                "perception":"dom_extension",
                "input":"os_native",
                "native_accessibility_trusted":crate::platform_input::accessibility_trusted(),
                "browser_support":["chrome","edge"],
                "extension_connected":self.extension_connected.load(Ordering::Acquire),
                "first_slice":["button","text_field","checkbox","select"],
                "profile":{
                    "name":self.profile.name,
                    "behavior":self.profile.behavior
                }
            })),
            "web.observe" => {
                let tab_id = required_string(&params, "tab_id")?;
                Ok(serde_json::to_value(self.current_observation(tab_id)?)?)
            }
            "tabs.list" => self.request_extension("tabs.list", json!({}), EXTENSION_TIMEOUT),
            "tabs.open" => {
                let url = required_string(&params, "url")?;
                if url.len() > 8192 || !(url.starts_with("http://") || url.starts_with("https://"))
                {
                    bail!("url must use HTTP or HTTPS and stay within 8192 bytes");
                }
                let active = match params.get("active") {
                    Some(value) => value.as_bool().context("active must be a boolean")?,
                    None => true,
                };
                for key in params
                    .as_object()
                    .context("tabs.open params must be an object")?
                    .keys()
                {
                    if !["url", "active"].contains(&key.as_str()) {
                        bail!("unexpected tabs.open argument {key}");
                    }
                }
                self.request_extension(
                    "tabs.open",
                    json!({"url":url,"active":active}),
                    EXTENSION_TIMEOUT,
                )
            }
            "web.act" => self.act(params),
            _ => bail!("unknown host method {method}"),
        }
    }

    fn act(&self, params: Value) -> Result<Value> {
        let request: ActionRequest = serde_json::from_value(params)?;
        request.validate()?;
        let before = self.current_observation(&request.tab_id)?;
        if !before
            .objects
            .iter()
            .any(|object| object.action_token.as_deref() == Some(&request.action_token))
        {
            bail!("action token is not present in the current Profile-filtered observation");
        }
        let extension_payload = match &request.payload {
            ActionPayload::Select { .. } => serde_json::to_value(&request.payload)?,
            _ => json!({"kind":"none"}),
        };
        let prepared: PreparedAction = serde_json::from_value(self.request_extension(
            "prepare_action",
            json!({
                "browser_instance_id":request.browser_instance_id,
                "tab_id":request.tab_id,
                "document_id":request.document_id,
                "basis_revision":request.basis_revision,
                "action_token":request.action_token,
                "operation":request.operation,
                "payload":extension_payload
            }),
            EXTENSION_TIMEOUT,
        )?)?;

        let mut engine = self.engine.lock().map_err(lock_error)?;
        let mut native = self.native.lock().map_err(lock_error)?;
        let mut source = SessionObservationSource {
            session: self,
            tab_id: request.tab_id.clone(),
            basis_document_id: request.document_id.clone(),
            quiet_window: if request.operation == saccade_protocol::ActionOperation::Select {
                SELECT_POST_ACTION_QUIET_WINDOW
            } else {
                POST_ACTION_QUIET_WINDOW
            },
        };
        let receipt: ActionReceipt =
            engine.execute(&request, &before, &prepared, native.as_mut(), &mut source)?;
        Ok(serde_json::to_value(receipt)?)
    }

    fn request_extension(&self, kind: &str, payload: Value, timeout: Duration) -> Result<Value> {
        if !self.extension_connected.load(Ordering::Acquire) {
            bail!("extension is not connected");
        }
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();
        self.pending
            .lock()
            .map_err(lock_error)?
            .insert(request_id, sender);
        let message = NativeEnvelope {
            protocol: HOST_PROTOCOL.into(),
            kind: kind.into(),
            request_id: Some(request_id),
            payload,
        };
        if let Err(error) = self.outbound.send(&message) {
            self.pending.lock().map_err(lock_error)?.remove(&request_id);
            return Err(error);
        }
        let response = match receiver.recv_timeout(timeout) {
            Ok(response) => response,
            Err(_) => {
                self.pending.lock().map_err(lock_error)?.remove(&request_id);
                bail!("extension request {kind} timed out");
            }
        };
        if let Some(error) = response.get("error").and_then(Value::as_str) {
            bail!("extension rejected {kind}: {error}");
        }
        Ok(response)
    }

    fn handle_hello(&self, payload: Value) -> Result<()> {
        let instance = required_string(&payload, "browser_instance_id")?.to_string();
        if instance.len() > 256 {
            bail!("browser instance identity is too long");
        }
        let mut current = self.browser_instance_id.lock().map_err(lock_error)?;
        if current.as_deref().is_some_and(|value| value != instance) {
            bail!("native host cannot switch browser instances");
        }
        *current = Some(instance);
        self.extension_connected.store(true, Ordering::Release);
        drop(current);
        self.write_grant()
    }

    fn handle_observation(&self, payload: Value) -> Result<()> {
        let mut snapshot: ObservationSnapshot = serde_json::from_value(payload)?;
        snapshot.validate()?;
        let current_instance = self.browser_instance_id.lock().map_err(lock_error)?.clone();
        if current_instance.as_deref() != Some(&snapshot.browser_instance_id) {
            bail!("browser instance mismatch");
        }
        let mut observations = self.observations.lock().map_err(lock_error)?;
        if observations.get(&snapshot.tab_id).is_some_and(|previous| {
            previous.document_id == snapshot.document_id && snapshot.revision <= previous.revision
        }) {
            bail!("observation revision did not advance");
        }
        self.profile.filter_observation(&mut snapshot);
        snapshot.validate()?;
        observations.insert(snapshot.tab_id.clone(), snapshot);
        drop(observations);
        self.observation_changed.notify_all();
        Ok(())
    }

    fn handle_extension_response(&self, message: NativeEnvelope) -> Result<()> {
        let request_id = message
            .request_id
            .context("extension response has no request id")?;
        let sender = self
            .pending
            .lock()
            .map_err(lock_error)?
            .remove(&request_id)
            .context("extension response has no pending request")?;
        sender
            .send(message.payload)
            .map_err(|_| anyhow!("extension response receiver closed"))
    }

    fn current_observation(&self, tab_id: &str) -> Result<ObservationSnapshot> {
        self.observations
            .lock()
            .map_err(lock_error)?
            .get(tab_id)
            .cloned()
            .context("no current observation for tab")
    }

    fn wait_for_settled_revision(
        &self,
        tab_id: &str,
        document_id: &str,
        revision: u64,
        quiet_window: Duration,
    ) -> Result<(ObservationSnapshot, bool)> {
        let deadline = Instant::now() + POST_ACTION_TIMEOUT;
        let mut observations = self.observations.lock().map_err(lock_error)?;
        let mut latest_revision = revision;
        let mut latest_document_id = document_id.to_string();
        let mut quiet_deadline = None;
        loop {
            let current = observations
                .get(tab_id)
                .context("tab observation disappeared")?;
            if current.document_id != latest_document_id || current.revision > latest_revision {
                latest_document_id.clone_from(&current.document_id);
                latest_revision = current.revision;
                quiet_deadline = Some(Instant::now() + quiet_window);
            }
            let now = Instant::now();
            if quiet_deadline.is_some_and(|quiet| now >= quiet) {
                return Ok((current.clone(), true));
            }
            let remaining = deadline.saturating_duration_since(now);
            if remaining.is_zero() {
                return Ok((current.clone(), false));
            }
            let wait_for = quiet_deadline
                .map(|quiet| quiet.saturating_duration_since(now).min(remaining))
                .unwrap_or(remaining);
            observations = self
                .observation_changed
                .wait_timeout(observations, wait_for)
                .map_err(lock_error)?
                .0;
        }
    }

    fn write_grant(&self) -> Result<()> {
        let Some(address) = self.endpoint.lock().map_err(lock_error)?.clone() else {
            return Ok(());
        };
        let browser_instance_id = self
            .browser_instance_id
            .lock()
            .map_err(lock_error)?
            .clone()
            .unwrap_or_else(|| "pending-extension".into());
        let grant = HostGrant {
            protocol: HOST_PROTOCOL.into(),
            browser_instance_id,
            address,
            capability_scheme: SESSION_CAPABILITY_SCHEME.into(),
            capability: self.capability.clone(),
        };
        let destination = self.runtime_dir.join("host-grant.json");
        let temporary = self.runtime_dir.join("host-grant.json.installing");
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(&grant)?)?;
        file.sync_all()?;
        fs::rename(temporary, destination)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn capability(&self) -> String {
        self.capability.clone()
    }
}

struct SessionObservationSource<'a> {
    session: &'a NativeHostSession,
    tab_id: String,
    basis_document_id: String,
    quiet_window: Duration,
}

impl ObservationSource for SessionObservationSource<'_> {
    fn current_observation(&mut self) -> Result<ObservationSnapshot, ClosedLoopError> {
        self.session
            .current_observation(&self.tab_id)
            .map_err(|error| ClosedLoopError::ObservationSource(error.to_string()))
    }

    fn settled_observation(
        &mut self,
        after_revision: u64,
    ) -> Result<(ObservationSnapshot, bool), ClosedLoopError> {
        self.session
            .wait_for_settled_revision(
                &self.tab_id,
                &self.basis_document_id,
                after_revision,
                self.quiet_window,
            )
            .map_err(|error| ClosedLoopError::ObservationSource(error.to_string()))
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .with_context(|| format!("{key} must be a non-empty string"))
}

fn control_error(id: u64, code: &str, detail: &str) -> ControlResponse {
    ControlResponse {
        id,
        ok: false,
        result: None,
        error: Some(ControlError {
            code: code.into(),
            detail: detail.into(),
        }),
    }
}

fn lock_error<T>(error: std::sync::PoisonError<T>) -> anyhow::Error {
    anyhow!("host state lock was poisoned: {error}")
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::mpsc;

    use saccade_control_sdk::NativePrimitive;
    use saccade_protocol::{
        ActionOperation, Affordance, DispatchStatus, FrameObservation, FrameStatus, ObjectKind,
        ObservationCoverage, ObservedObject, PostconditionStatus, Rect, SemanticRole, Transition,
        Visibility, OBSERVATION_SCHEMA,
    };

    use crate::profile::BanRule;

    use super::*;

    struct CapturingOutbound(mpsc::Sender<NativeEnvelope>);
    impl ExtensionOutbound for CapturingOutbound {
        fn send(&self, message: &NativeEnvelope) -> Result<()> {
            self.0.send(message.clone())?;
            Ok(())
        }
    }

    struct FakeNative(mpsc::Sender<NativePrimitive>);
    impl NativeInput for FakeNative {
        fn execute(
            &mut self,
            primitive: NativePrimitive,
            _: &PreparedAction,
            _: &ActionPayload,
            _: Option<&str>,
        ) -> DispatchStatus {
            self.0.send(primitive).unwrap();
            DispatchStatus::AcceptedByOs
        }
    }

    fn field(has_value: bool) -> ObservedObject {
        ObservedObject {
            object_id: "field-1".into(),
            object_revision: 1,
            frame_id: "frame-1".into(),
            kind: ObjectKind::Control,
            role: SemanticRole::TextField,
            document_bounds: Rect {
                x: 10.0,
                y: 20.0,
                width: 120.0,
                height: 30.0,
            },
            viewport_bounds: None,
            visibility: Visibility::Visible,
            name: Some("Email".into()),
            description: None,
            text: None,
            state: BTreeMap::from([
                ("enabled".into(), "true".into()),
                ("has_value".into(), has_value.to_string()),
            ]),
            affordances: BTreeSet::from([Affordance::Type]),
            transition: Transition::None,
            action_token: Some("token.0123456789abcdef0123456789abcdef0123456789".into()),
            loop_class_token: None,
            protected: false,
        }
    }

    fn snapshot(revision: u64, object: ObservedObject) -> ObservationSnapshot {
        ObservationSnapshot {
            schema: OBSERVATION_SCHEMA.into(),
            browser_instance_id: "browser-1".into(),
            tab_id: "tab-1".into(),
            document_id: "document-1".into(),
            revision,
            viewport_revision: 1,
            frames: vec![FrameObservation {
                frame_id: "frame-1".into(),
                parent_frame_id: None,
                document_id: "document-1".into(),
                origin: "https://fixture.test".into(),
                status: FrameStatus::Observed,
            }],
            objects: vec![object],
            changes: vec![],
            coverage: ObservationCoverage {
                source: "dom_extension".into(),
                observed_frame_count: 1,
                restricted_frame_count: 0,
                truncated: false,
            },
            limitations: vec![],
            gap: false,
        }
    }

    #[test]
    fn session_closes_text_loop_without_sending_value_to_extension_or_receipt() {
        let (out_tx, out_rx) = mpsc::channel();
        let (native_tx, native_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        let session = Arc::new(
            NativeHostSession::with_adapters(
                dir.path().to_path_buf(),
                Arc::new(CapturingOutbound(out_tx)),
                Box::new(FakeNative(native_tx)),
            )
            .unwrap(),
        );
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "hello".into(),
                request_id: None,
                payload: json!({"browser_instance_id":"browser-1"}),
            })
            .unwrap();
        let before_object = field(false);
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(1, before_object.clone())).unwrap(),
            })
            .unwrap();

        let request = ActionRequest {
            browser_instance_id: "browser-1".into(),
            tab_id: "tab-1".into(),
            document_id: "document-1".into(),
            basis_revision: 1,
            action_token: before_object.action_token.clone().unwrap(),
            operation: ActionOperation::Type,
            payload: ActionPayload::Text {
                text: "SENTINEL-SECRET".into(),
            },
        };
        let control = ControlRequest {
            id: 9,
            method: "web.act".into(),
            params: serde_json::to_value(&request).unwrap(),
            capability: session.capability(),
        };
        let worker_session = Arc::clone(&session);
        let worker = std::thread::spawn(move || worker_session.handle_control(control));

        let outbound = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(outbound.kind, "prepare_action");
        assert!(!serde_json::to_string(&outbound)
            .unwrap()
            .contains("SENTINEL-SECRET"));
        let prepared = PreparedAction {
            browser_instance_id: "browser-1".into(),
            tab_id: "tab-1".into(),
            document_id: "document-1".into(),
            basis_revision: 1,
            viewport_revision: 1,
            object_id: "field-1".into(),
            action_token: request.action_token.clone(),
            operation: ActionOperation::Type,
            screen_bounds: Rect {
                x: 10.0,
                y: 20.0,
                width: 120.0,
                height: 30.0,
            },
            visible: true,
            topmost: true,
            focus_verified: true,
            selection_index: None,
        };
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: outbound.request_id,
                payload: serde_json::to_value(prepared).unwrap(),
            })
            .unwrap();
        assert_eq!(
            native_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            NativePrimitive::UnicodeText
        );
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(2, field(true))).unwrap(),
            })
            .unwrap();

        let response = worker.join().unwrap();
        assert!(
            response.ok,
            "{:?}",
            response.error.map(|value| value.detail)
        );
        let receipt: ActionReceipt = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(receipt.postcondition, PostconditionStatus::Verified);
        assert!(receipt.settled);
        assert!(!serde_json::to_string(&receipt)
            .unwrap()
            .contains("SENTINEL-SECRET"));
    }

    #[test]
    fn profile_hides_control_and_rejects_its_token_before_prepare() {
        let (out_tx, out_rx) = mpsc::channel();
        let (native_tx, native_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        let profile = Profile {
            name: "focused".into(),
            behavior: "Use the visible controls in order.".into(),
            ban: vec![BanRule {
                control: "EMAIL".into(),
                condition: None,
            }],
        };
        let session = NativeHostSession::with_adapters_and_profile(
            dir.path().to_path_buf(),
            Arc::new(CapturingOutbound(out_tx)),
            Box::new(FakeNative(native_tx)),
            profile,
        )
        .unwrap();
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "hello".into(),
                request_id: None,
                payload: json!({"browser_instance_id":"browser-1"}),
            })
            .unwrap();
        let banned = field(false);
        let token = banned.action_token.clone().unwrap();
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(1, banned)).unwrap(),
            })
            .unwrap();

        let observed = session.handle_control(ControlRequest {
            id: 1,
            method: "web.observe".into(),
            params: json!({"tab_id":"tab-1"}),
            capability: session.capability(),
        });
        let observed: ObservationSnapshot =
            serde_json::from_value(observed.result.unwrap()).unwrap();
        assert!(observed.objects.is_empty());

        let capabilities = session.handle_control(ControlRequest {
            id: 2,
            method: "system.capabilities".into(),
            params: json!({}),
            capability: session.capability(),
        });
        let capabilities = capabilities.result.unwrap();
        assert_eq!(capabilities["schema"], "saccade.capabilities/4");
        assert_eq!(
            capabilities["profile"],
            json!({"name":"focused","behavior":"Use the visible controls in order."})
        );

        let rejected = session.handle_control(ControlRequest {
            id: 3,
            method: "web.act".into(),
            params: serde_json::to_value(ActionRequest {
                browser_instance_id: "browser-1".into(),
                tab_id: "tab-1".into(),
                document_id: "document-1".into(),
                basis_revision: 1,
                action_token: token,
                operation: ActionOperation::Type,
                payload: ActionPayload::Text {
                    text: "blocked".into(),
                },
            })
            .unwrap(),
            capability: session.capability(),
        });
        assert!(!rejected.ok);
        assert!(out_rx.try_recv().is_err());
        assert!(native_rx.try_recv().is_err());
    }

    #[test]
    fn tabs_open_is_forwarded_to_the_connected_extension() {
        let (out_tx, out_rx) = mpsc::channel();
        let (native_tx, _native_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        let session = Arc::new(
            NativeHostSession::with_adapters(
                dir.path().to_path_buf(),
                Arc::new(CapturingOutbound(out_tx)),
                Box::new(FakeNative(native_tx)),
            )
            .unwrap(),
        );
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "hello".into(),
                request_id: None,
                payload: json!({"browser_instance_id":"browser-1"}),
            })
            .unwrap();

        let control = ControlRequest {
            id: 4,
            method: "tabs.open".into(),
            params: json!({"url":"https://fixture.test/form","active":false}),
            capability: session.capability(),
        };
        let worker_session = Arc::clone(&session);
        let worker = std::thread::spawn(move || worker_session.handle_control(control));

        let outbound = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(outbound.kind, "tabs.open");
        assert_eq!(
            outbound.payload,
            json!({"url":"https://fixture.test/form","active":false})
        );
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: outbound.request_id,
                payload: json!({"tab_id":"tab-7","opened":true}),
            })
            .unwrap();

        let response = worker.join().unwrap();
        assert!(response.ok);
        assert_eq!(
            response.result.unwrap(),
            json!({"tab_id":"tab-7","opened":true})
        );
    }
}
