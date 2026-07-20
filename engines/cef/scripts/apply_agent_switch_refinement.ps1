[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path

$preparePath = Join-Path $scriptRoot 'prepare_windows.ps1'
$prepare = [IO.File]::ReadAllText($preparePath).Replace("`r`n", "`n")
if (-not $prepare.Contains("'refine_windows_agent_switch.ps1'")) {
  $needle = @'
& (Join-Path $scriptRoot 'remove_windows_agent_button_heap.ps1') `
  -Path (Join-Path $simpleRoot 'saccade_agent_switch_win.cc')
'@
  $replacement = $needle + @'
& (Join-Path $scriptRoot 'refine_windows_agent_switch.ps1') `
  -Path (Join-Path $simpleRoot 'saccade_agent_switch_win.cc')
'@
  if (-not $prepare.Contains($needle)) {
    throw 'Could not locate the Windows Agent heap migration hook'
  }
  $prepare = $prepare.Replace($needle, $replacement)
  [IO.File]::WriteAllText($preparePath, $prepare,
    [Text.UTF8Encoding]::new($false))
}

$buildPath = Join-Path $scriptRoot 'build_windows.ps1'
$build = [IO.File]::ReadAllText($buildPath)
if ($build -match 'build = 70') {
  $build = $build.Replace('build = 70', 'build = 71')
  [IO.File]::WriteAllText($buildPath, $build,
    [Text.UTF8Encoding]::new($false))
} elseif ($build -notmatch 'build = 71') {
  throw 'Windows build number was neither 70 nor 71'
}

'Agent switch refinement wired into Windows Build 71.'
