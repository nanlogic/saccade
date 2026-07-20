// Copyright (c) 2026 Saccade contributors.

#include "tests/cefsimple/saccade_direct_session_win.h"

#include <windows.h>

#include <array>
#include <cstdlib>
#include <string>
#include <vector>

#include "tests/cefsimple/saccade_windows_platform.h"

namespace {

std::wstring Environment(const wchar_t* name) {
  const DWORD needed = GetEnvironmentVariableW(name, nullptr, 0);
  if (needed == 0) return {};
  std::wstring value(needed, L'\0');
  const DWORD written = GetEnvironmentVariableW(name, value.data(), needed);
  if (written == 0 || written >= needed) return {};
  value.resize(written);
  return value;
}

std::wstring RandomHex(size_t bytes) {
  std::array<unsigned char, 32> random{};
  if (bytes > random.size() || !SaccadeRandomBytes(random.data(), bytes)) {
    return {};
  }
  static constexpr wchar_t hex[] = L"0123456789abcdef";
  std::wstring result;
  result.reserve(bytes * 2);
  for (size_t i = 0; i < bytes; ++i) {
    result.push_back(hex[random[i] >> 4]);
    result.push_back(hex[random[i] & 0x0f]);
  }
  return result;
}

bool SetEnvironment(const wchar_t* name, const std::wstring& value) {
  // Keep the CRT environment used by getenv() and the Win32 environment
  // inherited by CEF subprocesses in sync.
  return _wputenv_s(name, value.c_str()) == 0 &&
         SetEnvironmentVariableW(name, value.c_str()) != FALSE;
}

void RegisterCodexMcpBestEffort() {
  std::vector<wchar_t> module_path(512, L'\0');
  DWORD length = 0;
  while (module_path.size() <= 32768) {
    length = GetModuleFileNameW(
        nullptr, module_path.data(), static_cast<DWORD>(module_path.size()));
    if (length == 0) return;
    if (length < module_path.size()) break;
    module_path.resize(module_path.size() * 2, L'\0');
  }
  if (length == 0 || length >= module_path.size()) return;
  const std::wstring executable(module_path.data(), length);
  const size_t separator = executable.find_last_of(L"\\/");
  if (separator == std::wstring::npos) return;
  const std::wstring directory = executable.substr(0, separator);
  const std::wstring helper = directory + L"\\saccade-mcp.exe";
  const DWORD attributes = GetFileAttributesW(helper.c_str());
  if (attributes == INVALID_FILE_ATTRIBUTES ||
      (attributes & FILE_ATTRIBUTE_DIRECTORY) != 0) {
    return;
  }

  std::wstring command_line = L"\"" + helper + L"\" register-codex";
  STARTUPINFOW startup{};
  startup.cb = sizeof(startup);
  startup.dwFlags = STARTF_USESHOWWINDOW;
  startup.wShowWindow = SW_HIDE;
  PROCESS_INFORMATION process{};
  if (!CreateProcessW(nullptr, command_line.data(), nullptr, nullptr, FALSE,
                      CREATE_NO_WINDOW, nullptr, directory.c_str(), &startup,
                      &process)) {
    return;
  }
  WaitForSingleObject(process.hProcess, 5000);
  CloseHandle(process.hThread);
  CloseHandle(process.hProcess);
}
}  // namespace

SaccadeDirectSessionWin::SaccadeDirectSessionWin() {
  if (!Environment(L"SACCADE_ENGINE_SOCKET").empty() &&
      !Environment(L"SACCADE_ENGINE_GRANT_PATH").empty()) {
    configured_ = true;
    return;
  }
  const std::wstring local_app_data = Environment(L"LOCALAPPDATA");
  const std::wstring nonce = RandomHex(16);
  if (local_app_data.empty() || nonce.empty()) return;

  const std::wstring saccade_root = local_app_data + L"\\Saccade";
  const std::wstring cef_root = saccade_root + L"\\CEF";
  const std::wstring agent_root = cef_root + L"\\Agent";
  session_path_ = agent_root + L"\\session-" +
      std::to_wstring(GetCurrentProcessId()) + L"-" + nonce;
  if (!SaccadeEnsureOwnerOnlyDirectory(saccade_root) ||
      !SaccadeEnsureOwnerOnlyDirectory(cef_root) ||
      !SaccadeEnsureOwnerOnlyDirectory(agent_root) ||
      !SaccadeEnsureOwnerOnlyDirectory(session_path_)) {
    return;
  }

  // A normal installed launch performs the same idempotent, non-destructive
  // per-user Codex MCP registration as Saccade.app on macOS. Registration
  // failure never blocks the human browser and is recorded by the helper.
  RegisterCodexMcpBestEffort();

  const std::wstring pipe_name = L"\\\\.\\pipe\\Saccade-" + nonce;
  grant_path_ = session_path_ + L"\\grant.json";
  replay_path_ = session_path_ + L"\\replay.jsonl";
  pointer_path_ = agent_root + L"\\current-grant-path";
  if (!SetEnvironment(L"SACCADE_ENGINE_SOCKET", pipe_name) ||
      !SetEnvironment(L"SACCADE_ENGINE_GRANT_PATH", grant_path_) ||
      !SetEnvironment(L"SACCADE_ENGINE_REPLAY_PATH", replay_path_) ||
      !SetEnvironment(L"SACCADE_ENGINE_CURRENT_POINTER", pointer_path_) ||
      !SetEnvironment(L"SACCADE_ENGINE_BROKER", L"1") ||
      !SetEnvironment(L"SACCADE_PROFILE_MODE", L"normal") ||
      !SetEnvironment(L"SACCADE_PROFILE_NAME", L"default")) {
    return;
  }
  configured_ = true;
  owns_session_ = true;
}

SaccadeDirectSessionWin::~SaccadeDirectSessionWin() {
  if (!owns_session_) return;
  DeleteFileW(grant_path_.c_str());
  DeleteFileW((grant_path_ + L".tmp").c_str());
  DeleteFileW(replay_path_.c_str());
  RemoveDirectoryW(session_path_.c_str());
}
