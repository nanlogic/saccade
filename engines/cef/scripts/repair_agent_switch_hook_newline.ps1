[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$path = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) 'prepare_windows.ps1'
$text = [IO.File]::ReadAllText($path).Replace("`r`n", "`n")
$old = "'saccade_agent_switch_win.cc')& (Join-Path `$scriptRoot 'refine_windows_agent_switch.ps1'"
$new = "'saccade_agent_switch_win.cc')`n& (Join-Path `$scriptRoot 'refine_windows_agent_switch.ps1'"
if ($text.Contains($old)) {
  $text = $text.Replace($old, $new)
  [IO.File]::WriteAllText($path, $text, [Text.UTF8Encoding]::new($false))
}
if (-not ([IO.File]::ReadAllText($path).Contains(
    "'saccade_agent_switch_win.cc')`n& (Join-Path `$scriptRoot 'refine_windows_agent_switch.ps1'"))) {
  throw 'Agent switch refinement hook newline is still missing'
}
'Agent switch refinement hook formatting repaired.'
