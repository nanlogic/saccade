[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$buildPath = Join-Path $repoRoot 'engines\cef\scripts\build_windows.ps1'
$installPath = Join-Path $repoRoot 'engines\cef\scripts\install_windows.ps1'
$nl = [Environment]::NewLine

$build = (Get-Content -LiteralPath $buildPath -Raw).Replace('`r`n', $nl)
$releaseCommand = "& `$cargoPath build --release -p saccade-mcp --manifest-path (Join-Path `$repoRoot 'Cargo.toml')"
$dogfoodCommand = @(
  '# The release-profile executable is rejected by WDAC on the dogfood machine.'
  '# Keep CEF Release while packaging the policy-compatible MCP dev artifact.'
  "& `$cargoPath build -p saccade-mcp --manifest-path (Join-Path `$repoRoot 'Cargo.toml')"
) -join $nl
$build = $build.Replace($releaseCommand, $dogfoodCommand)

$packageCopy = "Copy-Item -Path (Join-Path `$sourceDir '*') -Destination `$packageDir -Recurse -Force"
if ($build -notmatch "packageDir 'cefsimple\.exe'") {
  $packageCopyAndCleanup = @(
    $packageCopy
    "Remove-Item -LiteralPath (Join-Path `$packageDir 'cefsimple.exe') -Force -ErrorAction SilentlyContinue"
    "Remove-Item -LiteralPath (Join-Path `$packageDir 'cefsimple.dll') -Force -ErrorAction SilentlyContinue"
  ) -join $nl
  $build = $build.Replace($packageCopy, $packageCopyAndCleanup)
}
$build = $build.Replace('target\release\saccade-mcp.exe', 'target\debug\saccade-mcp.exe')
$build = $build.Replace('  build = 67', '  build = 68')
if ($build -notmatch '(?m)^  mcp_build_profile =') {
  $build = [regex]::Replace(
    $build,
    '(?m)^  mcp = \$true\r?$',
    "  mcp = `$true${nl}  mcp_build_profile = 'dev_policy_compatible'"
  )
}
Set-Content -LiteralPath $buildPath -Value $build -Encoding utf8

$install = (Get-Content -LiteralPath $installPath -Raw).Replace('`r`n', $nl)
$installCopy = "Copy-Item -Path (Join-Path `$SourceDir '*') -Destination `$InstallDir -Recurse -Force"
if ($install -notmatch "InstallDir 'cefsimple\.exe'") {
  $installCopyAndCleanup = @(
    $installCopy
    "Remove-Item -LiteralPath (Join-Path `$InstallDir 'cefsimple.exe') -Force -ErrorAction SilentlyContinue"
    "Remove-Item -LiteralPath (Join-Path `$InstallDir 'cefsimple.dll') -Force -ErrorAction SilentlyContinue"
  ) -join $nl
  $install = $install.Replace($installCopy, $installCopyAndCleanup)
}
Set-Content -LiteralPath $installPath -Value $install -Encoding utf8

Write-Output 'Applied Windows MCP dogfood profile and stale-target cleanup.'
