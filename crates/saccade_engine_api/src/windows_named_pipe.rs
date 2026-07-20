//! Windows-only owner pipe client with bounded overlapped I/O.
//!
//! All unsafe Windows calls are isolated here. Buffers and `OVERLAPPED` values
//! remain alive until completion or `CancelIoEx` has been observed.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use serde_json::Value;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING, ERROR_PIPE_BUSY, GENERIC_READ,
    GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, OPEN_EXISTING, ReadFile, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::WaitNamedPipeW;
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use super::{ControlRequest, ControlResponse, EngineApiError, EngineControlError, EngineErrorCode};

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const READ_CHUNK_BYTES: usize = 16 * 1024;

pub(super) fn call(
    path: &Path,
    request: &ControlRequest,
    connect_timeout: Duration,
    write_timeout: Duration,
    read_timeout: Duration,
) -> Result<Value, EngineApiError> {
    let wide_path = wide_null(path.as_os_str());
    let pipe = connect(path, &wide_path, connect_timeout)?;

    let mut request_bytes = serde_json::to_vec(request).map_err(|error| {
        EngineApiError::new(EngineErrorCode::TransportUnavailable, error.to_string())
    })?;
    request_bytes.push(b'\n');
    write_all(&pipe, path, &request_bytes, write_timeout)?;
    let response_bytes = read_line(&pipe, path, read_timeout)?;
    let response: ControlResponse = serde_json::from_slice(&response_bytes).map_err(|error| {
        EngineApiError::new(EngineErrorCode::TransportUnavailable, error.to_string())
    })?;
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

struct OwnedHandle(HANDLE);

impl OwnedHandle {
    fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: this wrapper exclusively owns the valid Windows handle.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

fn connect(
    path: &Path,
    wide_path: &[u16],
    timeout: Duration,
) -> Result<OwnedHandle, EngineApiError> {
    let deadline = deadline_after(timeout);
    loop {
        if Instant::now() >= deadline {
            return Err(timeout_error("connect", path));
        }
        // SAFETY: `wide_path` is NUL terminated and all optional pointers are null.
        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            return Ok(OwnedHandle(handle));
        }
        let code = last_error();
        if code != ERROR_PIPE_BUSY && code != ERROR_FILE_NOT_FOUND {
            return Err(win32_error("connect", path, code));
        }
        let wait_ms = remaining_wait_ms(deadline).ok_or_else(|| timeout_error("connect", path))?;
        // SAFETY: `wide_path` remains NUL terminated for the duration of the call.
        let available = unsafe { WaitNamedPipeW(wide_path.as_ptr(), wait_ms) };
        if available == 0 && Instant::now() >= deadline {
            return Err(timeout_error("connect", path));
        }
    }
}

fn write_all(
    pipe: &OwnedHandle,
    path: &Path,
    bytes: &[u8],
    timeout: Duration,
) -> Result<(), EngineApiError> {
    let deadline = deadline_after(timeout);
    let mut offset = 0;
    while offset < bytes.len() {
        let written = write_once(pipe.raw(), path, &bytes[offset..], deadline)?;
        if written == 0 {
            return Err(EngineApiError::new(
                EngineErrorCode::TransportUnavailable,
                format!(
                    "Windows named-pipe write returned zero bytes for {}",
                    path.display()
                ),
            ));
        }
        offset += written;
    }
    Ok(())
}

fn write_once(
    handle: HANDLE,
    path: &Path,
    bytes: &[u8],
    deadline: Instant,
) -> Result<usize, EngineApiError> {
    let event = create_event(path, "write")?;
    let mut overlapped = OVERLAPPED::default();
    overlapped.hEvent = event.raw();
    let count = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    // SAFETY: the buffer, event, and `OVERLAPPED` remain alive until the operation
    // completes or cancellation has been synchronously observed below.
    let started = unsafe { WriteFile(handle, bytes.as_ptr(), count, null_mut(), &mut overlapped) };
    complete_overlapped(handle, path, "write", started, &mut overlapped, deadline)
        .map(|value| value as usize)
}

fn read_line(
    pipe: &OwnedHandle,
    path: &Path,
    timeout: Duration,
) -> Result<Vec<u8>, EngineApiError> {
    let deadline = deadline_after(timeout);
    let mut response = Vec::new();
    loop {
        if response.len() >= MAX_RESPONSE_BYTES {
            return Err(EngineApiError::new(
                EngineErrorCode::TransportUnavailable,
                "engine control response exceeded the 1 MiB limit",
            ));
        }
        let mut chunk = [0u8; READ_CHUNK_BYTES];
        let read = read_once(pipe.raw(), path, &mut chunk, deadline)?;
        if read == 0 {
            return Err(EngineApiError::new(
                EngineErrorCode::TransportUnavailable,
                "engine control endpoint closed without a response",
            ));
        }
        response.extend_from_slice(&chunk[..read]);
        if let Some(newline) = response.iter().position(|byte| *byte == b'\n') {
            response.truncate(newline + 1);
            return Ok(response);
        }
    }
}

fn read_once(
    handle: HANDLE,
    path: &Path,
    buffer: &mut [u8],
    deadline: Instant,
) -> Result<usize, EngineApiError> {
    let event = create_event(path, "read")?;
    let mut overlapped = OVERLAPPED::default();
    overlapped.hEvent = event.raw();
    let count = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: the mutable buffer, event, and `OVERLAPPED` remain alive until the
    // operation completes or cancellation has been synchronously observed.
    let started = unsafe {
        ReadFile(
            handle,
            buffer.as_mut_ptr(),
            count,
            null_mut(),
            &mut overlapped,
        )
    };
    complete_overlapped(handle, path, "read", started, &mut overlapped, deadline)
        .map(|value| value as usize)
}

fn create_event(path: &Path, phase: &str) -> Result<OwnedHandle, EngineApiError> {
    // SAFETY: unnamed event with null security attributes and name.
    let event = unsafe { CreateEventW(null(), 1, 0, null()) };
    if event.is_null() {
        return Err(win32_error(phase, path, last_error()));
    }
    Ok(OwnedHandle(event))
}

fn complete_overlapped(
    handle: HANDLE,
    path: &Path,
    phase: &str,
    started: i32,
    overlapped: &mut OVERLAPPED,
    deadline: Instant,
) -> Result<u32, EngineApiError> {
    if started == 0 {
        let code = last_error();
        if code != ERROR_IO_PENDING {
            return Err(win32_error(phase, path, code));
        }
    }
    let wait_ms = match remaining_wait_ms(deadline) {
        Some(value) => value,
        None => {
            cancel_and_drain(handle, overlapped);
            return Err(timeout_error(phase, path));
        }
    };
    // SAFETY: the event is owned by the live `OVERLAPPED` operation.
    let wait = unsafe { WaitForSingleObject(overlapped.hEvent, wait_ms) };
    if wait == WAIT_TIMEOUT {
        cancel_and_drain(handle, overlapped);
        return Err(timeout_error(phase, path));
    }
    if wait != WAIT_OBJECT_0 {
        let code = last_error();
        cancel_and_drain(handle, overlapped);
        return Err(win32_error(phase, path, code));
    }
    let mut transferred = 0u32;
    // SAFETY: the event signaled completion and all arguments remain valid.
    if unsafe { GetOverlappedResult(handle, overlapped, &mut transferred, 0) } == 0 {
        return Err(win32_error(phase, path, last_error()));
    }
    Ok(transferred)
}

fn cancel_and_drain(handle: HANDLE, overlapped: &mut OVERLAPPED) {
    // SAFETY: the operation belongs to `handle`; waiting after cancellation keeps
    // the caller's buffer and `OVERLAPPED` alive until Windows is done with them.
    unsafe {
        CancelIoEx(handle, overlapped);
        let mut transferred = 0u32;
        GetOverlappedResult(handle, overlapped, &mut transferred, 1);
    }
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

fn deadline_after(timeout: Duration) -> Instant {
    let now = Instant::now();
    now.checked_add(timeout).unwrap_or(now)
}

fn remaining_wait_ms(deadline: Instant) -> Option<u32> {
    let remaining = deadline.checked_duration_since(Instant::now())?;
    if remaining.is_zero() {
        return None;
    }
    let millis = remaining.as_millis();
    let rounded = if remaining.subsec_nanos() % 1_000_000 == 0 {
        millis
    } else {
        millis.saturating_add(1)
    };
    Some(rounded.clamp(1, u32::MAX as u128) as u32)
}

fn last_error() -> u32 {
    // SAFETY: GetLastError has no preconditions.
    unsafe { GetLastError() }
}

fn timeout_error(phase: &str, path: &Path) -> EngineApiError {
    EngineApiError::new(
        EngineErrorCode::Timeout,
        format!(
            "Windows named-pipe {phase} deadline expired for {}",
            path.display()
        ),
    )
}

fn win32_error(phase: &str, path: &Path, code: u32) -> EngineApiError {
    EngineApiError::new(
        EngineErrorCode::TransportUnavailable,
        format!(
            "Windows named-pipe {phase} failed for {}: {} (Win32 {code})",
            path.display(),
            std::io::Error::from_raw_os_error(code as i32)
        ),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use serde_json::json;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_PIPE_CONNECTED, GetLastError, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
    };

    use super::wide_null;
    use crate::{ControlRequest, EngineErrorCode, call_windows_named_pipe};

    #[test]
    fn server_that_withholds_response_hits_read_deadline() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::path::PathBuf::from(format!(
            r"\\.\pipe\Saccade-timeout-test-{}-{nonce}",
            std::process::id()
        ));
        let server_path = path.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let wide = wide_null(server_path.as_os_str());
            // SAFETY: the name is NUL terminated and this test owns the handle.
            let pipe = unsafe {
                CreateNamedPipeW(
                    wide.as_ptr(),
                    PIPE_ACCESS_DUPLEX,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    1,
                    4096,
                    4096,
                    0,
                    std::ptr::null(),
                )
            };
            assert_ne!(pipe, INVALID_HANDLE_VALUE);
            ready_tx.send(()).unwrap();
            // SAFETY: the server owns a valid pipe handle and accepts one client.
            let connected = unsafe { ConnectNamedPipe(pipe, std::ptr::null_mut()) } != 0
                || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
            assert!(connected);
            thread::sleep(Duration::from_millis(300));
            // SAFETY: the test owns the connected server handle.
            unsafe {
                DisconnectNamedPipe(pipe);
                CloseHandle(pipe);
            }
        });
        ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        let request = ControlRequest {
            id: 1,
            method: "ping".to_string(),
            params: json!({}),
            capability: "a".repeat(43),
        };
        let started = Instant::now();
        let error = call_windows_named_pipe(&path, &request, Duration::from_millis(75))
            .expect_err("withheld response must time out");
        let elapsed = started.elapsed();
        assert_eq!(error.code, EngineErrorCode::Timeout);
        assert!(error.detail.contains("read deadline expired"));
        assert!(elapsed >= Duration::from_millis(50));
        assert!(elapsed < Duration::from_millis(500));
        server.join().unwrap();
    }
}
