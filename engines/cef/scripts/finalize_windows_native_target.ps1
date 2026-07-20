[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$PreparePath,
  [Parameter(Mandatory = $true)][string]$BuildPath
)

$ErrorActionPreference = 'Stop'
$prepare = (Resolve-Path -LiteralPath $PreparePath).Path
$text = [IO.File]::ReadAllText($prepare).Replace("`r`n", "`n")
$marker = "Invoke-SaccadePatch -Marker 'saccade_windows_platform.cc'"
if (-not $text.Contains($marker)) { throw 'Prepare CMake marker missing' }
if (-not $text.Contains('prepare_windows_target_name.ps1')) {
  $insert = @"
& (Join-Path `$scriptRoot 'prepare_windows_target_name.ps1') ``
  -Path (Join-Path `$simpleRoot 'CMakeLists.txt')

"@
  $text = $text.Replace($marker, $insert + $marker)
}
[IO.File]::WriteAllText($prepare, $text, [Text.UTF8Encoding]::new($false))

$build = (Resolve-Path -LiteralPath $BuildPath).Path
$text = [IO.File]::ReadAllText($build).Replace("`r`n", "`n")
$text = $text.Replace('--target cefsimple --parallel',
                      '--target Saccade --parallel')
$old = @"
if (-not (Test-Path -LiteralPath (Join-Path `$sourceDir 'cefsimple.exe'))) {
  throw "Missing upstream cefsimple output: `$sourceDir"
}
New-Item -ItemType Directory -Force -Path `$packageDir | Out-Null
Copy-Item -Path (Join-Path `$sourceDir '*') -Destination `$packageDir -Recurse -Force

`$sourceExe = Join-Path `$packageDir 'cefsimple.exe'
`$sourceDll = Join-Path `$packageDir 'cefsimple.dll'
`$saccadeExe = Join-Path `$packageDir 'Saccade.exe'
Move-Item -LiteralPath `$sourceExe -Destination `$saccadeExe -Force
if (Test-Path -LiteralPath `$sourceDll) {
  Move-Item -LiteralPath `$sourceDll -Destination (Join-Path `$packageDir 'Saccade.dll') -Force
}
"@
$new = @"
if (-not (Test-Path -LiteralPath (Join-Path `$sourceDir 'Saccade.exe')) -or
    -not (Test-Path -LiteralPath (Join-Path `$sourceDir 'Saccade.dll'))) {
  throw "Missing upstream Saccade output: `$sourceDir"
}
New-Item -ItemType Directory -Force -Path `$packageDir | Out-Null
Copy-Item -Path (Join-Path `$sourceDir '*') -Destination `$packageDir -Recurse -Force

`$saccadeExe = Join-Path `$packageDir 'Saccade.exe'
"@
if ($text.Contains($old)) {
  $text = $text.Replace($old, $new)
} elseif (-not $text.Contains("Join-Path `$sourceDir 'Saccade.exe'")) {
  throw 'Build packaging block was not recognized'
}
[IO.File]::WriteAllText($build, $text, [Text.UTF8Encoding]::new($false))
