// Copyright (c) 2026 Saccade contributors.

#include "tests/cefsimple/saccade_agent_switch_win.h"

#include <windows.h>

#include <algorithm>
#include <condition_variable>
#include <memory>
#include <mutex>
#include <string>

#include "include/base/cef_callback.h"
#include "include/wrapper/cef_closure_task.h"
#include "tests/cefsimple/saccade_adapter.h"

namespace {

constexpr wchar_t kAgentButtonClass[] = L"SaccadeAgentSwitchWindow";
constexpr wchar_t kPromptClass[] = L"SaccadeProtectedValuePrompt";
constexpr UINT_PTR kPositionTimer = 1;
constexpr int kPromptEdit = 1001;
constexpr int kPromptFill = 1002;
constexpr int kPromptCancel = 1003;

struct AgentButtonState {
  CefRefPtr<CefBrowser> browser;
  int state = static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable);
};

struct PromptDialogState {
  std::wstring origin;
  std::wstring label;
  std::wstring value;
  bool confirmed = false;
  HWND edit = nullptr;
};

struct AsyncPromptState {
  std::mutex mutex;
  std::condition_variable ready;
  bool done = false;
  SaccadeProtectedValuePromptResult result;
};

std::wstring Utf8ToWide(const std::string& value) {
  if (value.empty()) return {};
  const int size = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
                                       value.data(), static_cast<int>(value.size()),
                                       nullptr, 0);
  if (size <= 0) return {};
  std::wstring result(static_cast<size_t>(size), L'\0');
  if (MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, value.data(),
                          static_cast<int>(value.size()), result.data(), size) !=
      size) {
    return {};
  }
  return result;
}

std::string WideToUtf8(const std::wstring& value) {
  if (value.empty()) return {};
  const int size = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS,
                                      value.data(), static_cast<int>(value.size()),
                                      nullptr, 0, nullptr, nullptr);
  if (size <= 0) return {};
  std::string result(static_cast<size_t>(size), '\0');
  if (WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, value.data(),
                          static_cast<int>(value.size()), result.data(), size,
                          nullptr, nullptr) != size) {
    return {};
  }
  return result;
}

HWND RootForBrowser(CefRefPtr<CefBrowser> browser) {
  if (!browser) return nullptr;
  HWND child = browser->GetHost()->GetWindowHandle();
  return child ? GetAncestor(child, GA_ROOT) : nullptr;
}

void PositionAgentButton(HWND button) {
  HWND root = GetParent(button);
  if (!root) return;
  RECT bounds{};
  if (!GetClientRect(root, &bounds)) return;
  constexpr int width = 104;
  constexpr int height = 28;
  const int right_reserve = 164;
  const int x = std::max(8, bounds.right - right_reserve - width);
  SetWindowPos(button, HWND_TOP, x, 5, width, height,
               SWP_NOACTIVATE | SWP_SHOWWINDOW);
}

void DrawAgentButton(HWND window, AgentButtonState* state) {
  PAINTSTRUCT paint{};
  HDC dc = BeginPaint(window, &paint);
  RECT bounds{};
  GetClientRect(window, &bounds);
  const bool on = state &&
      state->state == static_cast<int>(SaccadeAdapter::AgentUiState::kOn);
  const bool available = state && state->state !=
      static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable);
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

LRESULT CALLBACK AgentButtonProc(HWND window, UINT message, WPARAM wparam,
                                 LPARAM lparam) {
  auto* state = reinterpret_cast<AgentButtonState*>(
      GetWindowLongPtrW(window, GWLP_USERDATA));
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
      if (state && state->state !=
          static_cast<int>(SaccadeAdapter::AgentUiState::kUnavailable)) {
        state->state = static_cast<int>(
            SaccadeAdapter::GetInstance()->ToggleAgentForVisibleTab());
        InvalidateRect(window, nullptr, TRUE);
      }
      return 0;
    case WM_NCDESTROY:
      KillTimer(window, kPositionTimer);
      delete state;
      SetWindowLongPtrW(window, GWLP_USERDATA, 0);
      break;
  }
  return DefWindowProcW(window, message, wparam, lparam);
}

void RegisterAgentButtonClass() {
  static const bool registered = [] {
    WNDCLASSEXW klass{};
    klass.cbSize = sizeof(klass);
    klass.hInstance = GetModuleHandleW(nullptr);
    klass.lpfnWndProc = AgentButtonProc;
    klass.hCursor = LoadCursorW(nullptr, IDC_HAND);
    klass.lpszClassName = kAgentButtonClass;
    return RegisterClassExW(&klass) != 0 ||
           GetLastError() == ERROR_CLASS_ALREADY_EXISTS;
  }();
  (void)registered;
}

HWND FindAgentButton(HWND root) {
  return FindWindowExW(root, nullptr, kAgentButtonClass, nullptr);
}

