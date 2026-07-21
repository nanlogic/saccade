[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Path)

$ErrorActionPreference = 'Stop'
$resolved = (Resolve-Path -LiteralPath $Path).Path
$text = [IO.File]::ReadAllText($resolved).Replace("`r`n", "`n")

function Replace-Required([string]$Old, [string]$New) {
  $Old = $Old.Replace("`r`n", "`n")
  $New = $New.Replace("`r`n", "`n")
  if (-not $script:text.Contains($Old)) {
    throw "Missing Agent switch source fragment: $Old"
  }
  $script:text = $script:text.Replace($Old, $New)
}

$oldPosition = @'
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
'@

$newPosition = @'
constexpr LONG_PTR kAgentStateMask = 0x00ff;
constexpr LONG_PTR kAgentHoverFlag = 0x0100;
constexpr LONG_PTR kAgentPressedFlag = 0x0200;

int AgentSwitchState(HWND window) {
  return static_cast<int>(GetWindowLongPtrW(window, GWLP_USERDATA) &
                          kAgentStateMask) - 1;
}

bool AgentSwitchFlag(HWND window, LONG_PTR flag) {
  return (GetWindowLongPtrW(window, GWLP_USERDATA) & flag) != 0;
}

void SetAgentSwitchFlag(HWND window, LONG_PTR flag, bool enabled) {
  LONG_PTR value = GetWindowLongPtrW(window, GWLP_USERDATA);
  value = enabled ? value | flag : value & ~flag;
  SetWindowLongPtrW(window, GWLP_USERDATA, value);
}

void SetAgentSwitchState(HWND window, int state) {
  LONG_PTR value = GetWindowLongPtrW(window, GWLP_USERDATA);
  value = (value & ~kAgentStateMask) | ((state + 1) & kAgentStateMask);
  SetWindowLongPtrW(window, GWLP_USERDATA, value);
  const wchar_t* title = L"Agent unavailable for this tab";
  if (state == static_cast<int>(SaccadeAdapter::AgentUiState::kOn)) {
    title = L"Agent On for this tab";
  } else if (state ==
             static_cast<int>(SaccadeAdapter::AgentUiState::kOff)) {
    title = L"Agent Off for this tab";
  }
  SetWindowTextW(window, title);
}

int ScaleForWindow(HWND window, int dip) {
  HWND root = GetWindow(window, GW_OWNER);
  const UINT dpi = root ? GetDpiForWindow(root) : USER_DEFAULT_SCREEN_DPI;
  return MulDiv(dip, static_cast<int>(dpi), USER_DEFAULT_SCREEN_DPI);
}

void PositionAgentButton(HWND button) {
  HWND root = GetWindow(button, GW_OWNER);
  if (!root) return;
  if (!IsWindowVisible(root) || IsIconic(root)) {
    ShowWindow(button, SW_HIDE);
    return;
  }
  RECT bounds{};
  if (!GetWindowRect(root, &bounds)) return;

  // The switch lives in the omnibox, immediately before Chromium's bookmark
  // action. Values are DIPs and therefore track Windows display scaling.
  const int width = ScaleForWindow(button, 92);
  const int height = ScaleForWindow(button, 24);
  const int toolbar_top = ScaleForWindow(button, 51);
  const int right_reserve = ScaleForWindow(button, 190);
  const int inset = ScaleForWindow(button, 8);
  const int x = static_cast<int>(std::max<LONG>(
      bounds.left + inset, bounds.right - right_reserve - width));
  const int y = bounds.top + toolbar_top;

  RECT current{};
  GetWindowRect(button, &current);
  const bool resized = current.right - current.left != width ||
                       current.bottom - current.top != height;
  SetWindowPos(button, HWND_TOP, x, y, width, height,
               SWP_NOACTIVATE | SWP_SHOWWINDOW);
  if (resized) {
    const int radius = ScaleForWindow(button, 12);
    SetWindowRgn(button, CreateRoundRectRgn(0, 0, width + 1, height + 1,
                                            radius, radius), TRUE);
  }
}
'@
Replace-Required $oldPosition $newPosition

