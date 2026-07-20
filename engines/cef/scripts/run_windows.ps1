[CmdletBinding()]
param(
  [ValidateSet('normal', 'incognito')][string]$Mode = 'normal',
  [string]$Url = 'https://example.com',
  [string]$PackageRoot = '',
  [switch]$Wait
)

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptRoot '..\..\..')).Path
if (-not $PackageRoot) {
  $PackageRoot = Join-Path $repoRoot 'target\cef-windows64\Saccade'
}
$executable = Join-Path $PackageRoot 'Saccade.exe'
if (-not (Test-Path -LiteralPath $executable)) {
  throw "Saccade Windows package is missing; run build_windows.ps1 first: $executable"
}

$arguments = @("--url=$Url", '--no-first-run', '--no-default-browser-check')
$privateProfile = $null
if ($Mode -eq 'normal') {
  $profileRoot = Join-Path $env:LOCALAPPDATA 'Saccade\CEF\Profiles\default'
  New-Item -ItemType Directory -Force -Path $profileRoot | Out-Null
  $arguments += "--user-data-dir=$profileRoot"
} else {
  $privateParent = Join-Path $env:LOCALAPPDATA 'Saccade\CEF\Incognito'
  New-Item -ItemType Directory -Force -Path $privateParent | Out-Null
  $privateProfile = Join-Path $privateParent ("session.{0}" -f [Guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Path $privateProfile | Out-Null
  $arguments += "--user-data-dir=$privateProfile"
  $arguments += '--incognito'
}

$process = Start-Process -FilePath $executable -ArgumentList $arguments -PassThru
if ($Wait -or $Mode -eq 'incognito') {
  $process.WaitForExit()
}
if ($privateProfile -and $process.HasExited) {
  Remove-Item -LiteralPath $privateProfile -Recurse -Force
}
$process