void AddLabel(HWND parent, const wchar_t* text, int x, int y, int width,
              int height) {
  HWND label = CreateWindowExW(0, L"STATIC", text, WS_CHILD | WS_VISIBLE,
                               x, y, width, height, parent, nullptr,
                               GetModuleHandleW(nullptr), nullptr);
  SendMessageW(label, WM_SETFONT,
               reinterpret_cast<WPARAM>(GetStockObject(DEFAULT_GUI_FONT)), TRUE);
}

LRESULT CALLBACK PromptProc(HWND window, UINT message, WPARAM wparam,
                            LPARAM lparam) {
  auto* state = reinterpret_cast<PromptDialogState*>(
      GetWindowLongPtrW(window, GWLP_USERDATA));
  if (message == WM_CREATE) {
    const auto* create = reinterpret_cast<CREATESTRUCTW*>(lparam);
    state = static_cast<PromptDialogState*>(create->lpCreateParams);
    SetWindowLongPtrW(window, GWLP_USERDATA,
                      reinterpret_cast<LONG_PTR>(state));
    AddLabel(window, L"Fill protected field locally", 20, 16, 420, 24);
    const std::wstring details = L"Origin: " + state->origin +
        L"\r\nPage field (untrusted label): " + state->label;
    AddLabel(window, details.c_str(), 20, 48, 430, 42);
    AddLabel(window,
             L"The value goes directly to this page. It is not sent to the "
             L"LLM, logs, screenshots, or replay.",
             20, 98, 430, 42);
    state->edit = CreateWindowExW(WS_EX_CLIENTEDGE, L"EDIT", L"",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL | ES_PASSWORD,
        20, 146, 430, 27, window,
        reinterpret_cast<HMENU>(kPromptEdit), GetModuleHandleW(nullptr), nullptr);
    SendMessageW(state->edit, WM_SETFONT,
                 reinterpret_cast<WPARAM>(GetStockObject(DEFAULT_GUI_FONT)), TRUE);
    HWND fill = CreateWindowExW(0, L"BUTTON", L"Fill locally",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON,
        250, 190, 96, 30, window, reinterpret_cast<HMENU>(kPromptFill),
        GetModuleHandleW(nullptr), nullptr);
    HWND cancel = CreateWindowExW(0, L"BUTTON", L"Cancel",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        354, 190, 96, 30, window, reinterpret_cast<HMENU>(kPromptCancel),
        GetModuleHandleW(nullptr), nullptr);
    SendMessageW(fill, WM_SETFONT,
                 reinterpret_cast<WPARAM>(GetStockObject(DEFAULT_GUI_FONT)), TRUE);
    SendMessageW(cancel, WM_SETFONT,
                 reinterpret_cast<WPARAM>(GetStockObject(DEFAULT_GUI_FONT)), TRUE);
    SetFocus(state->edit);
    return 0;
  }
  if (message == WM_COMMAND && state) {
    const int command = LOWORD(wparam);
    if (command == kPromptFill) {
      const int length = GetWindowTextLengthW(state->edit);
      if (length > 0 && length <= 4096) {
        state->value.resize(static_cast<size_t>(length) + 1);
        const int copied = GetWindowTextW(state->edit, state->value.data(),
                                          length + 1);
        state->value.resize(copied > 0 ? static_cast<size_t>(copied) : 0);
        state->confirmed = !state->value.empty();
      }
      SetWindowTextW(state->edit, L"");
      DestroyWindow(window);
      return 0;
    }
    if (command == kPromptCancel) {
      SetWindowTextW(state->edit, L"");
      DestroyWindow(window);
      return 0;
    }
  }
  if (message == WM_CLOSE && state) {
    SetWindowTextW(state->edit, L"");
    DestroyWindow(window);
    return 0;
  }
  return DefWindowProcW(window, message, wparam, lparam);
}

void RegisterPromptClass() {
  static const bool registered = [] {
    WNDCLASSEXW klass{};
    klass.cbSize = sizeof(klass);
    klass.hInstance = GetModuleHandleW(nullptr);
    klass.lpfnWndProc = PromptProc;
    klass.hCursor = LoadCursorW(nullptr, IDC_ARROW);
    klass.hbrBackground = reinterpret_cast<HBRUSH>(COLOR_WINDOW + 1);
    klass.lpszClassName = kPromptClass;
    return RegisterClassExW(&klass) != 0 ||
           GetLastError() == ERROR_CLASS_ALREADY_EXISTS;
  }();
  (void)registered;
}