$oldDraw = @'
void DrawAgentButton(HWND window, int state) {
  PAINTSTRUCT paint{};
  HDC dc = BeginPaint(window, &paint);
  RECT bounds{};
  GetClientRect(window, &bounds);
  const bool on =
      state == static_cast<int>(SaccadeAdapter::AgentUiState::kOn);
  const bool available =
      state != static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable);
  HBRUSH background = CreateSolidBrush(on ? RGB(13, 148, 136)
                                           : RGB(64, 68, 75));
  HPEN border = CreatePen(PS_SOLID, 1, on ? RGB(45, 212, 191)
                                          : RGB(112, 118, 128));
  HGDIOBJ old_brush = SelectObject(dc, background);
  HGDIOBJ old_pen = SelectObject(dc, border);
  RoundRect(dc, bounds.left, bounds.top, bounds.right, bounds.bottom, 12, 12);
  SelectObject(dc, old_pen);
  SelectObject(dc, old_brush);
  DeleteObject(border);
  DeleteObject(background);
  SetBkMode(dc, TRANSPARENT);
  SetTextColor(dc, available ? RGB(245, 247, 250) : RGB(160, 164, 171));
  HFONT font = static_cast<HFONT>(GetStockObject(DEFAULT_GUI_FONT));
  HGDIOBJ old_font = SelectObject(dc, font);
  const wchar_t* title = on ? L"Agent On" : L"Agent Off";
  DrawTextW(dc, title, -1, &bounds,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
  SelectObject(dc, old_font);
  EndPaint(window, &paint);
}
'@

$newDraw = @'
void DrawAgentButton(HWND window, int state) {
  PAINTSTRUCT paint{};
  HDC dc = BeginPaint(window, &paint);
  RECT bounds{};
  GetClientRect(window, &bounds);
  const bool on =
      state == static_cast<int>(SaccadeAdapter::AgentUiState::kOn);
  const bool available =
      state != static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable);
  const bool hovered = AgentSwitchFlag(window, kAgentHoverFlag);
  const bool pressed = AgentSwitchFlag(window, kAgentPressedFlag);
  const bool focused = GetFocus() == window;

  HDC buffer = CreateCompatibleDC(dc);
  HBITMAP bitmap = CreateCompatibleBitmap(dc, bounds.right, bounds.bottom);
  HGDIOBJ old_bitmap = SelectObject(buffer, bitmap);

  const COLORREF surface = pressed ? RGB(73, 76, 81)
                           : hovered ? RGB(63, 66, 71)
                                     : RGB(53, 55, 59);
  const COLORREF border_color = focused ? RGB(138, 180, 248)
                                         : RGB(95, 99, 104);
  HBRUSH surface_brush = CreateSolidBrush(surface);
  HPEN border = CreatePen(PS_SOLID, focused ? ScaleForWindow(window, 2) : 1,
                          border_color);
  HGDIOBJ old_brush = SelectObject(buffer, surface_brush);
  HGDIOBJ old_pen = SelectObject(buffer, border);
  const int radius = ScaleForWindow(window, 12);
  RoundRect(buffer, bounds.left, bounds.top, bounds.right, bounds.bottom,
            radius, radius);

  const int pad = ScaleForWindow(window, 7);
  const int track_width = ScaleForWindow(window, 31);
  const int track_height = ScaleForWindow(window, 16);
  RECT track{bounds.right - pad - track_width,
             (bounds.bottom - track_height) / 2,
             bounds.right - pad,
             (bounds.bottom + track_height) / 2};
  HBRUSH track_brush = CreateSolidBrush(
      !available ? RGB(75, 78, 83)
                 : on ? RGB(26, 115, 232) : RGB(95, 99, 104));
  HPEN track_pen = CreatePen(PS_SOLID, 1,
      !available ? RGB(95, 99, 104)
                 : on ? RGB(138, 180, 248) : RGB(128, 134, 139));
  SelectObject(buffer, track_brush);
  SelectObject(buffer, track_pen);
  const int track_radius = ScaleForWindow(window, 8);
  RoundRect(buffer, track.left, track.top, track.right, track.bottom,
            track_radius, track_radius);

  const int knob_size = ScaleForWindow(window, 12);
  const int knob_gap = ScaleForWindow(window, 2);
  const int knob_left = on ? track.right - knob_gap - knob_size
                           : track.left + knob_gap;
  const int knob_top = (bounds.bottom - knob_size) / 2;
  HBRUSH knob_brush = CreateSolidBrush(
      available ? RGB(248, 249, 250) : RGB(154, 160, 166));
  HPEN knob_pen = CreatePen(PS_SOLID, 1,
      available ? RGB(218, 220, 224) : RGB(128, 134, 139));
  SelectObject(buffer, knob_brush);
  SelectObject(buffer, knob_pen);
  Ellipse(buffer, knob_left, knob_top,
          knob_left + knob_size, knob_top + knob_size);

  RECT label{pad, 0, track.left - ScaleForWindow(window, 4), bounds.bottom};
  SetBkMode(buffer, TRANSPARENT);
  SetTextColor(buffer, available ? RGB(241, 243, 244)
                                  : RGB(154, 160, 166));
  HFONT font = CreateFontW(-ScaleForWindow(window, 12), 0, 0, 0, FW_MEDIUM,
                           FALSE, FALSE, FALSE, DEFAULT_CHARSET,
                           OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
                           CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE,
                           L"Segoe UI");
  HGDIOBJ old_font = SelectObject(buffer, font);
  DrawTextW(buffer, L"Agent", -1, &label,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);

  BitBlt(dc, 0, 0, bounds.right, bounds.bottom, buffer, 0, 0, SRCCOPY);
  SelectObject(buffer, old_font);
  SelectObject(buffer, old_pen);
  SelectObject(buffer, old_brush);
  SelectObject(buffer, old_bitmap);
  DeleteObject(font);
  DeleteObject(knob_pen);
  DeleteObject(knob_brush);
  DeleteObject(track_pen);
  DeleteObject(track_brush);
  DeleteObject(border);
  DeleteObject(surface_brush);
  DeleteObject(bitmap);
  DeleteDC(buffer);
  EndPaint(window, &paint);
}
'@
Replace-Required $oldDraw $newDraw

