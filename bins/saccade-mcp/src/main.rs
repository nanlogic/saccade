use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use saccade_core::{ReadGrant, TabId, TabInfo, TabOwner, TabVisualMarker};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tiny_http::{Header, Response, Server, StatusCode};
use url::Url;

const REQUIRED_TOOL_COUNT: usize = 20;

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
    browser_backed_tabs: bool,
    tabs_grant_current: bool,
    tabs_grant_artifact: bool,
    web_truth: bool,
    web_actions: bool,
    web_act: bool,
    web_fill_agent_fields: bool,
    web_inspect_fields: bool,
    web_fill_form_live: bool,
    live_worker_audit: bool,
    dev_click_all_primary_actions: bool,
    dev_fill_smoke_form: bool,
    dev_get_report: bool,
    report_validate_run: bool,
    browser_worker_validate_run: bool,
    report_replay_summary: bool,
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

#[derive(Debug, Clone, Serialize)]
struct ArtifactIndexRecord<'a> {
    ts_ms: u128,
    tool: &'a str,
    kind: &'a str,
    summary: &'a str,
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
        && stdio_evidence.browser_backed_tabs
        && stdio_evidence.tabs_grant_current
        && stdio_evidence.tabs_grant_artifact
        && stdio_evidence.web_truth
        && stdio_evidence.web_actions
        && stdio_evidence.web_act
        && stdio_evidence.web_fill_agent_fields
        && stdio_evidence.web_inspect_fields
        && stdio_evidence.web_fill_form_live
        && stdio_evidence.live_worker_audit
        && stdio_evidence.dev_click_all_primary_actions
        && stdio_evidence.dev_fill_smoke_form
        && stdio_evidence.dev_get_report
        && stdio_evidence.report_validate_run
        && stdio_evidence.browser_worker_validate_run
        && stdio_evidence.report_replay_summary;
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
        browser_backed_tabs: stdio_evidence.browser_backed_tabs,
        tabs_grant_current: stdio_evidence.tabs_grant_current,
        tabs_grant_artifact: stdio_evidence.tabs_grant_artifact,
        web_truth: stdio_evidence.web_truth,
        web_actions: stdio_evidence.web_actions,
        web_act: stdio_evidence.web_act,
        web_fill_agent_fields: stdio_evidence.web_fill_agent_fields,
        web_inspect_fields: stdio_evidence.web_inspect_fields,
        web_fill_form_live: stdio_evidence.web_fill_form_live,
        live_worker_audit: stdio_evidence.live_worker_audit,
        dev_click_all_primary_actions: stdio_evidence.dev_click_all_primary_actions,
        dev_fill_smoke_form: stdio_evidence.dev_fill_smoke_form,
        dev_get_report: stdio_evidence.dev_get_report,
        report_validate_run: stdio_evidence.report_validate_run,
        browser_worker_validate_run: stdio_evidence.browser_worker_validate_run,
        report_replay_summary: stdio_evidence.report_replay_summary,
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
                true,
            ),
            tool(
                "saccade.dev.fill_smoke_form",
                ToolNamespace::Dev,
                ToolRisk::PolicyGated,
                "Fill non-sensitive smoke-test fields on a local form and return replay paths.",
                true,
                true,
                true,
            ),
            tool(
                "saccade.dev.get_report",
                ToolNamespace::Dev,
                ToolRisk::ReportOnly,
                "Fetch a compact development audit report by run ID.",
                true,
                false,
                true,
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
                "saccade.tabs.grant_current",
                ToolNamespace::Tabs,
                ToolRisk::PolicyGated,
                "Attach the user-selected current tab to a live co-pilot session after explicit user grant.",
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
                "saccade.web.fill_agent_fields",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Fill explicitly requested Agent-owned non-sensitive fields in a live browser tab.",
                true,
                true,
                true,
            ),
            tool(
                "saccade.web.inspect_fields",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Inspect explicitly named fields while redacting sensitive values.",
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
                true,
            ),
            tool(
                "saccade.report.validate_run",
                ToolNamespace::Report,
                ToolRisk::ReportOnly,
                "Validate a run directory and return compact status plus artifact paths.",
                true,
                false,
                true,
            ),
            tool(
                "saccade.report.replay_summary",
                ToolNamespace::Report,
                ToolRisk::ReportOnly,
                "Summarize replay JSONL without emitting full replay content.",
                true,
                false,
                true,
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
    browser_workers: BTreeMap<u64, BrowserWorkerClient>,
    dogfood_controls: BTreeMap<u64, DogfoodControlEndpoint>,
}

#[derive(Debug, Clone, Serialize)]
struct SessionTab {
    info: TabInfo,
    paused: bool,
    agent_input_grant: bool,
    grant_reason: Option<String>,
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
    browser_backed_tabs: bool,
    tabs_grant_current: bool,
    tabs_grant_artifact: bool,
    web_truth: bool,
    web_actions: bool,
    web_act: bool,
    web_fill_agent_fields: bool,
    web_inspect_fields: bool,
    web_fill_form_live: bool,
    live_worker_audit: bool,
    dev_click_all_primary_actions: bool,
    dev_fill_smoke_form: bool,
    dev_get_report: bool,
    report_validate_run: bool,
    browser_worker_validate_run: bool,
    report_replay_summary: bool,
    audit_report: String,
}

struct BrowserWorkerClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_request_id: u64,
}

impl std::fmt::Debug for BrowserWorkerClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrowserWorkerClient")
            .field("pid", &self.child.id())
            .field("next_request_id", &self.next_request_id)
            .finish()
    }
}

impl BrowserWorkerClient {
    fn spawn(url: &Url) -> Result<Self> {
        let workspace = workspace_root()?;
        let mut child = ProcessCommand::new("cargo")
            .current_dir(&workspace)
            .env("RUST_LOG", "error")
            .args(["run", "-q", "-p", "saccade-shell", "--"])
            .arg("browser-session-worker")
            .arg("--url")
            .arg(url.as_str())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("failed to spawn browser session worker")?;
        let stdin = child
            .stdin
            .take()
            .context("browser session worker stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("browser session worker stdout unavailable")?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_request_id: 0,
        })
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_request_id += 1;
        let id = self.next_request_id;
        writeln!(
            self.stdin,
            "{}",
            json!({
                "id": id,
                "method": method,
                "params": params,
            })
        )
        .context("failed to write browser worker request")?;
        self.stdin
            .flush()
            .context("failed to flush browser worker request")?;

        loop {
            let mut line = String::new();
            let read = self
                .stdout
                .read_line(&mut line)
                .context("failed to read browser worker response")?;
            if read == 0 {
                bail!("browser session worker exited before responding to {method}");
            }
            if line.trim().is_empty() {
                continue;
            }
            let response: Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if response.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if response.get("ok").and_then(Value::as_bool) == Some(true) {
                return Ok(response.get("result").cloned().unwrap_or(Value::Null));
            }
            let error = response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("browser session worker error");
            bail!("{error}");
        }
    }

    fn close(&mut self) {
        let _ = self.call("close", json!({}));
        let _ = self.child.wait();
    }
}

