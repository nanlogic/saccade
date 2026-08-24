[CmdletBinding()]
param(
  [switch]$Bootstrap,
  [switch]$PrepareOnly,
  [switch]$SkipBuild,
  [switch]$NoBrowser,
  [ValidateSet('auto', 'chrome', 'edge')]
  [string]$Browser = 'auto'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

$RepoRoot = Split-Path -Parent $PSScriptRoot
$ExtensionRoot = Join-Path $RepoRoot 'extension'
$ExtensionManifestPath = Join-Path $ExtensionRoot 'manifest.json'
$CandidatePath = Join-Path $ExtensionRoot 'candidate.json'
$SetupPackagePath = Join-Path $RepoRoot 'packages\setup\package.json'
$SetupCli = Join-Path $RepoRoot 'packages\setup\bin\saccade-setup.js'
$Runtime = Join-Path $RepoRoot 'target\release\saccade-runtime.exe'
$GeneratedRoot = Join-Path $RepoRoot '.saccade-source-install'
$ReleaseManifestPath = Join-Path $GeneratedRoot 'release.json'

function Invoke-External {
  param([string]$File, [string[]]$Arguments)
  & $File @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "$File failed with exit code $LASTEXITCODE"
  }
}

function Refresh-ProcessPath {
  $Machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
  $User = [Environment]::GetEnvironmentVariable('Path', 'User')
  $env:Path = "$Machine;$User"
}

function Install-WingetPackage {
  param([string]$Id, [string]$Override = '')
  $Arguments = @(
    'install', '--id', $Id, '--exact', '--silent', '--disable-interactivity',
    '--accept-package-agreements', '--accept-source-agreements'
  )
  if ($Override) { $Arguments += @('--override', $Override) }
  Invoke-External 'winget.exe' $Arguments
}

function Test-VcTools {
  $VsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
  if (-not (Test-Path $VsWhere)) { return $false }
  $Installation = & $VsWhere -latest -products '*' `
    -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
    -property installationPath
  return [bool]($Installation | Select-Object -First 1)
}

function Install-MissingPrerequisites {
  if (-not (Get-Command winget.exe -ErrorAction SilentlyContinue)) {
    throw 'PREREQUISITES_REQUIRED: winget is unavailable; install Node.js 18+, Rust stable MSVC, and Visual Studio C++ Build Tools.'
  }
  if (-not (Get-Command node.exe -ErrorAction SilentlyContinue)) {
    Install-WingetPackage 'OpenJS.NodeJS.LTS'
  }
  if (-not (Get-Command cargo.exe -ErrorAction SilentlyContinue)) {
    Install-WingetPackage 'Rustlang.Rustup'
  }
  if (-not (Test-VcTools)) {
    Install-WingetPackage 'Microsoft.VisualStudio.2022.BuildTools' `
      '--wait --quiet --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
  }
  Refresh-ProcessPath
  if (Get-Command rustup.exe -ErrorAction SilentlyContinue) {
    Invoke-External 'rustup.exe' @('default', 'stable-x86_64-pc-windows-msvc')
  }
}

function Get-ExtensionId {
  param([string]$PublicKey)
  $Bytes = [Convert]::FromBase64String($PublicKey)
  $Hasher = [Security.Cryptography.SHA256]::Create()
  try { $Hash = $Hasher.ComputeHash($Bytes) } finally { $Hasher.Dispose() }
  $Hex = -join ($Hash[0..15] | ForEach-Object { $_.ToString('x2') })
  return -join ($Hex.ToCharArray() | ForEach-Object {
    [char](97 + [Convert]::ToInt32($_.ToString(), 16))
  })
}

function Open-ExtensionManager {
  param([string]$Family)
  $Chrome = @(
    (Join-Path $env:ProgramFiles 'Google\Chrome\Application\chrome.exe'),
    (Join-Path ${env:ProgramFiles(x86)} 'Google\Chrome\Application\chrome.exe'),
    (Join-Path $env:LOCALAPPDATA 'Google\Chrome\Application\chrome.exe')
  ) | Where-Object { Test-Path $_ } | Select-Object -First 1
  $Edge = @(
    (Join-Path ${env:ProgramFiles(x86)} 'Microsoft\Edge\Application\msedge.exe'),
    (Join-Path $env:ProgramFiles 'Microsoft\Edge\Application\msedge.exe')
  ) | Where-Object { Test-Path $_ } | Select-Object -First 1
  if ($Family -eq 'chrome') { $Executable = $Chrome; $Url = 'chrome://extensions/' }
  elseif ($Family -eq 'edge') { $Executable = $Edge; $Url = 'edge://extensions/' }
  elseif ($Chrome) { $Executable = $Chrome; $Url = 'chrome://extensions/' }
  else { $Executable = $Edge; $Url = 'edge://extensions/' }
  if ($Executable) { & $Executable $Url }
}

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  throw 'This installer supports Windows x64 only.'
}
$Architecture = @($env:PROCESSOR_ARCHITECTURE, $env:PROCESSOR_ARCHITEW6432)
if ($Architecture -notcontains 'AMD64') {
  throw 'This installer supports Windows x64 only.'
}
foreach ($Required in @($ExtensionManifestPath, $CandidatePath, $SetupPackagePath, $SetupCli)) {
  if (-not (Test-Path $Required)) { throw "Incomplete Saccade source tree: missing $Required" }
}

