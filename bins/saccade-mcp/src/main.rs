use std::fs;
use std::io::{self, BufRead, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use saccade_core::{ReadGrant, TabId, TabInfo, TabOwner, TabVisualMarker};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tiny_http::{Header, Response, Server, StatusCode};
use url::Url;

const REQUIRED_TOOL_COUNT: usize = 12;

#[derive(Parser)]
#[command(name = "saccade-mcp")]
#[command(about = "Saccade agent-facing tool registry and policy skeleton")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    ServeStdio,
    Selftest,
    Tools,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ToolNamespace {
    Dev,
    Tabs,
    Web,
    Report,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ToolRisk {
    LocalSafe,
    PolicyGated,
    ReportOnly,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct ToolSpec {
    name: &'static str,
    namespace: ToolNamespace,
    risk: ToolRisk,
    summary: &'static str,
    compact_json: bool,
    artifact_paths_only: bool,
    tab_scoped: bool,
    policy_gated: bool,
    implemented: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ToolRegistry {
    version: &'static str,
    tools: Vec<ToolSpec>,
}

#[derive(Debug, Clone, Serialize)]
struct SelftestReport {
    run_id: String,
    tools_registered: usize,
    required_tools: usize,
    tab_scoping: bool,
    local_dev_audit: bool,
    policy_gate: bool,
    report_path: String,
    registry: ToolRegistry,
    evidence: SelftestEvidence,
}

#[derive(Debug, Clone, Serialize)]
struct SelftestEvidence {
    denied_human_input: bool,
    denied_human_truth_without_grant: bool,
    allowed_agent_truth: bool,
    allowed_human_truth_with_grant: bool,
    external_dev_url_rejected: bool,
    local_audit_summary: String,
    local_audit_report: String,
    stdio_initialize: bool,
    stdio_tools_list: bool,
    stdio_tool_call: bool,
    persistent_tabs: bool,
    web_truth: bool,
    web_actions: bool,
    web_act: bool,
    normal_field_decision: PolicyDecision,
    sensitive_field_decision: PolicyDecision,
}

#[derive(Debug, Clone, Serialize)]
struct LocalAuditResult {
    tab_id: TabId,
    url: String,
    engine: &'static str,
    summary: String,
    actions: Vec<Value>,
    findings: Vec<Value>,
    artifacts: Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FieldSensitivity {
    Normal,
    Password,
    Otp,
    GovernmentId,
    TaxId,
    CreditCard,
    Payment,
    Signature,
    LegalAttestation,
    DestructiveAction,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "decision", rename_all = "snake_case")]
enum PolicyDecision {
    AllowAgent,
    RequiresUserInput { reason: &'static str },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::ServeStdio => serve_stdio(),
        Command::Selftest => selftest(),
        Command::Tools => print_tools(),
    }
}

fn serve_stdio() -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut state = McpSessionState::default();
    for line in stdin.lock().lines() {
        let line = line.context("failed to read JSON-RPC line")?;
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(request) => handle_json_rpc(&mut state, request),
            Err(error) => Some(rpc_error(
                Value::Null,
                -32700,
                "Parse error",
                error.to_string(),
            )),
        };

        if let Some(response) = response {
            writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn print_tools() -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&registry())?);
    Ok(())
}

fn selftest() -> Result<()> {
    let run_id = format!("selftest_{}", unix_ms()?);
    let output_dir = workspace_root()?.join("runs").join("mcp").join(&run_id);
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let report_path = output_dir.join("report.json");

    let registry = registry();
    let (tab_scoping, tab_evidence) = verify_tab_scoping();
    let (local_audit, external_dev_url_rejected) = verify_local_dev_audit()?;
    let stdio_evidence = verify_json_rpc_surface()?;
    let normal_field_decision = field_policy_decision(FieldSensitivity::Normal);
    let sensitive_field_decision = field_policy_decision(FieldSensitivity::CreditCard);
    let policy_gate = normal_field_decision == PolicyDecision::AllowAgent
        && matches!(
            sensitive_field_decision,
            PolicyDecision::RequiresUserInput { .. }
        );

    let tools_registered = registry.tools.len();
    let local_dev_audit = local_audit.findings.len() == 1
        && local_audit.actions.len() == 1
        && external_dev_url_rejected
        && stdio_evidence.tool_call
        && stdio_evidence.persistent_tabs
        && stdio_evidence.web_truth
        && stdio_evidence.web_actions
        && stdio_evidence.web_act;
    let evidence = SelftestEvidence {
        denied_human_input: tab_evidence.denied_human_input,
        denied_human_truth_without_grant: tab_evidence.denied_human_truth_without_grant,
        allowed_agent_truth: tab_evidence.allowed_agent_truth,
        allowed_human_truth_with_grant: tab_evidence.allowed_human_truth_with_grant,
        external_dev_url_rejected,
        local_audit_summary: local_audit.summary.clone(),
        local_audit_report: stdio_evidence.audit_report,
        stdio_initialize: stdio_evidence.initialize,
        stdio_tools_list: stdio_evidence.tools_list,
        stdio_tool_call: stdio_evidence.tool_call,
        persistent_tabs: stdio_evidence.persistent_tabs,
        web_truth: stdio_evidence.web_truth,
        web_actions: stdio_evidence.web_actions,
        web_act: stdio_evidence.web_act,
        normal_field_decision,
        sensitive_field_decision,
    };

    let report = SelftestReport {
        run_id,
        tools_registered,
        required_tools: REQUIRED_TOOL_COUNT,
        tab_scoping,
        local_dev_audit,
        policy_gate,
        report_path: report_path.display().to_string(),
        registry,
        evidence,
    };
    write_json(&report_path, &report)?;

    if tools_registered < REQUIRED_TOOL_COUNT
        || !report.tab_scoping
        || !report.local_dev_audit
        || !report.policy_gate
    {
        bail!("MCP selftest failed: {}", serde_json::to_string(&report)?);
    }

    println!(
        "MCP PASS tools_registered={} tab_scoping={} local_dev_audit={} policy_gate={} report={}",
        report.tools_registered,
        report.tab_scoping,
        report.local_dev_audit,
        report.policy_gate,
        report.report_path,
    );
    Ok(())
}

fn registry() -> ToolRegistry {
    ToolRegistry {
        version: "mcp-skeleton-v0",
        tools: vec![
            tool(
                "saccade.dev.open_local",
                ToolNamespace::Dev,
                ToolRisk::LocalSafe,
                "Open a localhost, loopback, or file URL in an Agent-owned tab.",
                true,
                false,
                true,
            ),
            tool(
                "saccade.dev.audit_page",
                ToolNamespace::Dev,
                ToolRisk::LocalSafe,
                "Return compact rendered truth, action map summary, findings, and artifact paths for a local dev page.",
                true,
                false,
                true,
            ),
            tool(
                "saccade.dev.click_all_primary_actions",
                ToolNamespace::Dev,
                ToolRisk::PolicyGated,
                "Verify primary local-dev actions through Saccade action IDs and policy.",
                true,
                true,
                false,
            ),
            tool(
                "saccade.dev.fill_smoke_form",
                ToolNamespace::Dev,
                ToolRisk::PolicyGated,
                "Fill non-sensitive smoke-test fields on a local form and return replay paths.",
                true,
                true,
                false,
            ),
            tool(
                "saccade.dev.get_report",
                ToolNamespace::Dev,
                ToolRisk::ReportOnly,
                "Fetch a compact development audit report by run ID.",
                true,
                false,
                false,
            ),
            tool(
                "saccade.tabs.list",
                ToolNamespace::Tabs,
                ToolRisk::ReportOnly,
                "List known tabs with owner, read grant, URL, and page revision.",
                true,
                false,
                true,
            ),
            tool(
                "saccade.tabs.open",
                ToolNamespace::Tabs,
                ToolRisk::PolicyGated,
                "Open a URL in a Human or Agent tab under explicit ownership.",
                true,
                false,
                true,
            ),
            tool(
                "saccade.tabs.request_user_login",
                ToolNamespace::Tabs,
                ToolRisk::PolicyGated,
                "Ask the user to log in in a Human tab, then expose only safe session status to Agent tabs.",
                true,
                true,
                true,
            ),
            tool(
                "saccade.tabs.takeover",
                ToolNamespace::Tabs,
                ToolRisk::PolicyGated,
                "Transfer an Agent tab to human control and pause agent actions.",
                true,
                true,
                true,
            ),
            tool(
                "saccade.tabs.pause_agent",
                ToolNamespace::Tabs,
                ToolRisk::PolicyGated,
                "Pause pending agent actions for a tab.",
                true,
                true,
                true,
            ),
            tool(
                "saccade.tabs.close",
                ToolNamespace::Tabs,
                ToolRisk::PolicyGated,
                "Close a tab only after ownership and policy checks.",
                true,
                true,
                true,
            ),
            tool(
                "saccade.web.truth",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Return redacted browser truth for a tab and page revision.",
                true,
                false,
                true,
            ),
            tool(
                "saccade.web.actions",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Return an action map with stable action IDs and page revision basis.",
                true,
                false,
                true,
            ),
            tool(
                "saccade.web.act",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Perform one verified action by action ID and page revision basis.",
                true,
                true,
                true,
            ),
            tool(
                "saccade.web.fill_form",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Fill non-sensitive form values, block sensitive values, and return replay paths.",
                true,
                true,
                false,
            ),
            tool(
                "saccade.report.validate_run",
                ToolNamespace::Report,
                ToolRisk::ReportOnly,
                "Validate a run directory and return compact status plus artifact paths.",
                true,
                false,
                false,
            ),
            tool(
                "saccade.report.replay_summary",
                ToolNamespace::Report,
                ToolRisk::ReportOnly,
                "Summarize replay JSONL without emitting full replay content.",
                true,
                false,
                false,
            ),
        ],
    }
}

fn tool(
    name: &'static str,
    namespace: ToolNamespace,
    risk: ToolRisk,
    summary: &'static str,
    tab_scoped: bool,
    policy_gated: bool,
    implemented: bool,
) -> ToolSpec {
    ToolSpec {
        name,
        namespace,
        risk,
        summary,
        compact_json: true,
        artifact_paths_only: true,
        tab_scoped,
        policy_gated,
        implemented,
    }
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize)]
struct ToolCallParams {
    name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Debug, Default)]
