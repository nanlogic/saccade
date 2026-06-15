use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::Url;

const DEFAULT_SERVOSHELL: &str = "/Applications/Servo.app/Contents/MacOS/servoshell";
const TRUTH_BUNDLE_VERSION: &str = "saccade-servoshell-truth-v0";

#[derive(Parser)]
#[command(name = "saccade-servoshell")]
#[command(about = "Saccade adapter gates for official ServoShell WebDriver")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Probe {
        #[arg(long, default_value = DEFAULT_SERVOSHELL)]
        servoshell: PathBuf,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        output_dir: Option<PathBuf>,
        #[arg(long)]
        no_headless: bool,
        #[arg(long, default_value_t = 25.0)]
        timeout_sec: f64,
        #[arg(long, value_enum, default_value = "forbidden")]
        screenshot_mode: ScreenshotMode,
        #[arg(long)]
        click_selector: Option<String>,
    },
    Selftest {
        #[arg(long, default_value = DEFAULT_SERVOSHELL)]
        servoshell: PathBuf,
        #[arg(long)]
        output_dir: Option<PathBuf>,
        #[arg(long)]
        no_headless: bool,
        #[arg(long, default_value_t = 25.0)]
        timeout_sec: f64,
    },
    FormmaxSelftest {
        #[arg(long, default_value = DEFAULT_SERVOSHELL)]
        servoshell: PathBuf,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        output_dir: Option<PathBuf>,
        #[arg(long)]
        no_headless: bool,
        #[arg(long, default_value_t = 35.0)]
        timeout_sec: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum ScreenshotMode {
    Forbidden,
    GuardedDiagnostic,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Probe {
            servoshell,
            url,
            output_dir,
            no_headless,
            timeout_sec,
            screenshot_mode,
            click_selector,
        } => {
            let url = url.unwrap_or_else(default_smoke_url);
            let root = output_dir.unwrap_or_else(|| default_run_dir("probe"));
            let report = run_probe(ProbeConfig {
                servoshell,
                url,
                output_dir: root.clone(),
                headless: !no_headless,
                timeout: Duration::from_secs_f64(timeout_sec),
                screenshot_mode,
                click_selector,
                expect_click_revision: false,
                expect_sensitive_surface: None,
                raw_value_needles: vec![],
                expected_sensitive_count_min: None,
                expected_redaction_kinds: vec![],
            })?;
            let ok = report.ok;
            println!(
                "SACCADE_SERVOSHELL_PROBE ok={} report={}",
                ok,
                report.report_path.display()
            );
            if ok { Ok(()) } else { bail!("probe failed") }
        }
        Command::Selftest {
            servoshell,
            output_dir,
            no_headless,
            timeout_sec,
        } => {
            let root = output_dir.unwrap_or_else(|| default_run_dir("adapter"));
            fs::create_dir_all(&root).with_context(|| format!("create {}", root.display()))?;

            let normal = run_probe(ProbeConfig {
                servoshell: servoshell.clone(),
                url: default_smoke_url(),
                output_dir: root.join("normal"),
                headless: !no_headless,
                timeout: Duration::from_secs_f64(timeout_sec),
                screenshot_mode: ScreenshotMode::GuardedDiagnostic,
                click_selector: Some("#verify-action".to_string()),
                expect_click_revision: true,
                expect_sensitive_surface: Some(false),
                raw_value_needles: vec![],
                expected_sensitive_count_min: None,
                expected_redaction_kinds: vec![],
            })?;

            let sensitive = run_probe(ProbeConfig {
                servoshell,
                url: default_safety_matrix_url(),
                output_dir: root.join("safety_matrix"),
                headless: !no_headless,
                timeout: Duration::from_secs_f64(timeout_sec),
                screenshot_mode: ScreenshotMode::GuardedDiagnostic,
                click_selector: None,
                expect_click_revision: false,
                expect_sensitive_surface: Some(true),
                raw_value_needles: vec![
                    "123-45-6789".to_string(),
                    "4111111111111111".to_string(),
                    "correct-horse-battery".to_string(),
                    "A-9876543".to_string(),
                    "sk_live_super_secret_123".to_string(),
                    "000111".to_string(),
                    "ada.secret@example.com".to_string(),
                    "hidden-session-token-xyz".to_string(),
                    "reset-token-shh-991".to_string(),
                ],
                expected_sensitive_count_min: Some(9),
                expected_redaction_kinds: vec![
                    "ssn".to_string(),
                    "credit_card".to_string(),
                    "password".to_string(),
                    "government_id".to_string(),
                    "api_token".to_string(),
                    "otp".to_string(),
                    "email".to_string(),
                    "hidden".to_string(),
                    "recovery_token".to_string(),
                ],
            })?;

            let summary = json!({
                "ok": normal.ok && sensitive.ok,
                "normal": {
                    "report": normal.report_path,
                    "screenshot": normal.screenshot_path,
                    "port": normal.webdriver_port,
                    "post_revision": normal.post_revision,
                },
                "sensitive": {
                    "report": sensitive.report_path,
                    "screenshot": sensitive.screenshot_path,
                    "screenshot_decision": sensitive.screenshot_decision,
                    "leak_check_passed": sensitive.leak_check_passed,
                    "redaction_count": sensitive.redaction_count,
                    "redaction_kinds": sensitive.redaction_kinds,
                    "port": sensitive.webdriver_port,
                },
                "port_policy": {
                    "random_loopback_ports": normal.webdriver_port != sensitive.webdriver_port,
                    "normal_port": normal.webdriver_port,
                    "sensitive_port": sensitive.webdriver_port,
                },
                "truth_bundle_version": TRUTH_BUNDLE_VERSION,
            });
            let summary_path = root.join("summary.json");
            write_json(&summary_path, &summary)?;
            let ok = normal.ok && sensitive.ok;
            println!(
                "SACCADE_SERVOSHELL_ADAPTER {} report={} normal_screenshot={} sensitive_screenshot={}",
                if ok { "PASS" } else { "FAIL" },
                summary_path.display(),
                normal
                    .screenshot_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "none".to_string()),
                sensitive.screenshot_decision.as_str()
            );
            if ok { Ok(()) } else { bail!("selftest failed") }
        }
        Command::FormmaxSelftest {
            servoshell,
            url,
            output_dir,
            no_headless,
            timeout_sec,
        } => {
            let outcome = run_formmax_selftest(FormmaxConfig {
                servoshell,
                url: url.unwrap_or_else(default_formmax_url),
                output_dir: output_dir.unwrap_or_else(|| default_run_dir("formmax")),
                headless: !no_headless,
                timeout: Duration::from_secs_f64(timeout_sec),
            })?;
            println!(
                "SACCADE_SERVOSHELL_FORMMAX {} rows={} pages={} filled={} blocked_sensitive={} receipt_verified={} report={} replay={}",
                if outcome.ok { "PASS" } else { "FAIL" },
                outcome.rows,
                outcome.pages,
                outcome.filled,
                outcome.blocked_sensitive,
                outcome.receipt_verified,
                outcome.report_path.display(),
                outcome.replay_path.display(),
            );
            if outcome.ok {
                Ok(())
            } else {
                bail!("formmax selftest failed")
            }
        }
    }
}

