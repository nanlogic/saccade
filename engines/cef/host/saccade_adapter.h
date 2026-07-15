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

class SaccadeScreenshotObserver;
class SaccadeTextInsertObserver;

// Browser-process lifecycle adapter. It intentionally exposes no CEF type on
// the wire and never reads cookies or storage. Values and audit screenshots
// cross only their fixed, policy-gated command surfaces.
class SaccadeAdapter {
 public:
  enum class AgentUiState {
    kUnavailable,
    kOff,
    kOn,
    kPaused,
  };

  static SaccadeAdapter* GetInstance();

  void OnBrowserCreated(CefRefPtr<CefBrowser> browser);
  void OnBrowserFocused(CefRefPtr<CefBrowser> browser);
  void OnAddressChanged(CefRefPtr<CefBrowser> browser,
                        CefRefPtr<CefFrame> frame,
                        const CefString& url);
  void OnTitleChanged(CefRefPtr<CefBrowser> browser, const CefString& title);
  void OnLoadCompleted(CefRefPtr<CefBrowser> browser);
  void OnBrowserClosed(CefRefPtr<CefBrowser> browser);
  bool OnRendererMessage(CefRefPtr<CefBrowser> browser,
                         CefRefPtr<CefFrame> frame,
                         CefProcessId source_process,
                         CefRefPtr<CefProcessMessage> message);
  AgentUiState ToggleAgentForVisibleTab();
  AgentUiState GetAgentUiState();

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

  struct FormCommandState {
    std::string command;
    std::string payload;
    std::string error;
    uint64_t basis_page_revision = 0;
    bool done = false;
    bool ok = false;
  };

  struct BrowserRole {
    bool is_popup = false;
    int opener_id = 0;
  };

  friend class SaccadeScreenshotObserver;
  friend class SaccadeTextInsertObserver;

  SaccadeAdapter() = default;
  ~SaccadeAdapter();

  SaccadeAdapter(const SaccadeAdapter&) = delete;
  SaccadeAdapter& operator=(const SaccadeAdapter&) = delete;

  void ConfigureIfRequested();
  void StartBridge();
  void Stop();
  void ResetPageStateLocked(const std::string& reason);
  std::string CurrentTabIdLocked() const;
  void Serve();
  std::string HandleRequest(const std::string& line);
  std::string StatusJson();
  CefRefPtr<CefDictionaryValue> TruthResult();
  CefRefPtr<CefDictionaryValue> ActionsResult();
  std::string NextFactResponse(int id, int timeout_ms);
  std::string NextReceiptResponse(int id, int timeout_ms);
  std::string ActResponse(int id, CefRefPtr<CefDictionaryValue> params);
  std::string FormCommandResponse(int id,
                                  const std::string& command,
                                  CefRefPtr<CefDictionaryValue> params);
  std::string TypeFieldTextResponse(
      int id, CefRefPtr<CefDictionaryValue> params);
  std::string ScreenshotAuditResponse(
      int id,
      CefRefPtr<CefDictionaryValue> params);
  void NavigateOnUi(std::string url);
  void StartReflexOnUi();
  void DispatchFormCommandOnUi(int request_id,
                               std::string command,
                               std::string input_json);
  void DispatchTextOnUi(std::u16string text,
                        int browser_id,
                        uint64_t page_revision);
  void OnTextInsertResult(int message_id,
                          bool success,
                          const void* result,
                          size_t result_size);
  void CaptureScreenshotOnUi();
  void OnScreenshotResult(int message_id,
                          bool success,
                          const void* result,
                          size_t result_size);
  void DispatchPointerOnUi(int x, int y);
  void CloseOnUi();
  bool WriteGrant();
  void AppendValueFreeReplay(const std::string& event,
                             CefRefPtr<CefDictionaryValue> result,
                             uint64_t basis_page_revision,
                             uint64_t observed_page_revision);

  std::mutex state_mutex_;
  std::mutex grant_mutex_;
  CefRefPtr<CefBrowser> browser_;
  std::map<int, CefRefPtr<CefBrowser>> browsers_;
  std::map<int, BrowserRole> browser_roles_;
  std::string current_url_;
  std::string current_title_;
  uint64_t page_revision_ = 1;
  bool paused_ = false;
  bool configured_ = false;
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
  std::condition_variable form_cv_;
  std::map<int, FormCommandState> form_commands_;
  std::atomic<int> next_form_request_id_{1};
  std::condition_variable screenshot_cv_;
  bool screenshot_pending_ = false;
  bool screenshot_done_ = false;
  bool screenshot_ok_ = false;
  int screenshot_message_id_ = 0;
  std::string screenshot_error_;
  std::vector<unsigned char> screenshot_bytes_;
  CefRefPtr<CefRegistration> screenshot_registration_;
  std::condition_variable text_insert_cv_;
  bool text_insert_pending_ = false;
  bool text_insert_done_ = false;
  bool text_insert_ok_ = false;
  int text_insert_message_id_ = 0;
  std::string text_insert_error_;
  CefRefPtr<CefRegistration> text_insert_registration_;

  std::string socket_path_;
  std::string grant_path_;
  std::string replay_path_;
  std::string capability_;
  std::mutex replay_mutex_;
  std::atomic<bool> stopping_{false};
  std::atomic<int> listener_fd_{-1};
  std::thread server_thread_;
};

#endif  // SACCADE_CEF_HOST_SACCADE_ADAPTER_H_
