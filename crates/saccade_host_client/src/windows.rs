use std::ffi::{c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::time::{Duration, Instant};

use saccade_protocol::{ControlRequest, ControlResponse, ProtocolError, MAX_LOCAL_MESSAGE_BYTES};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_FILE_NOT_FOUND, ERROR_SEM_TIMEOUT, GENERIC_READ,
    GENERIC_WRITE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, ReadFile, WriteFile, OPEN_EXISTING};
use windows_sys::Win32::System::Pipes::WaitNamedPipeW;

use crate::{io_error, json_error};

pub(super) fn call(
    path: &Path,
    request: &ControlRequest,
    timeout: Duration,
) -> Result<serde_json::Value, ProtocolError> {
    let name = wide(path.as_os_str());
    wait_for_pipe(&name, timeout)?;
    // SAFETY: pointers are valid inputs or null optional parameters.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(ProtocolError::TransportUnavailable(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    let result = exchange(handle, request);
    // SAFETY: handle came from CreateFileW and is closed once.
    unsafe { CloseHandle(handle) };
    result
}

fn wait_for_pipe(name: &[u16], timeout: Duration) -> Result<(), ProtocolError> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ProtocolError::Timeout);
        }
        let wait_ms = remaining.min(Duration::from_millis(50)).as_millis().max(1) as u32;
        // SAFETY: name is NUL-terminated and alive for the synchronous call.
        if unsafe { WaitNamedPipeW(name.as_ptr(), wait_ms) } != 0 {
            return Ok(());
        }
        // A server that just handled a disconnected client briefly has no
        // named-pipe instance while it creates the replacement. WaitNamedPipeW
        // reports FILE_NOT_FOUND immediately in that window instead of honoring
        // its timeout, so retry within the caller's original budget.
        let error = unsafe { GetLastError() };
        if error == ERROR_FILE_NOT_FOUND {
            std::thread::sleep(remaining.min(Duration::from_millis(5)));
            continue;
        }
        if error == ERROR_SEM_TIMEOUT {
            continue;
        }
        return Err(ProtocolError::TransportUnavailable(
            std::io::Error::from_raw_os_error(error as i32).to_string(),
        ));
    }
}

pub(super) fn validate_private_file(path: &Path) -> Result<(), ProtocolError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(ProtocolError::TransportUnavailable("grant missing".into()))
    }
}

fn exchange(
    handle: *mut c_void,
    request: &ControlRequest,
) -> Result<serde_json::Value, ProtocolError> {
    let mut bytes = serde_json::to_vec(request).map_err(json_error)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_LOCAL_MESSAGE_BYTES {
        return Err(ProtocolError::MessageTooLarge);
    }
    let mut written = 0_u32;
    // SAFETY: bytes is readable and written is a valid output pointer.
    if unsafe {
        WriteFile(
            handle,
            bytes.as_ptr(),
            bytes.len() as u32,
            &mut written,
            std::ptr::null_mut(),
        )
    } == 0
        || written as usize != bytes.len()
    {
        return Err(io_error(std::io::Error::last_os_error()));
    }
    let mut response = Vec::new();
    let mut chunk = [0_u8; 8192];
    while !response.ends_with(b"\n") {
        let mut read = 0_u32;
        // SAFETY: chunk is writable and read is a valid output pointer.
        if unsafe {
            ReadFile(
                handle,
                chunk.as_mut_ptr(),
                chunk.len() as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io_error(std::io::Error::last_os_error()));
        }
        if read == 0 {
            return Err(ProtocolError::TransportUnavailable(
                "named pipe closed before response".into(),
            ));
        }
        response.extend_from_slice(&chunk[..read as usize]);
        if response.len() > MAX_LOCAL_MESSAGE_BYTES {
            return Err(ProtocolError::MessageTooLarge);
        }
    }
    let response: ControlResponse = serde_json::from_slice(&response).map_err(json_error)?;
    if response.id != request.id {
        return Err(ProtocolError::InvalidMessage("response id mismatch".into()));
    }
    if response.ok {
        Ok(response.result.unwrap_or(serde_json::Value::Null))
    } else {
        let detail = response
            .error
            .map(|value| format!("{}: {}", value.code, value.detail))
            .unwrap_or_else(|| "host request failed".into());
        Err(ProtocolError::InvalidMessage(detail))
    }
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}
