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
            })?;

            let sensitive = run_probe(ProbeConfig {
                servoshell,
                url: default_sensitive_url(),
                output_dir: root.join("sensitive"),
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
}

#[derive(Debug)]
struct ProbeOutcome {
    ok: bool,
    report_path: PathBuf,
    screenshot_path: Option<PathBuf>,
    screenshot_decision: String,
    post_revision: Option<String>,
    leak_check_passed: bool,
    webdriver_port: u16,
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
                        "browserName": "servo"
                    }
                }
            })),
        )
        .map(|response| response.body)
    }

    fn execute_sync(&self, session_id: &str, script: &str) -> Result<Value> {
        let response = self.request(
            "POST",
            &format!("/session/{session_id}/execute/sync"),
            Some(json!({"script": script, "args": []})),
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
            .set_read_timeout(Some(self.timeout.min(Duration::from_secs(10))))
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

fn default_sensitive_url() -> String {
    file_url("test_pages/browser_session_sensitive/index.html")
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
  const sensitiveRe = /(password|passcode|pwd|ssn|social|credit|card|cc-|cvv|cvc|otp|token|secret|passport|license|dob|birth|email|e-mail|phone)/i;
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
  const labelFor = (el) => {
    const aria = el.getAttribute("aria-label");
    if (aria) return aria.trim().slice(0, 80);
    const id = el.id;
    if (id) {
      const label = document.querySelector("label[for=\"" + CSS.escape(id) + "\"]");
      if (label) return label.textContent.trim().replace(/\s+/g, " ").slice(0, 80);
    }
    const parentLabel = el.closest("label");
    if (parentLabel) return parentLabel.textContent.trim().replace(/\s+/g, " ").slice(0, 80);
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
      label: labelFor(el),
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
