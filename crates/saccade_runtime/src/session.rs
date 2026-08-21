//! Native Host session authority for the cataloged control families.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use saccade_control_sdk::InputPolicy;
use saccade_protocol::{
    ActionPayload, ActionReceipt, ActionRequest, Affordance, ChangeKind, ControlError,
    ControlRequest, ControlResponse, DispatchStatus, HostGrant, LocalAddress, NativeEnvelope,
    ObservationChange, ObservationDelta, ObservationSnapshot, ObservedObject, PostconditionStatus,
    PreparedAction, SemanticRole, HOST_PROTOCOL, SESSION_CAPABILITY_SCHEME,
};
use serde_json::{json, Value};

use crate::browser_wake::{attached_browser, write_route, BrowserWakeRoute};
use crate::input_policy::{page_scope, LearnedBackend, LocalInputPolicy, PolicyEvidence};
use crate::native_messaging;
use crate::platform_input::PlatformInput;
use crate::profile::Profile;
use crate::{ClosedLoopEngine, ClosedLoopError, NativeInput, ObservationSource};

const EXTENSION_TIMEOUT: Duration = Duration::from_secs(10);
const FIRST_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(20);
const POST_ACTION_TIMEOUT: Duration = Duration::from_secs(2);
const POST_ACTION_QUIET_WINDOW: Duration = Duration::from_millis(300);

/// A compact journal entry for the parts of an object that grant action
/// authority. Geometry, visibility, and object_revision are intentionally not
/// included because they are the values local actionability waiting stabilizes.
#[derive(Clone, PartialEq)]
struct ActionAuthority {
    semantic: String,
    enabled: Option<String>,
    affordances: BTreeSet<Affordance>,
    action_token: Option<String>,
}

fn action_authority(object: &ObservedObject) -> ActionAuthority {
    let mut semantic_state = object.state.clone();
    let enabled = semantic_state.remove("enabled");
    ActionAuthority {
        semantic: serde_json::to_string(&json!({
            "object_id":object.object_id,
            "frame_id":object.frame_id,
            "kind":object.kind,
            "role":object.role,
            "name":object.name,
            "description":object.description,
            "text":object.text,
            "navigation_target":object.navigation_target,
            "navigation_disposition":object.navigation_disposition,
            "state":semantic_state,
            "transition":object.transition,
            "loop_class_token":object.loop_class_token,
            "protected":object.protected,
        }))
        .expect("action authority is serializable"),
        enabled,
        affordances: object.affordances.clone(),
        action_token: object.action_token.clone(),
    }
}

fn compatible_action_authority(left: &ActionAuthority, right: &ActionAuthority) -> bool {
    left.semantic == right.semantic
        && (left.enabled == right.enabled
            || (left.enabled.as_deref() == Some("false")
                && right.enabled.as_deref() == Some("true")))
        && ((left.action_token.is_none() && left.affordances.is_empty())
            || (left.affordances == right.affordances && left.action_token == right.action_token))
}

/// Geometry, visibility, object_revision, and one disabled-to-enabled edge may
/// change while a target moves, becomes uncovered, or becomes actionable.
/// Everything else that grants semantic authority remains exact; an old object
/// is never rebound to a new one.
fn same_action_authority(left: &ObservedObject, right: &ObservedObject) -> bool {
    compatible_action_authority(&action_authority(left), &action_authority(right))
}
const DEFERRED_CONTENT_QUIET_WINDOW: Duration = Duration::from_millis(750);
const SELECT_POST_ACTION_QUIET_WINDOW: Duration = Duration::from_millis(750);
const REFLEX_POST_ACTION_QUIET_WINDOW: Duration = Duration::from_millis(1);
const REFLEX_RECOVERY_BUDGET: Duration = Duration::from_millis(45);
const VERIFIED_POST_ACTION_QUIET_WINDOW: Duration = Duration::from_millis(25);
const OBSERVATION_HISTORY_LIMIT: usize = 256;

struct SettlementPolicy<'a> {
    quiet_window: Duration,
    allow_document_transition: bool,
    reflex_loop_class: Option<&'a str>,
    reflex_occurrence: Option<&'a str>,
}

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
    expected_extension_candidate: Option<ExtensionCandidate>,
    extension_candidate: Mutex<Option<ExtensionCandidate>>,
    extension_connected: AtomicBool,
    observations: Mutex<ObservationState>,
    observation_changed: Condvar,
    pending: Mutex<BTreeMap<u64, mpsc::Sender<Value>>>,
    next_request_id: AtomicU64,
    outbound: Arc<dyn ExtensionOutbound>,
    profile: Profile,
    input_policy: Mutex<Option<LocalInputPolicy>>,
    engine: Mutex<ClosedLoopEngine>,
    native: Mutex<Box<dyn NativeInput>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExtensionCandidate {
    schema: String,
    id: String,
    version: String,
}

impl ExtensionCandidate {
    fn from_value(value: &Value) -> Result<Self> {
        let schema = required_string(value, "schema")?.to_string();
        let id = required_string(value, "id")?.to_string();
        let version = required_string(value, "version")?.to_string();
        if schema != "saccade.extension-candidate/1" {
            bail!("extension candidate used the wrong schema");
        }
        if id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("extension candidate id must be a SHA-256 digest");
        }
        if version.is_empty() || version.len() > 64 {
            bail!("extension candidate version is invalid");
        }
        Ok(Self {
            schema,
            id,
            version,
        })
    }

    fn value(&self) -> Value {
        json!({"schema":self.schema,"id":self.id,"version":self.version})
    }
}

fn load_expected_extension_candidate(runtime_dir: &Path) -> Result<Option<ExtensionCandidate>> {
    let path = runtime_dir.join("expected-extension-candidate.json");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let value: Value =
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(Some(ExtensionCandidate::from_value(&value)?))
}

#[derive(Default)]
struct ObservationState {
    current: BTreeMap<String, ObservationSnapshot>,
    // One current full snapshot is enough to answer present-state reads. The
    // bounded journal retains only revision metadata and changed identities so
    // revision-bounded deltas can be folded without keeping 256 full pages.
    history: BTreeMap<String, VecDeque<ObservationJournalEntry>>,
    retired_documents: BTreeMap<String, BTreeSet<String>>,
    resync_pending: BTreeSet<String>,
    // Identities excluded by the active Profile are remembered per tab so a
    // later source delta can cross the filter boundary without leaking the
    // object, retaining a stale formerly-visible object, or fabricating a gap.
    profile_hidden: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Clone)]
