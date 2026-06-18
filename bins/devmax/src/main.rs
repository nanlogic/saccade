use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tiny_http::{Header, Response, Server, StatusCode};
use url::Url;

const ENGINE: &str = "static-fixture-v0";
const REQUIRED_FIXTURE_TOTAL: usize = 20;
const MIN_DETECTED: usize = 17;
const MAX_FALSE_POSITIVES: usize = 1;
const SERVO_ENGINE: &str = "servo-rendered-probe-v0";
const CHROME_ENGINE: &str = "chrome-cdp-reference-v1";
const SERVO_FIXTURES: &[&str] = &[
    "blank_page",
    "invisible_text",
    "offscreen_button",
    "modal_blocks_page",
    "canvas_chart_blank",
    "button_no_handler",
    "console_error",
    "missing_asset",
];

const FIXTURES: &[&str] = &[
    "blank_page",
    "console_error",
    "hydration_error",
    "missing_asset",
    "invisible_text",
    "overlapping_elements",
    "offscreen_button",
    "button_no_handler",
    "broken_form_validation",
    "lazy_route_error",
    "scroll_container_hidden_submit",
    "responsive_mobile_break",
    "modal_blocks_page",
    "canvas_chart_blank",
    "css_zindex_overlay_bug",
    "wrong_success_state",
    "stuck_loading_spinner",
    "disabled_primary_action",
    "duplicate_id_controls",
    "wrong_route_404",
];

