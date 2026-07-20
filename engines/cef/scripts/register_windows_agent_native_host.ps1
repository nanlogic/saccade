[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$InstallDir)

$ErrorActionPreference = 'Stop'
$resolvedInstall = (Resolve-Path -LiteralPath $InstallDir).Path
$hostExe = Join-Path $resolvedInstall 'saccade-mcp.exe'
if (-not (Test-Path -LiteralPath $hostExe)) {
  throw "Missing Saccade native host executable: $hostExe"
}
$manifestPath = Join-Path $resolvedInstall 'SaccadeAgentNativeHost.json'
$manifest = @{
  name = 'com.nanlogic.saccade_agent'
  description = 'Saccade per-tab Agent toolbar bridge'
  path = $hostExe
  type = 'stdio'
  allowed_origins = @('chrome-extension://kfmcgnphhefgadoabheodbhdndhfmonl/')
} | ConvertTo-Json -Depth 4
[IO.File]::WriteAllText($manifestPath, $manifest, [Text.UTF8Encoding]::new($false))

foreach ($registryPath in @(
  'HKCU:\Software\Chromium\NativeMessagingHosts\com.nanlogic.saccade_agent',
  'HKCU:\Software\Google\Chrome\NativeMessagingHosts\com.nanlogic.saccade_agent'
)) {
  New-Item -Path $registryPath -Force | Out-Null
  Set-Item -Path $registryPath -Value $manifestPath
}

$manifestPath
