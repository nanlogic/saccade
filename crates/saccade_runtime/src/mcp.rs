//! MCP adapter and per-Agent Browser projection over the single HostClient interface.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::browser_wake;
use crate::profile::Profile;
use crate::session::load_expected_extension_candidate;
use anyhow::{anyhow, bail, Context, Result};
use saccade_host_client::HostClient;
use saccade_protocol::{ActionReceipt, Affordance, ChangeKind, ObservationSnapshot, ProtocolError};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const MCP_VERSION: &str = "2025-03-26";
const ECONOMY_COALESCE_WINDOW: Duration = Duration::from_millis(150);
const MAX_FULL_PAGE_BYTES: usize = 14_000;
const MAX_CATALOG_PAGE_BYTES: usize = 48_000;
const MAX_DETAIL_OBJECTS: usize = 64;
const MAX_QUERY_OBJECTS: usize = 32;
const CATALOG_PREVIEW_CHARS: usize = 96;
const QUERY_MAX_SETTLE: Duration = Duration::from_millis(6_000);
const QUERY_QUIET_WINDOW: Duration = Duration::from_millis(250);
const INITIALIZE_INSTRUCTIONS: &str = "Call saccade.system.capabilities once before browser work and obey its Profile behavior. Saccade is the primary route for navigation and page Truth; if it remains unhealthy after one reconnect, stop instead of falling back to another browser. If the Extension is missing or outdated, tell the user to run `npx -y @nanlogic/saccade doctor`; it prints the exact store link and browser steps. Open a known URL with saccade.tabs.open, read one bounded Truth view, then use object-addressed saccade.act. Fold verified transitions and later revision deltas into the cached view. Resync only the exact tab if that cache is lost. Use Agent-client execution only after external_execution_required with retry_safe true.";
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

pub fn serve(grant_path: PathBuf) -> Result<()> {
    serve_mode(grant_path, McpMode::Truth)
}

pub fn serve_reference_actuator(grant_path: PathBuf) -> Result<()> {
    serve_mode(grant_path, McpMode::Reference)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum McpMode {
    Truth,
    Reference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TruthDeliveryMode {
    Live,
    Economy,
}

impl TruthDeliveryMode {
    fn from_arguments(arguments: &mut Value) -> Result<Self> {
        let value = arguments
            .as_object_mut()
            .context("tool arguments must be an object")?
            .remove("delivery_mode");
        match value.as_ref().and_then(Value::as_str) {
            None | Some("live") => Ok(Self::Live),
            Some("economy") => Ok(Self::Economy),
            Some(_) => bail!("delivery_mode must be live or economy"),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Economy => "economy",
        }
    }
}

fn serve_mode(grant_path: PathBuf, mode: McpMode) -> Result<()> {
    let startup_profile = grant_path
        .parent()
        .map(Profile::load)
        .transpose()?
        .unwrap_or_default();
    let host = HostClient::connect(grant_path)?;
    let subscriptions = Arc::new(Mutex::new(BTreeMap::new()));
    let output = Arc::new(Mutex::new(std::io::stdout()));
    let running = Arc::new(AtomicBool::new(true));
    spawn_resource_watcher(
        host.clone(),
        Arc::clone(&subscriptions),
        Arc::clone(&output),
        Arc::clone(&running),
    );
    let result = serve_shared_io(
        &host,
        BufReader::new(std::io::stdin()),
        output,
        subscriptions,
        mode,
        startup_profile,
    );
    running.store(false, Ordering::Release);
    result
}

fn serve_shared_io(
    host: &HostClient,
    input: impl BufRead,
    output: Arc<Mutex<impl Write>>,
    subscriptions: Arc<Mutex<BTreeMap<String, u64>>>,
    mode: McpMode,
    startup_profile: Profile,
) -> Result<()> {
    let mut agent_views = AgentViewState::new(mode == McpMode::Reference);
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(request) => request,
            Err(error) => {
                write_rpc(
                    &mut *output.lock().map_err(lock_error)?,
                    Value::Null,
                    Err((-32700, error.to_string())),
                )?;
                continue;
            }
        };
        let Some(id) = request.id.clone() else {
            continue;
        };
        let result = dispatch(
            host,
            &mut agent_views,
            &subscriptions,
            mode,
            &startup_profile,
            request,
        )
        .map_err(|error| (-32000, error.to_string()));
        write_rpc(&mut *output.lock().map_err(lock_error)?, id, result)?;
    }
    Ok(())
}

fn dispatch(
    host: &HostClient,
    agent_views: &mut AgentViewState,
    subscriptions: &Arc<Mutex<BTreeMap<String, u64>>>,
    mode: McpMode,
    startup_profile: &Profile,
    request: RpcRequest,
) -> Result<Value> {
    let diagnostics = diagnostic_input_overrides_enabled();
    if request.jsonrpc != "2.0" {
        bail!("unsupported JSON-RPC version {}", request.jsonrpc);
    }
    match request.method.as_str() {
        "initialize" => initialize(host, mode, startup_profile),
        "notifications/initialized" | "ping" => Ok(json!({})),
        "resources/list" => list_truth_resources(host),
        "resources/read" => {
            read_truth_resource(host, agent_views, &request.params, startup_profile)
        }
        "resources/subscribe" => subscribe_truth_resource(host, subscriptions, &request.params),
        "resources/unsubscribe" => unsubscribe_truth_resource(subscriptions, &request.params),
        "tools/list" => Ok(json!({"tools":tools(mode, diagnostics)})),
        "tools/call" => {
            let name = string(&request.params, "name")?;
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let value = call_tool(
                host,
                agent_views,
                name,
                arguments,
                mode,
                diagnostics,
                startup_profile,
            )?;
            let summary = tool_result_summary(&value);
            Ok(
                json!({"content":[{"type":"text","text":summary}],"structuredContent":value,"isError":false}),
            )
        }
        method => bail!("unknown JSON-RPC method {method}"),
    }
}

fn initialize(host: &HostClient, mode: McpMode, startup_profile: &Profile) -> Result<Value> {
    match host.call(
        NEXT_ID.fetch_add(1, Ordering::Relaxed),
        "system.capabilities",
        json!({}),
        Duration::from_millis(250),
    ) {
        Ok(_) | Err(ProtocolError::TransportUnavailable(_) | ProtocolError::Timeout) => {}
        Err(error) => return Err(error.into()),
    }
    let _ = startup_profile;
    Ok(json!({
        "protocolVersion":MCP_VERSION,
        "capabilities":{"tools":{"listChanged":false},"resources":{"subscribe":true,"listChanged":false}},
        "serverInfo":{"name":if mode == McpMode::Truth {"saccade-truth-layer"} else {"saccade-reference-actuator"},"version":env!("CARGO_PKG_VERSION")},
        "instructions":INITIALIZE_INSTRUCTIONS
    }))
}

fn truth_uri(tab_id: &str) -> String {
    format!("saccade://tabs/{tab_id}/truth")
}

fn tab_id_from_truth_uri(value: &Value) -> Result<String> {
    let uri = string(value, "uri")?;
    let tab_id = uri
        .strip_prefix("saccade://tabs/")
        .and_then(|rest| rest.strip_suffix("/truth"))
        .filter(|tab_id| !tab_id.is_empty() && tab_id.chars().all(|ch| ch.is_ascii_digit()))
        .context("resource URI must identify a Saccade tab Truth Layer")?;
    Ok(tab_id.to_string())
}

