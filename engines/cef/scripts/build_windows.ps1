[CmdletBinding()]
param(
  [string]$CefRoot = '',
  [string]$BuildRoot = '',
  [ValidateSet('Debug', 'Release')][string]$Configuration = 'Release',
  [string]$SigningThumbprint = $env:SACCADE_WINDOWS_SIGNING_THUMBPRINT,
  [string]$CmakePath = '',
  [string]$CargoPath = ''
)

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptRoot '..\..\..')).Path
if (-not $BuildRoot) { $BuildRoot = Join-Path $repoRoot 'target\cef-windows64' }
if (-not $CefRoot) {
  $CefRoot = (& (Join-Path $scriptRoot 'prepare_windows.ps1') | Select-Object -Last 1)
} else {
  $CefRoot = (& (Join-Path $scriptRoot 'prepare_windows.ps1') `
    -CefRoot $CefRoot | Select-Object -Last 1)
}

if ($CmakePath) {
  $cmakePath = (Resolve-Path -LiteralPath $CmakePath).Path
} else {
  $cmakeCommand = Get-Command cmake.exe -ErrorAction SilentlyContinue
  if (-not $cmakeCommand) {
    throw 'cmake.exe was not found on PATH; pass -CmakePath explicitly'
  }
  $cmakePath = $cmakeCommand.Source
}
if ($CargoPath) {
  $cargoPath = (Resolve-Path -LiteralPath $CargoPath).Path
} else {
  $cargoCommand = Get-Command cargo.exe -ErrorAction SilentlyContinue
  if (-not $cargoCommand) {
    throw 'cargo.exe was not found on PATH; pass -CargoPath explicitly'
  }
  $cargoPath = $cargoCommand.Source
}

$upstreamBuild = Join-Path $BuildRoot 'upstream'
New-Item -ItemType Directory -Force -Path $upstreamBuild | Out-Null
& $cmakePath -S $CefRoot -B $upstreamBuild -G 'Visual Studio 17 2022' -A x64 -DUSE_SANDBOX=ON
if ($LASTEXITCODE -ne 0) { throw "CEF CMake configure failed with exit code $LASTEXITCODE" }
& $cmakePath --build $upstreamBuild --config $Configuration --target Saccade --parallel
if ($LASTEXITCODE -ne 0) { throw "CEF build failed with exit code $LASTEXITCODE" }

# The release-profile executable is rejected by WDAC on the dogfood machine.
# Keep CEF Release while packaging the policy-compatible MCP dev artifact.
& $cargoPath build -p saccade-mcp --manifest-path (Join-Path $repoRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw "saccade-mcp build failed with exit code $LASTEXITCODE" }

$sourceDir = Join-Path $upstreamBuild "tests\cefsimple\$Configuration"
$packageDir = Join-Path $BuildRoot 'Saccade'
if (-not (Test-Path -LiteralPath (Join-Path $sourceDir 'Saccade.exe')) -or
    -not (Test-Path -LiteralPath (Join-Path $sourceDir 'Saccade.dll'))) {
  throw "Missing upstream Saccade output: $sourceDir"
}
if (Test-Path -LiteralPath $packageDir) {
  Remove-Item -LiteralPath $packageDir -Recurse -Force
}
New-Item -ItemType Directory -Path $packageDir | Out-Null
Copy-Item -Path (Join-Path $sourceDir '*') -Destination $packageDir -Recurse -Force
Remove-Item -LiteralPath (Join-Path $packageDir 'cefsimple.exe') -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath (Join-Path $packageDir 'cefsimple.dll') -Force -ErrorAction SilentlyContinue

$saccadeExe = Join-Path $packageDir 'Saccade.exe'
& (Join-Path $scriptRoot 'brand_windows_exe.ps1') `
  -Executable $saccadeExe `
  -Icon (Join-Path $repoRoot 'engines\cef\assets\Saccade.ico')

& (Join-Path $scriptRoot 'brand_windows_pak.ps1') `
  -PackageDir $packageDir `
  -Icon (Join-Path $repoRoot 'engines\cef\assets\Saccade.ico')

& (Join-Path $scriptRoot 'brand_windows_locales.ps1') -PackageDir $packageDir

$extensionSource = Join-Path $repoRoot 'engines\cef\extensions\saccade-new-tab'
$extensionDir = Join-Path $packageDir 'extensions\saccade-new-tab'
New-Item -ItemType Directory -Force -Path $extensionDir | Out-Null
Copy-Item -Path (Join-Path $extensionSource '*') -Destination $extensionDir -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'engines\cef\assets\Saccade.ico') `
  -Destination (Join-Path $extensionDir 'Saccade.ico') -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'engines\cef\assets\saccade-icon-windows.png') `
  -Destination (Join-Path $extensionDir 'Saccade.png') -Force

Copy-Item -LiteralPath (Join-Path $repoRoot 'target\debug\saccade-mcp.exe') `
  -Destination (Join-Path $packageDir 'saccade-mcp.exe') -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'engines\cef\release\saccade-current-tab-mcp.cmd') `
  -Destination (Join-Path $packageDir 'saccade-current-tab-mcp.cmd') -Force
Copy-Item -LiteralPath (Join-Path $CefRoot 'LICENSE.txt') -Destination (Join-Path $packageDir 'CEF_LICENSE.txt') -Force
Copy-Item -LiteralPath (Join-Path $CefRoot 'CREDITS.html') -Destination (Join-Path $packageDir 'CHROMIUM_CREDITS.html') -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'LICENSE') -Destination (Join-Path $packageDir 'SACCADE_LICENSE.txt') -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'NOTICE') -Destination (Join-Path $packageDir 'NOTICE.txt') -Force
Copy-Item -LiteralPath (Join-Path $repoRoot 'TRADEMARKS.md') -Destination (Join-Path $packageDir 'TRADEMARKS.md') -Force

$packageSigned = -not [string]::IsNullOrWhiteSpace($SigningThumbprint)
if ($packageSigned) {
  & (Join-Path $scriptRoot 'sign_windows_package.ps1') `
    -PackageDir $packageDir -CertificateThumbprint $SigningThumbprint
}

@{
  product = 'Saccade'
  version = '0.1.0-windows-dogfood'
  build = 79
  platform = 'windows64'
  cef_version = '150.0.11+gb887805+chromium-150.0.7871.115'
  chromium_version = '150.0.7871.115'
  google_api_credentials = 'not_bundled'
  sandbox = $true
  agent_bridge = $true
  agent_transport = 'owner_only_windows_pipe_v1'
  protected_fill = $true
  mcp = $true
  mcp_build_profile = 'dev_policy_compatible'
  mcp_tools = 32
  mcp_reflex_loop = 'same_webview_fact_native_input_receipt_v1'
  codex_registration = 'first_launch_installer_saccade_default_v2'
  saccade_new_tab = $true
  agent_toolbar_action = 'native_chromium_pinned_action'
  new_tab_branding = 'chromium_default_favicon_pak'
  ui_product_name = 'Saccade'
  signed = $packageSigned
  public_distribution_ready = $false
} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $packageDir 'SACCADE_VERSION.json') -Encoding utf8

& (Join-Path $scriptRoot 'grant_windows_lpac.ps1') -Path $packageDir
& (Join-Path $scriptRoot 'write_windows_package_manifest.ps1') `
  -PackageDir $packageDir | Out-Null

$packageDir
