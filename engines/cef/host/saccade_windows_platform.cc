// Copyright (c) 2026 Saccade contributors.

#include "tests/cefsimple/saccade_windows_platform.h"

#include <bcrypt.h>
#include <fcntl.h>
#include <io.h>
#include <sddl.h>
#include <sys/stat.h>

#include <atomic>
#include <cerrno>
#include <cstdarg>
#include <map>
#include <memory>
#include <mutex>
#include <vector>

namespace {

constexpr int kFirstPipeDescriptor = 100000;

struct PipeHandle {
  enum class Kind { kListener, kClient };
  Kind kind = Kind::kClient;
  std::string name;
  HANDLE handle = INVALID_HANDLE_VALUE;
  std::atomic<bool> stopping{false};
};

std::mutex g_pipe_mutex;
std::map<int, std::shared_ptr<PipeHandle>> g_pipe_handles;
std::atomic<int> g_next_pipe_descriptor{kFirstPipeDescriptor};

std::wstring Utf8ToWide(const std::string& value) {
  if (value.empty()) {
    return {};
  }
  const int size = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
                                       value.data(), static_cast<int>(value.size()),
                                       nullptr, 0);
  if (size <= 0) {
    return {};
  }
  std::wstring result(static_cast<size_t>(size), L'\0');
  if (MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, value.data(),
                          static_cast<int>(value.size()), result.data(), size) !=
      size) {
    return {};
  }
  return result;
}

bool CurrentUserSecurityDescriptor(PSECURITY_DESCRIPTOR* descriptor) {
  *descriptor = nullptr;
  HANDLE token = nullptr;
  if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token)) {
    return false;
  }
  DWORD bytes = 0;
  GetTokenInformation(token, TokenUser, nullptr, 0, &bytes);
  std::vector<unsigned char> storage(bytes);
  const bool loaded = bytes > 0 &&
      GetTokenInformation(token, TokenUser, storage.data(), bytes, &bytes);
  CloseHandle(token);
  if (!loaded) {
    return false;
  }
  auto* user = reinterpret_cast<TOKEN_USER*>(storage.data());
  LPWSTR sid = nullptr;
  if (!ConvertSidToStringSidW(user->User.Sid, &sid)) {
    return false;
  }
  const std::wstring sddl = L"D:P(A;;GA;;;" + std::wstring(sid) + L")";
  LocalFree(sid);
  return ConvertStringSecurityDescriptorToSecurityDescriptorW(
             sddl.c_str(), SDDL_REVISION_1, descriptor, nullptr) != FALSE;
}

SECURITY_ATTRIBUTES OwnerOnlySecurityAttributes(
    PSECURITY_DESCRIPTOR* descriptor) {
  SECURITY_ATTRIBUTES attributes{};
  attributes.nLength = sizeof(attributes);
  if (CurrentUserSecurityDescriptor(descriptor)) {
    attributes.lpSecurityDescriptor = *descriptor;
  }
  return attributes;
}

std::shared_ptr<PipeHandle> FindPipe(int descriptor) {
  std::lock_guard<std::mutex> lock(g_pipe_mutex);
  const auto item = g_pipe_handles.find(descriptor);
  return item == g_pipe_handles.end() ? nullptr : item->second;
}

int StorePipe(std::shared_ptr<PipeHandle> pipe) {
  const int descriptor = g_next_pipe_descriptor.fetch_add(1);
  std::lock_guard<std::mutex> lock(g_pipe_mutex);
  g_pipe_handles[descriptor] = std::move(pipe);
  return descriptor;
}

std::shared_ptr<PipeHandle> RemovePipe(int descriptor) {
  std::lock_guard<std::mutex> lock(g_pipe_mutex);
  const auto item = g_pipe_handles.find(descriptor);
  if (item == g_pipe_handles.end()) {
    return nullptr;
  }
  auto pipe = item->second;
  g_pipe_handles.erase(item);
  return pipe;
}

}  // namespace

bool SaccadeRandomBytes(void* output, size_t size) {
  return BCryptGenRandom(nullptr, static_cast<PUCHAR>(output),
                         static_cast<ULONG>(size),
                         BCRYPT_USE_SYSTEM_PREFERRED_RNG) == 0;
}