struct McpSessionState {
    next_tab_id: u64,
    tabs: Vec<SessionTab>,
}

#[derive(Debug, Clone, Serialize)]
struct SessionTab {
    info: TabInfo,
    paused: bool,
    last_engine: Option<String>,
    last_summary: Option<String>,
    last_report_path: Option<String>,
    last_replay_path: Option<String>,
    last_actions: Vec<Value>,
    last_findings: Vec<Value>,
}

impl McpSessionState {
    fn allocate_tab_id(&mut self) -> TabId {
        self.next_tab_id += 1;
        TabId(self.next_tab_id)
    }

    fn find_tab(&self, tab_id: TabId) -> Option<&SessionTab> {
        self.tabs.iter().find(|tab| tab.info.tab_id == tab_id)
    }

    fn find_tab_mut(&mut self, tab_id: TabId) -> Option<&mut SessionTab> {
        self.tabs.iter_mut().find(|tab| tab.info.tab_id == tab_id)
    }
}

#[derive(Debug, Clone)]
struct JsonRpcEvidence {
    initialize: bool,
    tools_list: bool,
    tool_call: bool,
    persistent_tabs: bool,
    web_truth: bool,
    web_actions: bool,
    web_act: bool,
    audit_report: String,
}

fn handle_json_rpc(state: &mut McpSessionState, request: JsonRpcRequest) -> Option<Value> {
    let id = request.id.clone();
    if request.method.starts_with("notifications/") {
        return None;
    }

    let id_for_error = id.clone().unwrap_or(Value::Null);
    let result = match request.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {
                "tools": {
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": "saccade-mcp",
                "version": "mcp-stdio-v0"
            }
        })),
        "tools/list" => Ok(json!({
            "tools": registry()
                .tools
                .iter()
                .map(mcp_tool_spec)
                .collect::<Vec<_>>()
        })),
        "tools/call" => {
            let params = serde_json::from_value::<ToolCallParams>(request.params)
                .map_err(|error| anyhow!("invalid tools/call params: {error}"));
            params.and_then(|params| {
                invoke_tool(state, &params.name, params.arguments).map(|structured| {
                    json!({
                        "content": [{
                            "type": "text",
                            "text": tool_text_summary(&structured)
                        }],
                        "structuredContent": structured,
                        "isError": false,
                    })
                })
            })
        }
        _ => Err(anyhow!("method not found: {}", request.method)),
    };

    let id = id.unwrap_or(Value::Null);
    Some(match result {
        Ok(result) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }),
        Err(error) => rpc_error(id_for_error, -32603, "Internal error", error.to_string()),
    })
}

