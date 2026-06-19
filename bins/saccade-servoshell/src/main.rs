use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use clap::{Parser, Subcommand, ValueEnum};
use saccade_core::{SitePolicy, classify_site_url_with_owned_domains, site_action_requires_user};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tiny_http::{Header, Response, Server, StatusCode};
use url::Url;

const DEFAULT_SERVOSHELL: &str = "/Applications/Servo.app/Contents/MacOS/servoshell";
const TRUTH_BUNDLE_VERSION: &str = "saccade-servoshell-truth-v0";
const PAGE_SETTLE_MIN: Duration = Duration::from_millis(2500);
const PAGE_SETTLE_MAX: Duration = Duration::from_millis(7000);
const PAGE_SETTLE_POLL: Duration = Duration::from_millis(250);
const PAGE_SETTLE_SHORT_TEXT_FLOOR: u64 = 500;

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
    Bridge {
        #[arg(long, default_value = DEFAULT_SERVOSHELL)]
        servoshell: PathBuf,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        output_dir: Option<PathBuf>,
        #[arg(long)]
        grant_path: Option<PathBuf>,
        #[arg(long)]
        no_headless: bool,
        #[arg(long, default_value_t = 35.0)]
        timeout_sec: f64,
        #[arg(long)]
        smoke: bool,
        #[arg(long)]
        until_ready: bool,
        #[arg(long)]
        read_article: bool,
        #[arg(long, default_value_t = 20000)]
        article_max_chars: usize,
        #[arg(long)]
        exit: bool,
        #[arg(long)]
        json: bool,
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
                servoshell: servoshell.clone(),
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

            let focused_text = "Saccade focused draft.";
            let focused_normal = run_focused_type_case(FocusedTypeConfig {
                servoshell: servoshell.clone(),
                url: default_focused_type_url(),
                output_dir: root.join("focused_type"),
                headless: !no_headless,
                timeout: Duration::from_secs_f64(timeout_sec),
                text: focused_text.to_string(),
                expect_sensitive_block: false,
                expect_contenteditable: false,
            })?;
            let focused_contenteditable = run_focused_type_case(FocusedTypeConfig {
                servoshell: servoshell.clone(),
                url: default_focused_contenteditable_url(),
                output_dir: root.join("focused_contenteditable"),
                headless: !no_headless,
                timeout: Duration::from_secs_f64(timeout_sec),
                text: focused_text.to_string(),
                expect_sensitive_block: false,
                expect_contenteditable: true,
            })?;
            let focused_sensitive = run_focused_type_case(FocusedTypeConfig {
                servoshell: servoshell.clone(),
                url: default_focused_sensitive_url(),
                output_dir: root.join("focused_sensitive"),
                headless: !no_headless,
                timeout: Duration::from_secs_f64(timeout_sec),
                text: focused_text.to_string(),
                expect_sensitive_block: true,
                expect_contenteditable: false,
            })?;
            let native_input = run_native_input_case(NativeInputConfig {
                servoshell: servoshell.clone(),
                url: default_native_input_url(),
                output_dir: root.join("native_input"),
                headless: !no_headless,
                timeout: Duration::from_secs_f64(timeout_sec),
                text: "saccade42".to_string(),
                expected_select_value: "gamma".to_string(),
            })?;
            let login_handoff = run_login_handoff_case(LoginHandoffConfig {
                servoshell: servoshell.clone(),
                fixture_root: workspace_path("test_pages/login_handoff"),
                output_dir: root.join("login_handoff"),
                headless: !no_headless,
                timeout: Duration::from_secs_f64(timeout_sec),
            })?;
            let ports = [
                normal.webdriver_port,
                sensitive.webdriver_port,
                focused_normal.webdriver_port,
                focused_contenteditable.webdriver_port,
                focused_sensitive.webdriver_port,
                native_input.webdriver_port,
                login_handoff.webdriver_port,
            ];
            let random_loopback_ports = ports
                .iter()
                .enumerate()
                .all(|(index, port)| !ports[index + 1..].iter().any(|other| other == port));

            let summary = json!({
                "ok": normal.ok && sensitive.ok && focused_normal.ok && focused_contenteditable.ok && focused_sensitive.ok && native_input.ok,
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
                "focused_type": {
                    "normal": {
                        "report": focused_normal.report_path,
                        "replay": focused_normal.replay_path,
                        "changed": focused_normal.changed,
                        "typing_method": focused_normal.typing_method,
                        "port": focused_normal.webdriver_port,
                    },
                    "contenteditable": {
                        "report": focused_contenteditable.report_path,
                        "replay": focused_contenteditable.replay_path,
                        "changed": focused_contenteditable.changed,
                        "typing_method": focused_contenteditable.typing_method,
                        "port": focused_contenteditable.webdriver_port,
                    },
                    "sensitive": {
                        "report": focused_sensitive.report_path,
                        "replay": focused_sensitive.replay_path,
                        "blocked": focused_sensitive.blocked_sensitive,
                        "port": focused_sensitive.webdriver_port,
                    },
                },
                "native_input": {
                    "report": native_input.report_path,
                    "replay": native_input.replay_path,
                    "input_value_matches": native_input.input_value_matches,
                    "input_events": native_input.input_events,
                    "select_value": native_input.select_value,
                    "select_input_events": native_input.select_input_events,
                    "select_change_events": native_input.select_change_events,
                    "select_method": native_input.select_method,
                    "port": native_input.webdriver_port,
                },
                "login_handoff": {
                    "report": login_handoff.report_path,
                    "replay": login_handoff.replay_path,
                    "human_login": login_handoff.human_login,
                    "handoff_done": login_handoff.handoff_done,
                    "agent_session": login_handoff.agent_session,
                    "password_exposed": login_handoff.password_exposed,
                    "otp_exposed": login_handoff.otp_exposed,
                    "agent_before_handoff_blocked_by_policy": login_handoff.agent_before_handoff_blocked_by_policy,
                    "screenshot_decision": login_handoff.screenshot_decision,
                    "port": login_handoff.webdriver_port,
                },
                "port_policy": {
                    "random_loopback_ports": random_loopback_ports,
                    "normal_port": normal.webdriver_port,
                    "sensitive_port": sensitive.webdriver_port,
                    "focused_type_port": focused_normal.webdriver_port,
                    "focused_contenteditable_port": focused_contenteditable.webdriver_port,
                    "focused_sensitive_port": focused_sensitive.webdriver_port,
                    "native_input_port": native_input.webdriver_port,
                    "login_handoff_port": login_handoff.webdriver_port,
                },
                "truth_bundle_version": TRUTH_BUNDLE_VERSION,
            });
            let summary_path = root.join("summary.json");
            write_json(&summary_path, &summary)?;
            let ok = normal.ok
                && sensitive.ok
                && focused_normal.ok
                && focused_contenteditable.ok
                && focused_sensitive.ok
                && native_input.ok
                && login_handoff.ok;
            println!(
                "SACCADE_SERVOSHELL_ADAPTER {} report={} normal_screenshot={} sensitive_screenshot={} focused_type={} contenteditable={} sensitive_type_blocked={} native_input={} select_value={} select_method={} login_handoff={} agent_session={}",
                if ok { "PASS" } else { "FAIL" },
                summary_path.display(),
                normal
                    .screenshot_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "none".to_string()),
                sensitive.screenshot_decision.as_str(),
                focused_normal.changed,
                focused_contenteditable.changed,
                focused_sensitive.blocked_sensitive,
                native_input.input_value_matches,
                native_input.select_value,
                native_input.select_method,
                login_handoff.handoff_done,
                login_handoff.agent_session,
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
        Command::Bridge {
            servoshell,
            url,
            output_dir,
            grant_path,
            no_headless,
            timeout_sec,
            smoke,
            until_ready,
            read_article,
            article_max_chars,
            exit,
            json,
        } => {
            let outcome = run_bridge(BridgeConfig {
                servoshell,
                url: url.unwrap_or_else(default_smoke_url),
                output_dir: output_dir.unwrap_or_else(|| default_run_dir("bridge")),
                grant_path,
                headless: !no_headless,
                timeout: Duration::from_secs_f64(timeout_sec),
                smoke,
                until_ready,
                read_article,
                article_max_chars,
                exit_after_ready: exit,
                print_json: json,
            })?;
            if json {
                let text = fs::read_to_string(&outcome.report_path)
                    .with_context(|| format!("read {}", outcome.report_path.display()))?;
                println!("{text}");
            } else {
                println!(
                    "SACCADE_SERVOSHELL_BRIDGE {} endpoint={} grant={} report={}",
                    if outcome.ok { "PASS" } else { "READY" },
                    outcome.endpoint,
                    outcome.grant_path.display(),
                    outcome.report_path.display(),
                );
            }
            Ok(())
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

#[derive(Debug)]
struct BridgeConfig {
    servoshell: PathBuf,
    url: String,
    output_dir: PathBuf,
    grant_path: Option<PathBuf>,
    headless: bool,
    timeout: Duration,
    smoke: bool,
    until_ready: bool,
    read_article: bool,
    article_max_chars: usize,
    exit_after_ready: bool,
    print_json: bool,
}

#[derive(Debug)]
struct BridgeOutcome {
    ok: bool,
    endpoint: String,
    grant_path: PathBuf,
    report_path: PathBuf,
}

#[derive(Debug, Clone)]
struct BridgeControlEndpoint {
    host: String,
    port: u16,
    protocol: String,
}

impl BridgeControlEndpoint {
    fn display_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Clone)]
struct BridgeControlState {
    client: WebDriverClient,
    session_id: String,
    webdriver_port: u16,
    page_revision: Arc<AtomicU64>,
    run_id: String,
    output_dir: PathBuf,
    report_path: PathBuf,
    replay_path: PathBuf,
    control_seq: Arc<AtomicU64>,
}

struct BridgeControlServer {
    endpoint: BridgeControlEndpoint,
    shutdown: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

#[derive(Debug)]
struct FocusedTypeConfig {
    servoshell: PathBuf,
    url: String,
    output_dir: PathBuf,
    headless: bool,
    timeout: Duration,
    text: String,
    expect_sensitive_block: bool,
    expect_contenteditable: bool,
}

#[derive(Debug)]
struct FocusedTypeOutcome {
    ok: bool,
    blocked_sensitive: bool,
    changed: bool,
    typing_method: String,
    report_path: PathBuf,
    replay_path: PathBuf,
    webdriver_port: u16,
}

#[derive(Debug)]
struct NativeInputConfig {
    servoshell: PathBuf,
    url: String,
    output_dir: PathBuf,
    headless: bool,
    timeout: Duration,
    text: String,
    expected_select_value: String,
}

#[derive(Debug)]
struct NativeInputOutcome {
    ok: bool,
    input_value_matches: bool,
    input_events: u64,
    select_value: String,
    select_input_events: u64,
    select_change_events: u64,
    select_method: String,
    report_path: PathBuf,
    replay_path: PathBuf,
    webdriver_port: u16,
}

#[derive(Debug)]
struct LoginHandoffConfig {
    servoshell: PathBuf,
    fixture_root: PathBuf,
    output_dir: PathBuf,
    headless: bool,
    timeout: Duration,
}

#[derive(Debug)]
struct LoginHandoffOutcome {
    ok: bool,
    human_login: bool,
    handoff_done: bool,
    agent_session: bool,
    password_exposed: bool,
    otp_exposed: bool,
    agent_before_handoff_blocked_by_policy: bool,
    screenshot_decision: String,
    report_path: PathBuf,
    replay_path: PathBuf,
    webdriver_port: u16,
}

fn run_bridge(cfg: BridgeConfig) -> Result<BridgeOutcome> {
    fs::create_dir_all(&cfg.output_dir)
        .with_context(|| format!("create {}", cfg.output_dir.display()))?;

    let webdriver_port = choose_loopback_port()?;
    let mut child = launch_servoshell_for_url(
        &cfg.servoshell,
        cfg.url.as_str(),
        cfg.headless,
        webdriver_port,
    )?;
    let client = WebDriverClient::new(webdriver_port, cfg.timeout);
    let mut session_id: Option<String> = None;
    let report_path = cfg.output_dir.join("report.json");
    let mut report = json!({
        "ok": false,
        "engine": "saccade-servoshell-bridge-v0",
        "runtime": "official_servoshell_webdriver",
        "servoshell": cfg.servoshell.clone(),
        "url": cfg.url.clone(),
        "headless": cfg.headless,
        "webdriver": {
            "host": "127.0.0.1",
            "port": webdriver_port,
            "port_policy": "random_loopback_private_to_launch_manager"
        },
        "policy": {
            "current_tab_grant_artifact": true,
            "agent_input_grant": true,
            "screenshots_default": "forbidden",
            "control_protocol_compat": "saccade-dogfood-control-v0"
        },
        "mode": {
            "smoke": cfg.smoke,
            "until_ready": cfg.until_ready,
            "read_article": cfg.read_article,
            "exit_after_ready": cfg.exit_after_ready,
            "json_stdout": cfg.print_json,
        },
        "output_dir": cfg.output_dir.clone(),
        "copilot_status_path": saccade_copilot_status_path(webdriver_port),
    });

    let result = (|| -> Result<(BridgeControlServer, PathBuf)> {
        let status = wait_for_status(&client, &mut child, cfg.timeout)?;
        report["webdriver"]["status"] = status;

        let session = client.new_session()?;
        let sid = extract_session_id(&session)?;
        session_id = Some(sid.clone());
        report["webdriver"]["new_session"] = session;
        report["webdriver"]["session_id"] = json!(sid);
        report["webdriver"]["initial_navigate"] = client.navigate(&sid, &cfg.url)?;

        let ready = wait_for_document_ready(&client, &sid, cfg.timeout)?;
        report["page"]["ready"] = ready.clone();

        let control_dir = cfg.output_dir.join("control");
        fs::create_dir_all(&control_dir)
            .with_context(|| format!("create {}", control_dir.display()))?;
        let state = BridgeControlState {
            client: client.clone(),
            session_id: sid,
            webdriver_port,
            page_revision: Arc::new(AtomicU64::new(1)),
            run_id: format!("servoshell_bridge_{}", unix_ms()),
            output_dir: control_dir.clone(),
            report_path: control_dir.join("report.json"),
            replay_path: control_dir.join("replay.jsonl"),
            control_seq: Arc::new(AtomicU64::new(0)),
        };
        report["control_artifacts"] = bridge_artifacts(&state);
        let server = start_bridge_control_server(state)?;
        let grant_path = cfg
            .grant_path
            .clone()
            .unwrap_or_else(default_servoshell_bridge_grant_path);
        write_bridge_grant(
            &grant_path,
            &cfg.url,
            ready.get("title").and_then(Value::as_str),
            &server.endpoint,
        )?;
        report["control_endpoint"] = bridge_endpoint_json(&server.endpoint);
        report["grant_path"] = json!(grant_path);

        if cfg.smoke {
            let ping = bridge_control_call(&server.endpoint, "ping", json!({}), cfg.timeout)?;
            let truth = bridge_control_call(&server.endpoint, "truth", json!({}), cfg.timeout)?;
            let actions = bridge_control_call(&server.endpoint, "actions", json!({}), cfg.timeout)?;
            report["smoke"] = json!({
                "ping": ping,
                "truth_engine": truth.get("engine").cloned().unwrap_or(Value::Null),
                "actions_count": actions.get("actions").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
                "same_webview_control": truth.get("same_webview_control").cloned().unwrap_or(Value::Null),
            });
        }
        if cfg.read_article {
            let article = bridge_control_call(
                &server.endpoint,
                "article_text",
                json!({"max_chars": cfg.article_max_chars}),
                cfg.timeout,
            )?;
            report["article_text"] = article;
        }
        if cfg.smoke || cfg.exit_after_ready {
            bridge_control_call(&server.endpoint, "shutdown", json!({}), cfg.timeout)?;
        }

        Ok((server, grant_path))
    })();

    match result {
        Ok((server, grant_path)) => {
            let one_shot = cfg.smoke || cfg.exit_after_ready;
            report["ok"] = json!(one_shot);
            report["live"] = json!(!one_shot);
            write_json(&report_path, &report)?;
            if one_shot {
                stop_bridge_control_server(server);
                if let Some(sid) = session_id.as_deref() {
                    if let Err(error) = client.delete_session(sid) {
                        report["webdriver"]["delete_session_error"] = json!(error.to_string());
                    }
                }
                report["process"] = finish_child(child);
                write_json(&report_path, &report)?;
                Ok(BridgeOutcome {
                    ok: true,
                    endpoint: report["control_endpoint"]
                        .get("host")
                        .and_then(Value::as_str)
                        .zip(
                            report["control_endpoint"]
                                .get("port")
                                .and_then(Value::as_u64),
                        )
                        .map(|(host, port)| format!("{host}:{port}"))
                        .unwrap_or_else(|| "unknown".to_string()),
                    grant_path,
                    report_path,
                })
            } else {
                let endpoint_addr = server.endpoint.display_addr();
                println!(
                    "SACCADE_SERVOSHELL_BRIDGE READY endpoint={} grant={} report={}",
                    endpoint_addr,
                    grant_path.display(),
                    report_path.display(),
                );
                let _ = server.handle.join();
                if let Some(sid) = session_id.as_deref() {
                    if let Err(error) = client.delete_session(sid) {
                        report["webdriver"]["delete_session_error"] = json!(error.to_string());
                    }
                }
                report["live_stopped"] = json!(true);
                report["process"] = finish_child(child);
                write_json(&report_path, &report)?;
                Ok(BridgeOutcome {
                    ok: false,
                    endpoint: endpoint_addr,
                    grant_path,
                    report_path,
                })
            }
        }
        Err(error) => {
            report["error"] = json!(error.to_string());
            if let Some(sid) = session_id.as_deref() {
                if let Err(error) = client.delete_session(sid) {
                    report["webdriver"]["delete_session_error"] = json!(error.to_string());
                }
            }
            report["process"] = finish_child(child);
            write_json(&report_path, &report)?;
            Err(error)
        }
    }
}

fn run_login_handoff_case(cfg: LoginHandoffConfig) -> Result<LoginHandoffOutcome> {
    fs::create_dir_all(&cfg.output_dir)
        .with_context(|| format!("create {}", cfg.output_dir.display()))?;

    let base_url = start_test_server(cfg.fixture_root.clone())?;
    let login_url = base_url
        .join("login.html")
        .context("build login handoff login URL")?;
    let dashboard_url = base_url
        .join("dashboard.html")
        .context("build login handoff dashboard URL")?;
    let port = choose_loopback_port()?;
    let mut child =
        launch_servoshell_for_url(&cfg.servoshell, login_url.as_str(), cfg.headless, port)?;
    let client = WebDriverClient::new(port, cfg.timeout);
    let mut session_id: Option<String> = None;
    let mut report = json!({
        "ok": false,
        "engine": "saccade-servoshell-login-handoff-v0",
        "runtime": "official_servoshell_webdriver",
        "scope": "same_session_handoff_after_explicit_done",
        "servoshell": cfg.servoshell.clone(),
        "base_url": base_url.as_str(),
        "login_url": login_url.as_str(),
        "dashboard_url": dashboard_url.as_str(),
        "headless": cfg.headless,
        "webdriver": {
            "host": "127.0.0.1",
            "port": port,
            "port_policy": "random_loopback_private_to_launch_manager"
        },
        "policy": {
            "no_agent_phase_before_done": true,
            "echo_credentials": false,
            "screenshots_default": "forbidden",
            "local_fixture_only": true
        },
        "fixture_root": cfg.fixture_root.clone(),
        "output_dir": cfg.output_dir.clone(),
    });

    let mut ok = false;
    let mut human_login = false;
    let mut handoff_done = false;
    let mut agent_session = false;
    let mut password_exposed = false;
    let mut otp_exposed = false;
    let agent_before_handoff_blocked_by_policy = true;
    let mut screenshot_decision = "not_evaluated".to_string();
    let replay_path = cfg.output_dir.join("replay.jsonl");
    let report_path = cfg.output_dir.join("report.json");

    let result = (|| -> Result<()> {
        let status = wait_for_status(&client, &mut child, cfg.timeout)?;
        report["webdriver"]["status"] = status;

        let session = client.new_session()?;
        let sid = extract_session_id(&session)?;
        session_id = Some(sid.clone());
        report["webdriver"]["new_session"] = session;
        report["webdriver"]["session_id"] = json!(sid);

        let login_ready = wait_for_title(&client, &sid, "Login", cfg.timeout)?;
        report["login_page"]["ready"] = login_ready;
        let login_truth = client.execute_sync(&sid, TRUTH_JS)?;
        report["login_page"]["truth_summary"] = summarize_truth(&login_truth);
        screenshot_decision = if truth_capture_allowed(&login_truth) {
            "allowed_but_not_captured_login_gate_no_pixels_needed".to_string()
        } else {
            "blocked_sensitive_surface".to_string()
        };
        report["login_page"]["screenshot"] = json!({
            "mode": ScreenshotMode::Forbidden,
            "decision": screenshot_decision,
            "captured": false,
        });

        let submit_result = client.execute_sync(&sid, LOGIN_HANDOFF_HUMAN_LOGIN_JS)?;
        report["human_phase"]["login_submit"] = login_handoff_submit_report(&submit_result);
        wait_for_title(&client, &sid, "Dashboard", cfg.timeout)?;

        let done = client.find_element(&sid, "#handoff-done")?;
        let done_id = extract_element_id(&done)
            .ok_or_else(|| anyhow!("handoff done response lacked element id: {done}"))?;
        report["human_phase"]["done_element"] = done;
        report["human_phase"]["done_click"] = client.click_element(&sid, &done_id)?;
        std::thread::sleep(Duration::from_millis(250));

        let human_probe = login_handoff_probe(&client, &sid)?;
        report["human_phase"]["probe"] = human_probe.clone();
        human_login = probe_text(&human_probe).contains("LOGGED_IN");
        handoff_done = human_probe
            .get("handoffDone")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        password_exposed |= probe_credentials_exposed(&human_probe, "password");
        otp_exposed |= probe_credentials_exposed(&human_probe, "otp");
        if !human_login || !handoff_done {
            bail!(
                "human login handoff failed: {}",
                login_handoff_report_probe(&human_probe)
            );
        }

        client.navigate(&sid, dashboard_url.as_str())?;
        wait_for_title(&client, &sid, "Dashboard", cfg.timeout)?;
        let agent_probe = login_handoff_probe(&client, &sid)?;
        report["agent_phase"]["probe"] = agent_probe.clone();
        agent_session = probe_text(&agent_probe).contains("LOGGED_IN")
            && agent_probe
                .get("cookie_present")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            && agent_probe
                .get("storage_shared")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            && agent_probe
                .get("handoffDone")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        password_exposed |= probe_credentials_exposed(&agent_probe, "password");
        otp_exposed |= probe_credentials_exposed(&agent_probe, "otp");

        if !agent_session || password_exposed || otp_exposed {
            bail!(
                "agent login handoff failed: {}",
                login_handoff_report_probe(&agent_probe)
            );
        }

        write_login_handoff_replay(
            &replay_path,
            screenshot_decision.as_str(),
            human_login,
            handoff_done,
            agent_session,
            agent_before_handoff_blocked_by_policy,
        )?;
        let leak_check = login_handoff_values_absent(&report, &replay_path)?;
        report["leak_check"] = leak_check.clone();
        if leak_check.get("passed").and_then(Value::as_bool) != Some(true) {
            bail!("login handoff credential leak check failed: {leak_check}");
        }

        ok = true;
        Ok(())
    })();

    if let Some(sid) = session_id.as_deref() {
        if let Err(error) = client.delete_session(sid) {
            report["webdriver"]["delete_session_error"] = json!(error.to_string());
        }
    }
    report["process"] = finish_child(child);
    if let Err(error) = result {
        report["error"] = json!(error.to_string());
    }
    report["ok"] = json!(ok);
    report["human_login"] = json!(human_login);
    report["handoff_done"] = json!(handoff_done);
    report["agent_session"] = json!(agent_session);
    report["password_exposed"] = json!(password_exposed);
    report["otp_exposed"] = json!(otp_exposed);
    report["agent_before_handoff_blocked_by_policy"] =
        json!(agent_before_handoff_blocked_by_policy);
    report["screenshot_decision"] = json!(screenshot_decision);
    report["artifacts"] = json!({
        "report": report_path,
        "replay": replay_path,
    });
    write_json(&report_path, &report)?;

    Ok(LoginHandoffOutcome {
        ok,
        human_login,
        handoff_done,
        agent_session,
        password_exposed,
        otp_exposed,
        agent_before_handoff_blocked_by_policy,
        screenshot_decision,
        report_path,
        replay_path,
        webdriver_port: port,
    })
}

fn run_native_input_case(cfg: NativeInputConfig) -> Result<NativeInputOutcome> {
    fs::create_dir_all(&cfg.output_dir)
        .with_context(|| format!("create {}", cfg.output_dir.display()))?;

    let port = choose_loopback_port()?;
    let text_char_count = cfg.text.chars().count();
    let mut child = launch_servoshell_for_url(&cfg.servoshell, &cfg.url, cfg.headless, port)?;
    let client = WebDriverClient::new(port, cfg.timeout);
    let mut session_id: Option<String> = None;
    let mut report = json!({
        "ok": false,
        "engine": "saccade-servoshell-native-input-v0",
        "runtime": "official_servoshell_webdriver",
        "servoshell": cfg.servoshell.clone(),
        "url": cfg.url.clone(),
        "headless": cfg.headless,
        "webdriver": {
            "host": "127.0.0.1",
            "port": port,
            "port_policy": "random_loopback_private_to_launch_manager"
        },
        "policy": {
            "fixture_only": true,
            "echo_text_values": false
        },
        "text": {
            "chars_requested": text_char_count,
            "logged": false
        },
        "select": {
            "expected_value": cfg.expected_select_value.clone(),
        },
        "output_dir": cfg.output_dir.clone(),
    });

    let mut ok = false;
    let mut input_value_matches = false;
    let mut input_events = 0;
    let mut select_value = String::new();
    let mut select_input_events = 0;
    let mut select_change_events = 0;
    let mut select_method = "not_attempted".to_string();
    let replay_path = cfg.output_dir.join("replay.jsonl");
    let report_path = cfg.output_dir.join("report.json");

    let result = (|| -> Result<()> {
        let status = wait_for_status(&client, &mut child, cfg.timeout)?;
        report["webdriver"]["status"] = status;

        let session = client.new_session()?;
        let sid = extract_session_id(&session)?;
        session_id = Some(sid.clone());
        report["webdriver"]["new_session"] = session;
        report["webdriver"]["session_id"] = json!(sid);

        wait_for_native_input_ready(&client, &sid, cfg.timeout)?;

        let input = client.find_element(&sid, "#probe")?;
        let input_id = extract_element_id(&input)
            .ok_or_else(|| anyhow!("native input response lacked element id: {input}"))?;
        report["input"]["element"] = input.clone();
        client.click_element(&sid, &input_id)?;
        client.send_keys(&sid, &input_id, &cfg.text)?;
        std::thread::sleep(Duration::from_millis(250));

        let input_probe = native_input_probe(&client, &sid)?;
        let value = input_probe
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("");
        input_value_matches = value == cfg.text;
        input_events = probe_count(&input_probe, "input");
        let keydown_events = probe_count(&input_probe, "keydown");
        let keyup_events = probe_count(&input_probe, "keyup");
        report["input"]["probe"] = native_input_report_probe(&input_probe);
        if !input_value_matches || input_events < 1 {
            bail!(
                "native text input failed: {}",
                native_input_report_probe(&input_probe)
            );
        }

        let select = client.find_element(&sid, "#choice")?;
        let select_id = extract_element_id(&select)
            .ok_or_else(|| anyhow!("native select response lacked element id: {select}"))?;
        report["select"]["element"] = select.clone();
        match client.send_keys(&sid, &select_id, "Gamma") {
            Ok(value) => report["select"]["webdriver_send_keys"] = value,
            Err(error) => {
                report["select"]["webdriver_send_keys_error"] = json!(format!("{error:#}"));
            }
        }
        std::thread::sleep(Duration::from_millis(250));

        select_method = "webdriver_element_value".to_string();
        let mut select_probe = match native_select_probe(&client, &sid) {
            Ok(probe) => {
                select_value = probe
                    .get("value")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                select_input_events = probe_count(&probe, "input");
                select_change_events = probe_count(&probe, "change");
                probe
            }
            Err(error) => {
                report["select"]["webdriver_probe_error"] = json!(format!("{error:#}"));
                Value::Null
            }
        };
        if select_value != cfg.expected_select_value
            || select_input_events < 1
            || select_change_events < 1
        {
            select_probe = client.execute_sync_args(
                &sid,
                NATIVE_SELECT_SET_JS,
                &[json!(cfg.expected_select_value.clone())],
            )?;
            select_value = select_probe
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            select_input_events = probe_count(&select_probe, "input");
            select_change_events = probe_count(&select_probe, "change");
            select_method = "js_select_fallback".to_string();
        }
        report["select"]["probe"] = native_select_report_probe(&select_probe);
        report["select"]["method"] = json!(select_method);
        if select_value != cfg.expected_select_value
            || select_input_events < 1
            || select_change_events < 1
        {
            bail!(
                "native select failed: {}",
                native_select_report_probe(&select_probe)
            );
        }

        write_native_input_replay(
            &replay_path,
            text_char_count,
            &input_probe,
            keydown_events,
            input_events,
            keyup_events,
            &select_value,
            select_input_events,
            select_change_events,
            &select_method,
        )?;
        let leak_check = native_input_values_absent(&report, &replay_path, &cfg.text)?;
        report["leak_check"] = leak_check.clone();
        if leak_check.get("passed").and_then(Value::as_bool) != Some(true) {
            bail!("native input value leak check failed: {leak_check}");
        }

        ok = true;
        Ok(())
    })();

    if let Some(sid) = session_id.as_deref() {
        if let Err(error) = client.delete_session(sid) {
            report["webdriver"]["delete_session_error"] = json!(error.to_string());
        }
    }
    report["process"] = finish_child(child);
    if let Err(error) = result {
        report["error"] = json!(error.to_string());
    }
    report["ok"] = json!(ok);
    report["input"]["value_matches"] = json!(input_value_matches);
    report["input"]["input_events"] = json!(input_events);
    report["select"]["value"] = json!(select_value);
    report["select"]["input_events"] = json!(select_input_events);
    report["select"]["change_events"] = json!(select_change_events);
    report["select"]["method"] = json!(select_method);
    report["artifacts"] = json!({
        "report": report_path,
        "replay": replay_path,
    });
    write_json(&report_path, &report)?;

    Ok(NativeInputOutcome {
        ok,
        input_value_matches,
        input_events,
        select_value,
        select_input_events,
        select_change_events,
        select_method,
        report_path,
        replay_path,
        webdriver_port: port,
    })
}

fn run_focused_type_case(cfg: FocusedTypeConfig) -> Result<FocusedTypeOutcome> {
    fs::create_dir_all(&cfg.output_dir)
        .with_context(|| format!("create {}", cfg.output_dir.display()))?;

    let port = choose_loopback_port()?;
    let mut child = launch_servoshell_for_url(&cfg.servoshell, &cfg.url, cfg.headless, port)?;
    let client = WebDriverClient::new(port, cfg.timeout);
    let mut session_id: Option<String> = None;
    let mut report = json!({
        "ok": false,
        "engine": "saccade-servoshell-focused-type-v0",
        "runtime": "official_servoshell_webdriver",
        "servoshell": cfg.servoshell,
        "url": cfg.url,
        "headless": cfg.headless,
        "webdriver": {
            "host": "127.0.0.1",
            "port": port,
            "port_policy": "random_loopback_private_to_launch_manager"
        },
        "policy": {
            "active_element_only": true,
            "block_sensitive": true,
            "echo_values": false
        },
        "text": {
            "chars_requested": cfg.text.chars().count(),
            "logged": false
        },
        "output_dir": cfg.output_dir,
    });

    let mut ok = false;
    let mut blocked_sensitive = false;
    let mut changed = false;
    let mut typing_method = "not_attempted".to_string();
    let replay_path = cfg.output_dir.join("replay.jsonl");
    let report_path = cfg.output_dir.join("report.json");

    let result = (|| -> Result<()> {
        let status = wait_for_status(&client, &mut child, cfg.timeout)?;
        report["webdriver"]["status"] = status;

        let session = client.new_session()?;
        let sid = extract_session_id(&session)?;
        session_id = Some(sid.clone());
        report["webdriver"]["new_session"] = session;
        report["webdriver"]["session_id"] = json!(sid);

        let before = wait_for_focused_type_preflight(&client, &sid, cfg.timeout)?;
        write_json(&cfg.output_dir.join("before.json"), &before)?;
        report["before"] = focused_type_report_probe(&before);

        if before.get("ok").and_then(Value::as_bool) != Some(true) {
            let reason = before
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("focused field is not writable");
            blocked_sensitive = reason == "focused_field_sensitive";
            if cfg.expect_sensitive_block && blocked_sensitive {
                typing_method = "blocked_by_policy".to_string();
                write_focused_type_replay(
                    &replay_path,
                    "focused_type_blocked_sensitive",
                    cfg.text.chars().count(),
                    &before,
                    &before,
                    &typing_method,
                )?;
                ok = true;
                return Ok(());
            }
            bail!("focused type preflight blocked unexpectedly: {before}");
        }
        if cfg.expect_sensitive_block {
            bail!("focused sensitive field was not blocked: {before}");
        }
        let is_contenteditable = before
            .get("contentEditable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if cfg.expect_contenteditable && !is_contenteditable {
            bail!("focused field was not contenteditable: {before}");
        }

        let element = client.active_element(&sid).or_else(|_| {
            before
                .get("selector")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("focused preflight lacked fallback selector"))
                .and_then(|selector| client.find_element(&sid, selector))
        })?;
        report["typing"]["active_element"] = element.clone();
        let element_id = extract_element_id(&element)
            .ok_or_else(|| anyhow!("active element response lacked element id: {element}"))?;
        let send_result = client.send_keys(&sid, &element_id, &cfg.text)?;
        typing_method = "webdriver_element_value".to_string();
        report["typing"]["send_keys"] = send_result;
        std::thread::sleep(Duration::from_millis(250));

        let mut after = client.execute_sync(&sid, TYPE_FOCUSED_PROBE_JS)?;
        changed = focused_type_changed(&before, &after, cfg.text.chars().count());
        if !changed && is_contenteditable {
            after = client.execute_sync_args(
                &sid,
                TYPE_FOCUSED_CONTENTEDITABLE_INSERT_JS,
                &[json!(cfg.text)],
            )?;
            typing_method = "js_contenteditable_insert_fallback".to_string();
            changed = focused_type_changed(&before, &after, cfg.text.chars().count());
        }
        write_json(&cfg.output_dir.join("after.json"), &after)?;
        report["after"] = focused_type_report_probe(&after);
        report["typing"]["method"] = json!(typing_method);
        report["typing"]["changed"] = json!(changed);
        if !changed {
            bail!("focused type did not change target length: before={before} after={after}");
        }

        write_focused_type_replay(
            &replay_path,
            "focused_text_typed",
            cfg.text.chars().count(),
            &before,
            &after,
            &typing_method,
        )?;
        let leak_check = focused_type_values_absent(&report, &replay_path, &cfg.text)?;
        report["leak_check"] = leak_check.clone();
        if leak_check.get("passed").and_then(Value::as_bool) != Some(true) {
            bail!("focused type value leak check failed: {leak_check}");
        }

        ok = true;
        Ok(())
    })();

    if let Some(sid) = session_id.as_deref() {
        if let Err(error) = client.delete_session(sid) {
            report["webdriver"]["delete_session_error"] = json!(error.to_string());
        }
    }
    report["process"] = finish_child(child);
    if let Err(error) = result {
        report["error"] = json!(error.to_string());
    }
    report["ok"] = json!(ok);
    report["blocked_sensitive"] = json!(blocked_sensitive);
    report["changed"] = json!(changed);
    report["typing_method"] = json!(typing_method);
    report["artifacts"] = json!({
        "report": report_path,
        "replay": replay_path,
    });
    write_json(&report_path, &report)?;

    Ok(FocusedTypeOutcome {
        ok,
        blocked_sensitive,
        changed,
        typing_method,
        report_path,
        replay_path,
        webdriver_port: port,
    })
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
    configure_saccade_copilot_status_env(&mut cmd, port)?;
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

        let ready = wait_for_document_ready(&client, &sid, cfg.timeout)?;
        report["webdriver"]["ready"] = ready;

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
    configure_saccade_copilot_status_env(&mut cmd, port)?;
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

    fn navigate(&self, session_id: &str, url: &str) -> Result<Value> {
        self.request(
            "POST",
            &format!("/session/{session_id}/url"),
            Some(json!({"url": url})),
        )
        .map(|response| response.body)
    }

    fn refresh(&self, session_id: &str) -> Result<Value> {
        self.request(
            "POST",
            &format!("/session/{session_id}/refresh"),
            Some(json!({})),
        )
        .map(|response| response.body)
    }

    fn back(&self, session_id: &str) -> Result<Value> {
        self.request(
            "POST",
            &format!("/session/{session_id}/back"),
            Some(json!({})),
        )
        .map(|response| response.body)
    }

    fn forward(&self, session_id: &str) -> Result<Value> {
        self.request(
            "POST",
            &format!("/session/{session_id}/forward"),
            Some(json!({})),
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

    fn active_element(&self, session_id: &str) -> Result<Value> {
        self.request(
            "GET",
            &format!("/session/{session_id}/element/active"),
            None,
        )
        .map(|response| response.body)
    }

    fn send_keys(&self, session_id: &str, element_id: &str, text: &str) -> Result<Value> {
        self.request(
            "POST",
            &format!("/session/{session_id}/element/{element_id}/value"),
            Some(json!({
                "text": text,
                "value": text.chars().map(|ch| ch.to_string()).collect::<Vec<_>>(),
            })),
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
        parse_http_response(&response).map_err(|error| {
            anyhow!(
                "parse webdriver response for {method} {path}: {error:#}; raw_prefix={}",
                preview_http_bytes(&response)
            )
        })
    }
}

fn preview_http_bytes(bytes: &[u8]) -> String {
    const LIMIT: usize = 512;
    if bytes.is_empty() {
        return "<empty>".to_string();
    }
    let mut preview = String::from_utf8_lossy(&bytes[..bytes.len().min(LIMIT)]).into_owned();
    preview = preview.replace('\r', "\\r").replace('\n', "\\n");
    if bytes.len() > LIMIT {
        preview.push_str("...");
    }
    preview
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

fn wait_for_title(
    client: &WebDriverClient,
    session_id: &str,
    title: &str,
    timeout: Duration,
) -> Result<Value> {
    let deadline = Instant::now() + timeout;
    let mut last = Value::Null;
    while Instant::now() < deadline {
        last = client.execute_sync(
            session_id,
            "return { title: document.title, readyState: document.readyState, url: document.URL };",
        )?;
        fail_if_servoshell_error_page(&last)?;
        if last.get("title").and_then(Value::as_str) == Some(title) {
            return Ok(last);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    bail!("page title did not become {title:?}: {last}");
}

fn wait_for_document_ready(
    client: &WebDriverClient,
    session_id: &str,
    timeout: Duration,
) -> Result<Value> {
    let deadline = Instant::now() + timeout;
    let mut last = Value::Null;
    while Instant::now() < deadline {
        last = client.execute_sync(
            session_id,
            "return { title: document.title, readyState: document.readyState, url: document.URL };",
        )?;
        fail_if_servoshell_error_page(&last)?;
        let ready_state = last.get("readyState").and_then(Value::as_str).unwrap_or("");
        if matches!(ready_state, "interactive" | "complete") {
            let remaining = deadline.saturating_duration_since(Instant::now());
            return wait_for_dynamic_page_settle(client, session_id, remaining, last);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    bail!("document did not become ready: {last}");
}

fn wait_for_dynamic_page_settle(
    client: &WebDriverClient,
    session_id: &str,
    timeout: Duration,
    ready_value: Value,
) -> Result<Value> {
    if timeout.is_zero() {
        return Ok(ready_value);
    }
    let max_wait = timeout.min(PAGE_SETTLE_MAX);
    let min_wait = max_wait.min(PAGE_SETTLE_MIN);
    let started_at = Instant::now();
    let min_until = started_at + min_wait;
    let deadline = started_at + max_wait;
    let mut last = ready_value;
    let mut last_signature: Option<(String, String, u64, u64, u64)> = None;
    let mut stable_polls = 0usize;

    while Instant::now() < deadline {
        last = client.execute_sync(
            session_id,
            "return { title: document.title, readyState: document.readyState, url: document.URL, bodyTextLength: document.body ? document.body.innerText.length : 0, actionCount: document.querySelectorAll(\"button,a,input,select,textarea,[role='button'],[contenteditable='true']\").length, childCount: document.body ? document.body.children.length : 0 };",
        )?;
        fail_if_servoshell_error_page(&last)?;
        let signature = page_settle_signature(&last);
        if last_signature.as_ref() == Some(&signature) {
            stable_polls = stable_polls.saturating_add(1);
        } else {
            last_signature = Some(signature);
            stable_polls = 0;
        }
        let body_text_length = last
            .get("bodyTextLength")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let enough_content = body_text_length >= PAGE_SETTLE_SHORT_TEXT_FLOOR;
        if Instant::now() >= min_until && stable_polls >= 2 && enough_content {
            return Ok(last);
        }
        std::thread::sleep(PAGE_SETTLE_POLL);
    }
    Ok(last)
}

fn fail_if_servoshell_error_page(value: &Value) -> Result<()> {
    if value.get("title").and_then(Value::as_str) == Some("Error loading page") {
        bail!("ServoShell reached its internal error page: {value}");
    }
    Ok(())
}

fn page_settle_signature(value: &Value) -> (String, String, u64, u64, u64) {
    (
        value
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        value
            .get("bodyTextLength")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        value
            .get("actionCount")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        value
            .get("childCount")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
    )
}

fn login_handoff_probe(client: &WebDriverClient, session_id: &str) -> Result<Value> {
    client.execute_sync(session_id, LOGIN_HANDOFF_PROBE_JS)
}

fn login_handoff_submit_report(value: &Value) -> Value {
    json!({
        "ok": value.get("ok").and_then(Value::as_bool).unwrap_or(false),
        "submitted": value.get("submitted").and_then(Value::as_bool).unwrap_or(false),
        "credentials_echoed": false,
    })
}

fn login_handoff_report_probe(probe: &Value) -> Value {
    json!({
        "title": probe.get("title").cloned().unwrap_or(Value::Null),
        "url": probe.get("url").cloned().unwrap_or(Value::Null),
        "text_has_logged_in": probe_text(probe).contains("LOGGED_IN"),
        "cookie_present": probe.get("cookie_present").cloned().unwrap_or(Value::Null),
        "storage_shared": probe.get("storage_shared").cloned().unwrap_or(Value::Null),
        "handoffDone": probe.get("handoffDone").cloned().unwrap_or(Value::Null),
        "credential_values_exposed": probe.get("credential_values_exposed").cloned().unwrap_or(Value::Null),
        "sensitive_field_count": probe.get("sensitive_fields").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
    })
}

fn probe_text(probe: &Value) -> String {
    probe
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn probe_credentials_exposed(probe: &Value, key: &str) -> bool {
    probe
        .get("credential_values_exposed")
        .and_then(|values| values.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

fn write_login_handoff_replay(
    path: &Path,
    screenshot_decision: &str,
    human_login: bool,
    handoff_done: bool,
    agent_session: bool,
    agent_before_handoff_blocked_by_policy: bool,
) -> Result<()> {
    let mut file = fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    let events = [
        json!({
            "kind": "login_page_seen",
            "engine": "saccade-servoshell-login-handoff-v0",
            "runtime": "official_servoshell_webdriver",
            "actor": "human",
            "screenshot_decision": screenshot_decision,
            "credentials_echoed": false,
        }),
        json!({
            "kind": "human_login_submitted",
            "actor": "human",
            "credentials_echoed": false,
        }),
        json!({
            "kind": "handoff_done_clicked",
            "actor": "human",
            "handoff_done": handoff_done,
            "agent_before_handoff_blocked_by_policy": agent_before_handoff_blocked_by_policy,
        }),
        json!({
            "kind": "agent_session_verified",
            "actor": "agent",
            "human_login": human_login,
            "agent_session": agent_session,
            "credentials_echoed": false,
        }),
    ];
    for event in events {
        writeln!(file, "{}", serde_json::to_string(&event)?)
            .with_context(|| format!("write {}", path.display()))?;
    }
    Ok(())
}

fn login_handoff_values_absent(report: &Value, replay_path: &Path) -> Result<Value> {
    let replay_text = fs::read_to_string(replay_path)
        .with_context(|| format!("read {}", replay_path.display()))?;
    let report_text = serde_json::to_string(report)?;
    let needles = ["human-only-password", "123456"];
    let leaked = needles
        .iter()
        .filter(|needle| report_text.contains(**needle) || replay_text.contains(**needle))
        .copied()
        .collect::<Vec<_>>();
    Ok(json!({
        "passed": leaked.is_empty(),
        "needles_checked": needles.len(),
        "leaked": leaked,
        "credentials_logged": false,
    }))
}

fn start_bridge_control_server(state: BridgeControlState) -> Result<BridgeControlServer> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("bind bridge control endpoint")?;
    listener
        .set_nonblocking(true)
        .context("set bridge control endpoint nonblocking")?;
    let addr = listener
        .local_addr()
        .context("read bridge control address")?;
    let endpoint = BridgeControlEndpoint {
        host: "127.0.0.1".to_string(),
        port: addr.port(),
        protocol: "saccade-dogfood-control-v0".to_string(),
    };
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_for_thread = shutdown.clone();
    let handle = thread::spawn(move || {
        while !shutdown_for_thread.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    handle_bridge_control_stream(stream, &state, &shutdown_for_thread);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(_) => {
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
    });

    Ok(BridgeControlServer {
        endpoint,
        shutdown,
        handle,
    })
}

fn stop_bridge_control_server(server: BridgeControlServer) {
    server.shutdown.store(true, Ordering::SeqCst);
    let _ = TcpStream::connect((server.endpoint.host.as_str(), server.endpoint.port));
    let _ = server.handle.join();
}

fn handle_bridge_control_stream(
    mut stream: TcpStream,
    state: &BridgeControlState,
    shutdown: &Arc<AtomicBool>,
) {
    let mut line = String::new();
    let read_result = {
        let mut reader = BufReader::new(&mut stream);
        reader.read_line(&mut line)
    };
    let response = match read_result {
        Ok(0) => json!({"id": null, "ok": false, "error": "empty bridge control request"}),
        Ok(_) => match serde_json::from_str::<Value>(&line) {
            Ok(request) => bridge_control_response(state, shutdown, &request),
            Err(error) => json!({
                "id": null,
                "ok": false,
                "error": format!("parse bridge control request: {error}"),
            }),
        },
        Err(error) => json!({
            "id": null,
            "ok": false,
            "error": format!("read bridge control request: {error}"),
        }),
    };
    let _ = writeln!(
        stream,
        "{}",
        serde_json::to_string(&response).unwrap_or_default()
    );
}

fn bridge_control_response(
    state: &BridgeControlState,
    shutdown: &Arc<AtomicBool>,
    request: &Value,
) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    match bridge_control_result(state, shutdown, method, params) {
        Ok(result) => match bridge_record_control_success(state, method, &result) {
            Ok(()) => json!({"id": id, "ok": true, "result": result}),
            Err(error) => json!({"id": id, "ok": false, "error": error.to_string()}),
        },
        Err(error) => {
            let error_text = error.to_string();
            let _ = bridge_record_control_error(state, method, &error_text);
            json!({"id": id, "ok": false, "error": error_text})
        }
    }
}

fn bridge_control_result(
    state: &BridgeControlState,
    shutdown: &Arc<AtomicBool>,
    method: &str,
    params: Value,
) -> Result<Value> {
    match method {
        "ping" => bridge_status_response(state, "saccade-servoshell-bridge-ping-v0", false),
        "shell_status" => {
            bridge_status_response(state, "saccade-servoshell-bridge-shell-status-v0", false)
        }
        "truth" => bridge_probe_response(state, "saccade-servoshell-bridge-truth-v0"),
        "actions" => bridge_probe_response(state, "saccade-servoshell-bridge-actions-v0"),
        "article_text" => bridge_article_text_response(state, params),
        "fill_agent_fields" => bridge_fill_agent_fields_response(state, params),
        "inspect_fields" => bridge_inspect_fields_response(state, params),
        "act" => bridge_act_response(state, params),
        "formmax_live_fill" => bridge_formmax_live_fill_response(state, params),
        "navigate" => {
            let url = params
                .get("url")
                .and_then(Value::as_str)
                .context("navigate requires params.url")?;
            Url::parse(url).with_context(|| format!("invalid navigate URL: {url}"))?;
            state.client.navigate(&state.session_id, url)?;
            wait_for_document_ready(&state.client, &state.session_id, state.client.timeout)?;
            state.page_revision.fetch_add(1, Ordering::SeqCst);
            bridge_status_response(state, "saccade-servoshell-bridge-navigate-v0", true)
        }
        "reload" => {
            state.client.refresh(&state.session_id)?;
            wait_for_document_ready(&state.client, &state.session_id, state.client.timeout)?;
            state.page_revision.fetch_add(1, Ordering::SeqCst);
            bridge_status_response(state, "saccade-servoshell-bridge-reload-v0", true)
        }
        "back" => {
            state.client.back(&state.session_id)?;
            wait_for_document_ready(&state.client, &state.session_id, state.client.timeout)?;
            state.page_revision.fetch_add(1, Ordering::SeqCst);
            bridge_status_response(state, "saccade-servoshell-bridge-back-v0", true)
        }
        "forward" => {
            state.client.forward(&state.session_id)?;
            wait_for_document_ready(&state.client, &state.session_id, state.client.timeout)?;
            state.page_revision.fetch_add(1, Ordering::SeqCst);
            bridge_status_response(state, "saccade-servoshell-bridge-forward-v0", true)
        }
        "shutdown" => {
            shutdown.store(true, Ordering::SeqCst);
            Ok(json!({
                "status": "ok",
                "runtime": "saccade-servoshell-bridge-v0",
                "engine": "saccade-servoshell-bridge-shutdown-v0",
                "summary": "bridge shutdown requested",
            }))
        }
        other => bail!("unsupported bridge control method {other:?}"),
    }
}

fn bridge_status_response(
    state: &BridgeControlState,
    engine: &str,
    changed: bool,
) -> Result<Value> {
    let page = state.client.execute_sync(
        &state.session_id,
        "return { url: document.URL, title: document.title, readyState: document.readyState };",
    )?;
    let page_url = page.get("url").and_then(Value::as_str).unwrap_or("");
    Ok(json!({
        "status": "ok",
        "runtime": "saccade-servoshell-bridge-v0",
        "engine": engine,
        "summary": "official ServoShell WebDriver bridge is attached to the visible browser session",
        "same_webview_control": true,
        "url": page.get("url").cloned().unwrap_or(Value::Null),
        "title": page.get("title").cloned().unwrap_or(Value::Null),
        "load_state": page.get("readyState").cloned().unwrap_or(Value::Null),
        "page_revision": state.page_revision.load(Ordering::SeqCst),
        "changed": changed,
        "site_policy": classify_site_url(page_url),
        "copilot": bridge_copilot_state(),
        "webdriver": {
            "host": "127.0.0.1",
            "port": state.webdriver_port,
        },
        "capabilities": [
            "ping",
            "shell_status",
            "truth",
            "actions",
            "article_text",
            "fill_agent_fields",
            "inspect_fields",
            "act",
            "formmax_live_fill",
            "navigate",
            "reload",
            "back",
            "forward"
        ],
        "artifacts": bridge_artifacts(state),
    }))
}

fn bridge_copilot_state() -> Value {
    bridge_copilot_state_for_visible_ui("official_servoshell_external_ui")
}

fn bridge_copilot_state_for_visible_ui(visible_ui: &str) -> Value {
    json!({
        "status": "granted",
        "badge": "Copilot Granted",
        "owner": "Human",
        "read_grant": "FullTruth",
        "agent_input_grant": true,
        "user_confirmation_required_for_side_effects": true,
        "sensitive_values_visible_to_user": true,
        "sensitive_values_exposed_to_agent": false,
        "page_dom_injected": false,
        "visible_ui": visible_ui,
    })
}

fn bridge_artifacts(state: &BridgeControlState) -> Value {
    json!({
        "run_dir": state.output_dir.display().to_string(),
        "report": state.report_path.display().to_string(),
        "replay": state.replay_path.display().to_string(),
        "block_report": bridge_block_report_path(state).display().to_string(),
        "copilot_status": saccade_copilot_status_path(state.webdriver_port).display().to_string(),
    })
}

fn saccade_copilot_status_path(port: u16) -> PathBuf {
    std::env::temp_dir().join(format!("saccade_copilot_status_{port}.json"))
}

fn configure_saccade_copilot_status_env(cmd: &mut ProcessCommand, port: u16) -> Result<()> {
    let path = saccade_copilot_status_path(port);
    write_json(
        &path,
        &json!({
            "copilot": bridge_copilot_state_for_visible_ui("servoshell_thin_fork_chrome")
        }),
    )?;
    cmd.env("SACCADE_COPILOT_STATUS_PATH", path);
    Ok(())
}

fn bridge_block_report_path(state: &BridgeControlState) -> PathBuf {
    state.output_dir.join("block_report.json")
}

fn bridge_current_url(state: &BridgeControlState) -> Result<String> {
    Ok(state
        .client
        .execute_sync(&state.session_id, "return document.URL;")?
        .as_str()
        .unwrap_or("")
        .to_string())
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

fn bridge_record_control_success(
    state: &BridgeControlState,
    method: &str,
    result: &Value,
) -> Result<()> {
    let event = bridge_control_event(state, method, true, bridge_result_summary(method, result));
    bridge_append_replay(state, &event)?;
    bridge_write_control_report(state, &event)
}

fn bridge_record_control_error(
    state: &BridgeControlState,
    method: &str,
    error: &str,
) -> Result<()> {
    let event = bridge_control_event(
        state,
        method,
        false,
        json!({
            "method": method,
            "status": "error",
            "error": "redacted_control_error",
        }),
    );
    bridge_append_replay(state, &event)?;
    bridge_write_control_report(state, &event)?;
    let _ = bridge_write_block_report(state, method, error, &event);
    Ok(())
}

fn bridge_write_block_report(
    state: &BridgeControlState,
    method: &str,
    error: &str,
    latest_event: &Value,
) -> Result<()> {
    let page = bridge_visible_block_page(state).unwrap_or_else(|fallback_error| {
        json!({
            "url": Value::Null,
            "title": Value::Null,
            "visible_text": "",
            "collection_error": redact_block_text(&fallback_error.to_string(), 240),
        })
    });
    let url = page.get("url").and_then(Value::as_str).unwrap_or("");
    let visible_text = page
        .get("visible_text")
        .and_then(Value::as_str)
        .unwrap_or("");
    let site_policy = classify_site_url(url);
    let visible_error_excerpt = visible_block_excerpt(visible_text);
    let request_id =
        extract_visible_request_id(visible_text).or_else(|| extract_visible_request_id(error));
    let block_kind = classify_block_kind(error, visible_text, &site_policy);
    let report = json!({
        "ok": false,
        "kind": "saccade_block_report",
        "run_id": state.run_id.as_str(),
        "method": method,
        "block_kind": block_kind,
        "url": redacted_url_for_block_report(url),
        "title": page.get("title").cloned().unwrap_or(Value::Null),
        "site_policy": site_policy,
        "error": redact_block_text(error, 320),
        "visible_error_excerpt": visible_error_excerpt,
        "request_id": request_id,
        "fallback_recommendation": fallback_recommendation(&site_policy),
        "values_logged": false,
        "screenshot_captured": false,
        "latest_event": latest_event,
        "artifacts": bridge_artifacts(state),
    });
    write_json(&bridge_block_report_path(state), &report)
}

fn bridge_visible_block_page(state: &BridgeControlState) -> Result<Value> {
    state.client.execute_sync(
        &state.session_id,
        "return {
          url: document.URL,
          title: document.title,
          visible_text: document.body ? document.body.innerText : ''
        };",
    )
}

fn bridge_control_event(
    state: &BridgeControlState,
    method: &str,
    ok: bool,
    result_summary: Value,
) -> Value {
    let seq = state.control_seq.fetch_add(1, Ordering::SeqCst) + 1;
    json!({
        "kind": "servoshell_bridge_control",
        "run_id": state.run_id.as_str(),
        "seq": seq,
        "method": method,
        "ok": ok,
        "page_revision": state.page_revision.load(Ordering::SeqCst),
        "values_logged": false,
        "result": result_summary,
        "copilot": bridge_copilot_state(),
    })
}

fn bridge_result_summary(method: &str, result: &Value) -> Value {
    let mut counts = serde_json::Map::new();
    bridge_insert_count(&mut counts, "actions", result.get("actions"));
    bridge_insert_count(&mut counts, "fields", result.get("fields"));
    bridge_insert_count(&mut counts, "filled", result.get("filled"));
    bridge_insert_count(&mut counts, "rejected", result.get("rejected"));
    for key in [
        "rows",
        "pages",
        "blocked_sensitive",
        "validation_errors",
        "replay_events",
        "requested",
        "article_text_length",
        "text_chars_returned",
        "body_text_length",
    ] {
        if let Some(value) = result.get(key).and_then(Value::as_u64) {
            counts.insert(key.to_string(), json!(value));
        }
    }
    if let Some(fields) = result.get("fields").and_then(Value::as_array) {
        let values_returned = fields
            .iter()
            .filter(|field| field.get("value_returned").and_then(Value::as_bool) == Some(true))
            .count();
        let values_redacted = fields
            .iter()
            .filter(|field| field.get("value_redacted").and_then(Value::as_bool) == Some(true))
            .count();
        counts.insert("values_returned".to_string(), json!(values_returned));
        counts.insert("values_redacted".to_string(), json!(values_redacted));
    }

    json!({
        "method": method,
        "status": result.get("status").cloned().unwrap_or(Value::Null),
        "runtime": result.get("runtime").cloned().unwrap_or(Value::Null),
        "engine": result.get("engine").cloned().unwrap_or(Value::Null),
        "same_webview_control": result.get("same_webview_control").cloned().unwrap_or(Value::Null),
        "page_revision": result.get("page_revision").cloned().unwrap_or(Value::Null),
        "changed": result.get("changed").cloned().unwrap_or(Value::Null),
        "receipt_verified": result.get("receipt_verified").cloned().unwrap_or(Value::Null),
        "verification": result.get("verification").cloned().unwrap_or(Value::Null),
        "extraction": result.get("extraction").cloned().unwrap_or(Value::Null),
        "site_policy": result.get("site_policy").cloned().unwrap_or(Value::Null),
        "policy": result.get("policy").cloned().unwrap_or(Value::Null),
        "counts": Value::Object(counts),
    })
}

fn classify_block_kind(
    error: &str,
    visible_text: &str,
    site_policy: &saccade_core::SitePolicy,
) -> &'static str {
    let text = format!("{error}\n{visible_text}").to_lowercase();
    if text.contains("site policy") {
        return "saccade_site_policy";
    }
    if text.contains("captcha")
        || text.contains("recaptcha")
        || text.contains("unusual traffic")
        || text.contains("access denied")
        || text.contains("can't process")
        || text.contains("cannot process")
        || text.contains("forbidden")
        || text.contains("blocked")
    {
        return "site_or_provider_block";
    }
    match site_policy.level {
        saccade_core::SiteRiskLevel::Orange | saccade_core::SiteRiskLevel::Red => {
            "high_risk_site_fallback"
        }
        _ => "control_error",
    }
}

fn fallback_recommendation(site_policy: &saccade_core::SitePolicy) -> &'static str {
    match site_policy.level {
        saccade_core::SiteRiskLevel::Red => {
            "Use the normal browser or official app for login/auth/security. The agent may continue only after a redacted handoff on a lower-risk page."
        }
        saccade_core::SiteRiskLevel::Orange => {
            "Use the normal browser for login and high-impact actions. Provide redacted non-sensitive text if you want the agent to summarize, draft, or checklist the next step."
        }
        saccade_core::SiteRiskLevel::Yellow => {
            "Keep the human in the loop. The agent may draft or inspect redacted state, but submit/publish/delete/payment/security actions require the user."
        }
        saccade_core::SiteRiskLevel::Green => {
            "Treat this as a compatibility or site block. Compare with a reference browser, record the request id, and do not add stealth or bypass behavior."
        }
    }
}

fn redacted_url_for_block_report(url: &str) -> Value {
    let Ok(mut parsed) = Url::parse(url) else {
        return Value::Null;
    };
    parsed.set_query(None);
    parsed.set_fragment(None);
    json!(parsed.as_str())
}

fn visible_block_excerpt(text: &str) -> Value {
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| is_block_related_line(line) || extract_visible_request_id(line).is_some())
        .take(6)
        .map(|line| redact_block_text(line, 180))
        .collect::<Vec<_>>();
    json!(lines)
}

fn is_block_related_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    [
        "can't process",
        "cannot process",
        "access denied",
        "blocked",
        "forbidden",
        "not authorized",
        "try again",
        "contact us",
        "request id",
        "captcha",
        "verify",
        "unusual traffic",
        "error",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn extract_visible_request_id(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|token| {
            token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
        })
        .find(|token| looks_like_request_id(token))
        .map(ToOwned::to_owned)
}

fn looks_like_request_id(token: &str) -> bool {
    let len = token.len();
    if !(24..=80).contains(&len) {
        return false;
    }
    let hyphen_count = token.chars().filter(|c| *c == '-').count();
    let hexish = token
        .chars()
        .all(|c| c.is_ascii_hexdigit() || c == '-' || c == '_');
    hexish && hyphen_count >= 2
}

fn redact_block_text(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for token in text.split_whitespace() {
        let redacted = redact_block_token(token);
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&redacted);
        if out.len() >= max_chars {
            out.truncate(max_chars);
            out.push_str("...");
            break;
        }
    }
    out
}

fn redact_block_token(token: &str) -> String {
    if token.contains('@') && token.contains('.') {
        return "[redacted-email]".into();
    }
    if token.starts_with("http://") || token.starts_with("https://") {
        return Url::parse(token)
            .ok()
            .map(|mut url| {
                url.set_query(None);
                url.set_fragment(None);
                url.to_string()
            })
            .unwrap_or_else(|| "[redacted-url]".into());
    }
    let digits = token.chars().filter(|c| c.is_ascii_digit()).count();
    if digits >= 8 && !looks_like_request_id(token) {
        return "[redacted-number]".into();
    }
    token.to_string()
}

fn bridge_insert_count(
    counts: &mut serde_json::Map<String, Value>,
    key: &str,
    value: Option<&Value>,
) {
    let Some(value) = value else {
        return;
    };
    let count = if let Some(items) = value.as_array() {
        Some(items.len() as u64)
    } else {
        value.as_u64()
    };
    if let Some(count) = count {
        counts.insert(key.to_string(), json!(count));
    }
}

fn bridge_append_replay(state: &BridgeControlState, event: &Value) -> Result<()> {
    if let Some(parent) = state.replay_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&state.replay_path)
        .with_context(|| format!("open {}", state.replay_path.display()))?;
    writeln!(file, "{event}").with_context(|| format!("write {}", state.replay_path.display()))
}

fn bridge_write_control_report(state: &BridgeControlState, latest: &Value) -> Result<()> {
    let report = json!({
        "ok": latest.get("ok").and_then(Value::as_bool).unwrap_or(false),
        "run_id": state.run_id.as_str(),
        "engine": "saccade-servoshell-bridge-v0",
        "runtime": "official_servoshell_webdriver",
        "webdriver": {
            "host": "127.0.0.1",
            "port": state.webdriver_port,
        },
        "page_revision": state.page_revision.load(Ordering::SeqCst),
        "copilot": bridge_copilot_state(),
        "latest": latest,
        "artifacts": bridge_artifacts(state),
    });
    write_json(&state.report_path, &report)
}

fn bridge_probe_response(state: &BridgeControlState, engine: &str) -> Result<Value> {
    let truth = state.client.execute_sync(&state.session_id, TRUTH_JS)?;
    let actions = truth
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let page_url = truth
        .pointer("/page/url")
        .and_then(Value::as_str)
        .unwrap_or("");
    Ok(json!({
        "status": "ok",
        "runtime": "saccade-servoshell-bridge-v0",
        "engine": engine,
        "summary": "redacted truth/actions collected from official ServoShell through the live bridge",
        "same_webview_control": true,
        "url": truth.pointer("/page/url").cloned().unwrap_or(Value::Null),
        "title": truth.pointer("/page/title").cloned().unwrap_or(Value::Null),
        "page_revision": state.page_revision.load(Ordering::SeqCst),
        "site_policy": classify_site_url(page_url),
        "actions": actions,
        "findings": [],
        "truth": {
            "page": truth.get("page").cloned().unwrap_or(Value::Null),
            "viewport": truth.get("viewport").cloned().unwrap_or(Value::Null),
            "safety": truth.get("safety").cloned().unwrap_or(Value::Null),
            "redaction_count": truth.get("redactions").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
            "action_count": truth.get("actions").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
        },
        "artifacts": bridge_artifacts(state),
    }))
}

fn bridge_article_text_response(state: &BridgeControlState, params: Value) -> Result<Value> {
    let page_url = bridge_current_url(state)?;
    let site_policy = classify_site_url(&page_url);
    if !site_policy.agent_read_allowed {
        bail!(
            "site policy {:?} blocks article_text on {}; use human fallback",
            site_policy.level,
            page_url
        );
    }
    let max_chars = params
        .get("max_chars")
        .and_then(Value::as_u64)
        .unwrap_or(20_000)
        .clamp(1_000, 100_000) as usize;
    let article =
        state
            .client
            .execute_sync_args(&state.session_id, ARTICLE_TEXT_JS, &[json!(max_chars)])?;
    let article_text_length = article
        .get("articleTextLength")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let text_chars_returned = article
        .get("text")
        .and_then(Value::as_str)
        .map(|text| text.chars().count() as u64)
        .unwrap_or_default();
    Ok(json!({
        "status": "ok",
        "runtime": "saccade-servoshell-bridge-v0",
        "engine": "saccade-servoshell-bridge-article-text-v0",
        "summary": "article/main text extracted from official ServoShell through the live bridge",
        "same_webview_control": true,
        "page_revision": state.page_revision.load(Ordering::SeqCst),
        "site_policy": site_policy,
        "url": article.get("url").cloned().unwrap_or(Value::Null),
        "title": article.get("title").cloned().unwrap_or(Value::Null),
        "body_text_length": article.get("bodyTextLength").and_then(Value::as_u64).unwrap_or_default(),
        "article_text_length": article_text_length,
        "text_chars_returned": text_chars_returned,
        "text_truncated": article.get("textTruncated").and_then(Value::as_bool).unwrap_or(false),
        "extraction": {
            "mode": article.get("mode").cloned().unwrap_or(Value::Null),
            "selector": article.get("selector").cloned().unwrap_or(Value::Null),
            "candidate_count": article.get("candidateCount").cloned().unwrap_or(Value::Null),
            "max_chars": max_chars,
        },
        "headings": article.get("headings").cloned().unwrap_or_else(|| json!([])),
        "text": article.get("text").cloned().unwrap_or(Value::Null),
        "artifacts": bridge_artifacts(state),
    }))
}

fn bridge_fill_agent_fields_response(state: &BridgeControlState, params: Value) -> Result<Value> {
    let fields = params
        .get("fields")
        .and_then(Value::as_object)
        .context("fill_agent_fields requires object params.fields")?;
    if fields.is_empty() {
        bail!("fill_agent_fields requires at least one field");
    }
    let page_url = bridge_current_url(state)?;
    let site_policy = classify_site_url(&page_url);
    if !site_policy.agent_fill_allowed {
        bail!(
            "site policy {:?} blocks agent fill on {}; use human fallback",
            site_policy.level,
            page_url
        );
    }
    let fill_result = state.client.execute_sync_args(
        &state.session_id,
        BRIDGE_FILL_AGENT_FIELDS_JS,
        &[Value::Object(fields.clone())],
    )?;
    let filled = fill_result
        .get("filled")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !filled.is_empty() {
        state.page_revision.fetch_add(1, Ordering::SeqCst);
    }
    Ok(json!({
        "status": "ok",
        "runtime": "saccade-servoshell-bridge-v0",
        "engine": "saccade-servoshell-bridge-fill-v0",
        "summary": "agent-owned non-sensitive fields filled through official ServoShell bridge",
        "same_webview_control": true,
        "page_revision": state.page_revision.load(Ordering::SeqCst),
        "site_policy": site_policy,
        "requested": fields.len(),
        "filled": fill_result.get("filled").cloned().unwrap_or_else(|| json!([])),
        "rejected": fill_result.get("rejected").cloned().unwrap_or_else(|| json!([])),
        "sensitive_fields_seen": fill_result.get("sensitiveFieldsSeen").cloned().unwrap_or(Value::Null),
        "artifacts": bridge_artifacts(state),
    }))
}

fn bridge_inspect_fields_response(state: &BridgeControlState, params: Value) -> Result<Value> {
    let fields = params
        .get("fields")
        .and_then(Value::as_array)
        .context("inspect_fields requires array params.fields")?;
    if fields.is_empty() {
        bail!("inspect_fields requires at least one field");
    }
    if fields.iter().any(|field| field.as_str().is_none()) {
        bail!("inspect_fields field ids must be strings");
    }
    let page_url = bridge_current_url(state)?;
    let site_policy = classify_site_url(&page_url);
    if !site_policy.agent_read_allowed {
        bail!(
            "site policy {:?} blocks field inspection on {}; use human fallback",
            site_policy.level,
            page_url
        );
    }
    let inspect_result = state.client.execute_sync_args(
        &state.session_id,
        BRIDGE_INSPECT_FIELDS_JS,
        &[json!(fields)],
    )?;
    Ok(json!({
        "status": "ok",
        "runtime": "saccade-servoshell-bridge-v0",
        "engine": "saccade-servoshell-bridge-inspect-fields-v0",
        "summary": "explicit field inspection completed through official ServoShell bridge with sensitive values masked",
        "same_webview_control": true,
        "page_revision": state.page_revision.load(Ordering::SeqCst),
        "site_policy": site_policy,
        "requested": fields.len(),
        "fields": inspect_result.get("fields").cloned().unwrap_or_else(|| json!([])),
        "sensitive_fields_seen": inspect_result.get("sensitiveFieldsSeen").cloned().unwrap_or(Value::Null),
        "artifacts": bridge_artifacts(state),
    }))
}

fn bridge_act_response(state: &BridgeControlState, params: Value) -> Result<Value> {
    let action_id = params
        .get("action_id")
        .and_then(Value::as_str)
        .context("act requires params.action_id")?;
    let basis_page_revision = params
        .get("basis_page_revision")
        .and_then(Value::as_u64)
        .context("act requires params.basis_page_revision")?;
    let before_truth = state.client.execute_sync(&state.session_id, TRUTH_JS)?;
    let before_revision = bridge_dom_revision(&before_truth);
    let action = before_truth
        .get("actions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|action| bridge_action_matches(action, action_id))
        .cloned()
        .with_context(|| format!("unknown action_id {action_id:?}"))?;
    if action.get("enabled").and_then(Value::as_bool) != Some(true) {
        bail!("action {action_id:?} is not enabled");
    }
    if action.get("sensitive").and_then(Value::as_bool) == Some(true) {
        bail!("action {action_id:?} targets a sensitive field and requires user control");
    }
    let semantic_id = bridge_action_id(&action);
    let page_url = before_truth
        .pointer("/page/url")
        .and_then(Value::as_str)
        .unwrap_or("");
    let action_label = action.get("label").and_then(Value::as_str);
    if let Some(reason) = site_action_requires_user(page_url, &semantic_id, action_label) {
        bail!("user confirmation required before action {semantic_id:?}: {reason}");
    }
    let selector = action
        .get("selector")
        .and_then(Value::as_str)
        .context("bridge action is missing selector")?;
    let element = state.client.find_element(&state.session_id, selector)?;
    let element_id = extract_element_id(&element)
        .with_context(|| format!("WebDriver did not return element id for {selector:?}"))?;
    state.client.click_element(&state.session_id, &element_id)?;
    thread::sleep(Duration::from_millis(120));
    let after_truth = state.client.execute_sync(&state.session_id, TRUTH_JS)?;
    let changed = bridge_truth_changed(&before_truth, &after_truth);
    if changed {
        state.page_revision.fetch_add(1, Ordering::SeqCst);
    }
    let actions = after_truth
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(json!({
        "status": "ok",
        "runtime": "saccade-servoshell-bridge-v0",
        "engine": "saccade-servoshell-bridge-act-v0",
        "summary": "safe action dispatched through official ServoShell bridge",
        "same_webview_control": true,
        "page_revision": state.page_revision.load(Ordering::SeqCst),
        "site_policy": classify_site_url(page_url),
        "actions": actions,
        "verification": {
            "mode": "servoshell_bridge_webdriver_click_v0",
            "action_id": semantic_id,
            "action_sent": true,
            "changed": changed,
            "no_effect": !changed,
            "basis_page_revision": basis_page_revision,
            "new_page_revision": state.page_revision.load(Ordering::SeqCst),
            "dom_page_revision_before": before_revision,
            "dom_page_revision_after": bridge_dom_revision(&after_truth),
            "body_text_length_changed": before_truth.pointer("/page/body_text_length") != after_truth.pointer("/page/body_text_length"),
        },
        "truth": {
            "sensitive_fields": after_truth.pointer("/safety/sensitive_count").cloned().unwrap_or(Value::Null),
        },
        "artifacts": bridge_artifacts(state),
    }))
}

fn bridge_formmax_live_fill_response(state: &BridgeControlState, params: Value) -> Result<Value> {
    let policy = params.get("policy").cloned().unwrap_or_else(|| json!({}));
    if policy
        .get("block_sensitive")
        .and_then(Value::as_bool)
        .is_some_and(|enabled| !enabled)
    {
        bail!("formmax_live_fill requires block_sensitive=true");
    }
    if policy
        .get("local_fixture_only")
        .and_then(Value::as_bool)
        .is_some_and(|enabled| !enabled)
    {
        bail!("formmax_live_fill requires local_fixture_only=true");
    }

    wait_for_formmax_ready(&state.client, &state.session_id, state.client.timeout)?;
    state.client.execute_sync_args(
        &state.session_id,
        FORMMAX_INIT_JS,
        &[json!(FORMMAX_HELPERS_JS)],
    )?;
    state.client.execute_sync_args(
        &state.session_id,
        FORMMAX_RENDER_PAGE_JS,
        &[json!(FORMMAX_HELPERS_JS), json!(0)],
    )?;
    for start in (0..48).step_by(16) {
        state.client.execute_sync_args(
            &state.session_id,
            FORMMAX_FILL_CHUNK_JS,
            &[
                json!(FORMMAX_HELPERS_JS),
                json!(0),
                json!(start),
                json!(start + 16),
            ],
        )?;
    }
    state.client.execute_sync_args(
        &state.session_id,
        FORMMAX_SUBMIT_PAGE_JS,
        &[json!(FORMMAX_HELPERS_JS), json!(1), json!(2)],
    )?;
    state.client.execute_sync_args(
        &state.session_id,
        FORMMAX_RENDER_PAGE_JS,
        &[json!(FORMMAX_HELPERS_JS), json!(1)],
    )?;
    for start in (0..48).step_by(16) {
        state.client.execute_sync_args(
            &state.session_id,
            FORMMAX_FILL_CHUNK_JS,
            &[
                json!(FORMMAX_HELPERS_JS),
                json!(1),
                json!(start),
                json!(start + 16),
            ],
        )?;
    }
    state.client.execute_sync_args(
        &state.session_id,
        FORMMAX_BLOCK_SENSITIVE_JS,
        &[json!(FORMMAX_HELPERS_JS)],
    )?;
    let result = state.client.execute_sync_args(
        &state.session_id,
        FORMMAX_FINALIZE_JS,
        &[json!(FORMMAX_HELPERS_JS)],
    )?;
    let filled = result.get("filled").and_then(Value::as_u64).unwrap_or(0);
    if filled > 0 {
        state.page_revision.fetch_add(1, Ordering::SeqCst);
    }
    bridge_append_formmax_events(state, &result)?;
    let page_url = bridge_current_url(state)?;
    Ok(bridge_formmax_response(
        state,
        "saccade-servoshell-bridge-v0",
        "saccade-servoshell-bridge-formmax-live-v0",
        state.page_revision.load(Ordering::SeqCst),
        json!(classify_site_url(&page_url)),
        &result,
    ))
}

fn bridge_append_formmax_events(state: &BridgeControlState, result: &Value) -> Result<()> {
    let events = result
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for event in events {
        let mut event = event;
        if let Some(object) = event.as_object_mut() {
            object.insert("run_id".into(), json!(state.run_id.as_str()));
            object.insert(
                "page_revision".into(),
                json!(state.page_revision.load(Ordering::SeqCst)),
            );
            object.insert("values_logged".into(), json!(false));
            object.insert(
                "bridge_runtime".into(),
                json!("saccade-servoshell-bridge-v0"),
            );
        }
        bridge_append_replay(state, &event)?;
    }
    Ok(())
}

fn bridge_formmax_response(
    state: &BridgeControlState,
    runtime: &str,
    engine: &str,
    page_revision: u64,
    site_policy: Value,
    result: &Value,
) -> Value {
    let rows = result.get("rows").and_then(Value::as_u64).unwrap_or(0);
    let pages = result.get("pages").and_then(Value::as_u64).unwrap_or(0);
    let filled = result.get("filled").and_then(Value::as_u64).unwrap_or(0);
    let blocked_sensitive = result
        .get("blocked_sensitive")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let receipt_verified = result
        .get("receipt_verified")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let validation_errors = result
        .get("validation_errors")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let replay_events = result
        .get("events")
        .and_then(Value::as_array)
        .map(|events| events.len() as u64)
        .unwrap_or(0)
        + 1;
    let receipt_row_count = result
        .pointer("/receipt/row_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    json!({
        "status": "ok",
        "runtime": runtime,
        "engine": engine,
        "summary": "FORMMAX capacity fixture filled and verified through official ServoShell bridge",
        "same_webview_control": true,
        "page_revision": page_revision,
        "site_policy": site_policy,
        "rows": rows,
        "pages": pages,
        "filled": filled,
        "blocked_sensitive": blocked_sensitive,
        "receipt_verified": receipt_verified,
        "validation_errors": validation_errors,
        "replay_events": replay_events,
        "receipt": {
            "row_count": receipt_row_count,
            "validation": result.pointer("/receipt/validation").cloned().unwrap_or(Value::Null),
            "sensitive_fields_present": result
                .pointer("/receipt/sensitive_fields_present")
                .cloned()
                .unwrap_or(Value::Null),
        },
        "policy": {
            "block_sensitive": true,
            "local_fixture_only": true,
            "same_live_tab": true,
            "values_logged": false,
        },
        "artifacts": bridge_artifacts(state),
    })
}

fn bridge_action_matches(action: &Value, action_id: &str) -> bool {
    action.get("action_id").and_then(Value::as_str) == Some(action_id)
        || action.get("id").and_then(Value::as_str) == Some(action_id)
}

fn bridge_action_id(action: &Value) -> String {
    action
        .get("action_id")
        .and_then(Value::as_str)
        .or_else(|| action.get("id").and_then(Value::as_str))
        .unwrap_or("unknown_action")
        .to_string()
}

fn bridge_dom_revision(truth: &Value) -> Value {
    truth
        .pointer("/page/revision")
        .cloned()
        .unwrap_or(Value::Null)
}

fn bridge_truth_changed(before: &Value, after: &Value) -> bool {
    bridge_dom_revision(before) != bridge_dom_revision(after)
        || before.pointer("/page/body_text_length") != after.pointer("/page/body_text_length")
        || before.get("actions") != after.get("actions")
}

fn bridge_control_call(
    endpoint: &BridgeControlEndpoint,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value> {
    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .with_context(|| format!("connect bridge control {}", endpoint.display_addr()))?;
    stream
        .set_read_timeout(Some(timeout))
        .context("set bridge control read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .context("set bridge control write timeout")?;
    writeln!(
        stream,
        "{}",
        json!({
            "id": 1,
            "method": method,
            "params": params,
        })
    )
    .with_context(|| format!("write bridge control {method}"))?;
    stream.flush().context("flush bridge control request")?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .with_context(|| format!("read bridge control {method} response"))?;
    let response: Value = serde_json::from_str(&line)
        .with_context(|| format!("parse bridge control {method} response"))?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        let error = response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("bridge control request failed");
        bail!("{error}");
    }
    Ok(response.get("result").cloned().unwrap_or(Value::Null))
}

fn write_bridge_grant(
    path: &Path,
    url: &str,
    title: Option<&str>,
    endpoint: &BridgeControlEndpoint,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let payload = json!({
        "status": "granted",
        "runtime": "saccade-servoshell-bridge-v0",
        "grant_type": "current_tab_copilot",
        "selected_tab_seen": true,
        "grant_required": true,
        "grant_given": true,
        "owner": "Human",
        "read_grant": "FullTruth",
        "agent_input_grant": true,
        "copilot": bridge_copilot_state(),
        "url": url,
        "title": title,
        "rendering_profile": "official-servoshell",
        "mcp_tool": "saccade.tabs.grant_current",
        "control_endpoint": bridge_endpoint_json(endpoint),
        "transport_status": "official_servoshell_bridge_control_v0",
        "note": "MCP v0 can call saccade.tabs.grant_current with this artifact. This bridge supports ping, shell_status, truth, actions, article_text, safe non-sensitive fill/inspect/act, local FORMMAX fill, navigate, reload, back, and forward through official ServoShell WebDriver.",
        "written_unix_ms": unix_ms(),
    });
    write_json(path, &payload)
}

fn bridge_endpoint_json(endpoint: &BridgeControlEndpoint) -> Value {
    json!({
        "protocol": endpoint.protocol,
        "scheme": "tcp",
        "host": endpoint.host,
        "port": endpoint.port,
    })
}

fn wait_for_native_input_ready(
    client: &WebDriverClient,
    session_id: &str,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let mut last = Value::Null;
    while Instant::now() < deadline {
        last = client.execute_sync(
            session_id,
            "return Boolean(window.__NATIVE_INPUT_PROBE && window.__NATIVE_SELECT_PROBE && document.getElementById('probe') && document.getElementById('choice'));",
        )?;
        if last.as_bool() == Some(true) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    bail!("native input fixture did not become ready: {last}");
}

fn native_input_probe(client: &WebDriverClient, session_id: &str) -> Result<Value> {
    parse_json_probe(
        client.execute_sync(session_id, "return window.__NATIVE_INPUT_PROBE();")?,
        "native input probe",
    )
}

fn native_select_probe(client: &WebDriverClient, session_id: &str) -> Result<Value> {
    parse_json_probe(
        client.execute_sync(session_id, "return window.__NATIVE_SELECT_PROBE();")?,
        "native select probe",
    )
}

fn parse_json_probe(value: Value, context: &str) -> Result<Value> {
    if let Some(text) = value.as_str() {
        serde_json::from_str(text).with_context(|| format!("parse {context} JSON"))
    } else if value.is_object() {
        Ok(value)
    } else {
        bail!("{context} returned non-object value: {value}");
    }
}

fn probe_count(probe: &Value, event_type: &str) -> u64 {
    probe
        .get("counts")
        .and_then(|counts| counts.get(event_type))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn native_input_report_probe(probe: &Value) -> Value {
    let value_length = probe
        .get("value")
        .and_then(Value::as_str)
        .map(|value| value.chars().count())
        .unwrap_or(0);
    json!({
        "ready": probe.get("ready").cloned().unwrap_or(Value::Null),
        "activeId": probe.get("activeId").cloned().unwrap_or(Value::Null),
        "focused": probe.get("focused").cloned().unwrap_or(Value::Null),
        "valueLength": value_length,
        "rect": probe.get("rect").cloned().unwrap_or(Value::Null),
        "counts": probe.get("counts").cloned().unwrap_or(Value::Null),
        "event_count": probe.get("events").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
        "selectionStart": probe.get("selectionStart").cloned().unwrap_or(Value::Null),
        "selectionEnd": probe.get("selectionEnd").cloned().unwrap_or(Value::Null),
        "echo_values": false,
    })
}

fn native_select_report_probe(probe: &Value) -> Value {
    json!({
        "ready": probe.get("ready").cloned().unwrap_or(Value::Null),
        "activeId": probe.get("activeId").cloned().unwrap_or(Value::Null),
        "focused": probe.get("focused").cloned().unwrap_or(Value::Null),
        "value": probe.get("value").cloned().unwrap_or(Value::Null),
        "selectedIndex": probe.get("selectedIndex").cloned().unwrap_or(Value::Null),
        "rect": probe.get("rect").cloned().unwrap_or(Value::Null),
        "counts": probe.get("counts").cloned().unwrap_or(Value::Null),
        "event_count": probe.get("events").and_then(Value::as_array).map(Vec::len).unwrap_or_default(),
    })
}

fn write_native_input_replay(
    path: &Path,
    chars_requested: usize,
    input_probe: &Value,
    keydown_events: u64,
    input_events: u64,
    keyup_events: u64,
    select_value: &str,
    select_input_events: u64,
    select_change_events: u64,
    select_method: &str,
) -> Result<()> {
    let value_length = input_probe
        .get("value")
        .and_then(Value::as_str)
        .map(|value| value.chars().count())
        .unwrap_or(0);
    let mut file = fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    let text_event = json!({
        "kind": "native_text_typed",
        "engine": "saccade-servoshell-native-input-v0",
        "runtime": "official_servoshell_webdriver",
        "method": "webdriver_element_value",
        "chars_requested": chars_requested,
        "value_length": value_length,
        "counts": {
            "keydown": keydown_events,
            "input": input_events,
            "keyup": keyup_events
        },
        "echo_values": false,
    });
    let select_event = json!({
        "kind": "native_select_changed",
        "engine": "saccade-servoshell-native-input-v0",
        "runtime": "official_servoshell_webdriver",
        "method": select_method,
        "selected_value": select_value,
        "counts": {
            "input": select_input_events,
            "change": select_change_events
        },
        "echo_text_values": false,
    });
    writeln!(file, "{}", serde_json::to_string(&text_event)?)
        .with_context(|| format!("write {}", path.display()))?;
    writeln!(file, "{}", serde_json::to_string(&select_event)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn native_input_values_absent(report: &Value, replay_path: &Path, text: &str) -> Result<Value> {
    let replay_text = fs::read_to_string(replay_path)
        .with_context(|| format!("read {}", replay_path.display()))?;
    let report_text = serde_json::to_string(report)?;
    let leaked = report_text.contains(text) || replay_text.contains(text);
    Ok(json!({
        "passed": !leaked,
        "typed_text_logged": leaked,
        "values_logged": false,
    }))
}

fn wait_for_focused_type_preflight(
    client: &WebDriverClient,
    session_id: &str,
    timeout: Duration,
) -> Result<Value> {
    let deadline = Instant::now() + timeout;
    let mut last = Value::Null;
    while Instant::now() < deadline {
        last = client.execute_sync(session_id, TYPE_FOCUSED_PROBE_JS)?;
        let reason = last.get("reason").and_then(Value::as_str);
        if last.get("ok").and_then(Value::as_bool) == Some(true)
            || reason == Some("focused_field_sensitive")
        {
            return Ok(last);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    bail!("focused type target did not become ready: {last}");
}

fn focused_type_report_probe(probe: &Value) -> Value {
    json!({
        "ok": probe.get("ok").and_then(Value::as_bool).unwrap_or(false),
        "reason": probe.get("reason").cloned().unwrap_or(Value::Null),
        "tag": probe.get("tag").cloned().unwrap_or(Value::Null),
        "type": probe.get("type").cloned().unwrap_or(Value::Null),
        "contentEditable": probe.get("contentEditable").cloned().unwrap_or(Value::Null),
        "idPresent": probe.get("idPresent").cloned().unwrap_or(Value::Null),
        "namePresent": probe.get("namePresent").cloned().unwrap_or(Value::Null),
        "selector_hash": probe.get("selector_hash").cloned().unwrap_or(Value::Null),
        "sensitivity": probe.get("sensitivity").cloned().unwrap_or(Value::Null),
        "valueLength": probe.get("valueLength").cloned().unwrap_or(Value::Null),
    })
}

fn focused_type_changed(before: &Value, after: &Value, requested_chars: usize) -> bool {
    let before_length = before
        .get("valueLength")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let after_length = after
        .get("valueLength")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    after.get("ok").and_then(Value::as_bool) == Some(true)
        && after_length >= before_length.saturating_add(requested_chars as u64)
}

fn write_focused_type_replay(
    path: &Path,
    kind: &str,
    chars_requested: usize,
    before: &Value,
    after: &Value,
    method: &str,
) -> Result<()> {
    let before_length = before
        .get("valueLength")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let after_length = after
        .get("valueLength")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let event = json!({
        "kind": kind,
        "engine": "saccade-servoshell-focused-type-v0",
        "runtime": "official_servoshell_webdriver",
        "method": method,
        "chars_requested": chars_requested,
        "field": {
            "tag": before.get("tag").cloned().unwrap_or(Value::Null),
            "type": before.get("type").cloned().unwrap_or(Value::Null),
            "contentEditable": before.get("contentEditable").cloned().unwrap_or(Value::Null),
            "idPresent": before.get("idPresent").cloned().unwrap_or(Value::Null),
            "namePresent": before.get("namePresent").cloned().unwrap_or(Value::Null),
            "selector_hash": before.get("selector_hash").cloned().unwrap_or(Value::Null),
            "sensitivity": before.get("sensitivity").cloned().unwrap_or(Value::Null),
        },
        "before_length": before_length,
        "after_length": after_length,
        "changed": after_length > before_length,
        "echo_values": false,
    });
    let mut file = fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    writeln!(file, "{}", serde_json::to_string(&event)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn focused_type_values_absent(report: &Value, replay_path: &Path, text: &str) -> Result<Value> {
    let replay_text = fs::read_to_string(replay_path)
        .with_context(|| format!("read {}", replay_path.display()))?;
    let report_text = serde_json::to_string(report)?;
    let leaked = report_text.contains(text) || replay_text.contains(text);
    Ok(json!({
        "passed": !leaked,
        "typed_text_logged": leaked,
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
            let response = if relative.contains("..") {
                Response::from_string("not found").with_status_code(StatusCode(404))
            } else {
                let path = root.join(relative);
                match fs::read(&path) {
                    Ok(body) => Response::from_data(body).with_header(
                        Header::from_bytes("Content-Type", content_type(&path)).unwrap(),
                    ),
                    Err(_) => Response::from_string("not found").with_status_code(StatusCode(404)),
                }
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

fn default_run_dir(prefix: &str) -> PathBuf {
    PathBuf::from("runs")
        .join("servoshell_adapter")
        .join(format!("{prefix}_{}", unix_ms()))
}

fn default_servoshell_bridge_grant_path() -> PathBuf {
    PathBuf::from("runs")
        .join("current_tab_grants")
        .join("servoshell_latest.json")
}

fn workspace_path(path: &str) -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(path)
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

fn default_focused_type_url() -> String {
    file_url("test_pages/focused_type/index.html")
}

fn default_focused_contenteditable_url() -> String {
    file_url("test_pages/focused_contenteditable/index.html")
}

fn default_focused_sensitive_url() -> String {
    file_url("test_pages/focused_sensitive/index.html")
}

fn default_native_input_url() -> String {
    file_url("test_pages/native_input/index.html")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_report_extracts_apple_style_request_id() {
        let text = "We can't process your request.\nGo back and try again.\n0fa693e0-e6ef-425f-91e3-05fdac5581d7";
        assert_eq!(
            extract_visible_request_id(text).as_deref(),
            Some("0fa693e0-e6ef-425f-91e3-05fdac5581d7")
        );
        let excerpt = visible_block_excerpt(text);
        assert!(excerpt.to_string().contains("can't process"));
        assert!(excerpt.to_string().contains("0fa693e0-e6ef"));
    }

    #[test]
    fn block_report_redacts_url_and_obvious_values() {
        let url = redacted_url_for_block_report(
            "https://appstoreconnect.apple.com/apps?token=secret#fragment",
        );
        assert_eq!(url.as_str(), Some("https://appstoreconnect.apple.com/apps"));
        let text = redact_block_text(
            "contact wayne@example.com card 4242424242424242 https://example.com/path?token=secret",
            240,
        );
        assert!(text.contains("[redacted-email]"));
        assert!(text.contains("[redacted-number]"));
        assert!(text.contains("https://example.com/path"));
        assert!(!text.contains("token=secret"));
    }

    #[test]
    fn servoshell_error_page_is_not_success_ready() {
        let value = json!({
            "title": "Error loading page",
            "readyState": "interactive",
            "url": "http://10.0.0.148:3000/demo/shimmer-ai-story?memory-pack=1"
        });
        let error = fail_if_servoshell_error_page(&value).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("ServoShell reached its internal error page")
        );
    }
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

const LOGIN_HANDOFF_PROBE_JS: &str = r###"
return (() => {
  const fieldState = (el) => {
    const value = el ? String(el.value || "") : "";
    return value.length > 0 ? "present_redacted" : "empty";
  };
  const sensitive = Array.from(document.querySelectorAll("#password,#otp,input[type='password'],input[autocomplete='one-time-code']")).map((el) => ({
    id: el.id || "",
    type: (el.getAttribute("type") || "").toLowerCase(),
    autocomplete: el.getAttribute("autocomplete") || "",
    value: null,
    value_state: fieldState(el),
    masked: true
  }));
  return {
    title: document.title,
    url: document.URL,
    text: document.body ? document.body.innerText : "",
    cookie_present: document.cookie.includes("saccade_session=demo"),
    storage_shared: localStorage.getItem("saccade_storage") === "shared",
    handoffDone: localStorage.getItem("saccade_handoff_done") === "true",
    sensitive_fields: sensitive,
    credential_values_exposed: {
      password: false,
      otp: false
    },
    credentials_echoed: false
  };
})();
"###;

const LOGIN_HANDOFF_HUMAN_LOGIN_JS: &str = r###"
return (() => {
  const username = document.getElementById("username");
  const password = document.getElementById("password");
  const otp = document.getElementById("otp");
  if (username) username.value = "wayne";
  if (password) password.value = "human-only-password";
  if (otp) otp.value = "123456";
  const form = document.getElementById("login-form");
  if (form) {
    if (typeof form.requestSubmit === "function") {
      form.requestSubmit();
    } else {
      form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    }
  }
  return {
    ok: Boolean(form),
    submitted: Boolean(form),
    credentials_echoed: false
  };
})();
"###;

const ARTICLE_TEXT_JS: &str = r###"
return (() => {
  const maxChars = Math.max(1000, Math.min(100000, Number(arguments[0] || 20000)));
  const normalize = (text) => String(text || "")
    .replace(/\r/g, "\n")
    .replace(/[ \t\f\v]+/g, " ")
    .replace(/\n[ \t]+/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
  const textOf = (el) => normalize(el ? (el.innerText || el.textContent || "") : "");
  const visible = (el) => {
    if (!el || !el.getBoundingClientRect) return false;
    const rect = el.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return false;
    for (let cur = el; cur && cur.nodeType === 1; cur = cur.parentElement) {
      const cs = getComputedStyle(cur);
      if (cur.hidden || cur.getAttribute("aria-hidden") === "true") return false;
      if (cs.display === "none" || cs.visibility === "hidden" || cs.visibility === "collapse") return false;
    }
    return true;
  };
  const selectorFor = (el) => {
    if (!el || !el.tagName) return "unknown";
    if (el.id) return `${el.tagName.toLowerCase()}#${el.id}`;
    const cls = el.classList && el.classList.length ? "." + Array.from(el.classList).slice(0, 3).join(".") : "";
    return `${el.tagName.toLowerCase()}${cls}`;
  };
  const penaltyFor = (el) => {
    const token = [
      el.id || "",
      el.className || "",
      el.getAttribute("role") || "",
      el.tagName || ""
    ].join(" ").toLowerCase();
    let penalty = 0;
    if (/(nav|menu|sidebar|footer|header|cookie|banner|ad-|advert|promo|related|comment|share|social)/.test(token)) penalty += 1200;
    if (["NAV", "FOOTER", "HEADER", "ASIDE"].includes(el.tagName)) penalty += 1500;
    return penalty;
  };
  const scoreCandidate = (el, preferred) => {
    if (!visible(el)) return null;
    const text = textOf(el);
    const len = text.length;
    if (len < 120) return null;
    const paragraphCount = el.querySelectorAll ? el.querySelectorAll("p, li").length : 0;
    const headingCount = el.querySelectorAll ? el.querySelectorAll("h1,h2,h3").length : 0;
    const linkText = Array.from(el.querySelectorAll ? el.querySelectorAll("a") : [])
      .map((a) => a.innerText || a.textContent || "")
      .join(" ")
      .length;
    const linkPenalty = len ? Math.floor((linkText / len) * 1200) : 0;
    const score = len + paragraphCount * 120 + headingCount * 180 + (preferred ? 3000 : 0) - penaltyFor(el) - linkPenalty;
    return { el, text, score, selector: selectorFor(el), preferred };
  };
  const candidates = [];
  const add = (el, preferred = false) => {
    const item = scoreCandidate(el, preferred);
    if (item && !candidates.some((existing) => existing.el === item.el)) candidates.push(item);
  };
  for (const selector of [
    "article",
    "main",
    "[role='main']",
    ".post-content",
    ".entry-content",
    ".article-content",
    ".blog-post",
    ".breakdown-content",
    ".content"
  ]) {
    document.querySelectorAll(selector).forEach((el) => add(el, true));
  }
  document.querySelectorAll("section, div").forEach((el) => {
    const text = textOf(el);
    if (text.length >= 800 && el.querySelectorAll("p, li").length >= 3) add(el, false);
  });
  add(document.body, false);
  candidates.sort((a, b) => b.score - a.score);
  const best = candidates[0] || { el: document.body, text: textOf(document.body), score: 0, selector: "body", preferred: false };
  const headings = Array.from((best.el || document).querySelectorAll("h1,h2,h3"))
    .map((el) => normalize(el.innerText || el.textContent || ""))
    .filter(Boolean)
    .slice(0, 20);
  const fullText = best.text;
  const returned = fullText.slice(0, maxChars);
  return {
    url: document.URL,
    title: document.title,
    readyState: document.readyState,
    bodyTextLength: document.body ? textOf(document.body).length : 0,
    articleTextLength: fullText.length,
    text: returned,
    textTruncated: fullText.length > returned.length,
    mode: best.preferred ? "semantic_container" : "scored_container",
    selector: best.selector,
    score: best.score,
    candidateCount: candidates.length,
    headings
  };
})();
"###;

const TRUTH_JS: &str = r###"
return (() => {
  const VERSION = "saccade-servoshell-truth-v0";
  const sensitiveRe = /(password|passcode|pwd|ssn|social|credit|card|cc-|cvv|cvc|otp|token|secret|passport|license|dob|birth|email|e-mail|phone|government|national|identity|tax|tin)/i;
  const visible = (el) => {
    if (!el || !el.getBoundingClientRect) return false;
    for (let cur = el; cur && cur.nodeType === 1; cur = cur.parentElement) {
      const cs = getComputedStyle(cur);
      const classes = cur.classList ? Array.from(cur.classList) : [];
      if (classes.some((name) => /^(hidden|is-hidden|visually-hidden|sr-only)$/.test(name))) return false;
      if (cur.hidden || cur.getAttribute("aria-hidden") === "true") return false;
      if (cs.display === "none" || cs.visibility === "hidden" || cs.visibility === "collapse") return false;
    }
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
  const slug = (s) => String(s || "").toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_+|_+$/g, "").slice(0, 48);
  const actionIdFor = (el, label, index) => {
    const tag = el.tagName.toLowerCase();
    const type = (el.getAttribute("type") || "").toLowerCase();
    const role = (el.getAttribute("role") || "").toLowerCase();
    const text = String(label || el.getAttribute("aria-label") || el.getAttribute("value") || "").trim();
    const key = slug(text || el.id || el.getAttribute("name") || tag || ("action_" + index));
    if (tag === "button" || type === "submit" || type === "button" || role === "button") {
      if (type === "submit" || /^(submit|send|continue|finish|done)$/i.test(text)) return "act_submit";
      if (/delete|remove|discard/i.test(text)) return "act_delete";
      if (/export|download/i.test(text)) return "act_export";
      return "act_" + (key || ("button_" + index));
    }
    if (tag === "a") return "act_link_" + (key || index);
    return "field_" + (el.id || el.getAttribute("name") || index);
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
    const label = labelFor(el, isSensitive);
    const actionId = actionIdFor(el, label, actions.length);
    actions.push({
      id: actionId,
      action_id: actionId,
      raw_id: "a_" + actions.length,
      kind: tag === "a" || role === "button" ? "click" : "field",
      role,
      selector: sel,
      label,
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
      url: document.URL,
      origin: (() => { try { return new URL(document.URL).origin; } catch (_) { return ""; } })(),
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

const BRIDGE_FILL_AGENT_FIELDS_JS: &str = r###"
return (() => {
  const requested = arguments[0] || {};
  const filled = [];
  const rejected = [];

  function sensitivityOf(el) {
    const token = [
      el.getAttribute("data-sensitive") || "",
      el.getAttribute("autocomplete") || "",
      el.getAttribute("name") || "",
      el.id || "",
      el.getAttribute("type") || ""
    ].join(" ").toLowerCase();
    const type = (el.getAttribute("type") || "").toLowerCase();
    if (type === "password" || /\b(password|passcode)\b/.test(token)) return "password";
    if (/\b(otp|one-time|totp|2fa|mfa)\b/.test(token)) return "otp";
    if (/\b(ssn|social security|tax id|tax_id|tin|ein)\b/.test(token)) return "government_or_tax_id";
    if (/\b(credit|card|cc-number|cc-csc|cvv|cvc|payment)\b/.test(token)) return "payment";
    if (/\b(signature|attestation|legal_attestation|esign|e-sign)\b/.test(token)) return "legal_attestation";
    return "none";
  }

  for (const [id, value] of Object.entries(requested)) {
    const el = document.getElementById(id);
    if (!el) {
      rejected.push({ id, reason: "not_found" });
      continue;
    }
    const owner = el.getAttribute("data-owner") || "";
    const declaredSensitivity = el.getAttribute("data-sensitive") || "none";
    const sensitivity = sensitivityOf(el);
    if (owner !== "agent" || declaredSensitivity !== "none" || sensitivity !== "none") {
      rejected.push({ id, reason: "not_agent_owned_non_sensitive", owner, sensitivity });
      continue;
    }
    if (el.type === "checkbox") {
      el.checked = Boolean(value);
    } else {
      el.value = String(value);
    }
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
    filled.push(id);
  }

  const body = document.body;
  const previousRevision = Number(body && body.dataset ? (body.dataset.sessionRevision || "0") : "0") || 0;
  if (filled.length && body && body.dataset) {
    body.dataset.sessionRevision = String(previousRevision + 1);
  }
  const sensitiveFieldsSeen = Array.from(document.querySelectorAll("input, select, textarea"))
    .filter((el) => sensitivityOf(el) !== "none" || (el.getAttribute("data-sensitive") || "none") !== "none")
    .length;
  return {
    filled,
    rejected,
    pageRevision: body && body.dataset ? Number(body.dataset.sessionRevision || "0") || 0 : 0,
    sensitiveFieldsSeen
  };
})();
"###;

const BRIDGE_INSPECT_FIELDS_JS: &str = r###"
return (() => {
  const requested = arguments[0] || [];
  const fields = [];

  function sensitivityOf(el) {
    const token = [
      el.getAttribute("data-sensitive") || "",
      el.getAttribute("autocomplete") || "",
      el.getAttribute("name") || "",
      el.id || "",
      el.getAttribute("type") || ""
    ].join(" ").toLowerCase();
    const type = (el.getAttribute("type") || "").toLowerCase();
    if (type === "password" || /\b(password|passcode)\b/.test(token)) return "password";
    if (/\b(otp|one-time|totp|2fa|mfa)\b/.test(token)) return "otp";
    if (/\b(ssn|social security|tax id|tax_id|tin|ein)\b/.test(token)) return "government_or_tax_id";
    if (/\b(credit|card|cc-number|cc-csc|cvv|cvc|payment)\b/.test(token)) return "payment";
    if (/\b(signature|attestation|legal_attestation|esign|e-sign)\b/.test(token)) return "legal_attestation";
    return "none";
  }

  function fieldValue(el) {
    if (el.type === "checkbox") return Boolean(el.checked);
    return String(el.value || "");
  }

  function hasValue(el) {
    if (el.type === "checkbox") return Boolean(el.checked);
    return String(el.value || "").trim().length > 0;
  }

  for (const id of requested) {
    const el = document.getElementById(id);
    if (!el) {
      fields.push({ id, status: "not_found" });
      continue;
    }
    const owner = el.getAttribute("data-owner") || "";
    const declaredSensitivity = el.getAttribute("data-sensitive") || "none";
    const sensitivity = sensitivityOf(el);
    const completionState = sensitivity === "none" && declaredSensitivity === "none"
      ? (hasValue(el) ? "value_present" : "empty")
      : (hasValue(el) ? "completed_without_value" : "requires_user_input");
    const record = {
      id,
      status: "ok",
      owner,
      declared_sensitivity: declaredSensitivity,
      sensitivity,
      completion_state: completionState
    };
    if (sensitivity === "none" && declaredSensitivity === "none") {
      record.value = fieldValue(el);
      record.value_returned = true;
    } else {
      record.value_redacted = true;
    }
    fields.push(record);
  }

  const sensitiveFieldsSeen = Array.from(document.querySelectorAll("input, select, textarea"))
    .filter((el) => sensitivityOf(el) !== "none" || (el.getAttribute("data-sensitive") || "none") !== "none")
    .length;
  return { fields, sensitiveFieldsSeen };
})();
"###;

const TYPE_FOCUSED_PROBE_JS: &str = r###"
return (() => {
  const el = document.activeElement;
  if (!el || el === document.body || el === document.documentElement) {
    return { ok: false, reason: "no_focused_field" };
  }

  const stableHash = (s) => {
    let h = 2166136261;
    for (let i = 0; i < s.length; i++) {
      h ^= s.charCodeAt(i);
      h = Math.imul(h, 16777619);
    }
    return ("00000000" + (h >>> 0).toString(16)).slice(-8);
  };
  const cssIdent = (s) => String(s).replace(/[^a-zA-Z0-9_-]/g, (c) => "\\" + c.charCodeAt(0).toString(16) + " ");
  const selectorFor = (el) => {
    if (el.id) return "#" + cssIdent(el.id);
    const name = el.getAttribute("name");
    if (name) return el.tagName.toLowerCase() + "[name=\"" + String(name).replace(/"/g, "\\\"") + "\"]";
    return el.tagName.toLowerCase();
  };
  const sensitivityOf = (el) => {
    const labelText = Array.from(el.labels || []).map((label) => label.textContent || "").join(" ");
    const token = [
      el.getAttribute("data-sensitive") || "",
      el.getAttribute("autocomplete") || "",
      el.getAttribute("aria-label") || "",
      el.getAttribute("placeholder") || "",
      el.getAttribute("name") || "",
      el.id || "",
      el.getAttribute("type") || "",
      labelText
    ].join(" ").toLowerCase();
    const type = (el.getAttribute("type") || "").toLowerCase();
    if (type === "password" || /\b(password|passcode)\b/.test(token)) return "password";
    if (/\b(otp|one-time|totp|2fa|mfa)\b/.test(token)) return "otp";
    if (/\b(ssn|social security|tax id|tax_id|tin|ein|passport|driver.?license)\b/.test(token)) return "government_or_tax_id";
    if (/\b(credit|card|cc-number|cc-csc|cvv|cvc|payment|routing|bank)\b/.test(token)) return "payment";
    if (/\b(signature|attestation|legal_attestation|esign|e-sign)\b/.test(token)) return "legal_attestation";
    return "none";
  };
  const writableKind = (el) => {
    const tag = el.tagName ? el.tagName.toLowerCase() : "";
    const type = (el.getAttribute("type") || "text").toLowerCase();
    if (tag === "textarea") return "textarea";
    if (tag === "input") {
      const allowed = new Set(["text", "search", "url", "email", "tel", "number"]);
      return allowed.has(type) ? "input" : "";
    }
    if (el.isContentEditable) return "contenteditable";
    return "";
  };
  const valueLength = (el, kind) => kind === "contenteditable"
    ? String(el.textContent || "").length
    : String(el.value || "").length;
  const selector = selectorFor(el);
  const sensitivity = sensitivityOf(el);
  if (sensitivity !== "none") {
    return {
      ok: false,
      reason: "focused_field_sensitive",
      sensitivity,
      selector_hash: stableHash(selector)
    };
  }
  const kind = writableKind(el);
  if (!kind) {
    return {
      ok: false,
      reason: "focused_element_not_text_writable",
      tag: el.tagName ? el.tagName.toLowerCase() : "",
      type: (el.getAttribute("type") || "").toLowerCase(),
      selector_hash: stableHash(selector)
    };
  }
  return {
    ok: true,
    tag: el.tagName ? el.tagName.toLowerCase() : "",
    type: (el.getAttribute("type") || "").toLowerCase(),
    contentEditable: Boolean(el.isContentEditable),
    idPresent: Boolean(el.id),
    namePresent: Boolean(el.getAttribute("name")),
    selector,
    selector_hash: stableHash(selector),
    sensitivity,
    valueLength: valueLength(el, kind)
  };
})();
"###;

const TYPE_FOCUSED_CONTENTEDITABLE_INSERT_JS: &str = r###"
return (() => {
  const text = String(arguments[0] || "");
  const el = document.activeElement;
  if (!el || !el.isContentEditable) {
    return { ok: false, reason: "focused_element_not_contenteditable" };
  }
  if (typeof document.execCommand === "function") {
    document.execCommand("insertText", false, text);
  } else {
    el.textContent = String(el.textContent || "") + text;
    el.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: text }));
  }
  return (() => {
    const previous = document.activeElement;
    return {
      ok: true,
      tag: previous.tagName ? previous.tagName.toLowerCase() : "",
      type: (previous.getAttribute("type") || "").toLowerCase(),
      contentEditable: Boolean(previous.isContentEditable),
      idPresent: Boolean(previous.id),
      namePresent: Boolean(previous.getAttribute("name")),
      sensitivity: "none",
      valueLength: String(previous.textContent || "").length
    };
  })();
})();
"###;

const NATIVE_SELECT_SET_JS: &str = r###"
return (() => {
  const value = String(arguments[0] || "");
  const select = document.getElementById("choice");
  if (!select) {
    return { ready: false, reason: "missing_select" };
  }
  select.focus();
  select.value = value;
  select.dispatchEvent(new Event("input", { bubbles: true }));
  select.dispatchEvent(new Event("change", { bubbles: true }));
  return JSON.parse(window.__NATIVE_SELECT_PROBE());
})();
"###;

const FORMMAX_FIELD_TRUTH_JS: &str = r###"
return (() => {
  const visible = (el) => {
    if (!el || !el.getBoundingClientRect) return false;
    for (let cur = el; cur && cur.nodeType === 1; cur = cur.parentElement) {
      const cs = getComputedStyle(cur);
      const classes = cur.classList ? Array.from(cur.classList) : [];
      if (classes.some((name) => /^(hidden|is-hidden|visually-hidden|sr-only)$/.test(name))) return false;
      if (cur.hidden || cur.getAttribute("aria-hidden") === "true") return false;
      if (cs.display === "none" || cs.visibility === "hidden" || cs.visibility === "collapse") return false;
    }
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
