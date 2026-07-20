[CmdletBinding()]
param(
  [string]$ConfigPath = (Join-Path $env:USERPROFILE '.codex\config.toml'),
  [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\Saccade'),
  [switch]$Repair
)

$ErrorActionPreference = 'Stop'
$ConfigPath = [IO.Path]::GetFullPath($ConfigPath)
$InstallDir = [IO.Path]::GetFullPath($InstallDir)
$mcpExecutable = Join-Path $InstallDir 'saccade-mcp.exe'
if (-not (Test-Path -LiteralPath $mcpExecutable)) {
  throw "Missing $mcpExecutable"
}

$arguments = @(
  'register-codex',
  '--config-path', $ConfigPath,
  '--install-dir', $InstallDir
)
if ($Repair) { $arguments += '--repair' }
& $mcpExecutable @arguments
if ($LASTEXITCODE -ne 0) {
  throw "Saccade MCP registration failed with exit code $LASTEXITCODE"
}
