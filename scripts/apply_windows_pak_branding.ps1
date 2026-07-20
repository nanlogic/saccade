[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$buildPath = Join-Path $repoRoot 'engines\cef\scripts\build_windows.ps1'
$lines = [System.Collections.ArrayList]@(Get-Content -LiteralPath $buildPath)

if (-not ($lines -match 'brand_windows_pak\.ps1')) {
  $exeBrandIndex = $lines.IndexOf("& (Join-Path `$scriptRoot 'brand_windows_exe.ps1') ``")
  if ($exeBrandIndex -lt 0) { throw 'Could not locate Windows EXE branding block' }
  $pakBlock = @(
    ''
    "& (Join-Path `$scriptRoot 'brand_windows_pak.ps1') ``"
    "  -PackageDir `$packageDir ``"
    "  -Icon (Join-Path `$repoRoot 'engines\cef\assets\Saccade.ico')"
  )
  $lines.InsertRange($exeBrandIndex + 3, $pakBlock)
}

for ($index = 0; $index -lt $lines.Count; $index++) {
  if ($lines[$index] -match '^  build = \d+$') { $lines[$index] = '  build = 69' }
}
if (-not ($lines -match '^  new_tab_branding =')) {
  $newTabIndex = $lines.IndexOf('  saccade_new_tab = $true')
  if ($newTabIndex -lt 0) { throw 'Could not locate new-tab manifest field' }
  $lines.Insert($newTabIndex + 1, "  new_tab_branding = 'chromium_default_favicon_pak'")
}
Set-Content -LiteralPath $buildPath -Value $lines -Encoding utf8
Write-Output 'Applied Windows Chromium favicon pack branding.'
