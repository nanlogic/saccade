use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};
use saccade_runtime::session::NativeHostSession;

fn main() -> Result<()> {
    let mode = std::env::args().nth(1);
    match mode.as_deref() {
        Some("native-host") => native_host(default_runtime_dir()),
        Some(origin) if is_extension_origin(origin) => {
            let runtime_dir = if launched_from_dev_runtime_app() {
                dev_runtime_dir()
            } else {
                default_runtime_dir()
            };
            native_host(runtime_dir)
        }
        Some("mcp") => saccade_runtime::mcp::serve(default_grant_path()),
        Some("reference-actuator-mcp") => {
            saccade_runtime::mcp::serve_reference_actuator(default_grant_path())
        }
        Some("doctor") => doctor(),
        Some("reference-actuator-repair") => reference_actuator_repair(),
        _ => bail!("usage: saccade-runtime <native-host|mcp|doctor|reference-actuator-mcp|reference-actuator-repair>"),
    }
}

fn is_extension_origin(value: &str) -> bool {
    let Some(id) = value
        .strip_prefix("chrome-extension://")
        .and_then(|value| value.strip_suffix('/'))
    else {
        return false;
    };
    id.len() == 32 && id.bytes().all(|byte| matches!(byte, b'a'..=b'p'))
}

fn launched_from_dev_runtime_app() -> bool {
    std::env::current_exe()
        .map(|path| is_dev_runtime_executable(&path))
        .unwrap_or(false)
}

fn is_dev_runtime_executable(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "Saccade Dev Runtime.app")
}

fn reference_actuator_repair() -> Result<()> {
    let accessibility_trusted = saccade_runtime::platform_input::request_accessibility();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema":"saccade.reference-actuator-repair/1",
            "accessibility_trusted":accessibility_trusted,
            "detail":if accessibility_trusted {
                "Accessibility is ready."
            } else {
                "Approve the optional Reference Actuator in Privacy & Security > Accessibility, then rerun the explicit actuator test."
            }
        }))?
    );
    Ok(())
}

fn native_host(runtime_dir: PathBuf) -> Result<()> {
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
            if let Err(error) = server.serve(handler) {
                eprintln!("saccade owner IPC server stopped: {error}");
            }
        })?;
    let mut input = std::io::stdin().lock();
    loop {
        let Some(message) = saccade_runtime::native_messaging::read_message(&mut input)? else {
            session.mark_extension_disconnected();
            return Ok(());
        };
        if let Err(error) = session.handle_native(message) {
            eprintln!("saccade native-host ignored invalid extension event: {error}");
            record_dev_native_error(session.runtime_dir(), &error.to_string());
        }
    }
}

fn record_dev_native_error(runtime_dir: &Path, error: &str) {
    if runtime_dir
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        != Some("Saccade Dev")
    {
        return;
    }
    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(runtime_dir.join("last-native-error.log"))
    else {
        return;
    };
    let _ = writeln!(file, "{}", error.chars().take(512).collect::<String>());
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
            "runtime_version":env!("CARGO_PKG_VERSION"),
            "mcp_contract_hash":saccade_runtime::mcp::truth_contract_hash(),
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{is_dev_runtime_executable, is_extension_origin};

    #[test]
    fn accepts_chromium_extension_origins_only() {
        assert!(is_extension_origin(
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop/"
        ));
        assert!(!is_extension_origin("chrome-extension://too-short/"));
        assert!(!is_extension_origin(
            "chrome-extension://abcdefghijklmnopabcdefghijklmnoq/"
        ));
        assert!(!is_extension_origin(
            "https://abcdefghijklmnopabcdefghijklmnop/"
        ));
    }

    #[test]
    fn identifies_only_the_development_app_bundle_as_the_dev_runtime() {
        let dev = Path::new("Users")
            .join("wayne")
            .join("Applications")
            .join("Saccade Dev Runtime.app")
            .join("Contents")
            .join("MacOS")
            .join("saccade-runtime");
        let production = Path::new("Users")
            .join("wayne")
            .join("Applications")
            .join("Saccade.app")
            .join("Contents")
            .join("MacOS")
            .join("saccade-runtime");

        assert!(is_dev_runtime_executable(&dev));
        assert!(!is_dev_runtime_executable(&production));
    }
}
