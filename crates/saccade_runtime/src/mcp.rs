//! MCP adapter and per-Agent Browser projection over the single HostClient interface.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use saccade_host_client::HostClient;
use saccade_protocol::{ActionReceipt, ObservationSnapshot};
use serde::Deserialize;
use serde_json::{json, Value};

const MCP_VERSION: &str = "2025-03-26";
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
    let host = HostClient::connect(grant_path)?;
    serve_io(&host, std::io::stdin().lock(), std::io::stdout().lock())
}

fn serve_io(host: &HostClient, input: impl BufRead, mut output: impl Write) -> Result<()> {
    let mut agent_views = AgentViewState::default();
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(request) => request,
            Err(error) => {
                write_rpc(&mut output, Value::Null, Err((-32700, error.to_string())))?;
                continue;
            }
        };
        let Some(id) = request.id.clone() else {
            continue;
        };
        let result =
            dispatch(host, &mut agent_views, request).map_err(|error| (-32000, error.to_string()));
        write_rpc(&mut output, id, result)?;
    }
    Ok(())
}

fn dispatch(
    host: &HostClient,
    agent_views: &mut AgentViewState,
    request: RpcRequest,
) -> Result<Value> {
    let diagnostics = diagnostic_input_overrides_enabled();
    if request.jsonrpc != "2.0" {
        bail!("unsupported JSON-RPC version {}", request.jsonrpc);
    }
    match request.method.as_str() {
        "initialize" => initialize(host),
        "notifications/initialized" | "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools":tools(diagnostics)})),
        "tools/call" => {
            let name = string(&request.params, "name")?;
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let value = call_tool(host, agent_views, name, arguments, diagnostics)?;
            let summary = tool_result_summary(&value);
            Ok(
                json!({"content":[{"type":"text","text":summary}],"structuredContent":value,"isError":false}),
            )
        }
        method => bail!("unknown JSON-RPC method {method}"),
    }
}

fn initialize(host: &HostClient) -> Result<Value> {
    let capabilities = host.call(
        NEXT_ID.fetch_add(1, Ordering::Relaxed),
        "system.capabilities",
        json!({}),
        Duration::from_secs(10),
    )?;
    let instructions = profile_instructions(&capabilities);
    Ok(json!({
        "protocolVersion":MCP_VERSION, "capabilities":{"tools":{"listChanged":false}},
        "serverInfo":{"name":"saccade-runtime","version":env!("CARGO_PKG_VERSION")},
        "instructions":instructions
    }))
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
    let base = "Observe a tab before acting, use only returned action tokens, and let web.act select the input backend.";
    if behavior.is_empty() {
        format!("Active Saccade Profile: {name}. {base}")
    } else {
        format!("Active Saccade Profile: {name}. User behavior: {behavior}\n{base}")
    }
}

