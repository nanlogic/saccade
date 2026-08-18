use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use saccade_protocol::{
    ControlError, ControlRequest, ControlResponse, ProtocolError, MAX_LOCAL_MESSAGE_BYTES,
};
use serde_json::Value;

use crate::{io_error, json_error};

pub(super) fn call(
    path: &Path,
    request: &ControlRequest,
    timeout: Duration,
) -> Result<Value, ProtocolError> {
    validate_owner_socket(path)?;
    let mut stream = UnixStream::connect(path).map_err(io_error)?;
    stream.set_read_timeout(Some(timeout)).map_err(io_error)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(io_error)?;
    let bytes = serde_json::to_vec(request).map_err(json_error)?;
    if bytes.len() > MAX_LOCAL_MESSAGE_BYTES {
        return Err(ProtocolError::MessageTooLarge);
    }
    stream.write_all(&bytes).map_err(io_error)?;
    stream.write_all(b"\n").map_err(io_error)?;
    stream.flush().map_err(io_error)?;
    let mut response = Vec::new();
    BufReader::new(stream)
        .take((MAX_LOCAL_MESSAGE_BYTES + 1) as u64)
        .read_until(b'\n', &mut response)
        .map_err(io_error)?;
    if response.len() > MAX_LOCAL_MESSAGE_BYTES {
        return Err(ProtocolError::MessageTooLarge);
    }
    decode_response(request, &response)
}

pub(super) fn validate_private_file(path: &Path) -> Result<(), ProtocolError> {
    let metadata = std::fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(ProtocolError::PermissionDenied(
            "grant is not a regular file".into(),
        ));
    }
    require_private_mode(metadata.permissions().mode())?;
    if let Some(parent) = path.parent() {
        require_private_mode(
            std::fs::symlink_metadata(parent)
                .map_err(io_error)?
                .permissions()
                .mode(),
        )?;
    }
    Ok(())
}

fn validate_owner_socket(path: &Path) -> Result<(), ProtocolError> {
    let metadata = std::fs::symlink_metadata(path).map_err(io_error)?;
    if !metadata.file_type().is_socket() {
        return Err(ProtocolError::PermissionDenied(
            "endpoint is not a Unix socket".into(),
        ));
    }
    require_private_mode(metadata.permissions().mode())?;
    let parent = path
        .parent()
        .ok_or_else(|| ProtocolError::PermissionDenied("socket has no parent".into()))?;
    require_private_mode(
        std::fs::symlink_metadata(parent)
            .map_err(io_error)?
            .permissions()
            .mode(),
    )
}

fn require_private_mode(mode: u32) -> Result<(), ProtocolError> {
    if mode & 0o077 == 0 {
        Ok(())
    } else {
        Err(ProtocolError::PermissionDenied(
            "owner-only mode required".into(),
        ))
    }
}

fn decode_response(request: &ControlRequest, bytes: &[u8]) -> Result<Value, ProtocolError> {
    if bytes.is_empty() {
        return Err(ProtocolError::TransportUnavailable(
            "Unix socket closed before response".into(),
        ));
    }
    let response: ControlResponse = serde_json::from_slice(bytes).map_err(json_error)?;
    if response.id != request.id {
        return Err(ProtocolError::InvalidMessage("response id mismatch".into()));
    }
    if response.ok {
        Ok(response.result.unwrap_or(Value::Null))
    } else {
        let error = response.error.unwrap_or(ControlError {
            code: "INTERNAL".into(),
            detail: "host request failed".into(),
        });
        Err(ProtocolError::InvalidMessage(format!(
            "{}: {}",
            error.code, error.detail
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::decode_response;
    use saccade_protocol::{ControlRequest, ProtocolError};

    #[test]
    fn empty_response_is_transport_rotation_not_invalid_json() {
        let request = ControlRequest {
            id: 1,
            method: "ping".into(),
            params: serde_json::json!({}),
            capability: "c".repeat(43),
        };
        assert!(matches!(
            decode_response(&request, b""),
            Err(ProtocolError::TransportUnavailable(_))
        ));
    }
}