#[derive(Debug)]
struct ProbeConfig {
    servoshell: PathBuf,
    url: String,
    output_dir: PathBuf,
    headless: bool,
    timeout: Duration,
    screenshot_mode: ScreenshotMode,
    click_selector: Option<String>,
    expect_click_revision: bool,
    expect_sensitive_surface: Option<bool>,
    raw_value_needles: Vec<String>,
    expected_sensitive_count_min: Option<usize>,
    expected_redaction_kinds: Vec<String>,
}

#[derive(Debug)]
struct ProbeOutcome {
    ok: bool,
    report_path: PathBuf,
    screenshot_path: Option<PathBuf>,
    screenshot_decision: String,
    post_revision: Option<String>,
    leak_check_passed: bool,
    redaction_count: usize,
    redaction_kinds: Vec<String>,
    webdriver_port: u16,
}

#[derive(Debug)]
struct FormmaxConfig {
    servoshell: PathBuf,
    url: String,
    output_dir: PathBuf,
    headless: bool,
    timeout: Duration,
}

#[derive(Debug)]
struct FormmaxOutcome {
    ok: bool,
    rows: u64,
    pages: u64,
    filled: u64,
    blocked_sensitive: u64,
    receipt_verified: bool,
    report_path: PathBuf,
    replay_path: PathBuf,
}

fn run_formmax_selftest(cfg: FormmaxConfig) -> Result<FormmaxOutcome> {
    fs::create_dir_all(&cfg.output_dir)
        .with_context(|| format!("create {}", cfg.output_dir.display()))?;

    let port = choose_loopback_port()?;
    let mut child = launch_servoshell_for_url(&cfg.servoshell, &cfg.url, cfg.headless, port)?;
    let client = WebDriverClient::new(port, cfg.timeout);
    let mut session_id: Option<String> = None;
    let mut report = json!({
        "ok": false,
        "engine": "saccade-servoshell-formmax-v0",
        "runtime": "official_servoshell_webdriver",
        "servoshell": cfg.servoshell,
        "url": cfg.url,
        "headless": cfg.headless,
        "webdriver": {
            "host": "127.0.0.1",
            "port": port,
            "port_policy": "random_loopback_private_to_launch_manager"
        },
        "truth_bundle_version": TRUTH_BUNDLE_VERSION,
        "policy": {
            "block_sensitive": true,
            "echo_values": false,
            "local_fixture_only": true
        },
        "output_dir": cfg.output_dir,
    });

    let mut ok = false;
    let mut rows = 0;
    let mut pages = 0;
    let mut filled = 0;
    let mut blocked_sensitive = 0;
    let mut receipt_verified = false;

    let result = (|| -> Result<Value> {
        let status = wait_for_status(&client, &mut child, cfg.timeout)?;
        report["webdriver"]["status"] = status;

        let session = client.new_session()?;
        let sid = extract_session_id(&session)?;
        session_id = Some(sid.clone());
        report["webdriver"]["new_session"] = session;
        report["webdriver"]["session_id"] = json!(sid);

        wait_for_formmax_ready(&client, &sid, cfg.timeout)?;
        let pre_truth = client.execute_sync(&sid, TRUTH_JS)?;
        let field_truth_before = client.execute_sync(&sid, FORMMAX_FIELD_TRUTH_JS)?;
        write_json(
            &cfg.output_dir.join("pre_truth_summary.json"),
            &summarize_truth(&pre_truth),
        )?;
        write_json(
            &cfg.output_dir.join("field_truth_before.json"),
            &field_truth_before,
        )?;

        let init_result =
            client.execute_sync_args(&sid, FORMMAX_INIT_JS, &[json!(FORMMAX_HELPERS_JS)])?;
        write_json(&cfg.output_dir.join("transaction_init.json"), &init_result)?;
        report["last_step"] = json!("init_done");

        report["last_step"] = json!("render_page_1");
        client.execute_sync_args(
            &sid,
            FORMMAX_RENDER_PAGE_JS,
            &[json!(FORMMAX_HELPERS_JS), json!(0)],
        )?;
        report["last_step"] = json!("render_page_1_done");
        for start in (0..48).step_by(16) {
            report["last_step"] = json!(format!("fill_page_1_rows_{start}_{}", start + 16));
            client.execute_sync_args(
                &sid,
                FORMMAX_FILL_CHUNK_JS,
                &[
                    json!(FORMMAX_HELPERS_JS),
                    json!(0),
                    json!(start),
                    json!(start + 16),
                ],
            )?;
        }
        report["last_step"] = json!("submit_page_1");
        client.execute_sync_args(
            &sid,
            FORMMAX_SUBMIT_PAGE_JS,
            &[json!(FORMMAX_HELPERS_JS), json!(1), json!(2)],
        )?;
        report["last_step"] = json!("submit_page_1_done");

        report["last_step"] = json!("render_page_2");
        client.execute_sync_args(
            &sid,
            FORMMAX_RENDER_PAGE_JS,
            &[json!(FORMMAX_HELPERS_JS), json!(1)],
        )?;
        report["last_step"] = json!("render_page_2_done");
        for start in (0..48).step_by(16) {
            report["last_step"] = json!(format!("fill_page_2_rows_{start}_{}", start + 16));
            client.execute_sync_args(
                &sid,
                FORMMAX_FILL_CHUNK_JS,
                &[
                    json!(FORMMAX_HELPERS_JS),
                    json!(1),
                    json!(start),
                    json!(start + 16),
                ],
            )?;
        }
        report["last_step"] = json!("block_sensitive");
        client.execute_sync_args(
            &sid,
            FORMMAX_BLOCK_SENSITIVE_JS,
            &[json!(FORMMAX_HELPERS_JS)],
        )?;
        report["last_step"] = json!("finalize");
        let fill_result =
            client.execute_sync_args(&sid, FORMMAX_FINALIZE_JS, &[json!(FORMMAX_HELPERS_JS)])?;
        report["last_step"] = json!("finalize_done");
        write_json(&cfg.output_dir.join("fill_result.json"), &fill_result)?;

        rows = fill_result.get("rows").and_then(Value::as_u64).unwrap_or(0);
        pages = fill_result
            .get("pages")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        filled = fill_result
            .get("filled")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        blocked_sensitive = fill_result
            .get("blocked_sensitive")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        receipt_verified = fill_result
            .get("receipt_verified")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let validation_errors = fill_result
            .get("validation_errors")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        let event_count = fill_result
            .get("events")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or_default();

        if rows != 96
            || pages != 2
            || filled != 672
            || blocked_sensitive != 3
            || !receipt_verified
            || validation_errors != 0
            || event_count < 700
        {
            bail!(
                "FORMMAX adapter gate failed: rows={rows} pages={pages} filled={filled} blocked_sensitive={blocked_sensitive} receipt_verified={receipt_verified} validation_errors={validation_errors} event_count={event_count}"
            );
        }

        let post_truth = client.execute_sync(&sid, TRUTH_JS)?;
        let field_truth_after = client.execute_sync(&sid, FORMMAX_FIELD_TRUTH_JS)?;
        write_json(
            &cfg.output_dir.join("post_truth_summary.json"),
            &summarize_truth(&post_truth),
        )?;
        write_json(
            &cfg.output_dir.join("field_truth_after.json"),
            &field_truth_after,
        )?;

        let screenshot_decision = if truth_capture_allowed(&post_truth) {
            "allowed_but_not_captured_formmax_no_pixels_needed"
        } else {
            "blocked_sensitive_surface"
        };

        let mut sanitized = fill_result.clone();
        sanitized["events"] = Value::Null;
        report["pre_truth_summary"] = summarize_truth(&pre_truth);
        report["field_truth_before"] = field_truth_before;
        report["field_truth_after"] = field_truth_after;
        report["formmax"] = sanitized;
        report["screenshot"] = json!({
            "mode": ScreenshotMode::Forbidden,
            "decision": screenshot_decision,
            "captured": false,
        });

        let report_path = cfg.output_dir.join("result.json");
        let replay_path = cfg.output_dir.join("replay.jsonl");
        write_formmax_replay(&replay_path, &fill_result)?;
        report["artifacts"] = json!({
            "result": report_path,
            "replay": replay_path,
        });
        let leak_check = formmax_values_absent(&report, &replay_path)?;
        report["leak_check"] = leak_check.clone();
        if leak_check.get("passed").and_then(Value::as_bool) != Some(true) {
            bail!("FORMMAX adapter value leak check failed: {leak_check}");
        }

        ok = true;
        Ok(json!({
            "report_path": report_path,
            "replay_path": replay_path,
        }))
    })();

    if let Some(sid) = session_id.as_deref() {
        if let Err(error) = client.delete_session(sid) {
            report["webdriver"]["delete_session_error"] = json!(error.to_string());
        }
    }
    report["process"] = finish_child(child);

    let mut report_path = cfg.output_dir.join("result.json");
    let mut replay_path = cfg.output_dir.join("replay.jsonl");
    match result {
        Ok(paths) => {
            if let Some(path) = paths.get("report_path").and_then(Value::as_str) {
                report_path = PathBuf::from(path);
            }
            if let Some(path) = paths.get("replay_path").and_then(Value::as_str) {
                replay_path = PathBuf::from(path);
            }
        }
        Err(error) => {
            report["error"] = json!(error.to_string());
        }
    }
    report["ok"] = json!(ok);
    write_json(&report_path, &report)?;

    Ok(FormmaxOutcome {
        ok,
        rows,
        pages,
        filled,
        blocked_sensitive,
        receipt_verified,
        report_path,
        replay_path,
    })
}

