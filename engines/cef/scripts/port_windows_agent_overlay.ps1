[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Path)

$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath $Path).Path
$text = [System.IO.File]::ReadAllText($resolved).Replace("`r`n", "`n")

function Replace-Required {
  param([string]$Old, [string]$New)
  $Old = $Old.Replace("`r`n", "`n")
  $New = $New.Replace("`r`n", "`n")
  if (-not $script:text.Contains($Old)) {
    throw "Required Agent overlay source fragment was not found: $Old"
  }
  $script:text = $script:text.Replace($Old, $New)
}

if (-not $text.Contains('HINSTANCE SaccadeModuleInstance()')) {
  Replace-Required `
    "constexpr int kPromptCancel = 1003;`n" `
    @"
constexpr int kPromptCancel = 1003;

HINSTANCE SaccadeModuleInstance() {
  HMODULE module = nullptr;
  GetModuleHandleExW(
      GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
          GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
      reinterpret_cast<LPCWSTR>(&SaccadeModuleInstance), &module);
  return module;
}
"@
}

$oldPosition = @"
void PositionAgentButton(HWND button) {
  HWND root = GetParent(button);
  if (!root) return;
  RECT bounds{};
  if (!GetClientRect(root, &bounds)) return;
  constexpr int width = 104;
  constexpr int height = 28;
  const int right_reserve = 164;
  const int x = static_cast<int>(std::max<LONG>(8, bounds.right - right_reserve - width));
  SetWindowPos(button, HWND_TOP, x, 5, width, height,
               SWP_NOACTIVATE | SWP_SHOWWINDOW);
}
"@
$oldPositionUnnormalized = $oldPosition.Replace(
  'static_cast<int>(std::max<LONG>(8, bounds.right - right_reserve - width))',
  'std::max(8, bounds.right - right_reserve - width)')
$newPosition = @"
void PositionAgentButton(HWND button) {
  HWND root = GetWindow(button, GW_OWNER);
  if (!root) return;
  RECT bounds{};
  if (!GetWindowRect(root, &bounds)) return;
  constexpr int width = 104;
  constexpr int height = 28;
  const int right_reserve = 164;
  const int x = static_cast<int>(std::max<LONG>(
      bounds.left + 8, bounds.right - right_reserve - width));
  SetWindowPos(button, HWND_TOP, x, bounds.top + 5, width, height,
               SWP_NOACTIVATE | SWP_SHOWWINDOW);
}
"@
$oldPosition = $oldPosition.Replace("`r`n", "`n")
$oldPositionUnnormalized = $oldPositionUnnormalized.Replace("`r`n", "`n")
$newPosition = $newPosition.Replace("`r`n", "`n")
if ($text.Contains($oldPosition)) {
  $text = $text.Replace($oldPosition, $newPosition)
} elseif ($text.Contains($oldPositionUnnormalized)) {
  $text = $text.Replace($oldPositionUnnormalized, $newPosition)
} elseif (-not $text.Contains('GetWindow(button, GW_OWNER)')) {
  throw 'Agent button positioning function was not recognized'
}

$oldFind = @"
HWND FindAgentButton(HWND root) {
  return FindWindowExW(root, nullptr, kAgentButtonClass, nullptr);
}
"@
$newFind = @"
HWND FindAgentButton(HWND root) {
  HWND candidate = nullptr;
  while ((candidate = FindWindowExW(nullptr, candidate, kAgentButtonClass,
                                    nullptr)) != nullptr) {
    if (GetWindow(candidate, GW_OWNER) == root) return candidate;
  }
  return nullptr;
}
"@
$oldFind = $oldFind.Replace("`r`n", "`n")
$newFind = $newFind.Replace("`r`n", "`n")
if ($text.Contains($oldFind)) {
  $text = $text.Replace($oldFind, $newFind)
} elseif (-not $text.Contains('GetWindow(candidate, GW_OWNER)')) {
  throw 'Agent button lookup function was not recognized'
}

Replace-Required `
  "        0, kAgentButtonClass, L`"Agent Off`",`n        WS_CHILD | WS_VISIBLE | WS_TABSTOP, 0, 0, 104, 28, root, nullptr," `
  "        WS_EX_TOOLWINDOW, kAgentButtonClass, L`"Agent Off`",`n        WS_POPUP | WS_VISIBLE | WS_TABSTOP, 0, 0, 104, 28, root, nullptr,"

$text = $text.Replace('GetModuleHandleW(nullptr)', 'SaccadeModuleInstance()')
[System.IO.File]::WriteAllText($resolved, $text,
  [System.Text.UTF8Encoding]::new($false))
