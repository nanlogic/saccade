use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{HOST_PROTOCOL, SESSION_CAPABILITY_SCHEME};

pub const MAX_LOCAL_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_NATIVE_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scheme", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalAddress {
    Unix { path: PathBuf },
    WindowsNamedPipe { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostGrant {
    pub protocol: String,
    pub browser_instance_id: String,
    pub address: LocalAddress,
    pub capability_scheme: String,
    pub capability: String,
}

impl HostGrant {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol != HOST_PROTOCOL
            || self.capability_scheme != SESSION_CAPABILITY_SCHEME
            || self.browser_instance_id.is_empty()
            || self.capability.len() < 43
        {
            return Err(ProtocolError::PermissionDenied("invalid host grant".into()));
        }
        match &self.address {
            LocalAddress::Unix { path } if path.is_absolute() => Ok(()),
            LocalAddress::WindowsNamedPipe { path }
                if path.to_string_lossy().starts_with(r"\\.\pipe\Saccade-") =>
            {
                Ok(())
            }
            _ => Err(ProtocolError::PermissionDenied(
                "invalid local endpoint".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    pub capability: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlResponse {
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ControlError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlError {
    pub code: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeEnvelope {
    pub protocol: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<u64>,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("transport unavailable: {0}")]
    TransportUnavailable(String),
    #[error("message exceeded protocol limit")]
    MessageTooLarge,
    #[error("invalid protocol message: {0}")]
    InvalidMessage(String),
    #[error("operation timed out")]
    Timeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grants_and_envelopes_fail_closed() {
        let value = serde_json::json!({
            "protocol": HOST_PROTOCOL, "browser_instance_id": "browser-1",
            "address": {"scheme": "tcp", "path": "loopback"},
            "capability_scheme": SESSION_CAPABILITY_SCHEME, "capability": "short"
        });
        assert!(serde_json::from_value::<HostGrant>(value).is_err());
        let value = serde_json::json!({
            "protocol": HOST_PROTOCOL, "kind": "hello", "payload": {}, "selector": "button"
        });
        assert!(serde_json::from_value::<NativeEnvelope>(value).is_err());
    }
}