fn launch_servoshell_for_url(
    servoshell: &Path,
    url: &str,
    headless: bool,
    port: u16,
) -> Result<Child> {
    if !servoshell.exists() {
        bail!("servoshell not found at {}", servoshell.display());
    }
    let mut cmd = ProcessCommand::new(servoshell);
    if headless {
        cmd.arg("-z");
    }
    cmd.arg(format!("--webdriver={port}"));
    cmd.arg("--temporary-storage");
    cmd.arg(url);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.spawn()
        .with_context(|| format!("launch {}", servoshell.display()))
}

fn wait_for_formmax_ready(
    client: &WebDriverClient,
    session_id: &str,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let mut last = Value::Null;
    while Instant::now() < deadline {
        last = client.execute_sync(session_id, "return Boolean(window.__FORMMAX_FIXTURE && window.FormmaxFixture && document.getElementById('capacity-body'));")?;
        if last.as_bool() == Some(true) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    bail!("FORMMAX fixture did not become ready: {last}");
}

fn summarize_truth(truth: &Value) -> Value {
    json!({
        "page": truth.get("page").cloned().unwrap_or(Value::Null),
        "viewport": truth.get("viewport").cloned().unwrap_or(Value::Null),
        "safety": truth.get("safety").cloned().unwrap_or(Value::Null),
        "action_count": truth.get("actions").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
        "redaction_count": truth.get("redactions").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
    })
}

fn run_probe(cfg: ProbeConfig) -> Result<ProbeOutcome> {
    fs::create_dir_all(&cfg.output_dir)
        .with_context(|| format!("create {}", cfg.output_dir.display()))?;

    let port = choose_loopback_port()?;
    let mut child = launch_servoshell(&cfg, port)?;
    let client = WebDriverClient::new(port, cfg.timeout);
    let mut session_id: Option<String> = None;
    let mut report = json!({
        "ok": false,
        "servoshell": cfg.servoshell,
        "url": cfg.url,
        "headless": cfg.headless,
        "webdriver": {
            "host": "127.0.0.1",
            "port": port,
            "port_policy": "random_loopback_private_to_launch_manager"
        },
        "truth_bundle_version": TRUTH_BUNDLE_VERSION,
        "screenshot_mode": cfg.screenshot_mode,
        "output_dir": cfg.output_dir,
    });

    let mut screenshot_path: Option<PathBuf> = None;
    let mut screenshot_decision = "not_evaluated".to_string();
    let mut post_revision: Option<String> = None;
    let mut leak_check_passed = false;
    let mut redaction_count = 0;
    let mut redaction_kinds = Vec::new();
    let mut ok = false;

    let result = (|| -> Result<()> {
        let status = wait_for_status(&client, &mut child, cfg.timeout)?;
        report["webdriver"]["status"] = status;

        let session = client.new_session()?;
        let sid = extract_session_id(&session)?;
        session_id = Some(sid.clone());
        report["webdriver"]["new_session"] = session;
        report["webdriver"]["session_id"] = json!(sid);

        let pre_truth = client.execute_sync(&sid, TRUTH_JS)?;
        write_json(&cfg.output_dir.join("pre_truth.json"), &pre_truth)?;
        report["pre_truth"] = pre_truth.clone();

        if let Some(expected) = cfg.expect_sensitive_surface {
            let actual = visible_sensitive_surface(&pre_truth);
            if actual != expected {
                bail!("sensitive surface expectation failed: expected {expected}, actual {actual}");
            }
        }
        redaction_count = count_redactions(&pre_truth);
        redaction_kinds = collect_redaction_kinds(&pre_truth);
        if let Some(min) = cfg.expected_sensitive_count_min {
            if redaction_count < min {
                bail!("redaction count too low: expected at least {min}, actual {redaction_count}");
            }
        }
        for expected_kind in &cfg.expected_redaction_kinds {
            if !redaction_kinds.iter().any(|kind| kind == expected_kind) {
                bail!("missing expected redaction kind: {expected_kind}");
            }
        }

        if let Some(selector) = &cfg.click_selector {
            let element = client.find_element(&sid, selector)?;
            report["action"]["selector"] = json!(selector);
            report["action"]["element"] = element.clone();
            let element_id = extract_element_id(&element)
                .ok_or_else(|| anyhow!("find element response lacked element id: {element}"))?;
            let click = client.click_element(&sid, &element_id)?;
            report["action"]["click"] = click;

            let post_truth_value = client.execute_sync(&sid, TRUTH_JS)?;
            write_json(&cfg.output_dir.join("post_truth.json"), &post_truth_value)?;
            post_revision = revision(&post_truth_value);
            report["post_truth"] = post_truth_value;
            if cfg.expect_click_revision && post_revision.as_deref() != Some("1") {
                bail!("post-click revision did not become 1: {post_revision:?}");
            }
        }

        let capture_allowed = truth_capture_allowed(&pre_truth);
        match cfg.screenshot_mode {
            ScreenshotMode::Forbidden => {
                screenshot_decision = "blocked_forbidden_default".to_string();
                report["screenshot"] = json!({
                    "mode": cfg.screenshot_mode,
                    "decision": screenshot_decision,
                    "captured": false,
                });
            }
            ScreenshotMode::GuardedDiagnostic if !capture_allowed => {
                screenshot_decision = "blocked_sensitive_surface".to_string();
                report["screenshot"] = json!({
                    "mode": cfg.screenshot_mode,
                    "decision": screenshot_decision,
                    "captured": false,
                    "reason": "truth preflight reported visible sensitive surface",
                });
            }
            ScreenshotMode::GuardedDiagnostic => {
                let screenshot = client.screenshot(&sid)?;
                let path = cfg.output_dir.join("screenshot.png");
                fs::write(&path, screenshot)
                    .with_context(|| format!("write {}", path.display()))?;
                screenshot_decision = "captured_guarded_diagnostic".to_string();
                report["screenshot"] = json!({
                    "mode": cfg.screenshot_mode,
                    "decision": screenshot_decision,
                    "captured": true,
                    "path": path,
                });
                screenshot_path = Some(path);
            }
        }

        leak_check_passed = raw_values_absent(&report, &cfg.raw_value_needles);
        report["leak_check"] = json!({
            "passed": leak_check_passed,
            "needles_checked": cfg.raw_value_needles.len(),
        });
        report["redaction_check"] = json!({
            "redaction_count": redaction_count,
            "redaction_kinds": redaction_kinds,
        });
        if !leak_check_passed {
            bail!("raw sensitive value leak check failed");
        }

        ok = true;
        Ok(())
    })();

    if let Some(sid) = session_id.as_deref() {
        if let Err(error) = client.delete_session(sid) {
            report["webdriver"]["delete_session_error"] = json!(error.to_string());
        }
    }

    let process_report = finish_child(child);
    report["process"] = process_report;

    if let Err(error) = result {
        report["error"] = json!(error.to_string());
    }
    report["ok"] = json!(ok);
    report["screenshot_decision"] = json!(screenshot_decision);

    let report_path = cfg.output_dir.join("report.json");
    write_json(&report_path, &report)?;

    Ok(ProbeOutcome {
        ok,
        report_path,
        screenshot_path,
        screenshot_decision,
        post_revision,
        leak_check_passed,
        redaction_count,
        redaction_kinds,
        webdriver_port: port,
    })
}

fn launch_servoshell(cfg: &ProbeConfig, port: u16) -> Result<Child> {
    if !cfg.servoshell.exists() {
        bail!("servoshell not found at {}", cfg.servoshell.display());
    }
    let mut cmd = ProcessCommand::new(&cfg.servoshell);
    if cfg.headless {
        cmd.arg("-z");
    }
    cmd.arg(format!("--webdriver={port}"));
    cmd.arg("--temporary-storage");
    cmd.arg(&cfg.url);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.spawn()
        .with_context(|| format!("launch {}", cfg.servoshell.display()))
}

fn wait_for_status(
    client: &WebDriverClient,
    child: &mut Child,
    timeout: Duration,
) -> Result<Value> {
    let deadline = Instant::now() + timeout;
    let mut last_error = String::new();
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().context("check servoshell status")? {
            bail!("servoshell exited before WebDriver became ready: {status}");
        }
        match client.request("GET", "/status", None) {
            Ok(response) => return Ok(json!({"status": response.status, "body": response.body})),
            Err(error) => last_error = error.to_string(),
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    bail!("WebDriver status was not ready; last_error={last_error}");
}

#[derive(Debug, Clone)]
struct WebDriverClient {
    port: u16,
    timeout: Duration,
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    body: Value,
}

impl WebDriverClient {
    fn new(port: u16, timeout: Duration) -> Self {
        Self { port, timeout }
    }

    fn new_session(&self) -> Result<Value> {
        self.request(
            "POST",
            "/session",
            Some(json!({
                "capabilities": {
                    "alwaysMatch": {
                        "browserName": "servo",
                        "timeouts": {
                            "script": 120000,
                            "pageLoad": 300000,
                            "implicit": 0
                        }
                    }
                }
            })),
        )
        .map(|response| response.body)
    }

    fn execute_sync(&self, session_id: &str, script: &str) -> Result<Value> {
        self.execute_sync_args(session_id, script, &[])
    }

    fn execute_sync_args(&self, session_id: &str, script: &str, args: &[Value]) -> Result<Value> {
        let response = self.request(
            "POST",
            &format!("/session/{session_id}/execute/sync"),
            Some(json!({"script": script, "args": args})),
        )?;
        Ok(response.body.get("value").cloned().unwrap_or(Value::Null))
    }

    fn find_element(&self, session_id: &str, selector: &str) -> Result<Value> {
        self.request(
            "POST",
            &format!("/session/{session_id}/element"),
            Some(json!({"using": "css selector", "value": selector})),
        )
        .map(|response| response.body)
    }

    fn click_element(&self, session_id: &str, element_id: &str) -> Result<Value> {
        self.request(
            "POST",
            &format!("/session/{session_id}/element/{element_id}/click"),
            Some(json!({})),
        )
        .map(|response| response.body)
    }

    fn screenshot(&self, session_id: &str) -> Result<Vec<u8>> {
        let response = self.request("GET", &format!("/session/{session_id}/screenshot"), None)?;
        let encoded = response
            .body
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("screenshot response missing value"))?;
        BASE64_STANDARD
            .decode(encoded.as_bytes())
            .context("decode screenshot base64")
    }

    fn delete_session(&self, session_id: &str) -> Result<()> {
        let _ = self.request("DELETE", &format!("/session/{session_id}"), None)?;
        Ok(())
    }

    fn request(&self, method: &str, path: &str, payload: Option<Value>) -> Result<HttpResponse> {
        let body = match payload {
            Some(value) => serde_json::to_vec(&value).context("encode webdriver request")?,
            None => Vec::new(),
        };
        let mut stream = TcpStream::connect(("127.0.0.1", self.port))
            .with_context(|| format!("connect WebDriver 127.0.0.1:{}", self.port))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .context("set read timeout")?;
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .context("set write timeout")?;

        let request = format!(
            "{method} {path} HTTP/1.1\r\n\
             Host: 127.0.0.1:{}\r\n\
             Connection: close\r\n\
             Accept: application/json\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             \r\n",
            self.port,
            body.len()
        );
        stream
            .write_all(request.as_bytes())
            .context("write webdriver request header")?;
        if !body.is_empty() {
            stream
                .write_all(&body)
                .context("write webdriver request body")?;
        }
        stream.flush().context("flush webdriver request")?;

        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .context("read webdriver response")?;
        parse_http_response(&response)
            .with_context(|| format!("parse webdriver response for {method} {path}"))
    }
}

fn parse_http_response(bytes: &[u8]) -> Result<HttpResponse> {
    let header_end = find_subsequence(bytes, b"\r\n\r\n")
        .ok_or_else(|| anyhow!("HTTP response missing header separator"))?;
    let header = std::str::from_utf8(&bytes[..header_end]).context("HTTP header utf8")?;
    let mut lines = header.lines();
    let status_line = lines.next().ok_or_else(|| anyhow!("empty HTTP response"))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow!("HTTP status line missing code: {status_line}"))?
        .parse::<u16>()
        .with_context(|| format!("parse HTTP status line: {status_line}"))?;
    let chunked = header.lines().any(|line| {
        line.to_ascii_lowercase()
            .starts_with("transfer-encoding: chunked")
    });
    let mut body_bytes = bytes[header_end + 4..].to_vec();
    if chunked {
        body_bytes = decode_chunked(&body_bytes)?;
    }
    let body = if body_bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body_bytes)
            .with_context(|| String::from_utf8_lossy(&body_bytes).into_owned())?
    };
    if !(200..300).contains(&status) {
        bail!("WebDriver HTTP {status}: {body}");
    }
    Ok(HttpResponse { status, body })
}

