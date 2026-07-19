use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use saccade_core::{
    ReadGrant, SitePolicy, TabId, TabInfo, TabOwner, TabVisualMarker,
    classify_site_url_with_owned_domains,
};
use saccade_engine_api::{
    CONTROL_PROTOCOL_VERSION, EngineApiError, EngineErrorCode, EngineGrant, call_control,
    read_owner_only_grant,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tiny_http::{Header, Response, Server, StatusCode};
use url::Url;

const REQUIRED_TOOL_COUNT: usize = 31;
const SACCADE_CONTRACT_VERSION: &str = "1.0";
const SACCADE_MIN_CONTRACT_VERSION: &str = "1.0";
const COLLECTOR_READY_TIMEOUT: Duration = Duration::from_secs(12);
const FORM_INVENTORY_STABILITY_INTERVAL: Duration = Duration::from_millis(200);
const DEFAULT_FORM_INVENTORY_WAIT_MS: u64 = 5_000;
const POST_EXECUTE_FORM_INVENTORY_WAIT_MS: u64 = 1_500;
const POST_EXECUTE_FORM_INVENTORY_MIN_WAIT_MS: u64 = 600;
const MCP_INSTRUCTIONS: &str = "When the user asks about a Human-created current Saccade tab, first call saccade.tabs.grant_current with no arguments; if its per-tab Agent switch is Off, ask the user to turn it On. When the user asks the LLM to open or start a browsing session, call saccade.tabs.open_agent; it reuses a running Saccade process or starts one, and only that new Agent tab begins On. After open_agent succeeds, use its tab_id and page_revision directly; do not call tabs.list or grant_current first. Use saccade.web.article_text for bounded reading or research. Once the user authorizes a form task, complete ordinary fields and reversible form input directly instead of asking the user to type or click. Contact email, company name, ordinary address, URL, and similar profile data are ordinary fields. Reuse exact values already provided or available in trusted context, and ask only when an exact value is missing or a materially different choice is genuinely ambiguous. Before filling, call form_inventory, then compile and execute one revision-bound plan containing all known authorized ordinary assignments. If form_execute_plan returns follow_up_required=true, use post_execute_inventory to compile and execute the newly revealed ordinary fields, stopping when no follow-up is required. Respect the user's stopping point: filling fields does not authorize Next, submit, purchase, publish, or another later action. A form_execute_plan result with verification_complete=true and follow_up_required=false is final verification; do not inventory or inspect those fields again. Never ask for or accept a raw protected value; request a named protected field through saccade.web.request_protected_fill so only the browser talks to the user. Never read or fill passwords, OTPs, or CVVs. The LLM host owns site-action decisions; Saccade enforces Agent On, protected-value isolation, revision/target validity, input validity, and receipts.";

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
    System,
    Browser,
    Downloads,
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
    stdio_contract_capabilities: bool,
    stdio_tool_call: bool,
    persistent_tabs: bool,
    browser_backed_tabs: bool,
    tabs_grant_current: bool,
    tabs_grant_artifact: bool,
    servoshell_bridge_grant: bool,
    servoshell_bridge_formmax_live: bool,
    servoshell_bridge_artifacts: bool,
    browser_navigate: bool,
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
    report_redacted_note: bool,
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
    let mut state = McpSessionState {
        installed_product: std::env::var("SACCADE_MCP_RUNTIME_PROFILE").as_deref()
            == Ok("installed_product"),
        ..McpSessionState::default()
    };
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
        && stdio_evidence.contract_capabilities
        && stdio_evidence.tool_call
        && stdio_evidence.persistent_tabs
        && stdio_evidence.browser_backed_tabs
        && stdio_evidence.tabs_grant_current
        && stdio_evidence.tabs_grant_artifact
        && stdio_evidence.servoshell_bridge_grant
        && stdio_evidence.servoshell_bridge_formmax_live
        && stdio_evidence.servoshell_bridge_artifacts
        && stdio_evidence.browser_navigate
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
        && stdio_evidence.report_replay_summary
        && stdio_evidence.report_redacted_note;
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
        stdio_contract_capabilities: stdio_evidence.contract_capabilities,
        stdio_tool_call: stdio_evidence.tool_call,
        persistent_tabs: stdio_evidence.persistent_tabs,
        browser_backed_tabs: stdio_evidence.browser_backed_tabs,
        tabs_grant_current: stdio_evidence.tabs_grant_current,
        tabs_grant_artifact: stdio_evidence.tabs_grant_artifact,
        servoshell_bridge_grant: stdio_evidence.servoshell_bridge_grant,
        servoshell_bridge_formmax_live: stdio_evidence.servoshell_bridge_formmax_live,
        servoshell_bridge_artifacts: stdio_evidence.servoshell_bridge_artifacts,
        browser_navigate: stdio_evidence.browser_navigate,
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
        report_redacted_note: stdio_evidence.report_redacted_note,
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
        version: "saccade-contract-v1",
        tools: vec![
            tool(
                "saccade.system.capabilities",
                ToolNamespace::System,
                ToolRisk::ReportOnly,
                "Return the Saccade contract version, feature set, limits, and lifecycle rules.",
                false,
                false,
                true,
            ),
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
                "saccade.browser.navigate",
                ToolNamespace::Browser,
                ToolRisk::PolicyGated,
                "Run browser-shell navigation on an already-granted visible Saccade dogfood tab.",
                true,
                true,
                true,
            ),
            tool(
                "saccade.downloads.list",
                ToolNamespace::Downloads,
                ToolRisk::ReportOnly,
                "List metadata-only receipts for downloads started while the selected tab was Agent On; never return file contents or full paths.",
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
                "saccade.tabs.open_agent",
                ToolNamespace::Tabs,
                ToolRisk::PolicyGated,
                "Open an Agent-owned On tab in the running Saccade browser, or start Saccade when it is not running.",
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
                "Attach the user-selected current tab after explicit grant; with no arguments, discover the running packaged open-saccade session.",
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
                "saccade.web.article_text",
                ToolNamespace::Web,
                ToolRisk::ReportOnly,
                "Read bounded redacted article/main text from the granted current tab at one page revision.",
                true,
                false,
                true,
            ),
            tool(
                "saccade.web.actions",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Refresh and return the live action map with stable action IDs, page revision, and layout epoch.",
                true,
                false,
                true,
            ),
            tool(
                "saccade.web.act",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Perform one receipt-verified action, locally rebasing the same semantic target after a layout-only change.",
                true,
                true,
                true,
            ),
            tool(
                "saccade.web.fill_agent_fields",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Complete authorized ordinary fields directly in a live browser tab; do not hand manual typing back to the user when exact values are known.",
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
                "saccade.web.render_preflight",
                ToolNamespace::Web,
                ToolRisk::ReportOnly,
                "Check local render/semantic consistency before asking the user to enter task data.",
                true,
                false,
                true,
            ),
            tool(
                "saccade.web.form_inventory",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Discover form fields and redacted state in the granted current tab.",
                true,
                true,
                true,
            ),
            tool(
                "saccade.web.request_protected_fill",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Ask the browser to show a user-controlled local fill prompt for one protected identifier; no value is accepted or returned by MCP.",
                true,
                true,
                true,
            ),
            tool(
                "saccade.web.form_compile_plan",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Compile all known authorized ordinary assignments into a non-writing plan against a fixed page revision.",
                true,
                true,
                true,
            ),
            tool(
                "saccade.web.form_execute_plan",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Execute and verify an unchanged ordinary-field plan without clicking Next or submitting.",
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
            tool(
                "saccade.report.redacted_note",
                ToolNamespace::Report,
                ToolRisk::ReportOnly,
                "Create a local redacted AI review packet for high-risk fallback content.",
                true,
                false,
                true,
            ),
        ],
    }
}

fn tool_available_in_runtime(name: &str, installed_product: bool) -> bool {
    if !installed_product {
        return true;
    }
    !name.starts_with("saccade.dev.")
        && !name.starts_with("saccade.report.")
        && !matches!(
            name,
            "saccade.tabs.open" | "saccade.tabs.request_user_login" | "saccade.web.fill_form"
        )
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
    installed_product: bool,
    next_tab_id: u64,
    tabs: Vec<SessionTab>,
    browser_workers: BTreeMap<u64, BrowserWorkerClient>,
    dogfood_controls: BTreeMap<u64, DogfoodControlEndpoint>,
    dogfood_control_runtimes: BTreeMap<u64, String>,
    dogfood_control_capabilities: BTreeMap<u64, Vec<String>>,
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
    contract_capabilities: bool,
    tool_call: bool,
    persistent_tabs: bool,
    browser_backed_tabs: bool,
    tabs_grant_current: bool,
    tabs_grant_artifact: bool,
    servoshell_bridge_grant: bool,
    servoshell_bridge_formmax_live: bool,
    servoshell_bridge_artifacts: bool,
    browser_navigate: bool,
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
    report_redacted_note: bool,
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
            "instructions": MCP_INSTRUCTIONS,
            "capabilities": {
                "tools": {
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": "saccade-mcp",
                "version": "saccade-contract-v1"
            },
            "saccade": contract_capabilities(state.installed_product)
        })),
        "tools/list" => Ok(json!({
            "tools": registry()
                .tools
                .iter()
                .filter(|tool| tool_available_in_runtime(tool.name, state.installed_product))
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
    let saccade_code = saccade_error_code(&detail);
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
            "data": {
                "saccade_code": saccade_code,
                "detail": detail,
                "retryable": matches!(
                    saccade_code,
                    "SACCADE_STALE_BASIS"
                        | "SACCADE_STALE_LAYOUT"
                        | "SACCADE_TIMEOUT"
                        | "SACCADE_PROVIDER_REJECTED"
                ),
                "requires_human": matches!(
                    saccade_code,
                    "SACCADE_CONSENT_REQUIRED"
                        | "SACCADE_HUMAN_VERIFICATION_REQUIRED"
                        | "SACCADE_PROVIDER_REJECTED"
                )
            },
        }
    })
}

fn saccade_error_code(detail: &str) -> &'static str {
    if detail.contains("human verification required")
        || detail.contains("HUMAN_VERIFICATION_REQUIRED")
    {
        "SACCADE_HUMAN_VERIFICATION_REQUIRED"
    } else if detail.contains("human verification provider rejected")
        || detail.contains("PROVIDER_REJECTED")
    {
        "SACCADE_PROVIDER_REJECTED"
    } else if detail.contains("CONSENT_REQUIRED") || detail.contains("Agent access is Off") {
        "SACCADE_CONSENT_REQUIRED"
    } else if detail.contains("AGENT_PAUSED") || detail.contains("agent runtime is paused") {
        "SACCADE_AGENT_PAUSED"
    } else if detail.contains("stale engine layout") || detail.contains("STALE_LAYOUT") {
        "SACCADE_STALE_LAYOUT"
    } else if detail.contains("tool arguments")
        || detail.contains("invalid ")
        || detail.contains("requires integer")
    {
        "SACCADE_INVALID_ARGUMENT"
    } else if detail.contains("stale") {
        "SACCADE_STALE_BASIS"
    } else if detail.contains("timeout") || detail.contains("timed out") {
        "SACCADE_TIMEOUT"
    } else if detail.contains("unknown tab_id") || detail.contains("not found") {
        "SACCADE_NOT_FOUND"
    } else if detail.contains("requires") || detail.contains("denied") || detail.contains("blocked")
    {
        "SACCADE_POLICY_DENIED"
    } else if detail.contains("unsupported") || detail.contains("only accepts") {
        "SACCADE_UNSUPPORTED"
    } else {
        "SACCADE_INTERNAL"
    }
}

fn rpc_error_detail(error: &Value) -> Option<&str> {
    let data = error.get("data")?;
    data.as_str()
        .or_else(|| data.get("detail").and_then(Value::as_str))
}

