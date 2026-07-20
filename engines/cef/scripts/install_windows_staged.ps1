[CmdletBinding()]
param(
  [string]$SourceDir = '',
  [string]$InstallDir = '',
  [string]$ProfileRoot = '',
  [switch]$NoLaunch,
  [switch]$SkipSystemIntegration,
  [switch]$TestFailAfterSwap
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptRoot '..\..\..')).Path
if (-not $SourceDir) {
  $SourceDir = Join-Path $repoRoot 'target\cef-windows64\Saccade'
}
if (-not $InstallDir) {
  $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\Saccade'
}
if (-not $ProfileRoot) {
  $ProfileRoot = Join-Path $env:LOCALAPPDATA 'Saccade\CEF\Profiles\default'
}
if ($SkipSystemIntegration -and -not $NoLaunch) {
  throw '-SkipSystemIntegration requires -NoLaunch'
}
if ($TestFailAfterSwap -and -not $SkipSystemIntegration) {
  throw '-TestFailAfterSwap is available only with -SkipSystemIntegration'
}

function Get-NormalizedFullPath([string]$Path) {
  return [IO.Path]::GetFullPath($Path).TrimEnd([IO.Path]::DirectorySeparatorChar)
}

function Test-PathWithin([string]$Candidate, [string]$Parent) {
  $candidateFull = Get-NormalizedFullPath $Candidate
  $parentFull = Get-NormalizedFullPath $Parent
  if ($candidateFull.Equals($parentFull, [StringComparison]::OrdinalIgnoreCase)) {
    return $true
  }
  $prefix = $parentFull + [IO.Path]::DirectorySeparatorChar
  return $candidateFull.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)
}

