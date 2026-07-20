[CmdletBinding()]
param(
  [string]$CacheRoot = "$env:LOCALAPPDATA\Saccade\cef\150.0.11"
)

$ErrorActionPreference = 'Stop'
$archiveName = 'cef_binary_150.0.11+gb887805+chromium-150.0.7871.115_windows64.tar.bz2'
$downloadName = 'cef_binary_150.0.11_windows64.tar.bz2'
$downloadUrl = 'https://cef-builds.spotifycdn.com/cef_binary_150.0.11%2Bgb887805%2Bchromium-150.0.7871.115_windows64.tar.bz2'
$expectedSha1 = '5166ca9e708c1e72e3bb1a8fbeb885faf4984202'
$expectedSha256 = '19df14cd13c8d491077fef921dbb9730a702c26bf5eee1ba7497178bb13df981'
$archivePath = Join-Path $CacheRoot $downloadName
$partialPath = "$archivePath.part"
$extractedRoot = Join-Path $CacheRoot ($archiveName -replace '\.tar\.bz2$', '')

New-Item -ItemType Directory -Force -Path $CacheRoot | Out-Null
if (-not (Test-Path -LiteralPath $archivePath)) {
  curl.exe --fail --location --output $partialPath $downloadUrl
  if ($LASTEXITCODE -ne 0) {
    throw "CEF download failed with exit code $LASTEXITCODE"
  }
  Move-Item -LiteralPath $partialPath -Destination $archivePath -Force
}

$actualSha1 = (Get-FileHash -Algorithm SHA1 -LiteralPath $archivePath).Hash.ToLowerInvariant()
$actualSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant()
if ($actualSha1 -ne $expectedSha1) {
  throw "CEF SHA-1 mismatch: expected $expectedSha1, got $actualSha1"
}
if ($actualSha256 -ne $expectedSha256) {
  throw "CEF SHA-256 mismatch: expected $expectedSha256, got $actualSha256"
}

if (-not (Test-Path -LiteralPath (Join-Path $extractedRoot 'CMakeLists.txt'))) {
  tar.exe -xjf $archivePath -C $CacheRoot
  if ($LASTEXITCODE -ne 0) {
    throw "CEF extraction failed with exit code $LASTEXITCODE"
  }
}

$extractedRoot
