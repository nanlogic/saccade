//! MCP adapter and per-Agent Browser projection over the single HostClient interface.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::browser_wake;
use crate::profile::Profile;
use anyhow::{anyhow, bail, Context, Result};
use saccade_host_client::HostClient;
use saccade_protocol::{ActionReceipt, ChangeKind, ObservationSnapshot, ProtocolError};
use serde::Deserialize;
use serde_json::{json, Value};

const MCP_VERSION: &str = "2025-03-26";
const ECONOMY_COALESCE_WINDOW: Duration = Duration::from_millis(150);
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum TruthViewMode {
    Auto,
    Full,
    Index,
    Region {
        region_id: String,
        document_id: String,
        basis_revision: u64,
    },
}

impl TruthViewMode {
    fn from_arguments(arguments: &mut Value) -> Result<Self> {
        let object = arguments
            .as_object_mut()
            .context("tool arguments must be an object")?;
        let mode = object
            .remove("view_mode")
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "auto".into());
        let region_id = object.remove("region_id");
        let document_id = object.remove("document_id");
        let basis_revision = object.remove("basis_revision");
        match mode.as_str() {
            "auto" if region_id.is_none() && document_id.is_none() && basis_revision.is_none() => {
                Ok(Self::Auto)
            }
            "full" if region_id.is_none() && document_id.is_none() && basis_revision.is_none() => {
                Ok(Self::Full)
            }
            "index" if region_id.is_none() && document_id.is_none() && basis_revision.is_none() => {
                Ok(Self::Index)
            }
            "region" => Ok(Self::Region {
                region_id: region_id
                    .as_ref()
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .context("region view requires region_id")?
                    .to_string(),
                document_id: document_id
                    .as_ref()
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .context("region view requires document_id")?
                    .to_string(),
                basis_revision: basis_revision
                    .as_ref()
                    .and_then(Value::as_u64)
                    .filter(|value| *value > 0)
                    .context("region view requires a positive basis_revision")?,
            }),
            "auto" | "full" | "index" => {
                bail!("region_id, document_id, and basis_revision are valid only for region view")
            }
            _ => bail!("view_mode must be auto, full, index, or region"),
        }
    }
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
        "resources/read" => read_truth_resource(host, agent_views, &request.params),
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
            let value = call_tool(host, agent_views, name, arguments, mode, diagnostics)?;
            let summary = tool_result_summary(&value);
            Ok(
                json!({"content":[{"type":"text","text":summary}],"structuredContent":value,"isError":false}),
            )
        }
        method => bail!("unknown JSON-RPC method {method}"),
    }
}