impl Drop for BrowserWorkerClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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
                "engine": {"type": "string", "enum": ["servo", "static", "chrome"], "default": "servo"},
                "replay": {"type": "boolean", "default": true}
            },
            "additionalProperties": false
        }),
        "saccade.dev.click_all_primary_actions" => json!({
            "type": "object",
            "properties": {
                "url": {"type": "string"},
                "tab_id": {"type": "integer"},
                "policy": {
                    "type": "object",
                    "properties": {
                        "max_actions": {"type": "integer", "default": 1},
                        "local_dev_only": {"type": "boolean", "const": true}
                    },
                    "additionalProperties": false
                }
            },
            "additionalProperties": false
        }),
        "saccade.dev.fill_smoke_form" => json!({
            "type": "object",
            "properties": {
                "fixture": {"type": "string", "default": "test_pages/formmax/index.html"},
                "input": {"type": "string"},
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
        "saccade.tabs.grant_current" => json!({
            "type": "object",
            "properties": {
                "url": {"type": "string"},
                "grant_path": {"type": "string"},
                "reason": {"type": "string"},
                "read_grant": {"type": "string", "enum": ["visible_summary_only", "full_truth"], "default": "full_truth"},
                "policy": {
                    "type": "object",
                    "properties": {
                        "local_dev_only": {"type": "boolean", "const": true},
                        "explicit_user_grant": {"type": "boolean", "const": true}
                    },
                    "additionalProperties": false
                }
            },
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
        "saccade.web.fill_agent_fields" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"},
                "basis_page_revision": {"type": "integer"},
                "fields": {
                    "type": "object",
                    "additionalProperties": {
                        "type": ["string", "number", "boolean"]
                    }
                },
                "policy": {
                    "type": "object",
                    "properties": {
                        "agent_owned_only": {"type": "boolean", "const": true},
                        "block_sensitive": {"type": "boolean", "const": true},
                        "live_worker_only": {"type": "boolean", "const": true}
                    },
                    "additionalProperties": false
                }
            },
            "required": ["tab_id", "basis_page_revision", "fields"],
            "additionalProperties": false
        }),
        "saccade.web.inspect_fields" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"},
                "fields": {
                    "type": "array",
                    "items": {"type": "string"},
                    "minItems": 1
                },
                "policy": {
                    "type": "object",
                    "properties": {
                        "redact_sensitive": {"type": "boolean", "const": true},
                        "explicit_fields_only": {"type": "boolean", "const": true},
                        "live_worker_only": {"type": "boolean", "const": true}
                    },
                    "additionalProperties": false
                }
            },
            "required": ["tab_id", "fields"],
            "additionalProperties": false
        }),
        "saccade.web.fill_form" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"},
                "basis_page_revision": {"type": "integer"},
                "fixture": {"type": "string", "default": "test_pages/formmax/index.html"},
                "input": {"type": "string"},
                "replay": {"type": "boolean", "default": true},
                "policy": {
                    "type": "object",
                    "properties": {
                        "block_sensitive": {"type": "boolean", "const": true},
                        "local_fixture_only": {"type": "boolean", "const": true},
                        "live_worker_only": {"type": "boolean", "const": true}
                    },
                    "additionalProperties": false
                }
            },
            "additionalProperties": false
        }),
        "saccade.dev.get_report" => json!({
            "type": "object",
            "properties": {
                "report_path": {"type": "string"}
            },
            "required": ["report_path"],
            "additionalProperties": false
        }),
        "saccade.report.validate_run" => json!({
            "type": "object",
            "properties": {
                "run_dir": {"type": "string"},
                "kind": {"type": "string", "enum": ["generic", "formmax", "browser_session_worker"], "default": "generic"}
            },
            "required": ["run_dir"],
            "additionalProperties": false
        }),
        "saccade.report.replay_summary" => json!({
            "type": "object",
            "properties": {
                "run_dir": {"type": "string"},
                "replay_path": {"type": "string"}
            },
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
        "saccade.dev.click_all_primary_actions" => {
            dev_click_all_primary_actions_tool(state, arguments)
        }
        "saccade.dev.fill_smoke_form" => dev_fill_smoke_form_tool(arguments),
        "saccade.dev.get_report" => dev_get_report_tool(arguments),
        "saccade.tabs.list" => tabs_list_tool(state),
        "saccade.tabs.open" => tabs_open_tool(state, arguments),
        "saccade.tabs.request_user_login" => tabs_request_user_login_tool(state, arguments),
        "saccade.tabs.grant_current" => tabs_grant_current_tool(state, arguments),
        "saccade.tabs.takeover" => tabs_takeover_tool(state, arguments),
        "saccade.tabs.pause_agent" => tabs_pause_agent_tool(state, arguments),
        "saccade.tabs.close" => tabs_close_tool(state, arguments),
        "saccade.web.truth" => web_truth_tool(state, arguments),
        "saccade.web.actions" => web_actions_tool(state, arguments),
        "saccade.web.act" => web_act_tool(state, arguments),
        "saccade.web.fill_agent_fields" => web_fill_agent_fields_tool(state, arguments),
        "saccade.web.inspect_fields" => web_inspect_fields_tool(state, arguments),
        "saccade.web.fill_form" => web_fill_form_tool(state, arguments),
        "saccade.report.validate_run" => report_validate_run_tool(arguments),
        "saccade.report.replay_summary" => report_replay_summary_tool(arguments),
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
    let mut tab = SessionTab {
        info,
        paused: false,
        agent_input_grant: false,
        grant_reason: None,
        last_engine: None,
        last_summary: None,
        last_report_path: None,
        last_replay_path: None,
        last_actions: Vec::new(),
        last_findings: Vec::new(),
    };
    let mut worker = if owner == TabOwner::Agent {
        Some(BrowserWorkerClient::spawn(&url)?)
    } else {
        None
    };
    if let Some(worker) = worker.as_mut() {
        let live_truth = worker.call("truth", json!({}))?;
        update_session_tab_from_browser_result(&mut tab, &live_truth);
    }
    state.tabs.push(tab.clone());
    if let Some(worker) = worker {
        state.browser_workers.insert(tab_id.0, worker);
    }

    Ok(json!({
        "status": "ok",
        "summary": if owner == TabOwner::Agent {
            "local URL opened in live Saccade browser session"
        } else {
            "local URL registered in Saccade MCP session state"
        },
        "runtime": if owner == TabOwner::Agent {
            "browser_session_worker_v0"
        } else {
            "mcp_session_state_v0"
        },
        "tab": tab.info,
        "actions": tab.last_actions,
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
    if !matches!(engine, "servo" | "static" | "chrome") {
        bail!("unsupported DEVMAX engine {engine:?}; expected servo, static, or chrome");
    }
    let replay = arguments
        .get("replay")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    if engine == "servo" {
        if let Some(tab_id) = tab_id {
            if state.browser_workers.contains_key(&tab_id.0) {
                let live_audit = call_browser_worker(state, tab_id, "audit", json!({}))?;
                if let Some(tab) = state.find_tab_mut(tab_id) {
                    update_session_tab_from_browser_result(tab, &live_audit);
                }
                let summary = live_audit
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or("live browser session audit completed")
                    .to_string();
                let live_engine = live_audit
                    .get("engine")
                    .and_then(Value::as_str)
                    .unwrap_or("saccade-browser-session-audit-v0")
                    .to_string();
                let actions = live_audit
                    .get("actions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let findings = live_audit
                    .get("findings")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let artifacts = live_audit
                    .get("artifacts")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let artifact_index = record_artifact_index(
                    "saccade.dev.audit_page",
                    "browser_worker_audit",
                    &summary,
                    artifacts.clone(),
                )?;
                return Ok(json!({
                    "status": "ok",
                    "summary": summary,
                    "tool": "saccade.dev.audit_page",
                    "runtime": live_audit.get("runtime").cloned().unwrap_or_else(|| json!("browser_session_worker_v0")),
                    "engine": live_engine,
                    "url": live_audit.get("url").cloned().unwrap_or_else(|| json!(url.as_str())),
                    "tab_id": tab_id.0,
                    "page_revision": live_audit.get("page_revision").cloned().unwrap_or(Value::Null),
                    "title": live_audit.get("title").cloned().unwrap_or(Value::Null),
                    "findings": findings.len(),
                    "actions": actions.len(),
                    "action_map": actions,
                    "finding_list": findings,
                    "artifacts": artifacts,
                    "artifact_index": artifact_index,
                }));
            }
        }
    }

    let devmax = run_devmax_audit(&url, engine, replay)?;
    if let Some(tab_id) = tab_id {
        update_tab_from_devmax(state, tab_id, &devmax)?;
    }
    let artifacts = devmax.artifacts.clone();
    let artifact_index = record_artifact_index(
        "saccade.dev.audit_page",
        "devmax_audit",
        &devmax.summary,
        artifacts.clone(),
    )?;
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
        "artifacts": artifacts,
        "artifact_index": artifact_index,
    }))
}

fn dev_click_all_primary_actions_tool(
    state: &mut McpSessionState,
    arguments: Value,
) -> Result<Value> {
    let (tab_id, url) = resolve_tab_or_url(state, &arguments)?;
    if !is_local_dev_url(&url) {
        bail!("saccade.dev.click_all_primary_actions only accepts local dev URLs: {url}");
    }
    let max_actions = arguments
        .pointer("/policy/max_actions")
        .and_then(Value::as_u64)
        .unwrap_or(1) as usize;
    if arguments
        .pointer("/policy/local_dev_only")
        .and_then(Value::as_bool)
        .is_some_and(|enabled| !enabled)
    {
        bail!("saccade.dev.click_all_primary_actions v0 requires local_dev_only=true");
    }

    let devmax = run_devmax_audit(&url, "servo", true)?;
    if let Some(tab_id) = tab_id {
        update_tab_from_devmax(state, tab_id, &devmax)?;
    }
    if devmax.action_map.len() > max_actions {
        bail!(
            "click_all_primary_actions v0 refuses {} actions; max_actions={}",
            devmax.action_map.len(),
            max_actions
        );
    }
    let artifacts = devmax.artifacts.clone();
    let artifact_index = record_artifact_index(
        "saccade.dev.click_all_primary_actions",
        "devmax_click_verification",
        &devmax.summary,
        artifacts.clone(),
    )?;

    Ok(json!({
        "status": "ok",
        "summary": "primary local-dev actions verified through Servo-backed DEVMAX audit",
        "url": url.as_str(),
        "tab_id": tab_id.map(|id| id.0),
        "actions_seen": devmax.action_map.len(),
        "actions_verified": devmax.action_map.len(),
        "actions": devmax.action_map,
        "findings": devmax.findings,
        "artifacts": artifacts,
        "artifact_index": artifact_index,
    }))
}

fn dev_fill_smoke_form_tool(arguments: Value) -> Result<Value> {
    web_fill_form_static_tool(arguments)
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
    artifacts: Value,
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
    let mut artifacts = report
        .get("artifacts")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if let Some(map) = artifacts.as_object_mut() {
        map.insert("report".into(), json!(report_path.clone()));
        map.insert("replay".into(), json!(replay_path.clone()));
    }

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
        artifacts,
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
        agent_input_grant: false,
        grant_reason: Some(reason.to_string()),
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

#[derive(Debug, Clone)]
struct CurrentTabGrantRequest {
    url: Url,
    reason: String,
    read_grant: ReadGrant,
    source: &'static str,
    grant_path: Option<String>,
    control_endpoint: Option<DogfoodControlEndpoint>,
}

#[derive(Debug, Clone)]
struct DogfoodControlEndpoint {
    host: String,
    port: u16,
    protocol: String,
}

fn tabs_grant_current_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let grant = current_tab_grant_from_args(&arguments)?;
    let same_webview_control = grant
        .control_endpoint
        .as_ref()
        .map(call_dogfood_control_ping)
        .transpose()?;
    let same_webview_control_ping = same_webview_control.is_some();

    let tab_id = state.allocate_tab_id();
    let info = tab(
        tab_id.0,
        TabOwner::Human,
        grant.read_grant,
        grant.url.as_str(),
        "Current Tab Co-Pilot",
    );
    let mut tab = SessionTab {
        info,
        paused: false,
        agent_input_grant: true,
        grant_reason: Some(grant.reason.clone()),
        last_engine: None,
        last_summary: None,
        last_report_path: None,
        last_replay_path: None,
        last_actions: Vec::new(),
        last_findings: Vec::new(),
    };
    let (live_truth, attached_via_control) = if let Some(endpoint) = grant.control_endpoint.as_ref()
    {
        let live_truth = call_dogfood_control(endpoint, "truth", json!({}))?;
        state.dogfood_controls.insert(tab_id.0, endpoint.clone());
        (live_truth, true)
    } else {
        let mut worker = BrowserWorkerClient::spawn(&grant.url)?;
        let live_truth = worker.call("truth", json!({}))?;
        state.browser_workers.insert(tab_id.0, worker);
        (live_truth, false)
    };
    update_session_tab_from_browser_result(&mut tab, &live_truth);
    state.tabs.push(tab.clone());
    let transport_status = if attached_via_control {
        "same_webview_control_truth_v0"
    } else if grant.source == "grant_artifact" && same_webview_control_ping {
        "same_webview_control_ping_plus_worker_truth_v0"
    } else if grant.source == "grant_artifact" {
        "worker_from_grant_artifact_v0"
    } else {
        "worker_from_direct_url_grant_v0"
    };

    Ok(json!({
        "status": "ok",
        "summary": "current Human tab attached to live Saccade co-pilot session after explicit grant",
        "runtime": tab_runtime(state, tab_id),
        "selected_tab_seen": true,
        "grant_required": true,
        "grant_given": true,
        "agent_input_grant": true,
        "reason": grant.reason,
        "source": grant.source,
        "grant_path": grant.grant_path,
        "same_webview_control_ping": same_webview_control_ping,
        "same_webview_control": same_webview_control,
        "same_webview_attached": attached_via_control,
        "same_webview_capabilities": if attached_via_control { json!(["ping", "truth", "actions"]) } else { json!([]) },
        "transport_status": transport_status,
        "transport_note": if attached_via_control {
            "MCP v0 validates the visible browser grant and reads redacted truth/actions from the same dogfood WebView control endpoint. Fill/act still need the next bridge step."
        } else {
            "MCP v0 validates the visible browser grant and can ping the same dogfood WebView control endpoint when present. Truth/actions use a live worker when no control endpoint is available."
        },
        "tab": tab.info,
        "truth": {
            "engine": tab.last_engine.clone(),
            "findings_count": tab.last_findings.len(),
            "actions_count": tab.last_actions.len(),
            "findings": tab.last_findings.clone(),
        },
        "actions": tab.last_actions.clone(),
        "artifacts": {
            "report": tab.last_report_path.clone(),
            "replay": tab.last_replay_path.clone(),
        }
    }))
}

fn current_tab_grant_from_args(arguments: &Value) -> Result<CurrentTabGrantRequest> {
    validate_current_tab_grant_policy(arguments)?;
    if arguments.get("grant_path").is_some() {
        current_tab_grant_from_artifact(arguments)
    } else {
        current_tab_grant_from_direct_args(arguments)
    }
}

fn validate_current_tab_grant_policy(arguments: &Value) -> Result<()> {
    if let Some(policy) = arguments.get("policy") {
        if policy
            .get("local_dev_only")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("saccade.tabs.grant_current v0 requires local_dev_only=true");
        }
        if policy
            .get("explicit_user_grant")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("saccade.tabs.grant_current requires explicit_user_grant=true");
        }
    }
    Ok(())
}

