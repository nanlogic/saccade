//! One platform-independent client API for the owner-only Saccade Host endpoint.

#![deny(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use saccade_protocol::{ControlRequest, HostGrant, LocalAddress, ProtocolError};
use serde_json::Value;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

#[derive(Debug, Clone)]
pub struct HostClient {
    grant_path: PathBuf,
}

impl HostClient {
    pub fn connect(grant_path: PathBuf) -> Result<Self, ProtocolError> {
        Ok(Self { grant_path })
    }

    pub fn grant_path(&self) -> &Path {
        &self.grant_path
    }

    pub fn call(
        &self,
        id: u64,
        method: impl Into<String>,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, ProtocolError> {
        let deadline = Instant::now() + timeout;
        let method = method.into();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ProtocolError::Timeout);
            }
            let result = read_owner_only_grant(&self.grant_path).and_then(|grant| {
                let request = ControlRequest {
                    id,
                    method: method.clone(),
                    params: params.clone(),
                    capability: grant.capability.clone(),
                };
                call_host(&grant, &request, remaining)
            });
            match result {
                Err(ProtocolError::TransportUnavailable(_)) if Instant::now() < deadline => {
                    thread::sleep(
                        Duration::from_millis(50)
                            .min(deadline.saturating_duration_since(Instant::now())),
                    );
                }
                value => return value,
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use saccade_protocol::{
        ControlRequest, ControlResponse, HostGrant, LocalAddress, HOST_PROTOCOL,
        SESSION_CAPABILITY_SCHEME,
    };
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    fn write_grant(path: &Path, socket: &Path, capability: &str) {
        let grant = HostGrant {
            protocol: HOST_PROTOCOL.into(),
            browser_instance_id: "browser-recovery".into(),
            address: LocalAddress::Unix {
                path: socket.into(),
            },
            capability_scheme: SESSION_CAPABILITY_SCHEME.into(),
            capability: capability.into(),
        };
        fs::write(path, serde_json::to_vec(&grant).unwrap()).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn serve_once(listener: UnixListener, expected_capability: String, value: &'static str) {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        let request: ControlRequest = serde_json::from_str(&line).unwrap();
        assert_eq!(request.capability, expected_capability);
        let response = ControlResponse {
            id: request.id,
            ok: true,
            result: Some(serde_json::json!({"host": value})),
            error: None,
        };
        serde_json::to_writer(&mut stream, &response).unwrap();
        stream.write_all(b"\n").unwrap();
    }

    #[test]
    fn one_client_recovers_when_grant_and_socket_appear_then_rotate() {
        let directory = tempfile::tempdir().unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let grant_path = directory.path().join("host-grant.json");
        let client = HostClient::connect(grant_path.clone()).unwrap();

        let first_socket = directory.path().join("first.sock");
        let first_listener = UnixListener::bind(&first_socket).unwrap();
        fs::set_permissions(&first_socket, fs::Permissions::from_mode(0o600)).unwrap();
        let first_server =
            thread::spawn(move || serve_once(first_listener, "a".repeat(43), "first"));
        thread::sleep(Duration::from_millis(75));
        write_grant(&grant_path, &first_socket, &"a".repeat(43));
        assert_eq!(
            client
                .call(1, "ping", serde_json::json!({}), Duration::from_secs(2))
                .unwrap()["host"],
            "first"
        );
        first_server.join().unwrap();

        let second_socket = directory.path().join("second.sock");
        let second_listener = UnixListener::bind(&second_socket).unwrap();
        fs::set_permissions(&second_socket, fs::Permissions::from_mode(0o600)).unwrap();
        write_grant(&grant_path, &second_socket, &"b".repeat(43));
        let second_server =
            thread::spawn(move || serve_once(second_listener, "b".repeat(43), "second"));
        assert_eq!(
            client
                .call(2, "ping", serde_json::json!({}), Duration::from_secs(2))
                .unwrap()["host"],
            "second"
        );
        second_server.join().unwrap();
    }
}

pub fn read_owner_only_grant(path: &Path) -> Result<HostGrant, ProtocolError> {
    validate_grant_file(path)?;
    let bytes = fs::read(path).map_err(io_error)?;
    let grant: HostGrant = serde_json::from_slice(&bytes).map_err(json_error)?;
    grant.validate()?;
    Ok(grant)
}

pub fn call_host(
    grant: &HostGrant,
    request: &ControlRequest,
    timeout: Duration,
) -> Result<Value, ProtocolError> {
    grant.validate()?;
    match &grant.address {
        LocalAddress::Unix { path } => call_unix(path, request, timeout),
        LocalAddress::WindowsNamedPipe { path } => call_windows(path, request, timeout),
    }
}

#[cfg(unix)]
fn validate_grant_file(path: &Path) -> Result<(), ProtocolError> {
    unix::validate_private_file(path)
}
#[cfg(windows)]
fn validate_grant_file(path: &Path) -> Result<(), ProtocolError> {
    windows::validate_private_file(path)
}
#[cfg(all(not(unix), not(windows)))]
fn validate_grant_file(_: &Path) -> Result<(), ProtocolError> {
    Err(ProtocolError::TransportUnavailable(
        "unsupported operating system".into(),
    ))
}

#[cfg(unix)]
fn call_unix(
    path: &Path,
    request: &ControlRequest,
    timeout: Duration,
) -> Result<Value, ProtocolError> {
    unix::call(path, request, timeout)
}
#[cfg(not(unix))]
fn call_unix(path: &Path, _: &ControlRequest, _: Duration) -> Result<Value, ProtocolError> {
    Err(ProtocolError::TransportUnavailable(format!(
        "Unix sockets are unavailable for {}",
        path.display()
    )))
}
#[cfg(windows)]
fn call_windows(
    path: &Path,
    request: &ControlRequest,
    timeout: Duration,
) -> Result<Value, ProtocolError> {
    windows::call(path, request, timeout)
}
#[cfg(not(windows))]
fn call_windows(path: &Path, _: &ControlRequest, _: Duration) -> Result<Value, ProtocolError> {
    Err(ProtocolError::TransportUnavailable(format!(
        "Windows named pipes are unavailable for {}",
        path.display()
    )))
}

pub(crate) fn io_error(error: std::io::Error) -> ProtocolError {
    if matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ) {
        ProtocolError::Timeout
    } else {
        ProtocolError::TransportUnavailable(error.to_string())
    }
}
pub(crate) fn json_error(error: serde_json::Error) -> ProtocolError {
    ProtocolError::InvalidMessage(error.to_string())
}