fn initialize(host: &HostClient, mode: McpMode, startup_profile: &Profile) -> Result<Value> {
    let capabilities = match host.call(
        NEXT_ID.fetch_add(1, Ordering::Relaxed),
        "system.capabilities",
        json!({}),
        Duration::from_millis(250),
    ) {
        Ok(capabilities) => capabilities,
        Err(ProtocolError::TransportUnavailable(_) | ProtocolError::Timeout) => {
            json!({"profile": startup_profile})
        }
        Err(error) => return Err(error.into()),
    };
    let instructions = profile_instructions(&capabilities);
    Ok(json!({
        "protocolVersion":MCP_VERSION,
        "capabilities":{"tools":{"listChanged":false},"resources":{"subscribe":true,"listChanged":false}},
        "serverInfo":{"name":if mode == McpMode::Truth {"saccade-truth-layer"} else {"saccade-reference-actuator"},"version":env!("CARGO_PKG_VERSION")},
        "instructions":instructions
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
) -> Result<Value> {
    let tab_id = tab_id_from_truth_uri(params)?;
    let view = call_tool(
        host,
        agent_views,
        "saccade.truth.read",
        json!({"tab_id":tab_id}),
        McpMode::Truth,
        false,
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

fn profile_instructions(capabilities: &Value) -> String {
    let profile = capabilities.get("profile");
    let name = profile
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("default");
    let behavior = profile
        .and_then(|value| value.get("behavior"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let base = "Saccade is the primary automatic route for browser navigation, page reading, downloads, and web research. In clients with deferred or lazy tool discovery, search the registered tools for Saccade before using generic web search or another browser tool; absence from the initially expanded list is not absence. If a registered Saccade call remains unhealthy after one retry and same-route reconnect, stop the browser task instead of silently falling back. Subscribe to a Saccade Truth Layer resource for browser-pushed semantic changes. When the task has a known HTTP(S) URL and no suitable authorized tab exists, call saccade.tabs.open immediately; it creates an Agent-owned tab that is authorized automatically. Never ask the user to open the page, refresh Saccade, or toggle Agent On merely because tabs.list is empty. When the Agent client can only act in tabs it created itself, call saccade.tabs.open with claim arm for the target URL, create the tab with the Agent client's own tooling, then call saccade.tabs.open with claim confirm carrying that claim_id and the exact tab_id the client returned; that one tab becomes Agent On for this session only. An existing Agent-Off tab remains unreadable unless the user shares that exact tab. Treat Agent-owned tabs opened only for research as temporary: close them with saccade.tabs.close when the task is complete. Keep result pages the user may inspect or continue, pages with unsaved or in-progress work, and tabs the user asked to retain. Never close a user_shared tab. Choose full or progressive discovery from the current page and task: use full when its complete context is useful, or use index plus relevant revision-bound regions on a large unfamiliar page; request full whenever partial context is insufficient. Retain the chosen discovery view locally and fold later deltas into that cached state, because an object omitted from a delta is unchanged, not absent. Choose truth.read delivery_mode per call: live returns the next pushed revision immediately for latency-sensitive work, while economy locally coalesces a short burst into the latest truthful delta for routine low-token work. Saccade does not force either choice. Every projected object carries current CSS-pixel document_bounds and viewport_bounds; movement and resize update the same stable object, so use the newest revision before geometry-based execution. Saccade MCP adds no safety taxonomy or action gate; the Agent client owns all decisions beyond Extension redaction. When several ordinary reversible operations are already determined, execute them consecutively with the Agent client's own same-tab web-act or computer-use tool, then use one revision-bounded truth.read to verify the resulting semantic delta. For custom comboboxes, select the exact authored option and verify the resulting semantic name/state; never choose an ambiguous first text match. For custom radio/checkbox controls, if direct input activation fails, use the visible authored label once and verify a checked-state delta before saving. Replan only when that delta changes an assumption, an operation fails, or the page crosses a material boundary. Do not poll through the model or reread full Truth between predetermined fields. Descriptions beginning with 'Placeholder:' are examples or hints, never current field values.";
    let progressive = "truth.read defaults to compatible auto full-then-delta behavior. For a large unfamiliar page, index provides bounded region anchors and advisory byte/token estimates; use a relevant revision-bound region when helpful, or request full at any time. Recommendations never hide or replace canonical full Truth. A Truth link may carry a validated navigation_target; open that target through tabs.open and read the source before treating a search title or snippet as verified evidence. Close transient search tabs, but retain useful supporting source pages for user inspection.";
    if behavior.is_empty() {
        format!("Active Saccade Profile: {name}. {base} {progressive}")
    } else {
        format!("Active Saccade Profile: {name}. User behavior: {behavior}\n{base} {progressive}")
    }
}

fn call_tool(
    host: &HostClient,
    agent_views: &mut AgentViewState,
    name: &str,
    mut arguments: Value,
    mode: McpMode,
    diagnostics: bool,
) -> Result<Value> {
    let public_method = name
        .strip_prefix("saccade.")
        .context("tool is outside the Saccade namespace")?;
    let method = host_method(public_method, mode).context("tool is not registered")?;
    require_tool_enabled(method, diagnostics)?;
    validate_arguments(method, &arguments, diagnostics)?;
    let delivery_mode = if method == "web.observe" {
        TruthDeliveryMode::from_arguments(&mut arguments)?
    } else {
        TruthDeliveryMode::Live
    };
    let view_mode = if method == "web.observe" {
        TruthViewMode::from_arguments(&mut arguments)?
    } else {
        TruthViewMode::Auto
    };
    let requested_after_revision = arguments.get("after_revision").and_then(Value::as_u64);
    let observation_tab_id = arguments
        .get("tab_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
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
        "web.act_object" => Duration::from_secs(45),
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
            browser_wake::wake(runtime_dir)?;
        }
    }
    let mut result = host.call(
        NEXT_ID.fetch_add(1, Ordering::Relaxed),
        method,
        arguments,
        timeout,
    )?;
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
            agent_views.collapse_object_aliases(&mut result);
            return Ok(result);
        }
        "system.capabilities" => {
            result["reference_actuator_active"] = Value::Bool(mode == McpMode::Reference);
            result["truth"]["delivery_modes"] = json!({
                "default":"live",
                "available":["live","economy"],
                "economy_coalesce_ms":ECONOMY_COALESCE_WINDOW.as_millis()
            });
            result["truth"]["view_modes"] = json!({
                "default":"auto",
                "available":["auto","full","index","region"],
                "full_always_available":true,
                "recommendations":"advisory",
                "region_basis":["document_id","revision"]
            });
            return Ok(result);
        }
        "web.observe" => {
            let observation = serde_json::from_value::<ObservationSnapshot>(result)?;
            observation.validate()?;
            let mut view = agent_views.project_with_mode(observation, &view_mode)?;
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
                "view_mode",
                "region_id",
                "document_id",
                "basis_revision",
            ],
            &["tab_id"],
        ),
        "web.act_object" => (
            &[
                "tab_id",
                "object_id",
                "operation",
                "document_id",
                "basis_revision",
                "option_object_id",
                "timeout_ms",
            ],
            &[
                "tab_id",
                "object_id",
                "operation",
                "document_id",
                "basis_revision",
            ],
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
            if value.get("view_mode").is_some_and(|mode| {
                !matches!(mode.as_str(), Some("auto" | "full" | "index" | "region"))
            }) {
                bail!("view_mode must be auto, full, index, or region");
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
    let mut tools = vec![
        json!({"name":"saccade.system.capabilities","description":"Discover and verify the primary Saccade route before browser navigation, page reading, downloads, or web research. Read the active Profile behavior and Runtime capabilities.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"saccade.tabs.list","description":"Discover authorized Chrome or Edge tabs managed by Saccade before web research or navigation.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"saccade.tabs.open","description":"Primary browser-navigation route: open an HTTP or HTTPS page, including a validated navigation_target from a current Truth link, in a tab managed by Saccade. Omit claim to have Saccade create and authorize the tab. When the Agent client must create the tab with its own browser tooling, call claim \"arm\" first for the target URL; the reply carries a short-lived single-use claim_id bound to that origin and creates, reads, and authorizes nothing. Then create the tab with the Agent client's own tooling and call claim \"confirm\" with that claim_id and the exact tab_id the client received. Only the first new tab matching the armed origin inside the claim window can be confirmed; any mismatch fails uniformly and consumes the claim. Runtime validation requires claim_id and tab_id for confirm, forbids them for other modes, and forbids active for every claim mode.","inputSchema":{"type":"object","properties":{"url":{"type":"string","minLength":1,"maxLength":8192},"active":{"type":"boolean"},"claim":{"type":"string","enum":["arm","confirm"]},"claim_id":{"type":"string","minLength":1},"tab_id":{"type":"string","minLength":1}},"required":["url"],"additionalProperties":false}}),
        json!({"name":"saccade.tabs.close","description":"Close a tab created by the Agent through Saccade. User-shared tabs cannot be closed by this tool.","inputSchema":{"type":"object","properties":{"tab_id":{"type":"string","minLength":1}},"required":["tab_id"],"additionalProperties":false}}),
        json!({"name":"saccade.truth.read","description":"Primary page-reading route for canonical Saccade Truth with an Agent-selected view. auto preserves compatible full-then-delta behavior; full always returns the complete current page; index returns bounded region anchors and honest cost metadata; region returns one revision-bound region. Recommendations are advisory and full Truth always remains available. Pass after_revision to wait locally instead of polling. Choose live for immediate delivery or economy for bounded coalescing.","inputSchema":{"type":"object","properties":{"tab_id":{"type":"string","minLength":1},"after_revision":{"type":"integer","minimum":0},"timeout_ms":{"type":"integer","minimum":1,"maximum":30000},"delivery_mode":{"type":"string","enum":["live","economy"],"default":"live"},"view_mode":{"type":"string","enum":["auto","full","index","region"],"default":"auto"},"region_id":{"type":"string","minLength":1},"document_id":{"type":"string","minLength":1},"basis_revision":{"type":"integer","minimum":1}},"required":["tab_id"],"additionalProperties":false}}),
        json!({"name":"saccade.act","description":"Operate one semantic object from current Truth using Saccade's registered software input. Name the object_id from truth.read; never a coordinate, and never a screenshot. The action token stays inside the Runtime. Pass the document_id and basis_revision the object was read at: a stale basis is refused rather than replayed. Returns dispatch accepted_by_software with verified true and a before/after semantic field when Saccade can prove the effect, accepted_but_unverified when the role has no defined evidence, or external_execution_required when the control is not registered for software input, in which case the Agent client performs the action itself. A revision increase alone is never treated as success.","inputSchema":{"type":"object","properties":{"tab_id":{"type":"string","minLength":1},"object_id":{"type":"string","minLength":1},"operation":{"type":"string","enum":["click","select"]},"document_id":{"type":"string","minLength":1},"basis_revision":{"type":"integer","minimum":1},"option_object_id":{"type":"string","minLength":1},"timeout_ms":{"type":"integer","minimum":1,"maximum":30000}},"required":["tab_id","object_id","operation","document_id","basis_revision"],"additionalProperties":false}}),
    ];
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

fn require_tool_enabled(method: &str, diagnostics: bool) -> Result<()> {
    if matches!(method, "web.act_native" | "web.act_soft") && !diagnostics {
        bail!("diagnostic input overrides are disabled");
    }
    Ok(())
}

#[derive(Default)]
struct AgentViewState {
    tabs: BTreeMap<String, ObservationSnapshot>,
    aliases: BTreeMap<String, AgentObjectAliases>,
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

    fn document_for_tab(&self, tab_id: &str) -> Option<&str> {
        self.tabs
            .get(tab_id)
            .map(|observation| observation.document_id.as_str())
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
            for field in ["object_id", "option_object_id"] {
                let Some(alias) = arguments.get(field).and_then(Value::as_str) else {
                    continue;
                };
                let internal = aliases.by_alias.get(alias).with_context(|| {
                    format!("unknown {field} {alias}; call saccade.truth.read for tab {tab_id} first")
                })?;
                arguments[field] = Value::String(internal.clone());
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

    fn project_with_mode(
        &mut self,
        observation: ObservationSnapshot,
        mode: &TruthViewMode,
    ) -> Result<Value> {
        let current = observation.clone();
        let auto = self.project(observation)?;
        let aliases = self
            .aliases
            .get(&current.tab_id)
            .filter(|aliases| aliases.document_id == current.document_id)
            .context("Agent object aliases are unavailable for the current document")?
            .by_internal
            .clone();
        let view = match mode {
            TruthViewMode::Auto => auto,
            TruthViewMode::Full => {
                full_agent_view(current.clone(), &aliases, self.include_authority)?
            }
            TruthViewMode::Index => agent_index_view(&current)?,
            TruthViewMode::Region {
                region_id,
                document_id,
                basis_revision,
            } => agent_region_view(
                &current,
                &aliases,
                self.include_authority,
                region_id,
                document_id,
                *basis_revision,
            )?,
        };
        attach_truth_cost(view, &current, &aliases, self.include_authority)
    }

    fn project(&mut self, observation: ObservationSnapshot) -> Result<Value> {
        let aliases = self.aliases_for(&observation);
        let previous = self
            .tabs
            .insert(observation.tab_id.clone(), observation.clone());
        let Some(previous) = previous else {
            return full_agent_view(observation, &aliases, self.include_authority);
        };
        if previous.document_id != observation.document_id || observation.gap {
            return full_agent_view(observation, &aliases, self.include_authority);
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
                    let kind = if change.kind == ChangeKind::Appeared {
                        "appeared"
                    } else {
                        "updated"
                    };
                    changes.push(json!({"kind":kind,"object":agent_object_value(object, current_default_frame.as_deref(), &aliases, self.include_authority)?}));
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

        let population = previous.objects.len().max(observation.objects.len());
        if changes.len() > 100 || (population > 20 && changes.len() * 2 > population) {
            return full_agent_view(observation, &aliases, self.include_authority);
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
            "changes":changes,
            "authorities":authorities,
            "frames":frames_changed.then_some(observation.frames),
            "coverage":observation.coverage,
            "limitations":observation.limitations,
            "gap":false
        });
        if !self.include_authority {
            view.as_object_mut()
                .expect("delta view must be an object")
                .remove("authorities");
        }
        Ok(view)
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

const REGION_OBJECT_TARGET: usize = 128;
const LARGE_FULL_VIEW_BYTES: usize = 24_000;

#[derive(Debug)]
struct AgentRegion {
    id: String,
    frame_id: String,
    object_indices: Vec<usize>,
}

fn agent_regions(observation: &ObservationSnapshot) -> Vec<AgentRegion> {
    let mut regions = Vec::new();
    for (frame_index, frame) in observation.frames.iter().enumerate() {
        let mut indices = observation
            .objects
            .iter()
            .enumerate()
            .filter(|(_, object)| object.frame_id == frame.frame_id)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        indices.sort_by(|left, right| {
            let left = &observation.objects[*left];
            let right = &observation.objects[*right];
            left.document_bounds
                .y
                .total_cmp(&right.document_bounds.y)
                .then_with(|| left.document_bounds.x.total_cmp(&right.document_bounds.x))
                .then_with(|| left.object_id.cmp(&right.object_id))
        });
        for (chunk_index, chunk) in indices.chunks(REGION_OBJECT_TARGET).enumerate() {
            regions.push(AgentRegion {
                id: format!("r{frame_index}-{chunk_index}"),
                frame_id: frame.frame_id.clone(),
                object_indices: chunk.to_vec(),
            });
        }
    }
    regions
}

fn role_name(role: saccade_protocol::SemanticRole) -> Result<String> {
    serde_json::to_value(role)?
        .as_str()
        .map(str::to_owned)
        .context("semantic role did not serialize as a string")
}

fn bounded_anchor(text: &str) -> String {
    text.chars().take(96).collect()
}

fn role_counts<'a>(
    objects: impl Iterator<Item = &'a saccade_protocol::ObservedObject>,
) -> Result<Value> {
    let mut counts = BTreeMap::<String, u64>::new();
    for object in objects {
        *counts.entry(role_name(object.role)?).or_default() += 1;
    }
    Ok(serde_json::to_value(counts)?)
}

fn region_descriptor(region: &AgentRegion, observation: &ObservationSnapshot) -> Result<Value> {
    let objects = region
        .object_indices
        .iter()
        .map(|index| &observation.objects[*index])
        .collect::<Vec<_>>();
    let mut preferred = objects
        .iter()
        .copied()
        .filter(|object| {
            matches!(
                object.role,
                saccade_protocol::SemanticRole::Heading
                    | saccade_protocol::SemanticRole::Label
                    | saccade_protocol::SemanticRole::Alert
                    | saccade_protocol::SemanticRole::Status
            )
        })
        .collect::<Vec<_>>();
    preferred.extend(objects.iter().copied().filter(|object| {
        !matches!(
            object.role,
            saccade_protocol::SemanticRole::Heading
                | saccade_protocol::SemanticRole::Label
                | saccade_protocol::SemanticRole::Alert
                | saccade_protocol::SemanticRole::Status
        )
    }));
    let mut seen = std::collections::BTreeSet::new();
    let anchors = preferred
        .into_iter()
        .filter_map(|object| {
            let text = object
                .name
                .as_deref()
                .or(object.text.as_deref())
                .or(object.description.as_deref())?;
            let text = bounded_anchor(text);
            if text.is_empty() || !seen.insert(text.clone()) {
                return None;
            }
            Some(json!({"role":role_name(object.role).ok()?,"text":text}))
        })
        .take(5)
        .collect::<Vec<_>>();
    let min_y = objects
        .iter()
        .map(|object| object.document_bounds.y)
        .reduce(f64::min)
        .unwrap_or(0.0);
    let max_y = objects
        .iter()
        .map(|object| object.document_bounds.y + object.document_bounds.height)
        .reduce(f64::max)
        .unwrap_or(min_y);
    Ok(json!({
        "region_id":region.id,
        "frame_id":region.frame_id,
        "document_y":{"start":min_y,"end":max_y},
        "object_count":objects.len(),
        "role_counts":role_counts(objects.iter().copied())?,
        "anchors":anchors
    }))
}

fn agent_index_view(observation: &ObservationSnapshot) -> Result<Value> {
    let regions = agent_regions(observation);
    let descriptors = regions
        .iter()
        .map(|region| region_descriptor(region, observation))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({
        "schema":"saccade.agent-truth-index/1",
        "mode":"index",
        "canonical_full_available":true,
        "browser_instance_id":observation.browser_instance_id,
        "tab_id":observation.tab_id,
        "document_id":observation.document_id,
        "revision":observation.revision,
        "viewport_revision":observation.viewport_revision,
            "geometry":observation.geometry,
        "object_count":observation.objects.len(),
        "role_counts":role_counts(observation.objects.iter())?,
        "regions":descriptors,
        "coverage":observation.coverage,
        "limitations":observation.limitations,
        "gap":observation.gap
    }))
}

fn agent_region_view(
    observation: &ObservationSnapshot,
    aliases: &BTreeMap<String, String>,
    include_authority: bool,
    region_id: &str,
    document_id: &str,
    basis_revision: u64,
) -> Result<Value> {
    if observation.document_id != document_id || observation.revision != basis_revision {
        bail!("region basis is stale; read a fresh index or full view")
    }
    let regions = agent_regions(observation);
    let region = regions
        .iter()
        .find(|region| region.id == region_id)
        .context("region_id is absent from the current index")?;
    let default_frame = default_frame_id(observation);
    let objects = region
        .object_indices
        .iter()
        .map(|index| {
            agent_object_value(
                &observation.objects[*index],
                default_frame.as_deref(),
                aliases,
                include_authority,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({
        "schema":"saccade.agent-region-view/1",
        "mode":"region",
        "partial":true,
        "canonical_full_available":true,
        "browser_instance_id":observation.browser_instance_id,
        "tab_id":observation.tab_id,
        "document_id":observation.document_id,
        "revision":observation.revision,
        "viewport_revision":observation.viewport_revision,
            "geometry":observation.geometry,
        "object_defaults":agent_object_defaults(default_frame.as_deref()),
        "region":region_descriptor(region, observation)?,
        "objects":objects,
        "coverage":observation.coverage,
        "limitations":observation.limitations,
        "gap":observation.gap
    }))
}

fn attach_truth_cost(
    mut view: Value,
    observation: &ObservationSnapshot,
    aliases: &BTreeMap<String, String>,
    include_authority: bool,
) -> Result<Value> {
    let full = full_agent_view(observation.clone(), aliases, include_authority)?;
    let full_bytes = serde_json::to_vec(&full)?.len();
    let view_bytes = serde_json::to_vec(&view)?.len();
    let region_count = agent_regions(observation).len();
    let mode = view.get("mode").and_then(Value::as_str).unwrap_or("full");
    let recommended_view = match mode {
        "delta" => "delta",
        "index" if full_bytes > LARGE_FULL_VIEW_BYTES && region_count > 1 => "region",
        "region" => "region",
        _ if full_bytes > LARGE_FULL_VIEW_BYTES && region_count > 1 => "index",
        _ => "full",
    };
    view["cost"] = json!({
        "advisory":true,
        "full_bytes":full_bytes,
        "full_estimated_tokens":full_bytes.div_ceil(4),
        "view_bytes_before_cost_metadata":view_bytes,
        "view_estimated_tokens_before_cost_metadata":view_bytes.div_ceil(4),
        "object_count":observation.objects.len(),
        "region_count":region_count,
        "recommended_view":recommended_view
    });
    Ok(view)
}

fn default_frame_id(observation: &ObservationSnapshot) -> Option<String> {
    (observation.frames.len() == 1).then(|| observation.frames[0].frame_id.clone())
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
        assert!(response["instructions"]
            .as_str()
            .unwrap()
            .contains("聪明的野蛮人 CEO"));
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
        for forbidden in ["x", "y", "coordinate", "screen_bounds", "action_token"] {
            assert!(
                !properties.contains_key(forbidden),
                "saccade.act must not accept {forbidden}"
            );
        }
        // Only what the Extension's software pipe actually implements.
        assert_eq!(
            schema["properties"]["operation"]["enum"],
            serde_json::json!(["click", "select"])
        );
        // Stale replay protection is not optional.
        let required = schema["required"].as_array().expect("required");
        for key in ["tab_id", "object_id", "operation", "document_id", "basis_revision"] {
            assert!(
                required.iter().any(|value| value == key),
                "{key} must be required"
            );
        }
        assert_eq!(host_method("act", McpMode::Truth), Some("web.act_object"));
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
            assert!(table.contains(role), "role {role} must have defined evidence");
            assert!(table.contains(field), "field {field} must be named");
        }
        // A revision bump alone, or an unrelated object changing, is never proof.
        let body = act_object_body(source);
        assert!(body.contains("target semantic state did not change"));
        assert!(body.contains("accepted_but_unverified"));
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
            truth_read["inputSchema"]["properties"]["view_mode"]["enum"],
            json!(["auto", "full", "index", "region"])
        );
        assert!(truth_tools
            .iter()
            .find(|tool| tool["name"] == "saccade.system.capabilities")
            .unwrap()["description"]
            .as_str()
            .unwrap()
            .contains("before browser navigation"));
        let tabs_open = truth_tools
            .iter()
            .find(|tool| tool["name"] == "saccade.tabs.open")
            .unwrap();
        assert!(tabs_open["description"]
            .as_str()
            .unwrap()
            .contains("Primary browser-navigation route"));
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
            .contains("Runtime validation requires claim_id and tab_id"));
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
            .contains("Primary page-reading route"));
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
        assert!(validate_arguments(
            "web.observe",
            &json!({"tab_id":"x","delivery_mode":"forced"}),
            false
        )
        .is_err());
        let mut economy = json!({"tab_id":"x","delivery_mode":"economy"});
        assert_eq!(
            TruthDeliveryMode::from_arguments(&mut economy).unwrap(),
            TruthDeliveryMode::Economy
        );
        assert!(economy.get("delivery_mode").is_none());
        let mut region = json!({
            "tab_id":"x",
            "view_mode":"region",
            "region_id":"r0-0",
            "document_id":"document-1",
            "basis_revision":4
        });
        assert_eq!(
            TruthViewMode::from_arguments(&mut region).unwrap(),
            TruthViewMode::Region {
                region_id: "r0-0".into(),
                document_id: "document-1".into(),
                basis_revision: 4
            }
        );
        assert!(region.get("view_mode").is_none());
        assert!(TruthViewMode::from_arguments(
            &mut json!({"tab_id":"x","view_mode":"region","region_id":"r0-0"})
        )
        .is_err());
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
        let instructions = profile_instructions(
            &json!({"profile":{"name":"focused","behavior":"Work in page order."}}),
        );
        assert!(instructions
            .starts_with("Active Saccade Profile: focused. User behavior: Work in page order."));
        assert!(instructions.contains("fold later deltas into that cached state"));
        assert!(instructions.contains("deferred or lazy tool discovery"));
        assert!(instructions.contains("instead of silently falling back"));
        assert!(instructions.contains("call saccade.tabs.open immediately"));
        assert!(instructions.contains("Never ask the user to open the page"));
        assert!(instructions.contains("Agent-Off tab remains unreadable"));
        assert!(instructions.contains("close them with saccade.tabs.close"));
        assert!(instructions.contains("Never close a user_shared tab"));
        assert!(instructions.contains("Choose full or progressive discovery"));
        assert!(instructions.contains("request full whenever partial context is insufficient"));
        assert!(!instructions.contains("Read one full view to plan"));
        assert!(instructions.contains("MCP adds no safety taxonomy or action gate"));
        assert!(instructions.contains("document_bounds and viewport_bounds"));
        assert!(instructions.contains("Placeholder:"));
        assert!(instructions.contains("index provides bounded region anchors"));
        assert!(instructions.contains("Recommendations never hide"));
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
        assert_eq!(
            geometry_delta["changes"][0]["object"]["document_bounds"]["x"],
            25.0
        );
        assert_eq!(
            geometry_delta["changes"][0]["object"]["viewport_bounds"]["x"],
            25.0
        );

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
        assert_eq!(unavailable["changes"].as_array().unwrap().len(), 1);
        assert_eq!(unavailable["changes"][0]["kind"], "updated");
        assert!(unavailable["changes"][0]["object"]
            .get("action_token")
            .is_none());

        let mut reference = AgentViewState::new(true);
        let reference_full = reference.project(first.clone()).unwrap();
        assert!(reference_full["objects"][0].get("action_token").is_some());
    }

    #[test]
    fn progressive_truth_index_and_regions_are_advisory_complete_and_revision_bound() {
        let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
        let mut observation: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
        let template = observation.objects[0].clone();
        observation.objects.clear();
        observation.changes.clear();
        for index in 0..140 {
            let mut object = template.clone();
            object.object_id = format!("object-{index}");
            object.action_token = None;
            object.name = Some(if index == 0 {
                "Account settings".into()
            } else {
                format!("Field {index}")
            });
            object.document_bounds.y = index as f64 * 32.0;
            observation.objects.push(object);
        }
        observation.validate().unwrap();
        let mut views = AgentViewState::default();
        let index = views
            .project_with_mode(observation.clone(), &TruthViewMode::Index)
            .unwrap();
        assert_eq!(index["schema"], "saccade.agent-truth-index/1");
        assert_eq!(index["mode"], "index");
        assert_eq!(index["canonical_full_available"], true);
        assert_eq!(index["regions"].as_array().unwrap().len(), 2);
        assert_eq!(index["regions"][0]["region_id"], "r0-0");
        assert_eq!(index["regions"][0]["object_count"], 128);
        assert_eq!(index["regions"][1]["object_count"], 12);
        assert_eq!(index["cost"]["recommended_view"], "region");
        assert!(index["cost"]["full_bytes"].as_u64().unwrap() > 24_000);

        let region = views
            .project_with_mode(
                observation.clone(),
                &TruthViewMode::Region {
                    region_id: "r0-1".into(),
                    document_id: observation.document_id.clone(),
                    basis_revision: observation.revision,
                },
            )
            .unwrap();
        assert_eq!(region["schema"], "saccade.agent-region-view/1");
        assert_eq!(region["partial"], true);
        assert_eq!(region["objects"].as_array().unwrap().len(), 12);

        assert!(views
            .project_with_mode(
                observation.clone(),
                &TruthViewMode::Region {
                    region_id: "r0-1".into(),
                    document_id: observation.document_id.clone(),
                    basis_revision: observation.revision + 1,
                },
            )
            .unwrap_err()
            .to_string()
            .contains("region basis is stale"));

        let full = views
            .project_with_mode(observation, &TruthViewMode::Full)
            .unwrap();
        assert_eq!(full["mode"], "full");
        assert_eq!(full["objects"].as_array().unwrap().len(), 140);
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
}
