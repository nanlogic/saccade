[CmdletBinding()]
param(
  [string]$OutputDir = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptRoot '..\..\..')).Path
if (-not $OutputDir) {
  $OutputDir = Join-Path $repoRoot 'runs\windows_dogfood\p0_4_staged_upgrade'
}
$OutputDir = [IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null

$testRoot = Join-Path ([IO.Path]::GetTempPath()) ("saccade-upgrade-test-{0}" -f [Guid]::NewGuid().ToString('N'))
$installDir = Join-Path $testRoot 'Programs\Saccade'
$profileRoot = Join-Path $testRoot 'Saccade\CEF\Profiles\default'
$installer = Join-Path $scriptRoot 'install_windows_staged.ps1'
$manifestWriter = Join-Path $scriptRoot 'write_windows_package_manifest.ps1'

function New-TestPackage(
  [string]$Root,
  [int]$Build,
  [hashtable]$AdditionalFiles
) {
  New-Item -ItemType Directory -Path $Root -Force | Out-Null
  @{
    'Saccade.exe' = "synthetic Saccade executable build $Build"
    'Saccade.dll' = "synthetic Saccade library build $Build"
    'saccade-mcp.exe' = "synthetic MCP build $Build"
  }.GetEnumerator() | ForEach-Object {
    Set-Content -LiteralPath (Join-Path $Root $_.Key) -Value $_.Value -Encoding utf8
  }
  foreach ($item in $AdditionalFiles.GetEnumerator()) {
    $path = Join-Path $Root $item.Key
    $parent = Split-Path -Parent $path
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    Set-Content -LiteralPath $path -Value $item.Value -Encoding utf8
  }
  [ordered]@{
    product = 'Saccade'
    version = "0.1.0-upgrade-test-$Build"
    build = $Build
    platform = 'windows64'
    public_distribution_ready = $false
  } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Root 'SACCADE_VERSION.json') -Encoding utf8
  & $manifestWriter -PackageDir $Root | Out-Null
}