fn current_tab_grant_from_direct_args(arguments: &Value) -> Result<CurrentTabGrantRequest> {
    let url = required_url_arg(arguments)?;
    if !is_local_dev_url(&url) {
        bail!("grant_current v0 only accepts localhost, loopback, or file URLs: {url}");
    }
    let reason = arguments
        .get("reason")
        .and_then(Value::as_str)
        .context("tool arguments must include string field reason when grant_path is absent")?
        .trim();
    if reason.is_empty() {
        bail!("grant_current reason must not be empty");
    }
    Ok(CurrentTabGrantRequest {
        url,
        reason: reason.to_string(),
        read_grant: read_grant_from_grant_value(
            arguments.get("read_grant").and_then(Value::as_str),
        )?,
        source: "direct_url",
        grant_path: None,
        control_endpoint: None,
    })
}

fn current_tab_grant_from_artifact(arguments: &Value) -> Result<CurrentTabGrantRequest> {
    let grant_path_arg = arguments
        .get("grant_path")
        .and_then(Value::as_str)
        .context("grant_path must be a string")?;
    let grant_path = safe_workspace_path(grant_path_arg)?;
    let grant: Value = serde_json::from_slice(
        &fs::read(&grant_path)
            .with_context(|| format!("failed to read {}", grant_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", grant_path.display()))?;

    if grant.get("status").and_then(Value::as_str) != Some("granted") {
        bail!("grant artifact status is not granted");
    }
    if grant.get("grant_type").and_then(Value::as_str) != Some("current_tab_copilot") {
        bail!("grant artifact is not a current_tab_copilot grant");
    }
    if grant.get("selected_tab_seen").and_then(Value::as_bool) != Some(true)
        || grant.get("grant_required").and_then(Value::as_bool) != Some(true)
        || grant.get("grant_given").and_then(Value::as_bool) != Some(true)
    {
        bail!("grant artifact is missing selected-tab grant evidence");
    }
    if grant.get("owner").and_then(Value::as_str) != Some("Human") {
        bail!("grant artifact owner must be Human");
    }
    if grant.get("agent_input_grant").and_then(Value::as_bool) != Some(true) {
        bail!("grant artifact does not allow agent co-pilot input");
    }

    let url_str = grant
        .get("url")
        .and_then(Value::as_str)
        .context("grant artifact is missing string url")?;
    let url =
        Url::parse(url_str).with_context(|| format!("invalid grant artifact URL: {url_str}"))?;
    if !is_local_dev_url(&url) {
        bail!("grant artifact URL must be localhost, loopback, or file URL: {url}");
    }
    let reason = arguments
        .get("reason")
        .and_then(Value::as_str)
        .filter(|reason| !reason.trim().is_empty())
        .map(str::trim)
        .unwrap_or("dogfood browser current-tab grant artifact");
    let read_grant = read_grant_from_grant_value(grant.get("read_grant").and_then(Value::as_str))?;
    let control_endpoint = dogfood_control_endpoint_from_grant(&grant)?;

    Ok(CurrentTabGrantRequest {
        url,
        reason: reason.to_string(),
        read_grant,
        source: "grant_artifact",
        grant_path: Some(grant_path.display().to_string()),
        control_endpoint,
    })
}

fn dogfood_control_endpoint_from_grant(grant: &Value) -> Result<Option<DogfoodControlEndpoint>> {
    let Some(endpoint) = grant
        .get("control_endpoint")
        .filter(|value| !value.is_null())
    else {
        return Ok(None);
    };
    let protocol = endpoint
        .get("protocol")
        .and_then(Value::as_str)
        .context("control_endpoint must include string protocol")?;
    if protocol != "saccade-dogfood-control-v0" {
        bail!("unsupported control endpoint protocol {protocol:?}");
    }
    let scheme = endpoint
        .get("scheme")
        .and_then(Value::as_str)
        .context("control_endpoint must include string scheme")?;
    if scheme != "tcp" {
        bail!("unsupported control endpoint scheme {scheme:?}");
    }
    let host = endpoint
        .get("host")
        .and_then(Value::as_str)
        .context("control_endpoint must include string host")?;
    if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
        bail!("control endpoint host must be loopback; got {host:?}");
    }
    let port = endpoint
        .get("port")
        .and_then(Value::as_u64)
        .context("control_endpoint must include integer port")?;
    if port == 0 || port > u16::MAX as u64 {
        bail!("control endpoint port is out of range: {port}");
    }
    Ok(Some(DogfoodControlEndpoint {
        host: host.to_string(),
        port: port as u16,
        protocol: protocol.to_string(),
    }))
}

fn call_dogfood_control_ping(endpoint: &DogfoodControlEndpoint) -> Result<Value> {
    call_dogfood_control(
        endpoint,
        "ping",
        json!({
            "protocol": endpoint.protocol,
        }),
    )
}

fn call_dogfood_control(
    endpoint: &DogfoodControlEndpoint,
    method: &str,
    params: Value,
) -> Result<Value> {
    let addr = dogfood_control_socket_addr(endpoint)?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
        .with_context(|| format!("failed to connect dogfood control endpoint {addr}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .context("failed to set dogfood control read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .context("failed to set dogfood control write timeout")?;
    writeln!(
        stream,
        "{}",
        json!({
            "id": 1,
            "method": method,
            "params": params,
        })
    )
    .with_context(|| format!("failed to write dogfood control {method}"))?;
    stream
        .flush()
        .with_context(|| format!("failed to flush dogfood control {method}"))?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .with_context(|| format!("failed to read dogfood control {method} response"))?;
    let response: Value = serde_json::from_str(&line)
        .with_context(|| format!("failed to parse dogfood control {method} response"))?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        let error = response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("dogfood control request failed");
        bail!("{error}");
    }
    Ok(response.get("result").cloned().unwrap_or(Value::Null))
}

fn dogfood_control_socket_addr(endpoint: &DogfoodControlEndpoint) -> Result<SocketAddr> {
    let ip = match endpoint.host.as_str() {
        "127.0.0.1" | "localhost" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        "::1" => IpAddr::V6(Ipv6Addr::LOCALHOST),
        other => bail!("control endpoint host must be loopback; got {other:?}"),
    };
    Ok(SocketAddr::new(ip, endpoint.port))
}

fn read_grant_from_grant_value(value: Option<&str>) -> Result<ReadGrant> {
    match value.unwrap_or("full_truth") {
        "visible_summary_only" | "VisibleSummaryOnly" => Ok(ReadGrant::VisibleSummaryOnly),
        "full_truth" | "FullTruth" => Ok(ReadGrant::FullTruth),
        other => bail!(
            "unsupported read_grant {other:?}; expected full_truth, FullTruth, visible_summary_only, or VisibleSummaryOnly"
        ),
    }
}

fn tabs_takeover_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    if let Some(mut worker) = state.browser_workers.remove(&tab_id.0) {
        worker.close();
    }
    let tab = state
        .find_tab_mut(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    tab.info.owner = TabOwner::Human;
    tab.paused = true;
    tab.agent_input_grant = false;
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
    tab.agent_input_grant = false;
    Ok(json!({
        "status": "ok",
        "summary": "agent paused for tab",
        "tab": tab,
    }))
}

fn tabs_close_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    if let Some(mut worker) = state.browser_workers.remove(&tab_id.0) {
        worker.close();
    }
    state.dogfood_controls.remove(&tab_id.0);
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
    if let Some(endpoint) = state.dogfood_controls.get(&tab_id.0).cloned() {
        let live_truth = call_dogfood_control(&endpoint, "truth", json!({}))?;
        if let Some(tab) = state.find_tab_mut(tab_id) {
            update_session_tab_from_browser_result(tab, &live_truth);
        }
    } else if state.browser_workers.contains_key(&tab_id.0) {
        let live_truth = call_browser_worker(state, tab_id, "truth", json!({}))?;
        if let Some(tab) = state.find_tab_mut(tab_id) {
            update_session_tab_from_browser_result(tab, &live_truth);
        }
    } else {
        ensure_tab_report(state, tab_id, engine_arg(&arguments)?)?;
    }
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
        "runtime": tab_runtime(state, tab_id),
        "artifacts": {
            "report": tab.last_report_path,
            "replay": tab.last_replay_path,
        }
    }))
}