struct ObservationJournalEntry {
    document_id: String,
    revision: u64,
    changes: Vec<ObservationChange>,
    authorities: BTreeMap<String, Option<ActionAuthority>>,
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
        let expected_extension_candidate = load_expected_extension_candidate(&runtime_dir)?;
        Ok(Self {
            capability: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes),
            runtime_dir,
            endpoint: Mutex::new(None),
            browser_instance_id: Mutex::new(None),
            expected_extension_candidate,
            extension_candidate: Mutex::new(None),
            extension_connected: AtomicBool::new(false),
            observations: Mutex::new(ObservationState::default()),
            observation_changed: Condvar::new(),
            pending: Mutex::new(BTreeMap::new()),
            next_request_id: AtomicU64::new(1),
            outbound,
            profile,
            // Execution policy is reference-actuator state. The default Truth
            // Layer Host must start without reading or creating it.
            input_policy: Mutex::new(None),
            engine: Mutex::new(ClosedLoopEngine::builtin()?),
            native: Mutex::new(native),
        })
    }

    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    pub fn install_endpoint(&self, address: LocalAddress) -> Result<()> {
        *self.endpoint.lock().map_err(lock_error)? = Some(address);
        // Do not publish an endpoint before the Extension proves that this
        // Native Host instance owns a valid live connection. Chromium may
        // start a short-lived replacement while another Host is still healthy;
        // publishing here would strand every MCP client on the replacement's
        // dead socket. A validated hello publishes the endpoint below.
        Ok(())
    }

    pub fn handle_native(&self, message: NativeEnvelope) -> Result<()> {
        if message.protocol != HOST_PROTOCOL {
            bail!("extension used the wrong host protocol");
        }
        match message.kind.as_str() {
            "hello" => self.handle_hello(message.payload),
            "observation" => self.handle_observation(message.payload),
            "observation.delta" => self.handle_observation_delta(message.payload),
            _ if message.request_id.is_some() => self.handle_extension_response(message),
            other => bail!("extension sent unsupported message kind {other}"),
        }
    }

    pub fn mark_extension_disconnected(&self) {
        self.extension_connected.store(false, Ordering::Release);
        if let Ok(mut candidate) = self.extension_candidate.lock() {
            *candidate = None;
        }
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
            "schema":"saccade.capabilities/6",
            "product":"truth_layer",
            "observation_schema":saccade_protocol::OBSERVATION_SCHEMA,
            "host_protocol":HOST_PROTOCOL,
            "perception":"dom_extension",
            "truth":{
                "full_then_delta":true,
                "resource_subscriptions":true,
                "stable_object_identity":true
            },
            "execution_owner":"agent_client",
            // Which browser is actually attached. Without this an operator
            // cannot tell a Chrome session from an Edge session, and evidence
            // gets attributed to the wrong browser.
            "attached_browser": attached_browser(&self.runtime_dir)
                .map(Value::String)
                .unwrap_or(Value::Null),
            "reference_actuator_available":true,
            "browser_support":["chrome","edge"],
            "extension_connected":self.extension_connected.load(Ordering::Acquire),
            "extension_candidate":self.extension_candidate.lock().map_err(lock_error)?.as_ref().map(ExtensionCandidate::value),
            "expected_extension_candidate":self.expected_extension_candidate.as_ref().map(ExtensionCandidate::value),
            "first_slice":["button","text_field","checkbox","select"],
            "profile":{
                "name":self.profile.name,
                "behavior":self.profile.behavior
            }
            })),
            "web.observe" => {
                let object = params
                    .as_object()
                    .context("web.observe params must be an object")?;
                for key in object.keys() {
                    if ![
                        "tab_id",
                        "after_revision",
                        "after_document_id",
                        "since_revision",
                        "timeout_ms",
                    ]
                    .contains(&key.as_str())
                    {
                        bail!("unexpected web.observe argument {key}");
                    }
                }
                let tab_id = required_string(&params, "tab_id")?;
                if params.get("timeout_ms").is_some() && params.get("after_revision").is_none() {
                    bail!("timeout_ms requires after_revision");
                }
                let snapshot = match params.get("after_revision") {
                    Some(value) => {
                        let revision = value
                            .as_u64()
                            .context("after_revision must be an integer")?;
                        let timeout_ms = params
                            .get("timeout_ms")
                            .map(|value| value.as_u64().context("timeout_ms must be an integer"))
                            .transpose()?
                            .unwrap_or(10_000);
                        if !(1..=30_000).contains(&timeout_ms) {
                            bail!("timeout_ms must be between 1 and 30000");
                        }
                        self.wait_for_observation_after(
                            tab_id,
                            revision,
                            params.get("after_document_id").and_then(Value::as_str),
                            Duration::from_millis(timeout_ms),
                        )?
                    }
                    None => self.current_observation(tab_id)?,
                };
                let since_revision = params
                    .get("since_revision")
                    .or_else(|| params.get("after_revision"))
                    .map(|value| value.as_u64().context("since revision must be an integer"))
                    .transpose()?;
                let snapshot = match since_revision {
                    Some(revision) => self.observation_since(tab_id, revision, snapshot)?,
                    None => snapshot,
                };
                Ok(serde_json::to_value(snapshot)?)
            }
            "tabs.list" => self.request_extension("tabs.list", json!({}), EXTENSION_TIMEOUT),
            "tabs.close" => {
                let tab_id = required_string(&params, "tab_id")?;
                for key in params
                    .as_object()
                    .context("tabs.close params must be an object")?
                    .keys()
                {
                    if key != "tab_id" {
                        bail!("unexpected tabs.close argument {key}");
                    }
                }
                let closed = self.request_extension(
                    "tabs.close",
                    json!({"tab_id":tab_id}),
                    EXTENSION_TIMEOUT,
                )?;
                let mut observations = self.observations.lock().map_err(lock_error)?;
                observations.current.remove(tab_id);
                observations.history.remove(tab_id);
                observations.retired_documents.remove(tab_id);
                observations.resync_pending.remove(tab_id);
                observations.profile_hidden.remove(tab_id);
                drop(observations);
                self.observation_changed.notify_all();
                Ok(closed)
            }
            "tabs.open" => {
                let url = required_string(&params, "url")?;
                if url.len() > 8192 || !(url.starts_with("http://") || url.starts_with("https://"))
                {
                    bail!("url must use HTTP or HTTPS and stay within 8192 bytes");
                }
                let claim = match params.get("claim") {
                    None => None,
                    Some(claim) => match claim.as_str() {
                        Some(mode @ ("arm" | "confirm")) => Some(mode.to_string()),
                        _ => bail!("claim must be arm or confirm"),
                    },
                };
                let allowed: &[&str] = match claim.as_deref() {
                    Some("confirm") => &["url", "claim", "claim_id", "tab_id"],
                    Some(_) => &["url", "claim"],
                    None => &["url", "active"],
                };
                for key in params
                    .as_object()
                    .context("tabs.open params must be an object")?
                    .keys()
                {
                    if !allowed.contains(&key.as_str()) {
                        bail!("unexpected tabs.open argument {key}");
                    }
                }
                // Arming creates no tab, so there is no tab identity to return
                // and nothing to wait for. The Agent client creates the tab with
                // its own tooling and confirms with the identity it received.
                if claim.as_deref() == Some("arm") {
                    return self.request_extension(
                        "tabs.open",
                        json!({"url":url,"claim":"arm"}),
                        EXTENSION_TIMEOUT,
                    );
                }
                if claim.as_deref() == Some("confirm") {
                    let claim_id = required_string(&params, "claim_id")?;
                    let requested_tab_id = required_string(&params, "tab_id")?;
                    let mut claimed = self.request_extension(
                        "tabs.open",
                        json!({
                            "url":url,
                            "claim":"confirm",
                            "claim_id":claim_id,
                            "tab_id":requested_tab_id,
                        }),
                        EXTENSION_TIMEOUT,
                    )?;
                    let tab_id = required_string(&claimed, "tab_id")?.to_string();
                    self.wait_for_first_observation(&tab_id, FIRST_OBSERVATION_TIMEOUT)?;
                    claimed["observation_ready"] = Value::Bool(true);
                    return Ok(claimed);
                }
                let active = match params.get("active") {
                    Some(value) => value.as_bool().context("active must be a boolean")?,
                    None => true,
                };
                let mut opened = self.request_extension(
                    "tabs.open",
                    json!({"url":url,"active":active}),
                    EXTENSION_TIMEOUT,
                )?;
                let tab_id = required_string(&opened, "tab_id")?.to_string();
                self.wait_for_first_observation(&tab_id, FIRST_OBSERVATION_TIMEOUT)?;
                opened["observation_ready"] = Value::Bool(true);
                Ok(opened)
            }
            "web.act_object" => self.act_object(params),
            "web.act" => self.act(params, None, None),
            "web.act_native" => self.act(params, Some(InputBackend::Native), None),
            "web.act_soft" => self.act(params, Some(InputBackend::Soft), None),
            "input_policy.list" => self.input_policy_list(params),
            "input_policy.remember_native" => self.remember_native_policy(params),
            "web.form.fill" => self.form_fill(params),
            "web.reflex.run" => self.reflex_run(params),
            _ => bail!("unknown host method {method}"),
        }
    }

    /// Roles the Extension's software pipe is registered to operate. Anything
    /// else is the Agent client's job, not a Saccade failure.
    fn software_typeable(role: SemanticRole) -> bool {
        matches!(
            role,
            SemanticRole::TextField
                | SemanticRole::SearchField
                | SemanticRole::TextArea
                | SemanticRole::ContentEditable
                | SemanticRole::SpinButton
        )
    }

    fn software_capable(role: SemanticRole) -> bool {
        matches!(
            role,
            SemanticRole::Button
                | SemanticRole::Link
                | SemanticRole::Checkbox
                | SemanticRole::Radio
                | SemanticRole::Switch
                | SemanticRole::Select
                | SemanticRole::Option
                | SemanticRole::Tab
                | SemanticRole::MenuItem
                | SemanticRole::ReflexTarget
                | SemanticRole::TextField
                | SemanticRole::SearchField
                | SemanticRole::TextArea
                | SemanticRole::ContentEditable
                | SemanticRole::SpinButton
        )
    }

    /// The single state field whose change proves this role was operated.
    /// Roles with no defined evidence return None and yield
    /// `accepted_but_unverified`; a revision bump or an unrelated object
    /// changing is never accepted as proof.
    fn verification_field(role: SemanticRole) -> Option<&'static str> {
        match role {
            SemanticRole::Checkbox | SemanticRole::Radio | SemanticRole::Switch => Some("checked"),
            SemanticRole::Tab => Some("selected"),
            SemanticRole::Option => Some("selected"),
            SemanticRole::Select => Some("expanded"),
            SemanticRole::Button => Some("pressed"),
            SemanticRole::MenuItem => Some("expanded"),
            SemanticRole::ReflexTarget => Some("reflex_occurrence"),
            SemanticRole::TextField
            | SemanticRole::SearchField
            | SemanticRole::TextArea
            | SemanticRole::ContentEditable
            | SemanticRole::SpinButton => Some("has_value"),
            _ => None,
        }
    }

    fn frame_url(snapshot: &ObservationSnapshot, frame_id: &str) -> Option<String> {
        snapshot
            .frames
            .iter()
            .find(|frame| frame.frame_id == frame_id)
            .and_then(|frame| frame.document_url.clone())
    }

    fn external_required(object_id: &str, role: SemanticRole, reason: &str) -> Value {
        json!({
            "dispatch": "external_execution_required",
            "verified": false,
            "failure_stage": "prepare",
            "failure_code": "external_execution_required",
            "retry_safe": true,
            "reason": reason,
            "object_id": object_id,
            "role": role,
        })
    }

    fn software_handoff(
        object_id: &str,
        role: SemanticRole,
        software_dispatch: &str,
        reason: &str,
    ) -> Value {
        json!({
            "dispatch": "external_execution_required",
            "software_dispatch": software_dispatch,
            "verified": false,
            "outcome": "external_execution_required",
            "failure_stage": "verify",
            "failure_code": "semantic_transition_not_observed",
            "retry_safe": true,
            "reason": reason,
            "object_id": object_id,
            "role": role,
        })
    }

    fn unchanged_state_allows_external_retry(
        role: SemanticRole,
        operation: &str,
        before: Option<&str>,
        after: Option<&str>,
    ) -> bool {
        if before.is_none() || before != after {
            return false;
        }
        match operation {
            // Truth deliberately exposes only emptiness for editable values.
            // Retrying is bounded only when the field was empty and remains
            // empty; a true -> true result could hide a successful replacement.
            "type" => before == Some("false"),
            "select" => true,
            "click" => matches!(
                role,
                SemanticRole::Checkbox
                    | SemanticRole::Radio
                    | SemanticRole::Switch
                    | SemanticRole::Tab
                    | SemanticRole::MenuItem
                    | SemanticRole::ReflexTarget
            ),
            _ => false,
        }
    }

    /// Object-addressed, software-only execution. The Agent names an object_id
    /// from Truth and never a coordinate; the action token stays inside the
    /// Runtime. Native input is never engaged from this path.
    fn act_object(&self, params: Value) -> Result<Value> {
        if params.get("actions").is_some() {
            return self.act_object_batch(params);
        }
        const ALLOWED: [&str; 9] = [
            "tab_id",
            "object_id",
            "operation",
            "document_id",
            "basis_revision",
            "option_object_id",
            "text",
            "timeout_ms",
            "ignore_learned_policy",
        ];
        let object = params
            .as_object()
            .context("act requires an object of arguments")?;
        for key in object.keys() {
            if !ALLOWED.contains(&key.as_str()) {
                bail!("unsupported act field {key}");
            }
        }
        let tab_id = required_string(&params, "tab_id")?.to_string();
        let object_id = required_string(&params, "object_id")?.to_string();
        let operation = required_string(&params, "operation")?.to_string();
        let document_id = required_string(&params, "document_id")?.to_string();
        let requested_basis_revision = params
            .get("basis_revision")
            .and_then(Value::as_u64)
            .context("basis_revision is required")?;
        if !matches!(operation.as_str(), "click" | "select" | "type") {
            bail!("operation must be click, select or type");
        }
        let selected_option_id = (operation == "select")
            .then(|| required_string(&params, "option_object_id").map(str::to_string))
            .transpose()?;
        let timeout = Duration::from_millis(
            params
                .get("timeout_ms")
                .and_then(Value::as_u64)
                .unwrap_or(5_000)
                .clamp(1, 30_000),
        );

        let mut before = self.current_observation(&tab_id)?;
        if before.document_id != document_id {
            bail!(
                "document_id/basis_revision no longer current; call saccade.truth.read for tab {tab_id}"
            );
        }
        let mut basis_revision = requested_basis_revision;
        if before.revision != requested_basis_revision {
            let mut target_ids = BTreeSet::from([object_id.clone()]);
            if let Some(option_id) = selected_option_id.as_ref() {
                target_ids.insert(option_id.clone());
            }
            if !self.objects_unchanged_since(
                &tab_id,
                &document_id,
                requested_basis_revision,
                before.revision,
                &target_ids,
            )? {
                bail!(
                    "target changed after basis_revision; call saccade.truth.read for tab {tab_id}"
                );
            }
            basis_revision = before.revision;
        }
        let mut target = before
            .objects
            .iter()
            .find(|candidate| candidate.object_id == object_id)
            .with_context(|| {
                format!(
                    "unknown object_id at revision {}; call saccade.truth.read for tab {tab_id} first",
                    before.revision
                )
            })?
            .clone();

        if operation == "type" && !Self::software_typeable(target.role) {
            return Ok(Self::external_required(
                &object_id,
                target.role,
                "control role is not registered for software typing",
            ));
        }
        if !Self::software_capable(target.role) {
            return Ok(Self::external_required(
                &object_id,
                target.role,
                "control role is not registered for software input",
            ));
        }
        // A link whose destination leaves HTTP(S) cannot be verified from Truth,
        // so it is handed to the Agent client rather than guessed at.
        if target.role == SemanticRole::Link {
            if let Some(destination) = &target.navigation_target {
                if !destination.starts_with("http://") && !destination.starts_with("https://") {
                    return Ok(Self::external_required(
                        &object_id,
                        target.role,
                        "navigation target is not an HTTP(S) destination",
                    ));
                }
            }
            // A download or a new browsing context never changes this
            // document's URL, so there is no evidence Saccade could produce.
            // Hand it over rather than dispatch something unverifiable.
            match target.navigation_disposition.as_deref() {
                Some("download") => {
                    return Ok(Self::external_required(
                        &object_id,
                        target.role,
                        "link downloads instead of navigating this document",
                    ));
                }
                Some("new_context") => {
                    return Ok(Self::external_required(
                        &object_id,
                        target.role,
                        "link opens a new browsing context that this document's URL cannot verify",
                    ));
                }
                _ => {}
            }
        }
        if self
            .engine
            .lock()
            .map_err(lock_error)?
            .input_policy(target.role)?
            == InputPolicy::NativeRequired
        {
            return Ok(Self::external_required(
                &object_id,
                target.role,
                "registry requires native input for this control",
            ));
        }
        // A remembered native escalation is a hand-off, not an error: the
        // Agent client must be told to act, not handed an exception.
        let control_name = target
            .name
            .clone()
            .unwrap_or_else(|| format!("{:?}", target.role));
        let page = self.page_scope_for_tab(&tab_id)?;
        let generation = self.software_generation();
        let ignore_learned_policy = params
            .get("ignore_learned_policy")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let learned = if ignore_learned_policy {
            None
        } else {
            self.with_input_policy(|policy| {
                Ok(policy.public_act_backend_for(
                    &page,
                    target.role,
                    &control_name,
                    generation.as_deref(),
                ))
            })?
        };
        if learned == Some(LearnedBackend::Native) {
            return Ok(Self::external_required(
                &object_id,
                target.role,
                "user-local input policy requires native input for this control",
            ));
        }

        let actionability_started = Instant::now();
        let required_affordance = match operation.as_str() {
            "click" => Affordance::Click,
            "select" => Affordance::Select,
            "type" => Affordance::Type,
            _ => unreachable!("operation was validated above"),
        };
        while (target.action_token.is_none() || !target.affordances.contains(&required_affordance))
            && actionability_started.elapsed() < timeout
        {
            std::thread::sleep(Duration::from_millis(16));
            let current = self.current_observation(&tab_id)?;
            if current.document_id != document_id {
                bail!("target document changed while waiting for actionability");
            }
            let Some(candidate) = current
                .objects
                .iter()
                .find(|candidate| candidate.object_id == object_id)
            else {
                bail!("target identity disappeared while waiting for actionability");
            };
            if !same_action_authority(&target, candidate) {
                bail!("target semantics changed while waiting for actionability");
            }
            before = current.clone();
            target = candidate.clone();
            basis_revision = current.revision;
        }
        let Some(mut token) = target
            .action_token
            .clone()
            .filter(|_| target.affordances.contains(&required_affordance))
        else {
            return Ok(json!({
                "object_id":object_id,
                "dispatch":"not_dispatched",
                "verified":false,
                "failure_stage":"prepare",
                "failure_code":"actionability_timeout_not_enabled",
                "retry_safe":true,
                "reason":"target did not become actionable before timeout",
                "basis_revision":basis_revision,
                "local_wait_ms":actionability_started.elapsed().as_millis() as u64,
            }));
        };
        let payload = match operation.as_str() {
            "select" => {
                let option = selected_option_id
                    .as_deref()
                    .context("option_object_id is required")?;
                json!({"kind":"select","option_object_id":option})
            }
            "type" => json!({"kind":"text","text":required_string(&params, "text")?}),
            _ => json!({"kind":"none"}),
        };
        // Reuse the audited closed loop; software backend only, never escalated.
        // Geometry can advance between the MCP call and Extension prepare. A
        // bounded local retry refreshes only the same semantic identity and
        // exact token; any semantic/token/document replacement still fails.
        let software_receipt = loop {
            let current = self.current_observation(&tab_id)?;
            if current.document_id != document_id {
                bail!("target document changed while preparing software action");
            }
            if current.revision != basis_revision {
                let Some(candidate) = current
                    .objects
                    .iter()
                    .find(|candidate| candidate.object_id == object_id)
                else {
                    bail!("target identity disappeared while preparing software action");
                };
                if !same_action_authority(&target, candidate) {
                    bail!("target semantics or token changed while preparing software action");
                }
                before = current.clone();
                target = candidate.clone();
                basis_revision = current.revision;
                token = target
                    .action_token
                    .clone()
                    .context("target lost action authority while preparing software action")?;
            }
            let attempt = self.act_inner(
                json!({
                    "browser_instance_id": before.browser_instance_id,
                    "tab_id": tab_id,
                    "document_id": document_id,
                    "basis_revision": basis_revision,
                    "action_token": token,
                    "operation": operation,
                    "payload": payload,
                    "_actionability_timeout_ms": timeout.as_millis() as u64,
                }),
                Some(InputBackend::Soft),
                Some(&page),
                false,
            );
            match attempt {
                Ok(receipt) => break receipt,
                Err(error)
                    if is_pre_dispatch_stale(&error.to_string())
                        && actionability_started.elapsed() < timeout =>
                {
                    std::thread::sleep(Duration::from_millis(16));
                }
                Err(error) => return Err(error),
            }
        };
        let pre_dispatch_wait_ms = actionability_started.elapsed().as_millis() as u64;
        let rebased_from_revision =
            (basis_revision != requested_basis_revision).then_some(requested_basis_revision);
        let before_url = Self::frame_url(&before, &target.frame_id);
        let field = Self::verification_field(target.role);
        let before_state = field.and_then(|name| target.state.get(name).cloned());
        let before_selected = selected_option_id.as_deref().and_then(|option_id| {
            before
                .objects
                .iter()
                .find(|candidate| candidate.object_id == option_id)
                .and_then(|candidate| candidate.state.get("selected").cloned())
        });
        let software_dispatch = software_receipt
            .get("dispatch_status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let local_wait_ms = pre_dispatch_wait_ms.saturating_add(
            software_receipt
                .get("local_wait_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        );
        if software_dispatch != "accepted_by_software" {
            return Ok(json!({
                "object_id":object_id,
                "dispatch":software_dispatch,
                "verified":false,
                "failure_stage":software_receipt.get("failure_stage").and_then(Value::as_str).unwrap_or("dispatch"),
                "failure_code":software_receipt.get("failure_code").and_then(Value::as_str).unwrap_or("software_action_rejected"),
                "retry_safe":software_receipt.get("retry_safe").and_then(Value::as_bool).unwrap_or(false),
                "reason":"software action did not dispatch",
                "basis_revision":basis_revision,
                "local_wait_ms":local_wait_ms,
            }));
        }

        // A same-document change advances the revision; a cross-document
        // navigation replaces the document and restarts it at 1. Either is
        // progress, and waiting only on the revision would miss navigation.
        let progressed = |snapshot: &ObservationSnapshot| {
            snapshot.document_id != document_id || snapshot.revision > basis_revision
        };
        let deadline = Instant::now() + timeout;
        let mut after = self.current_observation(&tab_id)?;
        while !progressed(&after) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
            after = self.current_observation(&tab_id)?;
        }
        let unverified = |reason: &str| {
            json!({
                "object_id": object_id,
                "dispatch": "accepted_by_software",
                "verified": false,
                "outcome": "accepted_but_unverified",
                "failure_stage":"verify",
                "failure_code":"semantic_transition_not_observed",
                "retry_safe":false,
                "reason": reason,
                "revision": after.revision,
                "local_wait_ms":local_wait_ms,
            })
        };
        if !progressed(&after) {
            let unchanged = if operation == "select" {
                before_selected.as_deref()
            } else {
                before_state.as_deref()
            };
            if Self::unchanged_state_allows_external_retry(
                target.role,
                &operation,
                unchanged,
                unchanged,
            ) {
                return Ok(Self::software_handoff(
                    &object_id,
                    target.role,
                    software_dispatch,
                    "software input produced no target semantic transition",
                ));
            }
            return Ok(unverified("no observation followed the action"));
        }
        // Fold the exact source-authored transition that followed this action.
        // MCP removes this internal snapshot, projects aliases/redaction, and
        // returns only changes not already proven by the compact verification
        // receipt. This lets an Agent continue after reveal/replacement and see
        // an unverified button's result without rereading the whole page.
        let transition = self.observation_since(&tab_id, basis_revision, after.clone())?;
        let with_transition = |mut result: Value| -> Result<Value> {
            if let Some(revision) = rebased_from_revision {
                result["rebased_from_revision"] = Value::from(revision);
            }
            result["post_action_observation"] = serde_json::to_value(&transition)?;
            result["local_wait_ms"] = Value::from(local_wait_ms);
            Ok(result)
        };

        if target.role == SemanticRole::Link {
            let Some(destination) = target.navigation_target.clone() else {
                return with_transition(unverified(
                    "link has no navigation_target to verify against",
                ));
            };
            let after_url = Self::frame_url(&after, &target.frame_id);
            let reached = after_url.as_deref() == Some(destination.as_str());
            let same_document = after.document_id == before.document_id;
            let already_there = before_url.as_deref() == Some(destination.as_str());
            let verified = if !same_document {
                reached
            } else {
                reached && !already_there
            };
            if !verified {
                return with_transition(unverified(if already_there && reached {
                    "document URL already equalled the navigation_target before the click"
                } else {
                    "current document URL does not match the link's navigation_target"
                }));
            }
            return with_transition(json!({
                "dispatch": "accepted_by_software",
                "verified": true,
                "failure_stage":null,
                "failure_code":null,
                "retry_safe":false,
                "verification": {
                    "object_id": object_id,
                    "field": "document_url",
                    "before": before_url,
                    "after": after_url,
                },
                "basis_revision": basis_revision,
                "revision": after.revision,
                "document_id": after.document_id,
            }));
        }

        if operation == "select" {
            let option_id = selected_option_id
                .as_deref()
                .context("option_object_id is required")?;
            let after_selected = after
                .objects
                .iter()
                .find(|candidate| candidate.object_id == option_id)
                .and_then(|candidate| candidate.state.get("selected").cloned());
            if after_selected.as_deref() != Some("true") || before_selected == after_selected {
                if Self::unchanged_state_allows_external_retry(
                    target.role,
                    &operation,
                    before_selected.as_deref(),
                    after_selected.as_deref(),
                ) {
                    return with_transition(Self::software_handoff(
                        &object_id,
                        target.role,
                        software_dispatch,
                        "software input did not select the requested option",
                    ));
                }
                return with_transition(unverified("chosen option did not become selected"));
            }
            return with_transition(json!({
                "dispatch": "accepted_by_software",
                "verified": true,
                "failure_stage":null,
                "failure_code":null,
                "retry_safe":false,
                "verification": {
                    "object_id": option_id,
                    "field": "selected",
                    "before": before_selected,
                    "after": after_selected,
                },
                "basis_revision": basis_revision,
                "revision": after.revision,
                "document_id": after.document_id,
            }));
        }
        let Some(field) = field else {
            return with_transition(unverified("role has no defined verification evidence"));
        };
        let after_state = after
            .objects
            .iter()
            .find(|candidate| candidate.object_id == object_id)
            .and_then(|candidate| candidate.state.get(field).cloned());
        if before_state.is_none() && after_state.is_none() {
            return with_transition(unverified("role has no defined verification evidence"));
        }
        if before_state == after_state {
            if Self::unchanged_state_allows_external_retry(
                target.role,
                &operation,
                before_state.as_deref(),
                after_state.as_deref(),
            ) {
                return with_transition(Self::software_handoff(
                    &object_id,
                    target.role,
                    software_dispatch,
                    "software input left the target semantic state unchanged",
                ));
            }
            return with_transition(unverified("target semantic state did not change"));
        }
        with_transition(json!({
            "dispatch": "accepted_by_software",
            "verified": true,
            "failure_stage":null,
            "failure_code":null,
            "retry_safe":false,
            "verification": {
                "object_id": object_id,
                "field": field,
                "before": before_state,
                "after": after_state,
            },
            "basis_revision": basis_revision,
            "revision": after.revision,
            "document_id": after.document_id,
        }))
    }

    /// Execute an already-planned set of independent form edits inside one
    /// local closed loop. Every step is still object-addressed, software-only,
    /// revision-rebased, and semantically verified. Submit/navigation buttons
    /// are deliberately excluded so one batch cannot hide a material action.
    fn act_object_batch(&self, params: Value) -> Result<Value> {
        const ALLOWED: [&str; 6] = [
            "tab_id",
            "document_id",
            "basis_revision",
            "actions",
            "timeout_ms",
            "ignore_learned_policy",
        ];
        let object = params
            .as_object()
            .context("act batch requires an object of arguments")?;
        for key in object.keys() {
            if !ALLOWED.contains(&key.as_str()) {
                bail!("unsupported act batch field {key}");
            }
        }
        let tab_id = required_string(&params, "tab_id")?.to_string();
        let document_id = required_string(&params, "document_id")?.to_string();
        let basis_revision = params
            .get("basis_revision")
            .and_then(Value::as_u64)
            .filter(|revision| *revision > 0)
            .context("basis_revision is required")?;
        let timeout_ms = params
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(5_000)
            .clamp(1, 30_000);
        let ignore_learned_policy = params
            .get("ignore_learned_policy")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let actions = params
            .get("actions")
            .and_then(Value::as_array)
            .context("actions must be an array")?;
        if actions.is_empty() || actions.len() > 32 {
            bail!("actions must contain between 1 and 32 form operations");
        }

        let initial = self.current_observation(&tab_id)?;
        if initial.document_id != document_id || initial.revision != basis_revision {
            bail!(
                "document_id/basis_revision no longer current; call saccade.truth.read for tab {tab_id}"
            );
        }
        let mut planned = Vec::with_capacity(actions.len());
        let mut target_ids = BTreeSet::new();
        for item in actions {
            let item_object = item
                .as_object()
                .context("each batch action must be an object")?;
            for key in item_object.keys() {
                if !["object_id", "operation", "option_object_id", "value"].contains(&key.as_str())
                {
                    bail!("unsupported batch action field {key}");
                }
            }
            let object_id = required_string(item, "object_id")?.to_string();
            if !target_ids.insert(object_id.clone()) {
                bail!("an act batch may target each control only once");
            }
            let operation = required_string(item, "operation")?;
            let target = initial
                .objects
                .iter()
                .find(|candidate| candidate.object_id == object_id)
                .context("batch object_id is not present in current Truth")?;
            if target.protected {
                bail!("protected controls cannot appear in an act batch");
            }
            let allowed = match operation {
                "type" => Self::software_typeable(target.role),
                "select" => target.role == SemanticRole::Select,
                "click" => matches!(
                    target.role,
                    SemanticRole::Checkbox | SemanticRole::Radio | SemanticRole::Switch
                ),
                _ => false,
            };
            if !allowed {
                bail!("act batches accept editable, select, checkbox, radio, and switch operations only");
            }
            if operation == "type" {
                item.get("value")
                    .and_then(Value::as_str)
                    .filter(|text| text.len() <= 8192)
                    .context("type batch action requires value within 8192 bytes")?;
            }
            if operation == "select" {
                let option_id = required_string(item, "option_object_id")?;
                let option = initial
                    .objects
                    .iter()
                    .find(|candidate| candidate.object_id == option_id)
                    .context("selected option is not present in current Truth")?;
                if option.role != SemanticRole::Option {
                    bail!("selected batch object is not an option");
                }
            }
            let mut normalized = item.clone();
            if operation == "type" {
                let value = normalized
                    .as_object_mut()
                    .and_then(|fields| fields.remove("value"))
                    .context("type batch action requires value")?;
                normalized["text"] = value;
            }
            planned.push(normalized);
        }

        let mut summaries = Vec::with_capacity(planned.len());
        for (index, mut item) in planned.into_iter().enumerate() {
            let current = self.current_observation(&tab_id)?;
            if current.document_id != document_id {
                bail!("act batch document changed before step {}", index + 1);
            }
            let step_object_id = item["object_id"].clone();
            let step_operation = item["operation"].clone();
            {
                let fields = item
                    .as_object_mut()
                    .expect("validated batch action must be an object");
                fields.insert("tab_id".into(), Value::String(tab_id.clone()));
                fields.insert("document_id".into(), Value::String(document_id.clone()));
                fields.insert("basis_revision".into(), Value::from(current.revision));
                fields.insert("timeout_ms".into(), Value::from(timeout_ms));
                fields.insert(
                    "ignore_learned_policy".into(),
                    Value::Bool(ignore_learned_policy),
                );
            }
            let mut result = self.act_object(item)?;
            let verified = result.get("verified").and_then(Value::as_bool) == Some(true);
            let verification = result
                .as_object_mut()
                .and_then(|object| object.remove("verification"));
            result
                .as_object_mut()
                .map(|object| object.remove("post_action_observation"));
            let mut summary = json!({
                "sequence":index + 1,
                "object_id":step_object_id,
                "operation":step_operation,
                "verified":verified,
            });
            if let Some(verification) = verification {
                summary["verification"] = verification;
            }
            summaries.push(summary);
            if !verified {
                let current = self.current_observation(&tab_id)?;
                let mut response = json!({
                    "schema":"saccade.batch-result/1",
                    "completed":index,
                    "all_verified":false,
                    "steps":summaries,
                    "failure":result,
                    "document_id":current.document_id,
                    "revision":current.revision,
                    "next_basis_revision":current.revision,
                });
                if current.revision != initial.revision {
                    response["post_action_observation"] = serde_json::to_value(
                        self.observation_since(&tab_id, initial.revision, current)?,
                    )?;
                }
                return Ok(response);
            }
        }
        let current = self.current_observation(&tab_id)?;
        let final_observation =
            self.observation_since(&tab_id, initial.revision, current.clone())?;
        Ok(json!({
            "schema":"saccade.batch-result/1",
            "completed":summaries.len(),
            "all_verified":true,
            "steps":summaries,
            "document_id":current.document_id,
            "revision":current.revision,
            "next_basis_revision":current.revision,
            "post_action_observation":final_observation,
        }))
    }

    fn act(
        &self,
        params: Value,
        backend_override: Option<InputBackend>,
        known_page_scope: Option<&str>,
    ) -> Result<Value> {
        self.act_inner(params, backend_override, known_page_scope, true)
    }

    /// `learn` is false for the public software-only route: Truth-mode
    /// execution must never record a native escalation, because it must never
    /// engage native input in the first place.
    fn act_inner(
        &self,
        mut params: Value,
        backend_override: Option<InputBackend>,
        known_page_scope: Option<&str>,
        learn: bool,
    ) -> Result<Value> {
        let actionability_timeout_ms = params
            .as_object_mut()
            .and_then(|object| object.remove("_actionability_timeout_ms"))
            .and_then(|value| value.as_u64())
            .unwrap_or(5_000)
            .clamp(1, 30_000);
        let request: ActionRequest = serde_json::from_value(params)?;
        request.validate()?;
        if let ActionPayload::File { path } = &request.payload {
            let upload = Path::new(path);
            if !upload.is_absolute() {
                bail!("upload path must be absolute");
            }
            let metadata = fs::symlink_metadata(upload)
                .with_context(|| "upload path is not an accessible regular file")?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                bail!("upload path must be a regular non-symlink file");
            }
        }
        let before = self.current_observation(&request.tab_id)?;
        let target = before
            .objects
            .iter()
            .find(|object| object.action_token.as_deref() == Some(&request.action_token))
            .context("action token is not present in the current Profile-filtered observation")?;
        let target_role = target.role;
        let control_name = target
            .name
            .clone()
            .unwrap_or_else(|| format!("{target_role:?}"));
        let default_policy = self
            .engine
            .lock()
            .map_err(lock_error)?
            .input_policy(target_role)?;
        let page = match (default_policy, known_page_scope) {
            (InputPolicy::SoftwarePreferred, Some(page)) => page.to_string(),
            (InputPolicy::SoftwarePreferred, None) => self.page_scope_for_tab(&request.tab_id)?,
            (InputPolicy::NativeRequired, _) => String::new(),
        };
        let (backend, registered_policy) = if learn {
            self.input_backend(target_role, &page, &control_name, backend_override)?
        } else {
            // The public route already applied its generation-aware policy
            // above. Reusing the Reference Actuator's generation-agnostic
            // resolver here would incorrectly resurrect an obsolete native
            // escalation and reject the explicitly software-only attempt.
            if backend_override != Some(InputBackend::Soft) {
                bail!("public software route requires the soft input backend");
            }
            (InputBackend::Soft, default_policy)
        };
        let reflex_loop_class = (target_role == SemanticRole::ReflexTarget)
            .then(|| target.loop_class_token.clone())
            .flatten();
        let reflex_occurrence = (target_role == SemanticRole::ReflexTarget)
            .then(|| target.state.get("reflex_occurrence").cloned())
            .flatten();
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
                "payload":extension_payload,
                "timeout_ms":actionability_timeout_ms,
                // The software command performs its own revision-bound
                // prepare and scroll immediately before dispatch. Avoid a
                // first-pass scroll that would advance Truth and stale the
                // command against itself. Native input still prepares its
                // physical geometry here.
                "defer_scroll":backend == InputBackend::Soft
            }),
            EXTENSION_TIMEOUT,
        )?)?;

        let mut engine = self.engine.lock().map_err(lock_error)?;
        let mut source = SessionObservationSource {
            session: self,
            tab_id: request.tab_id.clone(),
            basis_document_id: request.document_id.clone(),
            quiet_window: if target_role == SemanticRole::ReflexTarget {
                REFLEX_POST_ACTION_QUIET_WINDOW
            } else if request.operation == saccade_protocol::ActionOperation::Select {
                SELECT_POST_ACTION_QUIET_WINDOW
            } else if target.transition == saccade_protocol::Transition::DeferredContentPossible {
                DEFERRED_CONTENT_QUIET_WINDOW
            } else {
                POST_ACTION_QUIET_WINDOW
            },
            allow_document_transition: target_role != SemanticRole::ReflexTarget,
            reflex_loop_class,
            reflex_occurrence,
        };
        let mut software_wait_ms = None;
        let mut receipt: ActionReceipt = match backend {
            InputBackend::Native => {
                let mut native = self.native.lock().map_err(lock_error)?;
                engine.execute(&request, &before, &prepared, native.as_mut(), &mut source)?
            }
            InputBackend::Soft => {
                let mut software = SoftwareInput {
                    session: self,
                    timeout_ms: actionability_timeout_ms,
                    failure: None,
                    local_wait_ms: None,
                };
                let mut receipt =
                    engine.execute(&request, &before, &prepared, &mut software, &mut source)?;
                software_wait_ms = software.local_wait_ms;
                if let Some(failure) = software.failure {
                    receipt.failure_stage = Some(failure.stage);
                    receipt.failure_code = Some(failure.code);
                    receipt.retry_safe = Some(failure.retry_safe);
                }
                receipt
            }
        };
        receipt.post_action_observation = self.observation_since(
            &request.tab_id,
            request.basis_revision,
            receipt.post_action_observation,
        )?;
        if learn {
            self.learn_from_receipt(
                &page,
                target_role,
                &control_name,
                backend,
                registered_policy,
                &receipt,
            )?;
        }
        let mut value = serde_json::to_value(receipt)?;
        if let Some(local_wait_ms) = software_wait_ms {
            value["local_wait_ms"] = Value::from(local_wait_ms);
        }
        Ok(value)
    }

    /// The Extension candidate currently answering, which identifies the
    /// software input implementation a learned rule was concluded against.
    fn software_generation(&self) -> Option<String> {
        self.extension_candidate
            .lock()
            .ok()
            .and_then(|candidate| candidate.as_ref().map(|value| value.id.clone()))
    }

    fn input_backend(
        &self,
        role: SemanticRole,
        page: &str,
        control: &str,
        backend_override: Option<InputBackend>,
    ) -> Result<(InputBackend, InputPolicy)> {
        let policy = self.engine.lock().map_err(lock_error)?.input_policy(role)?;
        // The Reference Actuator resolves the rule exactly as it always has;
        // a new Extension generation must not change what it selects here.
        let learned =
            self.with_input_policy(|policy| Ok(policy.backend_for(page, role, control)))?;
        let backend = match (policy, learned, backend_override) {
            (InputPolicy::NativeRequired, _, Some(InputBackend::Soft)) => {
                bail!("registered control requires native input")
            }
            (InputPolicy::NativeRequired, _, _) => InputBackend::Native,
            (
                InputPolicy::SoftwarePreferred,
                Some(LearnedBackend::Native),
                Some(InputBackend::Soft),
            ) => bail!("user-local input policy requires native input"),
            (InputPolicy::SoftwarePreferred, _, Some(InputBackend::Native)) => InputBackend::Native,
            (InputPolicy::SoftwarePreferred, _, Some(InputBackend::Soft)) => InputBackend::Soft,
            (InputPolicy::SoftwarePreferred, Some(LearnedBackend::Native), None) => {
                InputBackend::Native
            }
            (InputPolicy::SoftwarePreferred, _, None) => InputBackend::Soft,
        };
        Ok((backend, policy))
    }

    fn page_scope_for_tab(&self, tab_id: &str) -> Result<String> {
        let listed = self.request_extension("tabs.list", json!({}), EXTENSION_TIMEOUT)?;
        let url = listed
            .get("tabs")
            .and_then(Value::as_array)
            .and_then(|tabs| {
                tabs.iter()
                    .find(|tab| tab.get("tab_id").and_then(Value::as_str) == Some(tab_id))
            })
            .and_then(|tab| tab.get("url"))
            .and_then(Value::as_str)
            .context("current tab URL is unavailable for input policy")?;
        page_scope(url)
    }

    fn learn_from_receipt(
        &self,
        page: &str,
        role: SemanticRole,
        control: &str,
        backend: InputBackend,
        registered_policy: InputPolicy,
        receipt: &ActionReceipt,
    ) -> Result<()> {
        if registered_policy != InputPolicy::SoftwarePreferred
            || backend != InputBackend::Soft
            || receipt.dispatch_status != DispatchStatus::AcceptedBySoftware
        {
            return Ok(());
        }
        let learned = match receipt.postcondition {
            PostconditionStatus::Verified => Some((
                LearnedBackend::Software,
                PolicyEvidence::VerifiedSoftwareReceipt,
            )),
            PostconditionStatus::VisibleStateUnchanged | PostconditionStatus::Unverified => Some((
                LearnedBackend::Native,
                PolicyEvidence::UnverifiedSoftwareReceipt,
            )),
            PostconditionStatus::TargetInvalidated => None,
        };
        if let Some((learned_backend, evidence)) = learned {
            let generation = self.software_generation();
            self.with_input_policy(|policy| {
                policy.remember(
                    page.to_string(),
                    role,
                    control.to_string(),
                    learned_backend,
                    evidence,
                    generation.clone(),
                )
            })?;
        }
        Ok(())
    }

    fn input_policy_list(&self, params: Value) -> Result<Value> {
        if !params.as_object().is_some_and(|object| object.is_empty()) {
            bail!("input_policy.list takes no arguments");
        }
        self.with_input_policy(|policy| {
            Ok(json!({
                "schema":"saccade.input-policy/1",
                "rules":policy.rules()
            }))
        })
    }

    fn remember_native_policy(&self, params: Value) -> Result<Value> {
        let object = params
            .as_object()
            .context("input_policy.remember_native params must be an object")?;
        for key in object.keys() {
            if !["tab_id", "action_token"].contains(&key.as_str()) {
                bail!("unexpected input policy argument {key}");
            }
        }
        let tab_id = required_string(&params, "tab_id")?;
        let action_token = required_string(&params, "action_token")?;
        let observation = self.current_observation(tab_id)?;
        let target = observation
            .objects
            .iter()
            .find(|object| object.action_token.as_deref() == Some(action_token))
            .context("action token is not current")?;
        if self
            .engine
            .lock()
            .map_err(lock_error)?
            .input_policy(target.role)?
            != InputPolicy::SoftwarePreferred
        {
            bail!("control is already registered as native-required");
        }
        let page = self.page_scope_for_tab(tab_id)?;
        let control = target
            .name
            .clone()
            .unwrap_or_else(|| format!("{:?}", target.role));
        let generation = self.software_generation();
        self.with_input_policy(|policy| {
            policy.remember(
                page.clone(),
                target.role,
                control.clone(),
                LearnedBackend::Native,
                PolicyEvidence::UserRememberedNative,
                generation.clone(),
            )
        })?;
        Ok(json!({
            "remembered":true,
            "page":page,
            "role":target.role,
            "control":control,
            "backend":"native"
        }))
    }

    fn with_input_policy<T>(
        &self,
        operation: impl FnOnce(&mut LocalInputPolicy) -> Result<T>,
    ) -> Result<T> {
        let mut policy = self.input_policy.lock().map_err(lock_error)?;
        if policy.is_none() {
            *policy = Some(LocalInputPolicy::load(&self.runtime_dir)?);
        }
        operation(policy.as_mut().expect("input policy was initialized"))
    }

    fn form_fill(&self, params: Value) -> Result<Value> {
        let object = params
            .as_object()
            .context("web.form.fill params must be an object")?;
        for key in object.keys() {
            if ![
                "browser_instance_id",
                "tab_id",
                "document_id",
                "basis_revision",
                "actions",
            ]
            .contains(&key.as_str())
            {
                bail!("unexpected web.form.fill argument {key}");
            }
        }
        let browser_instance_id = required_string(&params, "browser_instance_id")?;
        let tab_id = required_string(&params, "tab_id")?;
        let document_id = required_string(&params, "document_id")?;
        let basis_revision = params
            .get("basis_revision")
            .and_then(Value::as_u64)
            .context("basis_revision must be a positive integer")?;
        if basis_revision == 0 {
            bail!("basis_revision must be a positive integer");
        }
        let actions = params
            .get("actions")
            .and_then(Value::as_array)
            .context("actions must be an array")?;
        if actions.is_empty() || actions.len() > 32 {
            bail!("actions must contain between 1 and 32 form operations");
        }

        let initial = self.current_observation(tab_id)?;
        if initial.browser_instance_id != browser_instance_id
            || initial.document_id != document_id
            || initial.revision != basis_revision
        {
            bail!("form plan identity or revision is stale");
        }

        struct PlannedAction {
            object_id: String,
            role: SemanticRole,
            name: Option<String>,
            operation: saccade_protocol::ActionOperation,
            payload: ActionPayload,
        }

        let mut planned = Vec::with_capacity(actions.len());
        let mut target_ids = BTreeSet::new();
        for item in actions {
            let item_object = item
                .as_object()
                .context("each form action must be an object")?;
            for key in item_object.keys() {
                if !["action_token", "operation", "payload"].contains(&key.as_str()) {
                    bail!("unexpected form action argument {key}");
                }
            }
            let token = required_string(item, "action_token")?;
            let target = initial
                .objects
                .iter()
                .find(|candidate| candidate.action_token.as_deref() == Some(token))
                .context("form action token is not current")?;
            if !target_ids.insert(target.object_id.clone()) {
                bail!("a form plan may target each control only once");
            }
            if target.protected {
                bail!("protected controls cannot appear in an Agent form plan");
            }
            let operation = serde_json::from_value(
                item.get("operation")
                    .cloned()
                    .context("form action operation is required")?,
            )?;
            let payload: ActionPayload = serde_json::from_value(
                item.get("payload")
                    .cloned()
                    .context("form action payload is required")?,
            )?;
            let allowed = matches!(
                (target.role, operation),
                (
                    SemanticRole::TextField
                        | SemanticRole::SearchField
                        | SemanticRole::TextArea
                        | SemanticRole::ContentEditable
                        | SemanticRole::SpinButton,
                    saccade_protocol::ActionOperation::Type
                ) | (
                    SemanticRole::Select,
                    saccade_protocol::ActionOperation::Select
                ) | (
                    SemanticRole::Checkbox | SemanticRole::Radio | SemanticRole::Switch,
                    saccade_protocol::ActionOperation::Click
                )
            );
            if !allowed {
                bail!("form plans accept editable, select, checkbox, radio, and switch operations only");
            }
            ActionRequest {
                browser_instance_id: browser_instance_id.to_string(),
                tab_id: tab_id.to_string(),
                document_id: document_id.to_string(),
                basis_revision,
                action_token: token.to_string(),
                operation,
                payload: payload.clone(),
            }
            .validate()?;
            if let ActionPayload::Select { option_object_id } = &payload {
                let option = initial
                    .objects
                    .iter()
                    .find(|candidate| candidate.object_id == *option_object_id)
                    .context("selected option is not present in the initial form view")?;
                if option.role != SemanticRole::Option {
                    bail!("selected form object is not an option");
                }
            }
            planned.push(PlannedAction {
                object_id: target.object_id.clone(),
                role: target.role,
                name: target.name.clone(),
                operation,
                payload,
            });
        }

        let mut summaries = Vec::with_capacity(planned.len());
        let page = self.page_scope_for_tab(tab_id)?;
        for (index, step) in planned.into_iter().enumerate() {
            let mut completed_receipt = None;
            for _attempt in 0..8 {
                let current = self.current_observation(tab_id)?;
                if current.browser_instance_id != browser_instance_id
                    || current.document_id != document_id
                {
                    bail!("form document changed before step {}", index + 1);
                }
                let target = current
                    .objects
                    .iter()
                    .find(|candidate| candidate.object_id == step.object_id)
                    .context("form control disappeared before its closed loop")?;
                if target.role != step.role || target.name != step.name {
                    bail!("form control identity changed before its closed loop");
                }
                let action_token = target
                    .action_token
                    .clone()
                    .context("form control is no longer actionable")?;
                let request = ActionRequest {
                    browser_instance_id: current.browser_instance_id.clone(),
                    tab_id: current.tab_id.clone(),
                    document_id: current.document_id.clone(),
                    basis_revision: current.revision,
                    action_token,
                    operation: step.operation,
                    payload: step.payload.clone(),
                };
                match self.act(serde_json::to_value(request)?, None, Some(&page)) {
                    Ok(value) => {
                        let receipt: ActionReceipt = serde_json::from_value(value)?;
                        if receipt.dispatch_status == DispatchStatus::StaleBeforeDispatch {
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        completed_receipt = Some(receipt);
                        break;
                    }
                    Err(error) if is_pre_dispatch_stale(&error.to_string()) => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => return Err(error),
                }
            }
            let receipt = completed_receipt
                .with_context(|| format!("form step {} stayed stale", index + 1))?;
            summaries.push(json!({
                "sequence":index + 1,
                "role":step.role,
                "dispatch_status":receipt.dispatch_status,
                "postcondition":receipt.postcondition,
                "settled":receipt.settled
            }));
            if receipt.postcondition != PostconditionStatus::Verified {
                let current = self.current_observation(tab_id)?;
                return Ok(json!({
                    "schema":"saccade.form-result/1",
                    "completed":index,
                    "all_verified":false,
                    "steps":summaries,
                    "document_id":current.document_id,
                    "revision":current.revision,
                    "next_basis_revision":current.revision,
                    "post_action_observation":receipt.post_action_observation
                }));
            }
        }
        let current = self.current_observation(tab_id)?;
        let final_observation =
            self.observation_since(tab_id, initial.revision, current.clone())?;
        Ok(json!({
            "schema":"saccade.form-result/1",
            "completed":summaries.len(),
            "all_verified":true,
            "steps":summaries,
            "document_id":current.document_id,
            "revision":current.revision,
            "next_basis_revision":current.revision,
            "post_action_observation":final_observation
        }))
    }

    fn reflex_run(&self, params: Value) -> Result<Value> {
        let object = params
            .as_object()
            .context("web.reflex.run params must be an object")?;
        for key in object.keys() {
            if !["tab_id", "input_backend", "max_actions", "timeout_ms"].contains(&key.as_str()) {
                bail!("unexpected web.reflex.run argument {key}");
            }
        }
        let tab_id = required_string(&params, "tab_id")?;
        let backend_value = params.get("input_backend");
        let backend_override = backend_value
            .map(|value| value.as_str().context("input_backend must be a string"))
            .transpose()?
            .map(|name| match name {
                "native" => Ok(InputBackend::Native),
                "soft" => Ok(InputBackend::Soft),
                _ => bail!("input_backend must be native or soft"),
            })
            .transpose()?;
        let page = self.page_scope_for_tab(tab_id)?;
        let (backend, _) = self.input_backend(
            SemanticRole::ReflexTarget,
            &page,
            "ReflexTarget",
            backend_override,
        )?;
        let backend_name = backend.name();
        let max_actions = params
            .get("max_actions")
            .map(|value| value.as_u64().context("max_actions must be an integer"))
            .transpose()?
            .unwrap_or(500) as usize;
        let timeout_ms = params
            .get("timeout_ms")
            .map(|value| value.as_u64().context("timeout_ms must be an integer"))
            .transpose()?
            .unwrap_or(30_000);
        if !(1..=10_000).contains(&max_actions) || !(1..=60_000).contains(&timeout_ms) {
            bail!("reflex bounds are outside the registered contract");
        }

        let started = Instant::now();
        let deadline = started + Duration::from_millis(timeout_ms);
        let mut observation = self.current_observation(tab_id)?;
        observation.validate()?;
        let mut receipts = Vec::new();
        let mut latencies = Vec::new();
        let mut failures = 0_u64;
        let mut stale_retries = 0_u64;
        let mut bounded_recoveries = 0_u64;
        let mut stop_reason = "timeout";

        while Instant::now() < deadline && receipts.len() < max_actions {
            let target = observation.objects.iter().rev().find(|object| {
                object.role == SemanticRole::ReflexTarget && object.action_token.is_some()
            });
            let Some(target) = target else {
                thread::sleep(Duration::from_millis(2));
                observation = self.current_observation(tab_id)?;
                continue;
            };
            let request = ActionRequest {
                browser_instance_id: observation.browser_instance_id.clone(),
                tab_id: observation.tab_id.clone(),
                document_id: observation.document_id.clone(),
                basis_revision: observation.revision,
                action_token: target
                    .action_token
                    .clone()
                    .context("reflex target has no token")?,
                operation: saccade_protocol::ActionOperation::Click,
                payload: ActionPayload::None,
            };
            let before_occurrence = target.state.get("reflex_occurrence").cloned();
            let before_loop_class = target.loop_class_token.clone();
            let action_started = Instant::now();
            let result = self.act(serde_json::to_value(&request)?, Some(backend), Some(&page));
            let receipt: ActionReceipt = match result {
                Ok(value) => serde_json::from_value(value)?,
                Err(error) if is_reflex_recoverable_preparation(&error.to_string()) => {
                    let recovery_deadline = (action_started + REFLEX_RECOVERY_BUDGET).min(deadline);
                    let current = self.current_observation(tab_id)?;
                    if current.document_id == observation.document_id
                        && current.revision > observation.revision
                    {
                        bounded_recoveries += 1;
                        observation = current;
                        continue;
                    }
                    let remaining = recovery_deadline.saturating_duration_since(Instant::now());
                    let recovered = (!remaining.is_zero()).then(|| {
                        self.wait_for_observation_after(
                            tab_id,
                            observation.revision,
                            Some(&observation.document_id),
                            remaining,
                        )
                    });
                    if let Some(Ok(next)) = recovered {
                        bounded_recoveries += 1;
                        observation = next;
                        continue;
                    }
                    failures += 1;
                    stop_reason = "recovery_exhausted";
                    receipts.push(json!({
                        "sequence":receipts.len() + 1,
                        "error":error.to_string(),
                        "recovery_budget_ms":REFLEX_RECOVERY_BUDGET.as_millis()
                    }));
                    break;
                }
                Err(error) if is_pre_dispatch_stale(&error.to_string()) => {
                    stale_retries += 1;
                    observation = self.current_observation(tab_id)?;
                    continue;
                }
                Err(error) => {
                    failures += 1;
                    stop_reason = "action_rejected";
                    receipts.push(json!({
                        "sequence":receipts.len() + 1,
                        "error":error.to_string()
                    }));
                    break;
                }
            };
            let latency_ms = action_started.elapsed().as_secs_f64() * 1000.0;
            if receipt.dispatch_status == DispatchStatus::StaleBeforeDispatch {
                stale_retries += 1;
                observation = receipt.post_action_observation;
                continue;
            }
            let expected_dispatch = match backend {
                InputBackend::Soft => DispatchStatus::AcceptedBySoftware,
                InputBackend::Native => DispatchStatus::AcceptedByOs,
            };
            let verified = receipt.dispatch_status == expected_dispatch
                && receipt.postcondition == PostconditionStatus::Verified;
            let after_occurrence = receipt
                .post_action_observation
                .objects
                .iter()
                .rev()
                .find(|object| object.role == SemanticRole::ReflexTarget)
                .and_then(|object| object.state.get("reflex_occurrence"))
                .cloned();
            let after_same_loop_occurrences = receipt
                .post_action_observation
                .objects
                .iter()
                .filter(|object| {
                    object.role == SemanticRole::ReflexTarget
                        && object.loop_class_token == before_loop_class
                })
                .filter_map(|object| object.state.get("reflex_occurrence").cloned())
                .collect::<Vec<_>>();
            receipts.push(json!({
                "sequence":receipts.len() + 1,
                "basis_revision":receipt.basis_revision,
                "post_revision":receipt.post_revision,
                "dispatch_status":receipt.dispatch_status,
                "postcondition":receipt.postcondition,
                "before_occurrence":before_occurrence,
                "after_occurrence":after_occurrence,
                "after_same_loop_occurrences":after_same_loop_occurrences,
                "observation_to_receipt_ms":latency_ms
            }));
            latencies.push(latency_ms);
            observation = receipt.post_action_observation;
            if !verified {
                failures += 1;
                stop_reason = "unverified";
                break;
            }
        }
        if receipts.len() >= max_actions {
            stop_reason = "max_actions";
        }
        latencies.sort_by(f64::total_cmp);
        let percentile = |ratio: f64| -> f64 {
            if latencies.is_empty() {
                return 0.0;
            }
            let index = ((latencies.len() - 1) as f64 * ratio).ceil() as usize;
            latencies[index]
        };
        Ok(json!({
            "schema":"saccade.reflex.report/1",
            "input_backend":backend_name,
            "actions":receipts.len().saturating_sub(failures as usize),
            "failures":failures,
            "stale_retries":stale_retries,
            "bounded_recoveries":bounded_recoveries,
            "recovery_budget_ms":REFLEX_RECOVERY_BUDGET.as_millis(),
            "duration_ms":started.elapsed().as_secs_f64() * 1000.0,
            "latency_ms":{
                "p50":percentile(0.50),
                "p95":percentile(0.95),
                "max":latencies.last().copied().unwrap_or(0.0)
            },
            "stop_reason":stop_reason,
            "receipts":receipts
        }))
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
        self.extension_connected.store(false, Ordering::Release);
        let instance = required_string(&payload, "browser_instance_id")?.to_string();
        if instance.len() > 256 {
            bail!("browser instance identity is too long");
        }
        let candidate = payload
            .get("extension_candidate")
            .map(ExtensionCandidate::from_value)
            .transpose()?;
        if let Some(expected) = &self.expected_extension_candidate {
            if candidate.as_ref() != Some(expected) {
                bail!(
                    "live Extension candidate does not match the installed candidate: live={candidate:?} expected={expected:?}"
                );
            }
        }
        if let (Some(browser_family), Some(development), Some(wake_url)) = (
            payload.get("browser_family").and_then(Value::as_str),
            payload.get("development").and_then(Value::as_bool),
            payload.get("wake_url").and_then(Value::as_str),
        ) {
            write_route(
                &self.runtime_dir,
                &BrowserWakeRoute::new(browser_family, development, wake_url)?,
            )?;
        }
        let mut current = self.browser_instance_id.lock().map_err(lock_error)?;
        if current.as_deref().is_some_and(|value| value != instance) {
            bail!("native host cannot switch browser instances");
        }
        *current = Some(instance);
        *self.extension_candidate.lock().map_err(lock_error)? = candidate;
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
        if observations
            .retired_documents
            .get(&snapshot.tab_id)
            .is_some_and(|documents| documents.contains(&snapshot.document_id))
        {
            bail!("observation belongs to a retired document");
        }
        let document_changed = observations
            .current
            .get(&snapshot.tab_id)
            .is_some_and(|previous| previous.document_id != snapshot.document_id);
        let mut stream_gap = false;
        if let Some(previous) = observations.current.get(&snapshot.tab_id) {
            if previous.document_id == snapshot.document_id {
                if snapshot.revision <= previous.revision {
                    bail!("observation revision did not advance");
                }
                if snapshot.revision != previous.revision + 1 {
                    stream_gap = true;
                    snapshot.gap = true;
                    snapshot.changes.clear();
                }
            } else {
                let retired_document = previous.document_id.clone();
                observations
                    .retired_documents
                    .entry(snapshot.tab_id.clone())
                    .or_default()
                    .insert(retired_document);
            }
        }
        let hidden = snapshot
            .objects
            .iter()
            .filter(|object| self.profile.bans(object))
            .map(|object| object.object_id.clone())
            .collect();
        self.profile.filter_observation(&mut snapshot);
        snapshot.validate()?;
        observations.resync_pending.remove(&snapshot.tab_id);
        observations
            .profile_hidden
            .insert(snapshot.tab_id.clone(), hidden);
        if document_changed || stream_gap {
            observations.history.remove(&snapshot.tab_id);
        }
        let history = observations
            .history
            .entry(snapshot.tab_id.clone())
            .or_default();
        let authorities = snapshot
            .objects
            .iter()
            .map(|object| (object.object_id.clone(), Some(action_authority(object))))
            .collect();
        history.push_back(ObservationJournalEntry {
            document_id: snapshot.document_id.clone(),
            revision: snapshot.revision,
            changes: snapshot.changes.clone(),
            authorities,
        });
        while history.len() > OBSERVATION_HISTORY_LIMIT {
            history.pop_front();
        }
        observations
            .current
            .insert(snapshot.tab_id.clone(), snapshot);
        drop(observations);
        self.observation_changed.notify_all();
        Ok(())
    }

    fn handle_observation_delta(&self, payload: Value) -> Result<()> {
        let mut delta: ObservationDelta = serde_json::from_value(payload)?;
        delta.validate()?;
        let current_instance = self.browser_instance_id.lock().map_err(lock_error)?.clone();
        if current_instance.as_deref() != Some(&delta.browser_instance_id) {
            bail!("browser instance mismatch");
        }

        let source_changes = delta.changes.clone();
        let banned_changed_ids = delta
            .objects
            .iter()
            .filter(|object| self.profile.bans(object))
            .map(|object| object.object_id.clone())
            .collect::<BTreeSet<_>>();
        // Profile filtering remains downstream of the Extension compiler. Run
        // it against the changed-object subset before applying that subset to
        // the already-filtered materialized view.
        let mut changed = ObservationSnapshot {
            schema: saccade_protocol::OBSERVATION_SCHEMA.into(),
            browser_instance_id: delta.browser_instance_id.clone(),
            tab_id: delta.tab_id.clone(),
            document_id: delta.document_id.clone(),
            revision: delta.revision,
            viewport_revision: delta.viewport_revision,
            geometry: delta.geometry.clone(),
            frames: delta.frames.clone(),
            objects: std::mem::take(&mut delta.objects),
            changes: std::mem::take(&mut delta.changes),
            coverage: delta.coverage.clone(),
            limitations: delta.limitations.clone(),
            gap: false,
        };
        self.profile.filter_observation(&mut changed);
        changed.validate()?;
        delta.objects = changed.objects;
        delta.changes = changed.changes;
        delta.limitations = changed.limitations;

        let tab_id = delta.tab_id.clone();
        let mut observations = self.observations.lock().map_err(lock_error)?;
        if observations.resync_pending.contains(&tab_id) {
            return Ok(());
        }
        let continuous = observations.current.get(&tab_id).is_some_and(|previous| {
            previous.browser_instance_id == delta.browser_instance_id
                && previous.document_id == delta.document_id
                && previous.revision == delta.base_revision
        });
        if !continuous {
            observations.current.remove(&tab_id);
            observations.history.remove(&tab_id);
            observations.profile_hidden.remove(&tab_id);
            let request_resync = observations.resync_pending.insert(tab_id.clone());
            drop(observations);
            self.observation_changed.notify_all();
            if request_resync {
                self.request_observation_resync(&tab_id)?;
            }
            return Ok(());
        }

        let mut snapshot = observations
            .current
            .get(&tab_id)
            .cloned()
            .context("delta base observation disappeared")?;
        let hidden = observations
            .profile_hidden
            .entry(tab_id.clone())
            .or_default();
        let current_ids = snapshot
            .objects
            .iter()
            .map(|object| object.object_id.clone())
            .collect::<BTreeSet<_>>();
        let mut effective_changes = Vec::new();
        for mut change in source_changes {
            match change.kind {
                ChangeKind::Disappeared if hidden.remove(&change.object_id) => {}
                ChangeKind::Disappeared => effective_changes.push(change),
                ChangeKind::Appeared | ChangeKind::Updated
                    if banned_changed_ids.contains(&change.object_id) =>
                {
                    hidden.insert(change.object_id.clone());
                    if current_ids.contains(&change.object_id) {
                        change.kind = ChangeKind::Disappeared;
                        effective_changes.push(change);
                    }
                }
                ChangeKind::Updated if hidden.remove(&change.object_id) => {
                    change.kind = ChangeKind::Appeared;
                    effective_changes.push(change);
                }
                _ => effective_changes.push(change),
            }
        }
        delta.changes = effective_changes;
        let changed_ids = delta
            .changes
            .iter()
            .map(|change| change.object_id.as_str())
            .collect::<BTreeSet<_>>();
        for object in &mut snapshot.objects {
            if !changed_ids.contains(object.object_id.as_str()) {
                object.action_token = None;
            }
        }
        let mut upserts = std::mem::take(&mut delta.objects)
            .into_iter()
            .map(|object| (object.object_id.clone(), object))
            .collect::<BTreeMap<_, _>>();
        for change in &delta.changes {
            let position = snapshot
                .objects
                .iter()
                .position(|object| object.object_id == change.object_id);
            match change.kind {
                ChangeKind::Appeared if position.is_none() => {
                    snapshot.objects.push(
                        upserts
                            .remove(&change.object_id)
                            .context("appeared delta omitted its current object")?,
                    );
                }
                ChangeKind::Updated if position.is_some() => {
                    snapshot.objects[position.unwrap()] = upserts
                        .remove(&change.object_id)
                        .context("updated delta omitted its current object")?;
                }
                ChangeKind::Disappeared if position.is_some() => {
                    snapshot.objects.remove(position.unwrap());
                }
                _ => bail!("delta change does not apply to its base observation"),
            }
        }
        if !upserts.is_empty() {
            bail!("delta carried an object without a matching change");
        }
        for authority in delta.authorities {
            if let Some(object) = snapshot
                .objects
                .iter_mut()
                .find(|object| object.object_id == authority.object_id)
            {
                object.action_token = Some(authority.action_token);
            }
        }
        snapshot.revision = delta.revision;
        snapshot.viewport_revision = delta.viewport_revision;
        snapshot.geometry = delta.geometry;
        snapshot.frames = delta.frames;
        snapshot.changes = delta.changes;
        snapshot.coverage = delta.coverage;
        snapshot.limitations = delta.limitations;
        snapshot.gap = false;
        snapshot.validate()?;

        let history = observations.history.entry(tab_id.clone()).or_default();
        let authorities = snapshot
            .changes
            .iter()
            .map(|change| {
                let authority = snapshot
                    .objects
                    .iter()
                    .find(|object| object.object_id == change.object_id)
                    .map(action_authority);
                (change.object_id.clone(), authority)
            })
            .collect();
        history.push_back(ObservationJournalEntry {
            document_id: snapshot.document_id.clone(),
            revision: snapshot.revision,
            changes: snapshot.changes.clone(),
            authorities,
        });
        while history.len() > OBSERVATION_HISTORY_LIMIT {
            history.pop_front();
        }
        observations.current.insert(tab_id, snapshot);
        drop(observations);
        self.observation_changed.notify_all();
        Ok(())
    }

    fn request_observation_resync(&self, tab_id: &str) -> Result<()> {
        self.outbound.send(&NativeEnvelope {
            protocol: HOST_PROTOCOL.into(),
            kind: "observation.resync".into(),
            request_id: None,
            payload: json!({"tab_id":tab_id}),
        })
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
            .current
            .get(tab_id)
            .cloned()
            .context("no current observation for tab")
    }

    fn objects_unchanged_since(
        &self,
        tab_id: &str,
        document_id: &str,
        basis_revision: u64,
        current_revision: u64,
        object_ids: &BTreeSet<String>,
    ) -> Result<bool> {
        if current_revision <= basis_revision {
            return Ok(current_revision == basis_revision);
        }
        let observations = self.observations.lock().map_err(lock_error)?;
        let Some(history) = observations.history.get(tab_id) else {
            return Ok(false);
        };
        let at_revision = |revision: u64, object_id: &str| {
            history
                .iter()
                .rev()
                .find(|item| {
                    item.document_id == document_id
                        && item.revision <= revision
                        && item.authorities.contains_key(object_id)
                })
                .and_then(|item| item.authorities.get(object_id))
                .and_then(Option::as_ref)
        };
        Ok(object_ids.iter().all(|object_id| {
            matches!(
                (
                    at_revision(basis_revision, object_id),
                    at_revision(current_revision, object_id),
                ),
                (Some(left), Some(right)) if compatible_action_authority(left, right)
            )
        }))
    }

    fn observation_since(
        &self,
        tab_id: &str,
        basis_revision: u64,
        mut current: ObservationSnapshot,
    ) -> Result<ObservationSnapshot> {
        if current.revision <= basis_revision {
            current.changes.clear();
            return Ok(current);
        }
        let observations = self.observations.lock().map_err(lock_error)?;
        let Some(history) = observations.history.get(tab_id) else {
            current.gap = true;
            current.changes.clear();
            return Ok(current);
        };
        let Some(_base) = history.iter().find(|item| {
            item.document_id == current.document_id && item.revision == basis_revision
        }) else {
            current.gap = true;
            current.changes.clear();
            return Ok(current);
        };
        let mut touched = BTreeMap::<String, (ChangeKind, u64)>::new();
        for change in history
            .iter()
            .filter(|item| {
                item.document_id == current.document_id
                    && item.revision > basis_revision
                    && item.revision <= current.revision
            })
            .flat_map(|item| item.changes.iter())
        {
            touched
                .entry(change.object_id.clone())
                .and_modify(|entry| entry.1 = change.object_revision)
                .or_insert((change.kind, change.object_revision));
        }
        let current_objects = current
            .objects
            .iter()
            .map(|object| (object.object_id.as_str(), object))
            .collect::<BTreeMap<_, _>>();
        current.changes = touched
            .into_iter()
            .filter_map(|(object_id, (first_kind, last_revision))| {
                let existed_at_basis = first_kind != ChangeKind::Appeared;
                match (existed_at_basis, current_objects.get(object_id.as_str())) {
                    (false, Some(object)) => Some(ObservationChange {
                        kind: ChangeKind::Appeared,
                        object_id,
                        object_revision: object.object_revision,
                    }),
                    (true, Some(object)) => Some(ObservationChange {
                        kind: ChangeKind::Updated,
                        object_id,
                        object_revision: object.object_revision,
                    }),
                    (true, None) => Some(ObservationChange {
                        kind: ChangeKind::Disappeared,
                        object_id,
                        object_revision: last_revision,
                    }),
                    (false, None) => None,
                }
            })
            .collect();
        current.validate()?;
        Ok(current)
    }

    fn wait_for_first_observation(
        &self,
        tab_id: &str,
        timeout: Duration,
    ) -> Result<ObservationSnapshot> {
        let deadline = Instant::now() + timeout;
        let mut observations = self.observations.lock().map_err(lock_error)?;
        loop {
            if let Some(snapshot) = observations.current.get(tab_id) {
                return Ok(snapshot.clone());
            }
            if !self.extension_connected.load(Ordering::Acquire) {
                bail!("extension disconnected while opening tab");
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("collector did not produce the first observation for tab {tab_id}");
            }
            observations = self
                .observation_changed
                .wait_timeout(observations, remaining)
                .map_err(lock_error)?
                .0;
        }
    }

    fn wait_for_observation_after(
        &self,
        tab_id: &str,
        revision: u64,
        document_id: Option<&str>,
        timeout: Duration,
    ) -> Result<ObservationSnapshot> {
        let deadline = Instant::now() + timeout;
        let mut observations = self.observations.lock().map_err(lock_error)?;
        loop {
            if let Some(snapshot) = observations.current.get(tab_id) {
                if document_id.is_some_and(|basis| snapshot.document_id != basis)
                    || snapshot.revision > revision
                {
                    return Ok(snapshot.clone());
                }
                if snapshot.revision < revision {
                    let mut reset = snapshot.clone();
                    reset.gap = true;
                    reset.changes.clear();
                    return Ok(reset);
                }
            }
            if !self.extension_connected.load(Ordering::Acquire) {
                bail!("extension disconnected while waiting for tab observation");
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("no observation after revision {revision} for tab {tab_id}");
            }
            observations = self
                .observation_changed
                .wait_timeout(observations, remaining)
                .map_err(lock_error)?
                .0;
        }
    }

    fn wait_for_settled_revision(
        &self,
        tab_id: &str,
        document_id: &str,
        revision: u64,
        policy: SettlementPolicy<'_>,
        sufficient: &mut dyn FnMut(&ObservationSnapshot) -> bool,
    ) -> Result<(ObservationSnapshot, bool)> {
        let deadline = Instant::now() + POST_ACTION_TIMEOUT;
        let mut observations = self.observations.lock().map_err(lock_error)?;
        let mut latest_revision = revision;
        let mut latest_document_id = document_id.to_string();
        let mut quiet_deadline = None;
        let mut verified_deadline = None;
        let mut verified_revision = None;
        loop {
            let current = observations
                .current
                .get(tab_id)
                .context("tab observation disappeared")?;
            let fresh = current.document_id != document_id || current.revision > revision;
            if fresh && sufficient(current) {
                let identity = (current.document_id.clone(), current.revision);
                if verified_revision.as_ref() != Some(&identity) {
                    verified_revision = Some(identity);
                    verified_deadline = Some(
                        Instant::now() + policy.quiet_window.min(VERIFIED_POST_ACTION_QUIET_WINDOW),
                    );
                }
                if verified_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    return Ok((current.clone(), true));
                }
            } else {
                verified_revision = None;
                verified_deadline = None;
            }
            if current.revision > revision
                && policy
                    .reflex_loop_class
                    .zip(policy.reflex_occurrence)
                    .is_some_and(|(loop_class, occurrence)| {
                        current.objects.iter().any(|object| {
                            object.role == SemanticRole::ReflexTarget
                                && object.loop_class_token.as_deref() == Some(loop_class)
                                && object.state.get("reflex_occurrence").map(String::as_str)
                                    != Some(occurrence)
                        })
                    })
            {
                return Ok((current.clone(), true));
            }
            if current.revision > latest_revision
                || (policy.allow_document_transition && current.document_id != latest_document_id)
            {
                latest_document_id.clone_from(&current.document_id);
                latest_revision = current.revision;
                quiet_deadline = Some(Instant::now() + policy.quiet_window);
            }
            let now = Instant::now();
            if policy.reflex_loop_class.is_none()
                && quiet_deadline.is_some_and(|quiet| now >= quiet)
            {
                return Ok((current.clone(), true));
            }
            let remaining = deadline.saturating_duration_since(now);
            if remaining.is_zero() {
                return Ok((current.clone(), false));
            }
            let wait_for = quiet_deadline
                .map(|quiet| quiet.saturating_duration_since(now).min(remaining))
                .unwrap_or(remaining);
            let wait_for = verified_deadline
                .map(|verified| verified.saturating_duration_since(now).min(wait_for))
                .unwrap_or(wait_for);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputBackend {
    Native,
    Soft,
}

impl InputBackend {
    fn name(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Soft => "soft",
        }
    }
}

struct SoftwareInput<'a> {
    session: &'a NativeHostSession,
    timeout_ms: u64,
    failure: Option<SoftwareFailure>,
    local_wait_ms: Option<u64>,
}

