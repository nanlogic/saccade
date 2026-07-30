//! Native Host session authority for the cataloged control families.

use std::collections::{BTreeMap, BTreeSet};
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
    ActionPayload, ActionReceipt, ActionRequest, ControlError, ControlRequest, ControlResponse,
    DispatchStatus, HostGrant, LocalAddress, NativeEnvelope, ObservationSnapshot,
    PostconditionStatus, PreparedAction, SemanticRole, HOST_PROTOCOL, SESSION_CAPABILITY_SCHEME,
};
use serde_json::{json, Value};

use crate::input_policy::{page_scope, LearnedBackend, LocalInputPolicy, PolicyEvidence};
use crate::native_messaging;
use crate::platform_input::PlatformInput;
use crate::profile::Profile;
use crate::{ClosedLoopEngine, ClosedLoopError, NativeInput, ObservationSource};

const EXTENSION_TIMEOUT: Duration = Duration::from_secs(10);
const POST_ACTION_TIMEOUT: Duration = Duration::from_secs(2);
const POST_ACTION_QUIET_WINDOW: Duration = Duration::from_millis(300);
const SELECT_POST_ACTION_QUIET_WINDOW: Duration = Duration::from_millis(750);
const REFLEX_POST_ACTION_QUIET_WINDOW: Duration = Duration::from_millis(1);
const VERIFIED_POST_ACTION_QUIET_WINDOW: Duration = Duration::from_millis(25);

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
    extension_connected: AtomicBool,
    observations: Mutex<ObservationState>,
    observation_changed: Condvar,
    pending: Mutex<BTreeMap<u64, mpsc::Sender<Value>>>,
    next_request_id: AtomicU64,
    outbound: Arc<dyn ExtensionOutbound>,
    profile: Profile,
    input_policy: Mutex<LocalInputPolicy>,
    engine: Mutex<ClosedLoopEngine>,
    native: Mutex<Box<dyn NativeInput>>,
}

#[derive(Default)]
struct ObservationState {
    current: BTreeMap<String, ObservationSnapshot>,
    retired_documents: BTreeMap<String, BTreeSet<String>>,
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
        let input_policy = LocalInputPolicy::load(&runtime_dir)?;
        Ok(Self {
            capability: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes),
            runtime_dir,
            endpoint: Mutex::new(None),
            browser_instance_id: Mutex::new(None),
            extension_connected: AtomicBool::new(false),
            observations: Mutex::new(ObservationState::default()),
            observation_changed: Condvar::new(),
            pending: Mutex::new(BTreeMap::new()),
            next_request_id: AtomicU64::new(1),
            outbound,
            profile,
            input_policy: Mutex::new(input_policy),
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
            "system.capabilities" => {
                let learned_rules = self.input_policy.lock().map_err(lock_error)?.rules().len();
                Ok(json!({
                "schema":"saccade.capabilities/4",
                "observation_schema":saccade_protocol::OBSERVATION_SCHEMA,
                "host_protocol":HOST_PROTOCOL,
                "perception":"dom_extension",
                "input":"registry_selected",
                "input_backends":["native","soft"],
                "input_policy":{
                    "default_source":"control_catalog",
                    "local_learning":"per_page_control",
                    "learned_rules":learned_rules
                },
                "native_accessibility_trusted":crate::platform_input::accessibility_trusted(),
                "browser_support":["chrome","edge"],
                "extension_connected":self.extension_connected.load(Ordering::Acquire),
                "first_slice":["button","text_field","checkbox","select"],
                "restricted_surfaces":[{
                    "kind":"browser_owned_confirm",
                    "automatic_action":false,
                    "human_confirmation":"required"
                }],
                "profile":{
                    "name":self.profile.name,
                    "behavior":self.profile.behavior
                }
                }))
            }
            "web.observe" => {
                let object = params
                    .as_object()
                    .context("web.observe params must be an object")?;
                for key in object.keys() {
                    if !["tab_id", "after_revision", "timeout_ms"].contains(&key.as_str()) {
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
                            Duration::from_millis(timeout_ms),
                        )?
                    }
                    None => self.current_observation(tab_id)?,
                };
                Ok(serde_json::to_value(snapshot)?)
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
                let mut opened = self.request_extension(
                    "tabs.open",
                    json!({"url":url,"active":active}),
                    EXTENSION_TIMEOUT,
                )?;
                let tab_id = required_string(&opened, "tab_id")?.to_string();
                self.wait_for_first_observation(&tab_id, EXTENSION_TIMEOUT)?;
                opened["observation_ready"] = Value::Bool(true);
                Ok(opened)
            }
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

