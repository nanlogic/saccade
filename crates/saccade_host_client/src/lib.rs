//! One platform-independent client API for the owner-only Saccade Host endpoint.

#![deny(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

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
        read_owner_only_grant(&grant_path)?;
        Ok(Self { grant_path })
    }

    pub fn call(
        &self,
        id: u64,
        method: impl Into<String>,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, ProtocolError> {
        let grant = read_owner_only_grant(&self.grant_path)?;
        let request = ControlRequest {
            id,
            method: method.into(),
            params,
            capability: grant.capability.clone(),
        };
        call_host(&grant, &request, timeout)
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
