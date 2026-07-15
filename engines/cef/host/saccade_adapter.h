// Copyright (c) 2026 Saccade contributors.
// Use of this source code is governed by a BSD-style license.

#ifndef SACCADE_CEF_HOST_SACCADE_ADAPTER_H_
#define SACCADE_CEF_HOST_SACCADE_ADAPTER_H_

#include <atomic>
#include <condition_variable>
#include <deque>
#include <map>
#include <mutex>
#include <set>
#include <string>
#include <thread>
#include <vector>

#include "include/cef_browser.h"
#include "include/cef_process_message.h"

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
  bool OnRendererMessage(CefRefPtr<CefBrowser> browser,
                         CefRefPtr<CefFrame> frame,
                         CefProcessId source_process,
                         CefRefPtr<CefProcessMessage> message);

 private:
  struct ControlFact {
    std::string fact_id;
    std::string kind;
    bool sensitive = false;
    bool complete = false;
  };

  struct TargetFact {
    std::string action_id;
    std::string role;
    std::string label;
    double left = 0;
    double top = 0;
    double width = 0;
    double height = 0;
    double renderer_epoch_ms = 0;
    uint64_t page_revision = 0;
  };

  struct ReflexReceipt {
    std::string action_id;
    double client_x = 0;
    double client_y = 0;
    int hits = 0;
    int misses = 0;
    bool finished = false;
    double renderer_epoch_ms = 0;
    uint64_t basis_page_revision = 0;
    uint64_t observed_page_revision = 0;
  };

  SaccadeAdapter() = default;
  ~SaccadeAdapter();

  SaccadeAdapter(const SaccadeAdapter&) = delete;
  SaccadeAdapter& operator=(const SaccadeAdapter&) = delete;

  void StartIfRequested();
  void Stop();
  void Serve();
  std::string HandleRequest(const std::string& line);
  std::string StatusJson();
  CefRefPtr<CefDictionaryValue> TruthResult();
  CefRefPtr<CefDictionaryValue> ActionsResult();
  std::string NextFactResponse(int id, int timeout_ms);
  std::string NextReceiptResponse(int id, int timeout_ms);
  std::string ActResponse(int id, CefRefPtr<CefDictionaryValue> params);
  void NavigateOnUi(std::string url);
  void StartReflexOnUi();
  void DispatchPointerOnUi(int x, int y);
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
  bool collector_ready_ = false;
  std::string collector_error_;
  std::vector<ControlFact> controls_;
  std::deque<TargetFact> pending_facts_;
  std::map<std::string, TargetFact> actions_;
  std::set<std::string> dispatched_actions_;
  std::deque<ReflexReceipt> pending_receipts_;
  std::condition_variable fact_cv_;
  std::condition_variable receipt_cv_;

  std::string socket_path_;
  std::string grant_path_;
  std::string capability_;
  std::atomic<bool> stopping_{false};
  std::atomic<int> listener_fd_{-1};
  std::thread server_thread_;
};

#endif  // SACCADE_CEF_HOST_SACCADE_ADAPTER_H_
