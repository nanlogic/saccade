// Copyright (c) 2026 Saccade contributors.
// Use of this source code is governed by a BSD-style license.

#ifndef SACCADE_CEF_HOST_SACCADE_ADAPTER_H_
#define SACCADE_CEF_HOST_SACCADE_ADAPTER_H_

#include <atomic>
#include <mutex>
#include <string>
#include <thread>

#include "include/cef_browser.h"

// Browser-process lifecycle adapter. It intentionally exposes no CEF type on
// the wire and never reads page values, cookies, storage, or screenshots.
class SaccadeAdapter {
 public:
  static SaccadeAdapter* GetInstance();

  void OnBrowserCreated(CefRefPtr<CefBrowser> browser);
  void OnAddressChanged(CefRefPtr<CefBrowser> browser,
                        CefRefPtr<CefFrame> frame,
                        const CefString& url);
  void OnTitleChanged(CefRefPtr<CefBrowser> browser, const CefString& title);
  void OnBrowserClosed(CefRefPtr<CefBrowser> browser);

 private:
  SaccadeAdapter() = default;
  ~SaccadeAdapter();

  SaccadeAdapter(const SaccadeAdapter&) = delete;
  SaccadeAdapter& operator=(const SaccadeAdapter&) = delete;

  void StartIfRequested();
  void Stop();
  void Serve();
  std::string HandleRequest(const std::string& line);
  std::string StatusJson();
  void NavigateOnUi(std::string url);
  void CloseOnUi();
  bool WriteGrant();

  std::mutex state_mutex_;
  std::mutex grant_mutex_;
  CefRefPtr<CefBrowser> browser_;
  std::string current_url_;
  std::string current_title_;
  uint64_t page_revision_ = 1;
  bool paused_ = false;
  bool started_ = false;

  std::string socket_path_;
  std::string grant_path_;
  std::string capability_;
  std::atomic<bool> stopping_{false};
  std::atomic<int> listener_fd_{-1};
  std::thread server_thread_;
};

#endif  // SACCADE_CEF_HOST_SACCADE_ADAPTER_H_