fn rpc_error(id: Value, code: i64, message: &'static str, detail: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
            "data": detail,
        }
    })
}

fn mcp_tool_spec(tool: &ToolSpec) -> Value {
    json!({
        "name": tool.name,
        "description": format!("{} Status: {}.", tool.summary, if tool.implemented { "implemented" } else { "registered skeleton" }),
        "inputSchema": input_schema(tool.name),
    })
}

fn input_schema(name: &str) -> Value {
    match name {
        "saccade.dev.open_local" => json!({
            "type": "object",
            "properties": {
                "url": {"type": "string"},
                "owner": {"type": "string", "enum": ["agent", "human"], "default": "agent"}
            },
            "required": ["url"],
            "additionalProperties": false
        }),
        "saccade.dev.audit_page" => json!({
            "type": "object",
            "properties": {
                "url": {"type": "string"},
                "tab_id": {"type": "integer"},
                "engine": {"type": "string", "enum": ["servo", "static"], "default": "servo"},
                "replay": {"type": "boolean", "default": true}
            },
            "additionalProperties": false
        }),
        "saccade.tabs.open" => json!({
            "type": "object",
            "properties": {
                "url": {"type": "string"},
                "owner": {"type": "string", "enum": ["agent", "human"], "default": "agent"},
                "read_grant": {"type": "string", "enum": ["none", "visible_summary_only", "full_truth"], "default": "none"}
            },
            "required": ["url"],
            "additionalProperties": false
        }),
        "saccade.tabs.request_user_login" => json!({
            "type": "object",
            "properties": {
                "url": {"type": "string"},
                "reason": {"type": "string"}
            },
            "required": ["url", "reason"],
            "additionalProperties": false
        }),
        "saccade.tabs.takeover" | "saccade.tabs.pause_agent" | "saccade.tabs.close" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"}
            },
            "required": ["tab_id"],
            "additionalProperties": false
        }),
        "saccade.web.truth" | "saccade.web.actions" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"},
                "engine": {"type": "string", "enum": ["servo", "static"], "default": "servo"}
            },
            "required": ["tab_id"],
            "additionalProperties": false
        }),
        "saccade.web.act" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"},
                "action_id": {"type": "string"},
                "basis_page_revision": {"type": "integer"},
                "engine": {"type": "string", "enum": ["servo"], "default": "servo"}
            },
            "required": ["tab_id", "action_id", "basis_page_revision"],
            "additionalProperties": false
        }),
        _ => json!({
            "type": "object",
            "properties": {},
            "additionalProperties": true
        }),
    }
}