int SaccadeOpen(const char* path, int flags, ...) {
  int native_flags = _O_BINARY;
  if (flags & kSaccadeOpenWriteOnly) native_flags |= _O_WRONLY;
  else native_flags |= _O_RDONLY;
  if (flags & kSaccadeOpenCreate) native_flags |= _O_CREAT;
  if (flags & kSaccadeOpenTruncate) native_flags |= _O_TRUNC;
  if (flags & kSaccadeOpenAppend) native_flags |= _O_APPEND;
  const int descriptor = _open(path, native_flags, _S_IREAD | _S_IWRITE);
  if (descriptor >= 0 && (flags & kSaccadeOpenCreate)) {
    const std::wstring wide = Utf8ToWide(path);
    if (!wide.empty()) {
      SaccadeApplyOwnerOnlyDacl(wide);
    }
  }
  return descriptor;
}

SaccadeSSize SaccadeRead(int descriptor, void* output, size_t size) {
  if (auto pipe = FindPipe(descriptor)) {
    DWORD read = 0;
    if (pipe->kind != PipeHandle::Kind::kClient ||
        !ReadFile(pipe->handle, output, static_cast<DWORD>(size), &read,
                  nullptr)) {
      errno = EIO;
      return -1;
    }
    return static_cast<SaccadeSSize>(read);
  }
  return _read(descriptor, output, static_cast<unsigned int>(size));
}

SaccadeSSize SaccadeWrite(int descriptor, const void* input, size_t size) {
  if (auto pipe = FindPipe(descriptor)) {
    DWORD written = 0;
    if (pipe->kind != PipeHandle::Kind::kClient ||
        !WriteFile(pipe->handle, input, static_cast<DWORD>(size), &written,
                   nullptr)) {
      errno = EIO;
      return -1;
    }
    return static_cast<SaccadeSSize>(written);
  }
  return _write(descriptor, input, static_cast<unsigned int>(size));
}

int SaccadeClose(int descriptor) {
  if (auto pipe = RemovePipe(descriptor)) {
    if (pipe->handle != INVALID_HANDLE_VALUE) {
      if (pipe->kind == PipeHandle::Kind::kClient) {
        FlushFileBuffers(pipe->handle);
        DisconnectNamedPipe(pipe->handle);
      }
      CloseHandle(pipe->handle);
      pipe->handle = INVALID_HANDLE_VALUE;
    }
    return 0;
  }
  return _close(descriptor);
}

int SaccadeCommit(int descriptor) {
  return FindPipe(descriptor) ? 0 : _commit(descriptor);
}

int SaccadeChmodOwnerOnly(int descriptor, int mode) {
  (void)descriptor;
  (void)mode;
  return 0;
}

int SaccadeUnlink(const char* path) {
  const std::wstring wide = Utf8ToWide(path);
  if (wide.empty()) return -1;
  if (DeleteFileW(wide.c_str()) || GetLastError() == ERROR_FILE_NOT_FOUND) {
    return 0;
  }
  return -1;
}