function Get-RelativePackagePath([string]$Root, [string]$File) {
  $rootFull = Get-NormalizedFullPath $Root
  $fileFull = [IO.Path]::GetFullPath($File)
  $prefix = $rootFull + [IO.Path]::DirectorySeparatorChar
  if (-not $fileFull.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "File is outside package root: $fileFull"
  }
  return $fileFull.Substring($prefix.Length).Replace('\', '/')
}

function Remove-SafeInstallTree([string]$Path, [string]$Parent) {
  if (-not (Test-Path -LiteralPath $Path)) { return }
  $pathFull = Get-NormalizedFullPath $Path
  $parentFull = Get-NormalizedFullPath $Parent
  if ($pathFull.Equals($parentFull, [StringComparison]::OrdinalIgnoreCase) -or
      -not (Test-PathWithin $pathFull $parentFull)) {
    throw "Refusing to remove path outside the install transaction root: $pathFull"
  }
  Remove-Item -LiteralPath $pathFull -Recurse -Force
}

function Test-SaccadePackage([string]$PackageDir) {
  $packageFull = Get-NormalizedFullPath $PackageDir
  $manifestPath = Join-Path $packageFull 'SACCADE_MANIFEST.json'
  $versionPath = Join-Path $packageFull 'SACCADE_VERSION.json'
  foreach ($required in @(
    'Saccade.exe', 'Saccade.dll', 'saccade-mcp.exe',
    'SACCADE_VERSION.json', 'SACCADE_MANIFEST.json'
  )) {
    if (-not (Test-Path -LiteralPath (Join-Path $packageFull $required) -PathType Leaf)) {
      throw "Saccade package is missing required file: $required"
    }
  }

  $version = Get-Content -LiteralPath $versionPath -Raw | ConvertFrom-Json
  $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
  if ($version.product -ne 'Saccade' -or $version.platform -ne 'windows64' -or
      -not $version.version -or $null -eq $version.build) {
    throw "Invalid Saccade version manifest: $versionPath"
  }
  if ($manifest.schema -ne 'saccade-windows-package-manifest-v1' -or
      $manifest.product -ne 'Saccade' -or
      [string]$manifest.version -ne [string]$version.version -or
      [int]$manifest.build -ne [int]$version.build) {
    throw "Package manifest does not match SACCADE_VERSION.json: $manifestPath"
  }

  $expected = @{}
  foreach ($entry in @($manifest.files)) {
    $relative = [string]$entry.path
    $normalized = $relative.Replace('\', '/')
    $segments = @($normalized.Split('/'))
    if (-not $relative -or [IO.Path]::IsPathRooted($relative) -or
        $segments -contains '..' -or $segments -contains '.' -or
        $normalized.StartsWith('/') -or $normalized.Contains('//')) {
      throw "Unsafe path in Saccade package manifest: $relative"
    }
    if ($expected.ContainsKey($normalized)) {
      throw "Duplicate path in Saccade package manifest: $normalized"
    }
    $filePath = Join-Path $packageFull $normalized
    if (-not (Test-PathWithin $filePath $packageFull) -or
        -not (Test-Path -LiteralPath $filePath -PathType Leaf)) {
      throw "Manifest file is missing or escaped the package: $normalized"
    }
    $file = Get-Item -LiteralPath $filePath
    if ([int64]$entry.size -ne $file.Length) {
      throw "Package size mismatch: $normalized"
    }
    $actualHash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne ([string]$entry.sha256).ToLowerInvariant()) {
      throw "Package checksum mismatch: $normalized"
    }
    $expected[$normalized] = $true
  }

  foreach ($required in @('Saccade.exe', 'Saccade.dll', 'saccade-mcp.exe', 'SACCADE_VERSION.json')) {
    if (-not $expected.ContainsKey($required)) {
      throw "Required package file is not checksummed: $required"
    }
  }
  $actual = @(
    Get-ChildItem -LiteralPath $packageFull -File -Recurse -Force |
      Where-Object { $_.FullName -ne $manifestPath } |
      ForEach-Object { Get-RelativePackagePath $packageFull $_.FullName }
  )
  foreach ($relative in $actual) {
    if (-not $expected.ContainsKey($relative)) {
      throw "Unmanifested file in Saccade package: $relative"
    }
  }
  if ($actual.Count -ne $expected.Count) {
    throw "Saccade package file count does not match its manifest"
  }
  return $version
}

function Get-InstalledPackageProcesses([string]$Root) {
  $rootFull = Get-NormalizedFullPath $Root
  return @(
    Get-Process -Name @('Saccade', 'saccade-mcp') -ErrorAction SilentlyContinue |
      Where-Object {
        try {
          $_.Path -and (Test-PathWithin $_.Path $rootFull)
        } catch {
          $false
        }
      }
  )
}

function Stop-InstalledPackage([string]$Root, [TimeSpan]$Timeout) {
  $processes = @(Get-InstalledPackageProcesses $Root)
  foreach ($process in @($processes | Where-Object { $_.ProcessName -eq 'Saccade' })) {
    if (-not $process.HasExited) {
      [void]$process.CloseMainWindow()
    }
  }
  $deadline = [DateTime]::UtcNow.Add($Timeout)
  do {
    $remainingBrowsers = @(
      Get-InstalledPackageProcesses $Root |
        Where-Object { $_.ProcessName -eq 'Saccade' }
    )
    if ($remainingBrowsers.Count -eq 0) { break }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $deadline)
  if ($remainingBrowsers.Count -ne 0) {
    $ids = ($remainingBrowsers | ForEach-Object { $_.Id }) -join ', '
    throw "Installed Saccade did not exit after a graceful close request; process IDs: $ids"
  }

  # MCP/native-host instances have no window or shutdown RPC. Once the browser
  # has exited, terminate only helpers whose executable is inside InstallDir.
  foreach ($helper in @(Get-InstalledPackageProcesses $Root)) {
    if ($helper.ProcessName -eq 'saccade-mcp' -and -not $helper.HasExited) {
      Stop-Process -Id $helper.Id -Force
    }
  }
  $helperDeadline = [DateTime]::UtcNow.AddSeconds(5)
  do {
    $remaining = @(Get-InstalledPackageProcesses $Root)
    if ($remaining.Count -eq 0) { return }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $helperDeadline)
  $ids = ($remaining | ForEach-Object { $_.Id }) -join ', '
  throw "Installed package processes did not exit before replacement; process IDs: $ids"
}

function Stop-LaunchedProcess($Process) {
  if ($null -eq $Process) { return }
  try {
    if (-not $Process.HasExited) {
      [void]$Process.CloseMainWindow()
      if (-not $Process.WaitForExit(3000)) {
        Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        $Process.WaitForExit(3000)
      }
    }
  } catch {
    Write-Warning "Could not stop failed replacement process: $($_.Exception.Message)"
  }
}

$SourceDir = (Resolve-Path -LiteralPath $SourceDir).Path
$InstallDir = Get-NormalizedFullPath $InstallDir
$ProfileRoot = Get-NormalizedFullPath $ProfileRoot
$installParent = Split-Path -Parent $InstallDir
$installLeaf = Split-Path -Leaf $InstallDir
if (-not $installParent -or -not $installLeaf) {
  throw "InstallDir must name a child directory: $InstallDir"
}
if ((Test-PathWithin $SourceDir $InstallDir) -or (Test-PathWithin $InstallDir $SourceDir)) {
  throw 'SourceDir and InstallDir must not contain each other'
}
if ((Test-PathWithin $ProfileRoot $InstallDir) -or (Test-PathWithin $InstallDir $ProfileRoot)) {
  throw 'ProfileRoot must remain outside the application install directory'
}

$sourceVersion = Test-SaccadePackage $SourceDir
New-Item -ItemType Directory -Path $installParent -Force | Out-Null
$transactionId = [Guid]::NewGuid().ToString('N')
$stageDir = Join-Path $installParent ("{0}.staging.{1}.{2}" -f $installLeaf, $sourceVersion.build, $transactionId)
$backupDir = Join-Path $installParent ("{0}.backup.{1}" -f $installLeaf, $transactionId)
$replacementActivated = $false
$hadPrevious = Test-Path -LiteralPath $InstallDir
$launchedProcess = $null

try {
  Stop-InstalledPackage $InstallDir ([TimeSpan]::FromSeconds(12))
  New-Item -ItemType Directory -Path $stageDir | Out-Null
  Get-ChildItem -LiteralPath $SourceDir -Force |
    Copy-Item -Destination $stageDir -Recurse -Force
  [void](Test-SaccadePackage $stageDir)

  if ($hadPrevious) {
    Move-Item -LiteralPath $InstallDir -Destination $backupDir
  }
  try {
    Move-Item -LiteralPath $stageDir -Destination $InstallDir
    $replacementActivated = $true
  } catch {
    if ($hadPrevious -and (Test-Path -LiteralPath $backupDir) -and
        -not (Test-Path -LiteralPath $InstallDir)) {
      Move-Item -LiteralPath $backupDir -Destination $InstallDir
    }
    throw
  }
  [void](Test-SaccadePackage $InstallDir)

  if ($TestFailAfterSwap) {
    throw 'Injected post-swap failure for rollback regression coverage'
  }

  if (-not $SkipSystemIntegration) {
    & (Join-Path $scriptRoot 'grant_windows_lpac.ps1') -Path $InstallDir
    & (Join-Path $scriptRoot 'register_windows_default_browser.ps1') -InstallDir $InstallDir | Out-Null
    & (Join-Path $scriptRoot 'register_windows_agent_native_host.ps1') -InstallDir $InstallDir | Out-Null
    & (Join-Path $scriptRoot 'pin_windows_agent_action.ps1') | Out-Null

    $installedMcp = Join-Path $InstallDir 'saccade-mcp.exe'
    $registration = & $installedMcp register-codex
    if ($LASTEXITCODE -ne 0) {
      throw "Saccade MCP registration failed with exit code $LASTEXITCODE"
    }
    Write-Host "Saccade MCP registration: $registration"
    $toolSmoke = & $installedMcp tools 2>&1
    if ($LASTEXITCODE -ne 0 -or ($toolSmoke -join "`n") -notmatch 'saccade\.system\.capabilities') {
      throw 'Installed Saccade MCP smoke test failed'
    }

    $installedExe = Join-Path $InstallDir 'Saccade.exe'
    $shell = New-Object -ComObject WScript.Shell
    $shortcutPaths = @(
      (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Saccade.lnk'),
      (Join-Path ([Environment]::GetFolderPath('Programs')) 'Saccade.lnk')
    )
    foreach ($shortcutPath in $shortcutPaths) {
      $shortcut = $shell.CreateShortcut($shortcutPath)
      $shortcut.TargetPath = $installedExe
      $shortcut.WorkingDirectory = $InstallDir
      $shortcut.IconLocation = "$installedExe,0"
      $shortcut.Description = 'Saccade dogfood browser'
      $shortcut.Save()
    }

    if (-not $NoLaunch) {
      New-Item -ItemType Directory -Path $ProfileRoot -Force | Out-Null
      $launchedProcess = Start-Process -FilePath $installedExe -ArgumentList @(
        '--no-first-run',
        '--no-default-browser-check',
        "--user-data-dir=$ProfileRoot"
      ) -PassThru
      Start-Sleep -Milliseconds 1000
      if ($launchedProcess.HasExited) {
        throw "Installed Saccade exited during launch smoke test with code $($launchedProcess.ExitCode)"
      }
    }
  }

  if ($hadPrevious -and (Test-Path -LiteralPath $backupDir)) {
    Remove-SafeInstallTree $backupDir $installParent
  }
  Write-Host "Installed Saccade $($sourceVersion.version) build $($sourceVersion.build) to $InstallDir"
  Write-Output $InstallDir
} catch {
  $originalFailure = $_
  Stop-LaunchedProcess $launchedProcess
  try {
    if ($replacementActivated -and (Test-Path -LiteralPath $InstallDir)) {
      Remove-SafeInstallTree $InstallDir $installParent
    }
    if ($hadPrevious -and (Test-Path -LiteralPath $backupDir)) {
      Move-Item -LiteralPath $backupDir -Destination $InstallDir
    }
    if (Test-Path -LiteralPath $stageDir) {
      Remove-SafeInstallTree $stageDir $installParent
    }
  } catch {
    throw "Saccade installation failed: $($originalFailure.Exception.Message); rollback also failed: $($_.Exception.Message)"
  }
  throw $originalFailure
}
