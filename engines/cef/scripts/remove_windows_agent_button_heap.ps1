[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Path)

$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath $Path).Path
$text = [IO.File]::ReadAllText($resolved).Replace("`r`n", "`n")
$text = $text.Replace("      delete state;`n      SetWindowLongPtrW(window, GWLP_USERDATA, 0);", "      SetWindowLongPtrW(window, GWLP_USERDATA, 0);`n      delete state;")

function Replace-Required([string]$Old, [string]$New) {
  $Old = $Old.Replace("`r`n", "`n")
  $New = $New.Replace("`r`n", "`n")
  if (-not $script:text.Contains($Old)) { throw "Missing source fragment: $Old" }
  $script:text = $script:text.Replace($Old, $New)
}

Replace-Required @"
struct AgentButtonState {
  CefRefPtr<CefBrowser> browser;
  int state = static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable);
};

"@ ''
Replace-Required 'void DrawAgentButton(HWND window, AgentButtonState* state) {' `
  'void DrawAgentButton(HWND window, int state) {'
Replace-Required @"
  const bool on = state &&
      state->state == static_cast<int>(SaccadeAdapter::AgentUiState::kOn);
  const bool available = state && state->state !=
      static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable);
"@ @"
  const bool on =
      state == static_cast<int>(SaccadeAdapter::AgentUiState::kOn);
  const bool available =
      state != static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable);
"@
Replace-Required @"
  auto* state = reinterpret_cast<AgentButtonState*>(
      GetWindowLongPtrW(window, GWLP_USERDATA));
"@ @"
  int state = static_cast<int>(GetWindowLongPtrW(window, GWLP_USERDATA)) - 1;
"@
Replace-Required @"
      if (state && state->state !=
          static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable)) {
        state->state = static_cast<int>(
            SaccadeAdapter::GetInstance()->ToggleAgentForVisibleTab());
        InvalidateRect(window, nullptr, TRUE);
      }
"@ @"
      if (state !=
          static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable)) {
        state = static_cast<int>(
            SaccadeAdapter::GetInstance()->ToggleAgentForVisibleTab());
        SetWindowLongPtrW(window, GWLP_USERDATA, state + 1);
        InvalidateRect(window, nullptr, TRUE);
      }
"@
Replace-Required @"
      SetWindowLongPtrW(window, GWLP_USERDATA, 0);
      delete state;
"@ @"
      SetWindowLongPtrW(window, GWLP_USERDATA, 0);
"@
Replace-Required @"
    auto* button_state = new AgentButtonState();
    button_state->browser = browser;
    button_state->state = state;
"@ ''
Replace-Required '        SaccadeModuleInstance(), button_state);' `
  '        SaccadeModuleInstance(), nullptr);'
Replace-Required @"
    if (!button) {
      delete button_state;
      return;
    }
    SetWindowLongPtrW(button, GWLP_USERDATA,
                      reinterpret_cast<LONG_PTR>(button_state));
"@ @"
    if (!button) return;
    SetWindowLongPtrW(button, GWLP_USERDATA, state + 1);
"@
Replace-Required @"
  auto* button_state = reinterpret_cast<AgentButtonState*>(
      GetWindowLongPtrW(button, GWLP_USERDATA));
  if (button_state) {
    button_state->browser = browser;
    button_state->state = state;
  }
"@ @"
  SetWindowLongPtrW(button, GWLP_USERDATA, state + 1);
"@

[IO.File]::WriteAllText($resolved, $text, [Text.UTF8Encoding]::new($false))
