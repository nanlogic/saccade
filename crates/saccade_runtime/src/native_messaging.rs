//! Bounded Chrome/Edge Native Messaging framing.

use std::io::{Read, Write};

use saccade_protocol::{NativeEnvelope, HOST_PROTOCOL, MAX_NATIVE_MESSAGE_BYTES};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FramingError {
    #[error("native message I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("native message ended inside its length prefix")]
    TruncatedPrefix,
    #[error("native message length {0} is outside the allowed range")]
    InvalidLength(usize),
    #[error("invalid native message JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("native message used the wrong protocol")]
    WrongProtocol,
}

pub fn read_message(reader: &mut impl Read) -> Result<Option<NativeEnvelope>, FramingError> {
    let mut prefix = [0_u8; 4];
    let mut read = 0_usize;
    while read < prefix.len() {
        let count = reader.read(&mut prefix[read..])?;
        if count == 0 {
            if read == 0 {
                return Ok(None);
            }
            return Err(FramingError::TruncatedPrefix);
        }
        read += count;
    }
    let length = u32::from_ne_bytes(prefix) as usize;
    if length == 0 || length > MAX_NATIVE_MESSAGE_BYTES {
        return Err(FramingError::InvalidLength(length));
    }
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload)?;
    let message: NativeEnvelope = serde_json::from_slice(&payload)?;
    if message.protocol != HOST_PROTOCOL {
        return Err(FramingError::WrongProtocol);
    }
    Ok(Some(message))
}

pub fn write_message(
    writer: &mut impl Write,
    message: &NativeEnvelope,
) -> Result<(), FramingError> {
    if message.protocol != HOST_PROTOCOL {
        return Err(FramingError::WrongProtocol);
    }
    let payload = serde_json::to_vec(message)?;
    if payload.is_empty() || payload.len() > MAX_NATIVE_MESSAGE_BYTES {
        return Err(FramingError::InvalidLength(payload.len()));
    }
    let length =
        u32::try_from(payload.len()).map_err(|_| FramingError::InvalidLength(payload.len()))?;
    writer.write_all(&length.to_ne_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_round_trip_is_bounded_and_strict() {
        let message = NativeEnvelope {
            protocol: HOST_PROTOCOL.into(),
            kind: "hello".into(),
            request_id: None,
            payload: serde_json::json!({"browser_instance_id":"browser-1"}),
        };
        let mut bytes = Vec::new();
        write_message(&mut bytes, &message).unwrap();
        assert_eq!(
            read_message(&mut bytes.as_slice()).unwrap().unwrap().kind,
            "hello"
        );

        let oversized = ((MAX_NATIVE_MESSAGE_BYTES + 1) as u32)
            .to_ne_bytes()
            .to_vec();
        assert!(matches!(
            read_message(&mut oversized.as_slice()),
            Err(FramingError::InvalidLength(_))
        ));
    }
}
