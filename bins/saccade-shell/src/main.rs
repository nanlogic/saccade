use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::thread;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use tiny_http::{Header, Response, Server, StatusCode};
use url::Url;

#[derive(Parser)]
#[command(name = "saccade-shell")]
#[command(about = "Saccade trusted tab shell")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    SelftestTabs,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::SelftestTabs => selftest_tabs(),
    }
}

fn selftest_tabs() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("login_handoff"))?;
    let profile = saccade_browser::selftest_trusted_tabs(base_url)?;

    if !profile.input_isolated || !profile.read_policy_enforced || profile.webviews != 2 {
        bail!("trusted tabs selftest failed: {profile:?}");
    }

    println!(
        "TABS PASS webviews={} cookie_shared={} storage_shared={} input_isolated={} read_policy_enforced={}",
        profile.webviews,
        profile.cookie_shared,
        profile.storage_shared,
        profile.input_isolated,
        profile.read_policy_enforced,
    );
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
            let response = match std::fs::read(&path) {
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

fn workspace_root() -> Result<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .context("failed to resolve workspace root")
}