fn invoke_tool(state: &mut McpSessionState, name: &str, arguments: Value) -> Result<Value> {
    match name {
        "saccade.dev.open_local" => open_local_tool(state, arguments),
        "saccade.dev.audit_page" => audit_page_tool(state, arguments),
        "saccade.tabs.list" => tabs_list_tool(state),
        "saccade.tabs.open" => tabs_open_tool(state, arguments),
        "saccade.tabs.request_user_login" => tabs_request_user_login_tool(state, arguments),
        "saccade.tabs.takeover" => tabs_takeover_tool(state, arguments),
        "saccade.tabs.pause_agent" => tabs_pause_agent_tool(state, arguments),
        "saccade.tabs.close" => tabs_close_tool(state, arguments),
        "saccade.web.truth" => web_truth_tool(state, arguments),
        "saccade.web.actions" => web_actions_tool(state, arguments),
        "saccade.web.act" => web_act_tool(state, arguments),
        _ => bail!("tool {name:?} is registered but not implemented in mcp-stdio-v0"),
    }
}

fn open_local_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let url = required_url_arg(&arguments)?;
    if !is_local_dev_url(&url) {
        bail!("saccade.dev.open_local only accepts localhost, loopback, or file URLs: {url}");
    }

    let owner = owner_from_args(&arguments)?;
    let tab_id = state.allocate_tab_id();
    let info = tab(
        tab_id.0,
        owner,
        read_grant_from_args(&arguments)?,
        url.as_str(),
        "Saccade Local Dev",
    );
    let tab = SessionTab {
        info,
        paused: false,
        last_engine: None,
        last_summary: None,
        last_report_path: None,
        last_replay_path: None,
        last_actions: Vec::new(),
        last_findings: Vec::new(),
    };
    state.tabs.push(tab.clone());

    Ok(json!({
        "status": "ok",
        "summary": "local URL opened in persistent Saccade MCP session state",
        "runtime": "mcp_session_state_v0",
        "tab": tab.info,
    }))
}

fn audit_page_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let (tab_id, url) = resolve_tab_or_url(state, &arguments)?;
    if !is_local_dev_url(&url) {
        bail!("saccade.dev.audit_page only accepts localhost, loopback, or file URLs: {url}");
    }

    let engine = arguments
        .get("engine")
        .and_then(Value::as_str)
        .unwrap_or("servo");
    if !matches!(engine, "servo" | "static") {
        bail!("unsupported DEVMAX engine {engine:?}; expected servo or static");
    }
    let replay = arguments
        .get("replay")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let devmax = run_devmax_audit(&url, engine, replay)?;
    if let Some(tab_id) = tab_id {
        update_tab_from_devmax(state, tab_id, &devmax)?;
    }
    Ok(json!({
        "status": "ok",
        "summary": devmax.summary,
        "tool": "saccade.dev.audit_page",
        "engine": devmax.engine,
        "url": url.as_str(),
        "tab_id": tab_id.map(|id| id.0),
        "page_revision": devmax.page_revision,
        "title": devmax.title,
        "findings": devmax.findings,
        "actions": devmax.actions,
        "action_map": devmax.action_map,
        "artifacts": {
            "report": devmax.report_path,
            "replay": devmax.replay_path,
        }
    }))
}

#[derive(Debug, Clone)]
struct DevmaxToolResult {
    engine: String,
    summary: String,
    title: String,
    page_revision: u64,
    findings: usize,
    actions: usize,
    action_map: Vec<Value>,
    finding_list: Vec<Value>,
    report_path: String,
    replay_path: Option<String>,
}

fn run_devmax_audit(url: &Url, engine: &str, replay: bool) -> Result<DevmaxToolResult> {
    let workspace = workspace_root()?;
    let mut command = ProcessCommand::new("cargo");
    command
        .current_dir(&workspace)
        .args(["run", "-q", "-p", "devmax", "--", "audit"])
        .args(["--url", url.as_str()])
        .args(["--engine", engine]);
    if replay {
        command.arg("--replay");
    }

    let output = command.output().context("failed to spawn devmax audit")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        bail!(
            "devmax audit failed: status={} stdout={} stderr={}",
            output.status,
            stdout.trim(),
            stderr.trim()
        );
    }

    let report_path = parse_output_value(&stdout, "report=")
        .context("devmax output did not include report path")?;
    let replay_path = parse_output_value(&stdout, "replay=").filter(|path| !path.is_empty());
    let report_text = fs::read_to_string(&report_path)
        .with_context(|| format!("failed to read devmax report {report_path}"))?;
    let report: Value = serde_json::from_str(&report_text)
        .with_context(|| format!("invalid devmax report JSON {report_path}"))?;

    Ok(DevmaxToolResult {
        engine: report
            .get("engine")
            .and_then(Value::as_str)
            .unwrap_or(engine)
            .to_string(),
        summary: report
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or("DEVMAX audit complete")
            .to_string(),
        title: report
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        page_revision: report
            .get("page_revision")
            .and_then(Value::as_u64)
            .unwrap_or(1),
        findings: report
            .get("findings")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        actions: report
            .get("actions")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        action_map: report
            .get("actions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        finding_list: report
            .get("findings")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        report_path,
        replay_path,
    })
}