struct SoftwareFailure {
    stage: String,
    code: String,
    retry_safe: bool,
}

fn parse_software_failure(message: &str) -> Option<SoftwareFailure> {
    let encoded = message.split("saccade_action_error|").nth(1)?;
    let mut fields = encoded.split('|');
    Some(SoftwareFailure {
        stage: fields.next()?.to_string(),
        code: fields.next()?.to_string(),
        retry_safe: fields.next()? == "true",
    })
}

impl NativeInput for SoftwareInput<'_> {
    fn requires_physical_hit_testing(&self) -> bool {
        false
    }

    fn execute(
        &mut self,
        primitive: saccade_control_sdk::NativePrimitive,
        prepared: &PreparedAction,
        payload: &ActionPayload,
        _: Option<&str>,
    ) -> DispatchStatus {
        let supported = matches!(
            (primitive, prepared.operation, payload),
            (
                saccade_control_sdk::NativePrimitive::PrimaryClick,
                saccade_protocol::ActionOperation::Click,
                ActionPayload::None
            ) | (
                saccade_control_sdk::NativePrimitive::SelectOption,
                saccade_protocol::ActionOperation::Select,
                ActionPayload::Select { .. }
            ) | (
                // Setting a text value is a registered software primitive.
                saccade_control_sdk::NativePrimitive::UnicodeText,
                saccade_protocol::ActionOperation::Type,
                ActionPayload::Text { .. }
            )
        );
        if !supported {
            return DispatchStatus::Unsupported;
        }
        let request = json!({
            "browser_instance_id":prepared.browser_instance_id,
            "tab_id":prepared.tab_id,
            "document_id":prepared.document_id,
            "basis_revision":prepared.basis_revision,
            "action_token":prepared.action_token,
            "operation":prepared.operation,
            "payload":payload,
            "timeout_ms":self.timeout_ms
        });
        let command = if prepared.operation == saccade_protocol::ActionOperation::Click {
            "soft_click"
        } else {
            "soft_action"
        };
        match self
            .session
            .request_extension(command, request, EXTENSION_TIMEOUT)
        {
            Ok(response) if response.get("accepted").and_then(Value::as_bool) == Some(true) => {
                self.local_wait_ms = response
                    .get("local_wait_ms")
                    .and_then(Value::as_f64)
                    .map(|value| value.max(0.0).round() as u64);
                DispatchStatus::AcceptedBySoftware
            }
            Err(error) => {
                let message = error.to_string();
                self.failure = parse_software_failure(&message);
                if message.contains("stale action basis")
                    || message.contains("not current")
                    || message.contains("revision is stale")
                    || message.contains("current reflex target")
                {
                    DispatchStatus::StaleBeforeDispatch
                } else {
                    DispatchStatus::Rejected
                }
            }
            _ => DispatchStatus::Rejected,
        }
    }
}

