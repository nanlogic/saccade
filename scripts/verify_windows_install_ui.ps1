[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][int]$ProcessId,
  [string]$ScreenshotPath = '',
  [switch]$ToggleAgent
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class SaccadeWindowProbe {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lParam);
  [StructLayout(LayoutKind.Sequential)]
  public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc callback, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int maxCount);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, StringBuilder text, int maxCount);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
}
'@

function Get-WindowText([IntPtr]$Handle) {
  $value = New-Object System.Text.StringBuilder 512
  [void][SaccadeWindowProbe]::GetWindowText($Handle, $value, $value.Capacity)
  $value.ToString()
}

function Get-WindowClass([IntPtr]$Handle) {
  $value = New-Object System.Text.StringBuilder 256
  [void][SaccadeWindowProbe]::GetClassName($Handle, $value, $value.Capacity)
  $value.ToString()
}

$mainHandle = [IntPtr]::Zero
$callback = [SaccadeWindowProbe+EnumWindowsProc]{
  param([IntPtr]$handle, [IntPtr]$unused)
  [uint32]$owner = 0
  [void][SaccadeWindowProbe]::GetWindowThreadProcessId($handle, [ref]$owner)
  if ($owner -eq $ProcessId -and [SaccadeWindowProbe]::IsWindowVisible($handle) -and (Get-WindowClass $handle) -eq 'Chrome_WidgetWin_1') {
    $script:mainHandle = $handle
    return $false
  }
  return $true
}
[void][SaccadeWindowProbe]::EnumWindows($callback, [IntPtr]::Zero)
if ($mainHandle -eq [IntPtr]::Zero) { throw "No visible Saccade main window for PID $ProcessId" }

$agentHandle = [IntPtr]::Zero
$childCallback = [SaccadeWindowProbe+EnumWindowsProc]{
  param([IntPtr]$handle, [IntPtr]$unused)
  if ((Get-WindowClass $handle) -eq 'SaccadeAgentSwitchWindow') {
    $script:agentHandle = $handle
    return $false
  }
  return $true
}
[void][SaccadeWindowProbe]::EnumChildWindows($mainHandle, $childCallback, [IntPtr]::Zero)
if ($agentHandle -eq [IntPtr]::Zero) { throw 'Saccade Agent switch window was not found' }

$before = Get-WindowText $agentHandle
if ($ToggleAgent) {
  [void][SaccadeWindowProbe]::SendMessage($agentHandle, 0x0202, [IntPtr]::Zero, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 500
}
$after = Get-WindowText $agentHandle

if ($ScreenshotPath) {
  $screenshotDir = Split-Path -Parent $ScreenshotPath
  if ($screenshotDir) { New-Item -ItemType Directory -Force -Path $screenshotDir | Out-Null }
  [void][SaccadeWindowProbe]::SetForegroundWindow($mainHandle)
  Start-Sleep -Milliseconds 300
  $rect = New-Object SaccadeWindowProbe+RECT
  [void][SaccadeWindowProbe]::GetWindowRect($mainHandle, [ref]$rect)
  $width = $rect.Right - $rect.Left
  $height = $rect.Bottom - $rect.Top
  $bitmap = New-Object System.Drawing.Bitmap $width, $height
  $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
  try {
    $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
    $bitmap.Save($ScreenshotPath, [System.Drawing.Imaging.ImageFormat]::Png)
  } finally {
    $graphics.Dispose()
    $bitmap.Dispose()
  }
}

[pscustomobject]@{
  ProcessId = $ProcessId
  MainHandle = $mainHandle.ToInt64()
  MainTitle = Get-WindowText $mainHandle
  AgentHandle = $agentHandle.ToInt64()
  AgentBefore = $before
  AgentAfter = $after
  ScreenshotPath = $ScreenshotPath
}