$oldProc = @'
LRESULT CALLBACK AgentButtonProc(HWND window, UINT message, WPARAM wparam,
                                 LPARAM lparam) {
  int state = static_cast<int>(GetWindowLongPtrW(window, GWLP_USERDATA)) - 1;
  switch (message) {
    case WM_PAINT:
      DrawAgentButton(window, state);
      return 0;
    case WM_ERASEBKGND:
      return 1;
    case WM_TIMER:
      PositionAgentButton(window);
      return 0;
    case WM_LBUTTONUP:
    case WM_KEYUP:
      if (message == WM_KEYUP && wparam != VK_SPACE && wparam != VK_RETURN) {
        break;
      }
      if (state !=
          static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable)) {
        state = static_cast<int>(
            SaccadeAdapter::GetInstance()->ToggleAgentForVisibleTab());
        SetWindowLongPtrW(window, GWLP_USERDATA, state + 1);
        InvalidateRect(window, nullptr, TRUE);
      }
      return 0;
    case WM_NCDESTROY:
      KillTimer(window, kPositionTimer);
      SetWindowLongPtrW(window, GWLP_USERDATA, 0);
      break;
  }
  return DefWindowProcW(window, message, wparam, lparam);
}
'@

$newProc = @'
void ToggleAgentSwitch(HWND window) {
  int state = AgentSwitchState(window);
  if (state == static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable)) {
    return;
  }
  state = static_cast<int>(
      SaccadeAdapter::GetInstance()->ToggleAgentForVisibleTab());
  SetAgentSwitchState(window, state);
  InvalidateRect(window, nullptr, FALSE);
}

