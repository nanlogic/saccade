//! Owner-only local IPC. MCP reaches Native Host mode only through this boundary.

#[cfg(unix)]
mod unix {
    use std::fs;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use saccade_protocol::{
        ControlRequest, ControlResponse, LocalAddress, ProtocolError, MAX_LOCAL_MESSAGE_BYTES,
    };

    pub struct OwnerIpcServer {
        listener: UnixListener,
        path: PathBuf,
    }

    impl OwnerIpcServer {
        pub fn bind(runtime_dir: &Path) -> Result<Self, ProtocolError> {
            fs::create_dir_all(runtime_dir).map_err(io_error)?;
            fs::set_permissions(runtime_dir, fs::Permissions::from_mode(0o700))
                .map_err(io_error)?;
            let path = runtime_dir.join(format!("host-{}.sock", std::process::id()));
            if path.exists() {
                fs::remove_file(&path).map_err(io_error)?;
            }
            let listener = UnixListener::bind(&path).map_err(io_error)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(io_error)?;
            Ok(Self { listener, path })
        }

        pub fn address(&self) -> LocalAddress {
            LocalAddress::Unix {
                path: self.path.clone(),
            }
        }

        pub fn serve_once(
            self,
            handler: impl FnOnce(ControlRequest) -> ControlResponse,
        ) -> Result<(), ProtocolError> {
            let (stream, _) = self.listener.accept().map_err(io_error)?;
            handle_stream(stream, handler)
        }

        pub fn serve(
            self,
            handler: Arc<dyn Fn(ControlRequest) -> ControlResponse + Send + Sync>,
        ) -> Result<(), ProtocolError> {
            for incoming in self.listener.incoming() {
                let stream = incoming.map_err(io_error)?;
                let handler = Arc::clone(&handler);
                std::thread::Builder::new()
                    .name("saccade-owner-request".into())
                    .spawn(move || {
                        let _ = handle_stream(stream, |request| handler(request));
                    })
                    .map_err(io_error)?;
            }
            Ok(())
        }
    }

    impl Drop for OwnerIpcServer {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    fn handle_stream(
        stream: UnixStream,
        handler: impl FnOnce(ControlRequest) -> ControlResponse,
    ) -> Result<(), ProtocolError> {
        let mut line = Vec::new();
        BufReader::new(stream.try_clone().map_err(io_error)?)
            .take((MAX_LOCAL_MESSAGE_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(io_error)?;
        if line.len() > MAX_LOCAL_MESSAGE_BYTES {
            return Err(ProtocolError::MessageTooLarge);
        }
        let request: ControlRequest = serde_json::from_slice(&line).map_err(json_error)?;
        let bytes = serde_json::to_vec(&handler(request)).map_err(json_error)?;
        if bytes.len() > MAX_LOCAL_MESSAGE_BYTES {
            return Err(ProtocolError::MessageTooLarge);
        }
        let mut output = stream;
        output.write_all(&bytes).map_err(io_error)?;
        output.write_all(b"\n").map_err(io_error)?;
        output.flush().map_err(io_error)
    }

    fn io_error(error: std::io::Error) -> ProtocolError {
        ProtocolError::TransportUnavailable(error.to_string())
    }
    fn json_error(error: serde_json::Error) -> ProtocolError {
        ProtocolError::InvalidMessage(error.to_string())
    }
}

#[cfg(unix)]
pub use unix::OwnerIpcServer;

#[cfg(windows)]
mod windows {
    #![allow(unsafe_code)]

    use std::ffi::c_void;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use saccade_protocol::{
        ControlRequest, ControlResponse, LocalAddress, ProtocolError, MAX_LOCAL_MESSAGE_BYTES,
    };
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile, PIPE_ACCESS_DUPLEX};
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
        PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

    pub struct OwnerIpcServer {
        name: String,
    }

    impl OwnerIpcServer {
        pub fn bind(_runtime_dir: &Path) -> Result<Self, ProtocolError> {
            let mut random = [0_u8; 16];
            getrandom::fill(&mut random)
                .map_err(|error| ProtocolError::TransportUnavailable(error.to_string()))?;
            let suffix = random
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            Ok(Self {
                name: format!(r"\\.\pipe\Saccade-{}-{suffix}", std::process::id()),
            })
        }

        pub fn address(&self) -> LocalAddress {
            LocalAddress::WindowsNamedPipe {
                path: PathBuf::from(&self.name),
            }
        }

        pub fn serve(
            self,
            handler: Arc<dyn Fn(ControlRequest) -> ControlResponse + Send + Sync>,
        ) -> Result<(), ProtocolError> {
            loop {
                let handle = create_owner_pipe(&self.name)?;
                // SAFETY: handle is a live named-pipe server; null selects synchronous mode.
                let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) } != 0
                    // SAFETY: reads thread-local error state immediately after ConnectNamedPipe.
                    || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                if connected {
                    let result = handle_client(handle, &handler);
                    // SAFETY: handle is disconnected and closed exactly once.
                    unsafe {
                        DisconnectNamedPipe(handle);
                        CloseHandle(handle);
                    }
                    result?;
                } else {
                    // SAFETY: failed connection still leaves one live handle to close.
                    unsafe { CloseHandle(handle) };
                }
            }
        }
    }