fn contract_capabilities(installed_product: bool) -> Value {
    json!({
        "contract_version": SACCADE_CONTRACT_VERSION,
        "min_supported_contract_version": SACCADE_MIN_CONTRACT_VERSION,
        "features": [
            "current_tab_grant",
            "redacted_truth",
            "verified_safe_actions",
            "form_compile_execute",
            "user_confirmed_local_protected_fill",
            "render_preflight",
            "download_receipts",
            "value_free_replay",
            "typed_errors"
        ],
        "limits": {
            "form_inventory_max_page_size": 500,
            "form_plan_max_assignments": 5000,
            "default_control_connect_timeout_ms": 2000,
            "default_control_read_timeout_ms": 5000
        },
        "lifecycle": {
            "cancellation": "host stops issuing work, then calls saccade.tabs.pause_agent or saccade.tabs.close",
            "shutdown": "saccade.tabs.close releases the MCP tab state and its attached worker or bridge",
            "site_action_policy": "owned by the LLM host; Saccade adds no second confirmation layer"
        },
        "runtime_profile": if installed_product { "installed_product" } else { "developer" },
        "developer_tools_available": !installed_product,
        "data_boundary": {
            "never_returned_by_default": ["cookies", "storage", "control_capability", "sensitive_field_values", "sensitive_page_screenshots"]
        },
        "form_behavior": {
            "authorized_ordinary_fields": "fill_without_manual_handoff",
            "ask_user_when": "exact_value_missing_or_material_choice_ambiguous",
            "submission": "respect_explicit_user_stopping_point",
            "protected_identifiers": "local_browser_prompt_only",
            "secrets": "never_read_or_fill"
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
        "saccade.system.capabilities" => json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
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
        "saccade.browser.navigate" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"},
                "action": {"type": "string", "enum": ["status", "navigate", "reload", "back", "forward"]},
                "url": {"type": "string"},
                "policy": {
                    "type": "object",
                    "properties": {
                        "same_webview_only": {"type": "boolean", "const": true},
                        "agent_on_tab_only": {"type": "boolean", "const": true}
                    },
                    "additionalProperties": false
                }
            },
            "required": ["tab_id", "action"],
            "additionalProperties": false
        }),
        "saccade.downloads.list" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"}
            },
            "required": ["tab_id"],
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
        "saccade.tabs.open_agent" => json!({
            "type": "object",
            "properties": {
                "url": {"type": "string"}
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
                "browser_tab_id": {"type": "string"},
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
        "saccade.web.article_text" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"},
                "basis_page_revision": {"type": "integer"},
                "max_chars": {"type": "integer", "minimum": 1000, "maximum": 100000, "default": 20000},
                "mode": {"type": "string", "enum": ["minimal", "compact", "evidence"], "default": "minimal"}
            },
            "required": ["tab_id", "basis_page_revision"],
            "additionalProperties": false
        }),
        "saccade.web.act" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"},
                "action_id": {"type": "string"},
                "basis_page_revision": {"type": "integer"},
                "basis_layout_epoch": {"type": "integer", "description": "Optional layout epoch from the action map; Saccade still refreshes and validates it immediately before input."},
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
        "saccade.web.render_preflight" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"},
                "expected_surface": {
                    "type": "string",
                    "enum": ["page", "github_issue", "github_discussion"],
                    "default": "page"
                }
            },
            "required": ["tab_id"],
            "additionalProperties": false
        }),
        "saccade.web.form_inventory" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"},
                "mode": {"type": "string", "enum": ["minimal", "full", "actionable", "compact"], "default": "minimal"},
                "offset": {"type": "integer", "minimum": 0, "default": 0},
                "limit": {"type": "integer", "minimum": 1, "maximum": 500},
                "wait_for_fields_ms": {"type": "integer", "minimum": 0, "maximum": 10000, "default": 5000}
            },
            "required": ["tab_id"],
            "additionalProperties": false
        }),
        "saccade.web.request_protected_fill" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"},
                "basis_page_revision": {"type": "integer"},
                "field_id": {"type": "string", "minLength": 1, "maxLength": 256}
            },
            "required": ["tab_id", "basis_page_revision", "field_id"],
            "additionalProperties": false
        }),
        "saccade.web.form_compile_plan" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"},
                "basis_page_revision": {"type": "integer"},
                "assignments": {
                    "type": "object",
                    "minProperties": 1,
                    "maxProperties": 5000,
                    "additionalProperties": {"type": ["string", "number", "boolean"]}
                },
                "policy": {
                    "type": "object",
                    "properties": {
                        "block_sensitive": {"type": "boolean", "const": true},
                        "preserve_existing": {"type": "boolean", "const": true},
                        "no_submit": {"type": "boolean", "const": true}
                    },
                    "required": ["block_sensitive", "preserve_existing", "no_submit"],
                    "additionalProperties": false
                }
            },
            "required": ["tab_id", "basis_page_revision", "assignments", "policy"],
            "additionalProperties": false
        }),
        "saccade.web.form_execute_plan" => json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "integer"},
                "basis_page_revision": {"type": "integer"},
                "expected_plan_id": {"type": "string", "minLength": 1},
                "assignments": {
                    "type": "object",
                    "minProperties": 1,
                    "maxProperties": 5000,
                    "additionalProperties": {"type": ["string", "number", "boolean"]}
                },
                "policy": {
                    "type": "object",
                    "properties": {
                        "block_sensitive": {"type": "boolean", "const": true},
                        "preserve_existing": {"type": "boolean", "const": true},
                        "no_submit": {"type": "boolean", "const": true}
                    },
                    "required": ["block_sensitive", "preserve_existing", "no_submit"],
                    "additionalProperties": false
                }
            },
            "required": ["tab_id", "basis_page_revision", "expected_plan_id", "assignments", "policy"],
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
        "saccade.report.redacted_note" => json!({
            "type": "object",
            "properties": {
                "source_url": {"type": "string"},
                "title": {"type": "string"},
                "task": {
                    "type": "string",
                    "enum": ["evaluate_edit", "draft_reply", "summarize", "checklist"],
                    "default": "evaluate_edit"
                },
                "audience": {"type": "string"},
                "redacted_text": {"type": "string"},
                "policy": {
                    "type": "object",
                    "properties": {
                        "redacted_user_supplied": {"type": "boolean", "const": true},
                        "no_live_site_access": {"type": "boolean", "const": true}
                    },
                    "additionalProperties": false
                }
            },
            "required": ["redacted_text"],
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
    if !tool_available_in_runtime(name, state.installed_product) {
        bail!("tool {name:?} is unsupported in the installed product runtime");
    }
    match name {
        "saccade.system.capabilities" => Ok(json!({
            "status": "ok",
            "summary": "Saccade contract capabilities",
            "saccade": contract_capabilities(state.installed_product),
        })),
        "saccade.dev.open_local" => open_local_tool(state, arguments),
        "saccade.dev.audit_page" => audit_page_tool(state, arguments),
        "saccade.dev.click_all_primary_actions" => {
            dev_click_all_primary_actions_tool(state, arguments)
        }
        "saccade.dev.fill_smoke_form" => dev_fill_smoke_form_tool(arguments),
        "saccade.dev.get_report" => dev_get_report_tool(arguments),
        "saccade.browser.navigate" => browser_navigate_tool(state, arguments),
        "saccade.downloads.list" => downloads_list_tool(state, arguments),
        "saccade.tabs.list" => tabs_list_tool(state),
        "saccade.tabs.open" => tabs_open_tool(state, arguments),
        "saccade.tabs.open_agent" => tabs_open_agent_tool(state, arguments),
        "saccade.tabs.request_user_login" => tabs_request_user_login_tool(state, arguments),
        "saccade.tabs.grant_current" => tabs_grant_current_tool(state, arguments),
        "saccade.tabs.takeover" => tabs_takeover_tool(state, arguments),
        "saccade.tabs.pause_agent" => tabs_pause_agent_tool(state, arguments),
        "saccade.tabs.close" => tabs_close_tool(state, arguments),
        "saccade.web.truth" => web_truth_tool(state, arguments),
        "saccade.web.article_text" => web_article_text_tool(state, arguments),
        "saccade.web.actions" => web_actions_tool(state, arguments),
        "saccade.web.act" => web_act_tool(state, arguments),
        "saccade.web.fill_agent_fields" => web_fill_agent_fields_tool(state, arguments),
        "saccade.web.inspect_fields" => web_inspect_fields_tool(state, arguments),
        "saccade.web.render_preflight" => web_render_preflight_tool(state, arguments),
        "saccade.web.form_inventory" => web_form_inventory_tool(state, arguments),
        "saccade.web.request_protected_fill" => web_request_protected_fill_tool(state, arguments),
        "saccade.web.form_compile_plan" => web_form_compile_plan_tool(state, arguments),
        "saccade.web.form_execute_plan" => web_form_execute_plan_tool(state, arguments),
        "saccade.web.fill_form" => web_fill_form_tool(state, arguments),
        "saccade.report.validate_run" => report_validate_run_tool(arguments),
        "saccade.report.replay_summary" => report_replay_summary_tool(arguments),
        "saccade.report.redacted_note" => report_redacted_note_tool(arguments),
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

fn browser_navigate_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .context("tool arguments must include string field action")?;
    if let Some(policy) = arguments.get("policy") {
        if policy
            .get("same_webview_only")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("saccade.browser.navigate requires same_webview_only=true");
        }
        if policy
            .get("agent_on_tab_only")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("saccade.browser.navigate requires agent_on_tab_only=true");
        }
    }
    let Some(endpoint) = state.dogfood_controls.get(&tab_id.0).cloned() else {
        bail!("saccade.browser.navigate requires a same-WebView dogfood control tab");
    };
    ensure_agent_input_allowed(state, tab_id)?;

    let (method, params) = match action {
        "status" => ("shell_status", json!({})),
        "reload" => ("reload", json!({})),
        "back" => ("back", json!({})),
        "forward" => ("forward", json!({})),
        "navigate" => {
            let url = arguments
                .get("url")
                .and_then(Value::as_str)
                .context("saccade.browser.navigate action=navigate requires string field url")?;
            let url = Url::parse(url)
                .with_context(|| format!("invalid browser navigation URL: {url}"))?;
            ("navigate", json!({ "url": url.as_str() }))
        }
        other => bail!("unsupported browser navigation action {other:?}"),
    };

    ensure_dogfood_control_capability(state, tab_id, method)?;
    let dispatch_result = call_dogfood_control(&endpoint, method, params)?;
    let changed = dispatch_result
        .get("changed")
        .and_then(Value::as_bool)
        .unwrap_or(action != "status");
    let target_revision = dispatch_result
        .get("page_revision")
        .and_then(json_number_u64)
        .unwrap_or(0);
    let mut shell_result = dispatch_result.clone();
    if action != "status" && changed {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let truth = call_dogfood_control(&endpoint, "truth", json!({}))?;
            let ready = truth.get("collector_ready").and_then(Value::as_bool) == Some(true);
            let revision = truth
                .get("page_revision")
                .and_then(json_number_u64)
                .unwrap_or(0);
            if let Some(tab) = state.find_tab_mut(tab_id) {
                update_session_tab_from_browser_result(tab, &truth);
            }
            if ready && revision >= target_revision {
                shell_result = call_dogfood_control(&endpoint, "shell_status", json!({}))?;
                break;
            }
            if Instant::now() >= deadline {
                bail!("browser navigation timed out waiting for collector readiness");
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    if let Some(tab) = state.find_tab_mut(tab_id) {
        update_session_tab_from_browser_result(tab, &shell_result);
    }
    let tab = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    Ok(json!({
        "status": "ok",
        "summary": format!("browser shell {action} dispatched through same dogfood WebView"),
        "runtime": tab_runtime(state, tab_id),
        "tab_id": tab_id.0,
        "action": action,
        "url": tab.info.url,
        "title": tab.info.title,
        "page_revision": tab.info.page_revision,
        "changed": changed,
        "dispatch": dispatch_result,
        "shell": shell_result,
        "site_policy": classify_site_url(&tab.info.url),
        "policy": {
            "same_webview_only": true,
            "agent_on_tab_only": true,
            "page_dom_injected": false,
        },
    }))
}

fn downloads_list_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    ensure_truth_allowed(state, tab_id)?;
    let endpoint = state
        .dogfood_controls
        .get(&tab_id.0)
        .cloned()
        .context("saccade.downloads.list requires a granted Saccade browser tab")?;
    ensure_dogfood_control_capability(state, tab_id, "downloads")?;
    let mut result = call_dogfood_control(&endpoint, "downloads", json!({}))?;
    let page_revision = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?
        .info
        .page_revision;
    let runtime = tab_runtime(state, tab_id);
    if let Some(object) = result.as_object_mut() {
        object.insert("tab_id".to_string(), json!(tab_id.0));
        object.insert("page_revision".to_string(), json!(page_revision));
        object.insert("runtime".to_string(), json!(runtime));
        object.insert("file_contents_returned".to_string(), json!(false));
        object.insert("full_paths_returned".to_string(), json!(false));
        object.insert("auto_execute_allowed".to_string(), json!(false));
    }
    Ok(result)
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
    if let Ok((_, endpoint)) = current_agent_broker_endpoint() {
        if call_dogfood_control_ping(&endpoint).is_ok() {
            match call_dogfood_control(&endpoint, "tab_registry", json!({})) {
                Ok(registry) => {
                    return Ok(json!({
                        "status": "ok",
                        "summary": "live Saccade Agent On tab registry",
                        "source": "live_browser_tab_registry",
                        "tabs": registry.get("tabs").cloned().unwrap_or_else(|| json!([])),
                        "eligible_count": registry.get("eligible_count").cloned().unwrap_or(Value::Null),
                        "browser_count": registry.get("browser_count").cloned().unwrap_or(Value::Null),
                        "agent_off_tabs_omitted": registry.get("agent_off_tabs_omitted").cloned().unwrap_or(Value::Bool(true)),
                        "capabilities_exposed": false,
                        "cookies_or_storage_exposed": false,
                        "session_tabs": state.tabs,
                    }));
                }
                Err(error) => {
                    return Ok(json!({
                        "status": "warning",
                        "summary": "live Saccade broker is running but did not return a tab registry",
                        "source": "mcp_session_fallback",
                        "registry_error": error.to_string(),
                        "tabs": state.tabs,
                    }));
                }
            }
        }
    }
    Ok(json!({
        "status": "ok",
        "summary": format!("{} tab(s) in Saccade MCP session state", state.tabs.len()),
        "source": "mcp_session_state",
        "tabs": state.tabs,
    }))
}

fn tabs_open_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    open_local_tool(state, arguments)
}

fn tabs_open_agent_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let url = required_url_arg(&arguments)?;
    if !matches!(url.scheme(), "http" | "https" | "file") {
        bail!("open_agent allows only http, https, or file URLs");
    }
    let pointer = current_agent_pointer_path()?;
    let mut was_running = false;
    let mut previous_browser_tab_id = None;
    let mut running_endpoint = None;

    if pointer.exists() {
        let grant_path = current_agent_grant_path()?;
        let (_, broker) = read_current_tab_grant(&grant_path.display().to_string())?;
        let endpoint = dogfood_control_endpoint_from_grant(&broker)?
            .context("running Saccade broker is missing its control endpoint")?;
        if call_dogfood_control_ping(&endpoint).is_ok() {
            was_running = true;
            running_endpoint = Some(endpoint.clone());
            previous_browser_tab_id = broker
                .get("tab_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            call_dogfood_control(&endpoint, "open_agent_tab", json!({"url": url.as_str()}))?;
        } else {
            // A killed/crashed app can leave only its owner-only pointer. It is
            // not a running broker, so retire the pointer and start cleanly.
            fs::remove_file(&pointer).with_context(|| {
                format!(
                    "failed to retire stale Saccade pointer {}",
                    pointer.display()
                )
            })?;
        }
    }
    if !was_running {
        let executable = std::env::var_os("SACCADE_APP_EXECUTABLE")
            .map(PathBuf::from)
            .context("SACCADE_APP_EXECUTABLE is not configured by the packaged MCP launcher")?;
        if !executable.is_absolute() || !executable.is_file() {
            bail!(
                "configured Saccade app executable is unavailable: {}",
                executable.display()
            );
        }
        let agent_root = pointer
            .parent()
            .context("Saccade broker pointer has no parent directory")?;
        fs::create_dir_all(agent_root)
            .with_context(|| format!("failed to create {}", agent_root.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(agent_root, fs::Permissions::from_mode(0o700))?;
        }
        let launch_id = format!("{}.{}", std::process::id(), unix_ms()?);
        let session = agent_root.join(format!("session.mcp.{launch_id}"));
        fs::create_dir(&session)
            .with_context(|| format!("failed to create {}", session.display()))?;
        let socket_session = PathBuf::from("/private/tmp").join(format!("saccade-mcp.{launch_id}"));
        fs::create_dir(&socket_session)
            .with_context(|| format!("failed to create {}", socket_session.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&session, fs::Permissions::from_mode(0o700))?;
            fs::set_permissions(&socket_session, fs::Permissions::from_mode(0o700))?;
        }
        let grant_path = session.join("grant.json");
        let socket_path = socket_session.join("control.sock");
        let replay_path = session.join("replay.jsonl");
        let pointer_temp = agent_root.join(format!("current-grant-path.tmp.{launch_id}"));
        let mut pointer_file = {
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&pointer_temp)
            }
            #[cfg(not(unix))]
            {
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&pointer_temp)
            }
        }
        .with_context(|| format!("failed to create {}", pointer_temp.display()))?;
        writeln!(pointer_file, "{}", grant_path.display())?;
        pointer_file.sync_all()?;
        fs::rename(&pointer_temp, &pointer)
            .with_context(|| format!("failed to publish {}", pointer.display()))?;

        let spawn_result = ProcessCommand::new(&executable)
            .arg(format!("--url={}", url.as_str()))
            .args([
                "--use-native",
                "--no-first-run",
                "--no-default-browser-check",
                "--window-size=1440,1000",
            ])
            .env("SACCADE_ENGINE_INITIAL_TAB_GRANT", "1")
            .env("SACCADE_ENGINE_INITIAL_URL", url.as_str())
            .env("SACCADE_ENGINE_BROKER", "1")
            .env("SACCADE_ENGINE_SOCKET", &socket_path)
            .env("SACCADE_ENGINE_GRANT_PATH", &grant_path)
            .env("SACCADE_ENGINE_REPLAY_PATH", &replay_path)
            .env("SACCADE_ENGINE_CURRENT_POINTER", &pointer)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Err(error) = spawn_result {
            let _ = fs::remove_file(&pointer);
            let _ = fs::remove_dir_all(&session);
            let _ = fs::remove_dir_all(&socket_session);
            return Err(error).with_context(|| format!("failed to start {}", executable.display()));
        }
    }

    let deadline = Instant::now() + Duration::from_secs(12);
    let mut last_running_ping = Instant::now();
    let grant_path = loop {
        if pointer.exists() {
            if let Ok(path) = current_agent_grant_path()
                && let Ok((_, grant)) = read_current_tab_grant(&path.display().to_string())
                && grant.get("status").and_then(Value::as_str) == Some("granted")
                && grant.get("grant_type").and_then(Value::as_str) == Some("agent_created_tab")
                && grant
                    .get("tab_id")
                    .and_then(Value::as_str)
                    .is_some_and(|current| {
                        !current.is_empty() && previous_browser_tab_id.as_deref() != Some(current)
                    })
                && grant
                    .get("url")
                    .and_then(Value::as_str)
                    .and_then(|value| Url::parse(value).ok())
                    .is_some_and(|current| {
                        if url.scheme() == "file" {
                            current == url
                        } else {
                            current.scheme() == url.scheme() && current.host_str() == url.host_str()
                        }
                    })
                && dogfood_control_endpoint_from_grant(&grant)
                    .ok()
                    .flatten()
                    .is_some_and(|endpoint| call_dogfood_control_ping(&endpoint).is_ok())
            {
                break path;
            }
        }
        if was_running
            && last_running_ping.elapsed() >= Duration::from_millis(500)
            && running_endpoint
                .as_ref()
                .is_some_and(|endpoint| call_dogfood_control_ping(endpoint).is_err())
        {
            let _ = fs::remove_file(&pointer);
            return tabs_open_agent_tool(state, json!({"url": url.as_str()}));
        }
        if last_running_ping.elapsed() >= Duration::from_millis(500) {
            last_running_ping = Instant::now();
        }
        if Instant::now() >= deadline {
            let cleaned = cleanup_failed_agent_tab(&url, previous_browser_tab_id.as_deref());
            bail!(
                "Saccade did not publish the new Agent tab within 12 seconds; cleanup_succeeded={cleaned}"
            );
        }
        thread::sleep(Duration::from_millis(100));
    };

    let attach_result = tabs_grant_current_tool(
        state,
        json!({
            "grant_path": grant_path.display().to_string(),
            "reason": "LLM opened a dedicated Agent tab",
        }),
    );
    let attached = match attach_result {
        Ok(attached) => attached,
        Err(error) => {
            let cleaned = cleanup_failed_agent_tab(&url, previous_browser_tab_id.as_deref());
            bail!("failed to attach new Agent tab; cleanup_succeeded={cleaned}: {error}");
        }
    };
    minimal_open_agent_response(&attached, was_running)
}

fn minimal_open_agent_response(attached: &Value, was_running: bool) -> Result<Value> {
    let tab = attached
        .get("tab")
        .context("attached Agent tab response is missing tab")?;
    let tab_id = tab
        .get("tab_id")
        .cloned()
        .context("attached Agent tab response is missing tab_id")?;
    let page_revision = tab
        .get("page_revision")
        .cloned()
        .context("attached Agent tab response is missing page_revision")?;
    let owner = tab.get("owner").cloned().unwrap_or_else(|| json!("agent"));
    Ok(json!({
        "status": "ok",
        "summary": "Agent tab ready for direct read or action",
        "ready": attached.get("ready_for_read").and_then(Value::as_bool) == Some(true),
        "browser_was_running": was_running,
        "agent_input_grant": true,
        "tab": {
            "tab_id": tab_id,
            "owner": owner,
            "page_revision": page_revision,
        }
    }))
}

