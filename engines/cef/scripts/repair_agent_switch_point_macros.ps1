[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$cefRoot = 'C:\Users\wayne\AppData\Local\Saccade\cef\150.0.11\cef_binary_150.0.11+gb887805+chromium-150.0.7871.115_windows64'
$paths = @(
  (Join-Path $scriptRoot 'refine_windows_agent_switch.ps1'),
  (Join-Path $cefRoot 'tests\cefsimple\saccade_agent_switch_win.cc')
)
foreach ($path in $paths) {
  $text = [IO.File]::ReadAllText($path)
  $old = 'POINT point{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};'
  $new = @'
POINT point{static_cast<short>(LOWORD(lparam)),
                  static_cast<short>(HIWORD(lparam))};
'@.TrimEnd()
  if ($text.Contains($old)) {
    $text = $text.Replace($old, $new)
    [IO.File]::WriteAllText($path, $text, [Text.UTF8Encoding]::new($false))
  } elseif (-not $text.Contains('static_cast<short>(LOWORD(lparam))')) {
    throw "Pointer coordinate source was not recognized: $path"
  }
}
'Agent switch pointer coordinates no longer require windowsx.h.'