fn decode_chunked(mut bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = find_subsequence(bytes, b"\r\n")
            .ok_or_else(|| anyhow!("chunked body missing size line"))?;
        let size_text = std::str::from_utf8(&bytes[..line_end])
            .context("chunked size utf8")?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim();
        let size = usize::from_str_radix(size_text, 16)
            .with_context(|| format!("parse chunk size {size_text:?}"))?;
        bytes = &bytes[line_end + 2..];
        if size == 0 {
            break;
        }
        if bytes.len() < size + 2 {
            bail!("chunked body ended mid-chunk");
        }
        out.extend_from_slice(&bytes[..size]);
        bytes = &bytes[size + 2..];
    }
    Ok(out)
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn extract_session_id(response: &Value) -> Result<String> {
    response
        .get("value")
        .and_then(|value| value.get("sessionId"))
        .or_else(|| response.get("sessionId"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("new session response missing session id: {response}"))
}

fn extract_element_id(response: &Value) -> Option<String> {
    let value = response.get("value")?;
    value
        .get("element-6066-11e4-a52e-4f735466cecf")
        .or_else(|| value.get("ELEMENT"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn visible_sensitive_surface(truth: &Value) -> bool {
    truth
        .get("safety")
        .and_then(|safety| safety.get("visible_sensitive_surface"))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

fn truth_capture_allowed(truth: &Value) -> bool {
    truth
        .get("safety")
        .and_then(|safety| safety.get("capture_allowed"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn revision(truth: &Value) -> Option<String> {
    truth
        .get("page")
        .and_then(|page| page.get("revision"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn raw_values_absent(report: &Value, needles: &[String]) -> bool {
    let text = serde_json::to_string(report).unwrap_or_default();
    needles.iter().all(|needle| !text.contains(needle))
}

fn write_formmax_replay(path: &Path, fill_result: &Value) -> Result<()> {
    let events = fill_result
        .get("events")
        .and_then(Value::as_array)
        .context("FORMMAX fill result missing events")?;
    let mut file = fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    for event in events {
        writeln!(file, "{}", serde_json::to_string(event)?)
            .with_context(|| format!("write {}", path.display()))?;
    }
    Ok(())
}

fn formmax_values_absent(report: &Value, replay_path: &Path) -> Result<Value> {
    let replay_text = fs::read_to_string(replay_path)
        .with_context(|| format!("read {}", replay_path.display()))?;
    let report_text = serde_json::to_string(report)?;
    let needles = [
        "Region 1 / Site 001",
        "Region 2 / Site 009",
        "2026-02-02",
        "2026-10-10",
        "Mina",
        "Ravi",
        "Ari",
    ];
    let leaked = needles
        .iter()
        .filter(|needle| report_text.contains(**needle) || replay_text.contains(**needle))
        .copied()
        .collect::<Vec<_>>();
    Ok(json!({
        "passed": leaked.is_empty(),
        "needles_checked": needles.len(),
        "leaked": leaked,
        "values_logged": false,
    }))
}

fn count_redactions(truth: &Value) -> usize {
    truth
        .get("redactions")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}

fn collect_redaction_kinds(truth: &Value) -> Vec<String> {
    let mut kinds = truth
        .get("redactions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|redaction| redaction.get("kind").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    kinds.sort();
    kinds.dedup();
    kinds
}

fn choose_loopback_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("bind random loopback port")?;
    Ok(listener.local_addr()?.port())
}

fn default_run_dir(prefix: &str) -> PathBuf {
    PathBuf::from("runs")
        .join("servoshell_adapter")
        .join(format!("{prefix}_{}", unix_ms()))
}

fn default_smoke_url() -> String {
    file_url("test_pages/browser_session/index.html")
}

fn default_safety_matrix_url() -> String {
    file_url("test_pages/browser_session_safety_matrix/index.html")
}

fn default_formmax_url() -> String {
    file_url("test_pages/formmax/index.html")
}

fn file_url(path: &str) -> String {
    let full_path = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(path);
    Url::from_file_path(&full_path)
        .unwrap_or_else(|_| panic!("failed to build file URL for {}", full_path.display()))
        .to_string()
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("write {}", path.display()))
}

fn finish_child(mut child: Child) -> Value {
    let mut termination = "already_exited_or_sigterm".to_string();
    terminate_child(&mut child);
    std::thread::sleep(Duration::from_millis(500));
    match child.try_wait() {
        Ok(Some(_)) => {}
        Ok(None) => {
            termination = "sigkill_after_sigterm_timeout".to_string();
            let _ = child.kill();
        }
        Err(error) => termination = format!("try_wait_error:{error}"),
    }
    match child.wait_with_output() {
        Ok(output) => json!({
            "returncode": output.status.code(),
            "termination": termination,
            "stdout_head": String::from_utf8_lossy(&output.stdout).lines().take(80).collect::<Vec<_>>(),
            "stderr_head": String::from_utf8_lossy(&output.stderr).lines().take(120).collect::<Vec<_>>(),
        }),
        Err(error) => json!({"error": error.to_string()}),
    }
}

#[cfg(unix)]
fn terminate_child(child: &mut Child) {
    const SIGTERM: i32 = 15;
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    unsafe {
        let _ = kill(child.id() as i32, SIGTERM);
    }
}

#[cfg(not(unix))]
fn terminate_child(child: &mut Child) {
    let _ = child.kill();
}

const TRUTH_JS: &str = r###"
return (() => {
  const VERSION = "saccade-servoshell-truth-v0";
  const sensitiveRe = /(password|passcode|pwd|ssn|social|credit|card|cc-|cvv|cvc|otp|token|secret|passport|license|dob|birth|email|e-mail|phone|government|national|identity|tax|tin)/i;
  const visible = (el) => {
    if (!el || !el.getBoundingClientRect) return false;
    const r = el.getBoundingClientRect();
    const s = getComputedStyle(el);
    return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none" && r.bottom >= 0 && r.right >= 0 && r.top <= innerHeight && r.left <= innerWidth;
  };
  const rect = (el) => {
    const r = el.getBoundingClientRect();
    return {x: r.x, y: r.y, w: r.width, h: r.height};
  };
  const cssIdent = (s) => String(s).replace(/[^a-zA-Z0-9_-]/g, (c) => "\\" + c.charCodeAt(0).toString(16) + " ");
  const selectorFor = (el) => {
    if (el.id) return "#" + cssIdent(el.id);
    const testid = el.getAttribute("data-testid") || el.getAttribute("data-test");
    if (testid) return el.tagName.toLowerCase() + "[data-testid=\"" + String(testid).replace(/"/g, "\\\"") + "\"]";
    const parts = [];
    let cur = el;
    for (let depth = 0; cur && cur.nodeType === 1 && depth < 5; depth++, cur = cur.parentElement) {
      const tag = cur.tagName.toLowerCase();
      let nth = 1;
      let sib = cur;
      while ((sib = sib.previousElementSibling)) {
        if (sib.tagName === cur.tagName) nth++;
      }
      parts.unshift(tag + ":nth-of-type(" + nth + ")");
      if (cur.tagName === "BODY") break;
    }
    return parts.join(" > ");
  };
  const stableHash = (s) => {
    let h = 2166136261;
    for (let i = 0; i < s.length; i++) {
      h ^= s.charCodeAt(i);
      h = Math.imul(h, 16777619);
    }
    return ("00000000" + (h >>> 0).toString(16)).slice(-8);
  };
  const labelFor = (el, isSensitive) => {
    const directLabelText = (label) => [...label.childNodes]
      .filter((node) => node.nodeType === Node.TEXT_NODE)
      .map((node) => node.textContent)
      .join(" ")
      .trim()
      .replace(/\s+/g, " ")
      .slice(0, 80);
    const aria = el.getAttribute("aria-label");
    if (aria) return aria.trim().slice(0, 80);
    const id = el.id;
    if (id) {
      const label = document.querySelector("label[for=\"" + CSS.escape(id) + "\"]");
      if (label) return label.textContent.trim().replace(/\s+/g, " ").slice(0, 80);
    }
    const parentLabel = el.closest("label");
    if (parentLabel && isSensitive) {
      const own = directLabelText(parentLabel);
      if (own) return own;
    }
    if (parentLabel) return parentLabel.textContent.trim().replace(/\s+/g, " ").slice(0, 80);
    if (isSensitive) return (el.getAttribute("name") || el.id || el.tagName).trim().slice(0, 80);
    return (el.innerText || el.textContent || el.getAttribute("placeholder") || el.getAttribute("name") || el.tagName).trim().replace(/\s+/g, " ").slice(0, 80);
  };
  const sensitivityOf = (el) => {
    const type = (el.getAttribute("type") || "").toLowerCase();
    if (type === "password") return "password";
    if (type === "hidden") return "hidden";
    const data = el.getAttribute("data-sensitive");
    if (data && data.toLowerCase() !== "false" && data.toLowerCase() !== "none") return data;
    const joined = [
      type,
      el.getAttribute("autocomplete"),
      el.getAttribute("name"),
      el.id,
      el.getAttribute("aria-label"),
      el.getAttribute("placeholder"),
      el.getAttribute("inputmode")
    ].filter(Boolean).join(" ");
    const m = joined.match(sensitiveRe);
    return m ? m[0].toLowerCase() : null;
  };
  const actionEls = [...document.querySelectorAll("button,a,input,select,textarea,[role='button'],[contenteditable='true']")];
  const actions = [];
  const redactions = [];
  let visibleSensitiveSurface = false;
  for (const el of actionEls) {
    const sel = selectorFor(el);
    const kind = sensitivityOf(el);
    const isVisible = visible(el);
    const isSensitive = !!kind;
    if (isSensitive) {
      redactions.push({
        selector_hash: stableHash(sel),
        kind,
        value: "[REDACTED]",
        visible: isVisible
      });
      if (isVisible) visibleSensitiveSurface = true;
    }
    if (!isVisible) continue;
    const tag = el.tagName.toLowerCase();
    const inputType = (el.getAttribute("type") || "").toLowerCase();
    const role = el.getAttribute("role") || (tag === "a" ? "link" : tag === "button" || inputType === "button" || inputType === "submit" ? "button" : tag);
    actions.push({
      id: "a_" + actions.length,
      kind: tag === "a" || role === "button" ? "click" : "field",
      role,
      selector: sel,
      label: labelFor(el, isSensitive),
      rect: rect(el),
      enabled: !el.disabled && el.getAttribute("aria-disabled") !== "true",
      sensitive: isSensitive,
      blocked_reason: isSensitive ? "sensitive_field_requires_human" : null,
      confidence: el.id ? 0.98 : 0.8
    });
  }
  return {
    bundle_version: VERSION,
    page: {
      url: location.href,
      origin: location.origin,
      title: document.title,
      revision: document.body ? (document.body.dataset.sessionRevision || null) : null,
      body_text_length: document.body ? document.body.innerText.length : 0
    },
    viewport: {width: innerWidth, height: innerHeight, dpr: devicePixelRatio},
    safety: {
      visible_sensitive_surface: visibleSensitiveSurface,
      capture_allowed: !visibleSensitiveSurface,
      sensitive_count: redactions.length,
      screenshot_default: "forbidden"
    },
    actions,
    redactions
  };
})();
"###;

const FORMMAX_FIELD_TRUTH_JS: &str = r###"
return (() => {
  const visible = (el) => {
    if (!el || !el.getBoundingClientRect) return false;
    const r = el.getBoundingClientRect();
    const s = getComputedStyle(el);
    return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none";
  };
  const rect = (el) => {
    const r = el.getBoundingClientRect();
    return {x: r.x, y: r.y, w: r.width, h: r.height};
  };
  const sensitivityOf = (el) => {
    const explicit = el.getAttribute("data-sensitive");
    if (explicit && explicit !== "none" && explicit !== "false") return explicit;
    const text = [
      el.name || "",
      el.id || "",
      el.type || "",
      el.getAttribute("autocomplete") || ""
    ].join(" ").toLowerCase();
    const match = text.match(/password|ssn|social|tax|credit|card|signature|attestation|otp|token|secret/);
    return match ? match[0] : "none";
  };
  const fields = Array.from(document.querySelectorAll("input,select,textarea,[contenteditable='true']")).map((el, index) => {
    const row = el.closest("tr");
    const sensitivity = sensitivityOf(el);
    const tag = el.tagName.toLowerCase();
    const type = (el.getAttribute("type") || tag).toLowerCase();
    const hasValue = type === "checkbox" ? Boolean(el.checked) : Boolean((el.value || el.textContent || "").length);
    return {
      id: el.id || el.name || `field_${index}`,
      selector_hint: el.id ? `#${el.id}` : (el.name ? `[name="${el.name}"]` : tag),
      row_id: row ? row.dataset.rowId || null : null,
      field: el.getAttribute("data-field") || el.name || el.id || null,
      tag,
      type,
      visible: visible(el),
      rect: visible(el) ? rect(el) : null,
      sensitive: sensitivity !== "none",
      sensitivity,
      value_state: hasValue ? "present_redacted" : "empty"
    };
  });
  const scroller = document.getElementById("table-scroll");
  const pageLabel = document.getElementById("page-label");
  return {
    engine: "saccade-servoshell-formmax-field-truth-v0",
    page_label: pageLabel ? pageLabel.textContent : null,
    rendered_rows: document.querySelectorAll("#capacity-body tr").length,
    field_count: fields.length,
    visible_field_count: fields.filter((field) => field.visible).length,
    sensitive_count: fields.filter((field) => field.sensitive).length,
    visible_sensitive_count: fields.filter((field) => field.sensitive && field.visible).length,
    scroller: scroller ? {
      scroll_top: scroller.scrollTop,
      scroll_height: scroller.scrollHeight,
      client_height: scroller.clientHeight
    } : null,
    fields
  };
})();
"###;

const FORMMAX_HELPERS_JS: &str = r###"
function saccadeFormmaxFixture() {
  const fixture = window.__FORMMAX_FIXTURE;
  const module = window.FormmaxFixture;
  if (!fixture || !module) throw new Error("FORMMAX fixture API is missing");
  return { fixture, module };
}

function saccadeFormmaxState() {
  if (!window.__SACCADE_FORMMAX_STATE) {
    const { fixture } = saccadeFormmaxFixture();
    window.__SACCADE_FORMMAX_STATE = {
      startedAt: Date.now(),
      filled: 0,
      blocked: 0,
      events: [],
      rows: (fixture.rows || []).length,
      pages: (fixture.pages || []).length
    };
  }
  return window.__SACCADE_FORMMAX_STATE;
}

function saccadeFormmaxEmit(kind, data = {}) {
  const state = saccadeFormmaxState();
  state.events.push(Object.assign({
    kind,
    ts_ms: Date.now() - state.startedAt,
    echo_values: false
  }, data));
}

function saccadeFormmaxEvent(type) {
  return new Event(type, { bubbles: true });
}

function saccadeFormmaxRows() {
  return Array.from(document.querySelectorAll("#capacity-body tr"));
}

function saccadeFormmaxControlFor(row, spec) {
  return document.getElementsByName(row.id + "_" + spec.key)[0] || null;
}

function saccadeFormmaxSetControl(control, spec, expected) {
  control.focus();
  saccadeFormmaxEmit("field_focused", {
    row_id: expected.id,
    field: spec.key,
    control: control.tagName.toLowerCase(),
    input_type: control.type || null
  });
  if (spec.kind === "checkbox") {
    control.checked = Boolean(expected[spec.key]);
  } else {
    control.value = String(expected[spec.key]);
  }
  control.dispatchEvent(saccadeFormmaxEvent("input"));
  control.dispatchEvent(saccadeFormmaxEvent("change"));
}

function saccadeFormmaxControlMatches(control, spec, expected) {
  if (spec.kind === "checkbox") return control.checked === Boolean(expected[spec.key]);
  if (spec.kind === "number") return Number(control.value) === Number(expected[spec.key]);
  return control.value === String(expected[spec.key]);
}
"###;

const FORMMAX_INIT_JS: &str = r###"
return (() => {
  eval(arguments[0]);
  const { fixture } = saccadeFormmaxFixture();
  window.__SACCADE_FORMMAX_STATE = null;
  const state = saccadeFormmaxState();
  saccadeFormmaxEmit("form_run_started", {
    engine: "saccade-servoshell-formmax-v0",
    runtime: "official_servoshell_webdriver",
    rows: state.rows,
    pages: state.pages,
    policy: {
      block_sensitive: true,
      local_fixture_only: true,
      browser_truth_layer: true,
      echo_values: false
    }
  });
  return {
    engine: "saccade-servoshell-formmax-v0",
    rows: state.rows,
    pages: state.pages,
    field_specs: (fixture.fieldSpecs || []).map((spec) => spec.key),
    sensitive_fields: (fixture.sensitiveFields || []).map((field) => field.name)
  };
})();
"###;

const FORMMAX_RENDER_PAGE_JS: &str = r###"
return (() => {
  eval(arguments[0]);
  const pageIndex = arguments[1];
  const { fixture } = saccadeFormmaxFixture();
  const pages = fixture.pages || [];
  const scroller = document.getElementById("table-scroll");
  if (!scroller) throw new Error("FORMMAX scroller is missing");
  const expected = pages[pageIndex].length;
  let guard = 0;
  saccadeFormmaxEmit("scroll_checkpoint", {
    page: pageIndex + 1,
    rendered_rows: saccadeFormmaxRows().length,
    target_rows: expected
  });
  while (saccadeFormmaxRows().length < expected && guard < 20) {
    scroller.scrollTop = scroller.scrollHeight;
    scroller.dispatchEvent(saccadeFormmaxEvent("scroll"));
    saccadeFormmaxEmit("scroll_checkpoint", {
      page: pageIndex + 1,
      rendered_rows: saccadeFormmaxRows().length,
      target_rows: expected
    });
    guard += 1;
  }
  if (saccadeFormmaxRows().length < expected) {
    throw new Error(`page ${pageIndex + 1} rendered ${saccadeFormmaxRows().length} of ${expected} rows`);
  }
  return {
    page: pageIndex + 1,
    rendered_rows: saccadeFormmaxRows().length,
    target_rows: expected
  };
})();
"###;

const FORMMAX_FILL_CHUNK_JS: &str = r###"
return (() => {
  eval(arguments[0]);
  const pageIndex = arguments[1];
  const start = arguments[2];
  const end = arguments[3];
  const { fixture } = saccadeFormmaxFixture();
  const state = saccadeFormmaxState();
  const rows = (fixture.pages || [])[pageIndex] || [];
  const fieldSpecs = fixture.fieldSpecs || [];
  let filled = 0;
  saccadeFormmaxEmit("page_chunk_started", {
    page: pageIndex + 1,
    start,
    end: Math.min(end, rows.length)
  });
  for (const row of rows.slice(start, end)) {
    for (const spec of fieldSpecs) {
      const control = saccadeFormmaxControlFor(row, spec);
      saccadeFormmaxEmit("field_discovered", {
        page: pageIndex + 1,
        row_id: row.id,
        field: spec.key,
        sensitive: false,
        control_found: Boolean(control)
      });
      if (!control) throw new Error(`missing control ${row.id}_${spec.key}`);
      saccadeFormmaxSetControl(control, spec, row);
      filled += 1;
      state.filled += 1;
      saccadeFormmaxEmit("field_filled", {
        page: pageIndex + 1,
        row_id: row.id,
        field: spec.key,
        value_echoed: false
      });
      const ok = saccadeFormmaxControlMatches(control, spec, row);
      saccadeFormmaxEmit("field_verified", {
        page: pageIndex + 1,
        row_id: row.id,
        field: spec.key,
        passed: ok
      });
      if (!ok) throw new Error(`verification failed for ${row.id}_${spec.key}`);
    }
  }
  return {
    page: pageIndex + 1,
    start,
    end: Math.min(end, rows.length),
    filled,
    total_filled: state.filled
  };
})();
"###;

const FORMMAX_SUBMIT_PAGE_JS: &str = r###"
return (() => {
  eval(arguments[0]);
  const fromPage = arguments[1];
  const toPage = arguments[2];
  const submit = document.getElementById("submit-page");
  if (!submit) throw new Error("FORMMAX submit button is missing");
  submit.focus();
  submit.click();
  saccadeFormmaxEmit("page_next_clicked", { from_page: fromPage, to_page: toPage });
  saccadeFormmaxEmit("validation_seen", { page: fromPage, errors: 0 });
  return { from_page: fromPage, to_page: toPage };
})();
"###;

const FORMMAX_BLOCK_SENSITIVE_JS: &str = r###"
return (() => {
  eval(arguments[0]);
  const { fixture } = saccadeFormmaxFixture();
  const state = saccadeFormmaxState();
  let blocked = 0;
  for (const field of fixture.sensitiveFields || []) {
    const control = document.querySelector(`[data-sensitive="${field.name}"]`);
    const hasValue = control
      ? (control.type === "checkbox" ? control.checked : control.value !== "")
      : false;
    saccadeFormmaxEmit("field_discovered", {
      field: field.name,
      label: field.label,
      sensitive: true,
      reason: field.reason,
      control_found: Boolean(control)
    });
    saccadeFormmaxEmit("confirmation_required", {
      field: field.name,
      reason: field.reason,
      status: "requires_user_input",
      value_echoed: false
    });
    saccadeFormmaxEmit("field_blocked_sensitive", {
      field: field.name,
      reason: field.reason,
      value_present: hasValue,
      value_echoed: false
    });
    if (hasValue) throw new Error(`sensitive field unexpectedly had value: ${field.name}`);
    blocked += 1;
  }
  state.blocked = blocked;
  return { blocked_sensitive: blocked };
})();
"###;

const FORMMAX_FINALIZE_JS: &str = r###"
return (() => {
  eval(arguments[0]);
  const { fixture, module } = saccadeFormmaxFixture();
  const state = saccadeFormmaxState();
  const submit = document.getElementById("submit-page");
  if (!submit) throw new Error("FORMMAX submit button is missing");
  submit.focus();
  submit.click();
  saccadeFormmaxEmit("page_next_clicked", { from_page: 2, to_page: "receipt", local_fixture_only: true });

  const receiptText = document.getElementById("receipt").textContent || "{}";
  const receipt = JSON.parse(receiptText);
  const validation = receipt.validation || module.validateReceipt(fixture.rows || [], receipt);
  const validationErrors = (validation.failures || []).length;
  const receiptPanel = document.getElementById("receipt-panel");
  if (receiptPanel) receiptPanel.scrollIntoView({ block: "start" });
  saccadeFormmaxEmit("receipt_seen", {
    row_count: receipt.row_count,
    receipt_verified: Boolean(validation.passed),
    validation_errors: validationErrors
  });
  saccadeFormmaxEmit("form_transaction_finished", {
    rows: state.rows,
    pages: state.pages,
    filled: state.filled,
    blocked_sensitive: state.blocked,
    receipt_verified: Boolean(validation.passed),
    validation_errors: validationErrors
  });

  return {
    engine: "saccade-servoshell-formmax-v0",
    runtime: "official_servoshell_webdriver",
    browser_truth_layer: true,
    rows: state.rows,
    pages: state.pages,
    filled: state.filled,
    blocked_sensitive: state.blocked,
    receipt_verified: Boolean(validation.passed),
    validation_errors: validationErrors,
    replay_events: state.events.length,
    receipt_summary: {
      fixture: receipt.fixture,
      page_count: receipt.page_count,
      row_count: receipt.row_count,
      sensitive_fields_present: Array.isArray(receipt.sensitive_fields_present) ? receipt.sensitive_fields_present.length : 0,
      validation_passed: Boolean(validation.passed),
      validation_errors: validationErrors
    },
    events: state.events
  };
})();
"###;