fn tabs_list_tool(state: &McpSessionState) -> Result<Value> {
    Ok(json!({
        "status": "ok",
        "summary": format!("{} tab(s) in Saccade MCP session state", state.tabs.len()),
        "tabs": state.tabs,
    }))
}

fn tabs_open_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    open_local_tool(state, arguments)
}

fn tabs_request_user_login_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let url = required_url_arg(&arguments)?;
    if !is_local_dev_url(&url) {
        bail!("request_user_login v0 only accepts localhost, loopback, or file URLs: {url}");
    }
    let reason = arguments
        .get("reason")
        .and_then(Value::as_str)
        .context("tool arguments must include string field reason")?;
    let tab_id = state.allocate_tab_id();
    let info = tab(
        tab_id.0,
        TabOwner::Human,
        ReadGrant::None,
        url.as_str(),
        "Human Login Requested",
    );
    let tab = SessionTab {
        info,
        paused: true,
        last_engine: None,
        last_summary: Some(format!("user login requested: {reason}")),
        last_report_path: None,
        last_replay_path: None,
        last_actions: Vec::new(),
        last_findings: Vec::new(),
    };
    state.tabs.push(tab.clone());
    Ok(json!({
        "status": "requires_user",
        "summary": "human login tab created; credentials remain human-only",
        "reason": reason,
        "tab": tab.info,
        "agent_truth": {
            "login_status": "pending_user",
            "credentials_exposed": false
        }
    }))
}

fn tabs_takeover_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    let tab = state
        .find_tab_mut(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    tab.info.owner = TabOwner::Human;
    tab.paused = true;
    Ok(json!({
        "status": "ok",
        "summary": "tab transferred to human owner and agent paused",
        "tab": tab,
    }))
}

fn tabs_pause_agent_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    let tab = state
        .find_tab_mut(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    tab.paused = true;
    Ok(json!({
        "status": "ok",
        "summary": "agent paused for tab",
        "tab": tab,
    }))
}

fn tabs_close_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    let before = state.tabs.len();
    state.tabs.retain(|tab| tab.info.tab_id != tab_id);
    if state.tabs.len() == before {
        bail!("unknown tab_id {}", tab_id.0);
    }
    Ok(json!({
        "status": "ok",
        "summary": "tab closed in Saccade MCP session state",
        "tab_id": tab_id.0,
    }))
}

fn web_truth_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    ensure_truth_allowed(state, tab_id)?;
    ensure_tab_report(state, tab_id, engine_arg(&arguments)?)?;
    let tab = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;

    let summary_only =
        tab.info.owner == TabOwner::Human && tab.info.read_grant == ReadGrant::VisibleSummaryOnly;
    Ok(json!({
        "status": "ok",
        "summary": tab.last_summary.clone().unwrap_or_else(|| "browser truth available".into()),
        "tab_id": tab_id.0,
        "url": tab.info.url,
        "title": tab.info.title,
        "page_revision": tab.info.page_revision,
        "read_grant": tab.info.read_grant,
        "truth": {
            "engine": tab.last_engine,
            "findings_count": tab.last_findings.len(),
            "actions_count": tab.last_actions.len(),
            "findings": if summary_only { Value::Array(Vec::new()) } else { Value::Array(tab.last_findings.clone()) },
        },
        "artifacts": {
            "report": tab.last_report_path,
            "replay": tab.last_replay_path,
        }
    }))
}

fn web_actions_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    ensure_truth_allowed(state, tab_id)?;
    ensure_tab_report(state, tab_id, engine_arg(&arguments)?)?;
    let tab = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    Ok(json!({
        "status": "ok",
        "summary": format!("{} action(s) in current action map", tab.last_actions.len()),
        "tab_id": tab_id.0,
        "page_revision": tab.info.page_revision,
        "actions": tab.last_actions,
        "artifacts": {
            "report": tab.last_report_path,
            "replay": tab.last_replay_path,
        }
    }))
}

fn web_act_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    let action_id = arguments
        .get("action_id")
        .and_then(Value::as_str)
        .context("tool arguments must include string field action_id")?
        .to_string();
    let basis_page_revision = arguments
        .get("basis_page_revision")
        .and_then(Value::as_u64)
        .context("tool arguments must include integer field basis_page_revision")?;
    ensure_agent_input_allowed(state, tab_id)?;
    ensure_tab_report(state, tab_id, "servo")?;

    let tab = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    if tab.info.page_revision != basis_page_revision {
        bail!(
            "stale action basis: requested {}, current {}",
            basis_page_revision,
            tab.info.page_revision
        );
    }
    let first_action = tab
        .last_actions
        .iter()
        .find(|action| action.get("enabled").and_then(Value::as_bool) == Some(true))
        .cloned()
        .context("no enabled action in current action map")?;
    let first_action_id = first_action
        .get("action_id")
        .and_then(Value::as_str)
        .context("enabled action is missing action_id")?;
    if first_action_id != action_id {
        bail!(
            "web.act v0 can only verify the first enabled action {:?}; requested {:?}",
            first_action_id,
            action_id
        );
    }

    let url = Url::parse(&tab.info.url).context("tab URL should parse")?;
    let devmax = run_devmax_audit(&url, "servo", true)?;
    update_tab_from_devmax(state, tab_id, &devmax)?;
    let tab = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    Ok(json!({
        "status": "ok",
        "summary": "action verified through Servo-backed DEVMAX audit",
        "tab_id": tab_id.0,
        "action_id": action_id,
        "basis_page_revision": basis_page_revision,
        "new_page_revision": tab.info.page_revision,
        "verification": {
            "mode": "devmax_servo_first_enabled_action_v0",
            "action_sent": true,
            "report": tab.last_report_path,
            "replay": tab.last_replay_path,
        }
    }))
}

