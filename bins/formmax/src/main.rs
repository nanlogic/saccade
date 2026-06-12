use std::fs::{self, File};
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use serde_json::{Value, json};
use tiny_http::{Header, Response, Server, StatusCode};
use url::Url;

#[derive(Parser)]
#[command(name = "formmax")]
#[command(about = "Saccade practical form workflow harness")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Run {
        #[arg(long)]
        fixture: PathBuf,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        replay: bool,
    },
    ValidateRun {
        run_dir: PathBuf,
    },
}

#[derive(Debug, Deserialize)]
struct Manifest {
    row_count: usize,
    pages: usize,
    columns: Vec<String>,
    sensitive_fields: Vec<Value>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run {
            fixture,
            input,
            replay,
        } => run(fixture, input, replay),
        Command::ValidateRun { run_dir } => validate_run(run_dir),
    }
}

fn run(fixture: PathBuf, input: Option<PathBuf>, replay: bool) -> Result<()> {
    let workspace = workspace_root()?;
    let fixture = absolutize(&workspace, &fixture);
    let fixture_dir = fixture
        .parent()
        .map(Path::to_path_buf)
        .context("fixture path has no parent directory")?;
    let fixture_file = fixture
        .file_name()
        .and_then(|name| name.to_str())
        .context("fixture file name is not UTF-8")?;
    let input = input.unwrap_or_else(|| fixture_dir.join("capacity_input.json"));
    let input = absolutize(&workspace, &input);
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(&input).with_context(|| format!("failed to read {}", input.display()))?,
    )
    .with_context(|| format!("failed to parse {}", input.display()))?;
    let run_id = format!("run_{}", unix_ms()?);
    let output_dir = workspace.join("runs").join("formmax").join(&run_id);
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;

    let base_url = start_test_server(fixture_dir)?;
    let url = base_url
        .join(fixture_file)
        .with_context(|| format!("failed to build fixture URL from {fixture_file}"))?;
    let mut report =
        saccade_browser::run_formmax_fixture_with_config(saccade_browser::FormmaxRunConfig {
            url: url.clone(),
            artifact_dir: Some(output_dir.clone()),
        })?;

    let expected_filled = manifest.row_count * manifest.columns.len();
    if report.rows != manifest.row_count
        || report.pages != manifest.pages
        || report.filled != expected_filled
        || report.blocked_sensitive != manifest.sensitive_fields.len()
        || !report.receipt_verified
        || report.validation_errors != 0
    {
        bail!(
            "FORMMAX runner failed: rows={} pages={} filled={} expected_filled={} blocked_sensitive={} expected_sensitive={} receipt_verified={} validation_errors={}",
            report.rows,
            report.pages,
            report.filled,
            expected_filled,
            report.blocked_sensitive,
            manifest.sensitive_fields.len(),
            report.receipt_verified,
            report.validation_errors,
        );
    }

    let result_path = output_dir.join("result.json");
    let replay_path = output_dir.join("replay.jsonl");

    let mut report_value = serde_json::to_value(&report)?;
    report_value["run_id"] = json!(run_id);
    report_value["url"] = json!(url.to_string());
    report_value["artifacts"] = json!({
        "result": result_path.display().to_string(),
        "replay": if replay { Value::String(replay_path.display().to_string()) } else { Value::Null },
        "screenshots": report.screenshots.clone(),
    });
    fs::write(&result_path, serde_json::to_string_pretty(&report_value)?)
        .with_context(|| format!("failed to write {}", result_path.display()))?;
    if replay {
        write_replay(&replay_path, &mut report)?;
    }

    println!(
        "FORMMAX RUNNER PASS rows={} pages={} filled={} blocked_sensitive={} receipt_verified={} replay={}",
        report.rows,
        report.pages,
        report.filled,
        report.blocked_sensitive,
        report.receipt_verified,
        if replay {
            replay_path.display().to_string()
        } else {
            "disabled".to_string()
        }
    );
    Ok(())
}