    fn create_owner_pipe(name: &str) -> Result<*mut c_void, ProtocolError> {
        let descriptor_text = wide("D:P(A;;GA;;;OW)");
        let mut descriptor: *mut c_void = std::ptr::null_mut();
        // SAFETY: descriptor text is NUL-terminated and output pointer is valid.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                descriptor_text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(ProtocolError::PermissionDenied(
                "could not create owner-only pipe security descriptor".into(),
            ));
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let wide_name = wide(name);
        // SAFETY: wide name and security attributes live through this synchronous call.
        let handle = unsafe {
            CreateNamedPipeW(
                wide_name.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                &attributes,
            )
        };
        // SAFETY: descriptor was allocated by the conversion API and is freed once.
        unsafe { LocalFree(descriptor) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(ProtocolError::TransportUnavailable(
                std::io::Error::last_os_error().to_string(),
            ));
        }
        Ok(handle)
    }

    fn handle_client(
        handle: *mut c_void,
        handler: &Arc<dyn Fn(ControlRequest) -> ControlResponse + Send + Sync>,
    ) -> Result<(), ProtocolError> {
        let mut request_bytes = Vec::new();
        let mut chunk = [0_u8; 8192];
        while !request_bytes.ends_with(b"\n") {
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
                return Err(ProtocolError::TransportUnavailable(
                    std::io::Error::last_os_error().to_string(),
                ));
            }
            if read == 0 {
                return Err(ProtocolError::TransportUnavailable(
                    "named-pipe client closed before request".into(),
                ));
            }
            request_bytes.extend_from_slice(&chunk[..read as usize]);
            if request_bytes.len() > MAX_LOCAL_MESSAGE_BYTES {
                return Err(ProtocolError::MessageTooLarge);
            }
        }
        let request: ControlRequest = serde_json::from_slice(&request_bytes)
            .map_err(|error| ProtocolError::InvalidMessage(error.to_string()))?;
        let mut bytes = serde_json::to_vec(&handler(request))
            .map_err(|error| ProtocolError::InvalidMessage(error.to_string()))?;
        bytes.push(b'\n');
        if bytes.len() > MAX_LOCAL_MESSAGE_BYTES {
            return Err(ProtocolError::MessageTooLarge);
        }
        let mut written = 0_u32;
        // SAFETY: response bytes are readable and written is a valid output pointer.
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
            return Err(ProtocolError::TransportUnavailable(
                std::io::Error::last_os_error().to_string(),
            ));
        }
        Ok(())
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(windows)]
pub use windows::OwnerIpcServer;

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    use saccade_host_client::call_host;
    use saccade_protocol::{
        ControlRequest, ControlResponse, HostGrant, HOST_PROTOCOL, SESSION_CAPABILITY_SCHEME,
    };

    use super::OwnerIpcServer;

    #[test]
    fn owner_only_round_trip_uses_capability_bearer() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let server = OwnerIpcServer::bind(dir.path()).unwrap();
        let address = server.address();
        let capability = "c".repeat(43);
        let grant = HostGrant {
            protocol: HOST_PROTOCOL.into(),
            browser_instance_id: "browser-1".into(),
            address,
            capability_scheme: SESSION_CAPABILITY_SCHEME.into(),
            capability: capability.clone(),
        };
        let thread = std::thread::spawn(move || {
            server.serve_once(|request| {
                let allowed = request.capability == capability;
                ControlResponse {
                    id: request.id,
                    ok: allowed,
                    result: allowed.then(|| serde_json::json!({"route":"owner_only_ipc"})),
                    error: None,
                }
            })
        });
        let result = call_host(
            &grant,
            &ControlRequest {
                id: 7,
                method: "system.capabilities".into(),
                params: serde_json::json!({}),
                capability: "c".repeat(43),
            },
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(result["route"], "owner_only_ipc");
        thread.join().unwrap().unwrap();
    }
}