struct SessionObservationSource<'a> {
    session: &'a NativeHostSession,
    tab_id: String,
    basis_document_id: String,
    quiet_window: Duration,
    allow_document_transition: bool,
    reflex_loop_class: Option<String>,
    reflex_occurrence: Option<String>,
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
        sufficient: &mut dyn FnMut(&ObservationSnapshot) -> bool,
    ) -> Result<(ObservationSnapshot, bool), ClosedLoopError> {
        self.session
            .wait_for_settled_revision(
                &self.tab_id,
                &self.basis_document_id,
                after_revision,
                SettlementPolicy {
                    quiet_window: self.quiet_window,
                    allow_document_transition: self.allow_document_transition,
                    reflex_loop_class: self.reflex_loop_class.as_deref(),
                    reflex_occurrence: self.reflex_occurrence.as_deref(),
                },
                sufficient,
            )
            .map_err(|error| ClosedLoopError::ObservationSource(error.to_string()))
    }
}

fn is_pre_dispatch_stale(detail: &str) -> bool {
    [
        "stale action basis",
        "request identity or revision is stale",
        "action token is not current",
        "action token is not present in the current",
        "tab observation is not current",
        "prepared action failed identity, focus, geometry, visibility, or topmost revalidation",
    ]
    .iter()
    .any(|needle| detail.contains(needle))
}

