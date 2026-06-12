use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use saccade_core::{ReadGrant, TabId, TabInfo, TabOwner, TabVisualMarker};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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
        Command::Selftest => selftest(),
        Command::Tools => print_tools(),
    }
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
        && external_dev_url_rejected;
    let evidence = SelftestEvidence {
        denied_human_input: tab_evidence.denied_human_input,
        denied_human_truth_without_grant: tab_evidence.denied_human_truth_without_grant,
        allowed_agent_truth: tab_evidence.allowed_agent_truth,
        allowed_human_truth_with_grant: tab_evidence.allowed_human_truth_with_grant,
        external_dev_url_rejected,
        local_audit_summary: local_audit.summary.clone(),
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
            ),
            tool(
                "saccade.dev.audit_page",
                ToolNamespace::Dev,
                ToolRisk::LocalSafe,
                "Return compact rendered truth, action map summary, findings, and artifact paths for a local dev page.",
                true,
                false,
            ),
            tool(
                "saccade.dev.click_all_primary_actions",
                ToolNamespace::Dev,
                ToolRisk::PolicyGated,
                "Verify primary local-dev actions through Saccade action IDs and policy.",
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
            ),
            tool(
                "saccade.dev.get_report",
                ToolNamespace::Dev,
                ToolRisk::ReportOnly,
                "Fetch a compact development audit report by run ID.",
                true,
                false,
            ),
            tool(
                "saccade.tabs.list",
                ToolNamespace::Tabs,
                ToolRisk::ReportOnly,
                "List known tabs with owner, read grant, URL, and page revision.",
                true,
                false,
            ),
            tool(
                "saccade.tabs.open",
                ToolNamespace::Tabs,
                ToolRisk::PolicyGated,
                "Open a URL in a Human or Agent tab under explicit ownership.",
                true,
                false,
            ),
            tool(
                "saccade.tabs.request_user_login",
                ToolNamespace::Tabs,
                ToolRisk::PolicyGated,
                "Ask the user to log in in a Human tab, then expose only safe session status to Agent tabs.",
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
            ),
            tool(
                "saccade.tabs.pause_agent",
                ToolNamespace::Tabs,
                ToolRisk::PolicyGated,
                "Pause pending agent actions for a tab.",
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
            ),
            tool(
                "saccade.web.truth",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Return redacted browser truth for a tab and page revision.",
                true,
                false,
            ),
            tool(
                "saccade.web.actions",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Return an action map with stable action IDs and page revision basis.",
                true,
                false,
            ),
            tool(
                "saccade.web.act",
                ToolNamespace::Web,
                ToolRisk::PolicyGated,
                "Perform one verified action by action ID and page revision basis.",
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
            ),
            tool(
                "saccade.report.validate_run",
                ToolNamespace::Report,
                ToolRisk::ReportOnly,
                "Validate a run directory and return compact status plus artifact paths.",
                true,
                false,
            ),
            tool(
                "saccade.report.replay_summary",
                ToolNamespace::Report,
                ToolRisk::ReportOnly,
                "Summarize replay JSONL without emitting full replay content.",
                true,
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
    }
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
