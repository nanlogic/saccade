use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};
use saccade_runtime::session::NativeHostSession;

fn main() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("native-host") => native_host(default_runtime_dir()),
        Some("chrome-extension://bobfbgjplflcigednmccmbhlgclomgod/") => {
            native_host(dev_runtime_dir())
        }
        Some("mcp") => saccade_runtime::mcp::serve(default_grant_path()),
        Some("doctor") => doctor(),
        Some("repair") => repair(),
        _ => bail!("usage: saccade-runtime <native-host|mcp|doctor|repair>"),
    }
}

fn repair() -> Result<()> {
    let accessibility_trusted = saccade_runtime::platform_input::request_accessibility();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema":"saccade.repair/1",
            "accessibility_trusted":accessibility_trusted,
            "detail":if accessibility_trusted {
                "Accessibility is ready."
            } else {
                "Approve the installed Saccade Dev Runtime in Privacy & Security > Accessibility, then rerun dev.sh status."
            }
        }))?
    );
    Ok(())
}

fn native_host(runtime_dir: PathBuf) -> Result<()> {
    if !saccade_runtime::platform_input::accessibility_trusted() {
        let _ = saccade_runtime::platform_input::request_accessibility();
    }
    let session = Arc::new(NativeHostSession::new(runtime_dir)?);
    let server = saccade_runtime::owner_ipc::OwnerIpcServer::bind(session.runtime_dir())?;
    session.install_endpoint(server.address())?;
    let ipc_session = Arc::clone(&session);
    std::thread::Builder::new()
        .name("saccade-owner-ipc".into())
        .spawn(move || {
            let handler: Arc<
                dyn Fn(saccade_protocol::ControlRequest) -> saccade_protocol::ControlResponse
                    + Send
                    + Sync,
            > = Arc::new(move |request| ipc_session.handle_control(request));
            let _ = server.serve(handler);
        })?;
    let mut input = std::io::stdin().lock();
    loop {
        let Some(message) = saccade_runtime::native_messaging::read_message(&mut input)? else {
            session.mark_extension_disconnected();
            return Ok(());
        };
        if let Err(error) = session.handle_native(message) {
            eprintln!("saccade native-host ignored invalid extension event: {error}");
        }
    }
}

fn doctor() -> Result<()> {
    let grant_path = default_grant_path();
    let status = saccade_host_client::HostClient::connect(grant_path.clone()).and_then(|host| {
        host.call(
            1,
            "system.capabilities",
            serde_json::json!({}),
            Duration::from_secs(2),
        )
    });
    let (ready, capabilities, detail) = match status {
        Ok(value) => (
            value
                .get("extension_connected")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            Some(value),
            None,
        ),
        Err(error) => (false, None, Some(error.to_string())),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema":"saccade.doctor/1",
            "observation_schema":saccade_protocol::OBSERVATION_SCHEMA,
            "host_protocol":saccade_protocol::HOST_PROTOCOL,
            "accessibility_trusted":saccade_runtime::platform_input::accessibility_trusted(),
            "grant_path":grant_path,
            "ready":ready,
            "capabilities":capabilities,
            "detail":detail
        }))?
    );
    Ok(())
}

fn default_runtime_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("SACCADE_RUNTIME_DIR") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("LOCALAPPDATA").unwrap_or_default();
    #[cfg(not(target_os = "windows"))]
    let root = std::env::var_os("HOME").unwrap_or_default();
    #[cfg(target_os = "windows")]
    return PathBuf::from(root).join("Saccade");
    #[cfg(not(target_os = "windows"))]
    PathBuf::from(root).join("Library/Application Support/Saccade")
}

fn dev_runtime_dir() -> PathBuf {
    let root = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(root).join("Library/Application Support/Saccade Dev/runtime")
}

fn default_grant_path() -> PathBuf {
    default_runtime_dir().join("host-grant.json")
}