fn cleanup_failed_agent_tab(requested_url: &Url, previous_browser_tab_id: Option<&str>) -> bool {
    let Ok(grant_path) = current_agent_grant_path() else {
        return false;
    };
    let Ok((_, grant)) = read_current_tab_grant(&grant_path.display().to_string()) else {
        return false;
    };
    if grant.get("status").and_then(Value::as_str) != Some("granted")
        || grant.get("grant_type").and_then(Value::as_str) != Some("agent_created_tab")
    {
        return false;
    }
    let Some(tab_id) = grant
        .get("tab_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    if previous_browser_tab_id == Some(tab_id) {
        return false;
    }
    let Some(current_url) = grant
        .get("url")
        .and_then(Value::as_str)
        .and_then(|value| Url::parse(value).ok())
    else {
        return false;
    };
    let same_target = if requested_url.scheme() == "file" {
        current_url == *requested_url
    } else {
        current_url.scheme() == requested_url.scheme()
            && current_url.host_str() == requested_url.host_str()
    };
    if !same_target {
        return false;
    }
    let Ok(Some(endpoint)) = dogfood_control_endpoint_from_grant(&grant) else {
        return false;
    };
    call_dogfood_control_ping(&endpoint).is_ok()
        && call_dogfood_control(&endpoint, "close", json!({})).is_ok()
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
    owner: TabOwner,
    source: &'static str,
    grant_path: Option<String>,
    control_endpoint: Option<DogfoodControlEndpoint>,
}

#[derive(Debug, Clone)]
struct DogfoodControlEndpoint {
    host: String,
    port: u16,
    protocol: String,
    capability: String,
    engine_grant: Option<EngineGrant>,
}

fn tabs_grant_current_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let grant = current_tab_grant_from_args(&arguments)?;
    let same_webview_control = grant
        .control_endpoint
        .as_ref()
        .map(call_dogfood_control_ping)
        .transpose()?;
    let same_webview_control_ping = same_webview_control.is_some();
    let same_webview_control_capabilities =
        control_capabilities_from_ping(same_webview_control.as_ref());
    let advertised_same_webview_capabilities =
        advertised_same_webview_capabilities(&same_webview_control_capabilities);

    let tab_id = state.allocate_tab_id();
    let info = tab(
        tab_id.0,
        grant.owner,
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
        if same_webview_control_capabilities
            .iter()
            .any(|capability| capability == "resume")
        {
            call_dogfood_control(endpoint, "resume", json!({}))?;
        }
        let initial_method = if same_webview_control_capabilities
            .iter()
            .any(|capability| capability == "truth")
        {
            "truth"
        } else if same_webview_control_capabilities
            .iter()
            .any(|capability| capability == "shell_status")
        {
            "shell_status"
        } else {
            bail!("browser control endpoint must advertise truth or shell_status");
        };
        let mut live_truth = call_dogfood_control(endpoint, initial_method, json!({}))?;
        let collector_expected = initial_method == "truth"
            && same_webview_control_capabilities
                .iter()
                .any(|capability| capability == "article_text")
            && matches!(grant.url.scheme(), "http" | "https" | "file");
        if collector_expected {
            let deadline = Instant::now() + COLLECTOR_READY_TIMEOUT;
            while live_truth.get("collector_ready").and_then(Value::as_bool) != Some(true) {
                if Instant::now() >= deadline {
                    bail!("Saccade current-tab collector did not become ready within 12 seconds");
                }
                thread::sleep(Duration::from_millis(50));
                live_truth = call_dogfood_control(endpoint, initial_method, json!({}))?;
            }
        }
        state.dogfood_controls.insert(tab_id.0, endpoint.clone());
        state.dogfood_control_runtimes.insert(
            tab_id.0,
            control_runtime_from_ping(same_webview_control.as_ref(), endpoint),
        );
        state
            .dogfood_control_capabilities
            .insert(tab_id.0, same_webview_control_capabilities.clone());
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
        "summary": if grant.owner == TabOwner::Agent {
            "Agent-created Saccade tab opened On and attached to the live browser session"
        } else {
            "current Human tab attached to live Saccade co-pilot session after explicit grant"
        },
        "runtime": tab_runtime(state, tab_id),
        "selected_tab_seen": true,
        "grant_required": grant.owner == TabOwner::Human,
        "grant_given": true,
        "agent_input_grant": true,
        "reason": grant.reason,
        "source": grant.source,
        "grant_path": grant.grant_path,
        "same_webview_control_ping": same_webview_control_ping,
        "same_webview_control": same_webview_control.clone(),
        "same_webview_attached": attached_via_control,
        "collector_ready": live_truth.get("collector_ready").and_then(Value::as_bool),
        "ready_for_read": live_truth.get("collector_ready").and_then(Value::as_bool) == Some(true),
        "same_webview_capabilities": if attached_via_control {
            json!(advertised_same_webview_capabilities)
        } else {
            json!([])
        },
        "transport_status": transport_status,
        "transport_note": if attached_via_control {
            "MCP v0 validates the visible browser grant and routes only advertised same-WebView control capabilities through the granted browser endpoint."
        } else {
            "MCP v0 validates the visible browser grant and can ping the same dogfood WebView control endpoint when present. Truth/actions use a live worker when no control endpoint is available."
        },
        "tab": tab.info,
        "site_policy": classify_site_url(&tab.info.url),
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
    } else if arguments.get("url").is_none() {
        current_tab_grant_from_discovery(arguments)
    } else {
        current_tab_grant_from_direct_args(arguments)
    }
}

fn current_tab_grant_from_discovery(arguments: &Value) -> Result<CurrentTabGrantRequest> {
    let pointer = current_agent_pointer_path()?;
    let (grant_path, endpoint) = current_agent_broker_endpoint()?;
    if let Some(browser_tab_id) = arguments.get("browser_tab_id").and_then(Value::as_str) {
        call_dogfood_control(
            &endpoint,
            "select_tab",
            json!({"browser_tab_id": browser_tab_id}),
        )
        .with_context(|| format!("failed to select Saccade tab {browser_tab_id:?}"))?;
    }
    let mut discovered = arguments.clone();
    let object = discovered
        .as_object_mut()
        .context("grant_current arguments must be an object")?;
    object.insert(
        "grant_path".to_string(),
        Value::String(grant_path.display().to_string()),
    );
    object.entry("reason".to_string()).or_insert_with(|| {
        Value::String(if arguments.get("browser_tab_id").is_some() {
            "agent selected an eligible Agent On Saccade tab".to_string()
        } else {
            "user enabled Agent access for the current Saccade tab".to_string()
        })
    });
    object.remove("browser_tab_id");
    let mut grant = current_tab_grant_from_artifact(&discovered).with_context(|| {
        format!(
            "current Saccade tab is not granted; turn on its Agent switch ({})",
            pointer.display()
        )
    })?;
    grant.source = if arguments.get("browser_tab_id").is_some() {
        "current_agent_tab_registry"
    } else {
        "current_agent_pointer"
    };
    Ok(grant)
}

fn current_agent_broker_endpoint() -> Result<(PathBuf, DogfoodControlEndpoint)> {
    let grant_path = current_agent_grant_path()?;
    let (_, broker) = read_current_tab_grant(&grant_path.display().to_string())?;
    let endpoint = dogfood_control_endpoint_from_grant(&broker)?
        .context("running Saccade broker is missing its control endpoint")?;
    Ok((grant_path, endpoint))
}

fn current_agent_pointer_path() -> Result<PathBuf> {
    std::env::var_os("SACCADE_CURRENT_AGENT_POINTER")
        .map(PathBuf::from)
        .context("packaged Saccade MCP launcher did not configure its broker pointer")
}

