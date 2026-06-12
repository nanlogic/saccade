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
    SelftestLoginHandoff,
    SelftestSafety,
    SelftestNativeInput,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::SelftestTabs => selftest_tabs(),
        Command::SelftestLoginHandoff => selftest_login_handoff(),
        Command::SelftestSafety => selftest_safety(),
        Command::SelftestNativeInput => selftest_native_input(),
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

fn selftest_login_handoff() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("login_handoff"))?;
    let profile = saccade_browser::selftest_login_handoff(base_url)?;

    if !profile.human_login
        || !profile.agent_session
        || profile.password_exposed
        || profile.otp_exposed
        || !profile.agent_input_to_human_tab_blocked
        || !profile.done_clicked
    {
        bail!("login handoff selftest failed: {profile:?}");
    }

    println!(
        "LOGIN_HANDOFF PASS human_login={} agent_session={} password_exposed={} otp_exposed={} agent_input_to_human_tab_blocked={}",
        profile.human_login,
        profile.agent_session,
        profile.password_exposed,
        profile.otp_exposed,
        profile.agent_input_to_human_tab_blocked,
    );
    Ok(())
}

fn selftest_safety() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("login_handoff"))?;
    let profile = saccade_browser::selftest_safety(base_url)?;

    if !profile.human_login
        || !profile.agent_session
        || !profile.done_clicked
        || !profile.input_isolated
        || !profile.read_policy_enforced
        || !profile.agent_input_to_human_tab_blocked
        || !profile.human_can_see_agent_values
        || !profile.agent_can_see_agent_values
        || profile.agent_ssn_exposed
        || profile.agent_government_id_exposed
        || profile.agent_credit_card_exposed
        || profile.agent_user_password_exposed
        || profile.masked_sensitive_fields < 5
        || profile.sensitive_completed_without_value < 4
        || profile.sensitive_requires_user_input < 1
        || !profile.agent_knows_sensitive_field_status
    {
        bail!("safety selftest failed: {profile:?}");
    }

    println!(
        "SAFETY PASS human_login={} agent_session={} human_can_see_agent_values={} agent_can_see_agent_values={} ssn_exposed={} government_id_exposed={} credit_card_exposed={} user_password_exposed={} masked_sensitive_fields={} completed_without_value={} requires_user_input={} status_known={}",
        profile.human_login,
        profile.agent_session,
        profile.human_can_see_agent_values,
        profile.agent_can_see_agent_values,
        profile.agent_ssn_exposed,
        profile.agent_government_id_exposed,
        profile.agent_credit_card_exposed,
        profile.agent_user_password_exposed,
        profile.masked_sensitive_fields,
        profile.sensitive_completed_without_value,
        profile.sensitive_requires_user_input,
        profile.agent_knows_sensitive_field_status,
    );
    Ok(())
}

fn selftest_native_input() -> Result<()> {
    let workspace = workspace_root()?;
    let base_url = start_test_server(workspace.join("test_pages").join("native_input"))?;
    let profile = saccade_browser::selftest_native_input(base_url)?;

    if !profile.focused
        || profile.value != profile.expected_value
        || profile.keydown_events < profile.expected_value.len()
        || profile.input_events < profile.expected_value.len()
        || profile.keyup_events < profile.expected_value.len()
        || profile.dispatch_failed_keyboard_events != 0
        || profile.select_value != profile.expected_select_value
        || !profile.select_focused
        || profile.select_controls_shown < 1
        || profile.select_input_events < 1
        || profile.select_change_events < 1
    {
        bail!("native input selftest failed: {profile:?}");
    }

    println!(
        "NATIVE_INPUT PASS focused={} value_len={} keydown={} keypress={} beforeinput={} input={} keyup={} handled_keyboard={} consumed_keyboard={} dispatch_failed={} select_value={} select_input={} select_change={} select_controls={}",
        profile.focused,
        profile.value.len(),
        profile.keydown_events,
        profile.keypress_events,
        profile.beforeinput_events,
        profile.input_events,
        profile.keyup_events,
        profile.handled_keyboard_events,
        profile.consumed_keyboard_events,
        profile.dispatch_failed_keyboard_events,
        profile.select_value,
        profile.select_input_events,
        profile.select_change_events,
        profile.select_controls_shown,
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
