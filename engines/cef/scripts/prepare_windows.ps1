[CmdletBinding()]
param([string]$CefRoot = '')

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptRoot '..\..\..')).Path
if (-not $CefRoot) {
  $CefRoot = (& (Join-Path $scriptRoot 'fetch_windows.ps1') | Select-Object -Last 1)
}
$CefRoot = (Resolve-Path $CefRoot).Path
$simpleRoot = Join-Path $CefRoot 'tests\cefsimple'

function Invoke-SaccadePatch {
  param(
    [Parameter(Mandatory = $true)][string]$Marker,
    [Parameter(Mandatory = $true)][string]$PatchFile,
    [string[]]$Exclude = @()
  )
  $files = Get-ChildItem -LiteralPath $simpleRoot -Recurse -File
  if (Select-String -LiteralPath $files.FullName -SimpleMatch $Marker -List |
      Select-Object -First 1) {
    return
  }
  Push-Location $CefRoot
  try {
    $arguments = @('apply', '--unsafe-paths', '--whitespace=nowarn', '--recount')
    foreach ($path in $Exclude) { $arguments += "--exclude=$path" }
    $arguments += $PatchFile
    & git @arguments
    if ($LASTEXITCODE -ne 0) { throw "Patch failed: $PatchFile" }
  } finally {
    Pop-Location
  }
}

Invoke-SaccadePatch -Marker 'Saccade is a human-facing browser' `
  -PatchFile (Join-Path $repoRoot 'engines\cef\patches\0014-native-chrome-ui-default.patch')
Invoke-SaccadePatch -Marker 'chrome_app_icon_id = 32512' `
  -PatchFile (Join-Path $repoRoot 'engines\cef\patches\0029-windows-chrome-app-icon.patch')
Invoke-SaccadePatch -Marker 'DefaultSaccadeProfilePath' `
  -PatchFile (Join-Path $repoRoot 'engines\cef\patches\0030-windows-default-profile.patch')
Invoke-SaccadePatch -Marker 'void SimpleApp::OnBeforeCommandLineProcessing(' `
  -PatchFile (Join-Path $repoRoot 'engines\cef\patches\0031-windows-saccade-new-tab.patch')

$copiedFiles = @(
  'saccade_adapter.h',
  'saccade_renderer.cc',
  'saccade_renderer.h',
  'saccade_agent_switch_win.h',
  'saccade_direct_session_win.cc',
  'saccade_direct_session_win.h',
  'saccade_windows_platform.cc',
  'saccade_windows_platform.h'
)
foreach ($name in $copiedFiles) {
  Copy-Item -LiteralPath (Join-Path $repoRoot "engines\cef\host\$name") `
    -Destination (Join-Path $simpleRoot $name) -Force
}

& (Join-Path $scriptRoot 'prepare_windows_adapter_final.ps1') `
  -Source (Join-Path $repoRoot 'engines\cef\host\saccade_adapter.cc') `
  -Destination (Join-Path $simpleRoot 'saccade_adapter.cc')
$formScriptDestination = Join-Path $simpleRoot 'saccade_form_script.h'
$previousFormScriptHash = if (Test-Path -LiteralPath $formScriptDestination) {
  (Get-FileHash -LiteralPath $formScriptDestination -Algorithm SHA256).Hash
} else {
  ''
}
& (Join-Path $scriptRoot 'prepare_windows_form_script.ps1') `
  -Source (Join-Path $repoRoot 'engines\cef\host\saccade_form_script.h') `
  -Destination $formScriptDestination
$currentFormScriptHash =
  (Get-FileHash -LiteralPath $formScriptDestination -Algorithm SHA256).Hash
if ($currentFormScriptHash -ne $previousFormScriptHash) {
  # The upstream MSBuild project does not track this generated header in its
  # renderer object dependencies, so explicitly invalidate the translation unit.
  (Get-Item -LiteralPath (Join-Path $simpleRoot 'saccade_renderer.cc')).LastWriteTime = Get-Date
}
& (Join-Path $scriptRoot 'prepare_windows_agent_switch.ps1') `
  -Source (Join-Path $repoRoot 'engines\cef\host\saccade_agent_switch_win.cc') `
  -Destination (Join-Path $simpleRoot 'saccade_agent_switch_win.cc')
& (Join-Path $scriptRoot 'port_windows_agent_overlay.ps1') `
  -Path (Join-Path $simpleRoot 'saccade_agent_switch_win.cc')
& (Join-Path $scriptRoot 'remove_windows_agent_button_heap.ps1') `
  -Path (Join-Path $simpleRoot 'saccade_agent_switch_win.cc')
& (Join-Path $scriptRoot 'refine_windows_agent_switch.ps1') `
  -Path (Join-Path $simpleRoot 'saccade_agent_switch_win.cc')
& (Join-Path $scriptRoot 'hide_windows_agent_overlay.ps1') `
  -Path (Join-Path $simpleRoot 'saccade_agent_switch_win.cc')
& (Join-Path $scriptRoot 'prepare_windows_handler.ps1') `
  -Header (Join-Path $simpleRoot 'simple_handler.h') `
  -Implementation (Join-Path $simpleRoot 'simple_handler.cc')
& (Join-Path $scriptRoot 'prepare_windows_entry.ps1') `
  -Path (Join-Path $simpleRoot 'cefsimple_win.cc')

& (Join-Path $scriptRoot 'prepare_windows_target_name.ps1') `
  -Path (Join-Path $simpleRoot 'CMakeLists.txt')
Invoke-SaccadePatch -Marker 'saccade_windows_platform.cc' `
  -PatchFile (Join-Path $repoRoot 'engines\cef\patches\0033-windows-build64-sources-cmake.patch')
Invoke-SaccadePatch -Marker 'target_compile_definitions(${CEF_TARGET} PRIVATE OS_WIN)' `
  -PatchFile (Join-Path $repoRoot 'engines\cef\patches\0035-windows-agent-libraries-cmake.patch')

$icon = Join-Path $repoRoot 'engines\cef\assets\Saccade.ico'
$largeIcon = Join-Path $simpleRoot 'win\cefsimple.ico'
$smallIcon = Join-Path $simpleRoot 'win\small.ico'
Copy-Item -LiteralPath $icon -Destination $largeIcon -Force
Copy-Item -LiteralPath $icon -Destination $smallIcon -Force
$resourceTimestamp = Get-Date
(Get-Item -LiteralPath $largeIcon).LastWriteTime = $resourceTimestamp
(Get-Item -LiteralPath $smallIcon).LastWriteTime = $resourceTimestamp

$CefRoot
