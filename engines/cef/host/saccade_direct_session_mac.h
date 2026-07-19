// Copyright (c) 2026 Saccade contributors.
// Use of this source code is governed by a BSD-style license.

#ifndef SACCADE_CEF_HOST_SACCADE_DIRECT_SESSION_MAC_H_
#define SACCADE_CEF_HOST_SACCADE_DIRECT_SESSION_MAC_H_

#include <errno.h>
#include <fcntl.h>
#include <mach-o/dyld.h>
#include <pwd.h>
#include <spawn.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#include <array>
#include <string>
#include <vector>

extern char** environ;

// Owns the ephemeral bridge paths created only by a direct Saccade.app launch.
// Wrapper/test launches provide both endpoint paths and retain their existing
// lifecycle. This class never reads, copies, or removes browser profile data.
class SaccadeDirectSession {
 public:
  enum class Result {
    kDirectReady,
    kExternallyConfigured,
    kFailedClosed,
  };

  SaccadeDirectSession() = default;
  SaccadeDirectSession(const SaccadeDirectSession&) = delete;
  SaccadeDirectSession& operator=(const SaccadeDirectSession&) = delete;

  ~SaccadeDirectSession() { Cleanup(); }

  Result Prepare() {
    const char* socket = getenv("SACCADE_ENGINE_SOCKET");
    const char* grant = getenv("SACCADE_ENGINE_GRANT_PATH");
    if (socket || grant) {
      return socket && grant ? Result::kExternallyConfigured
                             : Result::kFailedClosed;
    }

    const std::string home = TrustedHome();
    if (home.empty()) {
      return Result::kFailedClosed;
    }
    const std::string library = home + "/Library";
    const std::string application_support = library + "/Application Support";
    const std::string saccade = application_support + "/Saccade";
    const std::string cef = saccade + "/CEF";
    agent_root_ = cef + "/Agent";
    profile_path_ = cef + "/Profiles/default";

    if (!EnsureOwnedDirectory(library, false) ||
        !EnsureOwnedDirectory(application_support, false) ||
        !EnsureOwnedDirectory(saccade, true) ||
        !EnsureOwnedDirectory(cef, true) ||
        !EnsureOwnedDirectory(agent_root_, true) ||
        !EnsureOwnedDirectory(cef + "/Profiles", true) ||
        !EnsureOwnedDirectory(profile_path_, true)) {
      return Result::kFailedClosed;
    }

    // MCP clients do not discover arbitrary applications on their own. A
    // signed Saccade install therefore performs an idempotent, non-destructive
    // Codex registration for each macOS user on that user's first launch.
    // Failure never blocks the human browser; the helper records a value-free
    // diagnostic state for the explicit repair flow.
    RegisterCodexMcpBestEffort();

    std::vector<char> session_template(agent_root_.begin(), agent_root_.end());
    const std::string suffix = "/session.XXXXXX";
    session_template.insert(session_template.end(), suffix.begin(), suffix.end());
    session_template.push_back('\0');
    char* created = mkdtemp(session_template.data());
    if (!created || chmod(created, 0700) != 0) {
      return Result::kFailedClosed;
    }
    session_path_ = created;
    socket_path_ = session_path_ + "/control.sock";
    grant_path_ = session_path_ + "/grant.json";
    replay_path_ = session_path_ + "/replay.jsonl";
    pointer_path_ = agent_root_ + "/current-grant-path";
    active_ = true;

    if (setenv("SACCADE_ENGINE_SOCKET", socket_path_.c_str(), 1) != 0 ||
        setenv("SACCADE_ENGINE_GRANT_PATH", grant_path_.c_str(), 1) != 0 ||
        setenv("SACCADE_ENGINE_REPLAY_PATH", replay_path_.c_str(), 1) != 0 ||
        setenv("SACCADE_ENGINE_CURRENT_POINTER", pointer_path_.c_str(), 1) != 0 ||
        setenv("SACCADE_ENGINE_BROKER", "1", 1) != 0 ||
        setenv("SACCADE_PROFILE_MODE", "normal", 0) != 0 ||
        setenv("SACCADE_PROFILE_NAME", "default", 0) != 0) {
      Cleanup();
      return Result::kFailedClosed;
    }
    return Result::kDirectReady;
  }

  const std::string& profile_path() const { return profile_path_; }
  bool active() const { return active_; }

