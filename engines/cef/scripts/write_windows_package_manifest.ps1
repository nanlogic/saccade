[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$PackageDir
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-PackageRelativePath([string]$Root, [string]$File) {
  $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd([IO.Path]::DirectorySeparatorChar)
  $fileFull = [IO.Path]::GetFullPath($File)
  $prefix = $rootFull + [IO.Path]::DirectorySeparatorChar
  if (-not $fileFull.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "File is outside package root: $fileFull"
  }
  return $fileFull.Substring($prefix.Length).Replace('\', '/')
}

$PackageDir = (Resolve-Path -LiteralPath $PackageDir).Path
$versionPath = Join-Path $PackageDir 'SACCADE_VERSION.json'
if (-not (Test-Path -LiteralPath $versionPath -PathType Leaf)) {
  throw "Missing package version manifest: $versionPath"
}
$version = Get-Content -LiteralPath $versionPath -Raw | ConvertFrom-Json
if ($version.product -ne 'Saccade' -or $version.platform -ne 'windows64' -or
    -not $version.version -or $null -eq $version.build) {
  throw "Invalid Saccade Windows version manifest: $versionPath"
}

$required = @(
  'Saccade.exe',
  'Saccade.dll',
  'saccade-mcp.exe',
  'SACCADE_VERSION.json'
)
foreach ($relative in $required) {
  if (-not (Test-Path -LiteralPath (Join-Path $PackageDir $relative) -PathType Leaf)) {
    throw "Package is missing required file: $relative"
  }
}

$manifestPath = Join-Path $PackageDir 'SACCADE_MANIFEST.json'
$files = @(
  Get-ChildItem -LiteralPath $PackageDir -File -Recurse -Force |
    Where-Object { $_.FullName -ne $manifestPath } |
    Sort-Object FullName |
    ForEach-Object {
      $relative = Get-PackageRelativePath $PackageDir $_.FullName
      [ordered]@{
        path = $relative
        size = $_.Length
        sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
      }
    }
)
if ($files.Count -eq 0) {
  throw "Cannot create an empty Saccade package manifest"
}

$manifest = [ordered]@{
  schema = 'saccade-windows-package-manifest-v1'
  product = 'Saccade'
  version = [string]$version.version
  build = [int]$version.build
  generated_utc = [DateTime]::UtcNow.ToString('o')
  files = $files
}
$manifest | ConvertTo-Json -Depth 6 |
  Set-Content -LiteralPath $manifestPath -Encoding utf8
Write-Output $manifestPath
