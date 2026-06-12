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

    let base_url = start_test_server(fixture_dir)?;
    let url = base_url
        .join(fixture_file)
        .with_context(|| format!("failed to build fixture URL from {fixture_file}"))?;
    let mut report = saccade_browser::run_formmax_fixture(url.clone())?;

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

    let run_id = format!("run_{}", unix_ms()?);
    let output_dir = workspace.join("runs").join("formmax").join(&run_id);
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let result_path = output_dir.join("result.json");
    let replay_path = output_dir.join("replay.jsonl");

    let mut report_value = serde_json::to_value(&report)?;
    report_value["run_id"] = json!(run_id);
    report_value["url"] = json!(url.to_string());
    report_value["artifacts"] = json!({
        "result": result_path.display().to_string(),
        "replay": if replay { Value::String(replay_path.display().to_string()) } else { Value::Null },
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

fn write_replay(path: &Path, report: &mut saccade_browser::FormmaxRunReport) -> Result<()> {
    let mut file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    for event in report.events.drain(..) {
        writeln!(file, "{}", event)?;
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
