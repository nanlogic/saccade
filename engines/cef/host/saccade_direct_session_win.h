// Copyright (c) 2026 Saccade contributors.

#ifndef SACCADE_CEF_HOST_SACCADE_DIRECT_SESSION_WIN_H_
#define SACCADE_CEF_HOST_SACCADE_DIRECT_SESSION_WIN_H_

#include <string>

class SaccadeDirectSessionWin {
 public:
  SaccadeDirectSessionWin();
  ~SaccadeDirectSessionWin();

  bool configured() const { return configured_; }

 private:
  SaccadeDirectSessionWin(const SaccadeDirectSessionWin&) = delete;
  SaccadeDirectSessionWin& operator=(const SaccadeDirectSessionWin&) = delete;

  bool configured_ = false;
  bool owns_session_ = false;
  std::wstring session_path_;
  std::wstring grant_path_;
  std::wstring replay_path_;
  std::wstring pointer_path_;
};

#endif  // SACCADE_CEF_HOST_SACCADE_DIRECT_SESSION_WIN_H_