fn web_actions_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    ensure_truth_allowed(state, tab_id)?;
    if let Some(endpoint) = state.dogfood_controls.get(&tab_id.0).cloned() {
        let live_actions = call_dogfood_control(&endpoint, "actions", json!({}))?;
        if let Some(tab) = state.find_tab_mut(tab_id) {
            update_session_tab_from_browser_result(tab, &live_actions);
        }
    } else if state.browser_workers.contains_key(&tab_id.0) {
        let live_actions = call_browser_worker(state, tab_id, "actions", json!({}))?;
        if let Some(tab) = state.find_tab_mut(tab_id) {
            update_session_tab_from_browser_result(tab, &live_actions);
        }
    } else {
        ensure_tab_report(state, tab_id, engine_arg(&arguments)?)?;
    }
    let tab = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    Ok(json!({
        "status": "ok",
        "summary": format!("{} action(s) in current action map", tab.last_actions.len()),
        "tab_id": tab_id.0,
        "page_revision": tab.info.page_revision,
        "actions": tab.last_actions,
        "runtime": tab_runtime(state, tab_id),
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
    if action_requires_user_confirmation(state, tab_id, &action_id)? {
        bail!(
            "user confirmation required before action {action_id:?} on a user-granted current tab"
        );
    }
    ensure_agent_input_allowed(state, tab_id)?;
    if let Some(endpoint) = state.dogfood_controls.get(&tab_id.0).cloned() {
        let live_act = call_dogfood_control(
            &endpoint,
            "act",
            json!({
                "action_id": action_id.clone(),
                "basis_page_revision": basis_page_revision,
            }),
        )?;
        if let Some(tab) = state.find_tab_mut(tab_id) {
            update_session_tab_from_browser_result(tab, &live_act);
        }
        let tab = state
            .find_tab(tab_id)
            .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
        return Ok(json!({
            "status": "ok",
            "summary": "action dispatched through same dogfood WebView",
            "runtime": tab_runtime(state, tab_id),
            "tab_id": tab_id.0,
            "action_id": action_id,
            "basis_page_revision": basis_page_revision,
            "new_page_revision": tab.info.page_revision,
            "verification": live_act.get("verification").cloned().unwrap_or(Value::Null),
            "artifacts": {
                "report": tab.last_report_path,
                "replay": tab.last_replay_path,
            },
        }));
    } else if state.browser_workers.contains_key(&tab_id.0) {
        let live_act = call_browser_worker(
            state,
            tab_id,
            "act",
            json!({
                "action_id": action_id.clone(),
                "basis_page_revision": basis_page_revision,
            }),
        )?;
        if let Some(tab) = state.find_tab_mut(tab_id) {
            update_session_tab_from_browser_result(tab, &live_act);
        }
        let tab = state
            .find_tab(tab_id)
            .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
        return Ok(json!({
            "status": "ok",
            "summary": "action dispatched through live Saccade browser session",
            "runtime": "browser_session_worker_v0",
            "tab_id": tab_id.0,
            "action_id": action_id,
            "basis_page_revision": basis_page_revision,
            "new_page_revision": tab.info.page_revision,
            "verification": live_act.get("verification").cloned().unwrap_or(Value::Null),
            "artifacts": {
                "report": tab.last_report_path,
                "replay": tab.last_replay_path,
            },
        }));
    }
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
    let artifacts = devmax.artifacts.clone();
    let artifact_index = record_artifact_index(
        "saccade.web.act",
        "web_action_verification",
        &devmax.summary,
        artifacts.clone(),
    )?;
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
        },
        "artifact_index": artifact_index,
    }))
}

fn web_fill_agent_fields_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    let basis_page_revision = arguments
        .get("basis_page_revision")
        .and_then(Value::as_u64)
        .context("tool arguments must include integer field basis_page_revision")?;
    let Some(fields) = arguments.get("fields").and_then(Value::as_object) else {
        bail!("tool arguments must include object field fields");
    };
    if fields.is_empty() {
        bail!("fields must contain at least one field");
    }
    if let Some(policy) = arguments.get("policy") {
        if policy
            .get("agent_owned_only")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("saccade.web.fill_agent_fields requires agent_owned_only=true");
        }
        if policy
            .get("block_sensitive")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("saccade.web.fill_agent_fields requires block_sensitive=true");
        }
        if policy
            .get("live_worker_only")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!(
                "saccade.web.fill_agent_fields requires live_worker_only=true for live browser sessions"
            );
        }
    }

    ensure_agent_input_allowed(state, tab_id)?;
    let has_live_session = state.browser_workers.contains_key(&tab_id.0)
        || state.dogfood_controls.contains_key(&tab_id.0);
    if !has_live_session {
        bail!("saccade.web.fill_agent_fields requires a live browser session tab");
    }
    let current_revision = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?
        .info
        .page_revision;
    if basis_page_revision != current_revision {
        bail!(
            "stale fill basis: requested {}, current {}",
            basis_page_revision,
            current_revision
        );
    }

    let live_fill = if let Some(endpoint) = state.dogfood_controls.get(&tab_id.0).cloned() {
        call_dogfood_control(
            &endpoint,
            "fill_agent_fields",
            json!({
                "fields": Value::Object(fields.clone()),
            }),
        )?
    } else {
        call_browser_worker(
            state,
            tab_id,
            "fill_agent_fields",
            json!({
                "fields": Value::Object(fields.clone()),
            }),
        )?
    };
    if let Some(tab) = state.find_tab_mut(tab_id) {
        update_session_tab_from_browser_result(tab, &live_fill);
    }
    let tab = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    Ok(json!({
        "status": "ok",
        "summary": "agent-owned non-sensitive fields filled through live Saccade browser session",
        "runtime": tab_runtime(state, tab_id),
        "tab_id": tab_id.0,
        "basis_page_revision": basis_page_revision,
        "new_page_revision": tab.info.page_revision,
        "filled": live_fill.get("filled").cloned().unwrap_or_else(|| json!([])),
        "rejected": live_fill.get("rejected").cloned().unwrap_or_else(|| json!([])),
        "sensitive_fields_seen": live_fill.get("sensitive_fields_seen").cloned().unwrap_or(Value::Null),
        "policy": {
            "agent_owned_only": true,
            "block_sensitive": true,
            "live_worker_only": true,
            "values_logged": false,
        },
        "artifacts": {
            "report": tab.last_report_path,
            "replay": tab.last_replay_path,
        },
    }))
}

