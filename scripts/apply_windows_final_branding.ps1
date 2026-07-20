[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$buildPath = Join-Path $repoRoot 'engines\cef\scripts\build_windows.ps1'
$installPath = Join-Path $repoRoot 'engines\cef\scripts\install_windows.ps1'

$build = [System.Collections.ArrayList]@(Get-Content -LiteralPath $buildPath)
if (-not ($build -match 'brand_windows_locales\.ps1')) {
  $pakIndex = $build.IndexOf("& (Join-Path `$scriptRoot 'brand_windows_pak.ps1') ``")
  if ($pakIndex -lt 0) { throw 'Could not locate Windows PAK branding block' }
  $build.InsertRange($pakIndex + 3, @(
    ''
    "& (Join-Path `$scriptRoot 'brand_windows_locales.ps1') -PackageDir `$packageDir"
  ))
}
for ($index = 0; $index -lt $build.Count; $index++) {
  if ($build[$index] -match '^  build = \d+$') { $build[$index] = '  build = 70' }
}
if (-not ($build -match '^  ui_product_name =')) {
  $brandingIndex = $build.IndexOf("  new_tab_branding = 'chromium_default_favicon_pak'")
  if ($brandingIndex -lt 0) { throw 'Could not locate new-tab branding field' }
  $build.Insert($brandingIndex + 1, "  ui_product_name = 'Saccade'")
}
Set-Content -LiteralPath $buildPath -Value $build -Encoding utf8

$install = [System.Collections.ArrayList]@(Get-Content -LiteralPath $installPath)
if (-not ($install -match 'register_windows_default_browser\.ps1')) {
  $grantIndex = $install.IndexOf("& (Join-Path `$scriptRoot 'grant_windows_lpac.ps1') -Path `$InstallDir")
  if ($grantIndex -lt 0) { throw 'Could not locate install ACL block' }
  $install.Insert($grantIndex + 1, "& (Join-Path `$scriptRoot 'register_windows_default_browser.ps1') -InstallDir `$InstallDir | Out-Null")
}
Set-Content -LiteralPath $installPath -Value $install -Encoding utf8
Write-Output 'Applied final Windows UI branding and default-browser registration.'
