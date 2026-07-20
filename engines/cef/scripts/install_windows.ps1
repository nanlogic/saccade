[CmdletBinding()]
param(
  [string]$SourceDir = '',
  [string]$InstallDir = '',
  [switch]$NoLaunch
)

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptRoot '..\..\..')).Path
if (-not $SourceDir) {
  $SourceDir = Join-Path $repoRoot 'target\cef-windows64\Saccade'
}
if (-not $InstallDir) {
  $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\Saccade'
}
$SourceDir = (Resolve-Path -LiteralPath $SourceDir).Path

$sourceExe = Join-Path $SourceDir 'Saccade.exe'
if (-not (Test-Path -LiteralPath $sourceExe)) {
  throw "Missing Saccade package: $sourceExe"
}

New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
Copy-Item -Path (Join-Path $SourceDir '*') -Destination $InstallDir -Recurse -Force
Remove-Item -LiteralPath (Join-Path $InstallDir 'cefsimple.exe') -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath (Join-Path $InstallDir 'cefsimple.dll') -Force -ErrorAction SilentlyContinue
& (Join-Path $scriptRoot 'grant_windows_lpac.ps1') -Path $InstallDir
& (Join-Path $scriptRoot 'register_windows_default_browser.ps1') -InstallDir $InstallDir | Out-Null
& (Join-Path $scriptRoot 'register_windows_agent_native_host.ps1') `
  -InstallDir $InstallDir | Out-Null
& (Join-Path $scriptRoot 'pin_windows_agent_action.ps1') | Out-Null

$installedMcp = Join-Path $InstallDir 'saccade-mcp.exe'
if (-not (Test-Path -LiteralPath $installedMcp)) {
  throw "Missing installed MCP: $installedMcp"
}
$registration = & $installedMcp register-codex
if ($LASTEXITCODE -ne 0) {
  throw "Saccade MCP registration failed with exit code $LASTEXITCODE"
}
Write-Host "Saccade MCP registration: $registration"
$installedExe = Join-Path $InstallDir 'Saccade.exe'
$shell = New-Object -ComObject WScript.Shell
$shortcutPaths = @(
  (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Saccade.lnk'),
  (Join-Path ([Environment]::GetFolderPath('Programs')) 'Saccade.lnk')
)
foreach ($shortcutPath in $shortcutPaths) {
  $shortcut = $shell.CreateShortcut($shortcutPath)
  $shortcut.TargetPath = $installedExe
  $shortcut.WorkingDirectory = $InstallDir
  $shortcut.IconLocation = "$installedExe,0"
  $shortcut.Description = 'Saccade dogfood browser'
  $shortcut.Save()
}

Write-Host "Installed Saccade to $InstallDir"
Write-Host "Created shortcuts: $($shortcutPaths -join ', ')"

if (-not $NoLaunch) {
  Start-Process -FilePath $installedExe -ArgumentList @(
    '--no-first-run',
    '--no-default-browser-check'
  )
}

$InstallDir
