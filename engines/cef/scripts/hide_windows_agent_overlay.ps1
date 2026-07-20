[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Path)

$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath $Path).Path
$text = [IO.File]::ReadAllText($resolved).Replace("`r`n", "`n")
$start = $text.IndexOf('void SaccadeUpdateAgentSwitch(CefRefPtr<CefBrowser> browser, int state) {')
$next = $text.IndexOf('void SaccadeShowHumanVerificationFailure(', $start)
if ($start -lt 0 -or $next -lt 0) {
  throw 'Could not locate the legacy Agent overlay implementation'
}
$replacement = @'
void SaccadeUpdateAgentSwitch(CefRefPtr<CefBrowser> browser, int state) {
  // Windows uses a real Chromium extension action in the toolbar. Retire any
  // legacy owned-window overlay left by a previous in-place build.
  HWND root = RootForBrowser(browser);
  if (root) {
    HWND old_button = FindAgentButton(root);
    if (old_button) DestroyWindow(old_button);
  }
  (void)state;
}

'@
$text = $text.Substring(0, $start) + $replacement + $text.Substring($next)
[IO.File]::WriteAllText($resolved, $text.Replace("`n", "`r`n"),
  [Text.UTF8Encoding]::new($false))
