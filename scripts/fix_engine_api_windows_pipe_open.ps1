[CmdletBinding()]
param([string]$Path = 'crates\saccade_engine_api\src\lib.rs')

$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath $Path).Path
$text = [IO.File]::ReadAllText($resolved).Replace("`r`n", "`n")
$old = @"
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
"@
$new = @"
    use std::os::windows::fs::OpenOptionsExt;

    validate_windows_pipe_path(path)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let stream = loop {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(path)
        {
            Ok(stream) => break stream,
            Err(error)
                if matches!(error.raw_os_error(), Some(2 | 231))
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => {
                return Err(EngineApiError::new(
                    EngineErrorCode::TransportUnavailable,
                    format!("failed to connect {}: {error}", path.display()),
                ));
            }
        }
    };
    transact(stream, request)
"@
if ($text.Contains($old)) {
  $text = $text.Replace($old, $new)
} elseif (-not $text.Contains('.share_mode(0)')) {
  throw 'Windows named-pipe open function was not recognized'
}
[IO.File]::WriteAllText($resolved, $text, [Text.UTF8Encoding]::new($false))