fn validate_run(run_dir: PathBuf) -> Result<()> {
    let workspace = workspace_root()?;
    let display_run_dir = run_dir.display().to_string();
    let run_dir = absolutize(&workspace, &run_dir);
    let result_path = run_dir.join("result.json");
    let report: Value = serde_json::from_slice(
        &fs::read(&result_path)
            .with_context(|| format!("failed to read {}", result_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", result_path.display()))?;
    let replay_path = replay_path_from_report(&run_dir, &report);
    let replay_text = fs::read_to_string(&replay_path)
        .with_context(|| format!("failed to read {}", replay_path.display()))?;
    let events = parse_replay(&replay_text)?;

    let rows = required_usize(&report, "rows")?;
    let pages = required_usize(&report, "pages")?;
    let filled = required_usize(&report, "filled")?;
    let blocked_sensitive = required_usize(&report, "blocked_sensitive")?;
    let validation_errors = required_usize(&report, "validation_errors")?;
    let receipt_verified = report
        .get("receipt_verified")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let receipt_rows = report
        .pointer("/receipt/rows")
        .and_then(Value::as_array)
        .context("result receipt rows missing")?;
    let receipt_validation_passed = report
        .pointer("/receipt/validation/passed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let screenshots = report
        .get("screenshots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let failures = validate_replay(
        &events,
        &replay_text,
        rows,
        pages,
        filled,
        blocked_sensitive,
        receipt_rows,
    );
    if !receipt_verified {
        bail!("FORMMAX validation failed: receipt_verified=false");
    }
    if !receipt_validation_passed {
        bail!("FORMMAX validation failed: receipt.validation.passed=false");
    }
    if validation_errors != 0 {
        bail!("FORMMAX validation failed: validation_errors={validation_errors}");
    }
    if receipt_rows.len() != rows {
        bail!(
            "FORMMAX validation failed: receipt row count {} != {rows}",
            receipt_rows.len()
        );
    }
    if screenshots.len() < 2 {
        bail!(
            "FORMMAX validation failed: expected at least 2 screenshots, got {}",
            screenshots.len()
        );
    }
    for screenshot in &screenshots {
        let Some(path) = screenshot.as_str() else {
            bail!("FORMMAX validation failed: screenshot path was not a string");
        };
        if !Path::new(path).exists() {
            bail!("FORMMAX validation failed: missing screenshot {path}");
        }
    }
    if !failures.is_empty() {
        bail!(
            "FORMMAX validation failed: {}",
            failures
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        );
    }

    println!(
        "FORMMAX VALIDATION PASS run={} rows={} pages={} filled={} blocked_sensitive={} events={} screenshots={} replay_value_leaks=0",
        display_run_dir,
        rows,
        pages,
        filled,
        blocked_sensitive,
        events.len(),
        screenshots.len()
    );
    Ok(())
}

fn write_replay(path: &Path, report: &mut saccade_browser::FormmaxRunReport) -> Result<()> {
    let mut file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    for event in report.events.drain(..) {
        writeln!(file, "{}", event)?;
    }
    Ok(())
}

fn replay_path_from_report(run_dir: &Path, report: &Value) -> PathBuf {
    report
        .pointer("/artifacts/replay")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .unwrap_or_else(|| run_dir.join("replay.jsonl"))
}

fn parse_replay(replay_text: &str) -> Result<Vec<Value>> {
    replay_text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str::<Value>(line)
                .with_context(|| format!("failed to parse replay line {}", index + 1))
        })
        .collect()
}

fn validate_replay(
    events: &[Value],
    replay_text: &str,
    rows: usize,
    pages: usize,
    filled: usize,
    blocked_sensitive: usize,
    receipt_rows: &[Value],
) -> Vec<String> {
    let mut failures = Vec::new();
    let count = |kind: &str| -> usize {
        events
            .iter()
            .filter(|event| event.get("kind").and_then(Value::as_str) == Some(kind))
            .count()
    };

    require_equal(&mut failures, "page_started", count("page_started"), pages);
    require_equal(
        &mut failures,
        "field_focused",
        count("field_focused"),
        filled,
    );
    require_equal(&mut failures, "field_filled", count("field_filled"), filled);
    require_equal(
        &mut failures,
        "field_verified",
        count("field_verified"),
        filled,
    );
    require_equal(
        &mut failures,
        "confirmation_required",
        count("confirmation_required"),
        blocked_sensitive,
    );
    require_equal(
        &mut failures,
        "field_blocked_sensitive",
        count("field_blocked_sensitive"),
        blocked_sensitive,
    );
    require_at_least(
        &mut failures,
        "field_discovered",
        count("field_discovered"),
        filled + blocked_sensitive,
    );
    require_at_least(
        &mut failures,
        "scroll_checkpoint",
        count("scroll_checkpoint"),
        pages,
    );
    require_equal(&mut failures, "receipt_seen", count("receipt_seen"), 1);
    require_equal(
        &mut failures,
        "form_transaction_finished",
        count("form_transaction_finished"),
        1,
    );

    for event in events {
        if event.get("echo_values").and_then(Value::as_bool) != Some(false) {
            failures.push(format!("event echo_values was not false: {event}"));
        }
        if event.get("value").is_some() {
            failures.push(format!("event contained raw value key: {event}"));
        }
    }

    for event in events.iter().filter(|event| {
        event.get("kind").and_then(Value::as_str) == Some("field_blocked_sensitive")
    }) {
        if event.get("value_present").and_then(Value::as_bool) != Some(false) {
            failures.push(format!("sensitive field had value_present=true: {event}"));
        }
    }

    for row in receipt_rows {
        for key in ["site_name", "owner", "target_date"] {
            let Some(value) = row.get(key).and_then(Value::as_str) else {
                continue;
            };
            if !value.is_empty() && replay_text.contains(value) {
                let id = row.get("id").and_then(Value::as_str).unwrap_or("unknown");
                failures.push(format!("replay echoed {id}.{key}"));
            }
        }
    }

    if rows == 0 {
        failures.push("rows was zero".to_string());
    }
    failures
}

fn require_equal(failures: &mut Vec<String>, label: &str, actual: usize, expected: usize) {
    if actual != expected {
        failures.push(format!("{label} count {actual} != {expected}"));
    }
}

fn require_at_least(failures: &mut Vec<String>, label: &str, actual: usize, expected: usize) {
    if actual < expected {
        failures.push(format!("{label} count {actual} < {expected}"));
    }
}

fn required_usize(value: &Value, key: &str) -> Result<usize> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .with_context(|| format!("result field {key:?} missing or not an integer"))
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
        Some("json") => "application/json; charset=utf-8",
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

fn absolutize(workspace: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace.join(path)
    }
}

fn unix_ms() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_millis())
}