LRESULT CALLBACK AgentButtonProc(HWND window, UINT message, WPARAM wparam,
                                 LPARAM lparam) {
  const int state = AgentSwitchState(window);
  switch (message) {
    case WM_PAINT:
      DrawAgentButton(window, state);
      return 0;
    case WM_ERASEBKGND:
      return 1;
    case WM_TIMER:
      PositionAgentButton(window);
      return 0;
    case WM_MOUSEMOVE: {
      if (!AgentSwitchFlag(window, kAgentHoverFlag)) {
        SetAgentSwitchFlag(window, kAgentHoverFlag, true);
        TRACKMOUSEEVENT tracking{sizeof(tracking), TME_LEAVE, window, 0};
        TrackMouseEvent(&tracking);
        InvalidateRect(window, nullptr, FALSE);
      }
      return 0;
    }
    case WM_MOUSELEAVE:
      SetAgentSwitchFlag(window, kAgentHoverFlag, false);
      SetAgentSwitchFlag(window, kAgentPressedFlag, false);
      InvalidateRect(window, nullptr, FALSE);
      return 0;
    case WM_LBUTTONDOWN:
      if (state !=
          static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable)) {
        SetFocus(window);
        SetCapture(window);
        SetAgentSwitchFlag(window, kAgentPressedFlag, true);
        InvalidateRect(window, nullptr, FALSE);
      }
      return 0;
    case WM_LBUTTONUP: {
      const bool was_pressed = AgentSwitchFlag(window, kAgentPressedFlag);
      if (GetCapture() == window) ReleaseCapture();
      SetAgentSwitchFlag(window, kAgentPressedFlag, false);
      RECT bounds{};
      POINT point{static_cast<short>(LOWORD(lparam)),
                  static_cast<short>(HIWORD(lparam))};
      GetClientRect(window, &bounds);
      if (was_pressed && PtInRect(&bounds, point)) ToggleAgentSwitch(window);
      InvalidateRect(window, nullptr, FALSE);
      return 0;
    }
    case WM_KEYDOWN:
      if ((wparam == VK_SPACE || wparam == VK_RETURN) &&
          state != static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable)) {
        SetAgentSwitchFlag(window, kAgentPressedFlag, true);
        InvalidateRect(window, nullptr, FALSE);
        return 0;
      }
      break;
    case WM_KEYUP:
      if (wparam == VK_SPACE || wparam == VK_RETURN) {
        const bool was_pressed = AgentSwitchFlag(window, kAgentPressedFlag);
        SetAgentSwitchFlag(window, kAgentPressedFlag, false);
        if (was_pressed) ToggleAgentSwitch(window);
        InvalidateRect(window, nullptr, FALSE);
        return 0;
      }
      break;
    case WM_SETFOCUS:
    case WM_KILLFOCUS:
      InvalidateRect(window, nullptr, FALSE);
      return 0;
    case WM_CAPTURECHANGED:
      SetAgentSwitchFlag(window, kAgentPressedFlag, false);
      InvalidateRect(window, nullptr, FALSE);
      return 0;
    case WM_NCDESTROY:
      KillTimer(window, kPositionTimer);
      SetWindowLongPtrW(window, GWLP_USERDATA, 0);
      break;
  }
  return DefWindowProcW(window, message, wparam, lparam);
}
'@
Replace-Required $oldProc $newProc

Replace-Required `
  '        WS_POPUP | WS_VISIBLE | WS_TABSTOP, 0, 0, 104, 28, root, nullptr,' `
  '        WS_POPUP | WS_VISIBLE | WS_TABSTOP, 0, 0, 92, 24, root, nullptr,'

$text = $text.Replace(
  '  SetWindowLongPtrW(button, GWLP_USERDATA, state + 1);' + "`n" +
  '  SetWindowTextW(button, state =='+ "`n" +
  '      static_cast<int>(SaccadeAdapter::AgentUiState::kOn)' + "`n" +
  '          ? L"Agent On" : L"Agent Off");',
  '  SetAgentSwitchState(button, state);')

[IO.File]::WriteAllText($resolved, $text, [Text.UTF8Encoding]::new($false))