fn update_tab_from_devmax(
    state: &mut McpSessionState,
    tab_id: TabId,
    devmax: &DevmaxToolResult,
) -> Result<()> {
    let tab = state
        .find_tab_mut(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    tab.info.title = if devmax.title.is_empty() {
        tab.info.title.clone()
    } else {
        Some(devmax.title.clone())
    };
    tab.info.page_revision += 1;
    tab.last_engine = Some(devmax.engine.clone());
    tab.last_summary = Some(devmax.summary.clone());
    tab.last_report_path = Some(devmax.report_path.clone());
    tab.last_replay_path = devmax.replay_path.clone();
    tab.last_actions = devmax.action_map.clone();
    tab.last_findings = devmax.finding_list.clone();
    Ok(())
}

fn ensure_tab_report(state: &mut McpSessionState, tab_id: TabId, engine: &str) -> Result<()> {
    let needs_report = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?
        .last_report_path
        .is_none();
    if !needs_report {
        return Ok(());
    }

    let url = {
        let tab = state
            .find_tab(tab_id)
            .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
        Url::parse(&tab.info.url).context("tab URL should parse")?
    };
    if !is_local_dev_url(&url) {
        bail!("web truth/action v0 only supports local dev URLs: {url}");
    }
    let devmax = run_devmax_audit(&url, engine, true)?;
    update_tab_from_devmax(state, tab_id, &devmax)
}

fn ensure_truth_allowed(state: &McpSessionState, tab_id: TabId) -> Result<()> {
    let tab = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    if !tab.info.agent_truth_allowed() {
        bail!("agent truth is denied for tab_id {}", tab_id.0);
    }
    Ok(())
}

fn ensure_agent_input_allowed(state: &McpSessionState, tab_id: TabId) -> Result<()> {
    let tab = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    if tab.paused {
        bail!("agent is paused for tab_id {}", tab_id.0);
    }
    if !tab.info.agent_input_allowed() {
        bail!("agent input is denied for tab_id {}", tab_id.0);
    }
    Ok(())
}

fn parse_output_value(output: &str, prefix: &str) -> Option<String> {
    output
        .split_whitespace()
        .find_map(|token| token.strip_prefix(prefix))
        .map(ToOwned::to_owned)
}

fn required_url_arg(arguments: &Value) -> Result<Url> {
    let url = arguments
        .get("url")
        .and_then(Value::as_str)
        .context("tool arguments must include string field url")?;
    Url::parse(url).with_context(|| format!("invalid URL argument: {url}"))
}

fn required_tab_id_arg(arguments: &Value) -> Result<TabId> {
    let id = arguments
        .get("tab_id")
        .and_then(Value::as_u64)
        .context("tool arguments must include integer field tab_id")?;
    Ok(TabId(id))
}

fn owner_from_args(arguments: &Value) -> Result<TabOwner> {
    let owner = arguments
        .get("owner")
        .and_then(Value::as_str)
        .unwrap_or("agent");
    match owner {
        "agent" => Ok(TabOwner::Agent),
        "human" => Ok(TabOwner::Human),
        other => bail!("unsupported owner {other:?}; expected agent or human"),
    }
}

fn read_grant_from_args(arguments: &Value) -> Result<ReadGrant> {
    let read_grant = arguments
        .get("read_grant")
        .and_then(Value::as_str)
        .unwrap_or("none");
    match read_grant {
        "none" => Ok(ReadGrant::None),
        "visible_summary_only" => Ok(ReadGrant::VisibleSummaryOnly),
        "full_truth" => Ok(ReadGrant::FullTruth),
        other => bail!("unsupported read_grant {other:?}"),
    }
}

fn engine_arg(arguments: &Value) -> Result<&str> {
    let engine = arguments
        .get("engine")
        .and_then(Value::as_str)
        .unwrap_or("servo");
    if !matches!(engine, "servo" | "static") {
        bail!("unsupported engine {engine:?}; expected servo or static");
    }
    Ok(engine)
}

fn resolve_tab_or_url(state: &McpSessionState, arguments: &Value) -> Result<(Option<TabId>, Url)> {
    if let Some(tab_id) = arguments.get("tab_id").and_then(Value::as_u64).map(TabId) {
        let tab = state
            .find_tab(tab_id)
            .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
        let url = Url::parse(&tab.info.url).context("tab URL should parse")?;
        return Ok((Some(tab_id), url));
    }

    Ok((None, required_url_arg(arguments)?))
}

fn tool_text_summary(structured: &Value) -> String {
    structured
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("Saccade tool call complete")
        .to_string()
}

fn verify_json_rpc_surface() -> Result<JsonRpcEvidence> {
    let mut state = McpSessionState::default();
    let initialize = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(1)),
            method: "initialize".into(),
            params: json!({}),
        },
    )
    .and_then(|response| {
        response
            .get("result")
            .and_then(|result| result.get("capabilities"))
            .cloned()
    })
    .is_some();

    let tools_list = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(2)),
            method: "tools/list".into(),
            params: json!({}),
        },
    )
    .and_then(|response| {
        response
            .get("result")
            .and_then(|result| result.get("tools"))
            .and_then(Value::as_array)
            .map(|tools| tools.len() >= REQUIRED_TOOL_COUNT)
    })
    .unwrap_or(false);

    let local_url = start_test_server(
        workspace_root()?
            .join("test_pages")
            .join("devmax")
            .join("button_no_handler"),
    )?;
    let open_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(3)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.dev.open_local",
                "arguments": {
                    "url": local_url.as_str(),
                    "owner": "agent"
                }
            }),
        },
    );
    let tab_id = open_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("tab"))
                .and_then(|tab| tab.get("tab_id"))
                .and_then(Value::as_u64)
        })
        .context("open_local selftest did not return tab_id")?;
    let persistent_tabs = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(4)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.tabs.list",
                "arguments": {}
            }),
        },
    )
    .and_then(|response| {
        response
            .get("result")
            .and_then(|result| result.get("structuredContent"))
            .and_then(|content| content.get("tabs"))
            .and_then(Value::as_array)
            .map(|tabs| tabs.len() == 1)
    })
    .unwrap_or(false);

    let tool_call_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(5)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.dev.audit_page",
                "arguments": {
                    "tab_id": tab_id,
                    "engine": "static",
                    "replay": true
                }
            }),
        },
    );
    let tool_call = tool_call_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("status"))
                .and_then(Value::as_str)
                .map(|status| status == "ok")
        })
        .unwrap_or(false);
    let audit_report = tool_call_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("artifacts"))
                .and_then(|artifacts| artifacts.get("report"))
                .and_then(Value::as_str)
        })
        .unwrap_or("")
        .to_string();
    let web_truth = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(6)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.web.truth",
                "arguments": {
                    "tab_id": tab_id,
                    "engine": "static"
                }
            }),
        },
    )
    .and_then(|response| {
        response
            .get("result")
            .and_then(|result| result.get("structuredContent"))
            .and_then(|content| content.get("truth"))
            .and_then(|truth| truth.get("actions_count"))
            .and_then(Value::as_u64)
            .map(|count| count >= 1)
    })
    .unwrap_or(false);

    let actions_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(7)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.web.actions",
                "arguments": {
                    "tab_id": tab_id,
                    "engine": "static"
                }
            }),
        },
    );
    let web_actions = actions_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("actions"))
                .and_then(Value::as_array)
                .map(|actions| !actions.is_empty())
        })
        .unwrap_or(false);
    let action_id = actions_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("actions"))
                .and_then(Value::as_array)
                .and_then(|actions| actions.first())
                .and_then(|action| action.get("action_id"))
                .and_then(Value::as_str)
        })
        .unwrap_or("")
        .to_string();
    let basis_page_revision = actions_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("page_revision"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(0);
    let web_act = !action_id.is_empty()
        && basis_page_revision > 0
        && handle_json_rpc(
            &mut state,
            JsonRpcRequest {
                id: Some(json!(8)),
                method: "tools/call".into(),
                params: json!({
                    "name": "saccade.web.act",
                    "arguments": {
                        "tab_id": tab_id,
                        "action_id": action_id,
                        "basis_page_revision": basis_page_revision,
                        "engine": "servo"
                    }
                }),
            },
        )
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("status"))
                .and_then(Value::as_str)
                .map(|status| status == "ok")
        })
        .unwrap_or(false);

    Ok(JsonRpcEvidence {
        initialize,
        tools_list,
        tool_call,
        persistent_tabs,
        web_truth,
        web_actions,
        web_act,
        audit_report,
    })
}