fn is_reflex_recoverable_preparation(detail: &str) -> bool {
    detail.contains(
        "prepared action failed identity, focus, geometry, visibility, or topmost revalidation",
    )
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
        ObservationAuthority, ObservationCoverage, ObservationDelta, ObservedObject,
        PostconditionStatus, Rect, SemanticRole, Transition, Visibility, OBSERVATION_SCHEMA,
    };

    use crate::profile::BanRule;

    use super::*;

    #[test]
    fn software_action_errors_preserve_machine_readable_stage_code_and_retry_safety() {
        let failure = parse_software_failure(
            "extension rejected: saccade_action_error|prepare|actionability_timeout_not_topmost|true|covered",
        )
        .unwrap();
        assert_eq!(failure.stage, "prepare");
        assert_eq!(failure.code, "actionability_timeout_not_topmost");
        assert!(failure.retry_safe);
        assert!(parse_software_failure("ordinary extension error").is_none());
    }

    #[test]
    fn actionability_rebase_allows_only_geometry_or_a_first_affordance() {
        let base = field(false);
        let mut moved = base.clone();
        moved.object_revision += 1;
        moved.document_bounds.x += 40.0;
        moved.visibility = Visibility::PartiallyOccluded;
        assert!(same_action_authority(&base, &moved));

        let mut disabled = base.clone();
        disabled.action_token = None;
        disabled.affordances.clear();
        disabled.state.insert("enabled".into(), "false".into());
        assert!(same_action_authority(&disabled, &base));
        assert!(!same_action_authority(&base, &disabled));

        let mut new_token = base.clone();
        new_token.action_token = Some("token.changed-0123456789abcdef0123456789abcdef".into());
        assert!(!same_action_authority(&base, &new_token));

        let mut semantic_change = base.clone();
        semantic_change
            .state
            .insert("has_value".into(), "true".into());
        assert!(!same_action_authority(&base, &semantic_change));
    }

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

    #[test]
    fn endpoint_is_published_only_after_a_valid_extension_hello() {
        let (out_tx, _out_rx) = mpsc::channel();
        let (native_tx, _native_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        let session = NativeHostSession::with_adapters(
            dir.path().to_path_buf(),
            Arc::new(CapturingOutbound(out_tx)),
            Box::new(FakeNative(native_tx)),
        )
        .unwrap();

        session
            .install_endpoint(LocalAddress::Unix {
                path: dir.path().join("host-test.sock"),
            })
            .unwrap();
        assert!(!dir.path().join("host-grant.json").exists());

        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "hello".into(),
                request_id: None,
                payload: json!({"browser_instance_id":"browser-1"}),
            })
            .unwrap();

        let grant: HostGrant = serde_json::from_slice(
            &fs::read(dir.path().join("host-grant.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            grant.address,
            LocalAddress::Unix {
                path: dir.path().join("host-test.sock")
            }
        );
    }

    #[test]
    fn expected_extension_candidate_rejects_stale_live_worker() {
        let (out_tx, _out_rx) = mpsc::channel();
        let (native_tx, _native_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        let expected = json!({
            "schema":"saccade.extension-candidate/1",
            "id":"a".repeat(64),
            "version":"0.3.20"
        });
        fs::write(
            dir.path().join("expected-extension-candidate.json"),
            serde_json::to_vec(&expected).unwrap(),
        )
        .unwrap();
        let session = NativeHostSession::with_adapters(
            dir.path().to_path_buf(),
            Arc::new(CapturingOutbound(out_tx)),
            Box::new(FakeNative(native_tx)),
        )
        .unwrap();

        let stale = session.handle_native(NativeEnvelope {
            protocol: HOST_PROTOCOL.into(),
            kind: "hello".into(),
            request_id: None,
            payload: json!({
                "browser_instance_id":"browser-1",
                "extension_candidate":{
                    "schema":"saccade.extension-candidate/1",
                    "id":"b".repeat(64),
                    "version":"0.3.19"
                }
            }),
        });
        assert!(stale.unwrap_err().to_string().contains("does not match"));

        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "hello".into(),
                request_id: None,
                payload: json!({
                    "browser_instance_id":"browser-1",
                    "extension_candidate":expected
                }),
            })
            .unwrap();
        let capabilities = session.handle_control(ControlRequest {
            id: 1,
            method: "system.capabilities".into(),
            params: json!({}),
            capability: session.capability(),
        });
        let capabilities = capabilities.result.unwrap();
        assert_eq!(capabilities["extension_connected"], true);
        assert_eq!(capabilities["extension_candidate"]["id"], "a".repeat(64));
        assert_eq!(
            capabilities["expected_extension_candidate"],
            capabilities["extension_candidate"]
        );
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
            navigation_target: None,
            navigation_disposition: None,
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

    fn named_field(id: &str, name: &str, token_suffix: &str, has_value: bool) -> ObservedObject {
        let mut object = field(has_value);
        object.object_id = id.into();
        object.name = Some(name.into());
        object.action_token = Some(format!("token.{token_suffix:0<48}"));
        object
    }

    fn reflex_target(x: f64) -> ObservedObject {
        let mut target = field(false);
        target.object_id = "reflex-1".into();
        target.role = SemanticRole::ReflexTarget;
        target.name = None;
        target.document_bounds.x = x;
        target.state = BTreeMap::from([
            ("enabled".into(), "true".into()),
            ("reflex_occurrence".into(), x.to_string()),
        ]);
        target.affordances = BTreeSet::from([Affordance::Click]);
        target.loop_class_token = Some("loop.0123456789abcdef0123456789abcdef0123456789".into());
        target
    }

    fn snapshot(revision: u64, object: ObservedObject) -> ObservationSnapshot {
        ObservationSnapshot {
            schema: OBSERVATION_SCHEMA.into(),
            browser_instance_id: "browser-1".into(),
            tab_id: "tab-1".into(),
            document_id: "document-1".into(),
            revision,
            viewport_revision: 1,
            geometry: None,
            frames: vec![FrameObservation {
                frame_id: "frame-1".into(),
                parent_frame_id: None,
                document_id: "document-1".into(),
                document_url: Some("https://fixture.test/page".into()),
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

    fn snapshot_many(revision: u64, objects: Vec<ObservedObject>) -> ObservationSnapshot {
        let mut result = snapshot(revision, objects[0].clone());
        result.objects = objects;
        result
    }

    fn delta(
        base_revision: u64,
        revision: u64,
        objects: Vec<ObservedObject>,
        changes: Vec<ObservationChange>,
        authorities: Vec<ObservationAuthority>,
    ) -> ObservationDelta {
        let base = snapshot(revision, field(false));
        ObservationDelta {
            browser_instance_id: base.browser_instance_id,
            tab_id: base.tab_id,
            document_id: base.document_id,
            base_revision,
            revision,
            viewport_revision: base.viewport_revision,
            geometry: base.geometry,
            frames: base.frames,
            objects,
            changes,
            authorities,
            coverage: base.coverage,
            limitations: base.limitations,
        }
    }

    #[test]
    fn late_observation_cannot_restore_a_retired_document() {
        let (out_tx, _out_rx) = mpsc::channel();
        let (native_tx, _native_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        let session = NativeHostSession::with_adapters(
            dir.path().to_path_buf(),
            Arc::new(CapturingOutbound(out_tx)),
            Box::new(FakeNative(native_tx)),
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
        let first = snapshot(1, field(false));
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(&first).unwrap(),
            })
            .unwrap();

        let mut second = snapshot(1, field(false));
        second.document_id = "document-2".into();
        second.frames[0].document_id = "document-2".into();
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(&second).unwrap(),
            })
            .unwrap();

        let mut late = first;
        late.revision = 2;
        assert!(session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(late).unwrap(),
            })
            .unwrap_err()
            .to_string()
            .contains("retired document"));
        assert_eq!(
            session.current_observation("tab-1").unwrap().document_id,
            "document-2"
        );
    }

    #[test]
    fn missing_extension_revision_forces_a_full_gap_reset() {
        let (out_tx, _out_rx) = mpsc::channel();
        let (native_tx, _native_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        let session = NativeHostSession::with_adapters(
            dir.path().to_path_buf(),
            Arc::new(CapturingOutbound(out_tx)),
            Box::new(FakeNative(native_tx)),
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
        session
            .handle_observation(serde_json::to_value(snapshot(1, field(false))).unwrap())
            .unwrap();
        let mut skipped = snapshot(3, field(true));
        skipped.objects[0].object_revision = 3;
        skipped.changes = vec![ObservationChange {
            kind: ChangeKind::Updated,
            object_id: "field-1".into(),
            object_revision: 3,
        }];
        session
            .handle_observation(serde_json::to_value(skipped).unwrap())
            .unwrap();
        let current = session.current_observation("tab-1").unwrap();
        assert!(current.gap);
        assert!(current.changes.is_empty());
    }

    #[test]
    fn compact_observation_journal_folds_changes_without_full_history() {
        let (out_tx, _out_rx) = mpsc::channel();
        let (native_tx, _native_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        let session = NativeHostSession::with_adapters(
            dir.path().to_path_buf(),
            Arc::new(CapturingOutbound(out_tx)),
            Box::new(FakeNative(native_tx)),
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

        let first = snapshot(1, field(false));
        session
            .handle_observation(serde_json::to_value(first).unwrap())
            .unwrap();

        let mut changed = field(true);
        changed.object_revision = 2;
        let mut second = snapshot(2, changed.clone());
        second.changes = vec![ObservationChange {
            kind: ChangeKind::Updated,
            object_id: changed.object_id.clone(),
            object_revision: 2,
        }];
        session
            .handle_observation(serde_json::to_value(second).unwrap())
            .unwrap();

        let mut appeared = field(false);
        appeared.object_id = "field-2".into();
        appeared.object_revision = 1;
        appeared.action_token = Some("token.2222222222222222222222222222222222222222".into());
        let mut third = snapshot_many(3, vec![changed.clone(), appeared.clone()]);
        third.changes = vec![ObservationChange {
            kind: ChangeKind::Appeared,
            object_id: appeared.object_id.clone(),
            object_revision: 1,
        }];
        session
            .handle_observation(serde_json::to_value(third).unwrap())
            .unwrap();
        assert!(session
            .objects_unchanged_since(
                "tab-1",
                "document-1",
                2,
                3,
                &BTreeSet::from(["field-1".to_string()]),
            )
            .unwrap());
        assert!(!session
            .objects_unchanged_since(
                "tab-1",
                "document-1",
                2,
                3,
                &BTreeSet::from(["field-2".to_string()]),
            )
            .unwrap());

        let mut fourth = snapshot(4, appeared.clone());
        fourth.changes = vec![ObservationChange {
            kind: ChangeKind::Disappeared,
            object_id: changed.object_id.clone(),
            object_revision: 2,
        }];
        session
            .handle_observation(serde_json::to_value(fourth.clone()).unwrap())
            .unwrap();

        let folded = session.observation_since("tab-1", 1, fourth).unwrap();
        assert_eq!(folded.changes.len(), 2);
        assert!(folded.changes.iter().any(|change| {
            change.object_id == "field-1" && change.kind == ChangeKind::Disappeared
        }));
        assert!(folded.changes.iter().any(|change| {
            change.object_id == "field-2" && change.kind == ChangeKind::Appeared
        }));

        let observations = session.observations.lock().unwrap();
        let journal = observations.history.get("tab-1").unwrap();
        assert_eq!(journal.len(), 4);
        assert_eq!(journal[0].revision, 1);
        assert!(journal[0].changes.is_empty());
    }

    #[test]
    fn transport_delta_materializes_one_current_truth_and_refreshes_authority() {
        let (out_tx, _out_rx) = mpsc::channel();
        let (native_tx, _native_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        let session = NativeHostSession::with_adapters(
            dir.path().to_path_buf(),
            Arc::new(CapturingOutbound(out_tx)),
            Box::new(FakeNative(native_tx)),
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

        let unchanged = named_field("field-2", "Name", "second", false);
        session
            .handle_observation(
                serde_json::to_value(snapshot_many(1, vec![field(false), unchanged.clone()]))
                    .unwrap(),
            )
            .unwrap();

        let mut changed = field(true);
        changed.object_revision = 2;
        let next = delta(
            1,
            2,
            vec![changed],
            vec![ObservationChange {
                kind: ChangeKind::Updated,
                object_id: "field-1".into(),
                object_revision: 2,
            }],
            vec![ObservationAuthority {
                object_id: "field-2".into(),
                action_token: "token.refreshed-0123456789abcdef0123456789abcdef".into(),
            }],
        );
        session
            .handle_observation_delta(serde_json::to_value(next).unwrap())
            .unwrap();

        let current = session.current_observation("tab-1").unwrap();
        assert_eq!(current.revision, 2);
        assert_eq!(current.objects.len(), 2);
        assert_eq!(current.objects[0].state["has_value"], "true");
        assert_eq!(
            current.objects[1].action_token.as_deref(),
            Some("token.refreshed-0123456789abcdef0123456789abcdef")
        );
    }

    #[test]
    fn transport_gap_requests_one_exact_tab_snapshot_and_waits_for_it() {
        let (out_tx, out_rx) = mpsc::channel();
        let (native_tx, _native_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        let session = NativeHostSession::with_adapters(
            dir.path().to_path_buf(),
            Arc::new(CapturingOutbound(out_tx)),
            Box::new(FakeNative(native_tx)),
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
        session
            .handle_observation(serde_json::to_value(snapshot(1, field(false))).unwrap())
            .unwrap();

        let skipped = delta(2, 3, vec![], vec![], vec![]);
        session
            .handle_observation_delta(serde_json::to_value(skipped).unwrap())
            .unwrap();
        assert!(session.current_observation("tab-1").is_err());
        let request = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(request.kind, "observation.resync");
        assert_eq!(request.request_id, None);
        assert_eq!(request.payload, json!({"tab_id":"tab-1"}));
        assert!(out_rx.try_recv().is_err());

        let mut replacement = snapshot(3, field(true));
        replacement.objects[0].object_revision = 2;
        session
            .handle_observation(serde_json::to_value(replacement).unwrap())
            .unwrap();
        assert_eq!(session.current_observation("tab-1").unwrap().revision, 3);
    }

    #[test]
    fn transport_deltas_preserve_profile_filter_transitions() {
        let (out_tx, _out_rx) = mpsc::channel();
        let (native_tx, _native_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        let profile = Profile {
            name: "filtered".into(),
            behavior: "Keep one named control hidden.".into(),
            ban: vec![BanRule {
                control: "Secret".into(),
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

        let hidden = named_field("field-1", "Secret", "hidden", false);
        session
            .handle_observation(serde_json::to_value(snapshot(1, hidden)).unwrap())
            .unwrap();
        assert!(session
            .current_observation("tab-1")
            .unwrap()
            .objects
            .is_empty());

        let mut revealed = named_field("field-1", "Public", "revealed", false);
        revealed.object_revision = 2;
        session
            .handle_observation_delta(
                serde_json::to_value(delta(
                    1,
                    2,
                    vec![revealed],
                    vec![ObservationChange {
                        kind: ChangeKind::Updated,
                        object_id: "field-1".into(),
                        object_revision: 2,
                    }],
                    vec![],
                ))
                .unwrap(),
            )
            .unwrap();
        let visible = session.current_observation("tab-1").unwrap();
        assert_eq!(visible.objects.len(), 1);
        assert_eq!(visible.changes[0].kind, ChangeKind::Appeared);

        let mut hidden_again = named_field("field-1", "Secret", "hidden-again", false);
        hidden_again.object_revision = 3;
        session
            .handle_observation_delta(
                serde_json::to_value(delta(
                    2,
                    3,
                    vec![hidden_again],
                    vec![ObservationChange {
                        kind: ChangeKind::Updated,
                        object_id: "field-1".into(),
                        object_revision: 3,
                    }],
                    vec![],
                ))
                .unwrap(),
            )
            .unwrap();
        let filtered = session.current_observation("tab-1").unwrap();
        assert!(filtered.objects.is_empty());
        assert_eq!(filtered.changes[0].kind, ChangeKind::Disappeared);

        session
            .handle_observation_delta(
                serde_json::to_value(delta(
                    3,
                    4,
                    vec![],
                    vec![ObservationChange {
                        kind: ChangeKind::Disappeared,
                        object_id: "field-1".into(),
                        object_revision: 3,
                    }],
                    vec![],
                ))
                .unwrap(),
            )
            .unwrap();
        assert!(session
            .current_observation("tab-1")
            .unwrap()
            .changes
            .is_empty());
    }

    #[test]
    fn settlement_ignores_a_fresh_revision_until_the_verifier_can_succeed() {
        let (out_tx, _out_rx) = mpsc::channel();
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
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(1, field(false))).unwrap(),
            })
            .unwrap();

        let waiting = Arc::clone(&session);
        let worker = std::thread::spawn(move || {
            let mut sufficient = |observation: &ObservationSnapshot| {
                observation.objects[0]
                    .state
                    .get("has_value")
                    .map(String::as_str)
                    == Some("true")
            };
            waiting.wait_for_settled_revision(
                "tab-1",
                "document-1",
                1,
                SettlementPolicy {
                    quiet_window: Duration::from_millis(300),
                    allow_document_transition: true,
                    reflex_loop_class: None,
                    reflex_occurrence: None,
                },
                &mut sufficient,
            )
        });
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(2, field(false))).unwrap(),
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(20));
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(3, field(true))).unwrap(),
            })
            .unwrap();
        let (settled, verified) = worker.join().unwrap().unwrap();
        assert_eq!(settled.revision, 3);
        assert!(verified);
    }

    #[test]
    fn observe_after_revision_waits_for_the_next_browser_push() {
        let (out_tx, _out_rx) = mpsc::channel();
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
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(1, field(false))).unwrap(),
            })
            .unwrap();

        let waiting = Arc::clone(&session);
        let worker = std::thread::spawn(move || {
            waiting.handle_control(ControlRequest {
                id: 9,
                method: "web.observe".into(),
                params: json!({
                    "tab_id":"tab-1",
                    "after_revision":1,
                    "timeout_ms":1000
                }),
                capability: waiting.capability(),
            })
        });
        std::thread::sleep(Duration::from_millis(20));
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(2, field(true))).unwrap(),
            })
            .unwrap();

        let response = worker.join().unwrap();
        assert!(response.ok, "{:?}", response.error);
        let observation: ObservationSnapshot =
            serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(observation.revision, 2);

        let invalid = session.handle_control(ControlRequest {
            id: 10,
            method: "web.observe".into(),
            params: json!({"tab_id":"tab-1","timeout_ms":100}),
            capability: session.capability(),
        });
        assert!(!invalid.ok);
    }

    #[test]
    fn impossible_future_revision_returns_a_full_gap_reset() {
        let (out_tx, _out_rx) = mpsc::channel();
        let (native_tx, _native_rx) = mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        let session = NativeHostSession::with_adapters(
            dir.path().to_path_buf(),
            Arc::new(CapturingOutbound(out_tx)),
            Box::new(FakeNative(native_tx)),
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
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(3, field(true))).unwrap(),
            })
            .unwrap();

        let response = session.handle_control(ControlRequest {
            id: 11,
            method: "web.observe".into(),
            params: json!({"tab_id":"tab-1","after_revision":44,"timeout_ms":1000}),
            capability: session.capability(),
        });
        assert!(response.ok, "{:?}", response.error);
        let observation: ObservationSnapshot =
            serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(observation.revision, 3);
        assert!(observation.gap);
        assert!(observation.changes.is_empty());
    }

    #[test]
    fn observe_after_revision_returns_when_navigation_resets_revision() {
        let (out_tx, _out_rx) = mpsc::channel();
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
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(15, field(false))).unwrap(),
            })
            .unwrap();

        let waiting = Arc::clone(&session);
        let worker = std::thread::spawn(move || {
            waiting.handle_control(ControlRequest {
                id: 11,
                method: "web.observe".into(),
                params: json!({
                    "tab_id":"tab-1",
                    "after_revision":15,
                    "after_document_id":"document-1",
                    "timeout_ms":1000
                }),
                capability: waiting.capability(),
            })
        });
        std::thread::sleep(Duration::from_millis(20));
        let mut navigated = snapshot(1, field(true));
        navigated.document_id = "document-2".into();
        navigated.frames[0].document_id = "document-2".into();
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(navigated).unwrap(),
            })
            .unwrap();

        let response = worker.join().unwrap();
        assert!(response.ok, "{:?}", response.error);
        let observation: ObservationSnapshot =
            serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(observation.document_id, "document-2");
        assert_eq!(observation.revision, 1);
    }

    #[test]
    fn session_closes_text_loop_keeping_the_value_out_of_preparation_and_receipt() {
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

        // Software-first text resolves the page scope before preparing.
        let scope = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(scope.kind, "tabs.list");
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: scope.request_id,
                payload: serde_json::json!({"tabs":[{"tab_id":"tab-1","url":"https://fixture.test/form","title":"","active":true,"observation_ready":true,"ownership":"agent","provenance":"saccade_tabs_open"}]}),
            })
            .unwrap();
        let outbound = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(outbound.kind, "prepare_action");
        assert_eq!(outbound.payload["defer_scroll"], Value::Bool(true));
        // Preparation never carries the value.
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
        // Setting a value requires sending it to the Extension. That is the
        // narrowed invariant: ordinary text transits, credential-class values
        // never can, because a protected field carries no type affordance.
        let dispatched = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(dispatched.kind, "soft_action");
        assert!(serde_json::to_string(&dispatched)
            .unwrap()
            .contains("SENTINEL-SECRET"));
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: dispatched.request_id,
                payload: serde_json::json!({"accepted":true}),
            })
            .unwrap();
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
    fn public_software_handoff_is_bounded_to_provably_unchanged_state() {
        assert!(NativeHostSession::unchanged_state_allows_external_retry(
            SemanticRole::TextField,
            "type",
            Some("false"),
            Some("false"),
        ));
        // Truth hides editable values, so true -> true cannot prove that a
        // replacement failed and must never authorize a duplicate type.
        assert!(!NativeHostSession::unchanged_state_allows_external_retry(
            SemanticRole::TextField,
            "type",
            Some("true"),
            Some("true"),
        ));
        assert!(NativeHostSession::unchanged_state_allows_external_retry(
            SemanticRole::Checkbox,
            "click",
            Some("false"),
            Some("false"),
        ));
        // A generic button may have an external side effect even when its
        // pressed state does not change.
        assert!(!NativeHostSession::unchanged_state_allows_external_retry(
            SemanticRole::Button,
            "click",
            Some("false"),
            Some("false"),
        ));
        assert!(!NativeHostSession::unchanged_state_allows_external_retry(
            SemanticRole::Checkbox,
            "click",
            Some("false"),
            Some("true"),
        ));
    }

    #[test]
    fn aria_option_is_software_clickable_and_verified_by_selected_state() {
        assert!(NativeHostSession::software_capable(SemanticRole::Option));
        assert_eq!(
            NativeHostSession::verification_field(SemanticRole::Option),
            Some("selected")
        );
    }

    #[test]
    fn opening_a_select_is_verified_by_expanded_state() {
        assert!(NativeHostSession::software_capable(SemanticRole::Select));
        assert_eq!(
            NativeHostSession::verification_field(SemanticRole::Select),
            Some("expanded")
        );
    }

    #[test]
    fn public_type_hands_an_unchanged_empty_field_to_the_agent_client() {
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
        // A native conclusion learned before software generations were
        // recorded must not be reapplied inside the public software-only
        // closed loop after its generation-aware preflight ignored it.
        session
            .with_input_policy(|policy| {
                policy.remember(
                    "https://fixture.test/form".into(),
                    SemanticRole::TextField,
                    "Full name".into(),
                    LearnedBackend::Native,
                    PolicyEvidence::UnverifiedSoftwareReceipt,
                    None,
                )
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

        let worker_session = Arc::clone(&session);
        let worker = std::thread::spawn(move || {
            worker_session.handle_control(ControlRequest {
                id: 12,
                method: "web.act_object".into(),
                params: json!({
                    "tab_id":"tab-1",
                    "object_id":"field-1",
                    "operation":"type",
                    "document_id":"document-1",
                    "basis_revision":1,
                    "text":"ordinary text",
                    "timeout_ms":1
                }),
                capability: worker_session.capability(),
            })
        });

        // The public preflight resolves the page once and passes that exact
        // scope into the shared closed loop; no duplicate tabs.list call.
        let scope = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(scope.kind, "tabs.list");
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: scope.request_id,
                payload: json!({"tabs":[{"tab_id":"tab-1","url":"https://fixture.test/form","title":"","active":true,"observation_ready":true,"ownership":"agent","provenance":"saccade_tabs_open"}]}),
            })
            .unwrap();

        let prepare = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(prepare.kind, "prepare_action");
        assert_eq!(prepare.payload["defer_scroll"], Value::Bool(true));
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: prepare.request_id,
                payload: serde_json::to_value(PreparedAction {
                    browser_instance_id: "browser-1".into(),
                    tab_id: "tab-1".into(),
                    document_id: "document-1".into(),
                    basis_revision: 1,
                    viewport_revision: 1,
                    object_id: "field-1".into(),
                    action_token: before_object.action_token.unwrap(),
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
                })
                .unwrap(),
            })
            .unwrap();

        let dispatched = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(dispatched.kind, "soft_action");
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: dispatched.request_id,
                payload: json!({"accepted":true}),
            })
            .unwrap();

        let response = worker.join().unwrap();
        assert!(response.ok, "{:?}", response.error);
        let result = response.result.unwrap();
        assert_eq!(result["dispatch"], "external_execution_required");
        assert_eq!(result["software_dispatch"], "accepted_by_software");
        assert_eq!(result["verified"], false);
        assert_eq!(result["retry_safe"], true);
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("ordinary text"));
    }

    #[test]
    fn registry_automatically_selects_software_click_for_reflex_target() {
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
        let mut before_target = reflex_target(10.0);
        // A transiently hidden software target is allowed to reach the
        // Extension's bounded actionability wait. Native execution must still
        // fail closed before any physical input.
        before_target.visibility = Visibility::Hidden;
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(1, before_target.clone())).unwrap(),
            })
            .unwrap();
        let request = ActionRequest {
            browser_instance_id: "browser-1".into(),
            tab_id: "tab-1".into(),
            document_id: "document-1".into(),
            basis_revision: 1,
            action_token: before_target.action_token.clone().unwrap(),
            operation: ActionOperation::Click,
            payload: ActionPayload::None,
        };
        let control = ControlRequest {
            id: 10,
            method: "web.act".into(),
            params: serde_json::to_value(&request).unwrap(),
            capability: session.capability(),
        };
        let worker_session = Arc::clone(&session);
        let worker = std::thread::spawn(move || worker_session.handle_control(control));

        let list = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(list.kind, "tabs.list");
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: list.request_id,
                payload: json!({"tabs":[{"tab_id":"tab-1","url":"https://fixture.test/game"}]}),
            })
            .unwrap();
        let prepare = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(prepare.kind, "prepare_action");
        let mut rebased_target = before_target.clone();
        rebased_target.document_bounds.x = 11.0;
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(2, rebased_target)).unwrap(),
            })
            .unwrap();
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: prepare.request_id,
                payload: serde_json::to_value(PreparedAction {
                    browser_instance_id: "browser-1".into(),
                    tab_id: "tab-1".into(),
                    document_id: "document-1".into(),
                    basis_revision: 2,
                    viewport_revision: 2,
                    object_id: "reflex-1".into(),
                    action_token: request.action_token.clone(),
                    operation: ActionOperation::Click,
                    // Software execution is object-addressed. It must not
                    // depend on physical screen geometry being available.
                    screen_bounds: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 0.0,
                        height: 0.0,
                    },
                    visible: false,
                    topmost: false,
                    focus_verified: false,
                    selection_index: None,
                })
                .unwrap(),
            })
            .unwrap();
        let software = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(software.kind, "soft_click");
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: software.request_id,
                payload: json!({"accepted":true}),
            })
            .unwrap();
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(3, reflex_target(50.0))).unwrap(),
            })
            .unwrap();

        let response = worker.join().unwrap();
        assert!(response.ok, "{:?}", response.error);
        let receipt: ActionReceipt = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(receipt.dispatch_status, DispatchStatus::AcceptedBySoftware);
        assert_eq!(receipt.postcondition, PostconditionStatus::Verified);
        assert!(native_rx.try_recv().is_err());
        session
            .with_input_policy(|policy| {
                policy.remember(
                    "https://fixture.test/game".into(),
                    SemanticRole::ReflexTarget,
                    "ReflexTarget".into(),
                    LearnedBackend::Native,
                    PolicyEvidence::UnverifiedSoftwareReceipt,
                    // Same generation as this session, so the rule binds.
                    session.software_generation(),
                )
            })
            .unwrap();
        assert_eq!(
            session
                .input_backend(
                    SemanticRole::ReflexTarget,
                    "https://fixture.test/game",
                    "ReflexTarget",
                    None,
                )
                .unwrap()
                .0,
            InputBackend::Native
        );
        assert!(session
            .input_backend(
                SemanticRole::ReflexTarget,
                "https://fixture.test/game",
                "ReflexTarget",
                Some(InputBackend::Soft),
            )
            .unwrap_err()
            .to_string()
            .contains("user-local input policy requires native"));
    }

    #[test]
    fn reflex_loop_recovers_invalid_preparation_within_bounded_budget() {
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
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(1, reflex_target(10.0))).unwrap(),
            })
            .unwrap();

        let control = ControlRequest {
            id: 30,
            method: "web.reflex.run".into(),
            params: json!({
                "tab_id":"tab-1",
                "input_backend":"soft",
                "max_actions":1,
                "timeout_ms":1000
            }),
            capability: session.capability(),
        };
        let worker_session = Arc::clone(&session);
        let worker = std::thread::spawn(move || worker_session.handle_control(control));

        let list = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(list.kind, "tabs.list");
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: list.request_id,
                payload: json!({"tabs":[{"tab_id":"tab-1","url":"https://fixture.test/game"}]}),
            })
            .unwrap();

        let invalid_prepare = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(invalid_prepare.kind, "prepare_action");
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: invalid_prepare.request_id,
                payload: serde_json::to_value(PreparedAction {
                    browser_instance_id: "browser-1".into(),
                    tab_id: "tab-1".into(),
                    document_id: "document-1".into(),
                    basis_revision: 1,
                    viewport_revision: 1,
                    object_id: "stale-reflex-object".into(),
                    action_token: invalid_prepare.payload["action_token"]
                        .as_str()
                        .unwrap()
                        .into(),
                    operation: ActionOperation::Click,
                    // Wrong identity exercises bounded recovery. A software
                    // prepare may legitimately carry no physical geometry.
                    screen_bounds: Rect {
                        width: 0.0,
                        ..reflex_target(10.0).document_bounds
                    },
                    visible: false,
                    topmost: true,
                    focus_verified: true,
                    selection_index: None,
                })
                .unwrap(),
            })
            .unwrap();
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(2, reflex_target(20.0))).unwrap(),
            })
            .unwrap();

        let prepare = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(prepare.kind, "prepare_action");
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: prepare.request_id,
                payload: serde_json::to_value(PreparedAction {
                    browser_instance_id: "browser-1".into(),
                    tab_id: "tab-1".into(),
                    document_id: "document-1".into(),
                    basis_revision: 2,
                    viewport_revision: 1,
                    object_id: "reflex-1".into(),
                    action_token: prepare.payload["action_token"].as_str().unwrap().into(),
                    operation: ActionOperation::Click,
                    screen_bounds: reflex_target(20.0).document_bounds,
                    visible: true,
                    topmost: true,
                    focus_verified: true,
                    selection_index: None,
                })
                .unwrap(),
            })
            .unwrap();
        let software = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(software.kind, "soft_click");
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: software.request_id,
                payload: json!({"accepted":true}),
            })
            .unwrap();
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(3, reflex_target(30.0))).unwrap(),
            })
            .unwrap();

        let response = worker.join().unwrap();
        assert!(response.ok, "{:?}", response.error);
        let report = response.result.unwrap();
        assert_eq!(report["actions"], 1);
        assert_eq!(report["failures"], 0);
        assert_eq!(report["bounded_recoveries"], 1);
        assert_eq!(report["recovery_budget_ms"], 45);
        assert_eq!(report["stop_reason"], "max_actions");
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
        assert_eq!(capabilities["schema"], "saccade.capabilities/6");
        assert_eq!(capabilities["product"], "truth_layer");
        assert_eq!(capabilities["execution_owner"], "agent_client");
        assert!(capabilities.get("native_accessibility_trusted").is_none());
        assert!(capabilities.get("restricted_surfaces").is_none());
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
        let mut ready = snapshot(1, field(false));
        ready.tab_id = "tab-7".into();
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(ready).unwrap(),
            })
            .unwrap();

        let response = worker.join().unwrap();
        assert!(response.ok);
        assert_eq!(
            response.result.unwrap(),
            json!({"tab_id":"tab-7","opened":true,"observation_ready":true})
        );
    }

    #[test]
    fn tabs_open_claim_arms_without_a_tab_and_confirms_one_agent_client_tab() {
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

        // Arm forwards only url and claim, and returns without any tab
        // identity or observation wait.
        let arm = ControlRequest {
            id: 4,
            method: "tabs.open".into(),
            params: json!({"url":"https://fixture.test/form","claim":"arm"}),
            capability: session.capability(),
        };
        let worker_session = Arc::clone(&session);
        let worker = std::thread::spawn(move || worker_session.handle_control(arm));
        let outbound = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(outbound.kind, "tabs.open");
        assert_eq!(
            outbound.payload,
            json!({"url":"https://fixture.test/form","claim":"arm"})
        );
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: outbound.request_id,
                payload: json!({
                    "claim":"armed",
                    "claim_id":"claim.abc",
                    "origin":"https://fixture.test",
                    "expires_in_ms":30000
                }),
            })
            .unwrap();
        let armed = worker.join().unwrap();
        assert!(armed.ok);
        let armed = armed.result.unwrap();
        assert_eq!(armed["claim"], "armed");
        assert!(armed.get("tab_id").is_none());
        assert!(armed.get("opened").is_none());
        assert!(armed.get("observation_ready").is_none());

        // Confirm forwards the Agent-supplied identity and only then waits for
        // Truth from that exact tab.
        let confirm = ControlRequest {
            id: 5,
            method: "tabs.open".into(),
            params: json!({
                "url":"https://fixture.test/form",
                "claim":"confirm",
                "claim_id":"claim.abc",
                "tab_id":"tab-7"
            }),
            capability: session.capability(),
        };
        let worker_session = Arc::clone(&session);
        let worker = std::thread::spawn(move || worker_session.handle_control(confirm));
        let outbound = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(
            outbound.payload,
            json!({
                "url":"https://fixture.test/form",
                "claim":"confirm",
                "claim_id":"claim.abc",
                "tab_id":"tab-7"
            })
        );
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: outbound.request_id,
                payload: json!({
                    "tab_id":"tab-7",
                    "claim":"confirmed",
                    "opened":false,
                    "provenance":"agent_client"
                }),
            })
            .unwrap();
        let mut ready = snapshot(1, field(false));
        ready.tab_id = "tab-7".into();
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(ready).unwrap(),
            })
            .unwrap();
        let confirmed = worker.join().unwrap();
        assert!(confirmed.ok);
        assert_eq!(
            confirmed.result.unwrap(),
            json!({
                "tab_id":"tab-7",
                "claim":"confirmed",
                "opened":false,
                "provenance":"agent_client",
                "observation_ready":true
            })
        );

        // active belongs to the Saccade-created route only, and arm carries no
        // tab identity.
        for rejected in [
            json!({"url":"https://fixture.test/form","claim":"arm","tab_id":"tab-7"}),
            json!({"url":"https://fixture.test/form","claim":"confirm","claim_id":"claim.abc","active":true}),
            json!({"url":"https://fixture.test/form","claim":"adopt"}),
        ] {
            let response = session.handle_control(ControlRequest {
                id: 6,
                method: "tabs.open".into(),
                params: rejected,
                capability: session.capability(),
            });
            assert!(!response.ok);
            assert!(out_rx.try_recv().is_err());
        }
    }

    #[test]
    fn tabs_close_is_forwarded_and_discards_host_observation_state() {
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
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot(1, field(false))).unwrap(),
            })
            .unwrap();

        let control = ControlRequest {
            id: 5,
            method: "tabs.close".into(),
            params: json!({"tab_id":"tab-1"}),
            capability: session.capability(),
        };
        let worker_session = Arc::clone(&session);
        let worker = std::thread::spawn(move || worker_session.handle_control(control));

        let outbound = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(outbound.kind, "tabs.close");
        assert_eq!(outbound.payload, json!({"tab_id":"tab-1"}));
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: outbound.request_id,
                payload: json!({"tab_id":"tab-1","closed":true}),
            })
            .unwrap();

        let response = worker.join().unwrap();
        assert!(response.ok);
        assert_eq!(
            response.result.unwrap(),
            json!({"tab_id":"tab-1","closed":true})
        );
        assert!(session.current_observation("tab-1").is_err());
    }

    #[test]
    fn form_fill_keeps_multiple_control_loops_local_and_redacts_values() {
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
        let first = named_field("field-1", "First", "first-a", false);
        let second = named_field("field-2", "Second", "second-a", false);
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot_many(
                    1,
                    vec![first.clone(), second.clone()],
                ))
                .unwrap(),
            })
            .unwrap();

        let secret_one = "FIRST-SENTINEL";
        let secret_two = "SECOND-SENTINEL";
        let control = ControlRequest {
            id: 20,
            method: "web.form.fill".into(),
            params: json!({
                "browser_instance_id":"browser-1",
                "tab_id":"tab-1",
                "document_id":"document-1",
                "basis_revision":1,
                "actions":[
                    {"action_token":first.action_token,"operation":"type","payload":{"kind":"text","text":secret_one}},
                    {"action_token":second.action_token,"operation":"type","payload":{"kind":"text","text":secret_two}}
                ]
            }),
            capability: session.capability(),
        };
        let worker_session = Arc::clone(&session);
        let worker = std::thread::spawn(move || worker_session.handle_control(control));

        let list = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(list.kind, "tabs.list");
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: list.request_id,
                payload: json!({"tabs":[{"tab_id":"tab-1","url":"https://fixture.test/form"}]}),
            })
            .unwrap();

        let stale_prepare = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(stale_prepare.kind, "prepare_action");
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "response".into(),
                request_id: stale_prepare.request_id,
                payload: json!({"error":"stale action basis"}),
            })
            .unwrap();
        session
            .handle_native(NativeEnvelope {
                protocol: HOST_PROTOCOL.into(),
                kind: "observation".into(),
                request_id: None,
                payload: serde_json::to_value(snapshot_many(
                    2,
                    vec![
                        named_field("field-1", "First", "first-b", false),
                        named_field("field-2", "Second", "second-b", false),
                    ],
                ))
                .unwrap(),
            })
            .unwrap();

        for (sequence, target_id) in ["field-1", "field-2"].into_iter().enumerate() {
            let prepare = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
            assert_eq!(prepare.kind, "prepare_action");
            assert_eq!(prepare.payload["operation"], "type");
            assert!(!serde_json::to_string(&prepare.payload)
                .unwrap()
                .contains("SENTINEL"));
            session
                .handle_native(NativeEnvelope {
                    protocol: HOST_PROTOCOL.into(),
                    kind: "response".into(),
                    request_id: prepare.request_id,
                    payload: serde_json::to_value(PreparedAction {
                        browser_instance_id: "browser-1".into(),
                        tab_id: "tab-1".into(),
                        document_id: "document-1".into(),
                        basis_revision: (sequence + 2) as u64,
                        viewport_revision: 1,
                        object_id: target_id.into(),
                        action_token: prepare.payload["action_token"].as_str().unwrap().into(),
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
                    })
                    .unwrap(),
                })
                .unwrap();
            // The value reaches the Extension because setting it requires that;
            // it must still never appear in the receipt.
            let dispatched = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
            assert_eq!(dispatched.kind, "soft_action");
            session
                .handle_native(NativeEnvelope {
                    protocol: HOST_PROTOCOL.into(),
                    kind: "response".into(),
                    request_id: dispatched.request_id,
                    payload: serde_json::json!({"accepted":true}),
                })
                .unwrap();
            let revision = (sequence + 3) as u64;
            let next_first = named_field("field-1", "First", &format!("first-{revision}"), true);
            let next_second = named_field(
                "field-2",
                "Second",
                &format!("second-{revision}"),
                sequence == 1,
            );
            session
                .handle_native(NativeEnvelope {
                    protocol: HOST_PROTOCOL.into(),
                    kind: "observation".into(),
                    request_id: None,
                    payload: serde_json::to_value(snapshot_many(
                        revision,
                        vec![next_first, next_second],
                    ))
                    .unwrap(),
                })
                .unwrap();
        }

        let response = worker.join().unwrap();
        assert!(response.ok, "{:?}", response.error);
        let result = response.result.unwrap();
        assert_eq!(result["all_verified"], true);
        assert_eq!(result["completed"], 2);
        assert_eq!(result["next_basis_revision"], 4);
        assert_eq!(result["revision"], 4);
        assert_eq!(result["document_id"], "document-1");
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains(secret_one));
        assert!(!serialized.contains(secret_two));
    }
}