fn web_inspect_fields_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    let Some(fields) = arguments.get("fields").and_then(Value::as_array) else {
        bail!("tool arguments must include array field fields");
    };
    if fields.is_empty() {
        bail!("fields must contain at least one field");
    }
    if fields.iter().any(|field| field.as_str().is_none()) {
        bail!("fields must contain only string field IDs");
    }
    if let Some(policy) = arguments.get("policy") {
        if policy
            .get("redact_sensitive")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("saccade.web.inspect_fields requires redact_sensitive=true");
        }
        if policy
            .get("explicit_fields_only")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("saccade.web.inspect_fields requires explicit_fields_only=true");
        }
        if policy
            .get("live_worker_only")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!(
                "saccade.web.inspect_fields requires live_worker_only=true for live browser sessions"
            );
        }
    }

    ensure_truth_allowed(state, tab_id)?;
    let has_live_session = state.browser_workers.contains_key(&tab_id.0)
        || state.dogfood_controls.contains_key(&tab_id.0);
    if !has_live_session {
        bail!("saccade.web.inspect_fields requires a live browser session tab");
    }
    let live_inspect = if let Some(endpoint) = state.dogfood_controls.get(&tab_id.0).cloned() {
        call_dogfood_control(
            &endpoint,
            "inspect_fields",
            json!({
                "fields": Value::Array(fields.clone()),
            }),
        )?
    } else {
        call_browser_worker(
            state,
            tab_id,
            "inspect_fields",
            json!({
                "fields": Value::Array(fields.clone()),
            }),
        )?
    };
    if let Some(tab) = state.find_tab_mut(tab_id) {
        update_session_tab_from_browser_result(tab, &live_inspect);
    }
    let tab = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    let inspected = live_inspect
        .get("fields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let values_returned = inspected
        .iter()
        .filter(|field| {
            field
                .get("value_returned")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let values_redacted = inspected
        .iter()
        .filter(|field| {
            field
                .get("value_redacted")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    Ok(json!({
        "status": "ok",
        "summary": "explicit field inspection completed through live Saccade browser session",
        "runtime": tab_runtime(state, tab_id),
        "tab_id": tab_id.0,
        "page_revision": tab.info.page_revision,
        "fields": inspected,
        "values_returned": values_returned,
        "values_redacted": values_redacted,
        "sensitive_fields_seen": live_inspect.get("sensitive_fields_seen").cloned().unwrap_or(Value::Null),
        "policy": {
            "redact_sensitive": true,
            "explicit_fields_only": true,
            "live_worker_only": true,
            "values_logged": false,
        },
        "artifacts": {
            "report": tab.last_report_path,
            "replay": tab.last_replay_path,
        },
    }))
}

fn web_fill_form_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    if arguments.get("tab_id").is_some() {
        return web_fill_form_live_tool(state, arguments);
    }
    web_fill_form_static_tool(arguments)
}

fn web_fill_form_live_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    let basis_page_revision = arguments
        .get("basis_page_revision")
        .and_then(Value::as_u64)
        .context("live saccade.web.fill_form requires integer basis_page_revision")?;
    if let Some(policy) = arguments.get("policy") {
        if policy
            .get("block_sensitive")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("live saccade.web.fill_form requires block_sensitive=true");
        }
        if policy
            .get("local_fixture_only")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("live saccade.web.fill_form requires local_fixture_only=true");
        }
        if policy
            .get("live_worker_only")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("live saccade.web.fill_form requires live_worker_only=true");
        }
    }

    ensure_agent_input_allowed(state, tab_id)?;
    if !state.browser_workers.contains_key(&tab_id.0) {
        bail!("live saccade.web.fill_form requires a live browser worker tab");
    }
    let current_revision = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?
        .info
        .page_revision;
    if basis_page_revision != current_revision {
        bail!(
            "stale form fill basis: requested {}, current {}",
            basis_page_revision,
            current_revision
        );
    }

    let live_fill = call_browser_worker(
        state,
        tab_id,
        "formmax_live_fill",
        json!({
            "policy": {
                "block_sensitive": true,
                "local_fixture_only": true,
            }
        }),
    )?;
    if let Some(tab) = state.find_tab_mut(tab_id) {
        update_session_tab_from_browser_result(tab, &live_fill);
    }
    let tab = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;

    Ok(json!({
        "status": "ok",
        "summary": "FORMMAX capacity fixture filled and validated through the live Saccade browser session",
        "runtime": "browser_session_worker_v0",
        "engine": live_fill.get("engine").cloned().unwrap_or_else(|| json!("saccade-browser-session-formmax-live-v0")),
        "tab_id": tab_id.0,
        "basis_page_revision": basis_page_revision,
        "new_page_revision": tab.info.page_revision,
        "rows": live_fill.get("rows").cloned().unwrap_or(Value::Null),
        "pages": live_fill.get("pages").cloned().unwrap_or(Value::Null),
        "filled": live_fill.get("filled").cloned().unwrap_or(Value::Null),
        "blocked_sensitive": live_fill.get("blocked_sensitive").cloned().unwrap_or(Value::Null),
        "receipt_verified": live_fill.get("receipt_verified").cloned().unwrap_or(Value::Null),
        "validation_errors": live_fill.get("validation_errors").cloned().unwrap_or(Value::Null),
        "receipt": live_fill.get("receipt").cloned().unwrap_or(Value::Null),
        "policy": {
            "block_sensitive": true,
            "local_fixture_only": true,
            "live_worker_only": true,
            "same_live_tab": true,
            "values_logged": false,
        },
        "artifacts": {
            "report": tab.last_report_path,
            "replay": tab.last_replay_path,
        },
    }))
}

fn web_fill_form_static_tool(arguments: Value) -> Result<Value> {
    let fixture = arguments
        .get("fixture")
        .and_then(Value::as_str)
        .unwrap_or("test_pages/formmax/index.html");
    let fixture = safe_workspace_path(fixture)?;
    let input = arguments
        .get("input")
        .and_then(Value::as_str)
        .map(safe_workspace_path)
        .transpose()?;
    let replay = arguments
        .get("replay")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    if let Some(policy) = arguments.get("policy") {
        if policy
            .get("block_sensitive")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("saccade.web.fill_form v0 requires block_sensitive=true");
        }
        if policy
            .get("local_fixture_only")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("saccade.web.fill_form v0 requires local_fixture_only=true");
        }
    }

    let workspace = workspace_root()?;
    let mut command = ProcessCommand::new("cargo");
    command
        .current_dir(&workspace)
        .args(["run", "-q", "-p", "formmax", "--", "run"])
        .arg("--fixture")
        .arg(&fixture);
    if let Some(input) = input.as_ref() {
        command.arg("--input").arg(input);
    }
    if replay {
        command.arg("--replay");
    }

    let output = command.output().context("failed to spawn formmax run")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        bail!(
            "formmax run failed: status={} stdout={} stderr={}",
            output.status,
            stdout.trim(),
            stderr.trim()
        );
    }

    let replay_path = parse_output_value(&stdout, "replay=")
        .filter(|path| path != "disabled")
        .context("formmax run output did not include replay path")?;
    let replay_path = safe_workspace_path(&replay_path)?;
    let run_dir = replay_path
        .parent()
        .map(Path::to_path_buf)
        .context("replay path has no parent")?;
    let validation = validate_formmax_run(&run_dir)?;
    let result_path = run_dir.join("result.json");
    let result_text = fs::read_to_string(&result_path)
        .with_context(|| format!("failed to read {}", result_path.display()))?;
    let result: Value = serde_json::from_str(&result_text)
        .with_context(|| format!("invalid result JSON {}", result_path.display()))?;
    let artifacts = json!({
        "result": result_path.display().to_string(),
        "replay": replay_path.display().to_string(),
        "screenshots": result.get("screenshots"),
    });
    let artifact_index = record_artifact_index(
        "saccade.web.fill_form",
        "formmax_fill",
        "FORMMAX local fixture filled and validated",
        artifacts.clone(),
    )?;

    Ok(json!({
        "status": "ok",
        "summary": "FORMMAX local fixture filled and validated",
        "policy": {
            "block_sensitive": true,
            "local_fixture_only": true,
        },
        "rows": result.get("rows"),
        "pages": result.get("pages"),
        "filled": result.get("filled"),
        "blocked_sensitive": result.get("blocked_sensitive"),
        "native_input": result.get("native_input"),
        "receipt_verified": result.get("receipt_verified"),
        "validation": validation,
        "artifacts": artifacts,
        "artifact_index": artifact_index,
    }))
}

fn dev_get_report_tool(arguments: Value) -> Result<Value> {
    let report_path = arguments
        .get("report_path")
        .and_then(Value::as_str)
        .context("tool arguments must include string field report_path")?;
    let report_path = safe_workspace_path(report_path)?;
    let report_text = fs::read_to_string(&report_path)
        .with_context(|| format!("failed to read {}", report_path.display()))?;
    let report: Value = serde_json::from_str(&report_text)
        .with_context(|| format!("invalid report JSON {}", report_path.display()))?;

    Ok(json!({
        "status": "ok",
        "summary": report
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or("report loaded"),
        "engine": report.get("engine"),
        "title": report.get("title"),
        "url": report.get("url"),
        "page_revision": report.get("page_revision"),
        "findings_count": report
            .get("findings")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        "actions_count": report
            .get("actions")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        "artifacts": {
            "report": report_path.display().to_string(),
            "replay": report
                .get("artifacts")
                .and_then(|artifacts| artifacts.get("replay")),
        }
    }))
}

fn report_validate_run_tool(arguments: Value) -> Result<Value> {
    let run_dir = arguments
        .get("run_dir")
        .and_then(Value::as_str)
        .context("tool arguments must include string field run_dir")?;
    let kind = arguments
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("generic");
    let run_dir = safe_workspace_path(run_dir)?;

    match kind {
        "generic" => validate_generic_run(&run_dir),
        "formmax" => validate_formmax_run(&run_dir),
        "browser_session_worker" => validate_browser_session_worker_run(&run_dir),
        other => bail!("unsupported validation kind {other:?}"),
    }
}

fn validate_generic_run(run_dir: &Path) -> Result<Value> {
    let report_path = run_dir.join("report.json");
    let result_path = run_dir.join("result.json");
    let replay_path = run_dir.join("replay.jsonl");
    let primary_report = if report_path.exists() {
        Some(report_path)
    } else if result_path.exists() {
        Some(result_path)
    } else {
        None
    };
    let Some(primary_report) = primary_report else {
        bail!(
            "run directory has no report.json or result.json: {}",
            run_dir.display()
        );
    };

    let report_text = fs::read_to_string(&primary_report)
        .with_context(|| format!("failed to read {}", primary_report.display()))?;
    let report: Value = serde_json::from_str(&report_text)
        .with_context(|| format!("invalid JSON {}", primary_report.display()))?;
    Ok(json!({
        "status": "ok",
        "summary": "generic run artifact check passed",
        "run_dir": run_dir.display().to_string(),
        "engine": report.get("engine"),
        "has_replay": replay_path.exists(),
        "artifacts": {
            "report": primary_report.display().to_string(),
            "replay": if replay_path.exists() { Some(replay_path.display().to_string()) } else { None },
        }
    }))
}

fn validate_formmax_run(run_dir: &Path) -> Result<Value> {
    let workspace = workspace_root()?;
    let output = ProcessCommand::new("cargo")
        .current_dir(&workspace)
        .args(["run", "-q", "-p", "formmax", "--", "validate-run"])
        .arg(run_dir)
        .output()
        .context("failed to spawn formmax validate-run")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        bail!(
            "formmax validate-run failed: status={} stdout={} stderr={}",
            output.status,
            stdout.trim(),
            stderr.trim()
        );
    }

    Ok(json!({
        "status": "ok",
        "summary": stdout.trim(),
        "run_dir": run_dir.display().to_string(),
        "artifacts": {
            "result": run_dir.join("result.json").display().to_string(),
            "replay": run_dir.join("replay.jsonl").display().to_string(),
        }
    }))
}