    fn act(
        &self,
        params: Value,
        backend_override: Option<InputBackend>,
        known_page_scope: Option<&str>,
    ) -> Result<Value> {
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
        let (backend, registered_policy) =
            self.input_backend(target_role, &page, &control_name, backend_override)?;
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
                "payload":extension_payload
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
            } else {
                POST_ACTION_QUIET_WINDOW
            },
            allow_document_transition: target_role != SemanticRole::ReflexTarget,
            reflex_loop_class,
            reflex_occurrence,
        };
        let receipt: ActionReceipt = match backend {
            InputBackend::Native => {
                let mut native = self.native.lock().map_err(lock_error)?;
                engine.execute(&request, &before, &prepared, native.as_mut(), &mut source)?
            }
            InputBackend::Soft => {
                let mut software = SoftwareInput { session: self };
                engine.execute(&request, &before, &prepared, &mut software, &mut source)?
            }
        };
        self.learn_from_receipt(
            &page,
            target_role,
            &control_name,
            backend,
            registered_policy,
            &receipt,
        )?;
        Ok(serde_json::to_value(receipt)?)
    }

    fn input_backend(
        &self,
        role: SemanticRole,
        page: &str,
        control: &str,
        backend_override: Option<InputBackend>,
    ) -> Result<(InputBackend, InputPolicy)> {
        let policy = self.engine.lock().map_err(lock_error)?.input_policy(role)?;
        let learned = self
            .input_policy
            .lock()
            .map_err(lock_error)?
            .backend_for(page, role, control);
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
            self.input_policy.lock().map_err(lock_error)?.remember(
                page.to_string(),
                role,
                control.to_string(),
                learned_backend,
                evidence,
            )?;
        }
        Ok(())
    }

    fn input_policy_list(&self, params: Value) -> Result<Value> {
        if !params.as_object().is_some_and(|object| object.is_empty()) {
            bail!("input_policy.list takes no arguments");
        }
        let policy = self.input_policy.lock().map_err(lock_error)?;
        Ok(json!({
            "schema":"saccade.input-policy/1",
            "rules":policy.rules()
        }))
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
        self.input_policy.lock().map_err(lock_error)?.remember(
            page.clone(),
            target.role,
            control.clone(),
            LearnedBackend::Native,
            PolicyEvidence::UserRememberedNative,
        )?;
        Ok(json!({
            "remembered":true,
            "page":page,
            "role":target.role,
            "control":control,
            "backend":"native"
        }))
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
                return Ok(json!({
                    "schema":"saccade.form-result/1",
                    "completed":index,
                    "all_verified":false,
                    "steps":summaries,
                    "post_action_observation":receipt.post_action_observation
                }));
            }
        }
        let final_observation = self.current_observation(tab_id)?;
        Ok(json!({
            "schema":"saccade.form-result/1",
            "completed":summaries.len(),
            "all_verified":true,
            "steps":summaries,
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
        if observations
            .retired_documents
            .get(&snapshot.tab_id)
            .is_some_and(|documents| documents.contains(&snapshot.document_id))
        {
            bail!("observation belongs to a retired document");
        }
        if let Some(previous) = observations.current.get(&snapshot.tab_id) {
            if previous.document_id == snapshot.document_id {
                if snapshot.revision <= previous.revision {
                    bail!("observation revision did not advance");
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
        self.profile.filter_observation(&mut snapshot);
        snapshot.validate()?;
        observations
            .current
            .insert(snapshot.tab_id.clone(), snapshot);
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
            .current
            .get(tab_id)
            .cloned()
            .context("no current observation for tab")
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
        timeout: Duration,
    ) -> Result<ObservationSnapshot> {
        let deadline = Instant::now() + timeout;
        let mut observations = self.observations.lock().map_err(lock_error)?;
        loop {
            if let Some(snapshot) = observations.current.get(tab_id) {
                if snapshot.revision > revision {
                    return Ok(snapshot.clone());
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
}

impl NativeInput for SoftwareInput<'_> {
    fn execute(
        &mut self,
        primitive: saccade_control_sdk::NativePrimitive,
        prepared: &PreparedAction,
        payload: &ActionPayload,
        _: Option<&str>,
    ) -> DispatchStatus {
        if primitive != saccade_control_sdk::NativePrimitive::PrimaryClick
            || prepared.operation != saccade_protocol::ActionOperation::Click
            || payload != &ActionPayload::None
        {
            return DispatchStatus::Unsupported;
        }
        let request = json!({
            "browser_instance_id":prepared.browser_instance_id,
            "tab_id":prepared.tab_id,
            "document_id":prepared.document_id,
            "basis_revision":prepared.basis_revision,
            "action_token":prepared.action_token,
            "operation":prepared.operation,
            "payload":{"kind":"none"}
        });
        match self
            .session
            .request_extension("soft_click", request, EXTENSION_TIMEOUT)
        {
            Ok(response) if response.get("accepted").and_then(Value::as_bool) == Some(true) => {
                DispatchStatus::AcceptedBySoftware
            }
            Err(error)
                if error.to_string().contains("stale action basis")
                    || error.to_string().contains("not current")
                    || error.to_string().contains("current reflex target") =>
            {
                DispatchStatus::StaleBeforeDispatch
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
    ]
    .iter()
    .any(|needle| detail.contains(needle))
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

    fn snapshot_many(revision: u64, objects: Vec<ObservedObject>) -> ObservationSnapshot {
        let mut result = snapshot(revision, objects[0].clone());
        result.objects = objects;
        result
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
        let before_target = reflex_target(10.0);
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
                    object_id: "reflex-1".into(),
                    action_token: request.action_token.clone(),
                    operation: ActionOperation::Click,
                    screen_bounds: before_target.document_bounds,
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
                payload: serde_json::to_value(snapshot(2, reflex_target(50.0))).unwrap(),
            })
            .unwrap();

        let response = worker.join().unwrap();
        assert!(response.ok, "{:?}", response.error);
        let receipt: ActionReceipt = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(receipt.dispatch_status, DispatchStatus::AcceptedBySoftware);
        assert_eq!(receipt.postcondition, PostconditionStatus::Verified);
        assert!(native_rx.try_recv().is_err());
        session
            .input_policy
            .lock()
            .unwrap()
            .remember(
                "https://fixture.test/game".into(),
                SemanticRole::ReflexTarget,
                "ReflexTarget".into(),
                LearnedBackend::Native,
                PolicyEvidence::UnverifiedSoftwareReceipt,
            )
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
            capabilities["restricted_surfaces"][0]["kind"],
            "browser_owned_confirm"
        );
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
    fn form_fill_keeps_multiple_control_loops_local_and_redacts_values() {
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
            assert_eq!(
                native_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
                NativePrimitive::UnicodeText
            );
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
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains(secret_one));
        assert!(!serialized.contains(secret_two));
    }
}