fn list_truth_resources(host: &HostClient) -> Result<Value> {
    let listed = host.call(
        NEXT_ID.fetch_add(1, Ordering::Relaxed),
        "tabs.list",
        json!({}),
        Duration::from_secs(10),
    )?;
    let resources = listed
        .get("tabs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|tab| tab.get("observation_ready").and_then(Value::as_bool) == Some(true))
        .filter_map(|tab| {
            let tab_id = tab.get("tab_id")?.as_str()?;
            let title = tab
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Saccade tab");
            Some(json!({
                "uri":truth_uri(tab_id),
                "name":format!("Truth Layer for tab {tab_id}"),
                "title":title,
                "mimeType":"application/vnd.saccade.agent-view+json"
            }))
        })
        .collect::<Vec<_>>();
    Ok(json!({"resources":resources}))
}

fn read_truth_resource(
    host: &HostClient,
    agent_views: &mut AgentViewState,
    params: &Value,
    startup_profile: &Profile,
) -> Result<Value> {
    let tab_id = tab_id_from_truth_uri(params)?;
    let view = call_tool(
        host,
        agent_views,
        "saccade.truth.read",
        json!({"tab_id":tab_id}),
        McpMode::Truth,
        false,
        startup_profile,
    )?;
    Ok(json!({"contents":[{
        "uri":truth_uri(&tab_id),
        "mimeType":"application/vnd.saccade.agent-view+json",
        "text":serde_json::to_string(&view)?
    }]}))
}

fn subscribe_truth_resource(
    host: &HostClient,
    subscriptions: &Arc<Mutex<BTreeMap<String, u64>>>,
    params: &Value,
) -> Result<Value> {
    let tab_id = tab_id_from_truth_uri(params)?;
    let observation: ObservationSnapshot = serde_json::from_value(host.call(
        NEXT_ID.fetch_add(1, Ordering::Relaxed),
        "web.observe",
        json!({"tab_id":tab_id}),
        Duration::from_secs(10),
    )?)?;
    observation.validate()?;
    subscriptions
        .lock()
        .map_err(lock_error)?
        .insert(tab_id, observation.revision);
    Ok(json!({}))
}

fn unsubscribe_truth_resource(
    subscriptions: &Arc<Mutex<BTreeMap<String, u64>>>,
    params: &Value,
) -> Result<Value> {
    let tab_id = tab_id_from_truth_uri(params)?;
    subscriptions.lock().map_err(lock_error)?.remove(&tab_id);
    Ok(json!({}))
}

fn spawn_resource_watcher(
    host: HostClient,
    subscriptions: Arc<Mutex<BTreeMap<String, u64>>>,
    output: Arc<Mutex<std::io::Stdout>>,
    running: Arc<AtomicBool>,
) {
    thread::Builder::new()
        .name("saccade-resource-watcher".into())
        .spawn(move || {
            while running.load(Ordering::Acquire) {
                let entries = subscriptions
                    .lock()
                    .map(|items| items.clone())
                    .unwrap_or_default();
                if entries.is_empty() {
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }
                for (tab_id, revision) in entries {
                    if !running.load(Ordering::Acquire) {
                        return;
                    }
                    let Ok(value) = host.call(
                        NEXT_ID.fetch_add(1, Ordering::Relaxed),
                        "web.observe",
                        json!({"tab_id":tab_id,"after_revision":revision,"timeout_ms":1000}),
                        Duration::from_millis(1500),
                    ) else {
                        continue;
                    };
                    let Ok(observation) = serde_json::from_value::<ObservationSnapshot>(value)
                    else {
                        continue;
                    };
                    if observation.validate().is_err() {
                        continue;
                    }
                    let mut items = match subscriptions.lock() {
                        Ok(items) => items,
                        Err(_) => return,
                    };
                    if items.get(&tab_id) != Some(&revision) {
                        continue;
                    }
                    items.insert(tab_id.clone(), observation.revision);
                    drop(items);
                    let notification = json!({
                        "jsonrpc":"2.0",
                        "method":"notifications/resources/updated",
                        "params":{"uri":truth_uri(&tab_id)}
                    });
                    let Ok(mut writer) = output.lock() else {
                        return;
                    };
                    if serde_json::to_writer(&mut *writer, &notification).is_err()
                        || writer.write_all(b"\n").is_err()
                        || writer.flush().is_err()
                    {
                        return;
                    }
                }
            }
        })
        .expect("resource watcher thread must start");
}

fn lock_error<T>(error: std::sync::PoisonError<T>) -> anyhow::Error {
    anyhow!("MCP state lock poisoned: {error}")
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn mcp_contract_hash(mode: McpMode) -> String {
    let contract = json!({
        "protocol_version":MCP_VERSION,
        "runtime_version":env!("CARGO_PKG_VERSION"),
        "instructions":INITIALIZE_INSTRUCTIONS,
        "tools":tools(mode, false),
    });
    sha256_hex(&serde_json::to_vec(&contract).expect("MCP contract serializes"))
}

pub fn truth_contract_hash() -> String {
    mcp_contract_hash(McpMode::Truth)
}

fn project_profile_capabilities(capabilities: &mut Value) -> String {
    let Some(profile) = capabilities.get("profile") else {
        return sha256_hex(b"null");
    };
    let digest = sha256_hex(&serde_json::to_vec(profile).expect("Profile serializes"));
    let name = profile
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("default")
        .to_string();
    let behavior = profile
        .get("behavior")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    capabilities["profile"] = json!({
        "name":name,
        "behavior":behavior,
        "behavior_delivery":"capabilities_once"
    });
    digest
}

fn disconnected_capabilities(host: &HostClient, startup_profile: &Profile) -> Result<Value> {
    let runtime_dir = host
        .grant_path()
        .parent()
        .context("Saccade grant path has no runtime directory")?;
    let expected_extension_candidate = load_expected_extension_candidate(runtime_dir)?
        .as_ref()
        .map(|candidate| candidate.value());
    Ok(json!({
        "schema":"saccade.capabilities/6",
        "product":"truth_layer",
        "observation_schema":saccade_protocol::OBSERVATION_SCHEMA,
        "host_protocol":saccade_protocol::HOST_PROTOCOL,
        "perception":"dom_extension",
        "truth":{
            "full_then_delta":true,
            "resource_subscriptions":true,
            "stable_object_identity":true
        },
        "execution_owner":"agent_client",
        "attached_browser":browser_wake::attached_browser(runtime_dir),
        "reference_actuator_available":true,
        "browser_support":["chrome","edge"],
        "extension_connected":false,
        "extension_candidate":Value::Null,
        "expected_extension_candidate":expected_extension_candidate,
        "first_slice":["button","text_field","checkbox","select"],
        "profile":{
            "name":&startup_profile.name,
            "behavior":&startup_profile.behavior
        }
    }))
}

fn call_tool(
    host: &HostClient,
    agent_views: &mut AgentViewState,
    name: &str,
    mut arguments: Value,
    mode: McpMode,
    diagnostics: bool,
    startup_profile: &Profile,
) -> Result<Value> {
    let public_method = name
        .strip_prefix("saccade.")
        .context("tool is outside the Saccade namespace")?;
    let method = host_method(public_method, mode).context("tool is not registered")?;
    require_tool_enabled(method, diagnostics)?;
    validate_arguments(method, &arguments, diagnostics)?;
    if matches!(method, "web.observe" | "web.act_object" | "tabs.close") {
        let tab_id = string(&arguments, "tab_id")?;
        agent_views.require_session_tab(tab_id)?;
    }
    if method == "web.act_object" {
        let tab_id = string(&arguments, "tab_id")?.to_string();
        let document_id = string(&arguments, "document_id")?.to_string();
        if agent_views.action_context_missing(&tab_id, &document_id)? {
            // The Host owns the canonical current snapshot. A rare local MCP
            // cursor loss must not force the model to re-read the page: restore
            // only this session tab and only when the exact document still
            // matches. Alias/object validation below continues to reject every
            // replacement rather than rebinding it silently.
            let observation: ObservationSnapshot = serde_json::from_value(host.call(
                NEXT_ID.fetch_add(1, Ordering::Relaxed),
                "web.observe",
                json!({"tab_id":tab_id}),
                Duration::from_secs(10),
            )?)?;
            observation.validate()?;
            agent_views.restore_action_context(observation, &document_id)?;
        }
        agent_views.infer_public_act_operations(&mut arguments)?;
    }
    if method == "web.observe" && arguments.get("object_ids").is_some() {
        let tab_id = string(&arguments, "tab_id")?;
        let document_id = string(&arguments, "document_id")?;
        let basis_revision = arguments
            .get("basis_revision")
            .and_then(Value::as_u64)
            .context("basis_revision must be an integer")?;
        let object_ids = arguments
            .get("object_ids")
            .and_then(Value::as_array)
            .context("object_ids must be an array")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .context("object_ids must contain strings")
            })
            .collect::<Result<Vec<_>>>()?;
        return agent_views.detail_view(tab_id, document_id, basis_revision, &object_ids);
    }
    let truth_query_timeout = (method == "web.observe")
        .then(|| arguments.get("timeout_ms").and_then(Value::as_u64))
        .flatten()
        .map(Duration::from_millis);
    let truth_query = if method == "web.observe" {
        arguments
            .as_object_mut()
            .expect("validated observe arguments must be an object")
            .remove("query")
    } else {
        None
    };
    let requested_resync =
        method == "web.observe" && arguments.get("resync").and_then(Value::as_bool) == Some(true);
    if method == "web.observe" {
        arguments
            .as_object_mut()
            .expect("validated observe arguments must be an object")
            .remove("resync");
        if requested_resync {
            let tab_id = arguments
                .get("tab_id")
                .and_then(Value::as_str)
                .expect("validated observe tab_id must be a string");
            agent_views.reset_cursor(tab_id);
        }
    }
    let delivery_mode = if method == "web.observe" {
        TruthDeliveryMode::from_arguments(&mut arguments)?
    } else {
        TruthDeliveryMode::Live
    };
    let requested_after_revision = arguments.get("after_revision").and_then(Value::as_u64);
    let observation_tab_id = arguments
        .get("tab_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if method == "web.observe" && !requested_resync {
        if let (Some(query), Some(tab_id), Some(after_revision)) = (
            truth_query.as_ref(),
            observation_tab_id.as_deref(),
            requested_after_revision,
        ) {
            // An action receipt may already have advanced this Agent cursor to
            // the requested basis. Project the dynamic working set directly
            // from that canonical snapshot instead of demanding an unrelated
            // future page revision or forcing the Agent to retry unbounded.
            if let Some(observation) = agent_views.observation_at_or_after(tab_id, after_revision) {
                let mut view = agent_views.project_query(observation, query)?;
                view["delivery_mode"] = Value::String(delivery_mode.name().into());
                return Ok(view);
            }
        }
    }
    if method == "web.observe" && !requested_resync {
        if let Some(tab_id) = observation_tab_id.as_deref() {
            if agent_views.has_pending_view(tab_id) {
                if requested_after_revision.is_some() {
                    bail!("finish the pending full Truth pages before waiting for a delta");
                }
                let mut page = agent_views
                    .continue_view(tab_id)?
                    .context("pending Truth page disappeared")?;
                page["delivery_mode"] = Value::String(delivery_mode.name().into());
                return Ok(page);
            }
            if truth_query.is_none() {
                if let Some(mut ambient) =
                    agent_views.take_ambient(tab_id, requested_after_revision)
                {
                    ambient["delivery_mode"] = Value::String(delivery_mode.name().into());
                    return Ok(ambient);
                }
            }
        }
    }
    if method == "web.observe"
        && delivery_mode == TruthDeliveryMode::Economy
        && requested_after_revision.is_none()
        && observation_tab_id
            .as_deref()
            .and_then(|tab_id| agent_views.revision_for_tab(tab_id))
            .is_some()
    {
        thread::sleep(ECONOMY_COALESCE_WINDOW);
    }
    agent_views.expand_object_aliases(method, &mut arguments)?;
    arguments = agent_views.hydrate_action_arguments(method, arguments)?;
    if method == "web.act_object" && benchmark_fresh_input_policy() {
        arguments["ignore_learned_policy"] = Value::Bool(true);
    }
    if method == "web.observe" && arguments.get("after_revision").is_none() {
        arguments
            .as_object_mut()
            .expect("validated observe arguments must be an object")
            .remove("timeout_ms");
        if let Some(tab_id) = arguments.get("tab_id").and_then(Value::as_str) {
            if let Some(revision) = agent_views.revision_for_tab(tab_id) {
                arguments
                    .as_object_mut()
                    .expect("validated observe arguments must be an object")
                    .insert("since_revision".into(), Value::from(revision));
            }
        }
    }
    if method == "web.observe" && arguments.get("after_revision").is_some() {
        if let Some(tab_id) = arguments.get("tab_id").and_then(Value::as_str) {
            if let Some(document_id) = agent_views.document_for_tab(tab_id).map(str::to_owned) {
                arguments
                    .as_object_mut()
                    .expect("validated observe arguments must be an object")
                    .insert("after_document_id".into(), Value::from(document_id));
            }
        }
    }
    let timeout = match method {
        "web.act_object" => Duration::from_secs(
            arguments
                .get("actions")
                .and_then(Value::as_array)
                .map(|actions| 10 + actions.len() as u64 * 6)
                .unwrap_or(45),
        ),
        "web.act" | "web.act_native" | "web.act_soft" => Duration::from_secs(30),
        "web.form.fill" => Duration::from_secs(
            arguments
                .get("actions")
                .and_then(Value::as_array)
                .map(|actions| 10 + actions.len() as u64 * 4)
                .unwrap_or(30),
        ),
        "web.reflex.run" => Duration::from_millis(
            arguments
                .get("timeout_ms")
                .and_then(Value::as_u64)
                .unwrap_or(30_000)
                + 10_000,
        ),
        "tabs.open" => Duration::from_secs(25),
        "web.observe" if arguments.get("after_revision").is_some() => Duration::from_millis(
            arguments
                .get("timeout_ms")
                .and_then(Value::as_u64)
                .unwrap_or(10_000)
                + 2_000,
        ),
        "system.capabilities" => Duration::from_millis(250),
        _ => Duration::from_secs(10),
    };
    // A claim never creates a tab, so it must never wake a browser either: the
    // Agent client already owns a live browser when it arms or confirms one.
    if method == "tabs.open" && arguments.get("claim").is_none() {
        let disconnected = match host.call(
            NEXT_ID.fetch_add(1, Ordering::Relaxed),
            "system.capabilities",
            json!({}),
            Duration::from_millis(250),
        ) {
            Ok(capabilities) => !capabilities
                .get("extension_connected")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            Err(ProtocolError::TransportUnavailable(_) | ProtocolError::Timeout) => true,
            Err(error) => return Err(error.into()),
        };
        if disconnected {
            let runtime_dir = host
                .grant_path()
                .parent()
                .context("Saccade grant path has no runtime directory")?;
            browser_wake::wake(runtime_dir).context(
                "Saccade Extension route is unavailable; run `npx -y @nanlogic/saccade doctor` for the exact store link and browser steps",
            )?;
        }
    }
    let mut result = match host.call(
        NEXT_ID.fetch_add(1, Ordering::Relaxed),
        method,
        arguments,
        timeout,
    ) {
        Ok(result) => result,
        Err(ProtocolError::TransportUnavailable(_) | ProtocolError::Timeout)
            if method == "system.capabilities" =>
        {
            disconnected_capabilities(host, startup_profile)?
        }
        Err(error) => return Err(error.into()),
    };
    if method == "web.observe" && delivery_mode == TruthDeliveryMode::Economy {
        if let (Some(tab_id), Some(since_revision)) =
            (observation_tab_id.as_deref(), requested_after_revision)
        {
            thread::sleep(ECONOMY_COALESCE_WINDOW);
            result = host.call(
                NEXT_ID.fetch_add(1, Ordering::Relaxed),
                method,
                json!({"tab_id":tab_id,"since_revision":since_revision}),
                Duration::from_secs(10),
            )?;
        }
    }
    match method {
        // The Agent addressed the object by alias, so every identity it gets
        // back must be that same alias, never the internal identity.
        "web.act_object" => {
            let transition = result
                .as_object_mut()
                .and_then(|object| object.remove("post_action_observation"));
            agent_views.collapse_object_aliases(&mut result);
            if let Some(transition) = transition {
                let observation: ObservationSnapshot = serde_json::from_value(transition)?;
                observation.validate()?;
                attach_act_transition(agent_views, &mut result, observation)?;
            }
            return Ok(result);
        }
        "system.capabilities" => {
            result["reference_actuator_active"] = Value::Bool(mode == McpMode::Reference);
            result["truth"]["delivery_modes"] = json!({
                "default":"live",
                "available":["live","economy"],
                "economy_coalesce_ms":ECONOMY_COALESCE_WINDOW.as_millis()
            });
            result["execution_contract"] = json!({
                "preferred_route":"saccade.act",
                "address_by":"object_id",
                "operation_inference":"current_truth_affordance_or_action_payload",
                "explicit_operation_required_only_when_ambiguous":true,
                "coordinates_accepted":false,
                "screenshot_required":false,
                "geometry_unit":"css_px",
                "geometry_coordinate_space":"content_viewport",
                "device_pixel_ratio_is_descriptive_only":true,
                "verification":"revision_bounded_semantic_state",
                "revision_growth_is_not_success":true,
                "external_execution_when":"saccade.act returns external_execution_required with retry_safe true",
                "external_execution_verification":"perform the Agent-client action in the same tab, then verify with Saccade Truth",
                "substituting_client_page_reads_for_truth":"unsupported"
            });
            if benchmark_fresh_input_policy() {
                result["benchmark_overrides"] = json!({
                    "fresh_input_policy":true,
                    "reason":"fair comparison excludes user-local learned execution history"
                });
            }
            result["truth"]["delivery_contract"] = json!({
                "strategy":"semantic_working_set_or_automatic_full_catalog_then_delta",
                "manual_view_selection":false,
                "semantic_query":true,
                "semantic_query_max_objects":MAX_QUERY_OBJECTS,
                "semantic_query_keeps_canonical_truth_local":true,
                "frame_scopes":["root","all"],
                "first_read":"query_working_set_or_full_or_automatic_stable_id_catalog",
                "oversized_full_page_max_bytes":MAX_FULL_PAGE_BYTES,
                "catalog_page_max_bytes":MAX_CATALOG_PAGE_BYTES,
                "catalog_detail_read":true,
                "catalog_detail_max_objects":MAX_DETAIL_OBJECTS,
                "updated_object_representation":"recursive_json_merge_patch",
                "continuation":"repeat truth.read with the same tab_id only when an exceptional catalog or delta page has page.complete false",
                "cursor_advances_after":"complete full or catalog sequence",
                "later_reads":"delta_since_this_agent_cursor",
                "action_transition":"verified_receipt_plus_same_frame_revision_bounded_changes",
                "ambient_action_churn":"other_frame_changes_are_queued_on_the_ordinary_truth_cursor",
                "full_reset_when":["document_changed","stream_gap","agent_requested_tab_resync"],
                "tab_scoped_resync":true
            });
            result["truth"]["mcp_session_tab_scope"] = Value::Bool(true);
            let profile_digest = project_profile_capabilities(&mut result);
            result["runtime_version"] = Value::String(env!("CARGO_PKG_VERSION").into());
            result["mcp_contract_hash"] = Value::String(mcp_contract_hash(mode));
            result["profile_digest"] = Value::String(profile_digest);
            return Ok(result);
        }
        "tabs.list" => {
            agent_views.project_session_tabs(&mut result)?;
            return Ok(result);
        }
        "tabs.open" => {
            agent_views.record_opened_tab(&result);
            return Ok(result);
        }
        "tabs.close" => {
            if result.get("closed").and_then(Value::as_bool) == Some(true) {
                if let Some(tab_id) = observation_tab_id.as_deref() {
                    agent_views.forget_session_tab(tab_id);
                }
            }
            return Ok(result);
        }
        "web.observe" => {
            let mut observation = serde_json::from_value::<ObservationSnapshot>(result)?;
            observation.validate()?;
            let mut view = if let Some(query) = truth_query.as_ref() {
                let started = std::time::Instant::now();
                let settle_limit = truth_query_timeout.unwrap_or(QUERY_MAX_SETTLE);
                let mut view = agent_views.project_query(observation.clone(), query)?;
                loop {
                    let elapsed = started.elapsed();
                    if elapsed >= settle_limit {
                        break;
                    }
                    let min_objects = query
                        .get("min_objects")
                        .and_then(Value::as_u64)
                        .unwrap_or(1);
                    let enough = view
                        .get("match_count")
                        .and_then(Value::as_u64)
                        .is_some_and(|count| count >= min_objects);
                    // `min_objects` is the caller's explicit hydration
                    // boundary. Once it is met, unrelated animation, ad, or
                    // iframe churn must not hold a ready working set until the
                    // overall settle timeout.
                    if enough {
                        break;
                    }
                    let wait = QUERY_QUIET_WINDOW.min(settle_limit - elapsed);
                    let next = host.call(
                        NEXT_ID.fetch_add(1, Ordering::Relaxed),
                        "web.observe",
                        json!({
                            "tab_id":observation.tab_id,
                            "after_revision":observation.revision,
                            "after_document_id":observation.document_id,
                            "timeout_ms":wait.as_millis() as u64
                        }),
                        wait + Duration::from_secs(1),
                    );
                    match next {
                        Ok(value) => {
                            observation = serde_json::from_value(value)?;
                            observation.validate()?;
                            view = agent_views.project_query(observation.clone(), query)?;
                        }
                        Err(ProtocolError::InvalidMessage(message))
                            if message.contains("no observation after revision")
                                && !enough
                                && started.elapsed() < settle_limit =>
                        {
                            continue;
                        }
                        Err(ProtocolError::Timeout)
                            if !enough && started.elapsed() < settle_limit =>
                        {
                            continue;
                        }
                        Err(ProtocolError::Timeout) => break,
                        Err(ProtocolError::InvalidMessage(message))
                            if message.contains("no observation after revision") =>
                        {
                            break;
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                view
            } else {
                agent_views.project(observation)?
            };
            view["delivery_mode"] = Value::String(delivery_mode.name().into());
            return Ok(view);
        }
        "web.act" | "web.act_native" | "web.act_soft" => {
            let receipt: ActionReceipt = serde_json::from_value(result)?;
            receipt.post_action_observation.validate()?;
            let view = agent_views.project(receipt.post_action_observation.clone())?;
            let retry = if receipt.dispatch_status
                == saccade_protocol::DispatchStatus::AcceptedBySoftware
                && matches!(
                    receipt.postcondition,
                    saccade_protocol::PostconditionStatus::VisibleStateUnchanged
                        | saccade_protocol::PostconditionStatus::Unverified
                ) {
                Some(json!({
                    "requires_fresh_authority":true,
                    "next_backend":"native",
                    "reason":"software input was accepted but not semantically verified; the local policy already learned native"
                }))
            } else {
                None
            };
            let mut agent_receipt = json!({
                "schema":"saccade.agent-receipt/1",
                "provenance":"reference_actuator",
                "browser_instance_id":receipt.browser_instance_id,
                "tab_id":receipt.tab_id,
                "document_id":receipt.document_id,
                "basis_revision":receipt.basis_revision,
                "prepared_revision":receipt.prepared_revision,
                "post_revision":receipt.post_revision,
                "operation":receipt.operation,
                "dispatch_status":receipt.dispatch_status,
                "local_wait_ms":receipt.local_wait_ms,
                "postcondition":receipt.postcondition,
                "settled":receipt.settled,
                "view":view
            });
            if let Some(retry) = retry {
                agent_receipt["retry"] = retry;
            }
            return Ok(agent_receipt);
        }
        "web.form.fill" => {
            let mut form = result;
            let observation_value = form
                .as_object_mut()
                .and_then(|object| object.remove("post_action_observation"))
                .context("form result omitted its final observation")?;
            let observation: ObservationSnapshot = serde_json::from_value(observation_value)?;
            observation.validate()?;
            let view = agent_views.project(observation)?;
            form["view"] = view;
            form["provenance"] = Value::String("reference_actuator".into());
            return Ok(form);
        }
        "web.reflex.run" => {
            result["provenance"] = Value::String("reference_actuator".into());
            return Ok(result);
        }
        _ => {}
    }
    Ok(result)
}

/// Advance the MCP-local Truth cache with the action's exact observation and
/// return only semantic changes the compact verification receipt did not
/// already prove. Canonical full Truth remains unchanged and available.
fn attach_act_transition(
    agent_views: &mut AgentViewState,
    result: &mut Value,
    observation: ObservationSnapshot,
) -> Result<()> {
    let tab_id = observation.tab_id.clone();
    let action_verified = result.get("verified").and_then(Value::as_bool) == Some(true)
        || result.get("all_verified").and_then(Value::as_bool) == Some(true);
    let mut target_objects = BTreeSet::new();
    if let Some(object_id) = result.get("object_id").and_then(Value::as_str) {
        target_objects.insert(object_id.to_string());
    }
    if let Some(object_id) = result
        .get("verification")
        .and_then(|verification| verification.get("object_id"))
        .and_then(Value::as_str)
    {
        target_objects.insert(object_id.to_string());
    }
    if let Some(steps) = result.get("steps").and_then(Value::as_array) {
        for object_id in steps.iter().filter_map(|step| {
            step.get("verification")
                .and_then(|verification| verification.get("object_id"))
                .and_then(Value::as_str)
        }) {
            target_objects.insert(object_id.to_string());
        }
    }
    let alias_frames = agent_views
        .tabs
        .get(&tab_id)
        .and_then(|current| {
            let aliases = agent_views.aliases.get(&tab_id)?;
            Some(
                current
                    .objects
                    .iter()
                    .filter_map(|object| {
                        aliases
                            .by_internal
                            .get(&object.object_id)
                            .map(|alias| (alias.clone(), object.frame_id.clone()))
                    })
                    .collect::<BTreeMap<_, _>>(),
            )
        })
        .unwrap_or_default();
    let target_frames = target_objects
        .iter()
        .filter_map(|object_id| alias_frames.get(object_id).cloned())
        .collect::<BTreeSet<_>>();

    let mut transition = agent_views.project(observation)?;
    while agent_views.has_pending_view(&tab_id) {
        let page = agent_views
            .continue_view(&tab_id)?
            .context("action transition continuation disappeared")?;
        if let (Some(target), Some(source)) = (
            transition.get_mut("changes").and_then(Value::as_array_mut),
            page.get("changes").and_then(Value::as_array),
        ) {
            target.extend(source.iter().cloned());
        }
    }
    if transition.get("mode").and_then(Value::as_str) == Some("delta") {
        let default_frame = transition
            .get("object_defaults")
            .and_then(|defaults| defaults.get("frame_id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let mut causal = Vec::new();
        let mut ambient = Vec::new();
        for change in transition
            .get_mut("changes")
            .and_then(Value::as_array_mut)
            .map(std::mem::take)
            .unwrap_or_default()
        {
            let object_id = change
                .get("object")
                .and_then(|object| object.get("object_id"))
                .or_else(|| change.get("object_id"))
                .and_then(Value::as_str);
            if object_id.is_some_and(|object_id| target_objects.contains(object_id)) {
                continue;
            }
            let frame_id = change
                .get("object")
                .and_then(|object| object.get("frame_id"))
                .and_then(Value::as_str)
                .or_else(|| {
                    object_id.and_then(|object_id| alias_frames.get(object_id).map(String::as_str))
                })
                .or(default_frame.as_deref());
            // The post-action observation is already the Runtime's bounded,
            // revision-verified transition. Return changes in the acted
            // object's frame so an iterative workflow can continue from the
            // receipt without asking for the same revision again. Changes in
            // other frames remain ambient and are delivered on the ordinary
            // Truth cursor. The verified target itself is omitted because its
            // compact before/after evidence is already in the receipt.
            let change_in_target_frame =
                frame_id.is_some_and(|frame_id| target_frames.contains(frame_id));
            if change_in_target_frame {
                causal.push(change);
            } else {
                ambient.push(change);
            }
        }
        transition["changes"] = Value::Array(causal);
        let changed_frames = transition
            .as_object_mut()
            .and_then(|object| object.remove("frames"))
            .filter(|frames| !frames.is_null());
        if !ambient.is_empty() || changed_frames.is_some() {
            let mut ambient_view = transition.clone();
            ambient_view["changes"] = Value::Array(ambient);
            if let Some(frames) = changed_frames {
                ambient_view["frames"] = frames;
            }
            let pending = ambient_view
                .get("changes")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            agent_views.queue_ambient(&tab_id, ambient_view)?;
            if !action_verified {
                result["ambient_changes_pending"] = Value::from(pending);
            }
        }
    }
    let empty_delta = transition.get("mode").and_then(Value::as_str) == Some("delta")
        && transition
            .get("changes")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty);
    if !empty_delta {
        result["transition"] = transition;
    }
    Ok(())
}

fn host_method(public_method: &str, mode: McpMode) -> Option<&'static str> {
    match public_method {
        "system.capabilities" => Some("system.capabilities"),
        "tabs.list" => Some("tabs.list"),
        "tabs.open" => Some("tabs.open"),
        "tabs.close" => Some("tabs.close"),
        "truth.read" => Some("web.observe"),
        "act" => Some("web.act_object"),
        "reference.act" if mode == McpMode::Reference => Some("web.act"),
        "reference.act_native" if mode == McpMode::Reference => Some("web.act_native"),
        "reference.act_soft" if mode == McpMode::Reference => Some("web.act_soft"),
        "reference.input_policy.list" if mode == McpMode::Reference => Some("input_policy.list"),
        "reference.input_policy.remember_native" if mode == McpMode::Reference => {
            Some("input_policy.remember_native")
        }
        "reference.form.fill" if mode == McpMode::Reference => Some("web.form.fill"),
        "reference.reflex.run" if mode == McpMode::Reference => Some("web.reflex.run"),
        _ => None,
    }
}

fn validate_arguments(method: &str, value: &Value, diagnostics: bool) -> Result<()> {
    if method == "web.act_object" {
        return validate_public_act(value);
    }
    if matches!(method, "web.act" | "web.act_native" | "web.act_soft") {
        validate_compact_action(value, &["click", "type", "select", "upload"])?;
        return Ok(());
    }
    let object = value
        .as_object()
        .context("tool arguments must be an object")?;
    let (allowed, required): (&[&str], &[&str]) = match method {
        "system.capabilities" | "tabs.list" | "input_policy.list" => (&[], &[]),
        "tabs.open" => (&["url", "active", "claim", "claim_id", "tab_id"], &["url"]),
        "tabs.close" => (&["tab_id"], &["tab_id"]),
        "web.observe" => (
            &[
                "tab_id",
                "after_revision",
                "timeout_ms",
                "delivery_mode",
                "resync",
                "object_ids",
                "document_id",
                "basis_revision",
                "query",
            ],
            &["tab_id"],
        ),
        "input_policy.remember_native" => {
            (&["tab_id", "action_token"], &["tab_id", "action_token"])
        }
        "web.form.fill" => (&["actions"], &["actions"]),
        "web.reflex.run" => (
            if diagnostics {
                &["tab_id", "input_backend", "max_actions", "timeout_ms"]
            } else {
                &["tab_id", "max_actions", "timeout_ms"]
            },
            &["tab_id"],
        ),
        _ => bail!("tool has no parameter contract"),
    };
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            bail!("unexpected argument: {key}");
        }
    }
    for key in required {
        if !object.contains_key(*key) {
            bail!("missing required argument: {key}");
        }
    }
    match method {
        "tabs.open" => {
            let url = string(value, "url")?;
            if url.len() > 8192 || !(url.starts_with("http://") || url.starts_with("https://")) {
                bail!("url must use HTTP or HTTPS and stay within 8192 bytes");
            }
            if value
                .get("active")
                .is_some_and(|active| !active.is_boolean())
            {
                bail!("active must be a boolean");
            }
            let claim = match value.get("claim") {
                None => None,
                Some(claim) => match claim.as_str() {
                    Some(mode @ ("arm" | "confirm")) => Some(mode),
                    _ => bail!("claim must be arm or confirm"),
                },
            };
            if claim.is_some() && value.get("active").is_some() {
                bail!("active does not apply to a claim; the Agent client creates the tab itself");
            }
            if claim == Some("confirm") {
                if string(value, "claim_id")?.is_empty() {
                    bail!("claim_id must name the armed claim");
                }
                if string(value, "tab_id")?.is_empty() {
                    bail!("tab_id must identify the tab the Agent client created");
                }
            } else {
                for key in ["claim_id", "tab_id"] {
                    if value.get(key).is_some() {
                        bail!("{key} applies only to claim confirm");
                    }
                }
            }
        }
        "tabs.close" => {
            string(value, "tab_id")?;
        }
        "web.observe" => {
            string(value, "tab_id")?;
            let detail_ids = value.get("object_ids");
            let query = value.get("query");
            if detail_ids.is_some() && query.is_some() {
                bail!("query cannot be combined with object_ids");
            }
            if let Some(ids) = detail_ids {
                let ids = ids.as_array().context("object_ids must be an array")?;
                if ids.is_empty() || ids.len() > MAX_DETAIL_OBJECTS {
                    bail!("object_ids must contain between 1 and {MAX_DETAIL_OBJECTS} identities");
                }
                if ids
                    .iter()
                    .any(|id| !matches!(id.as_str(), Some(value) if !value.is_empty()))
                {
                    bail!("object_ids must contain non-empty strings");
                }
                string(value, "document_id")?;
                value
                    .get("basis_revision")
                    .and_then(Value::as_u64)
                    .context("basis_revision must be an integer")?;
                for incompatible in ["after_revision", "timeout_ms", "delivery_mode", "resync"] {
                    if value.get(incompatible).is_some() {
                        bail!("{incompatible} cannot be combined with object_ids");
                    }
                }
            } else if value.get("document_id").is_some() || value.get("basis_revision").is_some() {
                bail!("document_id and basis_revision require object_ids");
            }
            if let Some(query) = query {
                let query = query.as_object().context("query must be an object")?;
                for key in query.keys() {
                    if ![
                        "text",
                        "text_any",
                        "roles",
                        "affordances",
                        "visible_only",
                        "frame_scope",
                        "min_objects",
                        "max_objects",
                    ]
                    .contains(&key.as_str())
                    {
                        bail!("unexpected query field {key}");
                    }
                }
                if !query.contains_key("text")
                    && !query.contains_key("text_any")
                    && !query.contains_key("roles")
                    && !query.contains_key("affordances")
                {
                    bail!("query requires text, roles, or affordances");
                }
                if let Some(text) = query.get("text") {
                    let text = text.as_str().context("query text must be a string")?;
                    if text.is_empty() || text.len() > 256 {
                        bail!("query text must contain between 1 and 256 bytes");
                    }
                }
                if let Some(values) = query.get("text_any") {
                    let values = values
                        .as_array()
                        .context("query text_any must be an array")?;
                    if values.is_empty()
                        || values.len() > 32
                        || values.iter().any(|value| {
                            !matches!(value.as_str(), Some(value) if !value.is_empty() && value.len() <= 256)
                        })
                    {
                        bail!("query text_any must contain 1 to 32 strings of 1 to 256 bytes");
                    }
                }
                for field in ["roles", "affordances"] {
                    if let Some(values) = query.get(field) {
                        let values = values.as_array().context("query filters must be arrays")?;
                        if values.is_empty()
                            || values.len() > 32
                            || values.iter().any(
                                |value| !matches!(value.as_str(), Some(value) if !value.is_empty()),
                            )
                        {
                            bail!("query {field} must contain 1 to 32 non-empty strings");
                        }
                    }
                }
                if query
                    .get("visible_only")
                    .is_some_and(|value| !value.is_boolean())
                {
                    bail!("query visible_only must be a boolean");
                }
                if query
                    .get("frame_scope")
                    .is_some_and(|value| !matches!(value.as_str(), Some("root" | "all")))
                {
                    bail!("query frame_scope must be root or all");
                }
                let max_objects = query
                    .get("max_objects")
                    .map(|value| {
                        value
                            .as_u64()
                            .context("query max_objects must be an integer")
                    })
                    .transpose()?
                    .unwrap_or(20);
                if !(1..=MAX_QUERY_OBJECTS as u64).contains(&max_objects) {
                    bail!("query max_objects must be between 1 and {MAX_QUERY_OBJECTS}");
                }
                let min_objects = query
                    .get("min_objects")
                    .map(|value| {
                        value
                            .as_u64()
                            .context("query min_objects must be an integer")
                    })
                    .transpose()?
                    .unwrap_or(1);
                if min_objects == 0 || min_objects > max_objects {
                    bail!("query min_objects must be between 1 and max_objects");
                }
                for incompatible in ["delivery_mode", "resync"] {
                    if value.get(incompatible).is_some() {
                        bail!("{incompatible} cannot be combined with query");
                    }
                }
            }
            if value
                .get("resync")
                .is_some_and(|resync| !resync.is_boolean())
            {
                bail!("resync must be a boolean");
            }
            if value
                .get("delivery_mode")
                .is_some_and(|delivery| !matches!(delivery.as_str(), Some("live" | "economy")))
            {
                bail!("delivery_mode must be live or economy");
            }
            if let Some(revision) = value.get("after_revision") {
                revision
                    .as_u64()
                    .context("after_revision must be an integer")?;
            }
            if let Some(timeout_ms) = value.get("timeout_ms") {
                let timeout_ms = timeout_ms
                    .as_u64()
                    .context("timeout_ms must be an integer")?;
                if !(1..=30_000).contains(&timeout_ms) {
                    bail!("timeout_ms must be between 1 and 30000");
                }
            }
        }
        "input_policy.remember_native" => {
            string(value, "tab_id")?;
            if string(value, "action_token")?.len() < 32 {
                bail!("action_token must be an opaque current token");
            }
        }
        "web.form.fill" => validate_form_fill(value)?,
        "web.reflex.run" => {
            string(value, "tab_id")?;
            if value
                .get("input_backend")
                .is_some_and(|backend| !matches!(backend.as_str(), Some("native" | "soft")))
            {
                bail!("input_backend must be native or soft");
            }
            let max_actions = value
                .get("max_actions")
                .map(|number| number.as_u64().context("max_actions must be an integer"))
                .transpose()?
                .unwrap_or(500);
            if !(1..=10_000).contains(&max_actions) {
                bail!("max_actions must be between 1 and 10000");
            }
            let timeout_ms = value
                .get("timeout_ms")
                .map(|number| number.as_u64().context("timeout_ms must be an integer"))
                .transpose()?
                .unwrap_or(30_000);
            if !(1..=60_000).contains(&timeout_ms) {
                bail!("timeout_ms must be between 1 and 60000");
            }
        }
        _ => {}
    }
    Ok(())
}

fn action_request_schema(operations: &[&str]) -> Value {
    let variants = operations
        .iter()
        .map(|operation| match *operation {
            "click" => json!({
                "properties":{"operation":{"const":"click"}}
            }),
            "type" => json!({
                "properties":{
                    "operation":{"const":"type"},
                    "text":{"type":"string"}
                },
                "required":["text"]
            }),
            "select" => json!({
                "properties":{
                    "operation":{"const":"select"},
                    "option_object_id":{"type":"string","minLength":1}
                },
                "required":["option_object_id"]
            }),
            "upload" => json!({
                "properties":{
                    "operation":{"const":"upload"},
                    "path":{"type":"string","minLength":1}
                },
                "required":["path"]
            }),
            _ => unreachable!("tool schema operation is allowlisted"),
        })
        .collect::<Vec<_>>();
    json!({
        "type":"object",
        "properties":{
            "action_token":{"type":"string","minLength":32},
            "operation":{"enum":operations},
            "text":{"type":"string"},
            "option_object_id":{"type":"string","minLength":1},
            "path":{"type":"string","minLength":1}
        },
        "required":["action_token","operation"],
        "oneOf":variants,
        "additionalProperties":false
    })
}

fn form_action_schema() -> Value {
    json!({
        "type":"object",
        "properties":{
            "action_token":{"type":"string","minLength":32},
            "operation":{"enum":["type","select","check"]},
            "text":{"type":"string"},
            "option_object_id":{"type":"string","minLength":1}
        },
        "required":["action_token","operation"],
        "oneOf":[
            {"properties":{"operation":{"const":"type"}},"required":["text"]},
            {"properties":{"operation":{"const":"select"}},"required":["option_object_id"]},
            {"properties":{"operation":{"const":"check"}}}
        ],
        "additionalProperties":false
    })
}

fn tools(mode: McpMode, diagnostics: bool) -> Vec<Value> {
    let public_act_schema = json!({
        "type":"object",
        "properties":{
            "tab_id":{"type":"string","minLength":1},
            "document_id":{"type":"string","minLength":1},
            "basis_revision":{"type":"integer","minimum":1},
            "object_id":{"type":"string","minLength":1},
            "operation":{"type":"string","enum":["click","select","type"]},
            "option_object_id":{"type":"string","minLength":1},
            "text":{"type":"string","maxLength":8192},
            "value":{"type":"string","maxLength":8192,"description":"Ordinary text to enter. Equivalent to text for a single action and consistent with batch actions."},
            "actions":{
                "type":"array","minItems":1,"maxItems":32,
                "items":{
                    "type":"object",
                    "properties":{
                        "object_id":{"type":"string","minLength":1},
                        "operation":{"type":"string","enum":["click","select","type"]},
                        "option_object_id":{"type":"string","minLength":1},
                        "value":{"type":"string","maxLength":8192,"description":"Ordinary text to enter when operation is type. Values for protected controls are rejected before dispatch."}
                    },
                    "required":["object_id"],
                    "additionalProperties":false
                }
            },
            "timeout_ms":{"type":"integer","minimum":1,"maximum":30000}
        },
        "required":["tab_id","document_id","basis_revision"],
        "additionalProperties":false
    });
    let mut tools = vec![
        json!({"name":"saccade.system.capabilities","description":"Call once before browser work. Returns live identity, the active Profile behavior, Truth delivery, and execution contract.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"saccade.tabs.list","description":"List this MCP session's Agent-owned tabs and current user-shared tabs.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"saccade.tabs.open","description":"Open and authorize an HTTP(S) tab. Omit claim for the normal route. claim arm/confirm is only for an Agent client that must create its own same-origin tab.","inputSchema":{"type":"object","properties":{"url":{"type":"string","minLength":1,"maxLength":8192},"active":{"type":"boolean"},"claim":{"type":"string","enum":["arm","confirm"]},"claim_id":{"type":"string","minLength":1},"tab_id":{"type":"string","minLength":1}},"required":["url"],"additionalProperties":false}}),
        json!({"name":"saccade.tabs.close","description":"Close a tab created by the Agent through Saccade. User-shared tabs cannot be closed by this tool.","inputSchema":{"type":"object","properties":{"tab_id":{"type":"string","minLength":1}},"required":["tab_id"],"additionalProperties":false}}),
        json!({"name":"saccade.truth.read","description":"Read canonical Truth for one tab. Query keys are text/text_any plus roles (plural array) or affordances. One query returns a bounded working set with nearby decision context; do not query adjacent labels separately. Otherwise the first view is full/catalog and later views are deltas. after_revision waits locally. resync resets only this tab.","inputSchema":{"type":"object","properties":{"tab_id":{"type":"string","minLength":1},"after_revision":{"type":"integer","minimum":0,"description":"Canonical lower bound; keep beside query."},"timeout_ms":{"type":"integer","minimum":1,"maximum":30000},"delivery_mode":{"type":"string","enum":["live","economy"],"default":"live"},"resync":{"type":"boolean","default":false},"object_ids":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string","minLength":1},"uniqueItems":true},"document_id":{"type":"string","minLength":1},"basis_revision":{"type":"integer","minimum":1},"query":{"type":"object","description":"Bounded semantic projection over current canonical Truth.","properties":{"text":{"type":"string","minLength":1,"maxLength":256},"text_any":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"string","minLength":1,"maxLength":256}},"roles":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"string","minLength":1},"uniqueItems":true},"affordances":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"string","minLength":1},"uniqueItems":true},"visible_only":{"type":"boolean","default":false},"frame_scope":{"type":"string","enum":["root","all"],"default":"all"},"min_objects":{"type":"integer","minimum":1,"maximum":32,"default":1},"max_objects":{"type":"integer","minimum":1,"maximum":32,"default":20}},"anyOf":[{"required":["text"]},{"required":["text_any"]},{"required":["roles"]},{"required":["affordances"]}],"additionalProperties":false}},"required":["tab_id"],"additionalProperties":false}}),
        json!({"name":"saccade.act","description":"Act on current Truth object IDs with registered software input. Omit operation when the current affordance or payload is unambiguous; pass it explicitly for a recognized control that may become enabled during the bounded local wait. Batch independent form edits. verified/all_verified is semantic proof; otherwise obey retry_safe and external_execution_required.","inputSchema":public_act_schema}),
    ];
    tools
        .iter_mut()
        .find(|tool| tool["name"] == "saccade.truth.read")
        .and_then(|tool| tool.pointer_mut("/inputSchema/properties/query/properties/roles"))
        .and_then(Value::as_object_mut)
        .expect("truth.read roles schema must be an object")
        .insert(
            "description".into(),
            json!("Canonical Truth roles. HTML/ARIA combobox and listbox controls project as select; both are accepted as input aliases."),
        );
    if mode == McpMode::Reference {
        tools.extend([
            json!({"name":"saccade.reference.act","description":"Reference-only closed-loop actuator using a current action token.","inputSchema":action_request_schema(&["click","type","select","upload"])}),
            json!({"name":"saccade.reference.input_policy.list","description":"List local reference-actuator input-backend records.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
            json!({"name":"saccade.reference.input_policy.remember_native","description":"Record that a page control requires the reference native backend.","inputSchema":{"type":"object","properties":{"tab_id":{"type":"string","minLength":1},"action_token":{"type":"string","minLength":32}},"required":["tab_id","action_token"],"additionalProperties":false}}),
            json!({"name":"saccade.reference.form.fill","description":"Reference-only bounded form actuator.","inputSchema":{"type":"object","properties":{"actions":{"type":"array","minItems":1,"maxItems":32,"items":form_action_schema()}},"required":["actions"],"additionalProperties":false}}),
            json!({"name":"saccade.reference.reflex.run","description":"Reference-only revision-bound reflex actuator.","inputSchema":{"type":"object","properties":{"tab_id":{"type":"string","minLength":1},"max_actions":{"type":"integer","minimum":1,"maximum":10000,"default":500},"timeout_ms":{"type":"integer","minimum":1,"maximum":60000,"default":30000}},"required":["tab_id"],"additionalProperties":false}}),
        ]);
    }
    if mode == McpMode::Reference && diagnostics {
        tools.push(json!({"name":"saccade.reference.act_native","description":"Reference diagnostic override using native OS input.","inputSchema":action_request_schema(&["click","type","select","upload"])}));
        tools.push(json!({"name":"saccade.reference.act_soft","description":"Reference diagnostic override using audited software input.","inputSchema":action_request_schema(&["click"])}));
        tools
            .iter_mut()
            .find(|tool| tool["name"] == "saccade.reference.reflex.run")
            .and_then(|tool| tool.pointer_mut("/inputSchema/properties"))
            .and_then(Value::as_object_mut)
            .expect("reflex tool properties must exist")
            .insert("input_backend".into(), json!({"enum":["native","soft"]}));
    }
    tools
}

fn diagnostic_input_overrides_enabled() -> bool {
    std::env::var("SACCADE_DIAGNOSTIC_INPUT_OVERRIDES").as_deref() == Ok("1")
}

fn benchmark_fresh_input_policy() -> bool {
    std::env::var("SACCADE_BENCHMARK_FRESH_INPUT_POLICY").as_deref() == Ok("1")
}

fn require_tool_enabled(method: &str, diagnostics: bool) -> Result<()> {
    if matches!(method, "web.act_native" | "web.act_soft") && !diagnostics {
        bail!("diagnostic input overrides are disabled");
    }
    Ok(())
}

struct PendingViewDelivery {
    observation: ObservationSnapshot,
    pages: Vec<Value>,
    next_page: usize,
}

#[derive(Default)]
struct AgentViewState {
    tabs: BTreeMap<String, ObservationSnapshot>,
    aliases: BTreeMap<String, AgentObjectAliases>,
    pending_view: BTreeMap<String, PendingViewDelivery>,
    pending_ambient: BTreeMap<String, VecDeque<Value>>,
    session_agent_tabs: BTreeSet<String>,
    session_shared_tabs: BTreeSet<String>,
    include_authority: bool,
}

#[derive(Default)]
struct AgentObjectAliases {
    document_id: String,
    by_internal: BTreeMap<String, String>,
    by_alias: BTreeMap<String, String>,
    next: u64,
}

impl AgentViewState {
    fn new(include_authority: bool) -> Self {
        Self {
            include_authority,
            ..Self::default()
        }
    }
    fn revision_for_tab(&self, tab_id: &str) -> Option<u64> {
        self.tabs
            .get(tab_id)
            .map(|observation| observation.revision)
    }

    fn observation_at_or_after(&self, tab_id: &str, revision: u64) -> Option<ObservationSnapshot> {
        self.tabs
            .get(tab_id)
            .filter(|observation| observation.revision >= revision)
            .cloned()
    }

    fn require_session_tab(&self, tab_id: &str) -> Result<()> {
        if self.session_agent_tabs.contains(tab_id) || self.session_shared_tabs.contains(tab_id) {
            return Ok(());
        }
        bail!(
            "tab {tab_id} is outside this MCP session; use tabs.open, or explicitly share it and call tabs.list"
        )
    }

    fn record_opened_tab(&mut self, result: &Value) {
        if let Some(tab_id) = result.get("tab_id").and_then(Value::as_str) {
            self.session_agent_tabs.insert(tab_id.to_string());
        }
    }

    fn project_session_tabs(&mut self, result: &mut Value) -> Result<()> {
        let tabs = result
            .get_mut("tabs")
            .and_then(Value::as_array_mut)
            .context("tabs.list result must contain tabs")?;
        let live_agent_tabs = tabs
            .iter()
            .filter(|tab| tab.get("ownership").and_then(Value::as_str) == Some("agent"))
            .filter_map(|tab| tab.get("tab_id").and_then(Value::as_str))
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        self.session_agent_tabs
            .retain(|tab_id| live_agent_tabs.contains(tab_id));
        self.session_shared_tabs.clear();
        tabs.retain(|tab| {
            let Some(tab_id) = tab.get("tab_id").and_then(Value::as_str) else {
                return false;
            };
            match tab.get("ownership").and_then(Value::as_str) {
                Some("user_shared") => {
                    self.session_shared_tabs.insert(tab_id.to_owned());
                    true
                }
                Some("agent") => self.session_agent_tabs.contains(tab_id),
                _ => false,
            }
        });
        result["session_scoped"] = Value::Bool(true);
        Ok(())
    }

    fn forget_session_tab(&mut self, tab_id: &str) {
        self.session_agent_tabs.remove(tab_id);
        self.session_shared_tabs.remove(tab_id);
        self.reset_cursor(tab_id);
    }

    fn reset_cursor(&mut self, tab_id: &str) {
        // Keep document-local aliases stable, but forget delivery state so the
        // next projection is one complete current view for this tab only.
        self.tabs.remove(tab_id);
        self.pending_view.remove(tab_id);
        self.pending_ambient.remove(tab_id);
    }

    fn action_context_missing(&self, tab_id: &str, document_id: &str) -> Result<bool> {
        match self.tabs.get(tab_id) {
            None => Ok(true),
            Some(observation) if observation.document_id == document_id => Ok(false),
            Some(_) => bail!(
                "document_id is stale for tab {tab_id}; read the current Truth for that exact tab"
            ),
        }
    }

    fn restore_action_context(
        &mut self,
        observation: ObservationSnapshot,
        expected_document_id: &str,
    ) -> Result<()> {
        if observation.document_id != expected_document_id {
            bail!(
                "document changed while restoring local action context; read the current Truth for tab {}",
                observation.tab_id
            );
        }
        let tab_id = observation.tab_id.clone();
        self.aliases_for(&observation);
        self.tabs.insert(tab_id, observation);
        Ok(())
    }

    fn has_pending_view(&self, tab_id: &str) -> bool {
        self.pending_view.contains_key(tab_id)
    }

    fn take_ambient(&mut self, tab_id: &str, after_revision: Option<u64>) -> Option<Value> {
        let pages = self.pending_ambient.get_mut(tab_id)?;
        while let Some(front) = pages.front() {
            let revision = front.get("revision").and_then(Value::as_u64).unwrap_or(0);
            if after_revision.is_some_and(|after| after >= revision) {
                pages.pop_front();
            } else {
                break;
            }
        }
        let page = pages.pop_front();
        if pages.is_empty() {
            self.pending_ambient.remove(tab_id);
        }
        page
    }

    fn queue_ambient(&mut self, tab_id: &str, view: Value) -> Result<()> {
        let pages = bounded_agent_pages(view)?;
        self.pending_ambient
            .entry(tab_id.to_string())
            .or_default()
            .extend(pages);
        Ok(())
    }

    fn continue_view(&mut self, tab_id: &str) -> Result<Option<Value>> {
        let Some(pending) = self.pending_view.get_mut(tab_id) else {
            return Ok(None);
        };
        let page = pending
            .pages
            .get(pending.next_page)
            .cloned()
            .context("pending full Truth page is out of bounds")?;
        pending.next_page += 1;
        if pending.next_page == pending.pages.len() {
            let completed = self
                .pending_view
                .remove(tab_id)
                .context("pending full Truth disappeared at completion")?;
            self.tabs.insert(tab_id.to_string(), completed.observation);
        }
        Ok(Some(page))
    }

    fn document_for_tab(&self, tab_id: &str) -> Option<&str> {
        self.tabs
            .get(tab_id)
            .map(|observation| observation.document_id.as_str())
    }

    fn detail_view(
        &self,
        tab_id: &str,
        document_id: &str,
        basis_revision: u64,
        object_ids: &[String],
    ) -> Result<Value> {
        if self.has_pending_view(tab_id) {
            bail!("finish the pending Truth catalog pages before reading object details");
        }
        let observation = self
            .tabs
            .get(tab_id)
            .context("no current Truth catalog exists for this tab")?;
        if observation.document_id != document_id || observation.revision != basis_revision {
            bail!("object detail basis is stale; read and fold the current Truth delta first");
        }
        let aliases = self
            .aliases
            .get(tab_id)
            .filter(|aliases| aliases.document_id == document_id)
            .context("Agent object aliases are stale or unavailable")?;
        let by_internal = observation
            .objects
            .iter()
            .map(|object| (object.object_id.as_str(), object))
            .collect::<BTreeMap<_, _>>();
        let default_frame = default_frame_id(observation);
        let mut objects = Vec::with_capacity(object_ids.len());
        for alias in object_ids {
            let internal = aliases
                .by_alias
                .get(alias)
                .with_context(|| format!("unknown object_id {alias} for this Truth catalog"))?;
            let object = by_internal
                .get(internal.as_str())
                .context("Truth catalog alias points to a missing object")?;
            objects.push(agent_object_value(
                object,
                default_frame.as_deref(),
                &aliases.by_internal,
                self.include_authority,
            )?);
        }
        Ok(json!({
            "schema":"saccade.agent-view/1",
            "mode":"details",
            "browser_instance_id":observation.browser_instance_id,
            "tab_id":observation.tab_id,
            "document_id":observation.document_id,
            "revision":observation.revision,
            "viewport_revision":observation.viewport_revision,
            "geometry":observation.geometry,
            "object_defaults":agent_object_defaults(default_frame.as_deref()),
            "objects":objects,
            "requested_object_ids":object_ids,
            "limitations":observation.limitations,
            "cursor_advanced":false
        }))
    }

    fn project_query(&mut self, observation: ObservationSnapshot, query: &Value) -> Result<Value> {
        let aliases = self.aliases_for(&observation);
        let tab_id = observation.tab_id.clone();
        let projected_delta = self
            .tabs
            .contains_key(&tab_id)
            .then(|| self.project(observation.clone()))
            .transpose()?;
        if self.has_pending_view(&tab_id) {
            // A query is one bounded working set, never a continuation protocol.
            while self.has_pending_view(&tab_id) {
                self.continue_view(&tab_id)?;
            }
        } else if projected_delta.is_none() {
            self.tabs.insert(tab_id.clone(), observation.clone());
        }
        // A working-set read is based on the latest complete canonical
        // observation, so every older queued ambient page is already folded.
        // Do not make the Agent drain stale geometry churn after a query.
        self.pending_ambient.remove(&tab_id);

        let query = query.as_object().context("query must be an object")?;
        let text_tokens = query.get("text").and_then(Value::as_str).map(|value| {
            value
                .split_whitespace()
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        });
        let text_any_tokens = query
            .get("text_any")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|value| {
                        value
                            .split_whitespace()
                            .map(str::to_lowercase)
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            });
        let roles = query.get("roles").and_then(Value::as_array).map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(canonical_query_role)
                .collect::<BTreeSet<_>>()
        });
        let affordances = query
            .get("affordances")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<BTreeSet<_>>()
            });
        let visible_only = query
            .get("visible_only")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let root_only = query.get("frame_scope").and_then(Value::as_str) == Some("root");
        let limit = query
            .get("max_objects")
            .and_then(Value::as_u64)
            .unwrap_or(20) as usize;
        let requested_min_objects = query
            .get("min_objects")
            .and_then(Value::as_u64)
            .unwrap_or(1) as usize;
        let default_frame = default_frame_id(&observation);
        let mut candidates = Vec::new();
        for object in &observation.objects {
            match object.visibility {
                saccade_protocol::Visibility::Hidden | saccade_protocol::Visibility::Unknown => {
                    continue;
                }
                saccade_protocol::Visibility::Visible if visible_only => {}
                saccade_protocol::Visibility::Visible
                | saccade_protocol::Visibility::Offscreen
                | saccade_protocol::Visibility::PartiallyOccluded
                    if !visible_only => {}
                _ => continue,
            }
            if root_only && Some(object.frame_id.as_str()) != default_frame.as_deref() {
                continue;
            }
            let role = serde_json::to_value(object.role)?;
            let role = role
                .as_str()
                .context("semantic role must serialize as a string")?;
            if roles
                .as_ref()
                .is_some_and(|allowed| !allowed.contains(role))
            {
                continue;
            }
            let missing_required_affordance = affordances.as_ref().is_some_and(|required| {
                let actual = object
                    .affordances
                    .iter()
                    .filter_map(|value| serde_json::to_value(value).ok())
                    .filter_map(|value| value.as_str().map(str::to_owned))
                    .collect::<BTreeSet<_>>();
                !required.iter().all(|value| actual.contains(*value))
            });
            if missing_required_affordance {
                continue;
            }
            if let Some(needles) = text_tokens.as_deref() {
                let haystack = query_search_text(&observation, object);
                if needles
                    .iter()
                    .any(|needle| !query_term_matches(&haystack, needle))
                {
                    continue;
                }
            }
            let matched_phrase_indexes = if let Some(phrases) = text_any_tokens.as_deref() {
                let haystack = query_search_text(&observation, object);
                let direct_haystack = query_object_text(object);
                let indexes = phrases
                    .iter()
                    .enumerate()
                    .filter_map(|(index, phrase)| {
                        phrase
                            .iter()
                            .all(|needle| query_term_matches(&haystack, needle))
                            .then_some(index)
                    })
                    .collect::<Vec<_>>();
                if indexes.is_empty() {
                    continue;
                }
                let direct_indexes = phrases
                    .iter()
                    .enumerate()
                    .filter_map(|(index, phrase)| {
                        phrase
                            .iter()
                            .all(|needle| query_term_matches(&direct_haystack, needle))
                            .then_some(index)
                    })
                    .collect::<Vec<_>>();
                (indexes, direct_indexes)
            } else {
                (Vec::new(), Vec::new())
            };
            candidates.push((object, matched_phrase_indexes));
        }
        let total_matches = candidates.len();
        let phrase_match_counts = text_any_tokens.as_deref().map(|phrases| {
            (0..phrases.len())
                .map(|phrase_index| {
                    candidates
                        .iter()
                        .filter(|(_, (indexes, _))| indexes.contains(&phrase_index))
                        .count()
                })
                .collect::<Vec<_>>()
        });
        let mut selected_indexes = Vec::with_capacity(limit.min(total_matches));
        if let Some(phrases) = text_any_tokens.as_deref() {
            for phrase_index in 0..phrases.len() {
                if selected_indexes.len() >= limit {
                    break;
                }
                let direct = candidates.iter().enumerate().find(
                    |(candidate_index, (_, (_, direct_indexes)))| {
                        !selected_indexes.contains(candidate_index)
                            && direct_indexes.contains(&phrase_index)
                    },
                );
                let contextual =
                    candidates
                        .iter()
                        .enumerate()
                        .find(|(candidate_index, (_, (indexes, _)))| {
                            !selected_indexes.contains(candidate_index)
                                && indexes.contains(&phrase_index)
                        });
                if let Some((candidate_index, _)) = direct.or(contextual) {
                    selected_indexes.push(candidate_index);
                }
            }
        }
        for candidate_index in 0..total_matches {
            if selected_indexes.len() >= limit {
                break;
            }
            if !selected_indexes.contains(&candidate_index) {
                selected_indexes.push(candidate_index);
            }
        }
        let selected_objects = selected_indexes
            .into_iter()
            .map(|candidate_index| candidates[candidate_index].0)
            .collect::<Vec<_>>();
        let selected_ids = selected_objects
            .iter()
            .map(|object| object.object_id.as_str())
            .collect::<BTreeSet<_>>();
        let mut working_set = selected_objects.clone();
        // A query for an actionable target should carry the bounded visible
        // context needed to decide that action. This is the semantic analogue
        // of a locator plus its local accessible snapshot: it avoids forcing
        // the Agent to issue separate reads for adjacent labels, record facts,
        // and sibling choices while keeping canonical Truth local.
        if working_set
            .iter()
            .any(|object| !object.affordances.is_empty())
            && working_set.len() < limit
        {
            let mut contextual = observation
                .objects
                .iter()
                .filter(|object| !selected_ids.contains(object.object_id.as_str()))
                .filter_map(|object| {
                    let distance = selected_objects
                        .iter()
                        .filter(|selected| selected.frame_id == object.frame_id)
                        .filter_map(|selected| nearby_context_distance(selected, object))
                        .min_by(|left, right| left.total_cmp(right))?;
                    Some((distance, object))
                })
                .collect::<Vec<_>>();
            contextual.sort_by(|(left_distance, left), (right_distance, right)| {
                left_distance
                    .total_cmp(right_distance)
                    .then_with(|| left.document_bounds.y.total_cmp(&right.document_bounds.y))
                    .then_with(|| left.document_bounds.x.total_cmp(&right.document_bounds.x))
            });
            working_set.extend(
                contextual
                    .into_iter()
                    .take(limit - working_set.len())
                    .map(|(_, object)| object),
            );
        }
        let context_count = working_set.len().saturating_sub(selected_objects.len());
        let matched = working_set
            .into_iter()
            .map(|object| {
                agent_object_value(
                    object,
                    default_frame.as_deref(),
                    &aliases,
                    self.include_authority,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let query_primary_object_ids = selected_objects
            .iter()
            .filter_map(|object| aliases.get(&object.object_id).cloned())
            .collect::<Vec<_>>();
        let frame_summaries = observation
            .frames
            .iter()
            .map(|frame| {
                let object_count = observation
                    .objects
                    .iter()
                    .filter(|object| object.frame_id == frame.frame_id)
                    .count();
                json!({
                    "frame_id":frame.frame_id,
                    "origin":frame.origin,
                    "status":frame.status,
                    "object_count":object_count,
                    "root":Some(frame.frame_id.as_str()) == default_frame.as_deref()
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "schema":"saccade.agent-view/1",
            "mode":"working_set",
            "browser_instance_id":observation.browser_instance_id,
            "tab_id":observation.tab_id,
            "document_id":observation.document_id,
            "revision":observation.revision,
            "viewport_revision":observation.viewport_revision,
            "geometry":observation.geometry,
            "object_defaults":agent_object_defaults(default_frame.as_deref()),
            "objects":matched,
            "match_count":total_matches,
            "context_count":context_count,
            "query_primary_object_ids":query_primary_object_ids,
            "text_any_match_counts":phrase_match_counts,
            "requested_min_objects":requested_min_objects,
            "settled":total_matches >= requested_min_objects,
            "truncated":total_matches > limit,
            "query":query,
            "frame_summaries":frame_summaries,
            "coverage":observation.coverage,
            "limitations":observation.limitations,
            "cursor_advanced":true,
            "gap":observation.gap
        }))
    }

    fn aliases_for(&mut self, observation: &ObservationSnapshot) -> BTreeMap<String, String> {
        let aliases = self.aliases.entry(observation.tab_id.clone()).or_default();
        if aliases.document_id != observation.document_id {
            *aliases = AgentObjectAliases {
                document_id: observation.document_id.clone(),
                next: 1,
                ..AgentObjectAliases::default()
            };
        }
        for object in &observation.objects {
            if aliases.by_internal.contains_key(&object.object_id) {
                continue;
            }
            let alias = format!("o{}", aliases.next);
            aliases.next += 1;
            aliases
                .by_internal
                .insert(object.object_id.clone(), alias.clone());
            aliases.by_alias.insert(alias, object.object_id.clone());
        }
        aliases.by_internal.clone()
    }

    fn expand_object_aliases(&self, method: &str, arguments: &mut Value) -> Result<()> {
        // saccade.act is addressed purely by the Agent-facing object alias, so
        // both the target and any chosen option must be resolved back to the
        // internal identity before the closed loop sees them.
        if method == "web.act_object" {
            let tab_id = string(arguments, "tab_id")?.to_string();
            let document_id = string(arguments, "document_id")?.to_string();
            let aliases = self
                .aliases
                .get(tab_id.as_str())
                .filter(|aliases| aliases.document_id == document_id)
                .context("Agent object aliases are stale or unavailable")?;
            let expand = |value: &mut Value| -> Result<()> {
                for field in ["object_id", "option_object_id"] {
                    let Some(alias) = value.get(field).and_then(Value::as_str) else {
                        continue;
                    };
                    let internal = aliases.by_alias.get(alias).with_context(|| {
                        format!(
                            "unknown {field} {alias}; call saccade.truth.read for tab {tab_id} first"
                        )
                    })?;
                    value[field] = Value::String(internal.clone());
                }
                Ok(())
            };
            if let Some(actions) = arguments.get_mut("actions").and_then(Value::as_array_mut) {
                for action in actions {
                    expand(action)?;
                }
            } else {
                expand(arguments)?;
            }
            return Ok(());
        }
        if !matches!(
            method,
            "web.act" | "web.act_native" | "web.act_soft" | "web.form.fill"
        ) {
            return Ok(());
        }
        let observation = self.action_context(method, arguments)?;
        let tab_id = observation.tab_id.as_str();
        let document_id = observation.document_id.as_str();
        let aliases = self
            .aliases
            .get(tab_id)
            .filter(|aliases| aliases.document_id == document_id)
            .context("Agent object aliases are stale or unavailable")?;
        if method == "web.form.fill" {
            if let Some(actions) = arguments.get_mut("actions").and_then(Value::as_array_mut) {
                for action in actions {
                    expand_compact_option_alias(action, aliases)?;
                }
            }
        } else {
            expand_compact_option_alias(arguments, aliases)?;
        }
        Ok(())
    }

    /// Compile the current Truth object's semantic affordance into the finite
    /// public action operation. The Agent chooses the target and supplies any
    /// payload; it does not have to restate that a one-action button clicks or
    /// that a field carrying text types. Explicit operations remain accepted
    /// for compatibility and genuinely multi-affordance objects.
    fn infer_public_act_operations(&self, arguments: &mut Value) -> Result<()> {
        let tab_id = string(arguments, "tab_id")?.to_string();
        let document_id = string(arguments, "document_id")?.to_string();
        let observation = self
            .tabs
            .get(&tab_id)
            .filter(|observation| observation.document_id == document_id)
            .context("current Truth is unavailable for operation inference")?;
        let aliases = self
            .aliases
            .get(&tab_id)
            .filter(|aliases| aliases.document_id == document_id)
            .context("Agent object aliases are stale or unavailable")?;

        let infer = |action: &mut Value, batch: bool| -> Result<()> {
            let alias = string(action, "object_id")?;
            let internal = aliases.by_alias.get(alias).with_context(|| {
                format!("unknown object_id {alias}; call saccade.truth.read for tab {tab_id} first")
            })?;
            let target = observation
                .objects
                .iter()
                .find(|object| &object.object_id == internal)
                .context("current Truth object disappeared before operation inference")?;

            let has_option = action.get("option_object_id").is_some();
            let has_text = action.get(if batch { "value" } else { "text" }).is_some()
                || (!batch && action.get("value").is_some());
            if has_option && has_text {
                bail!("an action cannot carry both text and option_object_id");
            }

            let explicit_operation = action.get("operation").and_then(Value::as_str);
            let operation = if let Some(explicit) = explicit_operation {
                explicit.to_string()
            } else if has_option {
                "select".to_string()
            } else if has_text {
                "type".to_string()
            } else {
                let operations = target
                    .affordances
                    .iter()
                    .filter_map(|affordance| match affordance {
                        Affordance::Click => Some("click"),
                        Affordance::Type => Some("type"),
                        Affordance::Select => Some("select"),
                        _ => None,
                    })
                    .collect::<BTreeSet<_>>();
                match operations.iter().copied().collect::<Vec<_>>().as_slice() {
                    [only] => (*only).to_string(),
                    [] if matches!(
                        target.role,
                        saccade_protocol::SemanticRole::Button
                            | saccade_protocol::SemanticRole::Link
                            | saccade_protocol::SemanticRole::Checkbox
                            | saccade_protocol::SemanticRole::Radio
                            | saccade_protocol::SemanticRole::Switch
                            | saccade_protocol::SemanticRole::Select
                            | saccade_protocol::SemanticRole::Option
                            | saccade_protocol::SemanticRole::Tab
                            | saccade_protocol::SemanticRole::MenuItem
                            | saccade_protocol::SemanticRole::ReflexTarget
                            | saccade_protocol::SemanticRole::TextField
                            | saccade_protocol::SemanticRole::SearchField
                            | saccade_protocol::SemanticRole::TextArea
                            | saccade_protocol::SemanticRole::ContentEditable
                            | saccade_protocol::SemanticRole::SpinButton
                    ) => bail!(
                        "object {alias} is a recognized action control but is not currently actionable; pass operation explicitly to use the bounded local actionability wait"
                    ),
                    [] => bail!(
                        "object {alias} has no saccade.act affordance; use the Agent client when Truth requires external execution"
                    ),
                    _ => bail!(
                        "object {alias} has multiple action affordances; pass operation explicitly"
                    ),
                }
            };

            let required_affordance = match operation.as_str() {
                "click" => Affordance::Click,
                "type" => Affordance::Type,
                "select" => Affordance::Select,
                _ => bail!("operation must be click, type, or select"),
            };
            let role_supports_operation = match operation.as_str() {
                "click" => matches!(
                    target.role,
                    saccade_protocol::SemanticRole::Button
                        | saccade_protocol::SemanticRole::Link
                        | saccade_protocol::SemanticRole::Checkbox
                        | saccade_protocol::SemanticRole::Radio
                        | saccade_protocol::SemanticRole::Switch
                        | saccade_protocol::SemanticRole::Select
                        | saccade_protocol::SemanticRole::Option
                        | saccade_protocol::SemanticRole::Tab
                        | saccade_protocol::SemanticRole::MenuItem
                        | saccade_protocol::SemanticRole::ReflexTarget
                ),
                "type" => matches!(
                    target.role,
                    saccade_protocol::SemanticRole::TextField
                        | saccade_protocol::SemanticRole::SearchField
                        | saccade_protocol::SemanticRole::TextArea
                        | saccade_protocol::SemanticRole::ContentEditable
                        | saccade_protocol::SemanticRole::SpinButton
                ),
                "select" => target.role == saccade_protocol::SemanticRole::Select,
                _ => false,
            };
            if !(target.affordances.contains(&required_affordance)
                || (explicit_operation.is_some() && role_supports_operation))
            {
                if explicit_operation.is_none() && role_supports_operation {
                    bail!(
                        "object {alias} is a recognized {operation} control but is not currently actionable; pass operation explicitly to use the bounded local actionability wait"
                    );
                }
                bail!("object {alias} does not support the requested {operation} operation");
            }
            action["operation"] = Value::String(operation);
            if !batch && action.get("text").is_none() {
                if let Some(value) = action.as_object_mut().and_then(|map| map.remove("value")) {
                    action["text"] = value;
                }
            }
            Ok(())
        };

        if let Some(actions) = arguments.get_mut("actions").and_then(Value::as_array_mut) {
            for action in actions {
                infer(action, true)?;
            }
        } else {
            infer(arguments, false)?;
        }
        Ok(())
    }

    /// Map internal object identities in an act result back to the Agent
    /// aliases the caller used.
    fn collapse_object_aliases(&self, result: &mut Value) {
        let reverse: BTreeMap<&str, &str> = self
            .aliases
            .values()
            .flat_map(|aliases| {
                aliases
                    .by_internal
                    .iter()
                    .map(|(internal, alias)| (internal.as_str(), alias.as_str()))
            })
            .collect();
        fn walk(value: &mut Value, reverse: &BTreeMap<&str, &str>) {
            match value {
                Value::Object(map) => {
                    for (key, item) in map.iter_mut() {
                        if key.ends_with("object_id") {
                            if let Some(alias) =
                                item.as_str().and_then(|current| reverse.get(current))
                            {
                                *item = Value::String((*alias).to_string());
                                continue;
                            }
                        }
                        walk(item, reverse);
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        walk(item, reverse);
                    }
                }
                _ => {}
            }
        }
        walk(result, &reverse);
    }

    fn hydrate_action_arguments(&self, method: &str, arguments: Value) -> Result<Value> {
        if !matches!(
            method,
            "web.act" | "web.act_native" | "web.act_soft" | "web.form.fill"
        ) {
            return Ok(arguments);
        }
        let observation = self.action_context(method, &arguments)?;
        let basis_revision = observation.revision;
        let envelope = || {
            json!({
                "browser_instance_id":observation.browser_instance_id,
                "tab_id":observation.tab_id,
                "document_id":observation.document_id,
                "basis_revision":basis_revision
            })
        };
        if method == "web.form.fill" {
            let actions = arguments
                .get("actions")
                .and_then(Value::as_array)
                .context("actions must be an array")?
                .iter()
                .map(compact_form_action_to_host)
                .collect::<Result<Vec<_>>>()?;
            let mut hydrated = envelope();
            hydrated["actions"] = Value::Array(actions);
            return Ok(hydrated);
        }
        let mut hydrated = envelope();
        let action = compact_action_fields(&arguments, false)?;
        let fields = hydrated
            .as_object_mut()
            .expect("hydrated action envelope must be an object");
        fields.extend(
            action
                .as_object()
                .expect("compact action fields must be an object")
                .clone(),
        );
        Ok(hydrated)
    }

    fn action_context<'a>(
        &'a self,
        method: &str,
        arguments: &Value,
    ) -> Result<&'a ObservationSnapshot> {
        let tokens = if method == "web.form.fill" {
            arguments
                .get("actions")
                .and_then(Value::as_array)
                .context("actions must be an array")?
                .iter()
                .map(|action| string(action, "action_token"))
                .collect::<Result<Vec<_>>>()?
        } else {
            vec![string(arguments, "action_token")?]
        };
        let mut context: Option<&ObservationSnapshot> = None;
        for token in tokens {
            let matches = self
                .tabs
                .values()
                .filter(|observation| {
                    observation
                        .objects
                        .iter()
                        .any(|object| object.action_token.as_deref() == Some(token))
                })
                .collect::<Vec<_>>();
            let current = match matches.as_slice() {
                [] => {
                    bail!("action token is stale or absent from this Agent's current Truth Layer")
                }
                [observation] => *observation,
                _ => bail!("action token is ambiguous across current Agent tabs"),
            };
            if let Some(first) = context {
                if first.browser_instance_id != current.browser_instance_id
                    || first.tab_id != current.tab_id
                    || first.document_id != current.document_id
                    || first.revision != current.revision
                {
                    bail!("form action tokens must belong to one current document revision");
                }
            } else {
                context = Some(current);
            }
        }
        context.context("action request has no current token")
    }

    fn project(&mut self, observation: ObservationSnapshot) -> Result<Value> {
        let aliases = self.aliases_for(&observation);
        let previous = self.tabs.get(&observation.tab_id).cloned();
        let Some(previous) = previous else {
            return self.start_full(observation, &aliases);
        };
        if previous.document_id != observation.document_id || observation.gap {
            return self.start_full(observation, &aliases);
        }

        let current_default_frame = default_frame_id(&observation);
        let previous_objects = previous
            .objects
            .iter()
            .map(|object| (object.object_id.as_str(), object))
            .collect::<BTreeMap<_, _>>();
        let mut changes = Vec::new();
        let mut changed_ids = std::collections::BTreeSet::new();
        let current_objects = observation
            .objects
            .iter()
            .map(|object| (object.object_id.as_str(), object))
            .collect::<BTreeMap<_, _>>();
        for change in &observation.changes {
            if !changed_ids.insert(change.object_id.clone()) {
                bail!("Extension Truth Layer delta repeats an object identity");
            }
            match change.kind {
                ChangeKind::Appeared | ChangeKind::Updated => {
                    let object = current_objects.get(change.object_id.as_str()).context(
                        "Extension Truth Layer delta references a missing current object",
                    )?;
                    if object.object_revision != change.object_revision {
                        bail!("Extension Truth Layer delta has the wrong object revision");
                    }
                    let current_value = agent_object_value(
                        object,
                        current_default_frame.as_deref(),
                        &aliases,
                        self.include_authority,
                    )?;
                    if change.kind == ChangeKind::Appeared {
                        changes.push(json!({"kind":"appeared","object":current_value}));
                        continue;
                    }
                    let prior = previous_objects.get(change.object_id.as_str()).context(
                        "Extension Truth Layer updated an object with no prior identity",
                    )?;
                    let prior_value = agent_object_value(
                        prior,
                        default_frame_id(&previous).as_deref(),
                        &aliases,
                        self.include_authority,
                    )?;
                    let mut patch = json_merge_patch(&prior_value, &current_value);
                    patch
                        .as_object_mut()
                        .context("updated object patch must be an object")?
                        .remove("object_id");
                    if patch.as_object().is_some_and(serde_json::Map::is_empty) {
                        continue;
                    }
                    changes.push(json!({
                        "kind":"updated",
                        "object_id":aliases.get(&change.object_id).context("missing Agent object alias")?,
                        "patch":patch
                    }));
                }
                ChangeKind::Disappeared => {
                    if current_objects.contains_key(change.object_id.as_str()) {
                        bail!("Extension Truth Layer says a current object disappeared");
                    }
                    changes.push(json!({
                        "kind":"disappeared",
                        "object_id":aliases.get(&change.object_id).context("missing Agent object alias")?
                    }));
                }
            }
        }

        let authorities = self.include_authority.then(|| {
            observation
                .objects
                .iter()
                .filter(|object| !changed_ids.contains(&object.object_id))
                .filter_map(|object| {
                    let action_token = object.action_token.as_ref()?;
                    let prior = previous_objects.get(object.object_id.as_str())?;
                    let alias = aliases.get(&object.object_id)?;
                    (prior.action_token.as_ref() != Some(action_token))
                        .then(|| json!({"object_id":alias,"action_token":action_token}))
                })
                .collect::<Vec<_>>()
        });
        let frames_changed = previous.frames != observation.frames;
        let mut view = json!({
            "schema":"saccade.agent-view/1",
            "mode":"delta",
            "browser_instance_id":observation.browser_instance_id,
            "tab_id":observation.tab_id,
            "document_id":observation.document_id,
            "revision":observation.revision,
            "viewport_revision":observation.viewport_revision,
            "geometry":observation.geometry,
            "object_defaults":agent_object_defaults(current_default_frame.as_deref()),
            "updated_object_patch":"json_merge_patch; merge recursively, null removes a field",
            "changes":changes,
            "authorities":authorities,
            "frames":frames_changed.then_some(observation.frames.clone()),
            "coverage":observation.coverage,
            "limitations":observation.limitations,
            "gap":false
        });
        if !self.include_authority {
            view.as_object_mut()
                .expect("delta view must be an object")
                .remove("authorities");
        }
        self.start_view(observation, view)
    }

    fn start_full(
        &mut self,
        observation: ObservationSnapshot,
        aliases: &BTreeMap<String, String>,
    ) -> Result<Value> {
        let tab_id = observation.tab_id.clone();
        self.tabs.remove(&tab_id);
        self.pending_view.remove(&tab_id);
        let full = full_agent_view(observation.clone(), aliases, self.include_authority)?;
        if serde_json::to_vec(&full)?.len() <= MAX_FULL_PAGE_BYTES {
            return self.start_view(observation, full);
        }
        let catalog = catalog_agent_view(&observation, aliases)?;
        self.start_view(observation, catalog)
    }

    fn start_view(&mut self, observation: ObservationSnapshot, view: Value) -> Result<Value> {
        let tab_id = observation.tab_id.clone();
        let pages = bounded_agent_pages(view)?;
        if pages.len() == 1 {
            self.tabs.insert(tab_id, observation);
            return Ok(pages
                .into_iter()
                .next()
                .expect("one Agent view page exists"));
        }
        let first = pages[0].clone();
        self.pending_view.insert(
            tab_id,
            PendingViewDelivery {
                observation,
                pages,
                next_page: 1,
            },
        );
        Ok(first)
    }
}

fn expand_compact_option_alias(action: &mut Value, aliases: &AgentObjectAliases) -> Result<()> {
    let Some(action) = action.as_object_mut() else {
        return Ok(());
    };
    if action.get("operation").and_then(Value::as_str) != Some("select") {
        return Ok(());
    }
    let candidate = action
        .get("option_object_id")
        .and_then(Value::as_str)
        .context("select action omitted option_object_id")?;
    let internal = aliases
        .by_alias
        .get(candidate)
        .cloned()
        .or_else(|| {
            aliases
                .by_internal
                .contains_key(candidate)
                .then(|| candidate.to_string())
        })
        .context("select option alias is stale or unknown")?;
    action.insert("option_object_id".into(), Value::String(internal));
    Ok(())
}

fn catalog_preview(value: &str) -> (String, bool) {
    let mut characters = value.chars();
    let preview = characters
        .by_ref()
        .take(CATALOG_PREVIEW_CHARS)
        .collect::<String>();
    (preview, characters.next().is_some())
}

fn catalog_agent_view(
    observation: &ObservationSnapshot,
    aliases: &BTreeMap<String, String>,
) -> Result<Value> {
    let entries = observation
        .objects
        .iter()
        .map(|object| {
            let alias = aliases
                .get(&object.object_id)
                .context("missing Agent object alias for Truth catalog")?;
            let (label_source, label) = if let Some(name) = object.name.as_deref() {
                (Some("name"), Some(name))
            } else if let Some(text) = object.text.as_deref() {
                (Some("text"), Some(text))
            } else {
                (None, None)
            };
            let (preview, preview_truncated) = label
                .map(catalog_preview)
                .map(|(preview, truncated)| (Some(preview), truncated))
                .unwrap_or((None, false));
            let mut entry = serde_json::Map::from_iter([
                ("object_id".into(), Value::String(alias.clone())),
                ("role".into(), serde_json::to_value(object.role)?),
            ]);
            if let Some(preview) = preview {
                entry.insert("label".into(), Value::String(preview));
            }
            if let Some(label_source) = label_source {
                entry.insert("label_source".into(), Value::String(label_source.into()));
            }
            if preview_truncated {
                entry.insert("label_truncated".into(), Value::Bool(true));
            }
            if !object.affordances.is_empty() {
                entry.insert(
                    "affordances".into(),
                    serde_json::to_value(&object.affordances)?,
                );
            }
            if object.visibility != saccade_protocol::Visibility::Visible {
                entry.insert(
                    "visibility".into(),
                    serde_json::to_value(object.visibility)?,
                );
            }
            if object.protected {
                entry.insert("protected".into(), Value::Bool(true));
            }
            Ok(Value::Object(entry))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({
        "schema":"saccade.agent-view/1",
        "mode":"catalog",
        "browser_instance_id":observation.browser_instance_id,
        "tab_id":observation.tab_id,
        "document_id":observation.document_id,
        "revision":observation.revision,
        "viewport_revision":observation.viewport_revision,
        "geometry":observation.geometry,
        "entries":entries,
        "entry_defaults":{
            "affordances":[],
            "visibility":"visible",
            "protected":false,
            "label_truncated":false
        },
        "object_count":observation.objects.len(),
        "detail_request":{
            "tool":"saccade.truth.read",
            "required":["tab_id","document_id","basis_revision","object_ids"],
            "max_object_ids":MAX_DETAIL_OBJECTS
        },
        "coverage":observation.coverage,
        "limitations":observation.limitations,
        "gap":observation.gap
    }))
}

fn full_agent_view(
    observation: ObservationSnapshot,
    aliases: &BTreeMap<String, String>,
    include_authority: bool,
) -> Result<Value> {
    let default_frame = default_frame_id(&observation);
    let objects = observation
        .objects
        .iter()
        .map(|object| {
            agent_object_value(object, default_frame.as_deref(), aliases, include_authority)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({
        "schema":"saccade.agent-view/1",
        "mode":"full",
        "browser_instance_id":observation.browser_instance_id,
        "tab_id":observation.tab_id,
        "document_id":observation.document_id,
        "revision":observation.revision,
        "viewport_revision":observation.viewport_revision,
            "geometry":observation.geometry,
        "object_defaults":agent_object_defaults(default_frame.as_deref()),
        "frames":observation.frames,
        "objects":objects,
        "coverage":observation.coverage,
        "limitations":observation.limitations,
        "gap":observation.gap
    }))
}

fn bounded_agent_pages(mut view: Value) -> Result<Vec<Value>> {
    let page_limit = if view.get("mode").and_then(Value::as_str) == Some("catalog") {
        MAX_CATALOG_PAGE_BYTES
    } else {
        MAX_FULL_PAGE_BYTES
    };
    if serde_json::to_vec(&view)?.len() <= page_limit {
        return Ok(vec![view]);
    }
    let collection = match view.get("mode").and_then(Value::as_str) {
        Some("full") => "objects",
        Some("delta") => "changes",
        Some("catalog") => "entries",
        _ => bail!("Agent view has no pageable mode"),
    };
    let items = view
        .as_object_mut()
        .context("Agent view must be an object")?
        .remove(collection)
        .and_then(|value| value.as_array().cloned())
        .with_context(|| format!("Agent view must contain {collection}"))?;
    let total_items = items.len();
    let mut chunks: Vec<Vec<Value>> = Vec::new();
    let mut current: Vec<Value> = Vec::new();
    for item in items {
        let mut candidate = current.clone();
        candidate.push(item.clone());
        let mut probe = view.clone();
        probe[collection] = Value::Array(candidate.clone());
        probe["page"] = json!({
            "index":u64::MAX,
            "count":u64::MAX,
            "item_kind":collection,
            "item_offset":total_items,
            "item_count":candidate.len(),
            "total_items":total_items,
            "complete":false,
            "next_call":"saccade.truth.read with the same tab_id"
        });
        if serde_json::to_vec(&probe)?.len() > page_limit && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
        }
        current.push(item);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    let count = chunks.len();
    let mut offset = 0usize;
    let pages = chunks
        .into_iter()
        .enumerate()
        .map(|(index, items)| {
            let item_count = items.len();
            let mut page = view.clone();
            page[collection] = Value::Array(items);
            page["page"] = json!({
                "index":index + 1,
                "count":count,
                "item_kind":collection,
                "item_offset":offset,
                "item_count":item_count,
                "total_items":total_items,
                "complete":index + 1 == count,
                "next_call":(index + 1 != count).then_some("saccade.truth.read with the same tab_id")
            });
            offset += item_count;
            page
        })
        .collect::<Vec<_>>();
    if pages.iter().any(|page| {
        serde_json::to_vec(page)
            .map(|bytes| bytes.len() > page_limit)
            .unwrap_or(true)
    }) {
        bail!("one Truth item exceeds the bounded Agent-view page limit");
    }
    Ok(pages)
}

fn query_term_matches(haystack: &str, needle: &str) -> bool {
    if needle
        .chars()
        .all(|character| character.is_ascii_alphanumeric())
    {
        haystack
            .split(|character: char| !character.is_alphanumeric())
            .any(|word| word == needle)
    } else {
        haystack.contains(needle)
    }
}

fn nearby_context_distance(
    selected: &saccade_protocol::ObservedObject,
    candidate: &saccade_protocol::ObservedObject,
) -> Option<f64> {
    if matches!(
        candidate.visibility,
        saccade_protocol::Visibility::Hidden | saccade_protocol::Visibility::Unknown
    ) {
        return None;
    }
    let selected_center_y = selected.document_bounds.y + selected.document_bounds.height / 2.0;
    let candidate_center_y = candidate.document_bounds.y + candidate.document_bounds.height / 2.0;
    let vertical = (selected_center_y - candidate_center_y).abs();
    if vertical > 260.0 {
        return None;
    }
    let selected_right = selected.document_bounds.x + selected.document_bounds.width;
    let candidate_right = candidate.document_bounds.x + candidate.document_bounds.width;
    let horizontal_gap = if candidate.document_bounds.x > selected_right {
        candidate.document_bounds.x - selected_right
    } else if selected.document_bounds.x > candidate_right {
        selected.document_bounds.x - candidate_right
    } else {
        0.0
    };
    (horizontal_gap <= 160.0).then_some(vertical + horizontal_gap)
}

fn query_search_text(
    observation: &ObservationSnapshot,
    object: &saccade_protocol::ObservedObject,
) -> String {
    let mut parts = vec![query_object_text(object)];
    if object.role != saccade_protocol::SemanticRole::Heading {
        let mut headings = observation
            .objects
            .iter()
            .filter(|candidate| {
                candidate.frame_id == object.frame_id
                    && candidate.role == saccade_protocol::SemanticRole::Heading
                    && candidate.document_bounds.y <= object.document_bounds.y
                    && object.document_bounds.y - candidate.document_bounds.y <= 800.0
            })
            .collect::<Vec<_>>();
        headings.sort_by(|left, right| right.document_bounds.y.total_cmp(&left.document_bounds.y));
        parts.extend(headings.into_iter().take(3).map(query_object_text));
    }
    parts.join(" ")
}

fn canonical_query_role(role: &str) -> &str {
    match role {
        "combobox" | "listbox" => "select",
        role => role,
    }
}

fn query_object_text(object: &saccade_protocol::ObservedObject) -> String {
    [
        object.name.as_deref(),
        object.text.as_deref(),
        object.description.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::to_owned)
    .collect::<Vec<_>>()
    .join(" ")
    .to_lowercase()
}

fn default_frame_id(observation: &ObservationSnapshot) -> Option<String> {
    let mut roots = observation
        .frames
        .iter()
        .filter(|frame| frame.parent_frame_id.is_none());
    let root = roots.next()?;
    roots.next().is_none().then(|| root.frame_id.clone())
}

fn agent_object_defaults(default_frame: Option<&str>) -> Value {
    let mut defaults = serde_json::Map::from_iter([
        ("visibility".into(), Value::String("visible".into())),
        ("transition".into(), Value::String("none".into())),
        ("protected".into(), Value::Bool(false)),
    ]);
    if let Some(frame_id) = default_frame {
        defaults.insert("frame_id".into(), Value::String(frame_id.into()));
    }
    Value::Object(defaults)
}

fn agent_object_value(
    object: &saccade_protocol::ObservedObject,
    default_frame: Option<&str>,
    aliases: &BTreeMap<String, String>,
    include_authority: bool,
) -> Result<Value> {
    let mut value = serde_json::to_value(object)?;
    let fields = value
        .as_object_mut()
        .context("observed object did not serialize as an object")?;
    fields.remove("object_revision");
    fields.remove("loop_class_token");
    if !include_authority {
        fields.remove("action_token");
    }
    fields.insert(
        "object_id".into(),
        Value::String(
            aliases
                .get(&object.object_id)
                .context("missing Agent object alias")?
                .clone(),
        ),
    );
    // `role` is the complete Agent semantic type; the evidence-only `kind`
    // duplicates it. Common safe values live once in object_defaults.
    fields.remove("kind");
    if fields.get("frame_id").and_then(Value::as_str) == default_frame {
        fields.remove("frame_id");
    }
    if fields.get("visibility").and_then(Value::as_str) == Some("visible") {
        fields.remove("visibility");
    }
    if fields.get("transition").and_then(Value::as_str) == Some("none") {
        fields.remove("transition");
    }
    if fields.get("protected").and_then(Value::as_bool) == Some(false) {
        fields.remove("protected");
    }
    Ok(value)
}

/// Return the RFC 7396-style merge patch that transforms `before` into
/// `after`. Updated Agent objects use this compact representation so geometry
/// or one semantic state change never retransmits the whole object.
fn json_merge_patch(before: &Value, after: &Value) -> Value {
    let (Some(before), Some(after)) = (before.as_object(), after.as_object()) else {
        return after.clone();
    };
    let mut patch = serde_json::Map::new();
    for key in before.keys() {
        if !after.contains_key(key) {
            patch.insert(key.clone(), Value::Null);
        }
    }
    for (key, after_value) in after {
        let Some(before_value) = before.get(key) else {
            patch.insert(key.clone(), after_value.clone());
            continue;
        };
        if before_value == after_value {
            continue;
        }
        let difference = json_merge_patch(before_value, after_value);
        if difference
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
        {
            continue;
        }
        patch.insert(key.clone(), difference);
    }
    Value::Object(patch)
}

fn validate_compact_action(value: &Value, operations: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .context("action arguments must be an object")?;
    if string(value, "action_token")?.len() < 32 {
        bail!("action_token must be opaque and current");
    }
    let operation = string(value, "operation")?;
    if !operations.contains(&operation) {
        bail!("action operation is not available for this tool");
    }
    let extra = match operation {
        "click" => None,
        "type" => Some("text"),
        "select" => Some("option_object_id"),
        "upload" => Some("path"),
        _ => unreachable!("operation was allowlisted"),
    };
    let allowed = ["action_token", "operation", extra.unwrap_or("")];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            bail!("unexpected action argument: {key}");
        }
    }
    if let Some(key) = extra {
        value
            .get(key)
            .and_then(Value::as_str)
            .filter(|text| key == "text" || !text.is_empty())
            .with_context(|| format!("{key} must be a string"))?;
    }
    Ok(())
}

fn compact_action_fields(value: &Value, form: bool) -> Result<Value> {
    let action_token = string(value, "action_token")?;
    let operation = string(value, "operation")?;
    let (host_operation, payload) = match operation {
        "click" if !form => ("click", json!({"kind":"none"})),
        "check" if form => ("click", json!({"kind":"none"})),
        "type" => (
            "type",
            json!({
                "kind":"text",
                "text":value.get("text").and_then(Value::as_str).context("text must be a string")?
            }),
        ),
        "select" => (
            "select",
            json!({
                "kind":"select",
                "option_object_id":string(value, "option_object_id")?
            }),
        ),
        "upload" if !form => (
            "upload",
            json!({"kind":"file","path":string(value, "path")?}),
        ),
        _ => bail!("action operation is not available in this context"),
    };
    Ok(json!({
        "action_token":action_token,
        "operation":host_operation,
        "payload":payload
    }))
}

fn compact_form_action_to_host(value: &Value) -> Result<Value> {
    compact_action_fields(value, true)
}

fn validate_public_action_shape(value: &Value, batch: bool) -> Result<()> {
    let action = value.as_object().context("action must be an object")?;
    string(value, "object_id")?;
    let operation = action
        .get("operation")
        .map(|_| string(value, "operation"))
        .transpose()?;
    if operation.is_some_and(|operation| !matches!(operation, "click" | "type" | "select")) {
        bail!("operation must be click, type, or select");
    }
    let has_option = action.get("option_object_id").is_some();
    let has_text = action.get("text").is_some();
    let has_value = action.get("value").is_some();
    if batch && has_text {
        bail!("batch type actions use value, not text");
    }
    if has_text && has_value {
        bail!("single type actions cannot carry both text and value");
    }
    if has_option && (has_text || has_value) {
        bail!("an action cannot carry both text and option_object_id");
    }
    if has_option {
        string(value, "option_object_id")?;
    }
    if has_text || has_value {
        let field = if has_value { "value" } else { "text" };
        value
            .get(field)
            .and_then(Value::as_str)
            .filter(|text| text.len() <= 8192)
            .with_context(|| format!("{field} must be a string within 8192 bytes"))?;
    }
    match operation {
        Some("click") if has_option || has_text || has_value => {
            bail!("click cannot carry text, value, or option_object_id")
        }
        Some("type") if !has_text && !has_value => {
            bail!("type requires text or value within 8192 bytes")
        }
        Some("type") if has_option => bail!("type cannot carry option_object_id"),
        Some("select") if !has_option => bail!("select requires option_object_id"),
        Some("select") if has_text || has_value => bail!("select cannot carry text or value"),
        _ => {}
    }
    Ok(())
}

fn validate_public_act(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .context("act arguments must be an object")?;
    for required in ["tab_id", "document_id", "basis_revision"] {
        if !object.contains_key(required) {
            bail!("missing required argument: {required}");
        }
    }
    string(value, "tab_id")?;
    string(value, "document_id")?;
    if value
        .get("basis_revision")
        .and_then(Value::as_u64)
        .filter(|revision| *revision > 0)
        .is_none()
    {
        bail!("basis_revision must be a positive integer");
    }
    if value.get("actions").is_some() {
        for key in object.keys() {
            if ![
                "tab_id",
                "document_id",
                "basis_revision",
                "actions",
                "timeout_ms",
            ]
            .contains(&key.as_str())
            {
                bail!("unexpected act batch argument {key}");
            }
        }
        let actions = value
            .get("actions")
            .and_then(Value::as_array)
            .context("actions must be an array")?;
        if actions.is_empty() || actions.len() > 32 {
            bail!("actions must contain between 1 and 32 form operations");
        }
        for item in actions {
            let action = item.as_object().context("batch action must be an object")?;
            for key in action.keys() {
                if !["object_id", "operation", "option_object_id", "value"].contains(&key.as_str())
                {
                    bail!("unexpected batch action argument {key}");
                }
            }
            validate_public_action_shape(item, true)?;
        }
        return Ok(());
    }
    for key in object.keys() {
        if ![
            "tab_id",
            "object_id",
            "operation",
            "document_id",
            "basis_revision",
            "option_object_id",
            "text",
            "value",
            "timeout_ms",
        ]
        .contains(&key.as_str())
        {
            bail!("unexpected act argument {key}");
        }
    }
    validate_public_action_shape(value, false)
}

fn validate_form_fill(value: &Value) -> Result<()> {
    let actions = value
        .get("actions")
        .and_then(Value::as_array)
        .context("actions must be an array")?;
    if actions.is_empty() || actions.len() > 32 {
        bail!("actions must contain between 1 and 32 form operations");
    }
    for item in actions {
        let object = item.as_object().context("form action must be an object")?;
        let operation = string(item, "operation")?;
        let extra = match operation {
            "type" => Some("text"),
            "select" => Some("option_object_id"),
            "check" => None,
            _ => bail!("form action operation must be type, select, or check"),
        };
        let allowed = ["action_token", "operation", extra.unwrap_or("")];
        for key in object.keys() {
            if !allowed.contains(&key.as_str()) {
                bail!("unexpected form action argument {key}");
            }
        }
        if string(item, "action_token")?.len() < 32 {
            bail!("action_token must be opaque and current");
        }
        if let Some(key) = extra {
            item.get(key)
                .and_then(Value::as_str)
                .filter(|text| key == "text" || !text.is_empty())
                .with_context(|| format!("{key} must be a string"))?;
        }
    }
    Ok(())
}

fn tool_result_summary(value: &Value) -> String {
    let schema = value
        .get("schema")
        .and_then(Value::as_str)
        .unwrap_or("saccade.result");
    if let Some(transition) = value.get("transition") {
        let verified = value
            .get("verified")
            .and_then(Value::as_bool)
            .or_else(|| value.get("all_verified").and_then(Value::as_bool))
            .unwrap_or(false);
        let revision = transition
            .get("revision")
            .and_then(Value::as_u64)
            .or_else(|| value.get("revision").and_then(Value::as_u64));
        let changes = transition
            .get("changes")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let signals = changes
            .iter()
            .filter_map(|change| change.get("object").or_else(|| change.get("patch")))
            .filter_map(|object| {
                object
                    .get("text")
                    .or_else(|| object.get("name"))
                    .and_then(Value::as_str)
            })
            .filter(|text| !text.is_empty())
            .take(2)
            .map(|text| text.chars().take(120).collect::<String>())
            .collect::<Vec<_>>();
        let mut summary = format!(
            "saccade.act verified={verified} revision={} transition_changes={} follow_up_read_required=false",
            revision
                .map(|revision| revision.to_string())
                .unwrap_or_else(|| "unknown".into()),
            changes.len()
        );
        if !signals.is_empty() {
            summary.push_str(" signals=");
            summary.push_str(&signals.join(" | "));
        }
        return summary;
    }
    let mode = value.get("mode").and_then(Value::as_str);
    let revision = value.get("revision").and_then(Value::as_u64);
    match (mode, revision) {
        (Some(mode), Some(revision)) => format!("{schema} {mode} revision {revision}"),
        _ => schema.to_string(),
    }
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .with_context(|| format!("{key} must be a non-empty string"))
}

fn write_rpc(
    output: &mut impl Write,
    id: Value,
    result: Result<Value, (i32, String)>,
) -> Result<()> {
    let response = match result {
        Ok(value) => json!({"jsonrpc":"2.0","id":id,"result":value}),
        Err((code, message)) => {
            json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
        }
    };
    serde_json::to_writer(&mut *output, &response)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_keeps_mcp_alive_while_native_host_is_temporarily_absent() {
        let directory = tempfile::tempdir().unwrap();
        let host = HostClient::connect(directory.path().join("missing-grant.json")).unwrap();
        let profile = Profile {
            name: "聪明的野蛮人 CEO".into(),
            behavior: "全权推进目标。".into(),
            ban: Vec::new(),
        };
        let response = initialize(&host, McpMode::Truth, &profile).unwrap();
        assert_eq!(response["serverInfo"]["name"], "saccade-truth-layer");
        let instructions = response["instructions"].as_str().unwrap();
        assert!(instructions.contains("Call saccade.system.capabilities once"));
        assert!(instructions.contains("instead of falling back"));
        assert!(instructions.contains("@nanlogic/saccade doctor"));
        assert!(!instructions.contains("聪明的野蛮人 CEO"));
        assert!(!instructions.contains("全权推进目标"));
        assert!(instructions.len() <= 800);
        assert!(serde_json::to_vec(&response).unwrap().len() <= 1_500);
    }

    #[test]
    fn capabilities_report_disconnected_without_waiting_for_an_absent_host() {
        let directory = tempfile::tempdir().unwrap();
        let host = HostClient::connect(directory.path().join("missing-grant.json")).unwrap();
        let profile = Profile {
            name: "offline profile".into(),
            behavior: "Keep the adapter available while the browser reconnects.".into(),
            ban: Vec::new(),
        };
        let mut agent_views = AgentViewState::new(false);
        let started = std::time::Instant::now();
        let response = call_tool(
            &host,
            &mut agent_views,
            "saccade.system.capabilities",
            json!({}),
            McpMode::Truth,
            false,
            &profile,
        )
        .unwrap();

        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(response["schema"], "saccade.capabilities/6");
        assert_eq!(response["extension_connected"], false);
        assert_eq!(response["extension_candidate"], Value::Null);
        assert_eq!(response["profile"]["name"], "offline profile");
        assert_eq!(
            response["profile"]["behavior_delivery"],
            "capabilities_once"
        );
        assert!(response["mcp_contract_hash"].as_str().is_some());
    }

    #[cfg(unix)]
    #[test]
    fn initialize_does_not_hide_invalid_host_grants() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let grant_path = directory.path().join("invalid-grant.json");
        std::fs::write(&grant_path, b"not-json").unwrap();
        std::fs::set_permissions(&grant_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let host = HostClient::connect(grant_path).unwrap();
        assert!(initialize(&host, McpMode::Truth, &Profile::default()).is_err());
    }

    /// Slice one function body so a later function cannot satisfy an assertion
    /// about this one.
    fn bounded<'a>(source: &'a str, signature: &str) -> &'a str {
        let start = source.split(signature).nth(1).expect("function exists");
        match start.find("\n    fn ") {
            Some(end) => &start[..end],
            None => start,
        }
    }

    fn act_object_body(source: &str) -> &str {
        bounded(source, "fn act_object(")
    }

    #[test]
    fn the_execution_contract_names_act_first_and_forbids_screenshot_substitution() {
        // A client that keeps its own executor must still be told, in the text
        // it always reads, that Truth is the observation route and act is the
        // execution route.
        let behavior = include_str!("mcp.rs");
        assert!(behavior.contains("saccade.act is the preferred way to operate a page"));
        assert!(behavior.contains("Never pass a coordinate"));
        assert!(behavior.contains("in place of Truth"));
        assert!(behavior.contains("external_execution_required"));
        assert!(behavior.contains("Never repeat an action when retry_safe is false"));
    }

    #[test]
    fn act_is_object_addressed_and_cannot_carry_coordinates() {
        // The whole point of the tool is that the wrong call is unrepresentable:
        // there is no place to put a pixel.
        let act = tools(McpMode::Truth, false)
            .into_iter()
            .find(|tool| tool["name"] == "saccade.act")
            .expect("saccade.act is a default public tool");
        let schema = &act["inputSchema"];
        assert_eq!(schema["additionalProperties"], Value::Bool(false));
        let properties = schema["properties"].as_object().expect("properties");
        for forbidden in [
            "x",
            "y",
            "coordinate",
            "screen_bounds",
            "action_token",
            "ignore_learned_policy",
        ] {
            assert!(
                !properties.contains_key(forbidden),
                "saccade.act must not accept {forbidden}"
            );
        }
        // Only what the Extension's software pipe actually implements.
        assert_eq!(
            schema["properties"]["operation"]["enum"],
            serde_json::json!(["click", "select", "type"])
        );
        // Stale replay protection is not optional.
        let required = schema["required"].as_array().expect("required");
        for key in ["tab_id", "document_id", "basis_revision"] {
            assert!(
                required.iter().any(|value| value == key),
                "{key} must be required"
            );
        }
        // Claude and other Agent tool registries reject top-level schema
        // composition. Runtime validation remains authoritative for requiring
        // exactly one of the single-action and batch forms.
        for composition in ["oneOf", "allOf", "anyOf"] {
            assert!(schema.get(composition).is_none());
        }
        assert_eq!(schema["properties"]["actions"]["maxItems"], 32);
        assert_eq!(
            schema["properties"]["actions"]["items"]["required"],
            json!(["object_id"])
        );
        assert_eq!(host_method("act", McpMode::Truth), Some("web.act_object"));
        assert!(validate_arguments(
            "web.act_object",
            &json!({
                "tab_id":"tab-1",
                "document_id":"doc-1",
                "basis_revision":7,
                "actions":[
                    {"object_id":"o1","value":"ordinary value"},
                    {"object_id":"o2","option_object_id":"o3"},
                    {"object_id":"o4"}
                ]
            }),
            false
        )
        .is_ok());
        for invalid in [
            json!({"tab_id":"tab-1","document_id":"doc-1","basis_revision":7,"actions":[]}),
            json!({"tab_id":"tab-1","document_id":"doc-1","basis_revision":7,"actions":[{"object_id":"o1","operation":"upload"}]}),
            json!({"tab_id":"tab-1","document_id":"doc-1","basis_revision":7,"object_id":"o1","operation":"click","actions":[{"object_id":"o2","operation":"click"}]}),
            json!({"tab_id":"tab-1","document_id":"doc-1","basis_revision":7,"actions":[{"object_id":"o1","operation":"type"}]}),
            json!({"tab_id":"tab-1","document_id":"doc-1","basis_revision":7,"object_id":"o1","value":"x","option_object_id":"o2"}),
            json!({"tab_id":"tab-1","document_id":"doc-1","basis_revision":7,"object_id":"o1","operation":"click","value":"x"}),
        ] {
            assert!(validate_arguments("web.act_object", &invalid, false).is_err());
        }
    }

    #[test]
    fn act_never_reaches_the_native_backend() {
        // Truth mode promotes software execution only; native input and the
        // Accessibility grant stay behind the Reference Actuator.
        let source = include_str!("session.rs");
        let body = act_object_body(source);
        assert!(!body.contains("InputBackend::Native"));
        assert!(body.contains("Some(InputBackend::Soft)"));
        assert!(body.contains("InputPolicy::NativeRequired"));
        assert!(body.contains("Self::external_required"));
        // and that helper is the only thing that emits the hand-off dispatch
        assert!(bounded(source, "fn external_required(").contains("external_execution_required"));
    }

    #[test]
    fn act_verification_evidence_is_defined_per_role() {
        let source = include_str!("session.rs");
        let table = bounded(source, "fn verification_field(");
        for (role, field) in [
            ("Checkbox", "checked"),
            ("Tab", "selected"),
            ("Button", "pressed"),
            ("MenuItem", "expanded"),
            ("ReflexTarget", "reflex_occurrence"),
        ] {
            assert!(
                table.contains(role),
                "role {role} must have defined evidence"
            );
            assert!(table.contains(field), "field {field} must be named");
        }
        // A revision bump alone, or an unrelated object changing, is never proof.
        let body = act_object_body(source);
        assert!(body.contains("target semantic state did not change"));
        assert!(body.contains("accepted_but_unverified"));
        assert!(body.contains("software input left the target semantic state unchanged"));
        assert!(body.contains("Self::software_handoff"));
    }

    #[test]
    fn act_link_verification_matches_the_navigation_target_exactly() {
        let source = include_str!("session.rs");
        let body = act_object_body(source);
        // Cross-document and same-document anchors both require the resulting
        // URL to equal the link's own navigation_target; arbitrary URL churn
        // must never be attributed to the click.
        assert!(body.contains("navigation_target"));
        assert!(body.contains("already_there"));
        assert!(body.contains("same_document"));
        assert!(body.contains("current document URL does not match"));
        // A non-HTTP(S) destination is handed to the Agent client.
        assert!(body.contains("navigation target is not an HTTP(S) destination"));
        assert!(body.contains("Self::external_required"));
    }

    #[test]
    fn rpc_and_first_slice_tools_are_strict() {
        let request: RpcRequest = serde_json::from_value(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        )
        .unwrap();
        assert_eq!(request.method, "initialize");
        let truth_tools = tools(McpMode::Truth, false);
        assert_eq!(truth_tools.len(), 6);
        assert_eq!(tools(McpMode::Reference, false).len(), 11);
        assert_eq!(tools(McpMode::Reference, true).len(), 13);
        assert_eq!(
            truth_tools
                .iter()
                .map(|tool| tool["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![
                "saccade.system.capabilities",
                "saccade.tabs.list",
                "saccade.tabs.open",
                "saccade.tabs.close",
                "saccade.truth.read",
                "saccade.act"
            ]
        );
        assert!(host_method("web.observe", McpMode::Truth).is_none());
        assert!(host_method("web.act", McpMode::Truth).is_none());
        assert_eq!(
            host_method("truth.read", McpMode::Truth),
            Some("web.observe")
        );
        assert!(tools(McpMode::Reference, false)
            .iter()
            .find(|tool| tool["name"] == "saccade.reference.reflex.run")
            .unwrap()
            .pointer("/inputSchema/properties/input_backend")
            .is_none());
        let truth_read = truth_tools
            .iter()
            .find(|tool| tool["name"] == "saccade.truth.read")
            .unwrap();
        assert_eq!(
            truth_read["inputSchema"]["properties"]["delivery_mode"]["enum"],
            json!(["live", "economy"])
        );
        assert_eq!(
            truth_read["inputSchema"]["properties"]["resync"]["type"],
            json!("boolean")
        );
        assert_eq!(truth_read["inputSchema"]["required"], json!(["tab_id"]));
        assert_eq!(
            truth_read["inputSchema"]["properties"]["query"]["properties"]["max_objects"]
                ["maximum"],
            MAX_QUERY_OBJECTS
        );
        assert_eq!(
            truth_read["inputSchema"]["properties"]["query"]["properties"]["min_objects"]
                ["default"],
            1
        );
        assert_eq!(
            truth_read["inputSchema"]["properties"]["query"]["properties"]["visible_only"]
                ["default"],
            false
        );
        assert!(
            truth_read["inputSchema"]["properties"]["query"]["properties"]["roles"]["description"]
                .as_str()
                .unwrap()
                .contains("both are accepted as input aliases")
        );
        assert!(truth_read["description"]
            .as_str()
            .unwrap()
            .contains("one tab"));
        assert!(truth_read["description"]
            .as_str()
            .unwrap()
            .contains("later views are deltas"));
        assert!(truth_read["inputSchema"]["properties"]
            .get("view_mode")
            .is_none());
        assert!(truth_tools
            .iter()
            .find(|tool| tool["name"] == "saccade.system.capabilities")
            .unwrap()["description"]
            .as_str()
            .unwrap()
            .contains("Call once before browser work"));
        let tabs_open = truth_tools
            .iter()
            .find(|tool| tool["name"] == "saccade.tabs.open")
            .unwrap();
        assert!(tabs_open["description"]
            .as_str()
            .unwrap()
            .contains("Open and authorize"));
        // The provisioned claim is callable over MCP, not extension-internal.
        let open_schema = &tabs_open["inputSchema"];
        assert_eq!(
            open_schema["properties"]["claim"]["enum"],
            json!(["arm", "confirm"])
        );
        assert_eq!(
            open_schema["properties"]["claim_id"]["type"],
            json!("string")
        );
        assert_eq!(open_schema["properties"]["tab_id"]["type"], json!("string"));
        assert_eq!(open_schema["required"], json!(["url"]));
        assert_eq!(open_schema["additionalProperties"], json!(false));
        // Claude and other Agent tool registries reject top-level schema
        // composition. Runtime validation below remains authoritative for the
        // cross-field arm/confirm constraints.
        for composition in ["oneOf", "allOf", "anyOf"] {
            assert!(open_schema.get(composition).is_none());
        }
        assert!(tabs_open["description"]
            .as_str()
            .unwrap()
            .contains("claim arm/confirm"));
        // The claim stays generic: no model, vendor, or client name in the wire
        // contract.
        let open_text = serde_json::to_string(tabs_open).unwrap().to_lowercase();
        for vendor in [
            "claude",
            "codex",
            "openai",
            "anthropic",
            "gemini",
            "playwright",
        ] {
            assert!(!open_text.contains(vendor), "tabs.open leaked {vendor}");
        }
        assert!(truth_read["description"]
            .as_str()
            .unwrap()
            .contains("canonical Truth for one tab"));
        assert!(require_tool_enabled("web.act_native", false).is_err());
        assert!(require_tool_enabled("web.act_soft", false).is_err());
        assert!(require_tool_enabled("web.act_native", true).is_ok());
        let action_schema = action_request_schema(&["click", "type", "select", "upload"]);
        assert_eq!(action_schema["oneOf"].as_array().unwrap().len(), 4);
        assert_eq!(
            action_schema["oneOf"][2]["required"],
            json!(["option_object_id"])
        );
        assert_eq!(form_action_schema()["oneOf"].as_array().unwrap().len(), 3);
        assert!(serde_json::from_value::<RpcRequest>(
            json!({"jsonrpc":"2.0","id":1,"method":"ping","unexpected":true})
        )
        .is_err());
        assert!(validate_arguments(
            "web.observe",
            &json!({"tab_id":"x","selector":"button"}),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "web.observe",
            &json!({"tab_id":"x","after_revision":4,"timeout_ms":1000}),
            false
        )
        .is_ok());
        assert!(validate_arguments(
            "web.observe",
            &json!({"tab_id":"x","after_revision":4,"delivery_mode":"economy"}),
            false
        )
        .is_ok());
        assert!(
            validate_arguments("web.observe", &json!({"tab_id":"x","resync":true}), false).is_ok()
        );
        assert!(
            validate_arguments("web.observe", &json!({"tab_id":"x","resync":"all"}), false)
                .is_err()
        );
        assert!(validate_arguments(
            "web.observe",
            &json!({"tab_id":"x","delivery_mode":"forced"}),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "web.observe",
            &json!({"tab_id":"x","view_mode":"full"}),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "web.observe",
            &json!({
                "tab_id":"x",
                "object_ids":["o3","o8"],
                "document_id":"doc-1",
                "basis_revision":7
            }),
            false
        )
        .is_ok());
        assert!(validate_arguments(
            "web.observe",
            &json!({"tab_id":"x","object_ids":["o3"],"basis_revision":7}),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "web.observe",
            &json!({
                "tab_id":"x",
                "query":{
                    "text_any":["Text input","Default checkbox"],
                    "roles":["text_field","select"],
                    "affordances":["type"],
                    "frame_scope":"root",
                    "min_objects":4,
                    "max_objects":12
                },
                "timeout_ms":10000
            }),
            false
        )
        .is_ok());
        assert!(validate_arguments(
            "web.observe",
            &json!({"tab_id":"x","query":{"text_any":[]}}),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "web.observe",
            &json!({"tab_id":"x","query":{"frame_scope":"root"}}),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "web.observe",
            &json!({
                "tab_id":"x",
                "query":{"roles":["button"],"max_objects":33}
            }),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "web.observe",
            &json!({
                "tab_id":"x",
                "query":{"roles":["button"],"min_objects":5,"max_objects":4}
            }),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "web.observe",
            &json!({
                "tab_id":"x",
                "query":{"roles":["button"]},
                "after_revision":7
            }),
            false
        )
        .is_ok());
        assert!(validate_arguments(
            "web.observe",
            &json!({
                "tab_id":"x",
                "query":{"roles":["button"]},
                "after_revision":7,
                "delivery_mode":"economy"
            }),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "web.observe",
            &json!({
                "tab_id":"x",
                "object_ids":["o3"],
                "document_id":"doc-1",
                "basis_revision":7,
                "after_revision":7
            }),
            false
        )
        .is_err());
        let mut economy = json!({"tab_id":"x","delivery_mode":"economy"});
        assert_eq!(
            TruthDeliveryMode::from_arguments(&mut economy).unwrap(),
            TruthDeliveryMode::Economy
        );
        assert!(economy.get("delivery_mode").is_none());
        assert!(validate_arguments(
            "web.act",
            &json!({
                "action_token":"token.0123456789abcdef0123456789abcdef",
                "operation":"click"
            }),
            false
        )
        .is_ok());
        assert!(validate_arguments(
            "web.act",
            &json!({
                "tab_id":"redundant",
                "action_token":"token.0123456789abcdef0123456789abcdef",
                "operation":"click"
            }),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "web.observe",
            &json!({"tab_id":"x","timeout_ms":1000}),
            false
        )
        .is_ok());
        assert!(validate_arguments(
            "tabs.open",
            &json!({"url":"https://fixture.test","active":true}),
            false
        )
        .is_ok());
        assert!(validate_arguments("tabs.open", &json!({"active":true}), false).is_err());
        assert!(
            validate_arguments("tabs.open", &json!({"url":"file:///tmp/form"}), false).is_err()
        );
        assert!(validate_arguments(
            "tabs.open",
            &json!({"url":"https://fixture.test","active":"yes"}),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "tabs.open",
            &json!({"url":"https://fixture.test","claim":"arm"}),
            false
        )
        .is_ok());
        assert!(validate_arguments(
            "tabs.open",
            &json!({"url":"https://fixture.test","claim":"confirm","claim_id":"claim.abc","tab_id":"9"}),
            false
        )
        .is_ok());
        // arm carries no tab identity, confirm requires both halves, and no
        // third claim mode exists.
        assert!(validate_arguments(
            "tabs.open",
            &json!({"url":"https://fixture.test","claim":"arm","tab_id":"9"}),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "tabs.open",
            &json!({"url":"https://fixture.test","claim":"confirm","claim_id":"claim.abc"}),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "tabs.open",
            &json!({"url":"https://fixture.test","claim":"confirm","tab_id":"9"}),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "tabs.open",
            &json!({"url":"https://fixture.test","claim":"adopt"}),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "tabs.open",
            &json!({"url":"https://fixture.test","claim":"arm","active":true}),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "tabs.open",
            &json!({"url":"https://fixture.test","claim_id":"claim.abc"}),
            false
        )
        .is_err());
        assert!(validate_arguments("tabs.close", &json!({"tab_id":"7"}), false).is_ok());
        assert!(validate_arguments("tabs.close", &json!({}), false).is_err());
        assert!(
            validate_arguments("tabs.close", &json!({"tab_id":"7","force":true}), false).is_err()
        );
        assert!(validate_arguments(
            "web.form.fill",
            &json!({
                "actions":[{
                    "action_token":"token.0123456789abcdef0123456789abcdef",
                    "operation":"type",
                    "text":"private immediate payload"
                }]
            }),
            false
        )
        .is_ok());
        assert!(validate_arguments(
            "web.reflex.run",
            &json!({"tab_id":"x","input_backend":"native"}),
            false
        )
        .is_err());
        assert!(validate_arguments(
            "web.reflex.run",
            &json!({"tab_id":"x","input_backend":"native"}),
            true
        )
        .is_ok());
        assert!(INITIALIZE_INSTRUCTIONS.contains("Fold verified transitions"));
        assert!(INITIALIZE_INSTRUCTIONS.contains("Resync only the exact tab"));
        assert!(INITIALIZE_INSTRUCTIONS.contains("external_execution_required"));
        assert!(INITIALIZE_INSTRUCTIONS.contains("exact store link"));
        assert!(INITIALIZE_INSTRUCTIONS.len() <= 800);

        let mut public_capabilities = json!({"profile":{
            "name":"focused",
            "behavior":"Work in page order.",
            "ban":["secret internal instruction"]
        }});
        let digest = project_profile_capabilities(&mut public_capabilities);
        assert_eq!(public_capabilities["profile"]["name"], "focused");
        assert_eq!(
            public_capabilities["profile"]["behavior"],
            "Work in page order."
        );
        assert_eq!(
            public_capabilities["profile"]["behavior_delivery"],
            "capabilities_once"
        );
        assert!(public_capabilities["profile"].get("ban").is_none());
        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|ch| ch.is_ascii_hexdigit()));

        let listed = json!({"tools":tools(McpMode::Truth, false)});
        let listed_bytes = serde_json::to_vec(&listed).unwrap().len();
        assert!(listed_bytes <= 7_000, "tools/list is {listed_bytes} bytes");
        let initialized = json!({
            "protocolVersion":MCP_VERSION,
            "capabilities":{"tools":{"listChanged":false},"resources":{"subscribe":true,"listChanged":false}},
            "serverInfo":{"name":"saccade-truth-layer","version":env!("CARGO_PKG_VERSION")},
            "instructions":INITIALIZE_INSTRUCTIONS
        });
        let initialize_bytes = serde_json::to_vec(&initialized).unwrap().len();
        assert!(initialize_bytes <= 1_500);
        assert!(initialize_bytes + listed_bytes <= 8_500);
        for tool in tools(McpMode::Truth, false) {
            let description = tool["description"].as_str().unwrap();
            assert!(description.len() <= 420);
            assert!(!description.contains("Work in page order"));
            assert!(!description.contains("Active Saccade Profile"));
        }
        let contract_hash = truth_contract_hash();
        assert_eq!(contract_hash.len(), 64);
        assert!(contract_hash.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_eq!(contract_hash, truth_contract_hash());
    }

    #[test]
    fn agent_view_is_full_once_then_reports_only_semantic_changes() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let mut first: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let mut unchanged = first.objects[0].clone();
        unchanged.object_id = "object-2".into();
        unchanged.name = Some("Unchanged control".into());
        unchanged.action_token = Some("token.1111111111111111111111111111111111111111".into());
        first.objects.push(unchanged);
        let mut views = AgentViewState::default();
        let full = views.project(first.clone()).unwrap();
        assert_eq!(full["schema"], "saccade.agent-view/1");
        assert_eq!(full["mode"], "full");
        assert_eq!(full["objects"].as_array().unwrap().len(), 2);
        assert_eq!(full["objects"][0]["object_id"], "o1");
        assert_eq!(full["objects"][1]["object_id"], "o2");
        assert_eq!(full["objects"][0]["document_bounds"]["x"], 10.0);
        assert_eq!(full["objects"][0]["viewport_bounds"]["width"], 100.0);
        assert_eq!(full["object_defaults"]["visibility"], "visible");
        assert_eq!(full["object_defaults"]["transition"], "none");
        assert_eq!(full["object_defaults"]["protected"], false);
        assert_eq!(full["object_defaults"]["frame_id"], "frame.fixture");
        for object in full["objects"].as_array().unwrap() {
            assert!(object.get("kind").is_none());
            assert!(object.get("frame_id").is_none());
            assert!(object.get("visibility").is_none());
            assert!(object.get("transition").is_none());
            assert!(object.get("protected").is_none());
            assert!(object.get("action_token").is_none());
        }

        first.revision += 1;
        first.viewport_revision += 1;
        first.objects[0].object_revision += 1;
        first.objects[0].document_bounds.x = 25.0;
        first.objects[0].viewport_bounds.as_mut().unwrap().x = 25.0;
        first.changes = vec![saccade_protocol::ObservationChange {
            kind: ChangeKind::Updated,
            object_id: first.objects[0].object_id.clone(),
            object_revision: first.objects[0].object_revision,
        }];
        let geometry_delta = views.project(first.clone()).unwrap();
        assert_eq!(geometry_delta["mode"], "delta");
        assert_eq!(geometry_delta["changes"][0]["kind"], "updated");
        assert_eq!(geometry_delta["changes"][0]["object_id"], "o1");
        assert_eq!(
            geometry_delta["changes"][0]["patch"]["document_bounds"]["x"],
            25.0
        );
        assert_eq!(
            geometry_delta["changes"][0]["patch"]["viewport_bounds"]["x"],
            25.0
        );
        assert!(geometry_delta["changes"][0]["patch"]["document_bounds"]
            .get("width")
            .is_none());

        first.revision += 1;
        first.viewport_revision += 1;
        first.objects[0].object_revision += 1;
        first.objects[0]
            .state
            .insert("pressed".into(), "true".into());
        first.changes = vec![saccade_protocol::ObservationChange {
            kind: ChangeKind::Updated,
            object_id: first.objects[0].object_id.clone(),
            object_revision: first.objects[0].object_revision,
        }];
        first.objects[0].action_token = Some("token.abcdef0123456789abcdef0123456789abcdef".into());
        first.objects[1].object_revision += 1;
        first.objects[1].action_token =
            Some("token.2222222222222222222222222222222222222222".into());
        let delta = views.project(first.clone()).unwrap();
        assert_eq!(delta["mode"], "delta");
        assert_eq!(delta["changes"].as_array().unwrap().len(), 1);
        assert_eq!(delta["changes"][0]["kind"], "updated");
        assert!(delta.get("authorities").is_none());
        assert!(delta.get("objects").is_none());

        first.revision += 1;
        first.objects[1].action_token = None;
        first.changes = vec![saccade_protocol::ObservationChange {
            kind: ChangeKind::Updated,
            object_id: first.objects[1].object_id.clone(),
            object_revision: first.objects[1].object_revision,
        }];
        let unavailable = views.project(first.clone()).unwrap();
        assert!(unavailable["changes"].as_array().unwrap().is_empty());

        let mut reference = AgentViewState::new(true);
        let reference_full = reference.project(first.clone()).unwrap();
        assert!(reference_full["objects"][0].get("action_token").is_some());
    }

    #[test]
    fn oversized_full_truth_becomes_a_bounded_catalog_then_dereferences_stable_ids() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let mut large: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let template = large.objects[0].clone();
        for index in 0..140 {
            let mut object = template.clone();
            object.object_id = format!("large-object-{index}");
            object.name = Some(format!("Generated control {index} {}", "x".repeat(180)));
            object.action_token = Some(format!(
                "token.{index:04}.0123456789abcdef0123456789abcdef0123456789abcdef"
            ));
            large.objects.push(object);
        }
        let expected_objects = large.objects.len();
        let tab_id = large.tab_id.clone();
        let mut views = AgentViewState::default();

        let first = views.project(large.clone()).unwrap();
        assert_eq!(first["mode"], "catalog");
        assert!(first.get("page").is_none());
        assert!(serde_json::to_vec(&first).unwrap().len() <= MAX_CATALOG_PAGE_BYTES);
        assert_eq!(views.revision_for_tab(&tab_id), Some(large.revision));
        assert!(!views.has_pending_view(&tab_id));

        let requested_id = first["entries"][0]["object_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            first["entries"][0]["label"]
                .as_str()
                .unwrap()
                .chars()
                .count()
                <= CATALOG_PREVIEW_CHARS
        );
        assert_eq!(first["entries"].as_array().unwrap().len(), expected_objects);
        assert_eq!(views.revision_for_tab(&tab_id), Some(large.revision));
        let details = views
            .detail_view(&tab_id, &large.document_id, large.revision, &[requested_id])
            .unwrap();
        assert_eq!(details["mode"], "details");
        assert_eq!(details["objects"].as_array().unwrap().len(), 1);
        assert_eq!(details["cursor_advanced"], false);
        assert_eq!(views.revision_for_tab(&tab_id), Some(large.revision));

        large.revision += 1;
        large.changes.clear();
        assert_eq!(views.project(large).unwrap()["mode"], "delta");
    }

    #[test]
    fn pending_catalog_pages_are_isolated_by_tab_and_resync_discards_only_that_sequence() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let mut large: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let template = large.objects[0].clone();
        for index in 0..600 {
            let mut object = template.clone();
            object.object_id = format!("pending-{index}");
            object.name = Some("y".repeat(240));
            large.objects.push(object);
        }
        let mut other = serde_json::from_str::<ObservationSnapshot>(fixture).unwrap();
        other.tab_id = "tab.other".into();
        other.document_id = "document.other".into();
        other.frames[0].document_id = "document.other".into();

        let mut views = AgentViewState::default();
        views.project(large.clone()).unwrap();
        assert!(views.has_pending_view(&large.tab_id));
        assert_eq!(views.project(other.clone()).unwrap()["mode"], "full");
        assert_eq!(views.revision_for_tab(&other.tab_id), Some(other.revision));

        views.reset_cursor(&large.tab_id);
        assert!(!views.has_pending_view(&large.tab_id));
        assert_eq!(views.revision_for_tab(&other.tab_id), Some(other.revision));
        let reset = views.project(large).unwrap();
        assert_eq!(reset["mode"], "catalog");
        assert_eq!(reset["page"]["index"], 1);
    }

    #[test]
    fn oversized_delta_is_bounded_without_advancing_the_cursor_early() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let mut observation: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let template = observation.objects[0].clone();
        for index in 0..199 {
            let mut object = template.clone();
            object.object_id = format!("delta-object-{index}");
            object.name = Some(format!("Stable control {index}"));
            observation.objects.push(object);
        }
        let tab_id = observation.tab_id.clone();
        let mut views = AgentViewState::default();
        views.project(observation.clone()).unwrap();
        while views.has_pending_view(&tab_id) {
            views.continue_view(&tab_id).unwrap();
        }
        let prior_revision = observation.revision;

        observation.revision += 1;
        observation.changes.clear();
        for (index, object) in observation.objects.iter_mut().take(160).enumerate() {
            object.object_revision += 1;
            object.name = Some(format!("Changed {index} {}", "z".repeat(220)));
            observation
                .changes
                .push(saccade_protocol::ObservationChange {
                    kind: ChangeKind::Updated,
                    object_id: object.object_id.clone(),
                    object_revision: object.object_revision,
                });
        }
        let expected_revision = observation.revision;
        let first = views.project(observation).unwrap();
        assert_eq!(first["mode"], "delta");
        assert_eq!(first["page"]["item_kind"], "changes");
        assert_eq!(first["page"]["complete"], false);
        let first_change = &first["changes"][0];
        assert_eq!(first_change["kind"], "updated");
        assert!(first_change.get("object").is_none());
        assert_eq!(first_change["object_id"], "o1");
        assert!(first_change["patch"]["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("Changed 0 ")));
        assert_eq!(views.revision_for_tab(&tab_id), Some(prior_revision));
        let mut changes = first["changes"].as_array().unwrap().len();
        while views.has_pending_view(&tab_id) {
            let page = views.continue_view(&tab_id).unwrap().unwrap();
            changes += page["changes"].as_array().unwrap().len();
        }
        assert_eq!(changes, 160);
        assert_eq!(views.revision_for_tab(&tab_id), Some(expected_revision));
    }

    #[test]
    fn agent_resync_resets_only_the_named_tab_cursor() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let tab_one: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let mut tab_two = tab_one.clone();
        tab_two.tab_id = "tab.other".into();
        tab_two.document_id = "document.other".into();
        tab_two.frames[0].document_id = "document.other".into();

        let mut views = AgentViewState::default();
        assert_eq!(views.project(tab_one.clone()).unwrap()["mode"], "full");
        assert_eq!(views.project(tab_two.clone()).unwrap()["mode"], "full");

        let mut next_one = tab_one;
        next_one.revision += 1;
        next_one.changes.clear();
        let mut next_two = tab_two;
        next_two.revision += 1;
        next_two.changes.clear();

        views.reset_cursor("tab.fixture");
        assert_eq!(views.project(next_one).unwrap()["mode"], "full");
        assert_eq!(views.project(next_two).unwrap()["mode"], "delta");
    }

    #[test]
    fn verified_act_returns_same_frame_transition_and_omits_the_proven_target() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let first: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let mut views = AgentViewState::new(false);
        views.project(first.clone()).unwrap();

        let mut second = first.clone();
        second.revision = first.revision + 1;
        second.objects[0].object_revision += 1;
        second.objects[0]
            .state
            .insert("has_value".into(), "true".into());
        let mut appeared = second.objects[0].clone();
        appeared.object_id = "object.appeared".into();
        appeared.object_revision = 1;
        appeared.name = Some("Revealed field".into());
        appeared.action_token = Some("token.appeared-0123456789abcdef0123456789abcdef".into());
        second.objects.push(appeared);
        second.changes = vec![
            saccade_protocol::ObservationChange {
                kind: ChangeKind::Updated,
                object_id: "object.fixture".into(),
                object_revision: second.objects[0].object_revision,
            },
            saccade_protocol::ObservationChange {
                kind: ChangeKind::Appeared,
                object_id: "object.appeared".into(),
                object_revision: 1,
            },
        ];
        second.validate().unwrap();

        let mut result = json!({
            "verified":true,
            "verification":{"object_id":"o1","field":"has_value","before":"false","after":"true"}
        });
        attach_act_transition(&mut views, &mut result, second).unwrap();
        assert_eq!(result["transition"]["changes"][0]["kind"], "appeared");
        assert_eq!(
            result["transition"]["changes"][0]["object"]["object_id"],
            "o2"
        );
        assert!(result.get("ambient_changes_pending").is_none());
        assert!(views.take_ambient("tab.fixture", None).is_none());
        assert_eq!(
            views.revision_for_tab("tab.fixture"),
            Some(first.revision + 1)
        );
    }

    #[test]
    fn unverified_act_returns_structural_change_in_its_target_frame() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let first: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let mut views = AgentViewState::new(false);
        views.project(first.clone()).unwrap();

        let mut second = first.clone();
        second.revision += 1;
        let mut appeared = second.objects[0].clone();
        appeared.object_id = "object.appeared".into();
        appeared.object_revision = 1;
        appeared.name = Some("Result panel".into());
        appeared.action_token = None;
        second.objects.push(appeared);
        second.changes = vec![saccade_protocol::ObservationChange {
            kind: ChangeKind::Appeared,
            object_id: "object.appeared".into(),
            object_revision: 1,
        }];
        second.validate().unwrap();

        let mut result = json!({
            "object_id":"o1",
            "verified":false,
            "outcome":"accepted_but_unverified"
        });
        attach_act_transition(&mut views, &mut result, second).unwrap();
        assert_eq!(result["transition"]["changes"][0]["kind"], "appeared");
        assert_eq!(
            result["transition"]["changes"][0]["object"]["object_id"],
            "o2"
        );
    }

    #[test]
    fn act_transition_returns_same_frame_updates_for_immediate_continuation() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let mut first: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let mut ambient = first.objects[0].clone();
        ambient.object_id = "object.ambient".into();
        ambient.object_revision = 1;
        ambient.name = Some("Ambient status".into());
        ambient.action_token = Some("token.ambient-0123456789abcdef0123456789abcdef".into());
        first.objects.push(ambient);
        first.validate().unwrap();

        let mut views = AgentViewState::new(false);
        views.project(first.clone()).unwrap();
        let mut second = first.clone();
        second.revision += 1;
        second.objects[0].object_revision += 1;
        second.objects[0]
            .state
            .insert("has_value".into(), "true".into());
        second.objects[1].object_revision += 1;
        second.objects[1]
            .state
            .insert("has_value".into(), "true".into());
        second.changes = vec![
            saccade_protocol::ObservationChange {
                kind: ChangeKind::Updated,
                object_id: "object.fixture".into(),
                object_revision: second.objects[0].object_revision,
            },
            saccade_protocol::ObservationChange {
                kind: ChangeKind::Updated,
                object_id: "object.ambient".into(),
                object_revision: second.objects[1].object_revision,
            },
        ];
        second.validate().unwrap();

        let mut result = json!({
            "verified":true,
            "verification":{"object_id":"o1","field":"has_value","before":"false","after":"true"}
        });
        attach_act_transition(&mut views, &mut result, second).unwrap();
        assert_eq!(result["transition"]["mode"], "delta");
        assert_eq!(result["transition"]["changes"][0]["object_id"], "o2");
        assert!(result.get("ambient_changes_pending").is_none());
        assert!(views.take_ambient("tab.fixture", None).is_none());
    }

    #[test]
    fn semantic_query_returns_one_bounded_working_set_and_keeps_full_truth_local() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let mut observation: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let mut submit = observation.objects[0].clone();
        submit.object_id = "object.submit-0123456789abcdef0123456789abcdef".into();
        submit.object_revision = 1;
        submit.name = Some("Submit application".into());
        submit.description = None;
        observation.objects.push(submit);
        let root_frame_id = observation.frames[0].frame_id.clone();
        let mut child_frame = observation.frames[0].clone();
        child_frame.frame_id = "frame.child".into();
        child_frame.parent_frame_id = Some(root_frame_id);
        observation.frames.push(child_frame);
        let mut child_submit = observation.objects[1].clone();
        child_submit.object_id = "object.child-submit-0123456789abcdef01234567".into();
        child_submit.frame_id = "frame.child".into();
        observation.objects.push(child_submit);
        let mut views = AgentViewState::new(false);
        views.pending_ambient.insert(
            observation.tab_id.clone(),
            VecDeque::from([json!({"mode":"delta","revision":1})]),
        );
        let view = views
            .project_query(
                observation.clone(),
                &json!({
                    "text_any":["work email","submit application"],
                    "roles":["text_field"],
                    "affordances":["type"],
                    "frame_scope":"root",
                    "min_objects":2,
                    "max_objects":2
                }),
            )
            .unwrap();
        assert_eq!(view["mode"], "working_set");
        assert_eq!(view["match_count"], 2);
        assert_eq!(view["requested_min_objects"], 2);
        assert_eq!(view["settled"], true);
        assert_eq!(view["objects"][0]["object_id"], "o1");
        assert_eq!(view["objects"][0]["name"], "Email");
        assert_eq!(view["objects"][1]["object_id"], "o2");
        assert_eq!(view["objects"][1]["name"], "Submit application");
        assert_eq!(view["frame_summaries"][0]["object_count"], 2);
        assert_eq!(
            views.revision_for_tab("tab.fixture"),
            Some(observation.revision)
        );
        assert!(!views.pending_ambient.contains_key("tab.fixture"));
        let current = views
            .observation_at_or_after("tab.fixture", observation.revision)
            .expect("the delivered canonical revision satisfies a follow-up query basis");
        assert_eq!(current.revision, observation.revision);
        assert!(views
            .observation_at_or_after("tab.fixture", observation.revision + 1)
            .is_none());
    }

    #[test]
    fn actionable_query_includes_bounded_nearby_decision_context_once() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let mut observation: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let template = observation.objects[0].clone();
        let mut approve = template.clone();
        approve.object_id = "object.approve-0000000000000000000000000001".into();
        approve.role = saccade_protocol::SemanticRole::Button;
        approve.name = Some("Approve record".into());
        approve.text = None;
        approve.affordances = BTreeSet::from([saccade_protocol::Affordance::Click]);
        approve.document_bounds.y = 300.0;
        let mut reject = approve.clone();
        reject.object_id = "object.reject-00000000000000000000000000001".into();
        reject.name = Some("Reject record".into());
        reject.document_bounds.x = 180.0;
        let mut risk = template.clone();
        risk.object_id = "object.risk-000000000000000000000000000001".into();
        risk.role = saccade_protocol::SemanticRole::Paragraph;
        risk.name = None;
        risk.text = Some("Risk score: 44".into());
        risk.affordances.clear();
        risk.action_token = None;
        risk.document_bounds.y = 230.0;
        let mut evidence = risk.clone();
        evidence.object_id = "object.evidence-00000000000000000000000001".into();
        evidence.text = Some("Evidence: missing".into());
        evidence.document_bounds.y = 265.0;
        let mut unrelated = risk.clone();
        unrelated.object_id = "object.far-000000000000000000000000000001".into();
        unrelated.text = Some("Unrelated footer".into());
        unrelated.document_bounds.y = 1000.0;
        observation.objects = vec![approve, reject, risk, evidence, unrelated];
        let mut views = AgentViewState::new(false);
        let view = views
            .project_query(
                observation,
                &json!({
                    "text":"Approve record",
                    "roles":["button"],
                    "frame_scope":"root",
                    "max_objects":8
                }),
            )
            .unwrap();
        assert_eq!(view["match_count"], 1);
        assert_eq!(view["query_primary_object_ids"], json!(["o1"]));
        assert_eq!(view["context_count"], 3);
        let text = serde_json::to_string(&view["objects"]).unwrap();
        for expected in [
            "Approve record",
            "Reject record",
            "Risk score: 44",
            "Evidence: missing",
        ] {
            assert!(text.contains(expected));
        }
        assert!(!text.contains("Unrelated footer"));
    }

    #[test]
    fn semantic_query_text_any_balances_named_targets_before_filling_the_limit() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let mut observation: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let template = observation.objects[0].clone();
        observation.objects = [
            (
                "object.news-heading-00000000000000000000000001",
                "News",
                saccade_protocol::SemanticRole::Heading,
            ),
            (
                "object.guides-000000000000000000000000000001",
                "Guides",
                saccade_protocol::SemanticRole::Link,
            ),
            (
                "object.guides-archive-0000000000000000000001",
                "Guides archive",
                saccade_protocol::SemanticRole::Link,
            ),
            (
                "object.reviews-00000000000000000000000000001",
                "Reviews",
                saccade_protocol::SemanticRole::Link,
            ),
            (
                "object.news-0000000000000000000000000000001",
                "News",
                saccade_protocol::SemanticRole::Link,
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (object_id, name, role))| {
            let mut object = template.clone();
            object.object_id = object_id.into();
            object.role = role;
            object.name = Some(name.into());
            object.text = None;
            object.description = None;
            object.document_bounds.y = index as f64 * 100.0;
            object
        })
        .collect();
        let mut views = AgentViewState::new(false);
        let view = views
            .project_query(
                observation,
                &json!({
                    "text_any":["Guides","Reviews","News"],
                    "roles":["link"],
                    "frame_scope":"root",
                    "min_objects":3,
                    "max_objects":3
                }),
            )
            .unwrap();
        assert_eq!(view["match_count"], 4);
        assert_eq!(view["text_any_match_counts"], json!([2, 1, 4]));
        assert_eq!(view["objects"][0]["name"], "Guides");
        assert_eq!(view["objects"][1]["name"], "Reviews");
        assert_eq!(view["objects"][2]["name"], "News");
    }

    #[test]
    fn semantic_query_ascii_terms_use_word_boundaries() {
        assert!(query_term_matches("male", "male"));
        assert!(!query_term_matches("female", "male"));
        assert!(query_term_matches(
            "placeholder: name@example.com",
            "name@example.com"
        ));
        assert!(query_term_matches("公司邮件地址", "邮件"));
    }

    #[test]
    fn semantic_query_accepts_aria_aliases_for_select_controls() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let mut observation: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let mut heading = observation.objects[0].clone();
        heading.object_id = "heading.basic-select".into();
        heading.role = saccade_protocol::SemanticRole::Heading;
        heading.text = Some("Basic select".into());
        heading.name = None;
        heading.description = None;
        heading.action_token = None;
        heading.affordances.clear();
        heading.state.clear();
        heading.document_bounds.y = 100.0;
        let mut select = observation.objects[0].clone();
        select.object_id = "select.favorite-food".into();
        select.role = saccade_protocol::SemanticRole::Select;
        select.name = Some("Favorite food".into());
        select.description = None;
        select.document_bounds.y = 180.0;
        observation.objects = vec![heading, select];
        let mut views = AgentViewState::new(false);
        let view = views
            .project_query(
                observation.clone(),
                &json!({
                    "text":"Basic select",
                    "roles":["combobox"],
                    "frame_scope":"root",
                    "min_objects":1,
                    "max_objects":8
                }),
            )
            .unwrap();
        assert_eq!(view["match_count"], 1);
        assert_eq!(view["objects"][0]["name"], "Favorite food");

        let alias_view = views
            .project_query(
                observation,
                &json!({
                    "text":"Basic select",
                    "roles":["listbox"],
                    "frame_scope":"root",
                    "min_objects":1,
                    "max_objects":8
                }),
            )
            .unwrap();
        assert_eq!(alias_view["match_count"], 1);
        assert_eq!(alias_view["objects"][0]["name"], "Favorite food");
    }

    #[test]
    fn mcp_sessions_hide_other_agent_tabs_but_keep_shared_tabs() {
        let mut views = AgentViewState::default();
        views.record_opened_tab(&json!({"tab_id":"local-agent"}));
        let mut listed = json!({"tabs":[
            {"tab_id":"local-agent","ownership":"agent","provenance":"saccade_tabs_open"},
            {"tab_id":"other-agent","ownership":"agent","provenance":"saccade_tabs_open"},
            {"tab_id":"shared","ownership":"user_shared","provenance":"user_shared"}
        ]});
        views.project_session_tabs(&mut listed).unwrap();
        assert_eq!(listed["session_scoped"], true);
        assert_eq!(listed["tabs"].as_array().unwrap().len(), 2);
        assert_eq!(listed["tabs"][0]["tab_id"], "local-agent");
        assert_eq!(listed["tabs"][1]["tab_id"], "shared");
        assert!(views.require_session_tab("local-agent").is_ok());
        assert!(views.require_session_tab("shared").is_ok());
        assert!(views.require_session_tab("other-agent").is_err());
        views.forget_session_tab("local-agent");
        assert!(views.require_session_tab("local-agent").is_err());
    }

    #[test]
    fn select_option_alias_is_expanded_before_host_validation() {
        let aliases = AgentObjectAliases {
            document_id: "document-1".into(),
            by_internal: BTreeMap::from([("internal-option".into(), "o7".into())]),
            by_alias: BTreeMap::from([("o7".into(), "internal-option".into())]),
            next: 8,
        };
        let mut action = json!({"operation":"select","option_object_id":"o7"});
        expand_compact_option_alias(&mut action, &aliases).unwrap();
        assert_eq!(action["option_object_id"], "internal-option");
        let mut stale = json!({"operation":"select","option_object_id":"o999"});
        assert!(expand_compact_option_alias(&mut stale, &aliases).is_err());
    }

    #[test]
    fn public_act_batch_expands_each_agent_object_alias() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let observation: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let internal = observation.objects[0].object_id.clone();
        let mut views = AgentViewState::default();
        views.project(observation.clone()).unwrap();
        let mut arguments = json!({
            "tab_id":observation.tab_id,
            "document_id":observation.document_id,
            "basis_revision":observation.revision,
            "actions":[{"object_id":"o1","operation":"type","value":"ordinary"}]
        });
        views
            .expand_object_aliases("web.act_object", &mut arguments)
            .unwrap();
        assert_eq!(arguments["actions"][0]["object_id"], internal);
    }

    #[test]
    fn public_act_compiles_current_truth_affordances_without_model_restatement() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let mut observation: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let mut button = observation.objects[0].clone();
        button.object_id = "internal-button".into();
        button.role = saccade_protocol::SemanticRole::Button;
        button.name = Some("Continue".into());
        button.affordances = BTreeSet::from([Affordance::Click]);
        let mut select = observation.objects[0].clone();
        select.object_id = "internal-select".into();
        select.role = saccade_protocol::SemanticRole::Select;
        select.name = Some("Country".into());
        select.affordances = BTreeSet::from([Affordance::Select]);
        let mut option = observation.objects[0].clone();
        option.object_id = "internal-option".into();
        option.role = saccade_protocol::SemanticRole::Option;
        option.name = Some("United States".into());
        option.affordances = BTreeSet::new();
        let mut delayed_button = observation.objects[0].clone();
        delayed_button.object_id = "internal-delayed-button".into();
        delayed_button.role = saccade_protocol::SemanticRole::Button;
        delayed_button.name = Some("Available shortly".into());
        delayed_button.affordances = BTreeSet::new();
        observation
            .objects
            .extend([button, select, option, delayed_button]);

        let tab_id = observation.tab_id.clone();
        let document_id = observation.document_id.clone();
        let revision = observation.revision;
        let canonical = observation.clone();
        let mut views = AgentViewState::default();
        views.project(observation).unwrap();

        let mut click = json!({
            "tab_id":tab_id,
            "document_id":document_id,
            "basis_revision":revision,
            "object_id":"o2"
        });
        views.infer_public_act_operations(&mut click).unwrap();
        assert_eq!(click["operation"], "click");

        let mut type_action = json!({
            "tab_id":tab_id,
            "document_id":document_id,
            "basis_revision":revision,
            "object_id":"o1",
            "value":"ordinary"
        });
        views.infer_public_act_operations(&mut type_action).unwrap();
        assert_eq!(type_action["operation"], "type");
        assert_eq!(type_action["text"], "ordinary");
        assert!(type_action.get("value").is_none());

        let mut select_action = json!({
            "tab_id":tab_id,
            "document_id":document_id,
            "basis_revision":revision,
            "object_id":"o3",
            "option_object_id":"o4"
        });
        views
            .infer_public_act_operations(&mut select_action)
            .unwrap();
        assert_eq!(select_action["operation"], "select");

        let mut ambiguous = json!({
            "tab_id":tab_id,
            "document_id":document_id,
            "basis_revision":revision,
            "object_id":"o1"
        });
        assert!(views
            .infer_public_act_operations(&mut ambiguous)
            .unwrap_err()
            .to_string()
            .contains("multiple action affordances"));

        let mut explicit_delayed_click = json!({
            "tab_id":tab_id,
            "document_id":document_id,
            "basis_revision":revision,
            "object_id":"o5",
            "operation":"click"
        });
        views
            .infer_public_act_operations(&mut explicit_delayed_click)
            .unwrap();
        assert_eq!(explicit_delayed_click["operation"], "click");

        let mut inferred_delayed_click = json!({
            "tab_id":tab_id,
            "document_id":document_id,
            "basis_revision":revision,
            "object_id":"o5"
        });
        assert!(views
            .infer_public_act_operations(&mut inferred_delayed_click)
            .unwrap_err()
            .to_string()
            .contains("not currently actionable"));

        views.reset_cursor(&tab_id);
        assert!(views.action_context_missing(&tab_id, &document_id).unwrap());
        views
            .restore_action_context(canonical, &document_id)
            .unwrap();
        let mut recovered_click = json!({
            "tab_id":tab_id,
            "document_id":document_id,
            "basis_revision":revision,
            "object_id":"o2"
        });
        views
            .infer_public_act_operations(&mut recovered_click)
            .unwrap();
        assert_eq!(recovered_click["operation"], "click");
    }

    #[test]
    fn compact_actions_hydrate_current_identity_and_form_check() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let observation: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let browser_instance_id = observation.browser_instance_id.clone();
        let document_id = observation.document_id.clone();
        let action_token = observation.objects[0].action_token.clone().unwrap();
        let mut views = AgentViewState::default();
        views.project(observation).unwrap();

        let hydrated = views
            .hydrate_action_arguments(
                "web.act",
                json!({
                    "action_token":action_token.clone(),
                    "operation":"click"
                }),
            )
            .unwrap();
        assert_eq!(hydrated["browser_instance_id"], browser_instance_id);
        assert_eq!(hydrated["document_id"], document_id);
        assert_eq!(hydrated["payload"], json!({"kind":"none"}));

        let form = views
            .hydrate_action_arguments(
                "web.form.fill",
                json!({
                    "actions":[{
                        "action_token":action_token.clone(),
                        "operation":"check"
                    }]
                }),
            )
            .unwrap();
        assert_eq!(form["actions"][0]["operation"], "click");
        assert_eq!(form["actions"][0]["payload"], json!({"kind":"none"}));

        assert!(views
            .hydrate_action_arguments(
                "web.act",
                json!({
                    "action_token":"token.0123456789abcdef0123456789abcdef",
                    "operation":"click"
                })
            )
            .is_err());

        let mut other: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        other.tab_id = "other-tab".into();
        other.document_id = "other-document".into();
        other.objects[0].action_token = Some("token.other.0123456789abcdef0123456789abcdef".into());
        views.project(other).unwrap();
        assert!(views
            .hydrate_action_arguments(
                "web.form.fill",
                json!({
                    "actions":[
                        {"action_token":action_token,"operation":"check"},
                        {"action_token":"token.other.0123456789abcdef0123456789abcdef","operation":"check"}
                    ]
                })
            )
            .is_err());
    }

    #[test]
    fn act_result_summary_surfaces_bounded_public_transition_signals() {
        let result = json!({
            "verified": true,
            "transition": {
                "revision": 17,
                "changes": [
                    {"object": {"role": "status", "text": "QUEUE-PROOF-ABC123"}},
                    {"patch": {"name": "Review complete"}},
                    {"object": {"text": "third signal must not be copied"}}
                ]
            }
        });
        let summary = tool_result_summary(&result);
        assert!(summary.contains("verified=true"));
        assert!(summary.contains("revision=17"));
        assert!(summary.contains("transition_changes=3"));
        assert!(summary.contains("follow_up_read_required=false"));
        assert!(summary.contains("QUEUE-PROOF-ABC123"));
        assert!(summary.contains("Review complete"));
        assert!(!summary.contains("third signal"));
    }
}
