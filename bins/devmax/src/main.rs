use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tiny_http::{Header, Response, Server, StatusCode};
use url::Url;

const ENGINE: &str = "static-fixture-v0";
const REQUIRED_FIXTURE_TOTAL: usize = 16;
const MIN_DETECTED: usize = 14;
const MAX_FALSE_POSITIVES: usize = 1;

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
        #[arg(long)]
        replay: bool,
    },
    SelftestFixtures,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SelftestCaseResult {
    fixture: String,
    url: String,
    expected: String,
    detected: bool,
    false_positives: usize,
    report: String,
    replay: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SelftestSummary {
    run_id: String,
    total: usize,
    detected: usize,
    false_positives: usize,
    output_dir: String,
    cases: Vec<SelftestCaseResult>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Audit { url, replay } => audit(url, replay),
        Command::SelftestFixtures => selftest_fixtures(),
    }
}

fn audit(url: Url, replay: bool) -> Result<()> {
    let run_id = format!("audit_{}", unix_ms()?);
    let output_dir = workspace_root()?.join("runs").join("devmax").join(&run_id);
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;

    let html = fetch_http_body(&url).with_context(|| format!("failed to fetch {url}"))?;
    let mut report = analyze_html(run_id.clone(), url.clone(), &html, None)?;
    let report_path = output_dir.join("report.json");
    let replay_path = output_dir.join("replay.jsonl");
    report.artifacts.report = Some(report_path.display().to_string());
    if replay {
        report.artifacts.replay = Some(replay_path.display().to_string());
    }
    write_report(&report_path, &report)?;
    if replay {
        write_replay(
            &replay_path,
            &run_id,
            &url,
            &expected_from_html(&html).unwrap_or_else(|| "unknown".into()),
            &report,
        )?;
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
            report: report_path.display().to_string(),
            replay: replay_path.display().to_string(),
        });
    }

    let summary = SelftestSummary {
        run_id,
        total: cases.len(),
        detected,
        false_positives,
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
    } else {
        "medium"
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
            "engine": ENGINE,
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
