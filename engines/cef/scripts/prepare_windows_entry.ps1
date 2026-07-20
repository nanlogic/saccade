[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Path)

$ErrorActionPreference = 'Stop'
$text = [System.IO.File]::ReadAllText($Path).Replace("`r`n", "`n")
if (-not $text.Contains('SaccadeDirectSessionWin direct_session')) {
  $text = $text.Replace(
    '#include "tests/cefsimple/simple_app.h"',
    '#include "tests/cefsimple/simple_app.h"' + "`n" +
    '#include "tests/cefsimple/saccade_direct_session_win.h"' + "`n" +
    '#include "tests/cefsimple/saccade_renderer.h"')
  $text = $text.Replace(
    'exit_code = CefExecuteProcess(main_args, nullptr, sandbox_info);',
    'CefRefPtr<SaccadeRendererApp> renderer_app(new SaccadeRendererApp);' + "`n" +
    '  exit_code = CefExecuteProcess(main_args, renderer_app, sandbox_info);')
  $text = $text.Replace(
    '  // Specify CEF global settings here.' + "`n" + '  CefSettings settings;',
    '  SaccadeDirectSessionWin direct_session;' + "`n`n" +
    '  // Specify CEF global settings here.' + "`n" + '  CefSettings settings;')
}
[System.IO.File]::WriteAllText($Path, $text,
  [System.Text.UTF8Encoding]::new($false))
