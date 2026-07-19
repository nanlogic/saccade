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
#include "include/cef_download_item.h"
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
  void OnHumanVerificationResourceResult(CefRefPtr<CefBrowser> browser,
                                         std::string provider,
                                         int http_status,
                                         int request_status);
  void RetryHumanVerification(CefRefPtr<CefBrowser> browser);
  void OnDownloadUpdated(CefRefPtr<CefBrowser> browser,
                         CefRefPtr<CefDownloadItem> download_item);
  void OnBrowserClosed(CefRefPtr<CefBrowser> browser);
  bool OnRendererMessage(CefRefPtr<CefBrowser> browser,
                         CefRefPtr<CefFrame> frame,
                         CefProcessId source_process,
                         CefRefPtr<CefProcessMessage> message);
  AgentUiState ToggleAgentForVisibleTab();
  AgentUiState GetAgentUiState();
  void OpenUserTabOnUi(std::string url);
  void OpenRoutedTabOnUi(std::string url);

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
    uint64_t layout_epoch = 0;
    bool opens_new_context = false;
    std::string destination_url;
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
    uint64_t basis_layout_epoch = 0;
    uint64_t observed_layout_epoch = 0;
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

  struct BrowserMetadata {
    std::string url;
    std::string title;
    uint64_t page_revision = 1;
  };

  struct DownloadState {
    uint32_t id = 0;
    int browser_id = 0;
    uint64_t page_revision = 0;
    std::string file_name;
    std::string mime_type;
    std::string source_origin;
    std::string status = "starting";
    int percent_complete = -1;
    int64_t received_bytes = 0;
    int64_t total_bytes = -1;
    int interrupt_reason = 0;
    bool agent_visible_at_start = false;
  };

  struct HumanVerificationFailure {
    std::string provider;
    int http_status = 0;
    int request_status = 0;
    uint64_t page_revision = 0;
    bool user_notified = false;
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
  bool RefreshActionMap(int timeout_ms);
  std::string CurrentTabIdLocked() const;
  void Serve();
  std::string HandleRequest(const std::string& line);
  std::string StatusJson();
  std::string TabRegistryJson();
  std::string DownloadsJson();
  std::string SelectTabResponse(int id, CefRefPtr<CefDictionaryValue> params);
  CefRefPtr<CefDictionaryValue> TruthResult();
  CefRefPtr<CefDictionaryValue> ActionsResult();
  std::string NextFactResponse(int id, int timeout_ms);
  std::string NextReceiptResponse(int id, int timeout_ms);
  std::string ActResponse(int id, CefRefPtr<CefDictionaryValue> params);
  std::string DragResponse(int id, CefRefPtr<CefDictionaryValue> params);
  std::string FormCommandResponse(int id,
                                  const std::string& command,
                                  CefRefPtr<CefDictionaryValue> params);
  std::string ProtectedFillResponse(
      int id, CefRefPtr<CefDictionaryValue> params);
  std::string TypeFieldTextResponse(
      int id, CefRefPtr<CefDictionaryValue> params);
  std::string ScreenshotAuditResponse(
      int id,
      CefRefPtr<CefDictionaryValue> params);
  void NavigateOnUi(std::string url);
  void NavigateHistoryOnUi(std::string action);
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
  void DispatchPointerOnUi(int x,
                           int y,
                           std::string action_id,
                           int browser_id,
                           uint64_t page_revision,
                           uint64_t layout_epoch);
  void ClearPendingAgentChildOpenerOnUi(int browser_id);
  void DispatchDragOnUi(int start_x,
                        int start_y,
                        int end_x,
                        int end_y,
                        std::string action_id,
                        int browser_id,
                        uint64_t page_revision,
                        uint64_t layout_epoch);
  void ReleaseDragOnUi(int end_x, int end_y, int browser_id);
  void CloseOnUi();
  void OpenAgentTabOnUi(std::string url);
  bool OpenChromeTabOnUi(std::string url, bool grant_to_agent);
  void RefreshCollectorOnUi();
  bool WriteGrant();
  bool WriteCurrentPointer();
  void RemoveCurrentPointerIfOwned();
  bool CurrentTabGrantedLocked() const;
  bool CurrentTabPausedLocked() const;
  bool CurrentTabActiveLocked() const;
  bool CurrentTabHasHumanVerificationFailureLocked() const;
  void RefreshAgentSwitchOnUi();
  void AppendValueFreeReplay(const std::string& event,
                             CefRefPtr<CefDictionaryValue> result,
                             uint64_t basis_page_revision,
                             uint64_t observed_page_revision);

  std::mutex state_mutex_;
  std::mutex grant_mutex_;
  CefRefPtr<CefBrowser> browser_;
  std::map<int, CefRefPtr<CefBrowser>> browsers_;
  std::map<int, BrowserRole> browser_roles_;
  std::map<int, BrowserMetadata> browser_metadata_;
  std::map<uint32_t, DownloadState> downloads_;
  std::map<int, HumanVerificationFailure> human_verification_failures_;
  std::set<int> agent_granted_browser_ids_;
  std::set<int> agent_created_browser_ids_;
  std::set<int> agent_paused_browser_ids_;
  std::map<int, std::string> pending_agent_child_openers_;
  std::deque<std::string> pending_agent_tab_urls_;
  std::deque<std::string> pending_user_tab_urls_;
  std::string current_url_;
  std::string current_title_;
  uint64_t page_revision_ = 1;
  uint64_t layout_epoch_ = 1;
  uint64_t last_layout_page_revision_ = 0;
  bool configured_ = false;
  bool started_ = false;
  bool collector_ready_ = false;
  std::string collector_error_;
  std::vector<ControlFact> controls_;
  std::deque<TargetFact> pending_facts_;
  std::map<std::string, TargetFact> actions_;
  std::map<std::string, TargetFact> staged_actions_;
  int action_scan_generation_ = 0;
  uint64_t action_map_serial_ = 0;
  std::set<std::string> dispatched_actions_;
  std::deque<ReflexReceipt> pending_receipts_;
  std::condition_variable fact_cv_;
  std::condition_variable action_map_cv_;
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
  std::string current_pointer_path_;
  std::string capability_;
  std::mutex replay_mutex_;
  std::atomic<bool> stopping_{false};
  std::atomic<int> listener_fd_{-1};
  std::thread server_thread_;
};

#endif  // SACCADE_CEF_HOST_SACCADE_ADAPTER_H_