function Get-TestRelativePath([string]$Root, [string]$File) {
  $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd([IO.Path]::DirectorySeparatorChar)
  $fileFull = [IO.Path]::GetFullPath($File)
  $prefix = $rootFull + [IO.Path]::DirectorySeparatorChar
  if (-not $fileFull.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "File is outside test root: $fileFull"
  }
  return $fileFull.Substring($prefix.Length).Replace('\', '/')
}

function Get-RelativeFiles([string]$Root) {
  return @(
    Get-ChildItem -LiteralPath $Root -File -Recurse -Force |
      ForEach-Object { Get-TestRelativePath $Root $_.FullName } |
      Sort-Object
  )
}

$report = [ordered]@{
  schema = 'saccade-windows-staged-upgrade-test-v1'
  verdict = 'FAIL'
  upgrades_completed = 0
  profile_preserved = $false
  stale_files_absent = $false
  rollback_restored_previous = $false
  transaction_artifacts_absent = $false
  locked_helper_stopped = $false
}

$lockedHelper = $null
try {
  New-Item -ItemType Directory -Path $installDir -Force | Out-Null
  Set-Content -LiteralPath (Join-Path $installDir 'obsolete-seed.dll') -Value 'seed stale file'
  Copy-Item -LiteralPath (Join-Path $env:SystemRoot 'System32\ping.exe') -Destination (Join-Path $installDir 'saccade-mcp.exe')
  $lockedHelper = Start-Process -FilePath (Join-Path $installDir 'saccade-mcp.exe') -ArgumentList @('-t', '127.0.0.1') -PassThru
  Start-Sleep -Milliseconds 250
  New-Item -ItemType Directory -Path $profileRoot -Force | Out-Null
  $profileSentinel = Join-Path $profileRoot 'profile-preservation.sentinel'
  Set-Content -LiteralPath $profileSentinel -Value 'profile must survive two upgrades' -Encoding utf8
  $profileHash = (Get-FileHash -LiteralPath $profileSentinel -Algorithm SHA256).Hash

  $package1 = Join-Path $testRoot 'packages\build101'
  $package2 = Join-Path $testRoot 'packages\build102'
  $package3 = Join-Path $testRoot 'packages\build103'
  New-TestPackage $package1 101 @{
    'resources\current-v1.pak' = 'v1 current resource'
    'resources\obsolete-v1.pak' = 'must disappear after v2'
  }
  New-TestPackage $package2 102 @{
    'resources\current-v2.pak' = 'v2 current resource'
  }
  New-TestPackage $package3 103 @{
    'resources\failed-v3.pak' = 'must disappear after rollback'
  }

  & $installer -SourceDir $package1 -InstallDir $installDir -ProfileRoot $profileRoot `
    -NoLaunch -SkipSystemIntegration | Out-Null
  $report.upgrades_completed = 1
  $lockedHelper.Refresh()
  if (-not $lockedHelper.HasExited) { throw 'Installer did not stop the locked package helper' }
  $report.locked_helper_stopped = $true
  if (Test-Path -LiteralPath (Join-Path $installDir 'obsolete-seed.dll')) {
    throw 'First staged upgrade retained a seed stale file'
  }

  & $installer -SourceDir $package2 -InstallDir $installDir -ProfileRoot $profileRoot `
    -NoLaunch -SkipSystemIntegration | Out-Null
  $report.upgrades_completed = 2
  if (Test-Path -LiteralPath (Join-Path $installDir 'resources\obsolete-v1.pak')) {
    throw 'Second staged upgrade retained a v1 stale file'
  }
  if (-not (Test-Path -LiteralPath (Join-Path $installDir 'resources\current-v2.pak'))) {
    throw 'Second staged upgrade is missing the v2 resource'
  }
  $expectedFiles = @(Get-RelativeFiles $package2)
  $installedFiles = @(Get-RelativeFiles $installDir)
  if (@(Compare-Object $expectedFiles $installedFiles).Count -ne 0) {
    throw 'Installed tree is not an exact copy of the v2 manifest package'
  }
  $report.stale_files_absent = $true

  $profileHashAfter = (Get-FileHash -LiteralPath $profileSentinel -Algorithm SHA256).Hash
  if ($profileHashAfter -ne $profileHash) {
    throw 'Profile sentinel changed during staged upgrades'
  }
  $report.profile_preserved = $true

  $rollbackTriggered = $false
  try {
    & $installer -SourceDir $package3 -InstallDir $installDir -ProfileRoot $profileRoot `
      -NoLaunch -SkipSystemIntegration -TestFailAfterSwap | Out-Null
  } catch {
    if ($_.Exception.Message -notmatch 'Injected post-swap failure') { throw }
    $rollbackTriggered = $true
  }
  if (-not $rollbackTriggered) { throw 'Rollback failure injection did not fire' }
  if (-not (Test-Path -LiteralPath (Join-Path $installDir 'resources\current-v2.pak')) -or
      (Test-Path -LiteralPath (Join-Path $installDir 'resources\failed-v3.pak'))) {
    throw 'Rollback did not restore the previous application tree'
  }
  $report.rollback_restored_previous = $true

  $transactionArtifacts = @(
    Get-ChildItem -LiteralPath (Split-Path -Parent $installDir) -Directory -Force |
      Where-Object { $_.Name -match '^Saccade\.(staging|backup)\.' }
  )
  if ($transactionArtifacts.Count -ne 0) {
    throw 'Staging or backup transaction directories remain after validation'
  }
  $report.transaction_artifacts_absent = $true
  $report.verdict = 'PASS'
} finally {
  if ($null -ne $lockedHelper -and -not $lockedHelper.HasExited) {
    Stop-Process -Id $lockedHelper.Id -Force -ErrorAction SilentlyContinue
  }
  $reportPath = Join-Path $OutputDir 'report.json'
  $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $reportPath -Encoding utf8
  if (Test-Path -LiteralPath $testRoot) {
    Remove-Item -LiteralPath $testRoot -Recurse -Force
  }
}

if ($report.verdict -ne 'PASS') {
  throw "Windows staged upgrade test failed; report: $reportPath"
}
Write-Host "WINDOWS_STAGED_UPGRADE PASS report=$reportPath"
Write-Output $reportPath
