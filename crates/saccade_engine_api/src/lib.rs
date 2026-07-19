//! Versioned, engine-neutral types shared by browser adapters and Saccade hosts.
//!
//! This crate deliberately contains no Servo, CEF, CDP, MCP, or product policy
//! types. Engines translate their native state into this boundary.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub const ADAPTER_CONTRACT_VERSION: &str = "1.0";
pub const CONTROL_PROTOCOL_VERSION: &str = "saccade-engine-control-v1";
pub const SESSION_CAPABILITY_SCHEME: &str = "saccade_session_bearer_v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterCapabilities {
    pub contract_version: String,
    pub capabilities: Vec<String>,
}

impl AdapterCapabilities {
    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|item| item == capability)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EngineTabId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub scheme: String,
    pub host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PageRevision(pub u64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageFact {
    pub fact_id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactBatch {
    pub tab_id: EngineTabId,
    pub origin: Origin,
    pub page_revision: PageRevision,
    pub facts: Vec<PageFact>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineAction {
    pub action_id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionMap {
    pub tab_id: EngineTabId,
    pub origin: Origin,
    pub page_revision: PageRevision,
    pub actions: Vec<EngineAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Accepted,
    Applied,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputReceipt {
    pub tab_id: EngineTabId,
    pub action_id: String,
    pub basis_page_revision: PageRevision,
    pub observed_page_revision: PageRevision,
    pub status: ReceiptStatus,
    pub verified: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub evidence: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EngineErrorCode {
    InvalidArgument,
    PermissionDenied,
    ConsentRequired,
    AgentPaused,
    UnsupportedCapability,
    StalePageRevision,
    StaleLayout,
    TabNotFound,
    TransportUnavailable,
    Timeout,
    Conflict,
    PolicyBlocked,
    FormCommandFailed,
    PostconditionFailed,
    ScreenshotBusy,
    ScreenshotFailed,
    HumanVerificationRequired,
    ProviderRejected,
    Internal,
}

#[derive(Debug, Error)]
#[error("{code:?}: {detail}")]
pub struct EngineApiError {
    pub code: EngineErrorCode,
    pub detail: String,
}

impl EngineApiError {
    pub fn new(code: EngineErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineAdapterDescriptor {
    pub contract_version: String,
    pub transport: String,
    pub provenance: String,
    pub page_dom_injected: bool,
    pub sensitive_values_exposed_to_agent: bool,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scheme", rename_all = "snake_case")]
pub enum TransportAddress {
    Unix { path: PathBuf },
    Tcp { host: String, port: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlEndpointDescriptor {
    pub protocol: String,
    #[serde(flatten)]
    pub address: TransportAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCapability {
    pub scheme: String,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineGrant {
    pub engine_adapter: EngineAdapterDescriptor,
    pub control_endpoint: ControlEndpointDescriptor,
    pub control_capability: SessionCapability,
}

impl EngineGrant {
    pub fn validate(&self) -> Result<(), EngineApiError> {
        if self.engine_adapter.contract_version != ADAPTER_CONTRACT_VERSION {
            return Err(EngineApiError::new(
                EngineErrorCode::UnsupportedCapability,
                format!(
                    "unsupported engine adapter contract {:?}",
                    self.engine_adapter.contract_version
                ),
            ));
        }
        if self.control_endpoint.protocol != CONTROL_PROTOCOL_VERSION {
            return Err(EngineApiError::new(
                EngineErrorCode::UnsupportedCapability,
                format!(
                    "unsupported engine control protocol {:?}",
                    self.control_endpoint.protocol
                ),
            ));
        }
        if self.engine_adapter.page_dom_injected
            || self.engine_adapter.sensitive_values_exposed_to_agent
        {
            return Err(EngineApiError::new(
                EngineErrorCode::PermissionDenied,
                "engine adapter violates the Saccade data boundary",
            ));
        }
        if self.control_capability.scheme != SESSION_CAPABILITY_SCHEME
            || self.control_capability.token.len() < 43
        {
            return Err(EngineApiError::new(
                EngineErrorCode::PermissionDenied,
                "invalid session capability",
            ));
        }
        if self.engine_adapter.provenance != "browser_process"
            || self.engine_adapter.transport != "owner_only_unix_v1"
        {
            return Err(EngineApiError::new(
                EngineErrorCode::PermissionDenied,
                "adapter contract 1.0 requires browser_process provenance and owner_only_unix_v1",
            ));
        }
        if self.engine_adapter.capabilities.is_empty()
            || !self
                .engine_adapter
                .capabilities
                .iter()
                .any(|capability| capability == "ping")
        {
            return Err(EngineApiError::new(
                EngineErrorCode::UnsupportedCapability,
                "engine adapter must advertise ping and at least one capability",
            ));
        }
        validate_transport_address(&self.control_endpoint.address)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    pub capability: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlResponse {
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<EngineControlError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineControlError {
    pub code: EngineErrorCode,
    pub detail: String,
}

pub fn read_owner_only_grant(path: &Path) -> Result<Value, EngineApiError> {
    validate_private_regular_file(path)?;
    let bytes = fs::read(path).map_err(|error| {
        EngineApiError::new(
            EngineErrorCode::TransportUnavailable,
            format!("failed to read {}: {error}", path.display()),
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        EngineApiError::new(
            EngineErrorCode::InvalidArgument,
            format!("failed to parse {}: {error}", path.display()),
        )
    })
}

pub fn call_control(
    grant: &EngineGrant,
    method: &str,
    params: Value,
    read_timeout: Duration,
) -> Result<Value, EngineApiError> {
    grant.validate()?;
    let request = ControlRequest {
        id: 1,
        method: method.to_string(),
        params,
        capability: grant.control_capability.token.clone(),
    };
    match &grant.control_endpoint.address {
        TransportAddress::Tcp { host, port } => {
            let address = loopback_socket_addr(host, *port)?;
            let stream =
                TcpStream::connect_timeout(&address, Duration::from_secs(2)).map_err(|error| {
                    EngineApiError::new(
                        EngineErrorCode::TransportUnavailable,
                        format!("failed to connect {address}: {error}"),
                    )
                })?;
            stream
                .set_read_timeout(Some(read_timeout))
                .map_err(io_error)?;
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .map_err(io_error)?;
            transact(stream, &request)
        }
        TransportAddress::Unix { path } => call_unix(path, &request, read_timeout),
    }
}

fn transact<S: std::io::Read + Write>(
    mut stream: S,
    request: &ControlRequest,
) -> Result<Value, EngineApiError> {
    serde_json::to_writer(&mut stream, request).map_err(json_error)?;
    stream.write_all(b"\n").map_err(io_error)?;
    stream.flush().map_err(io_error)?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(io_error)?;
    if line.is_empty() {
        return Err(EngineApiError::new(
            EngineErrorCode::TransportUnavailable,
            "engine control endpoint closed without a response",
        ));
    }
    let response: ControlResponse = serde_json::from_str(&line).map_err(json_error)?;
    if response.id != request.id {
        return Err(EngineApiError::new(
            EngineErrorCode::TransportUnavailable,
            "engine control response id did not match request",
        ));
    }
    if response.ok {
        return Ok(response.result.unwrap_or(Value::Null));
    }
    let error = response.error.unwrap_or(EngineControlError {
        code: EngineErrorCode::Internal,
        detail: "engine control request failed".to_string(),
    });
    Err(EngineApiError::new(error.code, error.detail))
}

#[cfg(unix)]
fn call_unix(
    path: &Path,
    request: &ControlRequest,
    read_timeout: Duration,
) -> Result<Value, EngineApiError> {
    validate_owner_only_socket(path)?;
    let stream = UnixStream::connect(path).map_err(|error| {
        EngineApiError::new(
            EngineErrorCode::TransportUnavailable,
            format!("failed to connect {}: {error}", path.display()),
        )
    })?;
    stream
        .set_read_timeout(Some(read_timeout))
        .map_err(io_error)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(io_error)?;
    transact(stream, request)
}

#[cfg(not(unix))]
fn call_unix(
    path: &Path,
    _request: &ControlRequest,
    _read_timeout: Duration,
) -> Result<Value, EngineApiError> {
    Err(EngineApiError::new(
        EngineErrorCode::UnsupportedCapability,
        format!("Unix transport is unavailable for {}", path.display()),
    ))
}

fn validate_transport_address(address: &TransportAddress) -> Result<(), EngineApiError> {
    match address {
        TransportAddress::Tcp { .. } => Err(EngineApiError::new(
            EngineErrorCode::PermissionDenied,
            "adapter contract 1.0 does not permit TCP transport",
        )),
        TransportAddress::Unix { path } => {
            if !path.is_absolute() {
                return Err(EngineApiError::new(
                    EngineErrorCode::InvalidArgument,
                    "engine Unix socket path must be absolute",
                ));
            }
            Ok(())
        }
    }
}

fn loopback_socket_addr(host: &str, port: u16) -> Result<SocketAddr, EngineApiError> {
    if port == 0 {
        return Err(EngineApiError::new(
            EngineErrorCode::InvalidArgument,
            "engine TCP port must be non-zero",
        ));
    }
    let ip = match host {
        "127.0.0.1" | "localhost" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        "::1" => IpAddr::V6(Ipv6Addr::LOCALHOST),
        _ => {
            return Err(EngineApiError::new(
                EngineErrorCode::PermissionDenied,
                format!("engine TCP endpoint must be loopback; got {host:?}"),
            ));
        }
    };
    Ok(SocketAddr::new(ip, port))
}

#[cfg(unix)]
fn validate_private_regular_file(path: &Path) -> Result<(), EngineApiError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        EngineApiError::new(
            EngineErrorCode::TransportUnavailable,
            format!("failed to inspect {}: {error}", path.display()),
        )
    })?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(EngineApiError::new(
            EngineErrorCode::PermissionDenied,
            format!("{} is not a regular owner grant", path.display()),
        ));
    }
    require_private_mode(path, metadata.permissions().mode())?;
    if let Some(parent) = path.parent() {
        let parent_metadata = fs::symlink_metadata(parent).map_err(io_error)?;
        require_private_mode(parent, parent_metadata.permissions().mode())?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_regular_file(path: &Path) -> Result<(), EngineApiError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(EngineApiError::new(
            EngineErrorCode::TransportUnavailable,
            format!("{} is not a regular owner grant", path.display()),
        ))
    }
}

#[cfg(unix)]
fn validate_owner_only_socket(path: &Path) -> Result<(), EngineApiError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        EngineApiError::new(
            EngineErrorCode::TransportUnavailable,
            format!("failed to inspect {}: {error}", path.display()),
        )
    })?;
    if !metadata.file_type().is_socket() {
        return Err(EngineApiError::new(
            EngineErrorCode::PermissionDenied,
            format!("{} is not a Unix socket", path.display()),
        ));
    }
    require_private_mode(path, metadata.permissions().mode())?;
    let parent = path.parent().ok_or_else(|| {
        EngineApiError::new(
            EngineErrorCode::InvalidArgument,
            "engine Unix socket has no parent directory",
        )
    })?;
    let parent_metadata = fs::symlink_metadata(parent).map_err(io_error)?;
    require_private_mode(parent, parent_metadata.permissions().mode())
}

#[cfg(unix)]
fn require_private_mode(path: &Path, mode: u32) -> Result<(), EngineApiError> {
    if mode & 0o077 != 0 {
        return Err(EngineApiError::new(
            EngineErrorCode::PermissionDenied,
            format!(
                "{} must not grant group/other permissions (mode {:o})",
                path.display(),
                mode & 0o777
            ),
        ));
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> EngineApiError {
    let code = if matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ) {
        EngineErrorCode::Timeout
    } else {
        EngineErrorCode::TransportUnavailable
    };
    EngineApiError::new(code, error.to_string())
}

fn json_error(error: serde_json::Error) -> EngineApiError {
    EngineApiError::new(EngineErrorCode::TransportUnavailable, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    use serde_json::json;

    use super::*;

    fn valid_grant(path: PathBuf) -> EngineGrant {
        EngineGrant {
            engine_adapter: EngineAdapterDescriptor {
                contract_version: ADAPTER_CONTRACT_VERSION.to_string(),
                transport: "owner_only_unix_v1".to_string(),
                provenance: "browser_process".to_string(),
                page_dom_injected: false,
                sensitive_values_exposed_to_agent: false,
                capabilities: vec![
                    "ping".to_string(),
                    "navigate".to_string(),
                    "pause".to_string(),
                ],
            },
            control_endpoint: ControlEndpointDescriptor {
                protocol: CONTROL_PROTOCOL_VERSION.to_string(),
                address: TransportAddress::Unix { path },
            },
            control_capability: SessionCapability {
                scheme: SESSION_CAPABILITY_SCHEME.to_string(),
                token: "a".repeat(43),
            },
        }
    }

    #[test]
    fn adapter_grant_is_engine_neutral_and_versioned() {
        let grant = valid_grant(PathBuf::from("/tmp/saccade/control.sock"));
        grant.validate().unwrap();
        let value = serde_json::to_value(grant).unwrap();
        assert_eq!(
            value.pointer("/engine_adapter/contract_version"),
            Some(&json!("1.0"))
        );
        assert!(value.get("engine").is_none());
    }

    #[test]
    fn adapter_rejects_value_exposure() {
        let mut grant = valid_grant(PathBuf::from("/tmp/saccade/control.sock"));
        grant.engine_adapter.sensitive_values_exposed_to_agent = true;
        assert_eq!(
            grant.validate().unwrap_err().code,
            EngineErrorCode::PermissionDenied
        );
    }

    #[test]
    fn adapter_contract_rejects_tcp_fallback() {
        let mut grant = valid_grant(PathBuf::from("/tmp/saccade/control.sock"));
        grant.control_endpoint.address = TransportAddress::Tcp {
            host: "127.0.0.1".to_string(),
            port: 41234,
        };
        assert_eq!(
            grant.validate().unwrap_err().code,
            EngineErrorCode::PermissionDenied
        );
    }

    #[test]
    fn browser_adapter_error_codes_deserialize_without_transport_collapse() {
        for (wire, expected) in [
            ("CONSENT_REQUIRED", EngineErrorCode::ConsentRequired),
            ("AGENT_PAUSED", EngineErrorCode::AgentPaused),
            ("FORM_COMMAND_FAILED", EngineErrorCode::FormCommandFailed),
            ("POLICY_BLOCKED", EngineErrorCode::PolicyBlocked),
            ("POSTCONDITION_FAILED", EngineErrorCode::PostconditionFailed),
            ("SCREENSHOT_BUSY", EngineErrorCode::ScreenshotBusy),
            ("SCREENSHOT_FAILED", EngineErrorCode::ScreenshotFailed),
            (
                "HUMAN_VERIFICATION_REQUIRED",
                EngineErrorCode::HumanVerificationRequired,
            ),
            ("PROVIDER_REJECTED", EngineErrorCode::ProviderRejected),
        ] {
            let value = serde_json::from_value::<EngineControlError>(json!({
                "code": wire,
                "detail": "redacted test detail"
            }))
            .expect("adapter error code should deserialize");
            assert_eq!(value.code, expected);
        }
    }

    #[cfg(unix)]
    #[test]
    fn owner_grant_rejects_group_readable_file() {
        let directory = tempfile::tempdir().unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = directory.path().join("grant.json");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o640)
            .open(&path)
            .unwrap();
        file.write_all(b"{}").unwrap();
        assert_eq!(
            read_owner_only_grant(&path).unwrap_err().code,
            EngineErrorCode::PermissionDenied
        );
    }
}
