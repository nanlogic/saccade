[CmdletBinding()]
param(
    [string]$CacheRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..\..\..")).Path
$LockPath = Join-Path $RepoRoot "engines\cef\cef.windows64.lock.json"
$Lock = Get-Content -Raw -LiteralPath $LockPath | ConvertFrom-Json

if ($Lock.platform -ne "windows64") {
    throw "Unexpected CEF platform in ${LockPath}: $($Lock.platform)"
}

if ([string]::IsNullOrWhiteSpace($CacheRoot)) {
    $CacheRoot = Join-Path $RepoRoot "target\cef-windows64"
}

$Package = $Lock.packages.minimal
$DownloadDir = Join-Path $CacheRoot "downloads"
$ArchivePath = Join-Path $DownloadDir $Package.file
$ExtractRoot = Join-Path $CacheRoot "upstream"
$ArchiveStem = $Package.file -replace '\.tar\.bz2$', ''
$CefRoot = Join-Path $ExtractRoot $ArchiveStem

New-Item -ItemType Directory -Force -Path $DownloadDir | Out-Null
New-Item -ItemType Directory -Force -Path $ExtractRoot | Out-Null

function Assert-ArchiveDigest {
    param([string]$Path)
    $Length = (Get-Item -LiteralPath $Path).Length
    if ($Length -ne [int64]$Package.size) {
        throw "CEF archive size mismatch: expected $($Package.size), got ${Length}"
    }
    $Actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    if ($Actual -ne $Package.sha256.ToLowerInvariant()) {
        throw "CEF SHA-256 mismatch: expected $($Package.sha256), got ${Actual}"
    }
}

if (Test-Path -LiteralPath $ArchivePath) {
    try {
        Assert-ArchiveDigest -Path $ArchivePath
    }
    catch {
        Remove-Item -Force -LiteralPath $ArchivePath
        throw
    }
}
else {
    Write-Host "Downloading pinned CEF $($Lock.cef_version) for Windows x64..."
    Invoke-WebRequest -Uri $Package.url -OutFile $ArchivePath -UseBasicParsing
    Assert-ArchiveDigest -Path $ArchivePath
}

if (-not (Test-Path -LiteralPath $CefRoot -PathType Container)) {
    $Tar = Get-Command tar.exe -ErrorAction SilentlyContinue
    if ($null -eq $Tar) {
        throw "tar.exe is required to extract the pinned CEF archive"
    }
    Write-Host "Extracting CEF into ${ExtractRoot}..."
    & $Tar.Source -xf $ArchivePath -C $ExtractRoot
    if ($LASTEXITCODE -ne 0) {
        throw "tar.exe failed with exit code $LASTEXITCODE"
    }
}

if (-not (Test-Path -LiteralPath (Join-Path $CefRoot "CMakeLists.txt") -PathType Leaf)) {
    throw "Pinned CEF root is incomplete: ${CefRoot}"
}

Write-Output $CefRoot