fn current_agent_grant_path() -> Result<PathBuf> {
    let pointer = current_agent_pointer_path()?;
    let metadata = fs::symlink_metadata(&pointer).with_context(|| {
        format!(
            "no running Saccade collaboration session at {}",
            pointer.display()
        )
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("Saccade current-agent pointer must be a regular file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            bail!("Saccade current-agent pointer must be owner-only");
        }
    }
    let raw = fs::read_to_string(&pointer)
        .with_context(|| format!("failed to read {}", pointer.display()))?;
    let grant_path = raw.trim();
    if grant_path.is_empty() || grant_path.len() > 4096 || grant_path.contains('\0') {
        bail!("Saccade current-agent pointer is invalid");
    }
    let grant_path = PathBuf::from(grant_path);
    if !grant_path.is_absolute() {
        bail!("Saccade current-agent pointer must contain an absolute grant path");
    }
    Ok(grant_path)
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
        owner: TabOwner::Human,
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
    let (grant_path, grant) = read_current_tab_grant(grant_path_arg)?;

    if grant.get("status").and_then(Value::as_str) != Some("granted") {
        bail!("grant artifact status is not granted");
    }
    let grant_type = grant.get("grant_type").and_then(Value::as_str);
    let owner = match grant_type {
        Some("current_tab_copilot") => TabOwner::Human,
        Some("agent_created_tab") => TabOwner::Agent,
        _ => bail!("grant artifact is not a supported per-tab grant"),
    };
    let expected_grant_required = owner == TabOwner::Human;
    if grant.get("selected_tab_seen").and_then(Value::as_bool) != Some(true)
        || grant.get("grant_required").and_then(Value::as_bool) != Some(expected_grant_required)
        || grant.get("grant_given").and_then(Value::as_bool) != Some(true)
    {
        bail!("grant artifact is missing selected-tab grant evidence");
    }
    let expected_owner = if owner == TabOwner::Agent {
        "agent"
    } else {
        "human"
    };
    let artifact_owner = grant
        .get("owner")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);
    if artifact_owner.as_deref() != Some(expected_owner) {
        bail!("grant artifact owner does not match its grant type");
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
    let control_endpoint = dogfood_control_endpoint_from_grant(&grant)?;
    let trusted_remote_control_grant =
        is_chrome_compatibility_grant(&grant, control_endpoint.as_ref())
            || is_official_servoshell_bridge_grant(&grant, control_endpoint.as_ref())
            || is_engine_adapter_grant(control_endpoint.as_ref());
    if !is_local_dev_url(&url) && !trusted_remote_control_grant {
        bail!(
            "grant artifact URL must be localhost, loopback, file, or an explicit trusted browser-control grant: {url}"
        );
    }
    let reason = arguments
        .get("reason")
        .and_then(Value::as_str)
        .filter(|reason| !reason.trim().is_empty())
        .map(str::trim)
        .unwrap_or("dogfood browser current-tab grant artifact");
    let read_grant = read_grant_from_grant_value(grant.get("read_grant").and_then(Value::as_str))?;
    Ok(CurrentTabGrantRequest {
        url,
        reason: reason.to_string(),
        read_grant,
        owner,
        source: "grant_artifact",
        grant_path: Some(grant_path.display().to_string()),
        control_endpoint,
    })
}

fn read_current_tab_grant(path: &str) -> Result<(PathBuf, Value)> {
    if let Ok(workspace_path) = safe_workspace_path(path) {
        let mut grant: Value = serde_json::from_slice(
            &fs::read(&workspace_path)
                .with_context(|| format!("failed to read {}", workspace_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", workspace_path.display()))?;
        if grant.get("engine_adapter").is_some() {
            grant = read_owner_only_grant(&workspace_path)
                .map_err(|error| anyhow!("failed to read owner-only engine grant: {error}"))?;
        }
        return Ok((workspace_path, grant));
    }

    let owner_path = PathBuf::from(path);
    if !owner_path.is_absolute() {
        bail!("owner-only grant path outside the workspace must be absolute");
    }
    let grant = read_owner_only_grant(&owner_path)
        .map_err(|error| anyhow!("failed to read owner-only engine grant: {error}"))?;
    Ok((owner_path, grant))
}

fn is_engine_adapter_grant(control_endpoint: Option<&DogfoodControlEndpoint>) -> bool {
    control_endpoint.is_some_and(|endpoint| endpoint.engine_grant.is_some())
}

fn is_chrome_compatibility_grant(
    grant: &Value,
    control_endpoint: Option<&DogfoodControlEndpoint>,
) -> bool {
    control_endpoint.is_some()
        && grant.get("runtime").and_then(Value::as_str) == Some("saccade-chrome-compat-cdp-v0")
        && grant.get("rendering_profile").and_then(Value::as_str) == Some("chrome-compatibility")
        && grant.get("transport_status").and_then(Value::as_str)
            == Some("chrome_compatibility_control_v0")
}

fn is_official_servoshell_bridge_grant(
    grant: &Value,
    control_endpoint: Option<&DogfoodControlEndpoint>,
) -> bool {
    control_endpoint.is_some()
        && grant.get("runtime").and_then(Value::as_str) == Some("saccade-servoshell-bridge-v0")
        && grant.get("rendering_profile").and_then(Value::as_str) == Some("official-servoshell")
        && grant.get("transport_status").and_then(Value::as_str)
            == Some("official_servoshell_bridge_control_v0")
        && grant
            .pointer("/copilot/page_dom_injected")
            .and_then(Value::as_bool)
            == Some(false)
        && grant
            .pointer("/copilot/sensitive_values_exposed_to_agent")
            .and_then(Value::as_bool)
            == Some(false)
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
    if protocol == CONTROL_PROTOCOL_VERSION {
        let engine_grant: EngineGrant = serde_json::from_value(json!({
            "engine_adapter": grant.get("engine_adapter").cloned().unwrap_or(Value::Null),
            "control_endpoint": endpoint,
            "control_capability": grant.get("control_capability").cloned().unwrap_or(Value::Null),
        }))
        .context("failed to parse engine-neutral control grant")?;
        engine_grant
            .validate()
            .map_err(|error| anyhow!("invalid engine-neutral control grant: {error}"))?;
        let capability = engine_grant.control_capability.token.clone();
        return Ok(Some(DogfoodControlEndpoint {
            host: String::new(),
            port: 0,
            protocol: protocol.to_string(),
            capability,
            engine_grant: Some(engine_grant),
        }));
    }
    if protocol != "saccade-dogfood-control-v1" {
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
    let capability = grant
        .pointer("/control_capability/token")
        .and_then(Value::as_str)
        .context("control grant must include a session capability token")?;
    if grant
        .pointer("/control_capability/scheme")
        .and_then(Value::as_str)
        != Some("saccade_session_bearer_v1")
    {
        bail!("control grant must use saccade_session_bearer_v1");
    }
    if capability.len() < 32 {
        bail!("control grant capability token is too short");
    }
    Ok(Some(DogfoodControlEndpoint {
        host: host.to_string(),
        port: port as u16,
        protocol: protocol.to_string(),
        capability: capability.to_string(),
        engine_grant: None,
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

fn control_runtime_from_ping(ping: Option<&Value>, endpoint: &DogfoodControlEndpoint) -> String {
    ping.and_then(|value| value.get("runtime"))
        .and_then(Value::as_str)
        .filter(|runtime| !runtime.trim().is_empty())
        .unwrap_or(endpoint.protocol.as_str())
        .to_string()
}

fn control_capabilities_from_ping(ping: Option<&Value>) -> Vec<String> {
    ping.and_then(|value| value.get("capabilities"))
        .and_then(Value::as_array)
        .map(|capabilities| {
            capabilities
                .iter()
                .filter_map(Value::as_str)
                .filter(|capability| !capability.trim().is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|capabilities| !capabilities.is_empty())
        .unwrap_or_else(default_dogfood_control_capabilities)
}

fn default_dogfood_control_capabilities() -> Vec<String> {
    [
        "ping",
        "shell_status",
        "tab_registry",
        "select_tab",
        "resume",
        "truth",
        "article_text",
        "actions",
        "navigate",
        "back",
        "forward",
        "reload",
        "fill_agent_fields",
        "inspect_fields",
        "render_preflight",
        "downloads",
        "act",
        "formmax_live_fill",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect()
}

fn advertised_same_webview_capabilities(capabilities: &[String]) -> Vec<String> {
    let mut advertised = Vec::new();
    for capability in capabilities {
        push_unique_capability(&mut advertised, capability.as_str());
    }
    if capabilities.iter().any(|capability| {
        matches!(
            capability.as_str(),
            "shell_status" | "navigate" | "back" | "forward" | "reload"
        )
    }) {
        push_unique_capability(&mut advertised, "saccade.browser.navigate");
    }
    advertised
}

fn push_unique_capability(capabilities: &mut Vec<String>, capability: &str) {
    if !capabilities.iter().any(|existing| existing == capability) {
        capabilities.push(capability.to_string());
    }
}

fn ensure_dogfood_control_capability(
    state: &McpSessionState,
    tab_id: TabId,
    method: &str,
) -> Result<()> {
    let Some(capabilities) = state.dogfood_control_capabilities.get(&tab_id.0) else {
        return Ok(());
    };
    if capabilities
        .iter()
        .any(|capability| capability.as_str() == method)
    {
        return Ok(());
    }
    bail!(
        "same-WebView control endpoint for tab_id {} does not advertise capability {method:?}",
        tab_id.0
    )
}

fn call_dogfood_control(
    endpoint: &DogfoodControlEndpoint,
    method: &str,
    params: Value,
) -> Result<Value> {
    if let Some(grant) = endpoint.engine_grant.as_ref() {
        let read_timeout = match method {
            "protected_fill" => Duration::from_secs(180),
            "formmax_live_fill" | "fill_agent_fields" | "inspect_fields" | "form_inventory"
            | "form_compile_plan" | "form_execute_plan" | "act" => Duration::from_secs(65),
            _ => Duration::from_secs(5),
        };
        return call_control(grant, method, params, read_timeout)
            .map_err(|error| engine_control_error(method, error));
    }
    let addr = dogfood_control_socket_addr(endpoint)?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
        .with_context(|| format!("failed to connect dogfood control endpoint {addr}"))?;
    let read_timeout = match method {
        "protected_fill" => Duration::from_secs(180),
        "formmax_live_fill" | "fill_agent_fields" | "inspect_fields" | "form_inventory"
        | "form_compile_plan" | "form_execute_plan" | "act" => Duration::from_secs(65),
        _ => Duration::from_secs(5),
    };
    stream
        .set_read_timeout(Some(read_timeout))
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
            "capability": endpoint.capability,
        })
    )
    .with_context(|| format!("failed to write dogfood control {method}"))?;
    stream
        .flush()
        .with_context(|| format!("failed to flush dogfood control {method}"))?;
    stream
        .shutdown(Shutdown::Write)
        .with_context(|| format!("failed to finish dogfood control {method} request"))?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| anyhow!("failed to read dogfood control {method} response: {error}"))?;
    let response: Value = serde_json::from_str(&line)
        .with_context(|| format!("failed to parse dogfood control {method} response"))?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        let error = response.get("error");
        let code = error
            .and_then(|value| value.get("code"))
            .and_then(Value::as_str);
        let detail = error
            .and_then(|value| value.get("detail"))
            .and_then(Value::as_str)
            .or_else(|| error.and_then(Value::as_str))
            .unwrap_or("dogfood control request failed");
        if let Some(code) = code {
            bail!("{code}: {detail}");
        }
        bail!("{detail}");
    }
    Ok(response.get("result").cloned().unwrap_or(Value::Null))
}

fn engine_control_error(method: &str, error: EngineApiError) -> anyhow::Error {
    let category = match error.code {
        EngineErrorCode::InvalidArgument => "invalid engine request",
        EngineErrorCode::PermissionDenied => "policy denied engine request",
        EngineErrorCode::ConsentRequired => "CONSENT_REQUIRED",
        EngineErrorCode::AgentPaused => "AGENT_PAUSED",
        EngineErrorCode::UnsupportedCapability => "unsupported engine capability",
        EngineErrorCode::StalePageRevision => "stale engine page revision",
        EngineErrorCode::StaleLayout => "stale engine layout",
        EngineErrorCode::TabNotFound => "engine tab not found",
        EngineErrorCode::Timeout => "engine control timeout",
        EngineErrorCode::TransportUnavailable => "engine transport unavailable",
        EngineErrorCode::Conflict => "engine request conflict",
        EngineErrorCode::PolicyBlocked => "policy blocked engine request",
        EngineErrorCode::FormCommandFailed => "engine form command failed",
        EngineErrorCode::PostconditionFailed => "engine postcondition failed",
        EngineErrorCode::ScreenshotBusy => "engine screenshot busy",
        EngineErrorCode::ScreenshotFailed => "engine screenshot failed",
        EngineErrorCode::HumanVerificationRequired => "HUMAN_VERIFICATION_REQUIRED",
        EngineErrorCode::ProviderRejected => "PROVIDER_REJECTED",
        EngineErrorCode::Internal => "engine internal error",
    };
    anyhow!("{category} for {method}: {}", error.detail)
}

fn dogfood_control_socket_addr(endpoint: &DogfoodControlEndpoint) -> Result<SocketAddr> {
    if endpoint.engine_grant.is_some() {
        bail!("engine-neutral control endpoint does not use a TCP socket address");
    }
    let ip = match endpoint.host.as_str() {
        "127.0.0.1" | "localhost" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        "::1" => IpAddr::V6(Ipv6Addr::LOCALHOST),
        other => bail!("control endpoint host must be loopback; got {other:?}"),
    };
    Ok(SocketAddr::new(ip, endpoint.port))
}

fn read_grant_from_grant_value(value: Option<&str>) -> Result<ReadGrant> {
    let raw = value.unwrap_or("full_truth");
    match canonical_ascii_token(raw).as_str() {
        "visiblesummaryonly" | "visible_summary_only" => Ok(ReadGrant::VisibleSummaryOnly),
        "fulltruth" | "full_truth" => Ok(ReadGrant::FullTruth),
        other => bail!(
            "unsupported read_grant {raw:?} ({other:?}); expected full_truth or visible_summary_only"
        ),
    }
}

fn classify_site_url(url: &str) -> SitePolicy {
    let owned_domains = runtime_owned_domains();
    classify_site_url_with_owned_domains(url, &owned_domains)
}

fn runtime_owned_domains() -> Vec<String> {
    std::env::var("SACCADE_OWNED_DOMAINS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|domain| !domain.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
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
    if state
        .dogfood_control_capabilities
        .get(&tab_id.0)
        .is_some_and(|capabilities| capabilities.iter().any(|item| item == "pause"))
    {
        let endpoint = state
            .dogfood_controls
            .get(&tab_id.0)
            .cloned()
            .with_context(|| format!("tab_id {} has no browser control endpoint", tab_id.0))?;
        call_dogfood_control(&endpoint, "pause", json!({}))?;
    }
    let tab = state
        .find_tab_mut(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
    tab.paused = true;
    Ok(json!({
        "status": "ok",
        "summary": "agent runtime paused for tab; human Agent On permission is unchanged",
        "agent_permission_unchanged": true,
        "agent_activity": "paused",
        "tab": tab,
    }))
}

fn tabs_close_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    if let Some(mut worker) = state.browser_workers.remove(&tab_id.0) {
        worker.close();
    }
    let close_result = if state
        .dogfood_control_capabilities
        .get(&tab_id.0)
        .is_some_and(|capabilities| capabilities.iter().any(|item| item == "close"))
    {
        state
            .dogfood_controls
            .get(&tab_id.0)
            .cloned()
            .map(|endpoint| call_dogfood_control(&endpoint, "close", json!({})))
            .transpose()?
    } else {
        None
    };
    state.dogfood_controls.remove(&tab_id.0);
    state.dogfood_control_runtimes.remove(&tab_id.0);
    state.dogfood_control_capabilities.remove(&tab_id.0);
    let before = state.tabs.len();
    state.tabs.retain(|tab| tab.info.tab_id != tab_id);
    if state.tabs.len() == before {
        bail!("unknown tab_id {}", tab_id.0);
    }
    Ok(json!({
        "status": "ok",
        "summary": "tab closed in Saccade MCP session state",
        "tab_id": tab_id.0,
        "browser_close": close_result,
    }))
}

fn web_truth_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    ensure_truth_allowed(state, tab_id)?;
    if let Some(endpoint) = state.dogfood_controls.get(&tab_id.0).cloned() {
        ensure_dogfood_control_capability(state, tab_id, "truth")?;
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
    let site_policy = classify_site_url(&tab.info.url);

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
        "site_policy": site_policy,
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

fn web_article_text_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    ensure_truth_allowed(state, tab_id)?;
    let basis_page_revision = arguments
        .get("basis_page_revision")
        .and_then(Value::as_u64)
        .context("saccade.web.article_text requires integer basis_page_revision")?;
    let current_revision = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?
        .info
        .page_revision;
    if basis_page_revision != current_revision {
        bail!(
            "stale MCP article basis: requested {}, current {}",
            basis_page_revision,
            current_revision
        );
    }
    let max_chars = arguments
        .get("max_chars")
        .and_then(Value::as_u64)
        .unwrap_or(20_000)
        .clamp(1_000, 100_000) as usize;
    let response_mode = arguments
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("minimal");
    if !matches!(response_mode, "minimal" | "compact" | "evidence") {
        bail!("saccade.web.article_text mode must be minimal, compact, or evidence");
    }
    let endpoint = state
        .dogfood_controls
        .get(&tab_id.0)
        .cloned()
        .context("saccade.web.article_text requires a granted browser current tab")?;
    ensure_dogfood_control_capability(state, tab_id, "article_text")?;
    let mut result = call_dogfood_control(
        &endpoint,
        "article_text",
        json!({
            "basis_page_revision": basis_page_revision,
            "max_chars": max_chars,
        }),
    )?;
    if let Some(tab) = state.find_tab_mut(tab_id) {
        update_session_tab_from_browser_result(tab, &result);
    }
    let text = result
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let original_chars = text.chars().count();
    let bounded = text.chars().take(max_chars).collect::<String>();
    let text_truncated = original_chars > max_chars
        || result
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    if let Some(object) = result.as_object_mut() {
        object.insert("text".to_string(), Value::String(bounded));
    }
    if response_mode == "minimal" {
        return Ok(minimal_article_response(
            &result,
            basis_page_revision,
            text_truncated,
        ));
    }
    if let Some(object) = result.as_object_mut() {
        object.insert(
            "text_chars_returned".to_string(),
            json!(original_chars.min(max_chars)),
        );
        object.insert("text_truncated".to_string(), json!(text_truncated));
        object.insert("max_chars".to_string(), json!(max_chars));
        object.insert("tab_id".to_string(), json!(tab_id.0));
    }
    if response_mode == "evidence" {
        if let Some(object) = result.as_object_mut() {
            object.insert("response_mode".to_string(), json!("evidence"));
        }
        return Ok(result);
    }
    Ok(json!({
        "text": result.get("text").cloned().unwrap_or_else(|| json!("")),
        "source_url": result.get("source_url").cloned().unwrap_or(Value::Null),
        "source_title": result.get("source_title").cloned().unwrap_or(Value::Null),
        "page_revision": result.get("page_revision").cloned().unwrap_or(json!(basis_page_revision)),
        "text_truncated": result.get("text_truncated").cloned().unwrap_or(json!(false)),
        "provenance": {
            "page_content_may_authorize_actions": false
        },
        "response_mode": "compact"
    }))
}

fn minimal_article_response(
    result: &Value,
    basis_page_revision: u64,
    text_truncated: bool,
) -> Value {
    let mut response = json!({
        "text": result.get("text").cloned().unwrap_or_else(|| json!("")),
        "page_revision": result
            .get("page_revision")
            .cloned()
            .unwrap_or_else(|| json!(basis_page_revision)),
        "untrusted": true,
    });
    if text_truncated {
        response["truncated"] = json!(true);
    }
    response
}

fn web_actions_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    ensure_truth_allowed(state, tab_id)?;
    let mut layout_epoch = Value::Null;
    let mut revision_cause = Value::Null;
    if let Some(endpoint) = state.dogfood_controls.get(&tab_id.0).cloned() {
        ensure_dogfood_control_capability(state, tab_id, "actions")?;
        let live_actions = call_dogfood_control(&endpoint, "actions", json!({}))?;
        layout_epoch = live_actions
            .get("layout_epoch")
            .cloned()
            .unwrap_or(Value::Null);
        revision_cause = live_actions
            .get("revision_cause")
            .cloned()
            .unwrap_or(Value::Null);
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
    let site_policy = classify_site_url(&tab.info.url);
    Ok(json!({
        "status": "ok",
        "summary": format!("{} action(s) in current action map", tab.last_actions.len()),
        "tab_id": tab_id.0,
        "page_revision": tab.info.page_revision,
        "layout_epoch": layout_epoch,
        "revision_cause": revision_cause,
        "actions": tab.last_actions,
        "site_policy": site_policy,
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
    let requested_layout_epoch = arguments.get("basis_layout_epoch").and_then(Value::as_u64);
    ensure_agent_input_allowed(state, tab_id)?;
    if let Some(endpoint) = state.dogfood_controls.get(&tab_id.0).cloned() {
        ensure_dogfood_control_capability(state, tab_id, "act")?;
        ensure_dogfood_control_capability(state, tab_id, "actions")?;
        ensure_dogfood_control_capability(state, tab_id, "next_receipt")?;
        let numeric_u64 = |value: Option<&Value>| {
            value
                .and_then(Value::as_u64)
                .or_else(|| value.and_then(Value::as_f64).map(|number| number as u64))
        };
        let mut fresh_actions = call_dogfood_control(&endpoint, "actions", json!({}))?;
        let mut effective_revision = numeric_u64(fresh_actions.get("page_revision"))
            .context("live action map is missing page_revision")?;
        let mut effective_layout_epoch =
            numeric_u64(fresh_actions.get("layout_epoch")).unwrap_or(0);
        let mut layout_rebased = effective_revision != basis_page_revision
            || requested_layout_epoch.is_some_and(|epoch| epoch != effective_layout_epoch);
        let mut action_still_present = fresh_actions
            .get("actions")
            .and_then(Value::as_array)
            .is_some_and(|actions| {
                actions.iter().any(|action| {
                    action.get("action_id").and_then(Value::as_str) == Some(action_id.as_str())
                })
            });
        if layout_rebased
            && fresh_actions.get("revision_cause").and_then(Value::as_str) != Some("layout")
        {
            bail!(
                "stale page basis: requested {}, current {}",
                basis_page_revision,
                effective_revision
            );
        }
        if !action_still_present {
            bail!("stale layout removed the requested action {action_id:?}");
        }
        let mut act_result = call_dogfood_control(
            &endpoint,
            "act",
            json!({
                "action_id": action_id.clone(),
                "basis_page_revision": effective_revision,
                "basis_layout_epoch": effective_layout_epoch,
            }),
        );
        if act_result
            .as_ref()
            .err()
            .is_some_and(|error| error.to_string().contains("stale engine layout"))
        {
            fresh_actions = call_dogfood_control(&endpoint, "actions", json!({}))?;
            effective_revision = numeric_u64(fresh_actions.get("page_revision"))
                .context("refreshed action map is missing page_revision")?;
            effective_layout_epoch = numeric_u64(fresh_actions.get("layout_epoch")).unwrap_or(0);
            action_still_present = fresh_actions
                .get("actions")
                .and_then(Value::as_array)
                .is_some_and(|actions| {
                    actions.iter().any(|action| {
                        action.get("action_id").and_then(Value::as_str) == Some(action_id.as_str())
                    })
                });
            if !action_still_present {
                bail!("stale layout removed the requested action {action_id:?}");
            }
            layout_rebased = true;
            act_result = call_dogfood_control(
                &endpoint,
                "act",
                json!({
                    "action_id": action_id.clone(),
                    "basis_page_revision": effective_revision,
                    "basis_layout_epoch": effective_layout_epoch,
                }),
            );
        }
        let live_act = act_result?;
        let receipt = call_dogfood_control(&endpoint, "next_receipt", json!({"timeout_ms": 3000}))?;
        if receipt.get("action_id").and_then(Value::as_str) != Some(action_id.as_str())
            || receipt.get("verified").and_then(Value::as_bool) != Some(true)
            || receipt.get("status").and_then(Value::as_str) != Some("applied")
        {
            bail!("native input receipt did not verify action {action_id:?}");
        }
        let opens_new_context =
            live_act.get("opens_new_context").and_then(Value::as_bool) == Some(true);
        let mut destination_ready = false;
        let mut settled_status = None;
        if opens_new_context {
            let deadline = Instant::now() + Duration::from_secs(10);
            while Instant::now() < deadline {
                if let Ok(status) = call_dogfood_control(&endpoint, "shell_status", json!({})) {
                    let revised = status
                        .get("page_revision")
                        .and_then(Value::as_f64)
                        .is_some_and(|revision| revision > basis_page_revision as f64);
                    let ready = status.get("agent_enabled").and_then(Value::as_bool) == Some(true)
                        && status.get("collector_ready").and_then(Value::as_bool) == Some(true);
                    settled_status = Some(status);
                    if revised && ready {
                        destination_ready = true;
                        break;
                    }
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
        if let Some(tab) = state.find_tab_mut(tab_id) {
            update_session_tab_from_browser_result(tab, &fresh_actions);
            update_session_tab_from_browser_result(tab, &live_act);
            if let Some(status) = settled_status.as_ref() {
                update_session_tab_from_browser_result(tab, status);
            }
        }
        let tab = state
            .find_tab(tab_id)
            .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
        let site_policy = classify_site_url(&tab.info.url);
        return Ok(json!({
            "status": "ok",
            "summary": if destination_ready {
                "action opened an Agent On child tab; destination is ready to read"
            } else {
                "action dispatched through same dogfood WebView"
            },
            "runtime": tab_runtime(state, tab_id),
            "tab_id": tab_id.0,
            "action_id": action_id,
            "requested_basis_page_revision": basis_page_revision,
            "basis_page_revision": effective_revision,
            "basis_layout_epoch": effective_layout_epoch,
            "layout_rebased": layout_rebased,
            "new_page_revision": tab.info.page_revision,
            "opens_new_context": opens_new_context,
            "destination_ready": destination_ready,
            "site_policy": site_policy,
            "verification": {
                "mode": "native_input_receipt_v1",
                "verified": true,
                "status": receipt.get("status").cloned().unwrap_or(Value::Null),
                "basis_page_revision": receipt.get("basis_page_revision").cloned().unwrap_or(Value::Null),
                "observed_page_revision": receipt.get("observed_page_revision").cloned().unwrap_or(Value::Null),
                "basis_layout_epoch": receipt.get("basis_layout_epoch").cloned().unwrap_or(Value::Null),
                "observed_layout_epoch": receipt.get("observed_layout_epoch").cloned().unwrap_or(Value::Null),
            },
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
        let site_policy = classify_site_url(&tab.info.url);
        return Ok(json!({
            "status": "ok",
            "summary": "action dispatched through live Saccade browser session",
            "runtime": "browser_session_worker_v0",
            "tab_id": tab_id.0,
            "action_id": action_id,
            "basis_page_revision": basis_page_revision,
            "new_page_revision": tab.info.page_revision,
            "site_policy": site_policy,
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
    let site_policy = classify_site_url(&tab.info.url);
    Ok(json!({
        "status": "ok",
        "summary": "action verified through Servo-backed DEVMAX audit",
        "tab_id": tab_id.0,
        "action_id": action_id,
        "basis_page_revision": basis_page_revision,
        "new_page_revision": tab.info.page_revision,
        "site_policy": site_policy,
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
    let (current_revision, current_url) = {
        let tab = state
            .find_tab(tab_id)
            .with_context(|| format!("unknown tab_id {}", tab_id.0))?;
        (tab.info.page_revision, tab.info.url.clone())
    };
    let site_policy = classify_site_url(&current_url);
    if basis_page_revision != current_revision {
        bail!(
            "stale fill basis: requested {}, current {}",
            basis_page_revision,
            current_revision
        );
    }

    let live_fill = if let Some(endpoint) = state.dogfood_controls.get(&tab_id.0).cloned() {
        ensure_dogfood_control_capability(state, tab_id, "fill_agent_fields")?;
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
        "site_policy": site_policy,
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
    let current_url = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?
        .info
        .url
        .clone();
    let site_policy = classify_site_url(&current_url);
    let live_inspect = if let Some(endpoint) = state.dogfood_controls.get(&tab_id.0).cloned() {
        ensure_dogfood_control_capability(state, tab_id, "inspect_fields")?;
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
        "site_policy": site_policy,
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

fn web_render_preflight_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    ensure_truth_allowed(state, tab_id)?;
    let endpoint = state
        .dogfood_controls
        .get(&tab_id.0)
        .cloned()
        .context("saccade.web.render_preflight requires a granted browser current tab")?;
    ensure_dogfood_control_capability(state, tab_id, "render_preflight")?;
    let expected_surface = arguments
        .get("expected_surface")
        .and_then(Value::as_str)
        .unwrap_or("page");
    let mut result = call_dogfood_control(
        &endpoint,
        "render_preflight",
        json!({"expected_surface": expected_surface}),
    )?;
    if let Some(tab) = state.find_tab_mut(tab_id) {
        update_session_tab_from_browser_result(tab, &result);
    }
    if let Some(object) = result.as_object_mut() {
        object.insert("tab_id".to_string(), json!(tab_id.0));
    }
    Ok(result)
}

struct StableFormInventory {
    result: Value,
    waited_ms: u64,
    stable: bool,
    timed_out: bool,
}

fn poll_stable_form_inventory(
    endpoint: &DogfoodControlEndpoint,
    params: Value,
    wait_for_fields_ms: u64,
    minimum_wait_ms: u64,
) -> Result<StableFormInventory> {
    let started = Instant::now();
    let deadline = started + Duration::from_millis(wait_for_fields_ms);
    let minimum_wait = Duration::from_millis(minimum_wait_ms.min(wait_for_fields_ms));
    let mut stable_field_count = None;
    let mut stable_samples = 0_u8;
    let result = loop {
        let result = call_dogfood_control(endpoint, "form_inventory", params.clone())?;
        let field_count = result
            .get("field_count")
            .and_then(json_number_u64)
            .unwrap_or(0);
        if field_count > 0 {
            if stable_field_count == Some(field_count) {
                stable_samples += 1;
            } else {
                stable_field_count = Some(field_count);
                stable_samples = 1;
            }
            if (stable_samples >= 2 && started.elapsed() >= minimum_wait) || wait_for_fields_ms == 0
            {
                break result;
            }
        } else {
            stable_field_count = None;
            stable_samples = 0;
        }
        if Instant::now() >= deadline {
            break result;
        }
        thread::sleep(FORM_INVENTORY_STABILITY_INTERVAL);
    };
    let waited_ms = started.elapsed().as_millis() as u64;
    let stable = stable_samples >= 2 && started.elapsed() >= minimum_wait;
    let timed_out = wait_for_fields_ms > 0 && !stable && waited_ms >= wait_for_fields_ms;
    Ok(StableFormInventory {
        result,
        waited_ms,
        stable,
        timed_out,
    })
}

fn web_form_inventory_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    ensure_truth_allowed(state, tab_id)?;
    let endpoint = state
        .dogfood_controls
        .get(&tab_id.0)
        .cloned()
        .context("saccade.web.form_inventory requires a granted browser current tab")?;
    ensure_dogfood_control_capability(state, tab_id, "form_inventory")?;
    let mode = arguments
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("minimal");
    if !matches!(mode, "minimal" | "full" | "actionable" | "compact") {
        bail!("saccade.web.form_inventory mode must be minimal, full, actionable, or compact");
    }
    let upstream_mode = if mode == "minimal" { "compact" } else { mode };
    let mut params = json!({"mode": upstream_mode});
    if let Some(offset) = arguments.get("offset") {
        params["offset"] = offset.clone();
    }
    if let Some(limit) = arguments.get("limit") {
        params["limit"] = limit.clone();
    }
    let wait_for_fields_ms = arguments
        .get("wait_for_fields_ms")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_FORM_INVENTORY_WAIT_MS)
        .min(10_000);
    let stable_inventory = poll_stable_form_inventory(&endpoint, params, wait_for_fields_ms, 0)?;
    let mut result = stable_inventory.result;
    let waited_for_fields_ms = stable_inventory.waited_ms;
    let field_inventory_stable = stable_inventory.stable;
    let field_wait_timed_out = stable_inventory.timed_out;
    if let Some(tab) = state.find_tab_mut(tab_id) {
        update_session_tab_from_browser_result(tab, &result);
    }
    if mode == "minimal" {
        return Ok(minimal_form_inventory_response(
            &result,
            field_inventory_stable,
            field_wait_timed_out,
        ));
    }
    if let Some(object) = result.as_object_mut() {
        object.insert("tab_id".to_string(), json!(tab_id.0));
        object.insert(
            "waited_for_fields_ms".to_string(),
            json!(waited_for_fields_ms),
        );
        object.insert(
            "field_inventory_stable".to_string(),
            json!(field_inventory_stable),
        );
        object.insert(
            "field_wait_timed_out".to_string(),
            json!(field_wait_timed_out),
        );
    }
    Ok(result)
}

fn minimal_form_inventory_response(
    result: &Value,
    field_inventory_stable: bool,
    field_wait_timed_out: bool,
) -> Value {
    let fields = result
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|field| {
            let protected = field
                .get("sensitivity")
                .and_then(Value::as_str)
                .is_some_and(|value| value != "none");
            let eligible = field.get("eligible").and_then(Value::as_bool) == Some(true);
            let native_type_eligible =
                field.get("native_type_eligible").and_then(Value::as_bool) == Some(true);
            let blocked_reason =
                field
                    .get("blocked_reason")
                    .and_then(Value::as_str)
                    .or_else(|| {
                        field
                            .get("blocked_reasons")
                            .and_then(Value::as_array)
                            .and_then(|values| values.first())
                            .and_then(Value::as_str)
                    });
            let status = if protected {
                field
                    .get("value_state")
                    .and_then(Value::as_str)
                    .unwrap_or("requires_user_input")
            } else if eligible {
                "fillable"
            } else if native_type_eligible {
                "native_typing"
            } else {
                blocked_reason.unwrap_or("blocked")
            };
            let mut item = serde_json::Map::new();
            item.insert(
                "field_id".to_string(),
                field.get("field_id").cloned().unwrap_or(Value::Null),
            );
            item.insert(
                "label".to_string(),
                field.get("label").cloned().unwrap_or(Value::Null),
            );
            item.insert(
                "type".to_string(),
                field.get("type").cloned().unwrap_or(Value::Null),
            );
            item.insert("status".to_string(), json!(status));
            if field.get("required").and_then(Value::as_bool) == Some(true) {
                item.insert("required".to_string(), json!(true));
            }
            if protected {
                item.insert("protected".to_string(), json!(true));
            }
            Value::Object(item)
        })
        .collect::<Vec<_>>();

    let mut response = json!({
        "page_revision": result.get("page_revision").cloned().unwrap_or(Value::Null),
        "field_count": result.get("field_count").cloned().unwrap_or_else(|| json!(fields.len())),
        "eligible_count": result.get("eligible_count").cloned().unwrap_or(Value::Null),
        "sensitive_count": result.get("sensitive_count").cloned().unwrap_or(json!(0)),
        "fields": fields,
        "ready": field_inventory_stable,
    });
    if result.get("has_more").and_then(Value::as_bool) == Some(true) {
        response["has_more"] = json!(true);
    }
    if field_wait_timed_out {
        response["timed_out"] = json!(true);
    }
    response
}

fn form_inventory_requires_follow_up(inventory: &Value) -> bool {
    inventory
        .get("fields")
        .and_then(Value::as_array)
        .is_some_and(|fields| {
            fields.iter().any(|field| {
                matches!(
                    field.get("status").and_then(Value::as_str),
                    Some("fillable" | "native_typing")
                )
            })
        })
}

fn web_request_protected_fill_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    ensure_agent_input_allowed(state, tab_id)?;
    let basis_page_revision = arguments
        .get("basis_page_revision")
        .and_then(Value::as_u64)
        .context("saccade.web.request_protected_fill requires integer basis_page_revision")?;
    let current_revision = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?
        .info
        .page_revision;
    if basis_page_revision != current_revision {
        bail!(
            "stale MCP protected-fill basis: requested {}, current {}",
            basis_page_revision,
            current_revision
        );
    }
    let field_id = arguments
        .get("field_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .context("saccade.web.request_protected_fill requires a bounded field_id")?;
    let endpoint = state
        .dogfood_controls
        .get(&tab_id.0)
        .cloned()
        .context("protected local fill requires a granted browser current tab")?;
    ensure_dogfood_control_capability(state, tab_id, "protected_fill")?;
    let mut result = call_dogfood_control(
        &endpoint,
        "protected_fill",
        json!({
            "basis_page_revision": basis_page_revision,
            "field_id": field_id,
        }),
    )?;
    if result.get("raw_value_returned").and_then(Value::as_bool) != Some(false)
        || result
            .get("sensitive_values_exposed")
            .and_then(Value::as_bool)
            != Some(false)
        || result.get("values_logged").and_then(Value::as_bool) != Some(false)
    {
        bail!("protected local fill response violated the value-free boundary");
    }
    if let Some(tab) = state.find_tab_mut(tab_id) {
        update_session_tab_from_browser_result(tab, &result);
    }
    if let Some(object) = result.as_object_mut() {
        object.insert("tab_id".to_string(), json!(tab_id.0));
        object.insert("local_browser_prompt".to_string(), Value::Bool(true));
        object.insert("model_received_value".to_string(), Value::Bool(false));
    }
    Ok(result)
}

fn web_form_compile_plan_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    ensure_truth_allowed(state, tab_id)?;
    let basis_page_revision = arguments
        .get("basis_page_revision")
        .and_then(Value::as_u64)
        .context("saccade.web.form_compile_plan requires integer basis_page_revision")?;
    let current_revision = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?
        .info
        .page_revision;
    if basis_page_revision != current_revision {
        bail!(
            "stale MCP form plan basis: requested {}, current {}",
            basis_page_revision,
            current_revision
        );
    }
    let endpoint = state
        .dogfood_controls
        .get(&tab_id.0)
        .cloned()
        .context("saccade.web.form_compile_plan requires a granted browser current tab")?;
    ensure_dogfood_control_capability(state, tab_id, "form_compile_plan")?;
    let params = json!({
        "basis_page_revision": basis_page_revision,
        "assignments": arguments.get("assignments").cloned().unwrap_or(Value::Null),
        "policy": arguments.get("policy").cloned().unwrap_or(Value::Null),
    });
    let mut result = call_dogfood_control(&endpoint, "form_compile_plan", params)?;
    if let Some(tab) = state.find_tab_mut(tab_id) {
        update_session_tab_from_browser_result(tab, &result);
    }
    if let Some(object) = result.as_object_mut() {
        object.insert("tab_id".to_string(), json!(tab_id.0));
    }
    Ok(result)
}

fn web_form_execute_plan_tool(state: &mut McpSessionState, arguments: Value) -> Result<Value> {
    let tab_id = required_tab_id_arg(&arguments)?;
    ensure_agent_input_allowed(state, tab_id)?;
    let basis_page_revision = arguments
        .get("basis_page_revision")
        .and_then(Value::as_u64)
        .context("saccade.web.form_execute_plan requires integer basis_page_revision")?;
    let current_revision = state
        .find_tab(tab_id)
        .with_context(|| format!("unknown tab_id {}", tab_id.0))?
        .info
        .page_revision;
    if basis_page_revision != current_revision {
        bail!(
            "stale MCP form execution basis: requested {}, current {}",
            basis_page_revision,
            current_revision
        );
    }
    let endpoint = state
        .dogfood_controls
        .get(&tab_id.0)
        .cloned()
        .context("saccade.web.form_execute_plan requires a granted browser current tab")?;
    ensure_dogfood_control_capability(state, tab_id, "form_execute_plan")?;
    let params = json!({
        "basis_page_revision": basis_page_revision,
        "expected_plan_id": arguments.get("expected_plan_id").cloned().unwrap_or(Value::Null),
        "assignments": arguments.get("assignments").cloned().unwrap_or(Value::Null),
        "policy": arguments.get("policy").cloned().unwrap_or(Value::Null),
    });
    let mut result = call_dogfood_control(&endpoint, "form_execute_plan", params)?;
    let post_execute = poll_stable_form_inventory(
        &endpoint,
        json!({"mode": "compact"}),
        POST_EXECUTE_FORM_INVENTORY_WAIT_MS,
        POST_EXECUTE_FORM_INVENTORY_MIN_WAIT_MS,
    )?;
    let post_execute_inventory = minimal_form_inventory_response(
        &post_execute.result,
        post_execute.stable,
        post_execute.timed_out,
    );
    let follow_up_required = form_inventory_requires_follow_up(&post_execute_inventory);
    if let Some(tab) = state.find_tab_mut(tab_id) {
        update_session_tab_from_browser_result(tab, &post_execute.result);
    }
    if let Some(object) = result.as_object_mut() {
        let verification_complete = object.get("receipt_verified").and_then(Value::as_bool)
            == Some(true)
            && object
                .get("failed")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty);
        object.insert("tab_id".to_string(), json!(tab_id.0));
        object.insert(
            "verification_complete".to_string(),
            json!(verification_complete),
        );
        object.insert("follow_up_required".to_string(), json!(follow_up_required));
        object.insert(
            "form_complete".to_string(),
            json!(verification_complete && !follow_up_required),
        );
        object.insert("post_execute_inventory".to_string(), post_execute_inventory);
        object.insert(
            "summary".to_string(),
            json!(if verification_complete && follow_up_required {
                "Form execution verified; newly revealed ordinary fields require a follow-up plan"
            } else if verification_complete {
                "Form execution verified; no further field inspection required"
            } else {
                "Form execution completed; review failed or rejected fields"
            }),
        );
    }
    Ok(result)
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
    let has_live_session = state.browser_workers.contains_key(&tab_id.0)
        || state.dogfood_controls.contains_key(&tab_id.0);
    if !has_live_session {
        bail!("live saccade.web.fill_form requires a live browser session tab");
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

    let live_fill = if let Some(endpoint) = state.dogfood_controls.get(&tab_id.0).cloned() {
        ensure_dogfood_control_capability(state, tab_id, "formmax_live_fill")?;
        call_dogfood_control(
            &endpoint,
            "formmax_live_fill",
            json!({
                "policy": {
                    "block_sensitive": true,
                    "local_fixture_only": true,
                }
            }),
        )?
    } else {
        call_browser_worker(
            state,
            tab_id,
            "formmax_live_fill",
            json!({
                "policy": {
                    "block_sensitive": true,
                    "local_fixture_only": true,
                }
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
        "summary": "FORMMAX capacity fixture filled and validated through the live Saccade browser session",
        "runtime": tab_runtime(state, tab_id),
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
        "replay_events": live_fill.get("replay_events").cloned().unwrap_or(Value::Null),
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

fn report_redacted_note_tool(arguments: Value) -> Result<Value> {
    let raw_text = arguments
        .get("redacted_text")
        .and_then(Value::as_str)
        .context("tool arguments must include string field redacted_text")?
        .trim();
    if raw_text.is_empty() {
        bail!("redacted_text must not be empty");
    }
    if raw_text.len() > 24_000 {
        bail!("redacted_text is too large for a fallback note; keep it under 24000 chars");
    }
    if let Some(policy) = arguments.get("policy") {
        if policy
            .get("redacted_user_supplied")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("saccade.report.redacted_note requires redacted_user_supplied=true");
        }
        if policy
            .get("no_live_site_access")
            .and_then(Value::as_bool)
            .is_some_and(|enabled| !enabled)
        {
            bail!("saccade.report.redacted_note requires no_live_site_access=true");
        }
    }

    let source_url = arguments
        .get("source_url")
        .and_then(Value::as_str)
        .unwrap_or("");
    let title = arguments
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Redacted fallback note");
    let task = arguments
        .get("task")
        .and_then(Value::as_str)
        .unwrap_or("evaluate_edit");
    let audience = arguments
        .get("audience")
        .and_then(Value::as_str)
        .unwrap_or("human operator and AI reviewer");
    let site_policy = if source_url.trim().is_empty() {
        Value::Null
    } else {
        json!(classify_site_url(source_url))
    };
    let redaction = sanitize_redacted_note_text(raw_text);
    let run_dir = workspace_root()?
        .join("runs")
        .join("redacted_notes")
        .join(format!("note_{}", unix_ms()?));
    fs::create_dir_all(&run_dir)
        .with_context(|| format!("failed to create {}", run_dir.display()))?;
    let note_path = run_dir.join("redacted_note.md");
    let prompt_path = run_dir.join("ai_review_prompt.md");
    let report_path = run_dir.join("note.json");
    let note_markdown = format_redacted_note_markdown(title, source_url, task, &redaction.text);
    fs::write(&note_path, note_markdown)
        .with_context(|| format!("failed to write {}", note_path.display()))?;
    let prompt_markdown = format_ai_review_prompt(
        title,
        source_url,
        task,
        audience,
        &site_policy,
        &redaction.text,
    );
    fs::write(&prompt_path, prompt_markdown)
        .with_context(|| format!("failed to write {}", prompt_path.display()))?;
    let report = json!({
        "status": if redaction.warnings.is_empty() { "ok" } else { "warning" },
        "engine": "saccade-redacted-note-v0",
        "summary": "redacted fallback note prepared for AI evaluation/editing without live-site access",
        "title": title,
        "task": task,
        "audience": audience,
        "source_url": redacted_note_url(source_url),
        "site_policy": site_policy,
        "redaction": {
            "user_supplied_redacted": true,
            "no_live_site_access": true,
            "values_logged": false,
            "warnings": redaction.warnings,
            "input_chars": raw_text.len(),
            "sanitized_chars": redaction.text.len(),
        },
        "recommended_ai_return_shape": [
            "risk_and_context_assessment",
            "questions_for_human",
            "edited_draft",
            "final_human_confirmation_checklist"
        ],
        "artifacts": {
            "run_dir": run_dir.display().to_string(),
            "report": report_path.display().to_string(),
            "redacted_note": note_path.display().to_string(),
            "ai_review_prompt": prompt_path.display().to_string(),
        }
    });
    write_json(&report_path, &report)?;
    let artifact_index = record_artifact_index(
        "saccade.report.redacted_note",
        "redacted_ai_review_packet",
        "redacted fallback note prepared for AI review/edit",
        report.get("artifacts").cloned().unwrap_or(Value::Null),
    )?;
    Ok(json!({
        "status": report.get("status").cloned().unwrap_or_else(|| json!("ok")),
        "summary": report.get("summary").cloned().unwrap_or(Value::Null),
        "task": task,
        "site_policy": report.get("site_policy").cloned().unwrap_or(Value::Null),
        "redaction": report.get("redaction").cloned().unwrap_or(Value::Null),
        "artifacts": report.get("artifacts").cloned().unwrap_or(Value::Null),
        "artifact_index": artifact_index,
    }))
}

#[derive(Debug)]
struct RedactedNote {
    text: String,
    warnings: Vec<String>,
}

fn sanitize_redacted_note_text(text: &str) -> RedactedNote {
    let mut warnings = Vec::new();
    let sanitized = text
        .split_whitespace()
        .map(|token| redact_note_token(token, &mut warnings))
        .collect::<Vec<_>>()
        .join(" ");
    warnings.sort();
    warnings.dedup();
    RedactedNote {
        text: sanitized,
        warnings,
    }
}

fn redact_note_token(token: &str, warnings: &mut Vec<String>) -> String {
    if token.contains('@') && token.contains('.') {
        warnings.push("email_like_token_redacted".into());
        return "[redacted-email]".into();
    }
    if token.starts_with("http://") || token.starts_with("https://") {
        if let Ok(mut url) = Url::parse(token) {
            if url.query().is_some() || url.fragment().is_some() {
                warnings.push("url_query_or_fragment_removed".into());
            }
            url.set_query(None);
            url.set_fragment(None);
            return url.to_string();
        }
    }
    let digits = token.chars().filter(|c| c.is_ascii_digit()).count();
    if digits >= 9 && !looks_like_public_request_id(token) {
        warnings.push("long_number_redacted".into());
        return "[redacted-number]".into();
    }
    if token.len() >= 24 && token.chars().any(|c| c.is_ascii_digit()) {
        let alpha = token.chars().filter(|c| c.is_ascii_alphabetic()).count();
        let numeric = token.chars().filter(|c| c.is_ascii_digit()).count();
        if alpha >= 8 && numeric >= 4 && !looks_like_public_request_id(token) {
            warnings.push("token_like_value_redacted".into());
            return "[redacted-token]".into();
        }
    }
    token.to_string()
}

fn looks_like_public_request_id(token: &str) -> bool {
    let cleaned = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
    let hyphens = cleaned.chars().filter(|c| *c == '-').count();
    let hexish = cleaned.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    cleaned.len() >= 24 && hyphens >= 2 && hexish
}

fn redacted_note_url(source_url: &str) -> Value {
    if source_url.trim().is_empty() {
        return Value::Null;
    }
    let Ok(mut url) = Url::parse(source_url) else {
        return json!(source_url);
    };
    url.set_query(None);
    url.set_fragment(None);
    json!(url.as_str())
}

fn format_redacted_note_markdown(title: &str, source_url: &str, task: &str, text: &str) -> String {
    format!(
        "# {title}\n\nSource URL: {}\nTask: `{task}`\n\n## Redacted Content\n\n{text}\n",
        redacted_note_url(source_url)
            .as_str()
            .unwrap_or("not provided")
    )
}

fn format_ai_review_prompt(
    title: &str,
    source_url: &str,
    task: &str,
    audience: &str,
    site_policy: &Value,
    text: &str,
) -> String {
    format!(
        "# Saccade Redacted AI Review Packet\n\nTitle: {title}\nSource URL: {}\nTask: `{task}`\nAudience: {audience}\nSite policy: {}\n\nSafety rules:\n- Treat the content below as user-supplied and already redacted, but do not ask for credentials, OTP, payment, legal signature, or private identifiers.\n- Do not claim to have accessed the live website.\n- Do not click, submit, publish, release, pay, sign, or change account/security settings.\n- Evaluate from an AI assistant/product/reviewer perspective, then edit the user-provided draft.\n\nReturn exactly these sections:\n1. Risk And Context Assessment\n2. Questions For Human\n3. Edited Draft\n4. Final Human Confirmation Checklist\n\n## Redacted Content\n\n{text}\n",
        redacted_note_url(source_url)
            .as_str()
            .unwrap_or("not provided"),
        site_policy
    )
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
    if let Some(page_revision) = result.get("page_revision").and_then(json_number_u64) {
        tab.info.page_revision = page_revision;
    }
    if let Some(paused) = result.get("paused").and_then(Value::as_bool) {
        tab.paused = paused;
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

fn json_number_u64(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        let number = value.as_f64()?;
        (number.is_finite() && number >= 0.0 && number <= u64::MAX as f64 && number.fract() == 0.0)
            .then_some(number as u64)
    })
}

fn tab_runtime(state: &McpSessionState, tab_id: TabId) -> String {
    if let Some(runtime) = state.dogfood_control_runtimes.get(&tab_id.0) {
        runtime.clone()
    } else if let Some(endpoint) = state.dogfood_controls.get(&tab_id.0) {
        endpoint.protocol.clone()
    } else if state.browser_workers.contains_key(&tab_id.0) {
        "browser_session_worker_v0".to_string()
    } else {
        "mcp_report_backed_v0".to_string()
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
    match canonical_ascii_token(owner).as_str() {
        "agent" => Ok(TabOwner::Agent),
        "human" => Ok(TabOwner::Human),
        _ => bail!("unsupported owner {owner:?}; expected agent or human"),
    }
}

fn read_grant_from_args(arguments: &Value) -> Result<ReadGrant> {
    let read_grant = arguments
        .get("read_grant")
        .and_then(Value::as_str)
        .unwrap_or("none");
    match canonical_ascii_token(read_grant).as_str() {
        "none" => Ok(ReadGrant::None),
        "visiblesummaryonly" | "visible_summary_only" => Ok(ReadGrant::VisibleSummaryOnly),
        "fulltruth" | "full_truth" => Ok(ReadGrant::FullTruth),
        other => bail!("unsupported read_grant {other:?}"),
    }
}

fn canonical_ascii_token(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|ch| *ch != '-' && *ch != ' ')
        .flat_map(char::to_lowercase)
        .collect()
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

fn verify_browser_navigate_json_rpc_surface() -> Result<bool> {
    let mut denied_state = McpSessionState::default();
    denied_state.tabs.push(SessionTab {
        info: tab(
            2,
            TabOwner::Agent,
            ReadGrant::FullTruth,
            "https://example.test/agent",
            "Agent Tab",
        ),
        paused: false,
        agent_input_grant: false,
        grant_reason: None,
        last_engine: None,
        last_summary: None,
        last_report_path: None,
        last_replay_path: None,
        last_actions: Vec::new(),
        last_findings: Vec::new(),
    });
    denied_state.dogfood_controls.insert(
        2,
        DogfoodControlEndpoint {
            host: "127.0.0.1".to_string(),
            port: 1,
            protocol: "saccade-dogfood-control-v1".to_string(),
            capability: "test-capability-token-with-sufficient-length".to_string(),
            engine_grant: None,
        },
    );
    let denied_without_grant = handle_json_rpc(
        &mut denied_state,
        JsonRpcRequest {
            id: Some(json!(210)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.browser.navigate",
                "arguments": {
                    "tab_id": 2,
                    "action": "reload",
                    "policy": {
                        "same_webview_only": true,
                        "agent_on_tab_only": true
                    }
                }
            }),
        },
    )
    .and_then(|response| {
        response
            .get("error")
            .and_then(rpc_error_detail)
            .map(|detail| detail.contains("user-granted Human current tab"))
    })
    .unwrap_or(false);

    let (endpoint, handle) = spawn_fake_dogfood_control_once(
        "navigate",
        json!({
            "status": "ok",
            "runtime": "saccade-dogfood-control-v1",
            "engine": "saccade-dogfood-control-shell-navigate-v0",
            "summary": "fake shell navigate completed",
            "same_webview_control": true,
            "rendering_profile": "servo-modern",
            "renderer_engine": "servo",
            "servo_grid_enabled": true,
            "url": "https://example.test/after",
            "title": "After Navigate",
            "load_state": "complete",
            "page_revision": 9,
            "can_go_back": true,
            "can_go_forward": false,
            "copilot_granted": true,
            "changed": true,
            "toolbar": {
                "visible": true,
                "clickable": true,
                "page_dom_injected": false
            },
            "artifacts": {
                "report": null,
                "replay": null
            }
        }),
    )?;

    let mut state = McpSessionState::default();
    state.tabs.push(SessionTab {
        info: tab(
            1,
            TabOwner::Human,
            ReadGrant::FullTruth,
            "https://example.test/before",
            "Before Navigate",
        ),
        paused: false,
        agent_input_grant: true,
        grant_reason: Some("fake dogfood browser navigation selftest".into()),
        last_engine: None,
        last_summary: None,
        last_report_path: None,
        last_replay_path: None,
        last_actions: Vec::new(),
        last_findings: Vec::new(),
    });
    state.dogfood_controls.insert(1, endpoint);

    let response = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(211)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.browser.navigate",
                "arguments": {
                    "tab_id": 1,
                    "action": "navigate",
                    "url": "https://example.test/after",
                    "policy": {
                        "same_webview_only": true,
                        "agent_on_tab_only": true
                    }
                }
            }),
        },
    );

    let server_ok = handle.join().unwrap_or(false);
    let content_ok = response
        .as_ref()
        .and_then(|response| {
            response
                .get("result")
                .and_then(|result| result.get("structuredContent"))
        })
        .is_some_and(|content| {
            content.get("status").and_then(Value::as_str) == Some("ok")
                && content.get("runtime").and_then(Value::as_str)
                    == Some("saccade-dogfood-control-v1")
                && content.get("action").and_then(Value::as_str) == Some("navigate")
                && content.get("url").and_then(Value::as_str) == Some("https://example.test/after")
                && content.get("changed").and_then(Value::as_bool) == Some(true)
                && content
                    .pointer("/policy/page_dom_injected")
                    .and_then(Value::as_bool)
                    == Some(false)
        });
    let tab_ok = state.find_tab(TabId(1)).is_some_and(|tab| {
        tab.info.url == "https://example.test/after"
            && tab.info.title.as_deref() == Some("After Navigate")
            && tab.info.page_revision == 9
    });

    Ok(denied_without_grant && server_ok && content_ok && tab_ok)
}

fn spawn_fake_dogfood_control_once(
    expected_method: &'static str,
    response_result: Value,
) -> Result<(DogfoodControlEndpoint, thread::JoinHandle<bool>)> {
    let listener =
        TcpListener::bind("127.0.0.1:0").context("failed to bind fake dogfood control")?;
    let addr = listener
        .local_addr()
        .context("failed to read fake dogfood control addr")?;
    let handle = thread::spawn(move || {
        let Ok((mut stream, _peer)) = listener.accept() else {
            return false;
        };
        let Ok(reader_stream) = stream.try_clone() else {
            return false;
        };
        let mut line = String::new();
        if BufReader::new(reader_stream).read_line(&mut line).is_err() {
            return false;
        }
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            return false;
        };
        if request.get("method").and_then(Value::as_str) != Some(expected_method) {
            return false;
        }
        if request.get("capability").and_then(Value::as_str)
            != Some("test-capability-token-with-sufficient-length")
        {
            return false;
        }
        let response = json!({
            "id": request.get("id").cloned().unwrap_or(Value::Null),
            "ok": true,
            "result": response_result,
        });
        writeln!(stream, "{response}").is_ok() && stream.flush().is_ok()
    });
    Ok((
        DogfoodControlEndpoint {
            host: "127.0.0.1".to_string(),
            port: addr.port(),
            protocol: "saccade-dogfood-control-v1".to_string(),
            capability: "test-capability-token-with-sufficient-length".to_string(),
            engine_grant: None,
        },
        handle,
    ))
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

    let contract_capabilities = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(21)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.system.capabilities",
                "arguments": {}
            }),
        },
    )
    .and_then(|response| {
        response
            .pointer("/result/structuredContent/saccade")
            .cloned()
    })
    .is_some_and(|capabilities| {
        capabilities.get("contract_version").and_then(Value::as_str)
            == Some(SACCADE_CONTRACT_VERSION)
            && capabilities
                .get("features")
                .and_then(Value::as_array)
                .is_some_and(|features| {
                    features
                        .iter()
                        .any(|feature| feature.as_str() == Some("typed_errors"))
                })
    });

    let browser_navigate = verify_browser_navigate_json_rpc_surface()?;

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
                        && content
                            .pointer("/tab/owner")
                            .and_then(Value::as_str)
                            .is_some_and(|owner| canonical_ascii_token(owner) == "human")
                        && content
                            .pointer("/tab/read_grant")
                            .and_then(Value::as_str)
                            .is_some_and(|read_grant| {
                                matches!(
                                    canonical_ascii_token(read_grant).as_str(),
                                    "fulltruth" | "full_truth"
                                )
                            })
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
    let copilot_submit_response = copilot_submit_action.as_ref().and_then(|action_id| {
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
    let copilot_submit_dispatched = copilot_submit_response
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
    let copilot_response_blob = [
        grant_current_response.as_ref(),
        copilot_fill_response.as_ref(),
        copilot_inspect_response.as_ref(),
        copilot_actions_response.as_ref(),
        copilot_submit_response.as_ref(),
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
        && copilot_submit_dispatched
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
            "owner": "human",
            "read_grant": "full_truth",
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
                        && content
                            .pointer("/tab/owner")
                            .and_then(Value::as_str)
                            .is_some_and(|owner| canonical_ascii_token(owner) == "human")
                        && content
                            .pointer("/tab/read_grant")
                            .and_then(Value::as_str)
                            .is_some_and(|read_grant| {
                                matches!(
                                    canonical_ascii_token(read_grant).as_str(),
                                    "fulltruth" | "full_truth"
                                )
                            })
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

    // The remaining checks spawn independent runners. Release live Servo workers first so the
    // macOS GL path is not overloaded by several background WebViews during static FORMMAX.
    for worker in state.browser_workers.values_mut() {
        worker.close();
    }
    state.browser_workers.clear();
    state.dogfood_controls.clear();
    state.dogfood_control_runtimes.clear();
    state.dogfood_control_capabilities.clear();

    let servoshell_bridge_grant = verify_servoshell_bridge_grant_json_rpc_surface()?;

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
    let report_redacted_note = handle_json_rpc(
        &mut state,
        JsonRpcRequest {
            id: Some(json!(14)),
            method: "tools/call".into(),
            params: json!({
                "name": "saccade.report.redacted_note",
                "arguments": {
                    "source_url": "https://appstoreconnect.apple.com/apps?token=SHOULD-REMOVE",
                    "title": "App Review fallback note",
                    "task": "evaluate_edit",
                    "audience": "Apple app review reply",
                    "redacted_text": "We can't process your request. Request 0fa693e0-e6ef-425f-91e3-05fdac5581d7. Draft reply: Thanks for the review. We fixed the sign-in copy and removed test account details. Contact wayne@example.com.",
                    "policy": {
                        "redacted_user_supplied": true,
                        "no_live_site_access": true
                    }
                }
            }),
        },
    )
    .and_then(|response| {
        let content = response
            .get("result")
            .and_then(|result| result.get("structuredContent"))?;
        let status_ok = content
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| status == "ok" || status == "warning");
        let prompt_exists = content
            .pointer("/artifacts/ai_review_prompt")
            .and_then(Value::as_str)
            .is_some_and(|path| Path::new(path).exists());
        let email_redacted = content
            .pointer("/redaction/warnings")
            .and_then(Value::as_array)
            .is_some_and(|warnings| {
                warnings
                    .iter()
                    .any(|warning| warning.as_str() == Some("email_like_token_redacted"))
            });
        Some(status_ok && prompt_exists && email_redacted)
    })
    .unwrap_or(false);

    Ok(JsonRpcEvidence {
        initialize,
        tools_list,
        contract_capabilities,
        tool_call,
        persistent_tabs,
        browser_backed_tabs,
        tabs_grant_current,
        tabs_grant_artifact,
        servoshell_bridge_grant,
        servoshell_bridge_formmax_live: servoshell_bridge_grant,
        servoshell_bridge_artifacts: servoshell_bridge_grant,
        browser_navigate,
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
        report_redacted_note,
        audit_report,
    })
}

fn verify_servoshell_bridge_grant_json_rpc_surface() -> Result<bool> {
    let workspace = workspace_root()?;
    let run_dir = workspace
        .join("runs")
        .join("mcp")
        .join(format!("servoshell_bridge_grant_{}", unix_ms()?));
    fs::create_dir_all(&run_dir)
        .with_context(|| format!("failed to create {}", run_dir.display()))?;
    let grant_path = run_dir.join("grant.json");
    let copilot_url = start_test_server(workspace.join("test_pages").join("current_tab_copilot"))?;
    let button_url = start_test_server(workspace.join("test_pages").join("browser_session"))?;
    let formmax_url = start_test_server(workspace.join("test_pages").join("formmax"))?;

    let servoshell_bin = std::env::var_os("SACCADE_SERVOSHELL_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/Applications/Servo.app/Contents/MacOS/servoshell"));
    if !servoshell_bin.exists() {
        bail!(
            "SACCADE_SERVOSHELL_BIN does not exist: {}",
            servoshell_bin.display()
        );
    }
    let mut command = ProcessCommand::new("cargo");
    command
        .args([
            "run",
            "-q",
            "-p",
            "saccade-servoshell",
            "--",
            "bridge",
            "--servoshell",
        ])
        .arg(&servoshell_bin)
        .args([
            "--url",
            copilot_url.as_str(),
            "--output-dir",
            run_dir
                .to_str()
                .context("servoshell bridge run_dir is not valid UTF-8")?,
            "--grant-path",
            grant_path
                .to_str()
                .context("servoshell bridge grant_path is not valid UTF-8")?,
            "--timeout-sec",
            "45",
        ])
        .current_dir(&workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().with_context(|| {
        format!(
            "failed to launch saccade-servoshell bridge with {}",
            servoshell_bin.display()
        )
    })?;

    let mut endpoint_for_shutdown: Option<DogfoodControlEndpoint> = None;
    let verification = (|| -> Result<bool> {
        wait_for_file_or_child_exit(&grant_path, &mut child, Duration::from_secs(75))?;
        let grant: Value = serde_json::from_slice(
            &fs::read(&grant_path)
                .with_context(|| format!("failed to read {}", grant_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", grant_path.display()))?;
        let endpoint = dogfood_control_endpoint_from_grant(&grant)?
            .context("servoshell bridge grant did not include a control endpoint")?;
        endpoint_for_shutdown = Some(endpoint);

        let mut state = McpSessionState::default();
        let grant_content = json_rpc_tool_content(
            &mut state,
            501,
            "saccade.tabs.grant_current",
            json!({
                "grant_path": grant_path.display().to_string(),
                "reason": "selftest attached official ServoShell bridge grant",
                "policy": {
                    "local_dev_only": true,
                    "explicit_user_grant": true
                }
            }),
        )?;
        let tab_id = grant_content
            .pointer("/tab/tab_id")
            .and_then(Value::as_u64)
            .context("servoshell bridge grant attach did not return tab_id")?;
        let capabilities = grant_content
            .get("same_webview_capabilities")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let has_capability = |needle: &str| {
            capabilities
                .iter()
                .any(|value| value.as_str() == Some(needle))
        };
        let grant_attached = grant_content.get("status").and_then(Value::as_str) == Some("ok")
            && grant_content.get("runtime").and_then(Value::as_str)
                == Some("saccade-servoshell-bridge-v0")
            && grant_content
                .get("same_webview_attached")
                .and_then(Value::as_bool)
                == Some(true)
            && grant_content
                .get("transport_status")
                .and_then(Value::as_str)
                == Some("same_webview_control_truth_v0")
            && grant_content
                .pointer("/truth/engine")
                .and_then(Value::as_str)
                == Some("saccade-servoshell-bridge-truth-v0")
            && has_capability("truth")
            && has_capability("actions")
            && has_capability("saccade.browser.navigate")
            && has_capability("fill_agent_fields")
            && has_capability("inspect_fields")
            && has_capability("act")
            && has_capability("formmax_live_fill");

        let truth_content = json_rpc_tool_content(
            &mut state,
            502,
            "saccade.web.truth",
            json!({
                "tab_id": tab_id,
                "engine": "servo"
            }),
        )?;
        let truth_ok = truth_content.get("runtime").and_then(Value::as_str)
            == Some("saccade-servoshell-bridge-v0")
            && truth_content
                .pointer("/truth/engine")
                .and_then(Value::as_str)
                == Some("saccade-servoshell-bridge-truth-v0");

        let actions_content = json_rpc_tool_content(
            &mut state,
            503,
            "saccade.web.actions",
            json!({
                "tab_id": tab_id,
                "engine": "servo"
            }),
        )?;
        let actions_ok = actions_content.get("runtime").and_then(Value::as_str)
            == Some("saccade-servoshell-bridge-v0")
            && actions_content
                .get("actions")
                .and_then(Value::as_array)
                .is_some_and(|actions| !actions.is_empty());

        let status_content = json_rpc_tool_content(
            &mut state,
            504,
            "saccade.browser.navigate",
            json!({
                "tab_id": tab_id,
                "action": "status",
                "policy": {
                    "same_webview_only": true,
                    "agent_on_tab_only": true
                }
            }),
        )?;
        let status_ok = status_content.get("runtime").and_then(Value::as_str)
            == Some("saccade-servoshell-bridge-v0")
            && status_content
                .pointer("/shell/engine")
                .and_then(Value::as_str)
                == Some("saccade-servoshell-bridge-shell-status-v0")
            && status_content
                .pointer("/shell/copilot/status")
                .and_then(Value::as_str)
                == Some("granted")
            && status_content
                .pointer("/shell/copilot/sensitive_values_exposed_to_agent")
                .and_then(Value::as_bool)
                == Some(false);

        let fill_basis_page_revision = grant_content
            .pointer("/tab/page_revision")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        let fill_content = json_rpc_tool_content(
            &mut state,
            505,
            "saccade.web.fill_agent_fields",
            json!({
                "tab_id": tab_id,
                "basis_page_revision": fill_basis_page_revision,
                "fields": {
                    "project-name": "ServoShell bridge co-pilot",
                    "capacity": "24",
                    "notes": "Official ServoShell bridge safe fill.",
                    "ssn": "SHOULD-NOT-WRITE",
                    "signature": "SHOULD-NOT-WRITE"
                },
                "policy": {
                    "agent_owned_only": true,
                    "block_sensitive": true,
                    "live_worker_only": true
                }
            }),
        )?;
        let fill_ok = fill_content.get("runtime").and_then(Value::as_str)
            == Some("saccade-servoshell-bridge-v0")
            && fill_content
                .get("filled")
                .and_then(Value::as_array)
                .is_some_and(|filled| {
                    ["project-name", "capacity", "notes"]
                        .iter()
                        .all(|id| filled.iter().any(|value| value.as_str() == Some(*id)))
                })
            && fill_content
                .get("rejected")
                .and_then(Value::as_array)
                .is_some_and(|rejected| {
                    ["ssn", "signature"].iter().all(|id| {
                        rejected
                            .iter()
                            .any(|value| value.get("id").and_then(Value::as_str) == Some(*id))
                    })
                });

        let inspect_content = json_rpc_tool_content(
            &mut state,
            506,
            "saccade.web.inspect_fields",
            json!({
                "tab_id": tab_id,
                "fields": ["project-name", "ssn", "signature"],
                "policy": {
                    "redact_sensitive": true,
                    "explicit_fields_only": true,
                    "live_worker_only": true
                }
            }),
        )?;
        let inspect_fields = inspect_content
            .get("fields")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let inspect_ok = inspect_content.get("runtime").and_then(Value::as_str)
            == Some("saccade-servoshell-bridge-v0")
            && inspect_fields.iter().any(|field| {
                field.get("id").and_then(Value::as_str) == Some("project-name")
                    && field.get("value_returned").and_then(Value::as_bool) == Some(true)
                    && field.get("value").and_then(Value::as_str)
                        == Some("ServoShell bridge co-pilot")
            })
            && ["ssn", "signature"].iter().all(|id| {
                inspect_fields.iter().any(|field| {
                    field.get("id").and_then(Value::as_str) == Some(*id)
                        && field.get("value_redacted").and_then(Value::as_bool) == Some(true)
                        && field.get("value").is_none()
                })
            });
        let bridge_response_blob = [
            &grant_content,
            &truth_content,
            &actions_content,
            &status_content,
            &fill_content,
            &inspect_content,
        ]
        .into_iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
        let no_sensitive_leak = !bridge_response_blob.contains("999-12-3456")
            && !bridge_response_blob.contains("SHOULD-NOT-WRITE");

        let navigate_content = json_rpc_tool_content(
            &mut state,
            507,
            "saccade.browser.navigate",
            json!({
                "tab_id": tab_id,
                "action": "navigate",
                "url": button_url.as_str(),
                "policy": {
                    "same_webview_only": true,
                    "agent_on_tab_only": true
                }
            }),
        )?;
        let navigate_ok = navigate_content.get("runtime").and_then(Value::as_str)
            == Some("saccade-servoshell-bridge-v0")
            && navigate_content
                .pointer("/shell/engine")
                .and_then(Value::as_str)
                == Some("saccade-servoshell-bridge-navigate-v0");

        let button_actions_content = json_rpc_tool_content(
            &mut state,
            508,
            "saccade.web.actions",
            json!({
                "tab_id": tab_id,
                "engine": "servo"
            }),
        )?;
        let button_action_id = button_actions_content
            .get("actions")
            .and_then(Value::as_array)
            .and_then(|actions| {
                actions.iter().find_map(|action| {
                    let label = action.get("label").and_then(Value::as_str).unwrap_or("");
                    if label == "Preview Action" {
                        action
                            .get("action_id")
                            .or_else(|| action.get("id"))
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                    } else {
                        None
                    }
                })
            })
            .context("ServoShell bridge button action was not found")?;
        let button_basis_page_revision = button_actions_content
            .get("page_revision")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        let act_content = json_rpc_tool_content(
            &mut state,
            509,
            "saccade.web.act",
            json!({
                "tab_id": tab_id,
                "action_id": button_action_id,
                "basis_page_revision": button_basis_page_revision,
                "engine": "servo"
            }),
        )?;
        let act_ok = act_content.get("runtime").and_then(Value::as_str)
            == Some("saccade-servoshell-bridge-v0")
            && act_content
                .pointer("/verification/changed")
                .and_then(Value::as_bool)
                == Some(true)
            && act_content
                .pointer("/verification/mode")
                .and_then(Value::as_str)
                == Some("servoshell_bridge_webdriver_click_v0");

        let formmax_navigate_content = json_rpc_tool_content(
            &mut state,
            510,
            "saccade.browser.navigate",
            json!({
                "tab_id": tab_id,
                "action": "navigate",
                "url": formmax_url.as_str(),
                "policy": {
                    "same_webview_only": true,
                    "agent_on_tab_only": true
                }
            }),
        )?;
        let formmax_basis_page_revision = formmax_navigate_content
            .get("page_revision")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        let formmax_content = json_rpc_tool_content(
            &mut state,
            511,
            "saccade.web.fill_form",
            json!({
                "tab_id": tab_id,
                "basis_page_revision": formmax_basis_page_revision,
                "policy": {
                    "block_sensitive": true,
                    "local_fixture_only": true,
                    "live_worker_only": true
                }
            }),
        )?;
        let formmax_ok = formmax_content.get("runtime").and_then(Value::as_str)
            == Some("saccade-servoshell-bridge-v0")
            && formmax_content.get("engine").and_then(Value::as_str)
                == Some("saccade-servoshell-bridge-formmax-live-v0")
            && formmax_content.get("rows").and_then(Value::as_u64) == Some(96)
            && formmax_content.get("pages").and_then(Value::as_u64) == Some(2)
            && formmax_content.get("filled").and_then(Value::as_u64) == Some(672)
            && formmax_content
                .get("blocked_sensitive")
                .and_then(Value::as_u64)
                == Some(3)
            && formmax_content
                .get("receipt_verified")
                .and_then(Value::as_bool)
                == Some(true)
            && formmax_content
                .get("validation_errors")
                .and_then(Value::as_u64)
                == Some(0);
        let formmax_report_path = formmax_content
            .pointer("/artifacts/report")
            .and_then(Value::as_str)
            .context("ServoShell bridge FORMMAX did not return report artifact")?;
        let formmax_replay_path = formmax_content
            .pointer("/artifacts/replay")
            .and_then(Value::as_str)
            .context("ServoShell bridge FORMMAX did not return replay artifact")?;
        let formmax_report_path = safe_workspace_path(formmax_report_path)?;
        let formmax_replay_path = safe_workspace_path(formmax_replay_path)?;
        let formmax_replay_summary = json_rpc_tool_content(
            &mut state,
            512,
            "saccade.report.replay_summary",
            json!({
                "replay_path": formmax_replay_path.display().to_string()
            }),
        )?;
        let expected_formmax_replay_events = formmax_content
            .get("replay_events")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let formmax_artifacts_ok = formmax_report_path.exists()
            && formmax_replay_path.exists()
            && formmax_replay_summary.get("status").and_then(Value::as_str) == Some("ok")
            && formmax_replay_summary
                .get("events")
                .and_then(Value::as_u64)
                .is_some_and(|events| events >= expected_formmax_replay_events)
            && formmax_replay_summary
                .get("value_like_fields")
                .and_then(Value::as_u64)
                == Some(0);

        let bridge_ok = grant_attached
            && truth_ok
            && actions_ok
            && status_ok
            && fill_ok
            && inspect_ok
            && no_sensitive_leak
            && navigate_ok
            && act_ok
            && formmax_ok
            && formmax_artifacts_ok;
        if !bridge_ok {
            bail!(
                "servoshell bridge MCP gate failed: {}",
                json!({
                    "grant_attached": grant_attached,
                    "truth_ok": truth_ok,
                    "actions_ok": actions_ok,
                    "status_ok": status_ok,
                    "fill_ok": fill_ok,
                    "inspect_ok": inspect_ok,
                    "no_sensitive_leak": no_sensitive_leak,
                    "navigate_ok": navigate_ok,
                    "act_ok": act_ok,
                    "formmax_ok": formmax_ok,
                    "formmax_artifacts_ok": formmax_artifacts_ok,
                    "capabilities": capabilities,
                    "fill": {
                        "runtime": fill_content.get("runtime").cloned().unwrap_or(Value::Null),
                        "filled": fill_content.get("filled").cloned().unwrap_or(Value::Null),
                        "rejected": fill_content.get("rejected").cloned().unwrap_or(Value::Null),
                    },
                    "inspect": {
                        "runtime": inspect_content.get("runtime").cloned().unwrap_or(Value::Null),
                        "values_returned": inspect_content.get("values_returned").cloned().unwrap_or(Value::Null),
                        "values_redacted": inspect_content.get("values_redacted").cloned().unwrap_or(Value::Null),
                    },
                    "navigate": {
                        "runtime": navigate_content.get("runtime").cloned().unwrap_or(Value::Null),
                        "engine": navigate_content.pointer("/shell/engine").cloned().unwrap_or(Value::Null),
                    },
                    "act": {
                        "runtime": act_content.get("runtime").cloned().unwrap_or(Value::Null),
                        "mode": act_content.pointer("/verification/mode").cloned().unwrap_or(Value::Null),
                        "changed": act_content.pointer("/verification/changed").cloned().unwrap_or(Value::Null),
                    },
                    "formmax": {
                        "runtime": formmax_content.get("runtime").cloned().unwrap_or(Value::Null),
                        "engine": formmax_content.get("engine").cloned().unwrap_or(Value::Null),
                        "rows": formmax_content.get("rows").cloned().unwrap_or(Value::Null),
                        "pages": formmax_content.get("pages").cloned().unwrap_or(Value::Null),
                        "filled": formmax_content.get("filled").cloned().unwrap_or(Value::Null),
                        "blocked_sensitive": formmax_content.get("blocked_sensitive").cloned().unwrap_or(Value::Null),
                        "receipt_verified": formmax_content.get("receipt_verified").cloned().unwrap_or(Value::Null),
                        "validation_errors": formmax_content.get("validation_errors").cloned().unwrap_or(Value::Null),
                        "artifacts": formmax_content.get("artifacts").cloned().unwrap_or(Value::Null),
                        "replay_summary": formmax_replay_summary,
                    },
                })
            );
        }
        Ok(true)
    })();

    let shutdown = endpoint_for_shutdown
        .as_ref()
        .map(|endpoint| call_dogfood_control(endpoint, "shutdown", json!({})));
    let finish = finish_child_with_timeout(&mut child, Duration::from_secs(15));
    if let Some(Err(error)) = shutdown {
        if verification.is_ok() {
            return Err(error.context("failed to shutdown servoshell bridge control endpoint"));
        }
    }
    if let Err(error) = finish {
        if verification.is_ok() {
            return Err(error.context("failed to finish servoshell bridge child"));
        }
    }
    verification
}

fn json_rpc_tool_content(
    state: &mut McpSessionState,
    id: u64,
    name: &str,
    arguments: Value,
) -> Result<Value> {
    let response = handle_json_rpc(
        state,
        JsonRpcRequest {
            id: Some(json!(id)),
            method: "tools/call".into(),
            params: json!({
                "name": name,
                "arguments": arguments,
            }),
        },
    )
    .with_context(|| format!("{name} did not return a JSON-RPC response"))?;
    if let Some(error) = response.get("error") {
        bail!("{name} failed: {error}");
    }
    response
        .pointer("/result/structuredContent")
        .cloned()
        .with_context(|| format!("{name} response did not include structuredContent"))
}

fn wait_for_file_or_child_exit(path: &Path, child: &mut Child, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if path.exists() {
            return Ok(());
        }
        if let Some(status) = child
            .try_wait()
            .context("failed to poll servoshell bridge child")?
        {
            bail!(
                "servoshell bridge exited before writing {}: {status}",
                path.display()
            );
        }
        thread::sleep(Duration::from_millis(200));
    }
    bail!(
        "timed out waiting for servoshell bridge grant {}",
        path.display()
    )
}

fn finish_child_with_timeout(child: &mut Child, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if child
            .try_wait()
            .context("failed to poll servoshell bridge child")?
            .is_some()
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    child
        .kill()
        .context("failed to kill timed-out servoshell bridge child")?;
    let _ = child.wait();
    bail!("servoshell bridge child did not exit before timeout and was killed")
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
    fn contract_capabilities_are_versioned_and_discoverable() {
        let mut state = McpSessionState::default();
        let response = handle_json_rpc(
            &mut state,
            JsonRpcRequest {
                id: Some(json!(1)),
                method: "tools/call".to_string(),
                params: json!({
                    "name": "saccade.system.capabilities",
                    "arguments": {}
                }),
            },
        )
        .expect("capabilities request should return a response");
        assert_eq!(
            response.pointer("/result/structuredContent/saccade/contract_version"),
            Some(&json!(SACCADE_CONTRACT_VERSION))
        );
        assert!(
            response
                .pointer("/result/structuredContent/saccade/features")
                .and_then(Value::as_array)
                .is_some_and(|features| features.iter().any(|feature| feature == "typed_errors"))
        );
    }

    #[test]
    fn installed_product_hides_and_rejects_workspace_only_tools() {
        let mut state = McpSessionState {
            installed_product: true,
            ..McpSessionState::default()
        };
        let listed = handle_json_rpc(
            &mut state,
            JsonRpcRequest {
                id: Some(json!(1)),
                method: "tools/list".to_string(),
                params: json!({}),
            },
        )
        .expect("tools/list should return a response");
        let names = listed
            .pointer("/result/tools")
            .and_then(Value::as_array)
            .expect("installed tools array")
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(names.contains(&"saccade.tabs.open_agent"));
        assert!(names.contains(&"saccade.web.form_inventory"));
        assert!(!names.iter().any(|name| name.starts_with("saccade.dev.")));
        assert!(!names.iter().any(|name| name.starts_with("saccade.report.")));
        assert!(!names.contains(&"saccade.tabs.open"));
        assert!(!names.contains(&"saccade.web.fill_form"));

        let rejected = handle_json_rpc(
            &mut state,
            JsonRpcRequest {
                id: Some(json!(2)),
                method: "tools/call".to_string(),
                params: json!({
                    "name": "saccade.dev.open_local",
                    "arguments": {"url": "http://127.0.0.1:1/"}
                }),
            },
        )
        .expect("hidden tool call should return an error response");
        assert_eq!(
            rejected.pointer("/error/data/saccade_code"),
            Some(&json!("SACCADE_UNSUPPORTED"))
        );
    }

    #[test]
    fn render_preflight_schema_exposes_task_surface_profiles() {
        let schema = input_schema("saccade.web.render_preflight");
        let values = schema
            .pointer("/properties/expected_surface/enum")
            .and_then(Value::as_array)
            .expect("expected_surface enum");
        assert!(values.iter().any(|value| value == "page"));
        assert!(values.iter().any(|value| value == "github_issue"));
        assert!(values.iter().any(|value| value == "github_discussion"));
    }

    #[test]
    fn minimal_article_response_meets_simple_page_payload_budget() {
        let response = minimal_article_response(
            &json!({"text": "x".repeat(129), "page_revision": 7}),
            7,
            false,
        );
        assert_eq!(response.get("page_revision"), Some(&json!(7)));
        assert_eq!(response.get("untrusted"), Some(&json!(true)));
        assert!(response.get("truncated").is_none());
        assert_eq!(response.as_object().map(|object| object.len()), Some(3));
        let wire = serde_json::to_string(&response).expect("minimal article should serialize");
        assert!(
            wire.len() <= 198,
            "129-character article payload should stay within the Playwright comparison budget, got {} bytes",
            wire.len()
        );
    }

    #[test]
    fn minimal_form_inventory_keeps_action_and_protected_status_without_values() {
        let response = minimal_form_inventory_response(
            &json!({
                "page_revision": 9,
                "field_count": 2,
                "eligible_count": 1,
                "sensitive_count": 1,
                "has_more": false,
                "fields": [
                    {
                        "field_id": "name", "label": "Name", "type": "text",
                        "required": true, "eligible": true,
                        "sensitivity": "none", "value_state": "empty"
                    },
                    {
                        "field_id": "passport", "label": "Passport number", "type": "text",
                        "eligible": false, "sensitivity": "government_identifier",
                        "value_state": "requires_user_input",
                        "blocked_reason": "protected_identifier",
                        "raw_value": "must-not-escape"
                    }
                ]
            }),
            true,
            false,
        );
        assert_eq!(response.get("ready"), Some(&json!(true)));
        assert_eq!(
            response.pointer("/fields/0/status"),
            Some(&json!("fillable"))
        );
        assert_eq!(
            response.pointer("/fields/1/status"),
            Some(&json!("requires_user_input"))
        );
        assert_eq!(response.pointer("/fields/1/protected"), Some(&json!(true)));
        assert!(!response.to_string().contains("must-not-escape"));
    }

    #[test]
    fn post_execute_inventory_flags_new_ordinary_fields_only() {
        let dynamic = json!({
            "fields": [
                {"field_id": "company", "status": "fillable"},
                {"field_id": "passport", "status": "requires_user_input", "protected": true}
            ]
        });
        assert!(form_inventory_requires_follow_up(&dynamic));
        assert!(!form_inventory_requires_follow_up(&json!({
            "fields": [
                {"field_id": "passport", "status": "requires_user_input", "protected": true},
                {"field_id": "name", "status": "preserve_existing_value"}
            ]
        })));
    }

    #[test]
    fn minimal_open_agent_response_keeps_only_routing_state() {
        let response = minimal_open_agent_response(
            &json!({
                "ready_for_read": true,
                "grant_path": "/must/not/escape",
                "same_webview_control": {"capability": "must-not-escape"},
                "tab": {
                    "tab_id": 7,
                    "owner": "agent",
                    "url": "https://example.com/private-query",
                    "title": "Example",
                    "page_revision": 11,
                    "visual_marker": {"badge": "Agent"}
                }
            }),
            true,
        )
        .expect("minimal Agent-open response");
        assert_eq!(response.pointer("/tab/tab_id"), Some(&json!(7)));
        assert_eq!(response.pointer("/tab/page_revision"), Some(&json!(11)));
        assert_eq!(response.get("ready"), Some(&json!(true)));
        assert_eq!(
            response.get("summary"),
            Some(&json!("Agent tab ready for direct read or action"))
        );
        assert_eq!(response.get("browser_was_running"), Some(&json!(true)));
        assert!(!response.to_string().contains("must-not-escape"));
        assert!(!response.to_string().contains("private-query"));
    }

    #[test]
    fn model_facing_read_schemas_default_to_minimal() {
        assert_eq!(
            input_schema("saccade.web.article_text").pointer("/properties/mode/default"),
            Some(&json!("minimal"))
        );
        assert_eq!(
            input_schema("saccade.web.form_inventory").pointer("/properties/mode/default"),
            Some(&json!("minimal"))
        );
    }

    #[test]
    fn initialization_defaults_to_agent_completed_ordinary_forms() {
        let mut state = McpSessionState::default();
        let response = handle_json_rpc(
            &mut state,
            JsonRpcRequest {
                id: Some(json!(1)),
                method: "initialize".to_string(),
                params: json!({}),
            },
        )
        .expect("initialize should return a response");
        let instructions = response
            .pointer("/result/instructions")
            .and_then(Value::as_str)
            .expect("model-facing instructions");
        assert!(instructions.contains("complete ordinary fields"));
        assert!(instructions.contains("instead of asking the user to type or click"));
        assert!(instructions.contains("Respect the user's stopping point"));
        assert!(instructions.contains("follow_up_required=true"));
        assert!(instructions.contains("Never read or fill passwords, OTPs, or CVVs"));
        assert_eq!(
            response.pointer("/result/saccade/form_behavior/authorized_ordinary_fields"),
            Some(&json!("fill_without_manual_handoff"))
        );
    }

    #[test]
    fn rpc_errors_expose_stable_saccade_codes() {
        let response = rpc_error(
            json!(7),
            -32603,
            "Internal error",
            "saccade.web.act requires integer basis_page_revision".to_string(),
        );
        assert_eq!(
            response.pointer("/error/data/saccade_code"),
            Some(&json!("SACCADE_INVALID_ARGUMENT"))
        );
        assert_eq!(
            rpc_error_detail(response.get("error").expect("error object")),
            Some("saccade.web.act requires integer basis_page_revision")
        );
        assert_eq!(
            saccade_error_code("tool arguments must include integer field tab_id"),
            "SACCADE_INVALID_ARGUMENT"
        );
        assert_eq!(
            saccade_error_code("stale action basis: requested 1, current 2"),
            "SACCADE_STALE_BASIS"
        );
        assert_eq!(
            saccade_error_code(
                "PROVIDER_REJECTED for act: human verification provider rejected the session"
            ),
            "SACCADE_PROVIDER_REJECTED"
        );
        let provider_error = rpc_error(
            json!(8),
            -32603,
            "Internal error",
            "PROVIDER_REJECTED for act: human verification provider rejected the session"
                .to_string(),
        );
        assert_eq!(
            provider_error.pointer("/error/data/requires_human"),
            Some(&json!(true))
        );
        assert_eq!(
            provider_error.pointer("/error/data/retryable"),
            Some(&json!(true))
        );
    }

    #[test]
    fn dogfood_control_socket_addr_maps_allowed_loopback_hosts() {
        let endpoint = DogfoodControlEndpoint {
            host: "localhost".to_string(),
            port: 49321,
            protocol: "saccade-dogfood-control-v1".to_string(),
            capability: "test-capability-token-with-sufficient-length".to_string(),
            engine_grant: None,
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
            protocol: "saccade-dogfood-control-v1".to_string(),
            capability: "test-capability-token-with-sufficient-length".to_string(),
            engine_grant: None,
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
                protocol: "saccade-dogfood-control-v1".to_string(),
                capability: "test-capability-token-with-sufficient-length".to_string(),
                engine_grant: None,
            },
        );

        assert_eq!(tab_runtime(&state, TabId(7)), "saccade-dogfood-control-v1");
        assert_eq!(tab_runtime(&state, TabId(8)), "mcp_report_backed_v0");
    }

    #[test]
    fn remote_grants_require_a_trusted_loopback_browser_control() {
        let endpoint = DogfoodControlEndpoint {
            host: "127.0.0.1".to_string(),
            port: 49323,
            protocol: "saccade-dogfood-control-v1".to_string(),
            capability: "test-capability-token-with-sufficient-length".to_string(),
            engine_grant: None,
        };
        let grant = json!({
            "runtime": "saccade-chrome-compat-cdp-v0",
            "rendering_profile": "chrome-compatibility",
            "transport_status": "chrome_compatibility_control_v0",
        });
        assert!(is_chrome_compatibility_grant(&grant, Some(&endpoint)));
        assert!(!is_chrome_compatibility_grant(&grant, None));
        assert!(!is_chrome_compatibility_grant(&json!({}), Some(&endpoint)));

        let servoshell_grant = json!({
            "runtime": "saccade-servoshell-bridge-v0",
            "rendering_profile": "official-servoshell",
            "transport_status": "official_servoshell_bridge_control_v0",
            "copilot": {
                "page_dom_injected": false,
                "sensitive_values_exposed_to_agent": false,
            }
        });
        assert!(is_official_servoshell_bridge_grant(
            &servoshell_grant,
            Some(&endpoint)
        ));
        assert!(!is_official_servoshell_bridge_grant(
            &servoshell_grant,
            None
        ));
        assert!(!is_official_servoshell_bridge_grant(
            &json!({}),
            Some(&endpoint)
        ));
    }

    #[test]
    fn control_grant_requires_a_session_capability() {
        let base = json!({
            "control_endpoint": {
                "protocol": "saccade-dogfood-control-v1",
                "scheme": "tcp",
                "host": "127.0.0.1",
                "port": 49324
            }
        });
        let missing = dogfood_control_endpoint_from_grant(&base).unwrap_err();
        assert!(missing.to_string().contains("session capability"));

        let grant = json!({
            "control_endpoint": base["control_endpoint"].clone(),
            "control_capability": {
                "scheme": "saccade_session_bearer_v1",
                "token": "test-capability-token-with-sufficient-length"
            }
        });
        let endpoint = dogfood_control_endpoint_from_grant(&grant)
            .expect("capability grant should parse")
            .expect("endpoint should be present");
        assert_eq!(endpoint.protocol, "saccade-dogfood-control-v1");
        assert_eq!(
            endpoint.capability,
            "test-capability-token-with-sufficient-length"
        );
    }

    #[test]
    fn engine_adapter_grant_is_capability_based() {
        let grant = json!({
            "engine_adapter": {
                "contract_version": "1.0",
                "transport": "owner_only_unix_v1",
                "provenance": "browser_process",
                "page_dom_injected": false,
                "sensitive_values_exposed_to_agent": false,
                "capabilities": ["ping", "shell_status", "navigate", "pause", "close"]
            },
            "control_endpoint": {
                "protocol": "saccade-engine-control-v1",
                "scheme": "unix",
                "path": "/tmp/saccade-test/control.sock"
            },
            "control_capability": {
                "scheme": "saccade_session_bearer_v1",
                "token": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            }
        });
        let endpoint = dogfood_control_endpoint_from_grant(&grant)
            .expect("engine-neutral grant should parse")
            .expect("engine-neutral endpoint should be present");
        assert_eq!(endpoint.protocol, CONTROL_PROTOCOL_VERSION);
        assert!(is_engine_adapter_grant(Some(&endpoint)));

        let mut unsafe_grant = grant;
        unsafe_grant["engine_adapter"]["sensitive_values_exposed_to_agent"] = json!(true);
        assert!(dogfood_control_endpoint_from_grant(&unsafe_grant).is_err());
    }

    #[test]
    fn cef_integer_like_page_revision_updates_mcp_tab() {
        assert_eq!(json_number_u64(&json!(2)), Some(2));
        assert_eq!(json_number_u64(&json!(2.0)), Some(2));
        assert_eq!(json_number_u64(&json!(2.5)), None);
        assert_eq!(json_number_u64(&json!(-1.0)), None);

        let mut tab = SessionTab {
            info: tab(
                1,
                TabOwner::Human,
                ReadGrant::FullTruth,
                "about:blank",
                "test",
            ),
            paused: false,
            agent_input_grant: true,
            grant_reason: None,
            last_engine: None,
            last_summary: None,
            last_report_path: None,
            last_replay_path: None,
            last_actions: Vec::new(),
            last_findings: Vec::new(),
        };
        update_session_tab_from_browser_result(&mut tab, &json!({"page_revision": 3.0}));
        assert_eq!(tab.info.page_revision, 3);
    }
}