fn validate_browser_session_worker_run(run_dir: &Path) -> Result<Value> {
    let report_path = run_dir.join("report.json");
    let replay_path = run_dir.join("replay.jsonl");
    if !report_path.exists() {
        bail!(
            "browser session worker report missing: {}",
            report_path.display()
        );
    }
    if !replay_path.exists() {
        bail!(
            "browser session worker replay missing: {}",
            replay_path.display()
        );
    }

    let report_text = fs::read_to_string(&report_path)
        .with_context(|| format!("failed to read {}", report_path.display()))?;
    let report: Value = serde_json::from_str(&report_text)
        .with_context(|| format!("invalid JSON {}", report_path.display()))?;
    let engine_ok =
        report.get("engine").and_then(Value::as_str) == Some("saccade-browser-session-worker-v0");
    let screenshots = report
        .pointer("/artifacts/screenshots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let missing_screenshots = screenshots
        .iter()
        .filter_map(Value::as_str)
        .filter(|path| !safe_workspace_path(path).is_ok_and(|path| path.exists()))
        .count();
    let skipped_sensitive = report
        .pointer("/artifacts/screenshot_skipped_sensitive")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let file = fs::File::open(&replay_path)
        .with_context(|| format!("failed to open {}", replay_path.display()))?;
    let mut events = 0usize;
    let mut truth_events = 0usize;
    let mut action_verified = 0usize;
    let mut screenshot_saved = 0usize;
    let mut screenshot_skipped = 0usize;
    let mut raw_value_leaks = 0usize;
    for line in BufReader::new(file).lines() {
        let line = line.context("failed to read replay line")?;
        if line.trim().is_empty() {
            continue;
        }
        events += 1;
        if line.contains("123-45-6789")
            || line.contains("4111111111111111")
            || line.contains("correct-horse-battery")
        {
            raw_value_leaks += 1;
        }
        let event: Value = serde_json::from_str(&line)
            .with_context(|| format!("invalid replay JSON in {}", replay_path.display()))?;
        match event.get("kind").and_then(Value::as_str).unwrap_or("") {
            "truth_collected" | "actions_collected" => truth_events += 1,
            "action_verified" => action_verified += 1,
            "screenshot_saved" => screenshot_saved += 1,
            "screenshot_skipped_sensitive_fields" => screenshot_skipped += 1,
            _ => {}
        }
    }

    if !engine_ok {
        bail!("browser session worker report has wrong engine");
    }
    if events == 0 || truth_events == 0 {
        bail!("browser session worker replay missing truth/actions events");
    }
    if missing_screenshots > 0 {
        bail!("browser session worker report references missing screenshot(s)");
    }
    if raw_value_leaks > 0 {
        bail!("browser session worker replay contains raw sensitive values");
    }

    Ok(json!({
        "status": "ok",
        "summary": "browser session worker artifact check passed",
        "run_dir": run_dir.display().to_string(),
        "engine": report.get("engine"),
        "events": events,
        "truth_events": truth_events,
        "action_verified": action_verified,
        "screenshots": screenshots.len(),
        "screenshot_saved_events": screenshot_saved,
        "screenshot_skipped_sensitive": skipped_sensitive,
        "screenshot_skipped_events": screenshot_skipped,
        "artifacts": {
            "report": report_path.display().to_string(),
            "replay": replay_path.display().to_string(),
            "screenshots": screenshots,
        }
    }))
}

fn report_replay_summary_tool(arguments: Value) -> Result<Value> {
    let replay_path = if let Some(path) = arguments.get("replay_path").and_then(Value::as_str) {
        safe_workspace_path(path)?
    } else {
        let run_dir = arguments
            .get("run_dir")
            .and_then(Value::as_str)
            .context("tool arguments must include run_dir or replay_path")?;
        safe_workspace_path(run_dir)?.join("replay.jsonl")
    };
    ensure_workspace_child(&replay_path)?;
    let file = fs::File::open(&replay_path)
        .with_context(|| format!("failed to open {}", replay_path.display()))?;
    let mut total = 0usize;
    let mut invalid = 0usize;
    let mut value_like_fields = 0usize;
    let mut first_ts_ms: Option<u64> = None;
    let mut last_ts_ms: Option<u64> = None;
    let mut kinds = BTreeMap::<String, usize>::new();

    for line in BufReader::new(file).lines() {
        let line = line.context("failed to read replay line")?;
        if line.trim().is_empty() {
            continue;
        }
        total += 1;
        match serde_json::from_str::<Value>(&line) {
            Ok(event) => {
                let kind = event
                    .get("kind")
                    .or_else(|| event.get("event"))
                    .or_else(|| event.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                *kinds.entry(kind).or_insert(0) += 1;
                if let Some(ts) = event.get("ts_ms").and_then(Value::as_u64) {
                    first_ts_ms.get_or_insert(ts);
                    last_ts_ms = Some(ts);
                }
                if object_has_value_like_field(&event) {
                    value_like_fields += 1;
                }
            }
            Err(_) => invalid += 1,
        }
    }

    Ok(json!({
        "status": if invalid == 0 { "ok" } else { "warning" },
        "summary": format!("{total} replay event(s), {invalid} invalid line(s)"),
        "events": total,
        "invalid_lines": invalid,
        "first_ts_ms": first_ts_ms,
        "last_ts_ms": last_ts_ms,
        "kinds": kinds,
        "value_like_fields": value_like_fields,
        "artifacts": {
            "replay": replay_path.display().to_string(),
        }
    }))
}

fn object_has_value_like_field(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            matches!(
                key.as_str(),
                "value" | "raw_value" | "password" | "ssn" | "credit_card"
            ) || object_has_value_like_field(value)
        }),
        Value::Array(items) => items.iter().any(object_has_value_like_field),
        _ => false,
    }
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

fn call_browser_worker(
    state: &mut McpSessionState,
    tab_id: TabId,
    method: &str,
    params: Value,
) -> Result<Value> {
    let worker = state
        .browser_workers
        .get_mut(&tab_id.0)
        .with_context(|| format!("tab_id {} has no browser worker", tab_id.0))?;
    worker.call(method, params)
}

fn update_session_tab_from_browser_result(tab: &mut SessionTab, result: &Value) {
    tab.info.title = result
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| tab.info.title.clone());
    if let Some(url) = result.get("url").and_then(Value::as_str) {
        tab.info.url = url.to_string();
    }
    if let Some(page_revision) = result.get("page_revision").and_then(Value::as_u64) {
        tab.info.page_revision = page_revision;
    }
    tab.last_engine = result
        .get("engine")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    tab.last_summary = result
        .get("summary")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    if let Some(actions) = result.get("actions").and_then(Value::as_array).cloned() {
        tab.last_actions = actions;
    }
    if let Some(findings) = result
        .get("findings")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| {
            result
                .pointer("/truth/findings")
                .and_then(Value::as_array)
                .cloned()
        })
    {
        tab.last_findings = findings;
    }
    tab.last_report_path = result
        .pointer("/artifacts/report")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    tab.last_replay_path = result
        .pointer("/artifacts/replay")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
}

fn tab_runtime(state: &McpSessionState, tab_id: TabId) -> &'static str {
    if state.dogfood_controls.contains_key(&tab_id.0) {
        "saccade-dogfood-control-v0"
    } else if state.browser_workers.contains_key(&tab_id.0) {
        "browser_session_worker_v0"
    } else {
        "mcp_report_backed_v0"
    }
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
    if !tab.info.agent_input_allowed() && !tab.agent_input_grant {
        bail!("agent input is denied for tab_id {}", tab_id.0);
    }
    Ok(())
}

fn action_requires_user_confirmation(
    state: &McpSessionState,
    tab_id: TabId,
    action_id: &str,
) -> Result<bool> {
    let tab = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    Ok(tab.agent_input_grant && side_effect_action_id(action_id))
}