#[derive(Parser)]
#[command(name = "devmax")]
#[command(about = "Saccade local development self-test harness")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Audit {
        #[arg(long)]
        url: Url,
        #[arg(long, default_value = "static")]
        engine: String,
        #[arg(long)]
        replay: bool,
    },
    SelftestFixtures,
    SelftestServoFixtures,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DevmaxReport {
    run_id: String,
    engine: String,
    page_revision: u64,
    url: String,
    title: String,
    summary: String,
    visual_health: VisualHealth,
    runtime_health: RuntimeHealth,
    actions: Vec<ActionInfo>,
    findings: Vec<Finding>,
    recommendations: Vec<Recommendation>,
    artifacts: ReportArtifacts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct VisualHealth {
    blank_page: bool,
    large_empty_regions: Vec<String>,
    invisible_text: Vec<VisualIssue>,
    overlaps: Vec<OverlapIssue>,
    offscreen_interactive: Vec<ActionInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RuntimeHealth {
    console_errors: Vec<String>,
    network_errors: Vec<String>,
    uncaught_exceptions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VisualIssue {
    text: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OverlapIssue {
    front: String,
    back: String,
    severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActionInfo {
    action_id: String,
    label: String,
    kind: String,
    enabled: bool,
    blocked_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Finding {
    finding_id: String,
    kind: String,
    severity: String,
    selector: Option<String>,
    message: String,
    evidence: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Recommendation {
    kind: String,
    message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ReportArtifacts {
    report: Option<String>,
    replay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    browser_screenshot: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    finding_crops: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    action_receipts: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chrome_manifest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chrome_screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chrome_truth: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chrome_network: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SelftestCaseResult {
    fixture: String,
    url: String,
    expected: String,
    detected: bool,
    false_positives: usize,
    finding_crops: usize,
    missing_finding_crops: usize,
    action_receipts: usize,
    report: String,
    replay: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SelftestSummary {
    run_id: String,
    total: usize,
    detected: usize,
    false_positives: usize,
    finding_crops: usize,
    missing_finding_crops: usize,
    multi_action_receipt_cases: usize,
    output_dir: String,
    cases: Vec<SelftestCaseResult>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Audit {
            url,
            engine,
            replay,
        } => audit(url, engine, replay),
        Command::SelftestFixtures => selftest_fixtures(),
        Command::SelftestServoFixtures => selftest_servo_fixtures(),
    }
}

fn audit(url: Url, engine: String, replay: bool) -> Result<()> {
    let run_id = format!("audit_{}", unix_ms()?);
    let output_dir = workspace_root()?.join("runs").join("devmax").join(&run_id);
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;

    let mut report = match engine.as_str() {
        "static" => {
            let html = fetch_http_body(&url).with_context(|| format!("failed to fetch {url}"))?;
            analyze_html(run_id.clone(), url.clone(), &html, None)?
        }
        "servo" => {
            let probe_artifacts = output_dir.join("browser_artifacts");
            let probe =
                saccade_browser::devmax_probe_with_artifacts(url.clone(), &probe_artifacts)?;
            analyze_servo_probe(run_id.clone(), url.clone(), probe)?
        }
        "chrome" => analyze_chrome_reference(run_id.clone(), url.clone(), &output_dir)?,
        other => bail!("unsupported DEVMAX engine {other:?}; expected static, servo, or chrome"),
    };
    let report_path = output_dir.join("report.json");
    let replay_path = output_dir.join("replay.jsonl");
    report.artifacts.report = Some(report_path.display().to_string());
    if replay {
        report.artifacts.replay = Some(replay_path.display().to_string());
    }
    write_report(&report_path, &report)?;
    if replay {
        write_replay(&replay_path, &run_id, &url, "unknown", &report)?;
    }

    println!(
        "DEVMAX AUDIT PASS report={} replay={} findings={}",
        report_path.display(),
        if replay {
            replay_path.display().to_string()
        } else {
            "none".into()
        },
        report.findings.len(),
    );
    Ok(())
}

fn selftest_fixtures() -> Result<()> {
    if FIXTURES.len() != REQUIRED_FIXTURE_TOTAL {
        bail!(
            "expected {REQUIRED_FIXTURE_TOTAL} fixtures, found {}",
            FIXTURES.len()
        );
    }

    let workspace = workspace_root()?;
    let fixture_root = workspace.join("test_pages").join("devmax");
    let base_url = start_test_server(fixture_root.clone())?;
    let run_id = format!("selftest_{}", unix_ms()?);
    let output_dir = workspace.join("runs").join("devmax").join(&run_id);
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;

    let mut cases = Vec::new();
    let mut detected = 0;
    let mut false_positives = 0;

    for fixture in FIXTURES {
        let url = base_url
            .join(&format!("{fixture}/index.html"))
            .with_context(|| format!("failed to build URL for fixture {fixture}"))?;
        let html = fetch_http_body(&url).with_context(|| format!("failed to fetch {url}"))?;
        let expected = expected_from_html(&html)
            .with_context(|| format!("fixture {fixture} is missing devmax expected metadata"))?;

        let fixture_dir = fixture_root.join(fixture);
        let mut report = analyze_html(run_id.clone(), url.clone(), &html, Some(&fixture_dir))?;
        let case_dir = output_dir.join(fixture);
        fs::create_dir_all(&case_dir)
            .with_context(|| format!("failed to create {}", case_dir.display()))?;
        let report_path = case_dir.join("report.json");
        let replay_path = case_dir.join("replay.jsonl");
        report.artifacts.report = Some(report_path.display().to_string());
        report.artifacts.replay = Some(replay_path.display().to_string());
        write_report(&report_path, &report)?;
        write_replay(&replay_path, &run_id, &url, &expected, &report)?;

        let expected_detected = report
            .findings
            .iter()
            .any(|finding| finding.kind == expected);
        let case_false_positives = report
            .findings
            .iter()
            .filter(|finding| finding.kind != expected)
            .count();
        detected += usize::from(expected_detected);
        false_positives += case_false_positives;
        cases.push(SelftestCaseResult {
            fixture: fixture.to_string(),
            url: url.to_string(),
            expected,
            detected: expected_detected,
            false_positives: case_false_positives,
            finding_crops: report.artifacts.finding_crops.len(),
            missing_finding_crops: missing_finding_crops(&report),
            action_receipts: report.artifacts.action_receipts.len(),
            report: report_path.display().to_string(),
            replay: replay_path.display().to_string(),
        });
    }

    let summary = SelftestSummary {
        run_id,
        total: cases.len(),
        detected,
        false_positives,
        finding_crops: cases.iter().map(|case| case.finding_crops).sum(),
        missing_finding_crops: cases.iter().map(|case| case.missing_finding_crops).sum(),
        multi_action_receipt_cases: cases.iter().filter(|case| case.action_receipts > 1).count(),
        output_dir: output_dir.display().to_string(),
        cases,
    };
    write_report(&output_dir.join("summary.json"), &summary)?;

    if summary.total != REQUIRED_FIXTURE_TOTAL
        || summary.detected < MIN_DETECTED
        || summary.false_positives > MAX_FALSE_POSITIVES
    {
        bail!(
            "DEVMAX FIXTURES FAIL total={} detected={} false_positives={} report={}",
            summary.total,
            summary.detected,
            summary.false_positives,
            output_dir.display(),
        );
    }

    println!(
        "DEVMAX FIXTURES PASS total={} detected={} false_positives={} report={}",
        summary.total,
        summary.detected,
        summary.false_positives,
        output_dir.display(),
    );
    Ok(())
}

fn selftest_servo_fixtures() -> Result<()> {
    let workspace = workspace_root()?;
    let fixture_root = workspace.join("test_pages").join("devmax");
    let base_url = start_test_server(fixture_root)?;
    let run_id = format!("servo_selftest_{}", unix_ms()?);
    let output_dir = workspace.join("runs").join("devmax").join(&run_id);
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;

    let mut cases = Vec::new();
    let mut detected = 0;
    let mut false_positives = 0;
    let mut missing_crop_total = 0;
    let mut multi_action_receipt_cases = 0;

    for fixture in SERVO_FIXTURES {
        let url = base_url
            .join(&format!("{fixture}/index.html"))
            .with_context(|| format!("failed to build URL for fixture {fixture}"))?;
        let html = fetch_http_body(&url).with_context(|| format!("failed to fetch {url}"))?;
        let expected = expected_from_html(&html)
            .with_context(|| format!("fixture {fixture} is missing devmax expected metadata"))?;
        let (report_path, replay_path) =
            run_servo_audit_child(&url).with_context(|| format!("Servo audit failed for {url}"))?;
        let report: DevmaxReport = serde_json::from_str(
            &fs::read_to_string(&report_path)
                .with_context(|| format!("failed to read {}", report_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", report_path.display()))?;

        let expected_detected = report
            .findings
            .iter()
            .any(|finding| finding.kind == expected);
        let case_false_positives = report
            .findings
            .iter()
            .filter(|finding| finding.kind != expected)
            .count();
        let case_missing_crops = missing_finding_crops(&report);
        let case_action_receipts = report.artifacts.action_receipts.len();
        detected += usize::from(expected_detected);
        false_positives += case_false_positives;
        missing_crop_total += case_missing_crops;
        multi_action_receipt_cases += usize::from(case_action_receipts > 1);
        cases.push(SelftestCaseResult {
            fixture: fixture.to_string(),
            url: url.to_string(),
            expected,
            detected: expected_detected,
            false_positives: case_false_positives,
            finding_crops: report.artifacts.finding_crops.len(),
            missing_finding_crops: case_missing_crops,
            action_receipts: case_action_receipts,
            report: report_path.display().to_string(),
            replay: replay_path.display().to_string(),
        });
    }

    let summary = SelftestSummary {
        run_id,
        total: cases.len(),
        detected,
        false_positives,
        finding_crops: cases.iter().map(|case| case.finding_crops).sum(),
        missing_finding_crops: missing_crop_total,
        multi_action_receipt_cases,
        output_dir: output_dir.display().to_string(),
        cases,
    };
    write_report(&output_dir.join("summary.json"), &summary)?;

    if summary.total != SERVO_FIXTURES.len()
        || summary.detected != SERVO_FIXTURES.len()
        || summary.false_positives > MAX_FALSE_POSITIVES
        || summary.missing_finding_crops > 0
        || summary.multi_action_receipt_cases == 0
    {
        bail!(
            "DEVMAX SERVO FIXTURES FAIL total={} detected={} false_positives={} missing_finding_crops={} multi_action_receipt_cases={} report={}",
            summary.total,
            summary.detected,
            summary.false_positives,
            summary.missing_finding_crops,
            summary.multi_action_receipt_cases,
            output_dir.display(),
        );
    }

    println!(
        "DEVMAX SERVO FIXTURES PASS total={} detected={} false_positives={} finding_crops={} multi_action_receipt_cases={} report={}",
        summary.total,
        summary.detected,
        summary.false_positives,
        summary.finding_crops,
        summary.multi_action_receipt_cases,
        output_dir.display(),
    );
    Ok(())
}

fn run_servo_audit_child(url: &Url) -> Result<(PathBuf, PathBuf)> {
    let exe = std::env::current_exe().context("failed to locate current devmax executable")?;
    let output = ProcessCommand::new(exe)
        .arg("audit")
        .arg("--url")
        .arg(url.as_str())
        .arg("--engine")
        .arg("servo")
        .arg("--replay")
        .env("RUST_LOG", "error")
        .output()
        .context("failed to run child devmax Servo audit")?;

    if !output.status.success() {
        bail!(
            "child devmax Servo audit failed: status={} stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let report = parse_output_path(&stdout, "report=")
        .with_context(|| format!("child output missing report path: {stdout}"))?;
    let replay = parse_output_path(&stdout, "replay=")
        .with_context(|| format!("child output missing replay path: {stdout}"))?;
    Ok((report, replay))
}

fn parse_output_path(stdout: &str, field: &str) -> Option<PathBuf> {
    let start = stdout.find(field)? + field.len();
    let rest = &stdout[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some(PathBuf::from(rest[..end].trim()))
}

fn analyze_html(
    run_id: String,
    url: Url,
    html: &str,
    fixture_dir: Option<&Path>,
) -> Result<DevmaxReport> {
    let title = extract_between(html, "<title>", "</title>").unwrap_or_else(|| "Untitled".into());
    let mut findings = Vec::new();
    let mut visual_health = VisualHealth::default();
    let mut runtime_health = RuntimeHealth::default();
    let mut actions = collect_actions(html);

    if is_blank_page(html) {
        visual_health.blank_page = true;
        findings.push(finding(
            "blank_page",
            "critical",
            Some("body"),
            "Page has no visible application content.",
            json!({ "engine": ENGINE }),
        ));
    }

    if html.contains("console.error(") {
        runtime_health
            .console_errors
            .push("console.error call found in page script".into());
        findings.push(finding(
            "console_error",
            "high",
            Some("script"),
            "Page emits a console error during startup.",
            json!({ "pattern": "console.error(" }),
        ));
    }

    if html.contains("throw new Error(") {
        runtime_health
            .uncaught_exceptions
            .push("throw new Error call found in page script".into());
    }

    if html.contains("data-hydration-error") || html.contains("Hydration failed") {
        findings.push(finding(
            "hydration_error",
            "high",
            Some("[data-hydration-error]"),
            "Hydration marker indicates client/server UI mismatch.",
            json!({ "marker": "data-hydration-error" }),
        ));
    }

    if has_missing_asset(html, fixture_dir) {
        runtime_health
            .network_errors
            .push("referenced asset is missing".into());
        findings.push(finding(
            "missing_asset",
            "high",
            Some("[src], [href]"),
            "Page references an asset that cannot be found in the fixture.",
            json!({ "checked_fixture_dir": fixture_dir.map(|path| path.display().to_string()) }),
        ));
    }

    if html.contains("data-devmax-invisible-text")
        || html.contains("color: white; background: white")
    {
        visual_health.invisible_text.push(VisualIssue {
            text: "Submit".into(),
            reason: "foreground and background are effectively identical".into(),
        });
        findings.push(finding(
            "invisible_text",
            "medium",
            Some("[data-devmax-invisible-text]"),
            "Text is present but visually invisible.",
            json!({ "text": "Submit" }),
        ));
    }

    if html.contains("data-devmax-overlap") {
        visual_health.overlaps.push(OverlapIssue {
            front: "promo_panel".into(),
            back: "primary_cta".into(),
            severity: "blocking".into(),
        });
        findings.push(finding(
            "overlapping_elements",
            "high",
            Some("[data-devmax-overlap]"),
            "Primary action is visually covered by another element.",
            json!({ "front": "promo_panel", "back": "primary_cta" }),
        ));
    }

    if html.contains("data-devmax-offscreen") || html.contains("left: -9999px") {
        let action = ActionInfo {
            action_id: "act_export".into(),
            label: "Export".into(),
            kind: "click".into(),
            enabled: true,
            blocked_by: Some("offscreen".into()),
        };
        visual_health.offscreen_interactive.push(action.clone());
        findings.push(finding(
            "offscreen_button",
            "medium",
            Some("[data-devmax-offscreen]"),
            "Interactive control is positioned outside the viewport.",
            json!({ "action": action }),
        ));
    }

    if html.contains("data-devmax-no-handler") {
        findings.push(finding(
            "button_no_handler",
            "high",
            Some("[data-devmax-no-handler]"),
            "Button is enabled but has no action handler marker.",
            json!({ "action_id": "act_save" }),
        ));
    }

    if html.contains("data-devmax-broken-validation") {
        findings.push(finding(
            "broken_form_validation",
            "high",
            Some("form"),
            "Form can enter an invalid state without showing a validation message.",
            json!({ "form": "signup" }),
        ));
    }

    if html.contains("data-devmax-lazy-route-error") {
        runtime_health
            .uncaught_exceptions
            .push("lazy route chunk rejects when opened".into());
        findings.push(finding(
            "lazy_route_error",
            "high",
            Some("[data-devmax-lazy-route-error]"),
            "Lazy route is linked but its chunk fails to load.",
            json!({ "route": "/settings" }),
        ));
    }

    if html.contains("data-devmax-scroll-hidden-submit") {
        findings.push(finding(
            "scroll_container_hidden_submit",
            "medium",
            Some("[data-devmax-scroll-hidden-submit]"),
            "Submit action is trapped below a clipped scroll container.",
            json!({ "container": "capacity_grid" }),
        ));
    }

    if html.contains("data-devmax-responsive-break") {
        findings.push(finding(
            "responsive_mobile_break",
            "medium",
            Some("[data-devmax-responsive-break]"),
            "Mobile layout marker indicates horizontal overflow.",
            json!({ "viewport": "390x844" }),
        ));
    }

    if html.contains("data-devmax-modal-blocks") {
        for action in actions.iter_mut() {
            if action.action_id == "act_submit" {
                action.blocked_by = Some("modal_overlay".into());
            }
        }
        findings.push(finding(
            "modal_blocks_page",
            "high",
            Some("[data-devmax-modal-blocks]"),
            "A modal overlay blocks interaction with the page.",
            json!({ "blocked_action": "act_submit" }),
        ));
    }

    if html.contains("data-devmax-blank-canvas") {
        findings.push(finding(
            "canvas_chart_blank",
            "medium",
            Some("canvas"),
            "Canvas chart element exists but fixture marks it as blank.",
            json!({ "canvas_id": "revenue-chart" }),
        ));
    }

    if html.contains("data-devmax-zindex-overlay") {
        visual_health.overlaps.push(OverlapIssue {
            front: "zindex_overlay".into(),
            back: "nav_menu".into(),
            severity: "blocking".into(),
        });
        findings.push(finding(
            "css_zindex_overlay_bug",
            "high",
            Some("[data-devmax-zindex-overlay]"),
            "Stacking context leaves an invisible overlay above controls.",
            json!({ "front": "zindex_overlay", "back": "nav_menu" }),
        ));
    }

    if html.contains("data-devmax-wrong-success") {
        findings.push(finding(
            "wrong_success_state",
            "high",
            Some("[data-devmax-wrong-success]"),
            "Success state is shown even though the fixture marks the request as failed.",
            json!({ "expected_state": "error", "visible_state": "success" }),
        ));
    }

    if html.contains("data-devmax-stuck-loading") {
        findings.push(finding(
            "stuck_loading_spinner",
            "high",
            Some("[data-devmax-stuck-loading]"),
            "Loading state remains visible without resolved content or an error path.",
            json!({ "state": "loading", "timeout_policy": "missing" }),
        ));
    }

    if html.contains("data-devmax-disabled-primary") {
        findings.push(finding(
            "disabled_primary_action",
            "medium",
            Some("[data-devmax-disabled-primary]"),
            "Primary action is disabled without an explanation or recovery path.",
            json!({ "action_id": "act_primary", "reason_visible": false }),
        ));
    }

    if html.contains("data-devmax-duplicate-id") || has_duplicate_id(html) {
        findings.push(finding(
            "duplicate_id_controls",
            "medium",
            Some("[id]"),
            "Page contains duplicate IDs that can break labels, selectors, and action maps.",
            json!({ "id_uniqueness": "violated" }),
        ));
    }

    if html.contains("data-devmax-wrong-route") {
        runtime_health
            .network_errors
            .push("primary navigation points at a missing route".into());
        findings.push(finding(
            "wrong_route_404",
            "high",
            Some("[data-devmax-wrong-route]"),
            "Primary navigation target is a missing route or broken deep link.",
            json!({ "route": "/admin/reports", "status": 404 }),
        ));
    }

    let summary = if findings.is_empty() {
        "No DEVMAX findings from static fixture analyzer.".into()
    } else {
        format!(
            "Detected {} issue(s); highest severity: {}.",
            findings.len(),
            highest_severity(&findings)
        )
    };
    let recommendations = findings
        .iter()
        .map(|finding| Recommendation {
            kind: "fix".into(),
            message: fix_hint(&finding.kind).into(),
        })
        .collect();

    Ok(DevmaxReport {
        run_id,
        engine: ENGINE.into(),
        page_revision: 1,
        url: url.to_string(),
        title,
        summary,
        visual_health,
        runtime_health,
        actions,
        findings,
        recommendations,
        artifacts: ReportArtifacts::default(),
    })
}

fn analyze_servo_probe(run_id: String, url: Url, probe: Value) -> Result<DevmaxReport> {
    let title = probe
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled")
        .to_string();
    let mut findings = Vec::new();
    let mut visual_health = VisualHealth::default();
    let mut runtime_health = RuntimeHealth::default();
    let mut actions = collect_probe_actions(&probe);

    for message in probe
        .pointer("/runtime/console_messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        if message
            .get("level")
            .and_then(Value::as_str)
            .is_some_and(|level| level == "error")
        {
            let text = message
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            runtime_health.console_errors.push(text);
            findings.push(finding(
                "console_error",
                "high",
                Some("console"),
                "Servo WebView delegate captured a page console error.",
                message,
            ));
        }
    }

    for request in probe
        .pointer("/runtime/network_requests")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        if request
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(|url| url.contains("missing"))
            && !request
                .get("is_for_main_frame")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            let url = request
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            runtime_health.network_errors.push(url);
            findings.push(finding(
                "missing_asset",
                "high",
                Some("[src], [href]"),
                "Servo WebView delegate observed a request for a missing fixture asset.",
                request,
            ));
        }
    }

    if probe
        .get("blankPage")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        visual_health.blank_page = true;
        findings.push(finding(
            "blank_page",
            "critical",
            Some("body"),
            "Browser-rendered page has no visible application content.",
            json!({
                "engine": SERVO_ENGINE,
                "body_text_length": probe.get("bodyTextLength").cloned().unwrap_or(Value::Null),
                "body_child_count": probe.get("bodyChildCount").cloned().unwrap_or(Value::Null),
            }),
        ));
    }

    for issue in probe
        .get("invisibleText")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let text = issue
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        visual_health.invisible_text.push(VisualIssue {
            text: text.clone(),
            reason: "computed foreground and background colors match".into(),
        });
        let selector = issue
            .get("selector")
            .and_then(Value::as_str)
            .map(str::to_string);
        findings.push(finding(
            "invisible_text",
            "medium",
            selector.as_deref(),
            "Browser computed styles indicate text is visually invisible.",
            issue,
        ));
    }

    for action in probe
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        if action
            .get("offscreen")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let action_info = action_info_from_probe(&action);
            visual_health
                .offscreen_interactive
                .push(action_info.clone());
            findings.push(finding(
                "offscreen_button",
                "medium",
                Some("button, a, input, select, textarea, [role=button]"),
                "Browser layout places an interactive control outside the viewport.",
                json!({ "action": action_info, "probe": action }),
            ));
        }

        if let Some(blocker) = action.get("blockedBy").and_then(Value::as_str)
            && !blocker.is_empty()
        {
            for report_action in actions.iter_mut() {
                if report_action.label == probe_action_label(&action) {
                    report_action.blocked_by = Some(blocker.into());
                }
            }
            visual_health.overlaps.push(OverlapIssue {
                front: blocker.into(),
                back: probe_action_label(&action),
                severity: "blocking".into(),
            });
            findings.push(finding(
                "modal_blocks_page",
                "high",
                Some("button, a, input, select, textarea, [role=button]"),
                "Browser hit-test geometry shows an overlay blocking a page action.",
                json!({ "blocked_by": blocker, "action": action }),
            ));
        }
    }

    for canvas in probe
        .pointer("/screenshot/canvas_checks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        if canvas
            .get("blank")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let selector = canvas
                .get("selector")
                .and_then(Value::as_str)
                .map(str::to_string);
            findings.push(finding(
                "canvas_chart_blank",
                "medium",
                selector.as_deref(),
                "Screenshot pixel check shows a canvas region with no rendered chart content.",
                canvas,
            ));
        }
    }

    if let Some(no_effect_receipt) = first_no_effect_receipt(&probe) {
        findings.push(finding(
            "button_no_handler",
            "high",
            Some("button, a, input, select, textarea, [role=button]"),
            "Browser click verification found an enabled action with no visible effect.",
            json!({ "receipt": no_effect_receipt }),
        ));
    }

    let summary = if findings.is_empty() {
        "No DEVMAX findings from Servo rendered probe.".into()
    } else {
        format!(
            "Detected {} Servo-backed issue(s); highest severity: {}.",
            findings.len(),
            highest_severity(&findings)
        )
    };
    let recommendations = findings
        .iter()
        .map(|finding| Recommendation {
            kind: "fix".into(),
            message: fix_hint(&finding.kind).into(),
        })
        .collect();

    let mut artifacts = ReportArtifacts::default();
    attach_probe_artifacts(&mut findings, &mut artifacts, &probe);

    Ok(DevmaxReport {
        run_id,
        engine: SERVO_ENGINE.into(),
        page_revision: 1,
        url: url.to_string(),
        title,
        summary,
        visual_health,
        runtime_health,
        actions,
        findings,
        recommendations,
        artifacts,
    })
}

fn analyze_chrome_reference(run_id: String, url: Url, output_dir: &Path) -> Result<DevmaxReport> {
    run_chrome_reference(&url, output_dir)?;
    let manifest_path = output_dir.join("chrome_reference_manifest.json");
    let truth_path = output_dir.join("chrome_truth.json");
    let network_path = output_dir.join("chrome_network.json");
    let screenshot_path = output_dir.join("chrome_page.png");
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let truth: Value = serde_json::from_str(
        &fs::read_to_string(&truth_path)
            .with_context(|| format!("failed to read {}", truth_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", truth_path.display()))?;
    let network: Value = serde_json::from_str(
        &fs::read_to_string(&network_path)
            .with_context(|| format!("failed to read {}", network_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", network_path.display()))?;

    let title = truth
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled")
        .to_string();
    let mut findings = Vec::new();
    let mut visual_health = VisualHealth::default();
    let mut runtime_health = RuntimeHealth::default();
    let mut actions = collect_probe_actions(&truth);
    let body_text_length = truth
        .get("bodyTextLength")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let body_child_count = truth
        .get("bodyChildCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    if body_text_length == 0 && body_child_count == 0 {
        visual_health.blank_page = true;
        findings.push(finding(
            "blank_page",
            "critical",
            Some("body"),
            "Chrome-rendered page has no visible application content.",
            json!({
                "engine": CHROME_ENGINE,
                "body_text_length": body_text_length,
                "body_child_count": body_child_count,
            }),
        ));
    }

    for action in truth
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        if action
            .get("offscreen")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let action_info = action_info_from_probe(&action);
            visual_health
                .offscreen_interactive
                .push(action_info.clone());
            findings.push(finding(
                "offscreen_button",
                "medium",
                Some("button, a, input, select, textarea, [role=button]"),
                "Chrome layout places an interactive control outside the viewport.",
                json!({ "action": action_info, "probe": action }),
            ));
        }

        if let Some(blocker) = action_blocker(&action) {
            for report_action in actions.iter_mut() {
                if report_action.label == probe_action_label(&action) {
                    report_action.blocked_by = Some(blocker.clone());
                }
            }
            visual_health.overlaps.push(OverlapIssue {
                front: blocker.clone(),
                back: probe_action_label(&action),
                severity: "blocking".into(),
            });
            findings.push(finding(
                "modal_blocks_page",
                "high",
                Some("button, a, input, select, textarea, [role=button]"),
                "Chrome hit-test geometry shows an overlay blocking a page action.",
                json!({ "blocked_by": blocker, "action": action }),
            ));
        }
    }

    let sensitive_count = truth
        .get("actions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|action| {
            action
                .pointer("/sensitivity/kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind != "none")
        })
        .count();
    if sensitive_count > 0 {
        findings.push(finding(
            "sensitive_fields_require_user",
            "info",
            Some("form"),
            "Sensitive fields are present in Chrome truth and require user input or confirmation.",
            json!({ "sensitive_fields": sensitive_count }),
        ));
    }

    let blocked_requests = manifest
        .pointer("/block_policy/blocked_requests")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let failed_requests = network
        .get("failed_requests")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unexpected_network_failures = failed_requests.saturating_sub(blocked_requests);
    if unexpected_network_failures > 0 {
        runtime_health.network_errors.push(format!(
            "{unexpected_network_failures} Chrome network request(s) failed outside the block policy"
        ));
        findings.push(finding(
            "network_error",
            "medium",
            Some("network"),
            "Chrome observed network failures outside the configured block policy.",
            json!({
                "failed_requests": failed_requests,
                "blocked_requests": blocked_requests,
            }),
        ));
    }

    let summary = if findings.is_empty() {
        format!(
            "No DEVMAX findings from Chrome reference audit; policy blocked {blocked_requests} resource(s)."
        )
    } else {
        format!(
            "Detected {} Chrome-backed issue(s); highest severity: {}; policy blocked {} resource(s).",
            findings.len(),
            highest_severity(&findings),
            blocked_requests
        )
    };
    let mut recommendations: Vec<Recommendation> = findings
        .iter()
        .map(|finding| Recommendation {
            kind: "fix".into(),
            message: fix_hint(&finding.kind).into(),
        })
        .collect();
    if blocked_requests > 0 {
        recommendations.push(Recommendation {
            kind: "stability".into(),
            message:
                "Keep Chrome block policy enabled for public pages with ad or analytics resources."
                    .into(),
        });
    }

    Ok(DevmaxReport {
        run_id,
        engine: CHROME_ENGINE.into(),
        page_revision: 1,
        url: url.to_string(),
        title,
        summary,
        visual_health,
        runtime_health,
        actions,
        findings,
        recommendations,
        artifacts: ReportArtifacts {
            report: None,
            replay: None,
            browser_screenshot: None,
            finding_crops: Vec::new(),
            action_receipts: Vec::new(),
            chrome_manifest: Some(manifest_path.display().to_string()),
            chrome_screenshot: Some(screenshot_path.display().to_string()),
            chrome_truth: Some(truth_path.display().to_string()),
            chrome_network: Some(network_path.display().to_string()),
        },
    })
}

fn run_chrome_reference(url: &Url, output_dir: &Path) -> Result<()> {
    let script = workspace_root()?
        .join("scripts")
        .join("capture_chrome_reference.sh");
    let output = ProcessCommand::new(&script)
        .arg(url.as_str())
        .arg(output_dir)
        .arg("1280")
        .arg("800")
        .output()
        .with_context(|| format!("failed to run {}", script.display()))?;
    if !output.status.success() {
        bail!(
            "Chrome reference capture failed: status={} stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn collect_probe_actions(probe: &Value) -> Vec<ActionInfo> {
    probe
        .get("actions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(action_info_from_probe)
        .collect()
}

fn action_info_from_probe(action: &Value) -> ActionInfo {
    let label = probe_action_label(action);
    ActionInfo {
        action_id: action
            .get("action_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                if label.eq_ignore_ascii_case("submit") {
                    "act_submit".into()
                } else if label.eq_ignore_ascii_case("export") {
                    "act_export".into()
                } else {
                    format!(
                        "act_{}",
                        action.get("index").and_then(Value::as_u64).unwrap_or(0)
                    )
                }
            }),
        label,
        kind: "click".into(),
        enabled: action
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| {
                !action
                    .get("disabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            }),
        blocked_by: action_blocker(action),
    }
}

fn action_blocker(action: &Value) -> Option<String> {
    action
        .get("blockedBy")
        .or_else(|| action.get("blocked_by"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn probe_action_label(action: &Value) -> String {
    action
        .get("label")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Action")
        .trim()
        .to_string()
}

fn collect_actions(html: &str) -> Vec<ActionInfo> {
    let mut actions = Vec::new();
    if html.contains("<button") {
        actions.push(ActionInfo {
            action_id: if html.contains("type=\"submit\"") {
                "act_submit".into()
            } else if html.contains("data-devmax-no-handler") {
                "act_save".into()
            } else {
                "act_primary".into()
            },
            label: extract_between(html, "<button", "</button>")
                .and_then(|button| button.split('>').next_back().map(clean_text))
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "Button".into()),
            kind: "click".into(),
            enabled: !html.contains("disabled"),
            blocked_by: None,
        });
    }
    actions
}

fn first_no_effect_receipt(probe: &Value) -> Option<Value> {
    probe
        .get("clickVerifications")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|receipt| {
            receipt
                .get("no_effect")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .cloned()
        .or_else(|| {
            probe
                .get("clickVerification")
                .filter(|receipt| {
                    receipt
                        .get("no_effect")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                })
                .cloned()
        })
}

fn attach_probe_artifacts(
    findings: &mut [Finding],
    artifacts: &mut ReportArtifacts,
    probe: &Value,
) {
    artifacts.browser_screenshot = probe
        .pointer("/screenshot/page_png")
        .and_then(Value::as_str)
        .map(str::to_string);
    artifacts.action_receipts = probe
        .get("clickVerifications")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let crops = probe
        .pointer("/screenshot/crops")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for finding in findings.iter_mut() {
        let Some(crop) = crop_for_finding(finding, &crops) else {
            continue;
        };
        let crop_ref = json!({
            "finding_id": finding.finding_id,
            "kind": finding.kind,
            "path": crop.get("path").cloned().unwrap_or(Value::Null),
            "category": crop.get("category").cloned().unwrap_or(Value::Null),
            "crop_rect": crop.get("crop_rect").cloned().unwrap_or(Value::Null),
        });
        set_evidence_field(&mut finding.evidence, "screenshot_crop", crop.clone());
        artifacts.finding_crops.push(crop_ref);
    }
}

fn missing_finding_crops(report: &DevmaxReport) -> usize {
    report
        .findings
        .iter()
        .filter(|finding| finding.evidence.get("screenshot_crop").is_none())
        .count()
}

fn crop_for_finding(finding: &Finding, crops: &[Value]) -> Option<Value> {
    match finding.kind.as_str() {
        "canvas_chart_blank" => {
            crop_by_category_and_selector(crops, "canvas", finding.selector.as_deref())
        }
        "invisible_text" => {
            crop_by_category_and_selector(crops, "invisible_text", finding.selector.as_deref())
        }
        "button_no_handler" | "modal_blocks_page" | "offscreen_button" => {
            crop_by_action_index(crops, &finding.evidence)
                .or_else(|| crop_by_category(crops, "page"))
        }
        _ => crop_by_category(crops, "page"),
    }
}

fn crop_by_category(crops: &[Value], category: &str) -> Option<Value> {
    crops
        .iter()
        .find(|crop| {
            crop.get("category")
                .and_then(Value::as_str)
                .is_some_and(|value| value == category)
        })
        .cloned()
}

fn crop_by_category_and_selector(
    crops: &[Value],
    category: &str,
    selector: Option<&str>,
) -> Option<Value> {
    let selector = selector.unwrap_or("");
    crops
        .iter()
        .find(|crop| {
            crop.get("category")
                .and_then(Value::as_str)
                .is_some_and(|value| value == category)
                && (selector.is_empty()
                    || crop
                        .get("selector")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value == selector))
        })
        .cloned()
        .or_else(|| crop_by_category(crops, category))
}

fn crop_by_action_index(crops: &[Value], evidence: &Value) -> Option<Value> {
    let action_index = evidence
        .pointer("/receipt/action/index")
        .or_else(|| evidence.pointer("/action/probe/index"))
        .or_else(|| evidence.pointer("/probe/index"))
        .or_else(|| evidence.pointer("/action/index"))
        .and_then(Value::as_u64)?;

    crops
        .iter()
        .find(|crop| {
            crop.get("category")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "action")
                && crop
                    .get("index")
                    .and_then(Value::as_u64)
                    .is_some_and(|index| index == action_index)
        })
        .cloned()
}

fn set_evidence_field(evidence: &mut Value, key: &str, value: Value) {
    if !evidence.is_object() {
        let original = std::mem::replace(evidence, Value::Null);
        *evidence = json!({ "value": original });
    }
    if let Some(object) = evidence.as_object_mut() {
        object.insert(key.to_string(), value);
    }
}

fn finding(
    kind: &str,
    severity: &str,
    selector: Option<&str>,
    message: &str,
    evidence: Value,
) -> Finding {
    Finding {
        finding_id: format!("find_{kind}"),
        kind: kind.into(),
        severity: severity.into(),
        selector: selector.map(str::to_string),
        message: message.into(),
        evidence,
    }
}

fn highest_severity(findings: &[Finding]) -> &'static str {
    if findings
        .iter()
        .any(|finding| finding.severity == "critical")
    {
        "critical"
    } else if findings.iter().any(|finding| finding.severity == "high") {
        "high"
    } else if findings.iter().any(|finding| finding.severity == "medium") {
        "medium"
    } else if findings.iter().any(|finding| finding.severity == "info") {
        "info"
    } else {
        "none"
    }
}

fn fix_hint(kind: &str) -> &'static str {
    match kind {
        "blank_page" => "Render a stable first screen before marking the app ready.",
        "console_error" => {
            "Remove startup console errors and add a regression test for the failing path."
        }
        "hydration_error" => "Align server and client markup before hydration.",
        "missing_asset" => "Fix the asset path or include the referenced file in the app bundle.",
        "invisible_text" => "Increase contrast or remove same-color foreground/background styling.",
        "overlapping_elements" => {
            "Move or resize the covering element so the primary action is reachable."
        }
        "offscreen_button" => "Keep interactive controls inside the responsive viewport.",
        "button_no_handler" => "Wire the button to an explicit handler or disable it until ready.",
        "broken_form_validation" => {
            "Show validation errors before accepting an invalid form state."
        }
        "lazy_route_error" => "Verify the lazy route chunk loads and handles failures visibly.",
        "scroll_container_hidden_submit" => {
            "Keep submit controls reachable outside clipped scroll regions."
        }
        "responsive_mobile_break" => "Audit mobile constraints and remove horizontal overflow.",
        "modal_blocks_page" => {
            "Dismiss or scope the modal overlay before interacting with page controls."
        }
        "canvas_chart_blank" => "Render a fallback or verify the chart draws non-empty pixels.",
        "css_zindex_overlay_bug" => "Fix stacking context and pointer-events for overlays.",
        "wrong_success_state" => "Tie visible success state to the verified request result.",
        "stuck_loading_spinner" => {
            "Resolve the loading state or show a deterministic error/retry path."
        }
        "disabled_primary_action" => {
            "Explain why the primary action is disabled and provide a reachable recovery path."
        }
        "duplicate_id_controls" => {
            "Use unique element IDs so labels, selectors, and action maps stay stable."
        }
        "wrong_route_404" => {
            "Fix the route target or show a handled not-found state before navigation."
        }
        "network_error" => "Fix failing network resources or add an explicit, reviewed block rule.",
        "sensitive_fields_require_user" => {
            "Leave sensitive fields to the user and expose only completion status to the agent."
        }
        _ => "Inspect the finding evidence and add a deterministic fix.",
    }
}

fn expected_from_html(html: &str) -> Option<String> {
    extract_meta_content(html, "devmax:expected")
}

fn extract_meta_content(html: &str, name: &str) -> Option<String> {
    let marker = format!("name=\"{name}\"");
    let start = html.find(&marker)?;
    let rest = &html[start..];
    let content = "content=\"";
    let value_start = rest.find(content)? + content.len();
    let rest = &rest[value_start..];
    let value_end = rest.find('"')?;
    Some(rest[..value_end].to_string())
}

fn extract_between(text: &str, start: &str, end: &str) -> Option<String> {
    let start_index = text.find(start)? + start.len();
    let tail = &text[start_index..];
    let end_index = tail.find(end)?;
    Some(tail[..end_index].trim().to_string())
}

fn is_blank_page(html: &str) -> bool {
    if html.contains("data-devmax-blank-page") {
        return true;
    }
    let body = extract_between(html, "<body>", "</body>").unwrap_or_default();
    clean_text(&strip_tags(&body)).is_empty()
}

fn has_missing_asset(html: &str, fixture_dir: Option<&Path>) -> bool {
    if html.contains("data-devmax-missing-asset") {
        return true;
    }
    for attr in ["src=\"", "href=\""] {
        let mut rest = html;
        while let Some(index) = rest.find(attr) {
            rest = &rest[index + attr.len()..];
            let Some(end) = rest.find('"') else {
                break;
            };
            let asset = &rest[..end];
            if asset.starts_with("http")
                || asset.starts_with('#')
                || asset.starts_with("data:")
                || asset.starts_with('/')
            {
                rest = &rest[end..];
                continue;
            }
            if let Some(fixture_dir) = fixture_dir
                && !fixture_dir.join(asset).exists()
            {
                return true;
            }
            if asset.contains("missing") {
                return true;
            }
            rest = &rest[end..];
        }
    }
    false
}

fn has_duplicate_id(html: &str) -> bool {
    let mut ids = Vec::new();
    let mut rest = html;
    while let Some(index) = rest.find("id=\"") {
        rest = &rest[index + 4..];
        let Some(end) = rest.find('"') else {
            break;
        };
        let id = &rest[..end];
        if ids.iter().any(|existing| existing == &id) {
            return true;
        }
        ids.push(id.to_string());
        rest = &rest[end..];
    }
    false
}

fn strip_tags(text: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn clean_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn write_report(path: &Path, value: &impl Serialize) -> Result<()> {
    let pretty = serde_json::to_string_pretty(value)?;
    fs::write(path, pretty).with_context(|| format!("failed to write {}", path.display()))
}

fn write_replay(
    path: &Path,
    run_id: &str,
    url: &Url,
    expected: &str,
    report: &DevmaxReport,
) -> Result<()> {
    let mut file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    writeln!(
        file,
        "{}",
        json!({
            "kind": "devmax_run_started",
            "run_id": run_id,
            "engine": report.engine,
            "url": url.to_string(),
            "expected": expected,
        })
    )?;
    for finding in &report.findings {
        writeln!(
            file,
            "{}",
            json!({
                "kind": "devmax_finding",
                "run_id": run_id,
                "finding": finding,
            })
        )?;
    }
    for receipt in &report.artifacts.action_receipts {
        writeln!(
            file,
            "{}",
            json!({
                "kind": "devmax_action_receipt",
                "run_id": run_id,
                "receipt": receipt,
            })
        )?;
    }
    writeln!(
        file,
        "{}",
        json!({
            "kind": "devmax_run_finished",
            "run_id": run_id,
            "findings": report.findings.len(),
        })
    )?;
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

fn fetch_http_body(url: &Url) -> Result<String> {
    if url.scheme() != "http" {
        bail!("only http:// URLs are supported by DEVMAX static audit v0");
    }
    let host = url.host_str().context("URL has no host")?;
    let port = url.port_or_known_default().context("URL has no port")?;
    let mut stream = TcpStream::connect((host, port))
        .with_context(|| format!("failed to connect to {host}:{port}"))?;
    let path = if let Some(query) = url.query() {
        format!("{}?{query}", url.path())
    } else {
        url.path().to_string()
    };
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n"
    )?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .context("failed to read HTTP response")?;
    let response = String::from_utf8_lossy(&response);
    let Some((headers, body)) = response.split_once("\r\n\r\n") else {
        bail!("invalid HTTP response");
    };
    if !headers.starts_with("HTTP/1.1 200") && !headers.starts_with("HTTP/1.0 200") {
        bail!(
            "HTTP request failed: {}",
            headers.lines().next().unwrap_or("")
        );
    }
    Ok(body.to_string())
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn workspace_root() -> Result<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .context("failed to resolve workspace root")
}

fn unix_ms() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_millis())
}