$Missing = @()
if (-not (Get-Command node.exe -ErrorAction SilentlyContinue)) { $Missing += 'Node.js 18+' }
if (-not (Get-Command cargo.exe -ErrorAction SilentlyContinue)) { $Missing += 'Rust stable MSVC' }
if (-not (Test-VcTools)) { $Missing += 'Visual Studio C++ Build Tools' }
if ($Missing.Count) {
  if (-not $Bootstrap) {
    throw "PREREQUISITES_REQUIRED: $($Missing -join ', '). Rerun with -Bootstrap only after the user approves installing them."
  }
  Install-MissingPrerequisites
}

$NodeVersion = (& node.exe --version).Trim()
$NodeMajor = [int]([regex]::Match($NodeVersion, '^v(\d+)').Groups[1].Value)
if ($NodeMajor -lt 18) { throw "Node.js 18+ is required; found $NodeVersion" }

if (-not $SkipBuild) {
  Push-Location $RepoRoot
  try { Invoke-External 'cargo.exe' @('build', '--release', '--locked', '--bin', 'saccade-runtime') }
  finally { Pop-Location }
}
if (-not (Test-Path $Runtime)) { throw "Windows Runtime was not built: $Runtime" }

$ExtensionManifest = Get-Content -Raw $ExtensionManifestPath | ConvertFrom-Json
$Candidate = Get-Content -Raw $CandidatePath | ConvertFrom-Json
$SetupPackage = Get-Content -Raw $SetupPackagePath | ConvertFrom-Json
if ($ExtensionManifest.name -ne 'Saccade' -or -not $ExtensionManifest.key) {
  throw 'The source Extension must retain the production name and deterministic public key.'
}
if ($ExtensionManifest.version -ne $Candidate.version) {
  throw 'The Extension manifest and candidate versions do not match.'
}
$ExtensionId = Get-ExtensionId $ExtensionManifest.key
if ($ExtensionId -notmatch '^[a-p]{32}$') { throw 'Could not derive the source Extension ID.' }

$ProbeRoot = Join-Path ([IO.Path]::GetTempPath()) "saccade-source-probe-$([Guid]::NewGuid())"
[IO.Directory]::CreateDirectory($ProbeRoot) | Out-Null
$PreviousRuntimeDir = $env:SACCADE_RUNTIME_DIR
try {
  $env:SACCADE_RUNTIME_DIR = $ProbeRoot
  $DoctorText = (& $Runtime doctor 2>$null | Out-String).Trim()
  $RuntimeDoctor = $DoctorText | ConvertFrom-Json
} finally {
  if ($null -eq $PreviousRuntimeDir) { Remove-Item Env:SACCADE_RUNTIME_DIR -ErrorAction SilentlyContinue }
  else { $env:SACCADE_RUNTIME_DIR = $PreviousRuntimeDir }
  Remove-Item -LiteralPath $ProbeRoot -Recurse -Force -ErrorAction SilentlyContinue
}
if ($RuntimeDoctor.runtime_version -ne $SetupPackage.version) {
  throw 'Runtime and setup package versions do not match.'
}
if ($RuntimeDoctor.mcp_contract_hash -notmatch '^[a-f0-9]{64}$') {
  throw 'Runtime doctor returned no valid MCP contract identity.'
}

[IO.Directory]::CreateDirectory($GeneratedRoot) | Out-Null
$Release = [ordered]@{
  schema = 'saccade.setup-release/1'
  published = $true
  source_build = $true
  source_extension = $ExtensionRoot
  version = $SetupPackage.version
  mcp_contract_hash = $RuntimeDoctor.mcp_contract_hash
  extension_candidate = $Candidate
  native_host = [ordered]@{
    name = 'com.nanlogic.saccade'
    allowed_origins = @("chrome-extension://$ExtensionId/")
  }
  artifacts = [ordered]@{
    'win32-x64' = [ordered]@{
      url = ([Uri](Resolve-Path $Runtime).Path).AbsoluteUri
      sha256 = (Get-FileHash -Algorithm SHA256 $Runtime).Hash.ToLowerInvariant()
      signed = $false
      source_build = $true
    }
  }
}
$ReleaseJson = $Release | ConvertTo-Json -Depth 12
[IO.File]::WriteAllText($ReleaseManifestPath, $ReleaseJson, [Text.UTF8Encoding]::new($false))

$Prepared = [ordered]@{
  schema = 'saccade.source-install/1'
  stage = 'prepared'
  runtime = $Runtime
  release_manifest = $ReleaseManifestPath
  extension_path = $ExtensionRoot
  extension_id = $ExtensionId
  extension_version = $Candidate.version
}
if ($PrepareOnly) {
  Write-Output "SACCADE_SOURCE_INSTALL_PREPARED $($Prepared | ConvertTo-Json -Compress)"
  return
}

Invoke-External 'node.exe' @($SetupCli, '--release-manifest', $ReleaseManifestPath)
& node.exe $SetupCli doctor
$DoctorStatus = $LASTEXITCODE
if ($DoctorStatus -eq 0) {
  $Ready = [ordered]@{
    schema = 'saccade.source-install/1'
    stage = 'ready'
    runtime = Join-Path $env:LOCALAPPDATA 'Saccade\runtime\saccade-runtime.exe'
    extension_id = $ExtensionId
    extension_version = $Candidate.version
  }
  Write-Output "SACCADE_SOURCE_INSTALL_READY $($Ready | ConvertTo-Json -Compress)"
  return
}

if (-not $NoBrowser) { Open-ExtensionManager $Browser }
$Pending = [ordered]@{
  schema = 'saccade.source-install/1'
  stage = 'extension_pending'
  extension_path = $ExtensionRoot
  extension_id = $ExtensionId
  extension_version = $Candidate.version
}
Write-Output "SACCADE_EXTENSION_PENDING $($Pending | ConvertTo-Json -Compress)"
$global:LASTEXITCODE = 0