fn side_effect_action_id(action_id: &str) -> bool {
    matches!(action_id, "act_submit" | "act_export" | "act_delete")
        || action_id.contains("purchase")
        || action_id.contains("payment")
        || action_id.contains("pay")
        || action_id.contains("sign")
        || action_id.contains("confirm")
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

    let local_url =
        start_test_server(workspace_root()?.join("test_pages").join("browser_session"))?;
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
    let browser_backed_tabs = open_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("runtime"))
                .and_then(Value::as_str)
                .map(|runtime| runtime == "browser_session_worker_v0")
        })
        .unwrap_or(false);
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
    let audit_replay = tool_call_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("artifacts"))
                .and_then(|artifacts| artifacts.get("replay"))
                .and_then(Value::as_str)
        })
        .unwrap_or("")
        .to_string();
    let live_audit_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(41)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.dev.audit_page",
                "arguments": {
                    "tab_id": tab_id,
                    "engine": "servo",
                    "replay": true
                }
            }),
        },
    );
    let live_worker_audit = live_audit_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .map(|content| {
                    content.get("status").and_then(Value::as_str) == Some("ok")
                        && content.get("runtime").and_then(Value::as_str)
                            == Some("browser_session_worker_v0")
                        && content.get("engine").and_then(Value::as_str)
                            == Some("saccade-browser-session-audit-v0")
                        && content
                            .pointer("/artifacts/report")
                            .and_then(Value::as_str)
                            .is_some_and(|path| path.contains("browser_session_worker"))
                })
        })
        .unwrap_or(false);
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
    let web_act_response = if !action_id.is_empty() && basis_page_revision > 0 {
        handle_json_rpc(
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
    } else {
        None
    };
    let web_act = web_act_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .map(|content| {
                    content.get("status").and_then(Value::as_str) == Some("ok")
                        && content.get("runtime").and_then(Value::as_str)
                            == Some("browser_session_worker_v0")
                        && content
                            .pointer("/verification/changed")
                            .and_then(Value::as_bool)
                            == Some(true)
                })
        })
        .unwrap_or(false);
    let browser_worker_run_dir = web_act_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("artifacts"))
                .and_then(|artifacts| artifacts.get("report"))
                .and_then(Value::as_str)
        })
        .and_then(|report| PathBuf::from(report).parent().map(Path::to_path_buf));
    let browser_worker_validate_run = browser_worker_run_dir
        .as_ref()
        .and_then(|run_dir| {
            handle_json_rpc(
                &mut state,
                JsonRpcRequest {
                    id: Some(json!(81)),
                    method: "tools/call".into(),
                    params: json!({
                        "name": "saccade.report.validate_run",
                        "arguments": {
                            "run_dir": run_dir.display().to_string(),
                            "kind": "browser_session_worker"
                        }
                    }),
                },
            )
        })
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("status"))
                .and_then(Value::as_str)
                .map(|status| status == "ok")
        })
        .unwrap_or(false);
    let flow_base_url =
        start_test_server(workspace_root()?.join("test_pages").join("login_handoff"))?;
    let flow_url = flow_base_url
        .join("user_flow.html")
        .context("failed to build user flow selftest URL")?;
    let flow_open_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(82)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.dev.open_local",
                "arguments": {
                    "url": flow_url.as_str(),
                    "owner": "agent"
                }
            }),
        },
    );
    let flow_tab_id = flow_open_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("tab"))
                .and_then(|tab| tab.get("tab_id"))
                .and_then(Value::as_u64)
        })
        .context("flow open selftest did not return tab_id")?;
    let flow_basis_page_revision = flow_open_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("tab"))
                .and_then(|tab| tab.get("page_revision"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(1);
    let web_fill_agent_fields_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(83)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.web.fill_agent_fields",
                "arguments": {
                    "tab_id": flow_tab_id,
                    "basis_page_revision": flow_basis_page_revision,
                    "fields": {
                        "task-1": "mcp-agent-task",
                        "ssn": "SHOULD-NOT-WRITE"
                    },
                    "policy": {
                        "agent_owned_only": true,
                        "block_sensitive": true,
                        "live_worker_only": true
                    }
                }
            }),
        },
    );
    let web_fill_agent_fields = web_fill_agent_fields_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .map(|content| {
                    let filled_task = content
                        .get("filled")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .any(|field| field.as_str() == Some("task-1"));
                    let rejected_ssn = content
                        .get("rejected")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .any(|field| field.get("id").and_then(Value::as_str) == Some("ssn"));
                    content.get("status").and_then(Value::as_str) == Some("ok")
                        && content.get("runtime").and_then(Value::as_str)
                            == Some("browser_session_worker_v0")
                        && filled_task
                        && rejected_ssn
                })
        })
        .unwrap_or(false);
    let web_inspect_fields_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(84)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.web.inspect_fields",
                "arguments": {
                    "tab_id": flow_tab_id,
                    "fields": ["task-1", "ssn"],
                    "policy": {
                        "redact_sensitive": true,
                        "explicit_fields_only": true,
                        "live_worker_only": true
                    }
                }
            }),
        },
    );
    let web_inspect_fields = web_inspect_fields_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .map(|content| {
                    let fields = content
                        .get("fields")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let task_returned = fields.iter().any(|field| {
                        field.get("id").and_then(Value::as_str) == Some("task-1")
                            && field.get("value_returned").and_then(Value::as_bool) == Some(true)
                            && field.get("value").and_then(Value::as_str) == Some("mcp-agent-task")
                    });
                    let ssn_redacted = fields.iter().any(|field| {
                        field.get("id").and_then(Value::as_str) == Some("ssn")
                            && field.get("value_redacted").and_then(Value::as_bool) == Some(true)
                            && field.get("value").is_none()
                    });
                    content.get("status").and_then(Value::as_str) == Some("ok")
                        && content.get("runtime").and_then(Value::as_str)
                            == Some("browser_session_worker_v0")
                        && task_returned
                        && ssn_redacted
                })
        })
        .unwrap_or(false);
    let formmax_base_url = start_test_server(workspace_root()?.join("test_pages").join("formmax"))?;
    let formmax_open_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(85)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.dev.open_local",
                "arguments": {
                    "url": formmax_base_url.as_str(),
                    "owner": "agent"
                }
            }),
        },
    );
    let formmax_tab_id = formmax_open_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("tab"))
                .and_then(|tab| tab.get("tab_id"))
                .and_then(Value::as_u64)
        })
        .context("FORMMAX live open selftest did not return tab_id")?;
    let formmax_basis_page_revision = formmax_open_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("tab"))
                .and_then(|tab| tab.get("page_revision"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(1);
    let web_fill_form_live_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(86)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.web.fill_form",
                "arguments": {
                    "tab_id": formmax_tab_id,
                    "basis_page_revision": formmax_basis_page_revision,
                    "policy": {
                        "block_sensitive": true,
                        "local_fixture_only": true,
                        "live_worker_only": true
                    }
                }
            }),
        },
    );
    let web_fill_form_live = web_fill_form_live_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .map(|content| {
                    content.get("status").and_then(Value::as_str) == Some("ok")
                        && content.get("runtime").and_then(Value::as_str)
                            == Some("browser_session_worker_v0")
                        && content.get("engine").and_then(Value::as_str)
                            == Some("saccade-browser-session-formmax-live-v0")
                        && content.get("rows").and_then(Value::as_u64) == Some(96)
                        && content.get("pages").and_then(Value::as_u64) == Some(2)
                        && content.get("filled").and_then(Value::as_u64) == Some(672)
                        && content.get("blocked_sensitive").and_then(Value::as_u64) == Some(3)
                        && content.get("receipt_verified").and_then(Value::as_bool) == Some(true)
                        && content
                            .pointer("/artifacts/replay")
                            .and_then(Value::as_str)
                            .is_some_and(|path| path.contains("browser_session_worker"))
                })
        })
        .unwrap_or(false);

    let copilot_base_url = start_test_server(
        workspace_root()?
            .join("test_pages")
            .join("current_tab_copilot"),
    )?;
    let grant_current_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(87)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.tabs.grant_current",
                "arguments": {
                    "url": copilot_base_url.as_str(),
                    "reason": "selftest user explicitly granted current tab assistance",
                    "read_grant": "full_truth",
                    "policy": {
                        "local_dev_only": true,
                        "explicit_user_grant": true
                    }
                }
            }),
        },
    );
    let copilot_tab_id = grant_current_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("tab"))
                .and_then(|tab| tab.get("tab_id"))
                .and_then(Value::as_u64)
        })
        .context("grant_current selftest did not return tab_id")?;
    let copilot_basis_page_revision = grant_current_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("tab"))
                .and_then(|tab| tab.get("page_revision"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(1);
    let grant_current_open = grant_current_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .map(|content| {
                    content.get("status").and_then(Value::as_str) == Some("ok")
                        && content.get("runtime").and_then(Value::as_str)
                            == Some("browser_session_worker_v0")
                        && content.get("selected_tab_seen").and_then(Value::as_bool) == Some(true)
                        && content.get("grant_required").and_then(Value::as_bool) == Some(true)
                        && content.get("agent_input_grant").and_then(Value::as_bool) == Some(true)
                        && content.pointer("/tab/owner").and_then(Value::as_str) == Some("Human")
                        && content.pointer("/tab/read_grant").and_then(Value::as_str)
                            == Some("FullTruth")
                })
        })
        .unwrap_or(false);
    let copilot_fill_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(88)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.web.fill_agent_fields",
                "arguments": {
                    "tab_id": copilot_tab_id,
                    "basis_page_revision": copilot_basis_page_revision,
                    "fields": {
                        "project-name": "MCP co-pilot capacity request",
                        "capacity": "24",
                        "notes": "Need blue-green launch capacity.",
                        "ssn": "SHOULD-NOT-WRITE",
                        "signature": "SHOULD-NOT-WRITE"
                    },
                    "policy": {
                        "agent_owned_only": true,
                        "block_sensitive": true,
                        "live_worker_only": true
                    }
                }
            }),
        },
    );
    let copilot_fill_ok = copilot_fill_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .map(|content| {
                    let filled = content
                        .get("filled")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let rejected = content
                        .get("rejected")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let normal_filled = ["project-name", "capacity", "notes"]
                        .iter()
                        .all(|id| filled.iter().any(|value| value.as_str() == Some(id)));
                    let sensitive_rejected = ["ssn", "signature"].iter().all(|id| {
                        rejected
                            .iter()
                            .any(|value| value.get("id").and_then(Value::as_str) == Some(id))
                    });
                    content.get("status").and_then(Value::as_str) == Some("ok")
                        && content.get("runtime").and_then(Value::as_str)
                            == Some("browser_session_worker_v0")
                        && normal_filled
                        && sensitive_rejected
                })
        })
        .unwrap_or(false);
    let copilot_inspect_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(89)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.web.inspect_fields",
                "arguments": {
                    "tab_id": copilot_tab_id,
                    "fields": ["project-name", "ssn", "signature"],
                    "policy": {
                        "redact_sensitive": true,
                        "explicit_fields_only": true,
                        "live_worker_only": true
                    }
                }
            }),
        },
    );
    let copilot_inspect_ok = copilot_inspect_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .map(|content| {
                    let fields = content
                        .get("fields")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let project_returned = fields.iter().any(|field| {
                        field.get("id").and_then(Value::as_str) == Some("project-name")
                            && field.get("value_returned").and_then(Value::as_bool) == Some(true)
                            && field.get("value").and_then(Value::as_str)
                                == Some("MCP co-pilot capacity request")
                    });
                    let ssn_redacted = fields.iter().any(|field| {
                        field.get("id").and_then(Value::as_str) == Some("ssn")
                            && field.get("value_redacted").and_then(Value::as_bool) == Some(true)
                            && field.get("value").is_none()
                    });
                    let signature_redacted = fields.iter().any(|field| {
                        field.get("id").and_then(Value::as_str) == Some("signature")
                            && field.get("value_redacted").and_then(Value::as_bool) == Some(true)
                            && field.get("value").is_none()
                    });
                    content.get("status").and_then(Value::as_str) == Some("ok")
                        && content.get("runtime").and_then(Value::as_str)
                            == Some("browser_session_worker_v0")
                        && project_returned
                        && ssn_redacted
                        && signature_redacted
                })
        })
        .unwrap_or(false);
    let copilot_actions_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(90)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.web.actions",
                "arguments": {
                    "tab_id": copilot_tab_id,
                    "engine": "servo"
                }
            }),
        },
    );
    let copilot_submit_action = copilot_actions_response.as_ref().and_then(|response| {
        response
            .get("result")
            .and_then(|result| result.get("structuredContent"))
            .and_then(|content| content.get("actions"))
            .and_then(Value::as_array)
            .and_then(|actions| {
                actions.iter().find_map(|action| {
                    let action_id = action.get("action_id").and_then(Value::as_str)?;
                    let label = action.get("label").and_then(Value::as_str).unwrap_or("");
                    if action_id == "act_submit" || label.eq_ignore_ascii_case("submit") {
                        Some(action_id.to_string())
                    } else {
                        None
                    }
                })
            })
    });
    let copilot_action_basis = copilot_actions_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|content| content.get("page_revision"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(copilot_basis_page_revision);
    let copilot_submit_block_response = copilot_submit_action.as_ref().and_then(|action_id| {
        handle_json_rpc(
            &mut state,
            JsonRpcRequest {
                id: Some(json!(91)),
                method: "tools/call".into(),
                params: json!({
                    "name": "saccade.web.act",
                    "arguments": {
                        "tab_id": copilot_tab_id,
                        "action_id": action_id,
                        "basis_page_revision": copilot_action_basis,
                        "engine": "servo"
                    }
                }),
            },
        )
    });
    let copilot_submit_blocked = copilot_submit_block_response
        .as_ref()
        .and_then(|response| {
            response
                .get("error")
                .and_then(|error| error.get("data"))
                .and_then(Value::as_str)
                .map(|detail| detail.contains("user confirmation required"))
        })
        .unwrap_or(false);
    let copilot_response_blob = [
        grant_current_response.as_ref(),
        copilot_fill_response.as_ref(),
        copilot_inspect_response.as_ref(),
        copilot_actions_response.as_ref(),
        copilot_submit_block_response.as_ref(),
    ]
    .into_iter()
    .flatten()
    .map(Value::to_string)
    .collect::<Vec<_>>()
    .join("\n");
    let copilot_no_sensitive_leak = !copilot_response_blob.contains("999-12-3456")
        && !copilot_response_blob.contains("SHOULD-NOT-WRITE");
    let tabs_grant_current = grant_current_open
        && copilot_fill_ok
        && copilot_inspect_ok
        && copilot_submit_action.is_some()
        && copilot_submit_blocked
        && copilot_no_sensitive_leak;
    let grant_artifact_dir = workspace_root()?.join("runs").join("mcp");
    fs::create_dir_all(&grant_artifact_dir)
        .with_context(|| format!("failed to create {}", grant_artifact_dir.display()))?;
    let grant_artifact_path =
        grant_artifact_dir.join(format!("current_tab_grant_{}.json", unix_ms()?));
    write_json(
        &grant_artifact_path,
        &json!({
            "status": "granted",
            "runtime": "saccade-dogfood-browser-v0",
            "grant_type": "current_tab_copilot",
            "selected_tab_seen": true,
            "grant_required": true,
            "grant_given": true,
            "owner": "Human",
            "read_grant": "FullTruth",
            "agent_input_grant": true,
            "url": copilot_base_url.as_str(),
            "title": "Current Tab Co-Pilot Fixture",
            "rendering_profile": "servo-modern",
            "shortcut": "Cmd+Shift+G",
            "mcp_tool": "saccade.tabs.grant_current",
            "transport_status": "url_grant_artifact_v0",
            "written_unix_ms": unix_ms()?,
        }),
    )?;
    let grant_artifact_response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(92)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.tabs.grant_current",
                "arguments": {
                    "grant_path": grant_artifact_path.display().to_string(),
                    "reason": "selftest imported dogfood browser grant artifact",
                    "policy": {
                        "local_dev_only": true,
                        "explicit_user_grant": true
                    }
                }
            }),
        },
    );
    let tabs_grant_artifact = grant_artifact_response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .map(|content| {
                    content.get("status").and_then(Value::as_str) == Some("ok")
                        && content.get("runtime").and_then(Value::as_str)
                            == Some("browser_session_worker_v0")
                        && content.get("source").and_then(Value::as_str) == Some("grant_artifact")
                        && content
                            .get("same_webview_attached")
                            .and_then(Value::as_bool)
                            == Some(false)
                        && content.get("transport_status").and_then(Value::as_str)
                            == Some("worker_from_grant_artifact_v0")
                        && content.pointer("/tab/owner").and_then(Value::as_str) == Some("Human")
                        && content.pointer("/tab/read_grant").and_then(Value::as_str)
                            == Some("FullTruth")
                        && content
                            .get("grant_path")
                            .and_then(Value::as_str)
                            .is_some_and(|path| path.ends_with(".json"))
                })
        })
        .unwrap_or(false);
    let dev_click_all_primary_actions = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(9)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.dev.click_all_primary_actions",
                "arguments": {
                    "tab_id": tab_id,
                    "policy": {
                        "max_actions": 1,
                        "local_dev_only": true
                    }
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
    let dev_fill_smoke_form = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(10)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.dev.fill_smoke_form",
                "arguments": {
                    "fixture": "test_pages/formmax/index.html",
                    "replay": true
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
    let dev_get_report = !audit_report.is_empty()
        && handle_json_rpc(
            &mut state,
            JsonRpcRequest {
                id: Some(json!(11)),
                method: "tools/call".into(),
                params: json!({
                    "name": "saccade.dev.get_report",
                    "arguments": {
                        "report_path": audit_report.clone()
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
    let audit_run_dir = PathBuf::from(&audit_report)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let report_validate_run = !audit_run_dir.as_os_str().is_empty()
        && handle_json_rpc(
            &mut state,
            JsonRpcRequest {
                id: Some(json!(12)),
                method: "tools/call".into(),
                params: json!({
                    "name": "saccade.report.validate_run",
                    "arguments": {
                        "run_dir": audit_run_dir.display().to_string(),
                        "kind": "generic"
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
    let report_replay_summary = !audit_replay.is_empty()
        && handle_json_rpc(
            &mut state,
            JsonRpcRequest {
                id: Some(json!(13)),
                method: "tools/call".into(),
                params: json!({
                    "name": "saccade.report.replay_summary",
                    "arguments": {
                        "replay_path": audit_replay
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
                .map(|status| status == "ok" || status == "warning")
        })
        .unwrap_or(false);

    Ok(JsonRpcEvidence {
        initialize,
        tools_list,
        tool_call,
        persistent_tabs,
        browser_backed_tabs,
        tabs_grant_current,
        tabs_grant_artifact,
        web_truth,
        web_actions,
        web_act,
        web_fill_agent_fields,
        web_inspect_fields,
        web_fill_form_live,
        live_worker_audit,
        dev_click_all_primary_actions,
        dev_fill_smoke_form,
        dev_get_report,
        report_validate_run,
        browser_worker_validate_run,
        report_replay_summary,
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

fn record_artifact_index(
    tool: &str,
    kind: &str,
    summary: &str,
    artifacts: Value,
) -> Result<String> {
    let index_path = workspace_root()?
        .join("runs")
        .join("mcp")
        .join("artifacts.jsonl");
    if let Some(parent) = index_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let record = ArtifactIndexRecord {
        ts_ms: unix_ms()?,
        tool,
        kind,
        summary,
        artifacts,
    };
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)
        .with_context(|| format!("failed to open {}", index_path.display()))?;
    writeln!(file, "{}", serde_json::to_string(&record)?)?;
    Ok(index_path.display().to_string())
}

fn safe_workspace_path(path: &str) -> Result<PathBuf> {
    let input = PathBuf::from(path);
    let workspace = workspace_root()?;
    let full_path = if input.is_absolute() {
        input
    } else {
        workspace.join(input)
    };
    let canonical = full_path
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", full_path.display()))?;
    ensure_workspace_child(&canonical)?;
    Ok(canonical)
}

fn ensure_workspace_child(path: &Path) -> Result<()> {
    let workspace = workspace_root()?
        .canonicalize()
        .context("failed to canonicalize workspace root")?;
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", path.display()))?;
    if !canonical.starts_with(&workspace) {
        bail!(
            "path {} is outside workspace {}",
            canonical.display(),
            workspace.display()
        );
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dogfood_control_socket_addr_maps_allowed_loopback_hosts() {
        let endpoint = DogfoodControlEndpoint {
            host: "localhost".to_string(),
            port: 49321,
            protocol: "saccade-dogfood-control-v0".to_string(),
        };
        assert_eq!(
            dogfood_control_socket_addr(&endpoint)
                .expect("localhost should map to loopback")
                .to_string(),
            "127.0.0.1:49321"
        );

        let endpoint = DogfoodControlEndpoint {
            host: "::1".to_string(),
            port: 49322,
            protocol: "saccade-dogfood-control-v0".to_string(),
        };
        assert_eq!(
            dogfood_control_socket_addr(&endpoint)
                .expect("ipv6 loopback should map to loopback")
                .to_string(),
            "[::1]:49322"
        );
    }

    #[test]
    fn tab_runtime_prefers_dogfood_control() {
        let mut state = McpSessionState::default();
        state.dogfood_controls.insert(
            7,
            DogfoodControlEndpoint {
                host: "127.0.0.1".to_string(),
                port: 49321,
                protocol: "saccade-dogfood-control-v0".to_string(),
            },
        );

        assert_eq!(tab_runtime(&state, TabId(7)), "saccade-dogfood-control-v0");
        assert_eq!(tab_runtime(&state, TabId(8)), "mcp_report_backed_v0");
    }
}