SaccadeProtectedValuePromptResult ShowProtectedPromptOnUi(
    CefRefPtr<CefBrowser> browser, const std::string& page_origin,
    const std::string& field_label) {
  RegisterPromptClass();
  PromptDialogState state;
  state.origin = Utf8ToWide(page_origin);
  state.label = Utf8ToWide(field_label);
  if (state.origin.empty()) state.origin = L"unknown origin";
  if (state.label.empty()) state.label = L"protected identifier";
  HWND owner = RootForBrowser(browser);
  HWND prompt = CreateWindowExW(
      WS_EX_DLGMODALFRAME, kPromptClass, L"Saccade protected value",
      WS_POPUP | WS_CAPTION | WS_SYSMENU, CW_USEDEFAULT, CW_USEDEFAULT,
      486, 270, owner, nullptr, GetModuleHandleW(nullptr), &state);
  if (!prompt) return {};
  RECT owner_rect{};
  RECT prompt_rect{};
  if (owner && GetWindowRect(owner, &owner_rect) &&
      GetWindowRect(prompt, &prompt_rect)) {
    SetWindowPos(prompt, HWND_TOP,
                 owner_rect.left + (owner_rect.right - owner_rect.left -
                                    (prompt_rect.right - prompt_rect.left)) / 2,
                 owner_rect.top + (owner_rect.bottom - owner_rect.top -
                                   (prompt_rect.bottom - prompt_rect.top)) / 2,
                 0, 0, SWP_NOSIZE);
  }
  if (owner) EnableWindow(owner, FALSE);
  ShowWindow(prompt, SW_SHOW);
  UpdateWindow(prompt);
  MSG message{};
  while (IsWindow(prompt) && GetMessageW(&message, nullptr, 0, 0) > 0) {
    if (!IsDialogMessageW(prompt, &message)) {
      TranslateMessage(&message);
      DispatchMessageW(&message);
    }
  }
  if (owner) {
    EnableWindow(owner, TRUE);
    SetForegroundWindow(owner);
  }
  SaccadeProtectedValuePromptResult result;
  result.confirmed = state.confirmed;
  if (state.confirmed) result.value = WideToUtf8(state.value);
  if (!state.value.empty()) {
    SecureZeroMemory(state.value.data(), state.value.size() * sizeof(wchar_t));
    state.value.clear();
  }
  return result;
}

}  // namespace

void SaccadeUpdateAgentSwitch(CefRefPtr<CefBrowser> browser, int state) {
  HWND root = RootForBrowser(browser);
  if (!root) return;
  RegisterAgentButtonClass();
  HWND button = FindAgentButton(root);
  if (!button) {
    auto* button_state = new AgentButtonState();
    button_state->browser = browser;
    button_state->state = state;
    button = CreateWindowExW(
        0, kAgentButtonClass, L"Agent Off",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP, 0, 0, 104, 28, root, nullptr,
        GetModuleHandleW(nullptr), button_state);
    if (!button) {
      delete button_state;
      return;
    }
    SetWindowLongPtrW(button, GWLP_USERDATA,
                      reinterpret_cast<LONG_PTR>(button_state));
    SetTimer(button, kPositionTimer, 250, nullptr);
  }
  auto* button_state = reinterpret_cast<AgentButtonState*>(
      GetWindowLongPtrW(button, GWLP_USERDATA));
  if (button_state) {
    button_state->browser = browser;
    button_state->state = state;
  }
  SetWindowTextW(button, state ==
      static_cast<int>(SaccadeAdapter::AgentUiState::kOn)
          ? L"Agent On" : L"Agent Off");
  PositionAgentButton(button);
  InvalidateRect(button, nullptr, TRUE);
}

void SaccadeShowHumanVerificationFailure(CefRefPtr<CefBrowser> browser,
                                         const std::string& provider) {
  HWND owner = RootForBrowser(browser);
  std::wstring name = Utf8ToWide(provider);
  if (name.empty()) name = L"The site's verification provider";
  const std::wstring message = name +
      L" rejected or could not create the verification session.\r\n\r\n"
      L"Saccade did not read or store challenge content, cookies, or "
      L"verification tokens. Reload the page to try again.";
  if (MessageBoxW(owner, message.c_str(),
                  L"Human verification could not start",
                  MB_OKCANCEL | MB_ICONWARNING | MB_SETFOREGROUND) == IDOK &&
      browser && browser->IsValid()) {
    SaccadeAdapter::GetInstance()->RetryHumanVerification(browser);
  }
}

SaccadeProtectedValuePromptResult SaccadePromptProtectedValue(
    CefRefPtr<CefBrowser> browser, const std::string& page_origin,
    const std::string& field_label) {
  HWND root = RootForBrowser(browser);
  DWORD ui_thread = root ? GetWindowThreadProcessId(root, nullptr) : 0;
  if (ui_thread != 0 && ui_thread == GetCurrentThreadId()) {
    return ShowProtectedPromptOnUi(browser, page_origin, field_label);
  }
  auto state = std::make_shared<AsyncPromptState>();
  CefPostTask(TID_UI, base::BindOnce(
      [](std::shared_ptr<AsyncPromptState> state,
         CefRefPtr<CefBrowser> browser, std::string origin,
         std::string label) {
        auto result = ShowProtectedPromptOnUi(browser, origin, label);
        {
          std::lock_guard<std::mutex> lock(state->mutex);
          state->result = std::move(result);
          state->done = true;
        }
        state->ready.notify_one();
      }, state, browser, page_origin, field_label));
  std::unique_lock<std::mutex> lock(state->mutex);
  state->ready.wait(lock, [&state] { return state->done; });
  return std::move(state->result);
}