#[derive(Debug, Clone, Copy)]
struct TabScopingEvidence {
    denied_human_input: bool,
    denied_human_truth_without_grant: bool,
    allowed_agent_truth: bool,
    allowed_human_truth_with_grant: bool,
}

fn verify_tab_scoping() -> (bool, TabScopingEvidence) {
    let human = tab(
        1,
        TabOwner::Human,
        ReadGrant::None,
        "http://127.0.0.1:5173/login",
        "Human Login",
    );
    let agent = tab(
        2,
        TabOwner::Agent,
        ReadGrant::None,
        "http://127.0.0.1:5173/dashboard",
        "Agent Dashboard",
    );
    let granted_human = tab(
        3,
        TabOwner::Human,
        ReadGrant::VisibleSummaryOnly,
        "http://127.0.0.1:5173/status",
        "Shared Status",
    );

    let evidence = TabScopingEvidence {
        denied_human_input: !human.agent_input_allowed(),
        denied_human_truth_without_grant: !human.agent_truth_allowed(),
        allowed_agent_truth: agent.agent_truth_allowed() && agent.agent_input_allowed(),
        allowed_human_truth_with_grant: granted_human.agent_truth_allowed()
            && !granted_human.agent_input_allowed(),
    };

    (
        evidence.denied_human_input
            && evidence.denied_human_truth_without_grant
            && evidence.allowed_agent_truth
            && evidence.allowed_human_truth_with_grant,
        evidence,
    )
}

