[CmdletBinding()]
param([string]$Path = (Join-Path $PSScriptRoot '..\bins\saccade-mcp\src\main.rs'))

$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path $Path).Path
$text = [System.IO.File]::ReadAllText($resolved).Replace("`r`n", "`n")

function Replace-Exact {
  param([string]$Old, [string]$New)
  if (-not $script:text.Contains($Old)) {
    if ($script:text.Contains($New)) { return }
    throw "MCP Windows port lost expected source fragment: $Old"
  }
  $script:text = $script:text.Replace($Old, $New)
}

Replace-Exact @'
        let executable = std::env::var_os("SACCADE_APP_EXECUTABLE")
            .map(PathBuf::from)
            .context("SACCADE_APP_EXECUTABLE is not configured by the packaged MCP launcher")?;
'@ @'
        let executable = std::env::var_os("SACCADE_APP_EXECUTABLE")
            .map(PathBuf::from)
            .or_else(default_saccade_app_executable)
            .context("Saccade app executable is not installed or configured")?;
'@

Replace-Exact @'
        let agent_root = pointer
'@ @'
        #[cfg(windows)]
        {
            let spawn_result = ProcessCommand::new(&executable)
                .arg(format!("--url={}", url.as_str()))
                .args([
                    "--use-native",
                    "--no-first-run",
                    "--no-default-browser-check",
                    "--window-size=1440,1000",
                ])
                .env("SACCADE_ENGINE_INITIAL_TAB_GRANT", "1")
                .env("SACCADE_ENGINE_INITIAL_URL", url.as_str())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if let Err(error) = spawn_result {
                return Err(error)
                    .with_context(|| format!("failed to start {}", executable.display()));
            }
        }
        #[cfg(not(windows))]
        {
        let agent_root = pointer
'@

Replace-Exact @'
        if let Err(error) = spawn_result {
            let _ = fs::remove_file(&pointer);
            let _ = fs::remove_dir_all(&session);
            let _ = fs::remove_dir_all(&socket_session);
            return Err(error).with_context(|| format!("failed to start {}", executable.display()));
        }
    }

    let deadline
'@ @'
        if let Err(error) = spawn_result {
            let _ = fs::remove_file(&pointer);
            let _ = fs::remove_dir_all(&session);
            let _ = fs::remove_dir_all(&socket_session);
            return Err(error).with_context(|| format!("failed to start {}", executable.display()));
        }
        }
    }

    let deadline
'@

Replace-Exact @'
fn current_agent_pointer_path() -> Result<PathBuf> {
    std::env::var_os("SACCADE_CURRENT_AGENT_POINTER")
        .map(PathBuf::from)
        .context("packaged Saccade MCP launcher did not configure its broker pointer")
}
'@ @'
fn current_agent_pointer_path() -> Result<PathBuf> {
    std::env::var_os("SACCADE_CURRENT_AGENT_POINTER")
        .map(PathBuf::from)
        .or_else(default_current_agent_pointer)
        .context("Saccade broker pointer is not configured")
}

#[cfg(windows)]
fn default_current_agent_pointer() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| root.join("Saccade/CEF/Agent/current-grant-path"))
}

#[cfg(not(windows))]
fn default_current_agent_pointer() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn default_saccade_app_executable() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| root.join("Programs/Saccade/Saccade.exe"))
        .filter(|path| path.is_file())
}

#[cfg(not(windows))]
fn default_saccade_app_executable() -> Option<PathBuf> {
    None
}
'@

[System.IO.File]::WriteAllText($resolved, $text,
  [System.Text.UTF8Encoding]::new($false))