int SaccadeRenameReplace(const char* source, const char* destination) {
  const std::wstring wide_source = Utf8ToWide(source);
  const std::wstring wide_destination = Utf8ToWide(destination);
  return !wide_source.empty() && !wide_destination.empty() &&
                 MoveFileExW(wide_source.c_str(), wide_destination.c_str(),
                             MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
             ? 0
             : -1;
}

int SaccadeProcessId() {
  return static_cast<int>(GetCurrentProcessId());
}

int SaccadeCreateNamedPipeListener(const std::string& pipe_name) {
  if (pipe_name.rfind("\\\\.\\pipe\\Saccade-", 0) != 0 ||
      pipe_name.size() > 240) {
    errno = EINVAL;
    return -1;
  }
  auto listener = std::make_shared<PipeHandle>();
  listener->kind = PipeHandle::Kind::kListener;
  listener->name = pipe_name;
  return StorePipe(std::move(listener));
}

int SaccadeAcceptNamedPipe(int listener_descriptor) {
  auto listener = FindPipe(listener_descriptor);
  if (!listener || listener->kind != PipeHandle::Kind::kListener ||
      listener->stopping.load()) {
    errno = EBADF;
    return -1;
  }
  const std::wstring pipe_name = Utf8ToWide(listener->name);
  PSECURITY_DESCRIPTOR descriptor = nullptr;
  if (pipe_name.empty() || !CurrentUserSecurityDescriptor(&descriptor)) {
    errno = EACCES;
    return -1;
  }
  SECURITY_ATTRIBUTES security{};
  security.nLength = sizeof(security);
  security.lpSecurityDescriptor = descriptor;
  HANDLE pipe = CreateNamedPipeW(
      pipe_name.c_str(), PIPE_ACCESS_DUPLEX,
      PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT |
          PIPE_REJECT_REMOTE_CLIENTS,
      8, 64 * 1024, 64 * 1024, 0,
      &security);
  LocalFree(descriptor);
  if (pipe == INVALID_HANDLE_VALUE) {
    errno = EIO;
    return -1;
  }
  const BOOL connected = ConnectNamedPipe(pipe, nullptr)
                             ? TRUE
                             : GetLastError() == ERROR_PIPE_CONNECTED;
  if (!connected || listener->stopping.load()) {
    CloseHandle(pipe);
    errno = EINTR;
    return -1;
  }
  auto client = std::make_shared<PipeHandle>();
  client->kind = PipeHandle::Kind::kClient;
  client->handle = pipe;
  return StorePipe(std::move(client));
}

void SaccadeShutdownNamedPipe(int listener_descriptor) {
  auto listener = FindPipe(listener_descriptor);
  if (!listener || listener->kind != PipeHandle::Kind::kListener) return;
  listener->stopping = true;
  const std::wstring name = Utf8ToWide(listener->name);
  HANDLE wake = CreateFileW(name.c_str(), GENERIC_READ | GENERIC_WRITE, 0,
                            nullptr, OPEN_EXISTING, 0, nullptr);
  if (wake != INVALID_HANDLE_VALUE) CloseHandle(wake);
}

bool SaccadeApplyOwnerOnlyDacl(const std::wstring& path) {
  PSECURITY_DESCRIPTOR descriptor = nullptr;
  if (!CurrentUserSecurityDescriptor(&descriptor)) return false;
  const BOOL ok = SetFileSecurityW(path.c_str(), DACL_SECURITY_INFORMATION,
                                   descriptor);
  LocalFree(descriptor);
  return ok != FALSE;
}

bool SaccadeEnsureOwnerOnlyDirectory(const std::wstring& path) {
  PSECURITY_DESCRIPTOR descriptor = nullptr;
  auto security = OwnerOnlySecurityAttributes(&descriptor);
  if (!descriptor) {
    return false;
  }
  const BOOL made = CreateDirectoryW(path.c_str(), &security);
  const DWORD error = GetLastError();
  LocalFree(descriptor);
  if (!made && error != ERROR_ALREADY_EXISTS) return false;
  return SaccadeApplyOwnerOnlyDacl(path);
}

bool SaccadeRemovePointerIfOwned(const std::string& pointer_path,
                                 const std::string& expected_contents) {
  const std::wstring wide = Utf8ToWide(pointer_path);
  if (wide.empty()) return false;
  HANDLE file = CreateFileW(wide.c_str(), GENERIC_READ,
                            FILE_SHARE_READ | FILE_SHARE_WRITE |
                                FILE_SHARE_DELETE,
                            nullptr, OPEN_EXISTING,
                            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
                            nullptr);
  if (file == INVALID_HANDLE_VALUE) return false;
  BY_HANDLE_FILE_INFORMATION info{};
  std::vector<char> bytes(expected_contents.size() + 1);
  DWORD read = 0;
  const bool matches = GetFileInformationByHandle(file, &info) &&
      (info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) == 0 &&
      ReadFile(file, bytes.data(), static_cast<DWORD>(expected_contents.size()),
               &read, nullptr) && read == expected_contents.size() &&
      std::string(bytes.data(), read) == expected_contents;
  CloseHandle(file);
  return matches && DeleteFileW(wide.c_str());
}
