[CmdletBinding()]
param(
  [string]$Path = (Join-Path $PSScriptRoot '..\crates\saccade_engine_api\src\lib.rs')
)

$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path $Path).Path
$text = [System.IO.File]::ReadAllText($resolved).Replace("`r`n", "`n")

function Replace-Exact {
  param([string]$Old, [string]$New)
  if (-not $script:text.Contains($Old)) {
    if ($script:text.Contains($New)) { return }
    throw "engine API Windows port lost expected source fragment: $Old"
  }
  $script:text = $script:text.Replace($Old, $New)
}

Replace-Exact @'
pub enum TransportAddress {
    Unix { path: PathBuf },
    Tcp { host: String, port: u16 },
}
'@ @'
pub enum TransportAddress {
    Unix { path: PathBuf },
    WindowsNamedPipe { path: PathBuf },
    Tcp { host: String, port: u16 },
}
'@

Replace-Exact @'
        if self.engine_adapter.provenance != "browser_process"
            || self.engine_adapter.transport != "owner_only_unix_v1"
        {
            return Err(EngineApiError::new(
                EngineErrorCode::PermissionDenied,
                "adapter contract 1.0 requires browser_process provenance and owner_only_unix_v1",
            ));
        }
'@ @'
        if self.engine_adapter.provenance != "browser_process" {
            return Err(EngineApiError::new(
                EngineErrorCode::PermissionDenied,
                "adapter contract 1.0 requires browser_process provenance",
            ));
        }
        let transport_matches = matches!(
            (&self.control_endpoint.address, self.engine_adapter.transport.as_str()),
            (TransportAddress::Unix { .. }, "owner_only_unix_v1")
                | (
                    TransportAddress::WindowsNamedPipe { .. },
                    "owner_only_windows_pipe_v1"
                )
        );
        if !transport_matches {
            return Err(EngineApiError::new(
                EngineErrorCode::PermissionDenied,
                "adapter transport does not match its owner-only endpoint scheme",
            ));
        }
'@

Replace-Exact @'
        TransportAddress::Unix { path } => call_unix(path, &request, read_timeout),
    }
}
'@ @'
        TransportAddress::Unix { path } => call_unix(path, &request, read_timeout),
        TransportAddress::WindowsNamedPipe { path } => {
            call_windows_named_pipe(path, &request, read_timeout)
        }
    }
}
'@

Replace-Exact @'
#[cfg(not(unix))]
fn call_unix(
'@ @'
#[cfg(windows)]
fn call_windows_named_pipe(
    path: &Path,
    request: &ControlRequest,
    _read_timeout: Duration,
) -> Result<Value, EngineApiError> {
    validate_windows_pipe_path(path)?;
    let stream = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| {
            EngineApiError::new(
                EngineErrorCode::TransportUnavailable,
                format!("failed to connect {}: {error}", path.display()),
            )
        })?;
    transact(stream, request)
}

#[cfg(not(windows))]
fn call_windows_named_pipe(
    path: &Path,
    _request: &ControlRequest,
    _read_timeout: Duration,
) -> Result<Value, EngineApiError> {
    Err(EngineApiError::new(
        EngineErrorCode::UnsupportedCapability,
        format!("Windows named-pipe transport is unavailable for {}", path.display()),
    ))
}

#[cfg(not(unix))]
fn call_unix(
'@

Replace-Exact @'
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
'@ @'
        TransportAddress::Unix { path } => {
            if !path.is_absolute() {
                return Err(EngineApiError::new(
                    EngineErrorCode::InvalidArgument,
                    "engine Unix socket path must be absolute",
                ));
            }
            Ok(())
        }
        TransportAddress::WindowsNamedPipe { path } => validate_windows_pipe_path(path),
    }
}

fn validate_windows_pipe_path(path: &Path) -> Result<(), EngineApiError> {
    let value = path.to_string_lossy();
    if !value.starts_with(r"\\.\pipe\Saccade-") || value.len() > 240 {
        return Err(EngineApiError::new(
            EngineErrorCode::PermissionDenied,
            "engine Windows named pipe must use the private Saccade pipe namespace",
        ));
    }
    Ok(())
}
'@

[System.IO.File]::WriteAllText($resolved, $text,
  [System.Text.UTF8Encoding]::new($false))