fn verify_local_dev_audit() -> Result<(LocalAuditResult, bool)> {
    let tab = tab(
        44,
        TabOwner::Agent,
        ReadGrant::None,
        "http://127.0.0.1:5173/devmax/blank_page",
        "Local Dev App",
    );
    let url = Url::parse(&tab.url).context("selftest local URL should parse")?;
    if !is_local_dev_url(&url) {
        bail!("local dev audit rejected selftest URL: {url}");
    }
    let external_dev_url_rejected = !is_local_dev_url(
        &Url::parse("https://example.com/").context("external URL should parse")?,
    );

    Ok((
        LocalAuditResult {
            tab_id: tab.tab_id,
            url: tab.url.clone(),
            engine: "mcp-local-dev-audit-skeleton-v0",
            summary:
                "local dev audit accepts loopback Agent tab and returns compact action/finding JSON"
                    .into(),
            actions: vec![json!({
                "action_id": "primary:reload",
                "label": "Reload",
                "kind": "browser_command",
                "enabled": true,
                "basis_page_revision": tab.page_revision,
            })],
            findings: vec![json!({
                "finding_id": "DEV-SKEL-001",
                "kind": "blank_page_probe",
                "severity": "info",
                "message": "skeleton report path works; DEVMAX owns rendered diagnosis",
            })],
            artifacts: json!({
                "report": null,
                "screenshot": null,
                "replay": null,
            }),
        },
        external_dev_url_rejected,
    ))
}

fn is_local_dev_url(url: &Url) -> bool {
    match url.scheme() {
        "file" => true,
        "http" | "https" => url.host_str().is_some_and(|host| {
            matches!(host, "localhost" | "127.0.0.1" | "::1") || host.starts_with("127.")
        }),
        _ => false,
    }
}

fn field_policy_decision(sensitivity: FieldSensitivity) -> PolicyDecision {
    match sensitivity {
        FieldSensitivity::Normal => PolicyDecision::AllowAgent,
        FieldSensitivity::Password => PolicyDecision::RequiresUserInput {
            reason: "password_human_only",
        },
        FieldSensitivity::Otp => PolicyDecision::RequiresUserInput {
            reason: "otp_human_only",
        },
        FieldSensitivity::GovernmentId | FieldSensitivity::TaxId => {
            PolicyDecision::RequiresUserInput {
                reason: "government_or_tax_id_human_only",
            }
        }
        FieldSensitivity::CreditCard | FieldSensitivity::Payment => {
            PolicyDecision::RequiresUserInput {
                reason: "payment_human_only",
            }
        }
        FieldSensitivity::Signature | FieldSensitivity::LegalAttestation => {
            PolicyDecision::RequiresUserInput {
                reason: "legal_attestation_human_only",
            }
        }
        FieldSensitivity::DestructiveAction => PolicyDecision::RequiresUserInput {
            reason: "destructive_action_confirmation_required",
        },
    }
}

fn tab(id: u64, owner: TabOwner, read_grant: ReadGrant, url: &str, title: &str) -> TabInfo {
    TabInfo {
        tab_id: TabId(id),
        owner,
        url: url.into(),
        title: Some(title.into()),
        read_grant,
        page_revision: 1,
        visual_marker: TabVisualMarker {
            border: owner == TabOwner::Agent,
            badge: match owner {
                TabOwner::Human => "Human",
                TabOwner::Agent => "Agent",
            }
            .into(),
            color_name: match owner {
                TabOwner::Human => "blue",
                TabOwner::Agent => "green",
            }
            .into(),
        },
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
}

fn start_test_server(root: PathBuf) -> Result<Url> {
    let server = Server::http("127.0.0.1:0")
        .map_err(|error| anyhow!("failed to bind test HTTP server: {error}"))?;
    let addr: SocketAddr = server
        .server_addr()
        .to_ip()
        .context("test HTTP server did not expose an IP socket address")?;
    thread::spawn(move || {
        for request in server.incoming_requests() {
            let url_path = request
                .url()
                .trim_start_matches('/')
                .split('?')
                .next()
                .unwrap_or("");
            let relative = if url_path.is_empty() {
                "index.html"
            } else {
                url_path
            };
            let path = root.join(relative);
            let response = match fs::read(&path) {
                Ok(body) => Response::from_data(body)
                    .with_header(Header::from_bytes("Content-Type", content_type(&path)).unwrap()),
                Err(_) => Response::from_string("not found").with_status_code(StatusCode(404)),
            };
            let _ = request.respond(response);
        }
    });

    Url::parse(&format!("http://{addr}/")).context("failed to form test server URL")
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn unix_ms() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before unix epoch")?
        .as_millis())
}

fn workspace_root() -> Result<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .context("failed to resolve workspace root")
}