fn call_tool(
    host: &HostClient,
    agent_views: &mut AgentViewState,
    name: &str,
    mut arguments: Value,
    diagnostics: bool,
) -> Result<Value> {
    let method = name
        .strip_prefix("saccade.")
        .context("tool is outside the Saccade namespace")?;
    if ![
        "system.capabilities",
        "tabs.list",
        "tabs.open",
        "web.observe",
        "web.act",
        "web.act_native",
        "web.act_soft",
        "input_policy.list",
        "input_policy.remember_native",
        "web.form.fill",
        "web.reflex.run",
    ]
    .contains(&method)
    {
        bail!("tool is not registered: {name}");
    }
    require_tool_enabled(method, diagnostics)?;
    validate_arguments(method, &arguments, diagnostics)?;
    agent_views.expand_object_aliases(method, &mut arguments)?;
    arguments = agent_views.hydrate_action_arguments(method, arguments)?;
    if method == "web.observe" && arguments.get("after_revision").is_none() {
        arguments
            .as_object_mut()
            .expect("validated observe arguments must be an object")
            .remove("timeout_ms");
    }
    let timeout = match method {
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
        "tabs.open" => Duration::from_secs(15),
        "web.observe" if arguments.get("after_revision").is_some() => Duration::from_millis(
            arguments
                .get("timeout_ms")
                .and_then(Value::as_u64)
                .unwrap_or(10_000)
                + 2_000,
        ),
        _ => Duration::from_secs(10),
    };
    let result = host.call(
        NEXT_ID.fetch_add(1, Ordering::Relaxed),
        method,
        arguments,
        timeout,
    )?;
    match method {
        "web.observe" => {
            let observation = serde_json::from_value::<ObservationSnapshot>(result)?;
            observation.validate()?;
            return agent_views.project(observation);
        }
        "web.act" | "web.act_native" | "web.act_soft" => {
            let receipt: ActionReceipt = serde_json::from_value(result)?;
            receipt.post_action_observation.validate()?;
            let view = agent_views.project(receipt.post_action_observation.clone())?;
            return Ok(json!({
                "schema":"saccade.agent-receipt/1",
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
            }));
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
            return Ok(form);
        }
        _ => {}
    }
    Ok(result)
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
        "tabs.open" => (&["url", "active"], &["url"]),
        "web.observe" => (&["tab_id", "after_revision", "timeout_ms"], &["tab_id"]),
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
        }
        "web.observe" => {
            string(value, "tab_id")?;
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

fn tools(diagnostics: bool) -> Vec<Value> {
    let mut tools = vec![
        json!({"name":"saccade.system.capabilities","description":"Read the active Profile behavior and Runtime capabilities.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"saccade.tabs.list","description":"List tabs managed by Saccade.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"saccade.tabs.open","description":"Open an HTTP or HTTPS tab managed by Saccade.","inputSchema":{"type":"object","properties":{"url":{"type":"string","minLength":1,"maxLength":8192},"active":{"type":"boolean"}},"required":["url"],"additionalProperties":false}}),
        json!({"name":"saccade.web.observe","description":"Read the Agent Browser: one full Truth Layer per document, then semantic deltas and opaque authority refreshes. Pass after_revision to wait locally for a newer browser revision instead of polling through the model. Without after_revision, this returns the current view immediately and ignores timeout_ms.","inputSchema":{"type":"object","properties":{"tab_id":{"type":"string","minLength":1},"after_revision":{"type":"integer","minimum":0},"timeout_ms":{"type":"integer","minimum":1,"maximum":30000}},"required":["tab_id"],"additionalProperties":false}}),
        json!({"name":"saccade.web.act","description":"Run one closed loop from a current action token. Use click for buttons/navigation, type with text, select with the observed option object_id, or upload with path. Saccade resolves the token only inside this Agent's current Truth Layer, selects the input backend, and returns a compact verified receipt.","inputSchema":action_request_schema(&["click","type","select","upload"])}),
        json!({"name":"saccade.input_policy.list","description":"List this user's local per-page learned input-backend records.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"saccade.input_policy.remember_native","description":"Remember that the current page control should use native input on future actions.","inputSchema":{"type":"object","properties":{"tab_id":{"type":"string","minLength":1},"action_token":{"type":"string","minLength":32}},"required":["tab_id","action_token"],"additionalProperties":false}}),
        json!({"name":"saccade.web.form.fill","description":"Fill a bounded same-document form plan in one request while every control retains its own closed loop. Use type for editables, select with the observed option object_id, and check only for checkbox/radio/switch controls. Saccade resolves all tokens from this Agent's current Truth Layer. Submit buttons and navigation must be a separate web.act click.","inputSchema":{"type":"object","properties":{"actions":{"type":"array","minItems":1,"maxItems":32,"items":form_action_schema()}},"required":["actions"],"additionalProperties":false}}),
        json!({"name":"saccade.web.reflex.run","description":"Keep a revision-bound reflex target loop local and return millisecond receipts using the Registry-selected backend.","inputSchema":{"type":"object","properties":{"tab_id":{"type":"string","minLength":1},"max_actions":{"type":"integer","minimum":1,"maximum":10000,"default":500},"timeout_ms":{"type":"integer","minimum":1,"maximum":60000,"default":30000}},"required":["tab_id"],"additionalProperties":false}}),
    ];
    if diagnostics {
        tools.push(json!({"name":"saccade.web.act_native","description":"Diagnostic override: run one revision-bound closed loop with native OS input.","inputSchema":action_request_schema(&["click","type","select","upload"])}));
        tools.push(json!({"name":"saccade.web.act_soft","description":"Diagnostic override: run one revision-bound click with registered software pointer input.","inputSchema":action_request_schema(&["click"])}));
        tools
            .iter_mut()
            .find(|tool| tool["name"] == "saccade.web.reflex.run")
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
}

#[derive(Default)]
struct AgentObjectAliases {
    document_id: String,
    by_internal: BTreeMap<String, String>,
    by_alias: BTreeMap<String, String>,
    next: u64,
}

impl AgentViewState {
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
        let previous = self
            .tabs
            .insert(observation.tab_id.clone(), observation.clone());
        let Some(previous) = previous else {
            return full_agent_view(observation, &aliases);
        };
        if previous.document_id != observation.document_id || observation.gap {
            return full_agent_view(observation, &aliases);
        }

        let previous_default_frame = default_frame_id(&previous);
        let current_default_frame = default_frame_id(&observation);
        let previous_objects = previous
            .objects
            .iter()
            .map(|object| (object.object_id.as_str(), object))
            .collect::<BTreeMap<_, _>>();
        let current_objects = observation
            .objects
            .iter()
            .map(|object| (object.object_id.as_str(), object))
            .collect::<BTreeMap<_, _>>();
        let mut changes = Vec::new();
        let mut changed_ids = std::collections::BTreeSet::new();
        for object in &observation.objects {
            match previous_objects.get(object.object_id.as_str()) {
                None => {
                    changed_ids.insert(object.object_id.clone());
                    changes.push(json!({"kind":"appeared","object":agent_object_value(object, current_default_frame.as_deref(), &aliases)?}));
                }
                Some(before)
                    if agent_object_fingerprint(
                        before,
                        previous_default_frame.as_deref(),
                        &aliases,
                    )? != agent_object_fingerprint(
                        object,
                        current_default_frame.as_deref(),
                        &aliases,
                    )? =>
                {
                    changed_ids.insert(object.object_id.clone());
                    changes.push(json!({"kind":"updated","object":agent_object_value(object, current_default_frame.as_deref(), &aliases)?}));
                }
                _ => {}
            }
        }
        for object in &previous.objects {
            if !current_objects.contains_key(object.object_id.as_str()) {
                changed_ids.insert(object.object_id.clone());
                changes.push(json!({
                    "kind":"disappeared",
                    "object_id":aliases.get(&object.object_id).context("missing Agent object alias")?
                }));
            }
        }

        let population = previous.objects.len().max(observation.objects.len());
        if changes.len() > 100 || (population > 20 && changes.len() * 2 > population) {
            return full_agent_view(observation, &aliases);
        }

        let authorities = observation
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
            .collect::<Vec<_>>();
        let frames_changed = previous.frames != observation.frames;
        Ok(json!({
            "schema":"saccade.agent-view/1",
            "mode":"delta",
            "browser_instance_id":observation.browser_instance_id,
            "tab_id":observation.tab_id,
            "document_id":observation.document_id,
            "revision":observation.revision,
            "viewport_revision":observation.viewport_revision,
            "object_defaults":agent_object_defaults(current_default_frame.as_deref()),
            "changes":changes,
            "authorities":authorities,
            "frames":frames_changed.then_some(observation.frames),
            "coverage":observation.coverage,
            "limitations":observation.limitations,
            "gap":false
        }))
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
) -> Result<Value> {
    let default_frame = default_frame_id(&observation);
    let objects = observation
        .objects
        .iter()
        .map(|object| agent_object_value(object, default_frame.as_deref(), aliases))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({
        "schema":"saccade.agent-view/1",
        "mode":"full",
        "browser_instance_id":observation.browser_instance_id,
        "tab_id":observation.tab_id,
        "document_id":observation.document_id,
        "revision":observation.revision,
        "viewport_revision":observation.viewport_revision,
        "object_defaults":agent_object_defaults(default_frame.as_deref()),
        "frames":observation.frames,
        "objects":objects,
        "coverage":observation.coverage,
        "limitations":observation.limitations,
        "gap":observation.gap
    }))
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

fn agent_object_fingerprint(
    object: &saccade_protocol::ObservedObject,
    default_frame: Option<&str>,
    aliases: &BTreeMap<String, String>,
) -> Result<Value> {
    let mut value = agent_object_value(object, default_frame, aliases)?;
    let fields = value
        .as_object_mut()
        .context("observed object did not serialize as an object")?;
    let actionable = fields.get("action_token").is_some();
    fields.remove("action_token");
    fields.insert("actionable".into(), Value::Bool(actionable));
    Ok(value)
}

fn agent_object_value(
    object: &saccade_protocol::ObservedObject,
    default_frame: Option<&str>,
    aliases: &BTreeMap<String, String>,
) -> Result<Value> {
    let mut value = serde_json::to_value(object)?;
    let fields = value
        .as_object_mut()
        .context("observed object did not serialize as an object")?;
    fields.remove("object_revision");
    fields.remove("document_bounds");
    fields.remove("viewport_bounds");
    fields.remove("loop_class_token");
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
    fn rpc_and_first_slice_tools_are_strict() {
        let request: RpcRequest = serde_json::from_value(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        )
        .unwrap();
        assert_eq!(request.method, "initialize");
        assert_eq!(tools(false).len(), 9);
        assert_eq!(tools(true).len(), 11);
        assert!(tools(false).iter().all(|tool| !matches!(
            tool["name"].as_str(),
            Some("saccade.web.act_native" | "saccade.web.act_soft")
        )));
        assert!(tools(false)
            .iter()
            .find(|tool| tool["name"] == "saccade.web.reflex.run")
            .unwrap()
            .pointer("/inputSchema/properties/input_backend")
            .is_none());
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
        assert_eq!(
            profile_instructions(
                &json!({"profile":{"name":"focused","behavior":"Work in page order."}})
            ),
            "Active Saccade Profile: focused. User behavior: Work in page order.\nObserve a tab before acting, use only returned action tokens, and let web.act select the input backend."
        );
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
        }

        first.revision += 1;
        first.viewport_revision += 1;
        first.objects[0].object_revision += 1;
        first.objects[0]
            .state
            .insert("pressed".into(), "true".into());
        first.objects[0].action_token = Some("token.abcdef0123456789abcdef0123456789abcdef".into());
        first.objects[1].object_revision += 1;
        first.objects[1].action_token =
            Some("token.2222222222222222222222222222222222222222".into());
        let delta = views.project(first.clone()).unwrap();
        assert_eq!(delta["mode"], "delta");
        assert_eq!(delta["changes"].as_array().unwrap().len(), 1);
        assert_eq!(delta["changes"][0]["kind"], "updated");
        assert_eq!(delta["authorities"].as_array().unwrap().len(), 1);
        assert_eq!(delta["authorities"][0]["object_id"], "o2");
        assert!(delta.get("objects").is_none());

        first.revision += 1;
        first.objects[1].action_token = None;
        let unavailable = views.project(first).unwrap();
        assert_eq!(unavailable["changes"].as_array().unwrap().len(), 1);
        assert_eq!(unavailable["changes"][0]["kind"], "updated");
        assert!(unavailable["changes"][0]["object"]
            .get("action_token")
            .is_none());
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