  void Cleanup() {
    if (!active_) {
      return;
    }
    RemovePointerIfOwned();
    unlink(socket_path_.c_str());
    unlink(grant_path_.c_str());
    unlink((grant_path_ + ".tmp").c_str());
    unlink(replay_path_.c_str());
    unlink((replay_path_ + ".audit.png").c_str());
    rmdir(session_path_.c_str());
    active_ = false;
  }

 private:
  static void RegisterCodexMcpBestEffort() {
    uint32_t size = 4096;
    std::vector<char> executable(size, '\0');
    if (_NSGetExecutablePath(executable.data(), &size) != 0) {
      if (size == 0 || size > 64 * 1024) {
        return;
      }
      executable.assign(size + 1, '\0');
      if (_NSGetExecutablePath(executable.data(), &size) != 0) {
        return;
      }
    }
    std::string executable_path(executable.data());
    const size_t separator = executable_path.rfind('/');
    if (separator == std::string::npos) {
      return;
    }
    const std::string helper = executable_path.substr(0, separator + 1) +
                               "saccade-connect-codex";
    struct stat status {};
    if (lstat(helper.c_str(), &status) != 0 || !S_ISREG(status.st_mode) ||
        (status.st_mode & S_IXUSR) == 0) {
      return;
    }

    posix_spawn_file_actions_t actions;
    if (posix_spawn_file_actions_init(&actions) != 0) {
      return;
    }
    posix_spawn_file_actions_addopen(&actions, STDOUT_FILENO, "/dev/null",
                                     O_WRONLY, 0);
    posix_spawn_file_actions_addopen(&actions, STDERR_FILENO, "/dev/null",
                                     O_WRONLY, 0);
    pid_t child = -1;
    char* const argv[] = {const_cast<char*>(helper.c_str()), nullptr};
    const int spawned =
        posix_spawn(&child, helper.c_str(), &actions, nullptr, argv, environ);
    posix_spawn_file_actions_destroy(&actions);
    if (spawned == 0 && child > 0) {
      int child_status = 0;
      while (waitpid(child, &child_status, 0) < 0 && errno == EINTR) {
      }
    }
  }

  static std::string TrustedHome() {
    const long hint = sysconf(_SC_GETPW_R_SIZE_MAX);
    std::vector<char> buffer(
        hint > 0 && hint < 1024 * 1024 ? static_cast<size_t>(hint) : 16384);
    passwd value{};
    passwd* result = nullptr;
    if (getpwuid_r(getuid(), &value, buffer.data(), buffer.size(), &result) != 0 ||
        !result || !result->pw_dir || result->pw_dir[0] != '/') {
      return {};
    }
    return result->pw_dir;
  }

  static bool EnsureOwnedDirectory(const std::string& path, bool make_private) {
    if (mkdir(path.c_str(), 0700) != 0 && errno != EEXIST) {
      return false;
    }
    struct stat status {};
    if (lstat(path.c_str(), &status) != 0 || !S_ISDIR(status.st_mode) ||
        status.st_uid != getuid()) {
      return false;
    }
    return !make_private || chmod(path.c_str(), 0700) == 0;
  }

  void RemovePointerIfOwned() const {
    const int fd = open(pointer_path_.c_str(), O_RDONLY | O_NOFOLLOW);
    if (fd < 0) {
      return;
    }
    struct stat opened {};
    std::array<char, 4096> buffer{};
    const ssize_t count = fstat(fd, &opened) == 0 && S_ISREG(opened.st_mode) &&
                                  opened.st_uid == getuid()
                              ? read(fd, buffer.data(), buffer.size())
                              : -1;
    close(fd);
    const std::string expected = grant_path_ + "\n";
    if (count != static_cast<ssize_t>(expected.size()) ||
        std::string(buffer.data(), static_cast<size_t>(count)) != expected) {
      return;
    }
    struct stat current {};
    if (lstat(pointer_path_.c_str(), &current) == 0 &&
        current.st_dev == opened.st_dev && current.st_ino == opened.st_ino) {
      unlink(pointer_path_.c_str());
    }
  }

  bool active_ = false;
  std::string agent_root_;
  std::string profile_path_;
  std::string session_path_;
  std::string socket_path_;
  std::string grant_path_;
  std::string replay_path_;
  std::string pointer_path_;
};

#endif  // SACCADE_CEF_HOST_SACCADE_DIRECT_SESSION_MAC_H_
