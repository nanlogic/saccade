// Copyright (c) 2026 Saccade contributors.
// Use of this source code is governed by a BSD-style license.

#include "tests/cefsimple/saccade_adapter.h"

#include <errno.h>
#include <fcntl.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <limits>
#include <memory>
#include <utility>

#include "include/base/cef_callback.h"
#include "include/cef_devtools_message_observer.h"
#include "include/cef_id_mappers.h"
#include "include/cef_parser.h"
#include "include/wrapper/cef_closure_task.h"
#include "include/wrapper/cef_helpers.h"
#if defined(OS_MAC)
#include "tests/cefsimple/saccade_agent_switch_mac.h"
#endif

namespace {

constexpr char kProtocol[] = "saccade-engine-control-v1";
constexpr char kContractVersion[] = "1.0";

const char* AgentUiStateName(SaccadeAdapter::AgentUiState state) {
  switch (state) {
    case SaccadeAdapter::AgentUiState::kOn:
      return "on";
    case SaccadeAdapter::AgentUiState::kOff:
      return "off";
    case SaccadeAdapter::AgentUiState::kPaused:
      return "paused";
    case SaccadeAdapter::AgentUiState::kUnavailable:
      return "unavailable";
  }
  return "unavailable";
}

struct ToolbarAgentRequest {
  std::mutex mutex;
  std::condition_variable ready;
  bool done = false;
  SaccadeAdapter::AgentUiState state =
      SaccadeAdapter::AgentUiState::kUnavailable;
};

std::string JsonString(CefRefPtr<CefValue> value) {
  return CefWriteJSON(value, JSON_WRITER_DEFAULT).ToString();
}

CefRefPtr<CefListValue> CapabilityList() {
  auto list = CefListValue::Create();
  std::vector<const char*> capabilities = {
      "ping",        "shell_status", "navigate",    "reload",
      "back",        "forward",      "pause",       "resume",
      "close",       "truth",        "actions",     "next_fact",
      "act",         "act_drag",     "next_receipt", "reflex_start",
      "form_inventory",
      "inspect_fields", "form_compile_plan", "form_execute_plan",
      "screenshot_policy", "screenshot_audit", "form_reveal_more",
      "article_text", "type_field_text", "render_preflight",
      "open_agent_tab", "tab_registry", "select_tab", "downloads"};
#if defined(OS_MAC)
  capabilities.push_back("protected_fill");
#endif
  list->SetSize(capabilities.size());
  for (size_t index = 0; index < capabilities.size(); ++index) {
    list->SetString(index, capabilities[index]);
  }
  return list;
}

bool ValidExpectedSurface(const std::string& expected_surface) {
  return expected_surface == "page" || expected_surface == "github_issue" ||
         expected_surface == "github_discussion";
}

std::string DownloadFileName(CefRefPtr<CefDownloadItem> item) {
  std::string name = item->GetFullPath().ToString();
  if (name.empty()) {
    name = item->GetSuggestedFileName().ToString();
  }
  const size_t separator = name.find_last_of("/\\");
  if (separator != std::string::npos) {
    name = name.substr(separator + 1);
  }
  return name.empty() || name == "." || name == ".." ? "download" : name;
}

std::string DownloadSourceOrigin(const std::string& url) {
  CefURLParts parts;
  if (!CefParseURL(url, parts)) {
    return "unknown";
  }
  const std::string scheme = CefString(&parts.scheme).ToString();
  if (scheme == "file") {
    return "file://local";
  }
  const std::string host = CefString(&parts.host).ToString();
  if (scheme.empty() || host.empty()) {
    return "unknown";
  }
  std::string origin = scheme + "://" + host;
  const std::string port = CefString(&parts.port).ToString();
  if (!port.empty()) {
    origin.append(":").append(port);
  }
  return origin;
}

bool GithubNewSurface(const std::string& path, const std::string& kind) {
  std::vector<std::string> segments;
  size_t start = 0;
  while (start < path.size()) {
    while (start < path.size() && path[start] == '/') {
      ++start;
    }
    if (start >= path.size()) {
      break;
    }
    const size_t end = path.find('/', start);
    segments.push_back(path.substr(start, end == std::string::npos
                                             ? std::string::npos
                                             : end - start));
    if (end == std::string::npos) {
      break;
    }
    start = end + 1;
  }
  return segments.size() >= 4 && !segments[0].empty() &&
         !segments[1].empty() && segments[2] == kind &&
         segments[3] == "new";
}

bool TaskSurfaceMatches(const std::string& expected_surface,
                        const std::string& url) {
  if (expected_surface == "page") {
    return true;
  }
  CefURLParts parts;
  if (!CefParseURL(url, parts)) {
    return false;
  }
  const std::string host = CefString(&parts.host).ToString();
  const std::string path = CefString(&parts.path).ToString();
  if (host != "github.com") {
    return false;
  }
  return expected_surface == "github_issue"
             ? GithubNewSurface(path, "issues")
             : GithubNewSurface(path, "discussions");
}

void OverridePreflightRoute(CefRefPtr<CefDictionaryValue> result,
                            const std::string& route,
                            const std::string& reason,
                            const std::string& typed_reason,
                            bool observation_consistent) {
  result->SetString("verdict", "red");
  result->SetString("recommended_route", route);
  result->SetBool("agent_input_allowed", false);
  auto reasons = CefListValue::Create();
  reasons->SetSize(1);
  reasons->SetString(0, reason);
  result->SetList("reason_codes", reasons);

  auto observations = result->GetDictionary("observations");
  if (observations) {
    observations->SetBool("observation_base_consistent",
                          observation_consistent);
  }
  auto agreement = result->GetDictionary("agreement");
  if (!agreement) {
    return;
  }
  agreement->SetString("structural_verdict", "red");
  agreement->SetString("recommended_route", route);
  auto typed_reasons = CefListValue::Create();
  typed_reasons->SetSize(1);
  typed_reasons->SetString(0, typed_reason);
  agreement->SetList("typed_reason_codes", typed_reasons);
  auto observation_base = agreement->GetDictionary("observation_base");
  if (observation_base) {
    observation_base->SetBool("consistent", observation_consistent);
  }
}

std::string Fnv1aUtf16(const std::u16string& value) {
  uint32_t result = 2166136261U;
  for (char16_t character : value) {
    result ^= static_cast<uint32_t>(character);
    result *= 16777619U;
  }
  char output[9] = {};
  std::snprintf(output, sizeof(output), "%08x", result);
  return output;
}

std::string Response(int id, CefRefPtr<CefDictionaryValue> result) {
  auto root = CefDictionaryValue::Create();
  root->SetInt("id", id);
  root->SetBool("ok", true);
  root->SetDictionary("result", result);
  auto value = CefValue::Create();
  value->SetDictionary(root);
  return JsonString(value);
}

std::string ErrorResponse(int id,
                          const std::string& code,
                          const std::string& detail) {
  auto error = CefDictionaryValue::Create();
  error->SetString("code", code);
  error->SetString("detail", detail);
  auto root = CefDictionaryValue::Create();
  root->SetInt("id", id);
  root->SetBool("ok", false);
  root->SetDictionary("error", error);
  auto value = CefValue::Create();
  value->SetDictionary(root);
  return JsonString(value);
}

std::string TrustedOrigin(const std::string& url) {
  CefURLParts parts;
  if (!CefParseURL(url, parts)) {
    return "unknown";
  }
  const std::string scheme = CefString(&parts.scheme).ToString();
  const std::string host = CefString(&parts.host).ToString();
  const std::string port = CefString(&parts.port).ToString();
  if (scheme == "file") {
    return "file://";
  }
  if (scheme.empty() || host.empty()) {
    return "unknown";
  }
  return scheme + "://" + host + (port.empty() ? "" : ":" + port);
}

bool ConstantTimeEqual(const std::string& left, const std::string& right) {
  size_t different = left.size() ^ right.size();
  const size_t count = left.size() > right.size() ? left.size() : right.size();
  for (size_t index = 0; index < count; ++index) {
    const unsigned char a =
        index < left.size() ? static_cast<unsigned char>(left[index]) : 0;
    const unsigned char b =
        index < right.size() ? static_cast<unsigned char>(right[index]) : 0;
    different |= a ^ b;
  }
  return different == 0;
}

std::string RandomCapability() {
  std::array<unsigned char, 32> bytes{};
  arc4random_buf(bytes.data(), bytes.size());
  static constexpr char kHex[] = "0123456789abcdef";
  std::string result;
  result.reserve(bytes.size() * 2);
  for (unsigned char byte : bytes) {
    result.push_back(kHex[byte >> 4]);
    result.push_back(kHex[byte & 0x0f]);
  }
  return result;
}

bool WriteAll(int fd, const std::string& text) {
  size_t written = 0;
  while (written < text.size()) {
    const ssize_t count = write(fd, text.data() + written, text.size() - written);
    if (count < 0 && errno == EINTR) {
      continue;
    }
    if (count <= 0) {
      return false;
    }
    written += static_cast<size_t>(count);
  }
  return true;
}

int RequestTimeoutMs(CefRefPtr<CefDictionaryValue> params) {
  if (!params || !params->HasKey("timeout_ms")) {
    return 2000;
  }
  int timeout_ms = 0;
  if (params->GetType("timeout_ms") == VTYPE_INT) {
    timeout_ms = params->GetInt("timeout_ms");
  } else if (params->GetType("timeout_ms") == VTYPE_DOUBLE) {
    timeout_ms = static_cast<int>(params->GetDouble("timeout_ms"));
  }
  if (timeout_ms < 1) {
    return 1;
  }
  return timeout_ms > 5000 ? 5000 : timeout_ms;
}

uint64_t RequestRevision(CefRefPtr<CefDictionaryValue> params) {
  if (!params || !params->HasKey("basis_page_revision")) {
    return 0;
  }
  if (params->GetType("basis_page_revision") == VTYPE_INT) {
    const int revision = params->GetInt("basis_page_revision");
    return revision > 0 ? static_cast<uint64_t>(revision) : 0;
  }
  if (params->GetType("basis_page_revision") == VTYPE_DOUBLE) {
    const double revision = params->GetDouble("basis_page_revision");
    if (std::isfinite(revision) && revision >= 1 &&
        revision <= 9007199254740991.0) {
      return static_cast<uint64_t>(revision);
    }
  }
  return 0;
}

bool ValidAssignments(CefRefPtr<CefDictionaryValue> params) {
  if (!params || params->GetType("assignments") != VTYPE_DICTIONARY) {
    return false;
  }
  auto assignments = params->GetDictionary("assignments");
  CefDictionaryValue::KeyList keys;
  if (!assignments || !assignments->GetKeys(keys) || keys.size() > 5000) {
    return false;
  }
  for (const auto& key : keys) {
    if (key.empty() || key.length() > 256) {
      return false;
    }
    const cef_value_type_t type = assignments->GetType(key);
    if (type != VTYPE_BOOL && type != VTYPE_INT && type != VTYPE_DOUBLE &&
        type != VTYPE_STRING) {
      return false;
    }
    if (type == VTYPE_STRING && assignments->GetString(key).length() > 16384) {
      return false;
    }
  }
  return true;
}

bool ValidInspectFields(CefRefPtr<CefDictionaryValue> params) {
  if (!params || (!params->HasKey("field_ids") && !params->HasKey("fields"))) {
    return true;
  }
  const char* key = params->HasKey("field_ids") ? "field_ids" : "fields";
  if (params->GetType(key) != VTYPE_LIST) {
    return false;
  }
  auto fields = params->GetList(key);
  if (!fields || fields->GetSize() > 500) {
    return false;
  }
  for (size_t index = 0; index < fields->GetSize(); ++index) {
    if (fields->GetType(index) != VTYPE_STRING ||
        fields->GetString(index).empty() ||
        fields->GetString(index).length() > 256) {
      return false;
    }
  }
  return true;
}

}  // namespace

class SaccadeScreenshotObserver : public CefDevToolsMessageObserver {
 public:
  explicit SaccadeScreenshotObserver(SaccadeAdapter* adapter)
      : adapter_(adapter) {}

  void OnDevToolsMethodResult(CefRefPtr<CefBrowser> browser,
                              int message_id,
                              bool success,
                              const void* result,
                              size_t result_size) override {
    CEF_REQUIRE_UI_THREAD();
    adapter_->OnScreenshotResult(message_id, success, result, result_size);
  }

 private:
  SaccadeAdapter* const adapter_;
  IMPLEMENT_REFCOUNTING(SaccadeScreenshotObserver);
};

class SaccadeTextInsertObserver : public CefDevToolsMessageObserver {
 public:
  explicit SaccadeTextInsertObserver(SaccadeAdapter* adapter)
      : adapter_(adapter) {}

  void OnDevToolsMethodResult(CefRefPtr<CefBrowser> browser,
                              int message_id,
                              bool success,
                              const void* result,
                              size_t result_size) override {
    CEF_REQUIRE_UI_THREAD();
    adapter_->OnTextInsertResult(message_id, success, result, result_size);
  }

 private:
  SaccadeAdapter* const adapter_;
  IMPLEMENT_REFCOUNTING(SaccadeTextInsertObserver);
};

SaccadeAdapter* SaccadeAdapter::GetInstance() {
  static SaccadeAdapter adapter;
  return &adapter;
}

SaccadeAdapter::~SaccadeAdapter() {
  Stop();
}

void SaccadeAdapter::OnBrowserCreated(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();
  std::string agent_url;
  std::string user_url;
  bool agent_child_opened = false;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    const int browser_id = browser->GetIdentifier();
    browsers_[browser_id] = browser;
    browser_roles_[browser_id] = {
        .is_popup = browser->IsPopup(),
        .opener_id = browser->GetHost()->GetOpenerIdentifier(),
    };
    BrowserMetadata metadata;
    if (browser->GetMainFrame()) {
      metadata.url = browser->GetMainFrame()->GetURL().ToString();
    }
    metadata.page_revision = page_revision_;
    auto pending_agent_child = std::find_if(
        pending_agent_child_openers_.begin(),
        pending_agent_child_openers_.end(),
        [&metadata](const auto& pending) {
          return !metadata.url.empty() && pending.second == metadata.url;
        });
    if (!pending_agent_tab_urls_.empty()) {
      agent_url = std::move(pending_agent_tab_urls_.front());
      pending_agent_tab_urls_.pop_front();
      agent_granted_browser_ids_.insert(browser_id);
      agent_created_browser_ids_.insert(browser_id);
      agent_paused_browser_ids_.erase(browser_id);
      browser_ = browser;
      current_url_ = agent_url;
      current_title_.clear();
      ++page_revision_;
      metadata.url = agent_url;
      metadata.title.clear();
      metadata.page_revision = page_revision_;
      ResetPageStateLocked("Agent opened a dedicated browser tab");
    } else if (!pending_user_tab_urls_.empty()) {
      user_url = std::move(pending_user_tab_urls_.front());
      pending_user_tab_urls_.pop_front();
      browser_ = browser;
      current_url_ = user_url;
      current_title_.clear();
      ++page_revision_;
      metadata.url = user_url;
      metadata.title.clear();
      metadata.page_revision = page_revision_;
      ResetPageStateLocked("Human opened a browser tab");
    } else if (pending_agent_child != pending_agent_child_openers_.end()) {
      pending_agent_child_openers_.erase(pending_agent_child);
      agent_child_opened = true;
      agent_granted_browser_ids_.insert(browser_id);
      agent_created_browser_ids_.insert(browser_id);
      agent_paused_browser_ids_.erase(browser_id);
      browser_ = browser;
      current_url_ = metadata.url;
      current_title_.clear();
      ++page_revision_;
      metadata.title.clear();
      metadata.page_revision = page_revision_;
      ResetPageStateLocked("Agent action opened a child browser tab");
    } else {
      const char* initial_grant = getenv("SACCADE_ENGINE_INITIAL_TAB_GRANT");
      if (browsers_.size() == 1 && initial_grant &&
          std::string(initial_grant) == "1") {
        agent_granted_browser_ids_.insert(browser_id);
        agent_created_browser_ids_.insert(browser_id);
        agent_paused_browser_ids_.erase(browser_id);
        const char* initial_url = getenv("SACCADE_ENGINE_INITIAL_URL");
        if (initial_url && initial_url[0] != '\0') {
          current_url_ = initial_url;
          metadata.url = initial_url;
        }
      }
    }
    browser_metadata_[browser_id] = metadata;
    if (!browser_) {
      browser_ = browser;
      if (current_url_.empty() && browser->GetMainFrame()) {
        current_url_ = browser->GetMainFrame()->GetURL().ToString();
      }
    }
  }
  ConfigureIfRequested();
  if (started_ && (!agent_url.empty() || agent_child_opened)) {
    WriteGrant();
  }
  RefreshAgentSwitchOnUi();
  if (!agent_url.empty() && browser->GetMainFrame()) {
    browser->GetMainFrame()->LoadURL(agent_url);
  } else if (!user_url.empty() && browser->GetMainFrame()) {
    browser->GetMainFrame()->LoadURL(user_url);
  }
}

void SaccadeAdapter::OnBrowserFocused(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();
  bool refresh_grant = false;
  CefRefPtr<CefFrame> frame;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!browser || browsers_.find(browser->GetIdentifier()) == browsers_.end() ||
        (browser_ && browser_->IsSame(browser))) {
      return;
    }
    browser_ = browser;
    frame = browser->GetMainFrame();
    const int browser_id = browser_->GetIdentifier();
    auto& metadata = browser_metadata_[browser_id];
    if (frame) {
      metadata.url = frame->GetURL().ToString();
    }
    current_url_ = metadata.url;
    current_title_ = metadata.title;
    ++page_revision_;
    metadata.page_revision = page_revision_;
    ResetPageStateLocked("visible tab changed while command was pending");
    refresh_grant = started_;
  }
  fact_cv_.notify_all();
  action_map_cv_.notify_all();
  receipt_cv_.notify_all();
  form_cv_.notify_all();
  screenshot_cv_.notify_all();
  if (refresh_grant) {
    WriteGrant();
  }
  RefreshAgentSwitchOnUi();
  if (frame) {
    frame->SendProcessMessage(
        PID_RENDERER,
        CefProcessMessage::Create("saccade.collector.refresh_v1"));
  }
}

void SaccadeAdapter::OnAddressChanged(CefRefPtr<CefBrowser> browser,
                                      CefRefPtr<CefFrame> frame,
                                      const CefString& url) {
  CEF_REQUIRE_UI_THREAD();
  if (!frame->IsMain()) {
    return;
  }
  bool refresh_grant = false;
  bool agent_child_promoted = false;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    const int browser_id = browser ? browser->GetIdentifier() : 0;
    const std::string changed_url = url.ToString();
    if (browser_id != 0 &&
        agent_granted_browser_ids_.find(browser_id) ==
            agent_granted_browser_ids_.end()) {
      const auto pending_agent_child = std::find_if(
          pending_agent_child_openers_.begin(),
          pending_agent_child_openers_.end(),
          [&changed_url](const auto& pending) {
            return pending.second == changed_url;
          });
      if (pending_agent_child != pending_agent_child_openers_.end()) {
        pending_agent_child_openers_.erase(pending_agent_child);
        agent_granted_browser_ids_.insert(browser_id);
        agent_created_browser_ids_.insert(browser_id);
        agent_paused_browser_ids_.erase(browser_id);
        browser_ = browser;
        agent_child_promoted = true;
      }
    }
    auto& metadata = browser_metadata_[browser_id];
    human_verification_failures_.erase(browser_id);
    metadata.url = changed_url;
    if (browser_ && browser_->IsSame(browser)) {
      current_url_ = metadata.url;
      ++page_revision_;
      metadata.page_revision = page_revision_;
      ResetPageStateLocked("page changed while form command was pending");
      refresh_grant = started_;
    } else {
      ++metadata.page_revision;
    }
  }
  form_cv_.notify_all();
  if (refresh_grant) {
    WriteGrant();
  }
  RefreshAgentSwitchOnUi();
  if (agent_child_promoted) {
    RefreshCollectorOnUi();
  }
}

void SaccadeAdapter::OnTitleChanged(CefRefPtr<CefBrowser> browser,
                                    const CefString& title) {
  CEF_REQUIRE_UI_THREAD();
  std::lock_guard<std::mutex> lock(state_mutex_);
  const int browser_id = browser ? browser->GetIdentifier() : 0;
  browser_metadata_[browser_id].title = title.ToString();
  if (browser_ && browser_->IsSame(browser)) {
    current_title_ = title.ToString();
  }
}

void SaccadeAdapter::OnLoadCompleted(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefFrame> frame;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!browser_ || !browser_->IsSame(browser)) {
      return;
    }
    frame = browser_->GetMainFrame();
  }
  if (frame) {
    frame->SendProcessMessage(
        PID_RENDERER,
        CefProcessMessage::Create("saccade.collector.refresh_v1"));
  }
}

void SaccadeAdapter::OnHumanVerificationResourceResult(
    CefRefPtr<CefBrowser> browser,
    std::string provider,
    int http_status,
    int request_status) {
  if (!CefCurrentlyOn(TID_UI)) {
    CefPostTask(
        TID_UI,
        base::BindOnce(&SaccadeAdapter::OnHumanVerificationResourceResult,
                       base::Unretained(this), browser, std::move(provider),
                       http_status, request_status));
    return;
  }
  CEF_REQUIRE_UI_THREAD();
  if (!browser || !browser->IsValid()) {
    return;
  }
  const int browser_id = browser->GetIdentifier();
  const bool succeeded = request_status == UR_SUCCESS && http_status >= 200 &&
                         http_status < 400;
  bool notify_user = false;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (browsers_.find(browser_id) == browsers_.end()) {
      return;
    }
    if (succeeded) {
      human_verification_failures_.erase(browser_id);
      return;
    }
    const auto metadata = browser_metadata_.find(browser_id);
    const uint64_t revision = metadata == browser_metadata_.end()
                                  ? page_revision_
                                  : metadata->second.page_revision;
    auto& failure = human_verification_failures_[browser_id];
    const bool new_failure = failure.page_revision != revision ||
                             failure.provider != provider ||
                             failure.http_status != http_status ||
                             failure.request_status != request_status;
    failure.provider = std::move(provider);
    failure.http_status = http_status;
    failure.request_status = request_status;
    failure.page_revision = revision;
    if (new_failure || !failure.user_notified) {
      failure.user_notified = true;
      notify_user = true;
    }
  }
#if defined(OS_MAC)
  const char* suppress_alert = getenv("SACCADE_SUPPRESS_HUMAN_VERIFICATION_ALERT");
  if (notify_user && (!suppress_alert || strcmp(suppress_alert, "1") != 0)) {
    std::string provider_name;
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      const auto failure = human_verification_failures_.find(browser_id);
      if (failure != human_verification_failures_.end()) {
        provider_name = failure->second.provider;
      }
    }
    SaccadeShowHumanVerificationFailure(browser, provider_name);
  }
#else
  (void)notify_user;
#endif
}

void SaccadeAdapter::RetryHumanVerification(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();
  if (!browser || !browser->IsValid()) {
    return;
  }
  bool refresh_grant = false;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    human_verification_failures_.erase(browser->GetIdentifier());
    if (browser_ && browser_->IsSame(browser)) {
      ++page_revision_;
      browser_metadata_[browser->GetIdentifier()].page_revision = page_revision_;
      ResetPageStateLocked("human verification retry reloaded the page");
      refresh_grant = started_;
    }
  }
  fact_cv_.notify_all();
  action_map_cv_.notify_all();
  receipt_cv_.notify_all();
  form_cv_.notify_all();
  screenshot_cv_.notify_all();
  if (refresh_grant) {
    WriteGrant();
  }
  browser->ReloadIgnoreCache();
}

void SaccadeAdapter::OnDownloadUpdated(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefDownloadItem> download_item) {
  CEF_REQUIRE_UI_THREAD();
  if (!browser || !download_item || !download_item->IsValid()) {
    return;
  }
  std::lock_guard<std::mutex> lock(state_mutex_);
  const int browser_id = browser->GetIdentifier();
  const uint32_t download_id = download_item->GetId();
  auto [entry, inserted] = downloads_.try_emplace(download_id);
  auto& download = entry->second;
  if (inserted) {
    download.id = download_id;
    download.browser_id = browser_id;
    download.agent_visible_at_start =
        agent_granted_browser_ids_.find(browser_id) !=
        agent_granted_browser_ids_.end();
    const auto metadata = browser_metadata_.find(browser_id);
    download.page_revision = metadata == browser_metadata_.end()
                                 ? page_revision_
                                 : metadata->second.page_revision;
    std::string source_url = download_item->GetOriginalUrl().ToString();
    if (source_url.empty()) {
      source_url = download_item->GetURL().ToString();
    }
    download.source_origin = DownloadSourceOrigin(source_url);
  }
  download.file_name = DownloadFileName(download_item);
  download.mime_type = download_item->GetMimeType().ToString();
  download.percent_complete = download_item->GetPercentComplete();
  download.received_bytes = download_item->GetReceivedBytes();
  download.total_bytes = download_item->GetTotalBytes();
  download.interrupt_reason =
      static_cast<int>(download_item->GetInterruptReason());
  if (download_item->IsComplete()) {
    download.status = "complete";
  } else if (download_item->IsCanceled()) {
    download.status = "canceled";
  } else if (download_item->IsInterrupted()) {
    download.status = "interrupted";
  } else if (download_item->IsInProgress()) {
    download.status = "in_progress";
  } else {
    download.status = "starting";
  }
  while (downloads_.size() > 128) {
    downloads_.erase(downloads_.begin());
  }
}

bool SaccadeAdapter::OnRendererMessage(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    CefProcessId source_process,
    CefRefPtr<CefProcessMessage> message) {
  CEF_REQUIRE_UI_THREAD();
  if (source_process != PID_RENDERER || !frame ||
      !browser_ || !browser_->IsSame(browser) || !message ||
      !message->IsValid()) {
    return false;
  }

  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!CurrentTabActiveLocked()) {
      return true;
    }
  }

  const std::string name = message->GetName().ToString();
  if (name.rfind("saccade.renderer.", 0) != 0) {
    return false;
  }
  const bool is_form_response =
      name == "saccade.renderer.form_response_v1";
  if (!is_form_response && !frame->IsMain()) {
    return false;
  }
  auto arguments = message->GetArgumentList();
  if (!arguments) {
    return true;
  }

  if (is_form_response && arguments->GetSize() == 3) {
    const int request_id = arguments->GetInt(0);
    const bool response_ok = arguments->GetBool(1);
    const std::string payload = arguments->GetString(2).ToString();
    bool refresh = false;
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      auto pending = form_commands_.find(request_id);
      if (pending == form_commands_.end() || pending->second.done) {
        return true;
      }
      auto& state = pending->second;
      ++state.received_responses;

      const bool bounded = payload.size() <= 1024 * 1024;
      auto parsed = response_ok && bounded
                        ? CefParseJSON(payload, JSON_PARSER_RFC)
                        : nullptr;
      const bool valid = parsed && parsed->GetType() == VTYPE_DICTIONARY;
      if (valid) {
        auto result = parsed->GetDictionary();
        const int field_count =
            state.command == "inventory" ? result->GetInt("field_count") : 0;
        const int eligible_count =
            state.command == "inventory" ? result->GetInt("eligible_count") : 0;
        if (state.command == "inventory" && field_count > 0) {
          ++state.form_frames_detected;
        }
        if (state.command == "inventory") {
          FormCommandState::FramePayload frame_payload;
          frame_payload.frame_identifier = frame->GetIdentifier().ToString();
          frame_payload.payload = payload;
          frame_payload.is_main = frame->IsMain();
          const auto order = std::find(state.frame_dispatch_order.begin(),
                                       state.frame_dispatch_order.end(),
                                       frame_payload.frame_identifier);
          frame_payload.dispatch_order =
              order == state.frame_dispatch_order.end()
                  ? static_cast<int>(state.frame_dispatch_order.size())
                  : static_cast<int>(
                        std::distance(state.frame_dispatch_order.begin(), order));
          for (auto parent = frame->GetParent(); parent;
               parent = parent->GetParent()) {
            ++frame_payload.depth;
          }
          state.frame_payloads.push_back(std::move(frame_payload));
        }
        const bool better =
            state.successful_responses == 0 ||
            field_count > state.best_field_count ||
            (field_count == state.best_field_count &&
             eligible_count > state.best_eligible_count) ||
            (field_count == state.best_field_count &&
             eligible_count == state.best_eligible_count &&
             frame->IsMain() && !state.best_frame_is_main);
        if (better) {
          state.payload = payload;
          state.best_field_count = field_count;
          state.best_eligible_count = eligible_count;
          state.best_frame_identifier = frame->GetIdentifier().ToString();
          state.best_frame_is_main = frame->IsMain();
        }
        ++state.successful_responses;
      } else if (state.error.empty()) {
        state.error = response_ok && !bounded
                          ? "renderer form response was too large"
                          : (!response_ok && bounded && !payload.empty()
                                 ? payload
                                 : "fixed renderer form command failed");
      }

      if (state.received_responses >= state.expected_responses) {
        if (state.command == "inventory") {
          FinalizeFormInventoryLocked(state);
        } else {
          state.done = true;
          state.ok = state.successful_responses > 0;
        }
        if (!state.ok && state.error.empty()) {
          state.error = "all renderer frame form commands failed";
        }

        if (state.ok &&
            (state.command == "execute" ||
             state.command == "protected_fill" ||
             state.command == "reveal_more")) {
          auto selected = CefParseJSON(state.payload, JSON_PARSER_RFC);
          const char* counter = state.command == "reveal_more"
                                    ? "changed_scrollers"
                                    : "write_attempted_count";
          if (selected && selected->GetType() == VTYPE_DICTIONARY &&
              selected->GetDictionary()->GetInt(counter) > 0) {
            ++page_revision_;
            ResetPageStateLocked(
                "page changed while form command was pending");
            refresh = true;
          }
        }
      }
    }
    form_cv_.notify_all();
    if (refresh && browser && browser->GetMainFrame()) {
      browser->GetMainFrame()->SendProcessMessage(
          PID_RENDERER,
          CefProcessMessage::Create("saccade.collector.refresh_v1"));
    }
    return true;
  }
  if (name == "saccade.renderer.ready_v1" && arguments->GetSize() == 1) {
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      collector_ready_ = true;
    }
    fact_cv_.notify_all();
    return true;
  }

  if (name == "saccade.renderer.layout_changed_v1" &&
      arguments->GetSize() == 4) {
    const double width = arguments->GetDouble(0);
    const double height = arguments->GetDouble(1);
    const double device_scale = arguments->GetDouble(2);
    const double renderer_epoch_ms = arguments->GetDouble(3);
    if (!std::isfinite(width) || !std::isfinite(height) ||
        !std::isfinite(device_scale) || !std::isfinite(renderer_epoch_ms) ||
        width <= 0 || height <= 0 || device_scale <= 0) {
      return true;
    }
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      ++layout_epoch_;
      ++page_revision_;
      last_layout_page_revision_ = page_revision_;
      if (browser_) {
        browser_metadata_[browser_->GetIdentifier()].page_revision =
            page_revision_;
      }
      // Layout-only revisions are common while reflex targets animate. Keep
      // already accepted native inputs and their receipts alive; navigation
      // and tab changes still use the full reset path below.
      const auto dispatched_actions = dispatched_actions_;
      const auto dispatched_action_facts = dispatched_action_facts_;
      auto pending_receipts = std::move(pending_receipts_);
      ResetPageStateLocked("layout changed while action was pending");
      dispatched_actions_ = dispatched_actions;
      dispatched_action_facts_ = dispatched_action_facts;
      pending_receipts_ = std::move(pending_receipts);
    }
    fact_cv_.notify_all();
    action_map_cv_.notify_all();
    receipt_cv_.notify_all();
    form_cv_.notify_all();
    screenshot_cv_.notify_all();
    return true;
  }

  if (name == "saccade.renderer.controls_reset_v1" &&
      arguments->GetSize() == 1) {
    std::lock_guard<std::mutex> lock(state_mutex_);
    controls_.clear();
    return true;
  }

  if (name == "saccade.renderer.collector_error_v1" &&
      arguments->GetSize() == 2) {
    const std::string stage = arguments->GetString(0).ToString();
    if (!stage.empty() && stage.size() <= 64) {
      std::lock_guard<std::mutex> lock(state_mutex_);
      collector_error_ = stage;
    }
    return true;
  }

  if (name == "saccade.renderer.control_v1" && arguments->GetSize() == 5) {
    ControlFact fact;
    fact.fact_id = arguments->GetString(0).ToString();
    fact.kind = arguments->GetString(1).ToString();
    fact.sensitive = arguments->GetBool(2);
    fact.complete = arguments->GetBool(3);
    if (fact.fact_id.empty() || fact.fact_id.size() > 128 ||
        fact.kind.empty() || fact.kind.size() > 64) {
      return true;
    }
    std::lock_guard<std::mutex> lock(state_mutex_);
    for (auto& current : controls_) {
      if (current.fact_id == fact.fact_id) {
        current = std::move(fact);
        return true;
      }
    }
    if (controls_.size() < 256) {
      controls_.push_back(std::move(fact));
    }
    return true;
  }

  if (name == "saccade.renderer.actions_begin_v1" &&
      arguments->GetSize() == 2) {
    const int generation = arguments->GetInt(0);
    if (generation <= 0) {
      return true;
    }
    std::lock_guard<std::mutex> lock(state_mutex_);
    action_scan_generation_ = generation;
    staged_actions_.clear();
    return true;
  }

  if (name == "saccade.renderer.action_v1" && arguments->GetSize() == 11) {
    TargetFact fact;
    fact.action_id = arguments->GetString(0).ToString();
    fact.role = arguments->GetString(1).ToString();
    fact.label = arguments->GetString(2).ToString();
    fact.left = arguments->GetDouble(3);
    fact.top = arguments->GetDouble(4);
    fact.width = arguments->GetDouble(5);
    fact.height = arguments->GetDouble(6);
    fact.renderer_epoch_ms = arguments->GetDouble(8);
    fact.opens_new_context = arguments->GetBool(7);
    fact.destination_url = arguments->GetString(10).ToString();
    const int generation = arguments->GetInt(9);
    if (fact.action_id.empty() || fact.action_id.size() > 128 ||
        (fact.role != "target" && fact.role != "button" &&
         fact.role != "link" && fact.role != "surface") ||
        fact.label.size() > 128 || fact.destination_url.size() > 2048 ||
        !std::isfinite(fact.left) || !std::isfinite(fact.top) ||
        !std::isfinite(fact.width) || !std::isfinite(fact.height) ||
        !std::isfinite(fact.renderer_epoch_ms) || fact.width <= 0 ||
        fact.height <= 0 || fact.width > 4096 || fact.height > 4096 ||
        std::abs(fact.left) > 100000 || std::abs(fact.top) > 100000) {
      return true;
    }
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      if (generation != action_scan_generation_) {
        return true;
      }
      fact.page_revision = page_revision_;
      fact.layout_epoch = layout_epoch_;
      if (staged_actions_.size() < 256) {
        staged_actions_[fact.action_id] = fact;
      }
    }
    return true;
  }

  if (name == "saccade.renderer.actions_end_v1" &&
      arguments->GetSize() == 2) {
    const int generation = arguments->GetInt(0);
    bool added = false;
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      if (generation != action_scan_generation_) {
        return true;
      }
      for (const auto& [action_id, fact] : staged_actions_) {
        if (actions_.find(action_id) == actions_.end()) {
          if (pending_facts_.size() >= 256) {
            pending_facts_.pop_front();
          }
          pending_facts_.push_back(fact);
          added = true;
        }
      }
      actions_.swap(staged_actions_);
      staged_actions_.clear();
      ++action_map_serial_;
    }
    action_map_cv_.notify_all();
    if (added) {
      fact_cv_.notify_one();
    }
    return true;
  }

  if (name == "saccade.renderer.receipt_v1" &&
      arguments->GetSize() == 7) {
    ReflexReceipt receipt;
    receipt.action_id = arguments->GetString(0).ToString();
    receipt.client_x = arguments->GetDouble(1);
    receipt.client_y = arguments->GetDouble(2);
    receipt.hits = arguments->GetInt(3);
    receipt.misses = arguments->GetInt(4);
    receipt.finished = arguments->GetBool(5);
    receipt.renderer_epoch_ms = arguments->GetDouble(6);
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      const auto action = dispatched_action_facts_.find(receipt.action_id);
      if (action == dispatched_action_facts_.end() ||
          !std::isfinite(receipt.client_x) ||
          !std::isfinite(receipt.client_y) ||
          !std::isfinite(receipt.renderer_epoch_ms) ||
          dispatched_actions_.erase(receipt.action_id) != 1) {
        return true;
      }
      receipt.basis_page_revision = action->second.page_revision;
      receipt.observed_page_revision = page_revision_;
      receipt.basis_layout_epoch = action->second.layout_epoch;
      receipt.observed_layout_epoch = layout_epoch_;
      dispatched_action_facts_.erase(action);
      if (pending_receipts_.size() >= 256) {
        pending_receipts_.pop_front();
      }
      pending_receipts_.push_back(receipt);
    }
    receipt_cv_.notify_one();
    return true;
  }

  return true;
}

void SaccadeAdapter::OnBrowserClosed(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();
  bool stop = false;
  bool refresh_grant = false;
  CefRefPtr<CefFrame> next_frame;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    browsers_.erase(browser->GetIdentifier());
    browser_roles_.erase(browser->GetIdentifier());
    browser_metadata_.erase(browser->GetIdentifier());
    human_verification_failures_.erase(browser->GetIdentifier());
    agent_granted_browser_ids_.erase(browser->GetIdentifier());
    agent_created_browser_ids_.erase(browser->GetIdentifier());
    agent_paused_browser_ids_.erase(browser->GetIdentifier());
    if (browser_ && browser_->IsSame(browser)) {
      browser_ = browsers_.empty() ? nullptr : browsers_.begin()->second;
      next_frame = browser_ ? browser_->GetMainFrame() : nullptr;
      const int next_id = browser_ ? browser_->GetIdentifier() : 0;
      auto metadata = browser_metadata_.find(next_id);
      current_url_ = metadata == browser_metadata_.end()
                         ? (next_frame ? next_frame->GetURL().ToString() : "")
                         : metadata->second.url;
      current_title_ =
          metadata == browser_metadata_.end() ? "" : metadata->second.title;
      ++page_revision_;
      if (metadata != browser_metadata_.end()) {
        metadata->second.page_revision = page_revision_;
      }
      ResetPageStateLocked("visible tab closed while command was pending");
      refresh_grant = started_ && browser_;
    }
    stop = browsers_.empty();
  }
  fact_cv_.notify_all();
  action_map_cv_.notify_all();
  receipt_cv_.notify_all();
  form_cv_.notify_all();
  screenshot_cv_.notify_all();
  if (refresh_grant) {
    WriteGrant();
  }
  if (!stop) {
    RefreshAgentSwitchOnUi();
  }
  if (next_frame) {
    next_frame->SendProcessMessage(
        PID_RENDERER,
        CefProcessMessage::Create("saccade.collector.refresh_v1"));
  }
  if (stop) {
    Stop();
  }
}

void SaccadeAdapter::ConfigureIfRequested() {
  const char* socket_path = getenv("SACCADE_ENGINE_SOCKET");
  const char* grant_path = getenv("SACCADE_ENGINE_GRANT_PATH");
  if (!socket_path || !grant_path) {
    return;
  }
  bool enable_broker = false;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!configured_) {
      socket_path_ = socket_path;
      grant_path_ = grant_path;
      const char* replay_path = getenv("SACCADE_ENGINE_REPLAY_PATH");
      replay_path_ = replay_path && replay_path[0] != '\0'
                         ? replay_path
                         : grant_path_ + ".replay.jsonl";
      const char* current_pointer = getenv("SACCADE_ENGINE_CURRENT_POINTER");
      current_pointer_path_ =
          current_pointer && current_pointer[0] != '\0' ? current_pointer : "";
      configured_ = true;
    }
    const char* broker = getenv("SACCADE_ENGINE_BROKER");
    const char* legacy_grant = getenv("SACCADE_ENGINE_GRANT_CURRENT_TAB");
    enable_broker = (broker && std::string(broker) == "1") ||
                    (legacy_grant && std::string(legacy_grant) == "1");
    // The legacy flag grants only the first browser that configures the
    // bridge. Reapplying it from later OnBrowserCreated calls would silently
    // turn Human-created tabs On, including native Help tabs.
    if (!started_ && legacy_grant && std::string(legacy_grant) == "1" &&
        browser_) {
      agent_granted_browser_ids_.insert(browser_->GetIdentifier());
      agent_paused_browser_ids_.erase(browser_->GetIdentifier());
    }
  }
  if (enable_broker) {
    StartBridge();
  }
}

void SaccadeAdapter::StartBridge() {
  std::lock_guard<std::mutex> lock(state_mutex_);
  if (!configured_ || started_ || !browser_) {
    return;
  }
  capability_ = RandomCapability();
  if (!capability_.empty()) {
    started_ = true;
    stopping_ = false;
    server_thread_ = std::thread(&SaccadeAdapter::Serve, this);
  }
}

SaccadeAdapter::AgentUiState SaccadeAdapter::ToggleAgentForVisibleTab() {
  CEF_REQUIRE_UI_THREAD();
  bool enabled = false;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!configured_ || !browser_) {
      return AgentUiState::kUnavailable;
    }
    if (!started_) {
      return AgentUiState::kUnavailable;
    }
    const int browser_id = browser_->GetIdentifier();
    if (agent_granted_browser_ids_.erase(browser_id) == 0) {
      agent_granted_browser_ids_.insert(browser_id);
      agent_paused_browser_ids_.erase(browser_id);
      enabled = true;
    } else {
      agent_paused_browser_ids_.erase(browser_id);
      ResetPageStateLocked("human disabled Agent access for this tab");
    }
  }
  WriteGrant();
  RefreshAgentSwitchOnUi();
  if (enabled) {
    RefreshCollectorOnUi();
  }
  return GetAgentUiState();
}

SaccadeAdapter::AgentUiState SaccadeAdapter::GetAgentUiState() {
  std::lock_guard<std::mutex> lock(state_mutex_);
  if (!configured_) {
    return getenv("SACCADE_ENGINE_SOCKET") &&
                   getenv("SACCADE_ENGINE_GRANT_PATH")
               ? AgentUiState::kOff
               : AgentUiState::kUnavailable;
  }
  if (!browser_) {
    return AgentUiState::kUnavailable;
  }
  if (!started_) {
    return AgentUiState::kOff;
  }
  if (!CurrentTabGrantedLocked()) {
    return AgentUiState::kOff;
  }
  return CurrentTabPausedLocked() ? AgentUiState::kPaused : AgentUiState::kOn;
}

void SaccadeAdapter::Stop() {
  if (!started_) {
    return;
  }
  stopping_ = true;
  fact_cv_.notify_all();
  action_map_cv_.notify_all();
  receipt_cv_.notify_all();
  form_cv_.notify_all();
  screenshot_cv_.notify_all();
  text_insert_cv_.notify_all();
  const int listener = listener_fd_.exchange(-1);
  if (listener >= 0) {
    shutdown(listener, SHUT_RDWR);
    close(listener);
  }
  if (server_thread_.joinable() &&
      server_thread_.get_id() != std::this_thread::get_id()) {
    server_thread_.join();
  }
  if (!socket_path_.empty()) {
    unlink(socket_path_.c_str());
  }
  if (!grant_path_.empty()) {
    unlink(grant_path_.c_str());
    unlink((grant_path_ + ".tmp").c_str());
  }
  RemoveCurrentPointerIfOwned();
  started_ = false;
}

void SaccadeAdapter::Serve() {
  unlink(socket_path_.c_str());
  const int listener = socket(AF_UNIX, SOCK_STREAM, 0);
  if (listener < 0) {
    return;
  }
  listener_fd_ = listener;

  sockaddr_un address{};
  address.sun_family = AF_UNIX;
  if (socket_path_.size() >= sizeof(address.sun_path)) {
    close(listener);
    listener_fd_ = -1;
    return;
  }
  std::strncpy(address.sun_path, socket_path_.c_str(), sizeof(address.sun_path) - 1);
  if (bind(listener, reinterpret_cast<sockaddr*>(&address), sizeof(address)) != 0 ||
      chmod(socket_path_.c_str(), 0600) != 0 || listen(listener, 8) != 0) {
    close(listener);
    listener_fd_ = -1;
    unlink(socket_path_.c_str());
    return;
  }
  // Publish broker availability even when the first navigation has not yet
  // committed. An Off broker contains no URL or tab identity and is not a read
  // grant; publishing it early lets an LLM reuse the running app safely.
  if (!WriteGrant()) {
    close(listener);
    listener_fd_ = -1;
    unlink(socket_path_.c_str());
    return;
  }

  while (!stopping_) {
    const int client = accept(listener, nullptr, nullptr);
    if (client < 0) {
      if (errno == EINTR) {
        continue;
      }
      break;
    }
    std::string line;
    std::array<char, 4096> buffer{};
    while (line.size() < 64 * 1024) {
      const ssize_t count = read(client, buffer.data(), buffer.size());
      if (count < 0 && errno == EINTR) {
        continue;
      }
      if (count <= 0) {
        break;
      }
      line.append(buffer.data(), static_cast<size_t>(count));
      const size_t newline = line.find('\n');
      if (newline != std::string::npos) {
        line.resize(newline);
        break;
      }
    }
    std::string response = HandleRequest(line);
    response.push_back('\n');
    WriteAll(client, response);
    close(client);
  }
}

std::string SaccadeAdapter::HandleRequest(const std::string& line) {
  CefRefPtr<CefValue> parsed = CefParseJSON(line, JSON_PARSER_RFC);
  if (!parsed || parsed->GetType() != VTYPE_DICTIONARY) {
    return ErrorResponse(0, "INVALID_ARGUMENT", "request must be a JSON object");
  }
  auto request = parsed->GetDictionary();
  const int id = request->HasKey("id") ? request->GetInt("id") : 0;
  const std::string capability = request->GetString("capability").ToString();
  if (!ConstantTimeEqual(capability, capability_)) {
    return ErrorResponse(id, "PERMISSION_DENIED", "invalid session capability");
  }
  const std::string method = request->GetString("method").ToString();

  if (method == "ping") {
    auto result = CefDictionaryValue::Create();
    result->SetString("runtime", "saccade-engine-adapter-v1");
    result->SetString("contract_version", kContractVersion);
    result->SetString("protocol", kProtocol);
    result->SetList("capabilities", CapabilityList());
    return Response(id, result);
  }
  if (method == "shell_status") {
    CefRefPtr<CefValue> status = CefParseJSON(StatusJson(), JSON_PARSER_RFC);
    return Response(id, status->GetDictionary());
  }
  if (method == "tab_registry") {
    CefRefPtr<CefValue> registry =
        CefParseJSON(TabRegistryJson(), JSON_PARSER_RFC);
    return Response(id, registry->GetDictionary());
  }
  if (method == "select_tab") {
    return SelectTabResponse(id, request->GetDictionary("params"));
  }
  if (method == "open_agent_tab") {
    auto params = request->GetDictionary("params");
    const std::string url = params ? params->GetString("url").ToString() : "";
    CefURLParts parts;
    if (url.empty() || !CefParseURL(url, parts)) {
      return ErrorResponse(id, "INVALID_ARGUMENT",
                           "open_agent_tab requires an absolute URL");
    }
    const std::string scheme = CefString(&parts.scheme).ToString();
    if (scheme != "http" && scheme != "https" && scheme != "file") {
      return ErrorResponse(id, "INVALID_ARGUMENT",
                           "open_agent_tab allows only http, https, or file URLs");
    }
    CefPostTask(TID_UI, base::BindOnce(&SaccadeAdapter::OpenAgentTabOnUi,
                                      base::Unretained(this), url));
    auto result = CefDictionaryValue::Create();
    result->SetBool("opening", true);
    result->SetString("url", url);
    result->SetString("initial_agent_state", "on");
    return Response(id, result);
  }
  if (method == "toolbar_agent_state") {
    auto result = CefDictionaryValue::Create();
    result->SetString("state", AgentUiStateName(GetAgentUiState()));
    result->SetString("scope", "visible_tab");
    return Response(id, result);
  }
  if (method == "toolbar_toggle_agent") {
    auto pending = std::make_shared<ToolbarAgentRequest>();
    if (!CefPostTask(
            TID_UI,
            base::BindOnce(
                [](SaccadeAdapter* adapter,
                   std::shared_ptr<ToolbarAgentRequest> request) {
                  const auto state = adapter->ToggleAgentForVisibleTab();
                  {
                    std::lock_guard<std::mutex> lock(request->mutex);
                    request->state = state;
                    request->done = true;
                  }
                  request->ready.notify_one();
                },
                base::Unretained(this), pending))) {
      return ErrorResponse(id, "UNAVAILABLE",
                           "could not dispatch Agent toggle to browser UI");
    }
    std::unique_lock<std::mutex> lock(pending->mutex);
    if (!pending->ready.wait_for(lock, std::chrono::seconds(3),
                                 [&pending] { return pending->done; })) {
      return ErrorResponse(id, "TIMEOUT", "Agent toggle timed out");
    }
    auto result = CefDictionaryValue::Create();
    result->SetString("state", AgentUiStateName(pending->state));
    result->SetString("scope", "visible_tab");
    return Response(id, result);
  }
  if (method == "resume") {
    auto result = CefDictionaryValue::Create();
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      if (!CurrentTabGrantedLocked()) {
        return ErrorResponse(id, "CONSENT_REQUIRED", "Agent access is Off");
      }
      if (browser_) {
        agent_paused_browser_ids_.erase(browser_->GetIdentifier());
      }
      result->SetBool("agent_enabled", true);
      result->SetBool("paused", false);
      result->SetString("agent_activity", "idle");
      result->SetDouble("page_revision", static_cast<double>(page_revision_));
    }
    WriteGrant();
    CefPostTask(TID_UI, base::BindOnce(&SaccadeAdapter::RefreshAgentSwitchOnUi,
                                      base::Unretained(this)));
    CefPostTask(TID_UI, base::BindOnce(&SaccadeAdapter::RefreshCollectorOnUi,
                                      base::Unretained(this)));
    return Response(id, result);
  }
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!CurrentTabGrantedLocked()) {
      return ErrorResponse(
          id, "CONSENT_REQUIRED",
          "Agent access is Off for the visible tab; the user must turn it On");
    }
    if (CurrentTabPausedLocked()) {
      return ErrorResponse(id, "AGENT_PAUSED", "agent runtime is paused");
    }
  }
  if (method == "truth") {
    return Response(id, TruthResult());
  }
  if (method == "actions") {
    if (!RefreshActionMap(2000)) {
      return ErrorResponse(id, "TIMEOUT",
                           "live action-map refresh timed out");
    }
    return Response(id, ActionsResult());
  }
  if (method == "downloads") {
    CefRefPtr<CefValue> downloads =
        CefParseJSON(DownloadsJson(), JSON_PARSER_RFC);
    return Response(id, downloads->GetDictionary());
  }
  if (method == "next_fact") {
    return NextFactResponse(id, RequestTimeoutMs(request->GetDictionary("params")));
  }
  if (method == "next_receipt") {
    return NextReceiptResponse(
        id, RequestTimeoutMs(request->GetDictionary("params")));
  }
  if (method == "act") {
    return ActResponse(id, request->GetDictionary("params"));
  }
  if (method == "act_drag") {
    return DragResponse(id, request->GetDictionary("params"));
  }
  if (method == "form_inventory") {
    return FormCommandResponse(id, "inventory",
                               request->GetDictionary("params"));
  }
  if (method == "render_preflight") {
    return FormCommandResponse(id, "render_preflight",
                               request->GetDictionary("params"));
  }
  if (method == "inspect_fields") {
    return FormInspectFieldsResponse(id, request->GetDictionary("params"));
  }
  if (method == "form_compile_plan") {
    return FormCompilePlanResponse(id, request->GetDictionary("params"));
  }
  if (method == "form_execute_plan") {
    return FormExecutePlanResponse(id, request->GetDictionary("params"));
  }
  if (method == "type_field_text") {
    return TypeFieldTextResponse(id, request->GetDictionary("params"));
  }
  if (method == "form_reveal_more") {
    return FormCommandResponse(id, "reveal_more",
                               request->GetDictionary("params"));
  }
  if (method == "screenshot_policy") {
    return FormCommandResponse(id, "screenshot_policy",
                               request->GetDictionary("params"));
  }
  if (method == "screenshot_audit") {
    return ScreenshotAuditResponse(id, request->GetDictionary("params"));
  }
  if (method == "article_text") {
    return FormCommandResponse(id, "article_text",
                               request->GetDictionary("params"));
  }
  if (method == "protected_fill") {
    return ProtectedFillResponse(id, request->GetDictionary("params"));
  }
  if (method == "reflex_start") {
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      if (!CurrentTabGrantedLocked()) {
        return ErrorResponse(id, "CONSENT_REQUIRED", "Agent access is Off");
      }
      if (!collector_ready_) {
        return ErrorResponse(id, "TRANSPORT_UNAVAILABLE",
                             "renderer collector is not ready");
      }
    }
    CefPostTask(TID_UI, base::BindOnce(&SaccadeAdapter::StartReflexOnUi,
                                      base::Unretained(this)));
    auto result = CefDictionaryValue::Create();
    result->SetBool("started", true);
    return Response(id, result);
  }
  if (method == "navigate") {
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      if (!CurrentTabGrantedLocked()) {
        return ErrorResponse(id, "CONSENT_REQUIRED", "Agent access is Off");
      }
    }
    auto params = request->GetDictionary("params");
    const std::string url = params ? params->GetString("url").ToString() : "";
    if (url.empty()) {
      return ErrorResponse(id, "INVALID_ARGUMENT", "navigate requires url");
    }
    // The adapter is a process-lifetime singleton, so this unretained task
    // receiver cannot expire before the CEF UI thread.
    CefPostTask(TID_UI, base::BindOnce(&SaccadeAdapter::NavigateOnUi,
                                      base::Unretained(this), url));
    auto result = CefDictionaryValue::Create();
    result->SetBool("changed", true);
    result->SetString("url", url);
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      result->SetString("title", current_title_);
      result->SetDouble("page_revision", static_cast<double>(page_revision_ + 1));
    }
    return Response(id, result);
  }
  if (method == "back" || method == "forward" || method == "reload") {
    bool changed = false;
    uint64_t next_revision = 0;
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      if (!browser_) {
        return ErrorResponse(id, "TRANSPORT_UNAVAILABLE",
                             "visible browser is unavailable");
      }
      changed = method == "reload" ||
                (method == "back" && browser_->CanGoBack()) ||
                (method == "forward" && browser_->CanGoForward());
      if (changed) {
        ++page_revision_;
        browser_metadata_[browser_->GetIdentifier()].page_revision =
            page_revision_;
        ResetPageStateLocked("browser navigation changed the page");
      }
      next_revision = page_revision_;
    }
    if (changed) {
      CefPostTask(TID_UI,
                  base::BindOnce(&SaccadeAdapter::NavigateHistoryOnUi,
                                 base::Unretained(this), method));
    }
    auto result = CefDictionaryValue::Create();
    result->SetBool("changed", changed);
    result->SetString("action", method);
    result->SetDouble("page_revision", static_cast<double>(next_revision));
    return Response(id, result);
  }
  if (method == "pause") {
    auto result = CefDictionaryValue::Create();
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      if (browser_) {
        agent_paused_browser_ids_.insert(browser_->GetIdentifier());
      }
      ResetPageStateLocked("agent runtime paused");
      result->SetBool("agent_enabled", CurrentTabGrantedLocked());
      result->SetBool("paused", true);
      result->SetString("agent_activity", "paused");
      result->SetDouble("page_revision", static_cast<double>(page_revision_));
    }
    WriteGrant();
    CefPostTask(TID_UI, base::BindOnce(&SaccadeAdapter::RefreshAgentSwitchOnUi,
                                      base::Unretained(this)));
    return Response(id, result);
  }
  if (method == "close") {
    auto result = CefDictionaryValue::Create();
    result->SetBool("closing", true);
    CefPostTask(TID_UI, base::BindOnce(&SaccadeAdapter::CloseOnUi,
                                      base::Unretained(this)));
    return Response(id, result);
  }
  return ErrorResponse(id, "UNSUPPORTED_CAPABILITY",
                       "method is not advertised by this adapter");
}

std::string SaccadeAdapter::StatusJson() {
  auto result = CefDictionaryValue::Create();
  std::lock_guard<std::mutex> lock(state_mutex_);
  const bool granted = CurrentTabGrantedLocked();
  const bool paused = CurrentTabPausedLocked();
  result->SetString("url", granted ? current_url_ : "");
  result->SetString("title", granted ? current_title_ : "");
  result->SetDouble("page_revision", static_cast<double>(page_revision_));
  result->SetDouble("layout_epoch", static_cast<double>(layout_epoch_));
  result->SetString(
      "revision_cause",
      last_layout_page_revision_ == page_revision_ ? "layout" : "page");
  result->SetBool("paused", !granted || paused);
  result->SetBool("agent_enabled", granted);
  result->SetString("agent_activity",
                    !granted ? "disconnected" : (paused ? "paused" : "idle"));
  result->SetBool("collector_ready", granted && collector_ready_);
  result->SetString("collector_error", granted ? collector_error_ : "");
  result->SetString("tab_identity", granted ? CurrentTabIdLocked() : "");
  result->SetInt("browser_count", static_cast<int>(browsers_.size()));
  int popup_count = 0;
  for (const auto& [browser_id, role] : browser_roles_) {
    if (role.is_popup) {
      ++popup_count;
    }
  }
  result->SetInt("popup_count", popup_count);
  const int current_id = browser_ ? browser_->GetIdentifier() : 0;
  const auto verification_failure =
      human_verification_failures_.find(current_id);
  const bool verification_required =
      granted && verification_failure != human_verification_failures_.end();
  result->SetBool("human_verification_required", verification_required);
  result->SetBool("human_verification_retryable", verification_required);
  result->SetBool("human_verification_content_exposed", false);
  if (verification_required) {
    result->SetString("human_verification_provider",
                      verification_failure->second.provider);
    result->SetInt("human_verification_http_status",
                   verification_failure->second.http_status);
    result->SetInt("human_verification_request_status",
                   verification_failure->second.request_status);
  }
  const auto current_role = browser_roles_.find(current_id);
  result->SetBool("current_is_popup",
                  current_role != browser_roles_.end() &&
                      current_role->second.is_popup);
  result->SetInt("current_opener_id",
                 current_role == browser_roles_.end()
                     ? 0
                     : current_role->second.opener_id);
  auto value = CefValue::Create();
  value->SetDictionary(result);
  return JsonString(value);
}

std::string SaccadeAdapter::TabRegistryJson() {
  auto result = CefDictionaryValue::Create();
  auto tabs = CefListValue::Create();
  std::lock_guard<std::mutex> lock(state_mutex_);
  const int current_id = browser_ ? browser_->GetIdentifier() : 0;
  int index = 0;
  for (const int browser_id : agent_granted_browser_ids_) {
    const auto browser = browsers_.find(browser_id);
    if (browser == browsers_.end()) {
      continue;
    }
    const auto role = browser_roles_.find(browser_id);
    if (role != browser_roles_.end() && role->second.is_popup) {
      continue;
    }
    const auto metadata = browser_metadata_.find(browser_id);
    const std::string url =
        metadata == browser_metadata_.end() ? "" : metadata->second.url;
    const std::string title =
        metadata == browser_metadata_.end() ? "" : metadata->second.title;
    const uint64_t revision =
        metadata == browser_metadata_.end() ? 1 : metadata->second.page_revision;
    auto tab = CefDictionaryValue::Create();
    tab->SetString("browser_tab_id", "cef:" + std::to_string(browser_id));
    tab->SetString("owner",
                   agent_created_browser_ids_.find(browser_id) ==
                           agent_created_browser_ids_.end()
                       ? "human"
                       : "agent");
    tab->SetBool("agent_enabled", true);
    const bool paused =
        agent_paused_browser_ids_.find(browser_id) != agent_paused_browser_ids_.end();
    tab->SetBool("paused", paused);
    tab->SetString("agent_activity", paused ? "paused" : "idle");
    tab->SetBool("active", browser_id == current_id);
    tab->SetString("title", title);
    tab->SetString("origin", TrustedOrigin(url));
    tab->SetDouble("page_revision", static_cast<double>(revision));
    tab->SetBool("is_popup", false);
    tabs->SetSize(index + 1);
    tabs->SetDictionary(index++, tab);
  }
  result->SetString("status", "ok");
  result->SetString("summary",
                    "safe registry of Agent On tabs; Agent Off tabs omitted");
  result->SetInt("browser_count", static_cast<int>(browsers_.size()));
  result->SetInt("eligible_count", index);
  result->SetList("tabs", tabs);
  result->SetBool("agent_off_tabs_omitted", true);
  result->SetBool("capabilities_exposed", false);
  result->SetBool("cookies_or_storage_exposed", false);
  auto value = CefValue::Create();
  value->SetDictionary(result);
  return JsonString(value);
}

std::string SaccadeAdapter::DownloadsJson() {
  auto result = CefDictionaryValue::Create();
  auto items = CefListValue::Create();
  std::lock_guard<std::mutex> lock(state_mutex_);
  const int current_id = browser_ ? browser_->GetIdentifier() : 0;
  int index = 0;
  for (const auto& [download_id, download] : downloads_) {
    if (download.browser_id != current_id || !download.agent_visible_at_start) {
      continue;
    }
    auto item = CefDictionaryValue::Create();
    item->SetDouble("download_id", static_cast<double>(download.id));
    item->SetString("file_name", download.file_name);
    item->SetString("mime_type", download.mime_type);
    item->SetString("source_origin", download.source_origin);
    item->SetString("status", download.status);
    item->SetInt("percent_complete", download.percent_complete);
    item->SetDouble("received_bytes",
                    static_cast<double>(download.received_bytes));
    item->SetDouble("total_bytes", static_cast<double>(download.total_bytes));
    item->SetInt("interrupt_reason", download.interrupt_reason);
    item->SetDouble("basis_page_revision",
                    static_cast<double>(download.page_revision));
    item->SetString("download_root", "browser_default");
    item->SetBool("full_path_exposed", false);
    item->SetBool("contents_exposed", false);
    item->SetBool("auto_executed", false);
    items->SetSize(index + 1);
    items->SetDictionary(index++, item);
  }
  result->SetString("status", "ok");
  result->SetString("summary",
                    std::to_string(index) +
                        " Agent-visible download receipt(s)");
  result->SetInt("download_count", index);
  result->SetList("downloads", items);
  result->SetBool("agent_off_downloads_omitted", true);
  result->SetBool("full_paths_exposed", false);
  result->SetBool("contents_exposed", false);
  result->SetBool("auto_execute_allowed", false);
  auto value = CefValue::Create();
  value->SetDictionary(result);
  return JsonString(value);
}

std::string SaccadeAdapter::SelectTabResponse(
    int id,
    CefRefPtr<CefDictionaryValue> params) {
  const std::string browser_tab_id =
      params ? params->GetString("browser_tab_id").ToString() : "";
  constexpr char kPrefix[] = "cef:";
  if (browser_tab_id.rfind(kPrefix, 0) != 0) {
    return ErrorResponse(id, "INVALID_ARGUMENT",
                         "select_tab requires browser_tab_id like cef:<id>");
  }
  char* end = nullptr;
  const long parsed =
      strtol(browser_tab_id.c_str() + strlen(kPrefix), &end, 10);
  if (!end || *end != '\0' || parsed <= 0 ||
      parsed > std::numeric_limits<int>::max()) {
    return ErrorResponse(id, "INVALID_ARGUMENT",
                         "select_tab received an invalid browser_tab_id");
  }
  const int browser_id = static_cast<int>(parsed);
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    const auto browser = browsers_.find(browser_id);
    if (browser == browsers_.end()) {
      return ErrorResponse(id, "TAB_NOT_FOUND", "tab is not available");
    }
    if (agent_granted_browser_ids_.find(browser_id) ==
        agent_granted_browser_ids_.end()) {
      return ErrorResponse(id, "CONSENT_REQUIRED",
                           "Agent access is Off for this tab");
    }
    const auto role = browser_roles_.find(browser_id);
    if (role != browser_roles_.end() && role->second.is_popup) {
      return ErrorResponse(id, "INVALID_ARGUMENT",
                           "popup windows are not attachable by tab registry");
    }
    browser_ = browser->second;
    agent_paused_browser_ids_.erase(browser_id);
    const auto metadata = browser_metadata_.find(browser_id);
    current_url_ = metadata == browser_metadata_.end() ? "" : metadata->second.url;
    current_title_ =
        metadata == browser_metadata_.end() ? "" : metadata->second.title;
    ++page_revision_;
    browser_metadata_[browser_id].page_revision = page_revision_;
    ResetPageStateLocked("Agent selected an eligible browser tab");
  }
  WriteGrant();
  CefPostTask(TID_UI, base::BindOnce(&SaccadeAdapter::RefreshAgentSwitchOnUi,
                                    base::Unretained(this)));
  CefPostTask(TID_UI, base::BindOnce(&SaccadeAdapter::RefreshCollectorOnUi,
                                    base::Unretained(this)));
  CefRefPtr<CefValue> status = CefParseJSON(StatusJson(), JSON_PARSER_RFC);
  return Response(id, status->GetDictionary());
}

CefRefPtr<CefDictionaryValue> SaccadeAdapter::TruthResult() {
  auto result = CefDictionaryValue::Create();
  auto fields = CefListValue::Create();
  std::lock_guard<std::mutex> lock(state_mutex_);
  result->SetString("tab_id", CurrentTabIdLocked());
  result->SetString("url", current_url_);
  result->SetDouble("page_revision", static_cast<double>(page_revision_));
  result->SetDouble("layout_epoch", static_cast<double>(layout_epoch_));
  result->SetBool("collector_ready", collector_ready_);
  result->SetString("collector_error", collector_error_);
  result->SetBool("sensitive_values_exposed", false);
  auto provenance = CefDictionaryValue::Create();
  provenance->SetString("page_title", "untrusted_page_content");
  provenance->SetString("page_text", "untrusted_page_content");
  provenance->SetString("action_labels", "untrusted_page_content");
  provenance->SetString("policy_and_side_effect_authorization",
                        "llm_host_policy");
  provenance->SetBool("page_content_may_authorize_actions", false);
  result->SetDictionary("provenance", provenance);
  fields->SetSize(controls_.size());
  for (size_t index = 0; index < controls_.size(); ++index) {
    auto field = CefDictionaryValue::Create();
    field->SetString("fact_id", controls_[index].fact_id);
    field->SetString("kind", controls_[index].kind);
    field->SetBool("sensitive", controls_[index].sensitive);
    field->SetBool("complete", controls_[index].complete);
    fields->SetDictionary(index, field);
  }
  result->SetList("fields", fields);
  return result;
}

CefRefPtr<CefDictionaryValue> SaccadeAdapter::ActionsResult() {
  auto result = CefDictionaryValue::Create();
  auto actions = CefListValue::Create();
  std::lock_guard<std::mutex> lock(state_mutex_);
  result->SetString("tab_id", CurrentTabIdLocked());
  result->SetDouble("page_revision", static_cast<double>(page_revision_));
  result->SetDouble("layout_epoch", static_cast<double>(layout_epoch_));
  result->SetString(
      "revision_cause",
      last_layout_page_revision_ == page_revision_ ? "layout" : "page");
  actions->SetSize(actions_.size());
  size_t index = 0;
  for (const auto& [action_id, fact] : actions_) {
    auto action = CefDictionaryValue::Create();
    action->SetString("action_id", action_id);
    action->SetString("kind",
                      fact.role == "surface" ? "pointer_drag"
                                             : "pointer_click");
    action->SetString("role", fact.role);
    action->SetString("label", fact.label);
    action->SetString("label_provenance", "untrusted_page_content");
    action->SetString("authorization_source", "llm_host_policy");
    action->SetBool("requires_user_confirmation", false);
    action->SetBool("opens_new_context", fact.opens_new_context);
    action->SetDouble("basis_page_revision",
                      static_cast<double>(fact.page_revision));
    action->SetDouble("basis_layout_epoch",
                      static_cast<double>(fact.layout_epoch));
    actions->SetDictionary(index++, action);
  }
  result->SetList("actions", actions);
  return result;
}

std::string SaccadeAdapter::NextFactResponse(int id, int timeout_ms) {
  std::unique_lock<std::mutex> lock(state_mutex_);
  if (!fact_cv_.wait_for(lock, std::chrono::milliseconds(timeout_ms), [this] {
        return stopping_ || !pending_facts_.empty();
      }) || pending_facts_.empty()) {
    return ErrorResponse(id, "TIMEOUT", "no renderer fact before timeout");
  }
  TargetFact fact = std::move(pending_facts_.front());
  pending_facts_.pop_front();
  lock.unlock();

  auto rect = CefDictionaryValue::Create();
  rect->SetDouble("left", fact.left);
  rect->SetDouble("top", fact.top);
  rect->SetDouble("width", fact.width);
  rect->SetDouble("height", fact.height);
  auto result = CefDictionaryValue::Create();
  result->SetString("fact_id", fact.action_id);
  result->SetString("kind", "semantic_object");
  result->SetString("role", fact.role);
  result->SetString("label", fact.label);
  result->SetString("action_id", fact.action_id);
  result->SetString("label_provenance", "untrusted_page_content");
  result->SetString("authorization_source", "llm_host_policy");
  result->SetBool("requires_user_confirmation", false);
  result->SetDouble("page_revision", static_cast<double>(fact.page_revision));
  result->SetDouble("layout_epoch", static_cast<double>(fact.layout_epoch));
  result->SetDouble("renderer_epoch_ms", fact.renderer_epoch_ms);
  result->SetDictionary("rect", rect);
  return Response(id, result);
}

std::string SaccadeAdapter::NextReceiptResponse(int id, int timeout_ms) {
  std::unique_lock<std::mutex> lock(state_mutex_);
  if (!receipt_cv_.wait_for(
          lock, std::chrono::milliseconds(timeout_ms),
          [this] { return stopping_ || !pending_receipts_.empty(); }) ||
      pending_receipts_.empty()) {
    return ErrorResponse(id, "TIMEOUT", "no input receipt before timeout");
  }
  ReflexReceipt receipt = std::move(pending_receipts_.front());
  pending_receipts_.pop_front();
  lock.unlock();

  auto result = CefDictionaryValue::Create();
  result->SetString("action_id", receipt.action_id);
  result->SetString("status", "applied");
  result->SetBool("verified", true);
  result->SetDouble("basis_page_revision",
                    static_cast<double>(receipt.basis_page_revision));
  result->SetDouble("observed_page_revision",
                    static_cast<double>(receipt.observed_page_revision));
  result->SetDouble("basis_layout_epoch",
                    static_cast<double>(receipt.basis_layout_epoch));
  result->SetDouble("observed_layout_epoch",
                    static_cast<double>(receipt.observed_layout_epoch));
  result->SetDouble("client_x", receipt.client_x);
  result->SetDouble("client_y", receipt.client_y);
  result->SetInt("hits", receipt.hits);
  result->SetInt("misses", receipt.misses);
  result->SetBool("finished", receipt.finished);
  result->SetDouble("renderer_epoch_ms", receipt.renderer_epoch_ms);
  result->SetBool("values_logged", false);
  AppendValueFreeReplay("pointer_applied", result,
                        receipt.basis_page_revision,
                        receipt.observed_page_revision);
  return Response(id, result);
}

std::string SaccadeAdapter::ActResponse(
    int id,
    CefRefPtr<CefDictionaryValue> params) {
  const std::string action_id =
      params ? params->GetString("action_id").ToString() : "";
  const uint64_t basis_page_revision = RequestRevision(params);
  const uint64_t basis_layout_epoch =
      params && params->HasKey("basis_layout_epoch")
          ? static_cast<uint64_t>(params->GetDouble("basis_layout_epoch"))
          : 0;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (CurrentTabHasHumanVerificationFailureLocked()) {
      return ErrorResponse(
          id, "PROVIDER_REJECTED",
          "human verification provider rejected the session; reload the page and let the user complete verification if shown");
    }
  }
  if (!RefreshActionMap(2000)) {
    return ErrorResponse(id, "TIMEOUT", "live action-map refresh timed out");
  }
  TargetFact fact;
  int browser_id = 0;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!CurrentTabGrantedLocked()) {
      return ErrorResponse(id, "PERMISSION_DENIED", "agent is paused");
    }
    if (basis_page_revision == 0 || basis_page_revision != page_revision_) {
      const bool layout_changed = last_layout_page_revision_ == page_revision_;
      return ErrorResponse(
          id, layout_changed ? "STALE_LAYOUT" : "STALE_PAGE_REVISION",
          layout_changed ? "layout changed after the action map was read"
                         : "action basis does not match current page");
    }
    if (basis_layout_epoch != 0 && basis_layout_epoch != layout_epoch_) {
      return ErrorResponse(id, "STALE_LAYOUT",
                           "action layout epoch does not match current layout");
    }
    const auto action = actions_.find(action_id);
    if (action == actions_.end() ||
        action->second.page_revision != basis_page_revision) {
      return ErrorResponse(id, "INVALID_ARGUMENT", "unknown action id");
    }
    fact = action->second;
    browser_id = browser_ ? browser_->GetIdentifier() : 0;
    if (browser_id == 0) {
      return ErrorResponse(id, "TRANSPORT_UNAVAILABLE",
                           "visible browser is unavailable");
    }
    if (!dispatched_actions_.insert(action_id).second) {
      return ErrorResponse(id, "INVALID_ARGUMENT",
                           "action id is already awaiting a receipt");
    }
    dispatched_action_facts_[action_id] = fact;
  }
  const int x = static_cast<int>(std::lround(fact.left + fact.width / 2.0));
  const int y = static_cast<int>(std::lround(fact.top + fact.height / 2.0));
  CefPostTask(TID_UI, base::BindOnce(&SaccadeAdapter::DispatchPointerOnUi,
                                    base::Unretained(this), x, y, action_id,
                                    browser_id, basis_page_revision,
                                    fact.layout_epoch, fact.role == "target"));
  auto result = CefDictionaryValue::Create();
  result->SetString("action_id", action_id);
  result->SetString("status", "accepted");
  result->SetBool("opens_new_context", fact.opens_new_context);
  result->SetDouble("basis_page_revision",
                    static_cast<double>(basis_page_revision));
  result->SetDouble("basis_layout_epoch",
                    static_cast<double>(fact.layout_epoch));
  return Response(id, result);
}

std::string SaccadeAdapter::DragResponse(
    int id,
    CefRefPtr<CefDictionaryValue> params) {
  const std::string action_id =
      params ? params->GetString("action_id").ToString() : "";
  const std::string direction =
      params ? params->GetString("direction").ToString() : "";
  if (direction != "north" && direction != "south" &&
      direction != "east" && direction != "west") {
    return ErrorResponse(id, "INVALID_ARGUMENT",
                         "drag direction must be north, south, east, or west");
  }
  const uint64_t basis_page_revision = RequestRevision(params);
  const uint64_t basis_layout_epoch =
      params && params->HasKey("basis_layout_epoch")
          ? static_cast<uint64_t>(params->GetDouble("basis_layout_epoch"))
          : 0;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (CurrentTabHasHumanVerificationFailureLocked()) {
      return ErrorResponse(
          id, "PROVIDER_REJECTED",
          "human verification provider rejected the session; reload the page and let the user complete verification if shown");
    }
  }
  if (!RefreshActionMap(2000)) {
    return ErrorResponse(id, "TIMEOUT", "live action-map refresh timed out");
  }
  TargetFact fact;
  int browser_id = 0;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!CurrentTabGrantedLocked()) {
      return ErrorResponse(id, "PERMISSION_DENIED", "agent is paused");
    }
    if (basis_page_revision == 0 || basis_page_revision != page_revision_) {
      const bool layout_changed = last_layout_page_revision_ == page_revision_;
      return ErrorResponse(
          id, layout_changed ? "STALE_LAYOUT" : "STALE_PAGE_REVISION",
          layout_changed ? "layout changed after the action map was read"
                         : "action basis does not match current page");
    }
    if (basis_layout_epoch != 0 && basis_layout_epoch != layout_epoch_) {
      return ErrorResponse(id, "STALE_LAYOUT",
                           "action layout epoch does not match current layout");
    }
    const auto action = actions_.find(action_id);
    if (action == actions_.end() ||
        action->second.page_revision != basis_page_revision) {
      return ErrorResponse(id, "INVALID_ARGUMENT", "unknown action id");
    }
    if (action->second.role != "surface") {
      return ErrorResponse(id, "INVALID_ARGUMENT",
                           "drag action requires a visible surface fact");
    }
    if (!dispatched_actions_.insert(action_id).second) {
      return ErrorResponse(id, "INVALID_ARGUMENT",
                           "action id is already awaiting a receipt");
    }
    fact = action->second;
    dispatched_action_facts_[action_id] = fact;
    browser_id = browser_ ? browser_->GetIdentifier() : 0;
    if (browser_id == 0) {
      dispatched_actions_.erase(action_id);
      dispatched_action_facts_.erase(action_id);
      return ErrorResponse(id, "TRANSPORT_UNAVAILABLE",
                           "visible browser is unavailable");
    }
  }

  const int start_x =
      static_cast<int>(std::lround(fact.left + fact.width / 2.0));
  const int start_y =
      static_cast<int>(std::lround(fact.top + fact.height / 2.0));
  int end_x = start_x;
  int end_y = start_y;
  if (direction == "north") {
    end_y -= static_cast<int>(std::lround(fact.height * 0.3));
  } else if (direction == "south") {
    end_y += static_cast<int>(std::lround(fact.height * 0.3));
  } else if (direction == "east") {
    end_x += static_cast<int>(std::lround(fact.width * 0.3));
  } else {
    end_x -= static_cast<int>(std::lround(fact.width * 0.3));
  }
  CefPostTask(TID_UI, base::BindOnce(&SaccadeAdapter::DispatchDragOnUi,
                                    base::Unretained(this), start_x, start_y,
                                    end_x, end_y, action_id, browser_id,
                                    basis_page_revision, fact.layout_epoch));
  auto result = CefDictionaryValue::Create();
  result->SetString("action_id", action_id);
  result->SetString("status", "accepted");
  result->SetString("direction", direction);
  result->SetDouble("basis_page_revision",
                    static_cast<double>(basis_page_revision));
  result->SetDouble("basis_layout_epoch",
                    static_cast<double>(fact.layout_epoch));
  return Response(id, result);
}

std::string SaccadeAdapter::FormCommandResponse(
    int id,
    const std::string& command,
    CefRefPtr<CefDictionaryValue> params) {
  return FormCommandResponseForFrame(id, command, params, "");
}

std::string SaccadeAdapter::FormCommandResponseForFrame(
    int id,
    const std::string& command,
    CefRefPtr<CefDictionaryValue> params,
    const std::string& frame_identifier) {
  if ((command == "compile" || command == "execute") &&
      !ValidAssignments(params)) {
    return ErrorResponse(id, "INVALID_ARGUMENT",
                         "form command requires scalar assignments");
  }
  if (command == "inspect" && !ValidInspectFields(params)) {
    return ErrorResponse(id, "INVALID_ARGUMENT",
                         "inspect field_ids must be a bounded string list");
  }
  if (command == "execute") {
    const std::string expected =
        params ? params->GetString("expected_plan_id").ToString() : "";
    if (expected.empty() || expected.size() > 128) {
      return ErrorResponse(id, "INVALID_ARGUMENT",
                           "execute requires expected_plan_id");
    }
  }

  std::string expected_surface = "page";
  if (command == "render_preflight") {
    expected_surface =
        params ? params->GetString("expected_surface").ToString() : "page";
    if (expected_surface.empty()) {
      expected_surface = "page";
    }
    if (!ValidExpectedSurface(expected_surface)) {
      return ErrorResponse(
          id, "INVALID_ARGUMENT",
          "render_preflight expected_surface must be page, github_issue, or github_discussion");
    }
  }

  const bool revision_required =
      command != "inventory" && command != "render_preflight";
  const uint64_t basis_page_revision = RequestRevision(params);
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!CurrentTabGrantedLocked()) {
      return ErrorResponse(id, "PERMISSION_DENIED", "agent is paused");
    }
    if (!collector_ready_) {
      return ErrorResponse(id, "TRANSPORT_UNAVAILABLE",
                           "renderer collector is not ready");
    }
    if (revision_required &&
        (basis_page_revision == 0 || basis_page_revision != page_revision_)) {
      return ErrorResponse(id, "STALE_PAGE_REVISION",
                           "form basis does not match current page");
    }
  }

  auto original_input = CefValue::Create();
  original_input->SetDictionary(params ? params->Copy(false)
                                       : CefDictionaryValue::Create());
  const std::string original_input_json = JsonString(original_input);
  auto renderer_params = params ? params->Copy(false)
                                : CefDictionaryValue::Create();
  if (command == "inventory") {
    renderer_params->SetString("mode", "full");
    renderer_params->SetInt("offset", 0);
    renderer_params->SetInt("limit", 500);
  }
  auto input = CefValue::Create();
  input->SetDictionary(renderer_params);
  const std::string input_json = JsonString(input);
  const int request_id = next_form_request_id_.fetch_add(1);
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    FormCommandState state;
    state.command = command;
    state.input_json = original_input_json;
    state.target_frame_identifier = frame_identifier;
    state.basis_page_revision = revision_required ? basis_page_revision
                                                  : page_revision_;
    form_commands_[request_id] = std::move(state);
  }
  CefPostTask(TID_UI,
              base::BindOnce(&SaccadeAdapter::DispatchFormCommandOnUi,
                             base::Unretained(this), request_id, command,
                             input_json));

  std::unique_lock<std::mutex> lock(state_mutex_);
  if (!form_cv_.wait_for(lock, std::chrono::seconds(5), [this, request_id] {
        const auto pending = form_commands_.find(request_id);
        return stopping_ ||
               (pending != form_commands_.end() && pending->second.done);
      })) {
    form_commands_.erase(request_id);
    return ErrorResponse(id, "TIMEOUT", "renderer form command timed out");
  }
  auto pending = form_commands_.find(request_id);
  if (pending == form_commands_.end()) {
    return ErrorResponse(id, "TRANSPORT_UNAVAILABLE",
                         "renderer form command disappeared");
  }
  FormCommandState state = std::move(pending->second);
  form_commands_.erase(pending);
  const uint64_t observed_revision = page_revision_;
  const std::string observed_url = current_url_;
  const std::string observed_title = current_title_;
  lock.unlock();
  if (!state.ok) {
    return ErrorResponse(id, "FORM_COMMAND_FAILED",
                         state.error.empty() ? "fixed form command failed"
                                             : state.error);
  }
  auto parsed = CefParseJSON(state.payload, JSON_PARSER_RFC);
  if (!parsed || parsed->GetType() != VTYPE_DICTIONARY) {
    return ErrorResponse(id, "FORM_COMMAND_FAILED",
                         "renderer returned invalid fixed form result");
  }
  auto result = parsed->GetDictionary();
  if (result->HasKey("fixed_command_error")) {
    return ErrorResponse(id, "FORM_COMMAND_FAILED",
                         result->GetString("fixed_command_error"));
  }
  result->SetDouble("basis_page_revision",
                    static_cast<double>(state.basis_page_revision));
  result->SetDouble("page_revision", static_cast<double>(observed_revision));
  result->SetBool("sensitive_values_exposed", false);
  if (command == "article_text") {
    result->SetString("source_url", observed_url);
    result->SetString("source_title", observed_title);
    auto provenance = CefDictionaryValue::Create();
    provenance->SetString("title", "untrusted_page_content");
    provenance->SetString("text", "untrusted_page_content");
    provenance->SetString("headings", "untrusted_page_content");
    provenance->SetBool("page_content_may_authorize_actions", false);
    result->SetDictionary("provenance", provenance);
  }
  if (command == "render_preflight") {
    const bool observation_consistent =
        state.basis_page_revision == observed_revision;
    const bool task_surface_match =
        TaskSurfaceMatches(expected_surface, observed_url);
    result->SetString("source_url", observed_url);
    result->SetString("source_title", observed_title);
    result->SetString("expected_surface", expected_surface);
    result->SetBool("task_surface_match", task_surface_match);
    auto observations = result->GetDictionary("observations");
    if (observations) {
      observations->SetDouble(
          "start_page_revision",
          static_cast<double>(state.basis_page_revision));
      observations->SetDouble("end_page_revision",
                              static_cast<double>(observed_revision));
      observations->SetBool("observation_base_consistent",
                            observation_consistent);
      observations->SetBool("task_surface_match", task_surface_match);
    }
    auto agreement = result->GetDictionary("agreement");
    if (agreement) {
      auto observation_base = agreement->GetDictionary("observation_base");
      if (observation_base) {
        observation_base->SetDouble(
            "start_page_revision",
            static_cast<double>(state.basis_page_revision));
        observation_base->SetDouble("end_page_revision",
                                    static_cast<double>(observed_revision));
        observation_base->SetBool("consistent", observation_consistent);
      }
    }
    if (!observation_consistent) {
      OverridePreflightRoute(
          result, "refresh_replan",
          "observation_base_changed_during_preflight",
          "AGREEMENT_REVISION_MISMATCH", false);
    } else if (!task_surface_match) {
      OverridePreflightRoute(result, "navigate_task_surface",
                             "expected_task_surface_url_mismatch",
                             "AGREEMENT_TASK_SURFACE_MISMATCH", true);
    }
  }
  AppendValueFreeReplay("form_" + command, result,
                        state.basis_page_revision, observed_revision);
  return Response(id, result);
}

std::string SaccadeAdapter::FormCompilePlanResponse(
    int id,
    CefRefPtr<CefDictionaryValue> params) {
  if (!ValidAssignments(params)) {
    return ErrorResponse(id, "INVALID_ARGUMENT",
                         "form compilation requires scalar assignments");
  }
  const uint64_t basis_page_revision = RequestRevision(params);
  auto assignments = params->GetDictionary("assignments");
  CefDictionaryValue::KeyList assignment_keys;
  assignments->GetKeys(assignment_keys);

  std::map<std::string, FormFieldRoute> routes;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!CurrentTabGrantedLocked()) {
      return ErrorResponse(id, "PERMISSION_DENIED", "agent is paused");
    }
    if (!collector_ready_) {
      return ErrorResponse(id, "TRANSPORT_UNAVAILABLE",
                           "renderer collector is not ready");
    }
    if (basis_page_revision == 0 || basis_page_revision != page_revision_) {
      return ErrorResponse(id, "STALE_PAGE_REVISION",
                           "form basis does not match current page");
    }
    if (form_field_routes_revision_ != basis_page_revision) {
      return ErrorResponse(id, "STALE_FORM_INVENTORY",
                           "refresh the unified form inventory before compiling");
    }
    routes = form_field_routes_;
  }

  std::map<std::string, CefRefPtr<CefDictionaryValue>> grouped_assignments;
  std::map<std::string, std::map<std::string, std::string>> public_by_local;
  std::vector<CefRefPtr<CefDictionaryValue>> rejected_entries;
  for (const auto& public_field_id : assignment_keys) {
    const auto route = routes.find(public_field_id.ToString());
    if (route == routes.end()) {
      auto rejected = CefDictionaryValue::Create();
      rejected->SetString("field_id", public_field_id);
      rejected->SetString("reason", "not_found");
      rejected_entries.push_back(rejected);
      continue;
    }
    auto& frame_assignments = grouped_assignments[route->second.frame_identifier];
    if (!frame_assignments) {
      frame_assignments = CefDictionaryValue::Create();
    }
    frame_assignments->SetValue(
        route->second.renderer_field_id,
        assignments->GetValue(public_field_id)->Copy());
    public_by_local[route->second.frame_identifier]
                   [route->second.renderer_field_id] =
        public_field_id.ToString();
  }

  std::vector<CefRefPtr<CefDictionaryValue>> eligible_entries;
  for (const auto& [frame_identifier, frame_assignments] : grouped_assignments) {
    auto frame_params = params->Copy(false);
    frame_params->SetDictionary("assignments", frame_assignments->Copy(false));
    const std::string response = FormCommandResponseForFrame(
        id, "compile", frame_params, frame_identifier);
    auto parsed = CefParseJSON(response, JSON_PARSER_RFC);
    auto root = parsed && parsed->GetType() == VTYPE_DICTIONARY
                    ? parsed->GetDictionary()
                    : nullptr;
    if (!root || !root->GetBool("ok")) {
      return root ? response
                  : ErrorResponse(id, "FORM_COMMAND_FAILED",
                                  "frame form compile returned invalid data");
    }
    auto frame_result = root->GetDictionary("result");
    const auto& reverse = public_by_local[frame_identifier];
    for (const char* list_name : {"eligible", "rejected"}) {
      auto list = frame_result ? frame_result->GetList(list_name) : nullptr;
      for (size_t index = 0; list && index < list->GetSize(); ++index) {
        auto source = list->GetDictionary(index);
        if (!source) {
          continue;
        }
        auto entry = source->Copy(false);
        const std::string local_id = entry->GetString("field_id").ToString();
        const auto public_id = reverse.find(local_id);
        if (public_id == reverse.end()) {
          continue;
        }
        entry->SetString("field_id", public_id->second);
        if (std::strcmp(list_name, "eligible") == 0) {
          eligible_entries.push_back(entry);
        } else {
          rejected_entries.push_back(entry);
        }
      }
    }
  }

  const auto by_field_id = [](const CefRefPtr<CefDictionaryValue>& left,
                              const CefRefPtr<CefDictionaryValue>& right) {
    return left->GetString("field_id").ToString() <
           right->GetString("field_id").ToString();
  };
  std::sort(eligible_entries.begin(), eligible_entries.end(), by_field_id);
  std::sort(rejected_entries.begin(), rejected_entries.end(), by_field_id);
  auto eligible = CefListValue::Create();
  std::string plan_basis;
  for (const auto& entry : eligible_entries) {
    const std::string field_id = entry->GetString("field_id").ToString();
    if (!plan_basis.empty()) {
      plan_basis.push_back('|');
    }
    plan_basis.append(field_id);
    eligible->SetDictionary(eligible->GetSize(), entry->Copy(false));
  }
  auto rejected = CefListValue::Create();
  for (const auto& entry : rejected_entries) {
    rejected->SetDictionary(rejected->GetSize(), entry->Copy(false));
  }

  auto result = CefDictionaryValue::Create();
  result->SetString("plan_id",
                    "form_plan_v2_" +
                        Fnv1aUtf16(CefString(plan_basis).ToString16()));
  result->SetList("eligible", eligible);
  result->SetList("rejected", rejected);
  result->SetDouble("basis_page_revision",
                    static_cast<double>(basis_page_revision));
  result->SetDouble("page_revision",
                    static_cast<double>(basis_page_revision));
  result->SetBool("frame_routing_compiled", true);
  result->SetBool("sensitive_values_exposed", false);
  AppendValueFreeReplay("form_compile_composited", result,
                        basis_page_revision, basis_page_revision);
  return Response(id, result);
}

std::string SaccadeAdapter::FormInspectFieldsResponse(
    int id,
    CefRefPtr<CefDictionaryValue> params) {
  if (!ValidInspectFields(params)) {
    return ErrorResponse(id, "INVALID_ARGUMENT",
                         "inspect field_ids must be a bounded string list");
  }
  const uint64_t basis_page_revision = RequestRevision(params);
  std::map<std::string, FormFieldRoute> routes;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (basis_page_revision == 0 || basis_page_revision != page_revision_) {
      return ErrorResponse(id, "STALE_PAGE_REVISION",
                           "form basis does not match current page");
    }
    if (form_field_routes_revision_ != basis_page_revision) {
      return ErrorResponse(id, "STALE_FORM_INVENTORY",
                           "refresh the unified form inventory before inspection");
    }
    routes = form_field_routes_;
  }

  std::vector<std::string> requested;
  auto requested_list = params
                            ? params->GetList(params->HasKey("field_ids")
                                                  ? "field_ids"
                                                  : "fields")
                            : nullptr;
  if (requested_list) {
    for (size_t index = 0; index < requested_list->GetSize(); ++index) {
      requested.push_back(requested_list->GetString(index).ToString());
    }
  } else {
    for (const auto& [field_id, route] : routes) {
      requested.push_back(field_id);
    }
  }

  std::map<std::string, CefRefPtr<CefListValue>> grouped_fields;
  std::map<std::string, std::map<std::string, std::string>> public_by_local;
  auto fields = CefListValue::Create();
  for (const auto& public_field_id : requested) {
    const auto route = routes.find(public_field_id);
    if (route == routes.end()) {
      auto missing = CefDictionaryValue::Create();
      missing->SetString("field_id", public_field_id);
      missing->SetString("status", "not_found");
      fields->SetDictionary(fields->GetSize(), missing);
      continue;
    }
    auto& local_fields = grouped_fields[route->second.frame_identifier];
    if (!local_fields) {
      local_fields = CefListValue::Create();
    }
    local_fields->SetString(local_fields->GetSize(),
                            route->second.renderer_field_id);
    public_by_local[route->second.frame_identifier]
                   [route->second.renderer_field_id] = public_field_id;
  }

  int sensitive_count = 0;
  for (const auto& [frame_identifier, local_fields] : grouped_fields) {
    auto frame_params = params->Copy(false);
    frame_params->SetList("field_ids", local_fields->Copy());
    const std::string response = FormCommandResponseForFrame(
        id, "inspect", frame_params, frame_identifier);
    auto parsed = CefParseJSON(response, JSON_PARSER_RFC);
    auto root = parsed && parsed->GetType() == VTYPE_DICTIONARY
                    ? parsed->GetDictionary()
                    : nullptr;
    if (!root || !root->GetBool("ok")) {
      return root ? response
                  : ErrorResponse(id, "FORM_COMMAND_FAILED",
                                  "frame form inspection returned invalid data");
    }
    auto frame_result = root->GetDictionary("result");
    sensitive_count += frame_result ? frame_result->GetInt("sensitive_count") : 0;
    auto frame_fields = frame_result ? frame_result->GetList("fields") : nullptr;
    const auto& reverse = public_by_local[frame_identifier];
    for (size_t index = 0; frame_fields && index < frame_fields->GetSize(); ++index) {
      auto source = frame_fields->GetDictionary(index);
      if (!source) {
        continue;
      }
      auto entry = source->Copy(false);
      const auto public_id =
          reverse.find(entry->GetString("field_id").ToString());
      if (public_id == reverse.end()) {
        continue;
      }
      entry->SetString("field_id", public_id->second);
      fields->SetDictionary(fields->GetSize(), entry);
    }
  }
  auto result = CefDictionaryValue::Create();
  result->SetList("fields", fields);
  result->SetInt("sensitive_count", sensitive_count);
  result->SetBool("values_logged", false);
  result->SetBool("sensitive_values_exposed", false);
  result->SetDouble("basis_page_revision",
                    static_cast<double>(basis_page_revision));
  result->SetDouble("page_revision",
                    static_cast<double>(basis_page_revision));
  AppendValueFreeReplay("form_inspect_composited", result,
                        basis_page_revision, basis_page_revision);
  return Response(id, result);
}

std::string SaccadeAdapter::ProtectedFillResponse(
    int id,
    CefRefPtr<CefDictionaryValue> params) {
  const std::string field_id =
      params ? params->GetString("field_id").ToString() : "";
  if (field_id.empty() || field_id.size() > 256) {
    return ErrorResponse(id, "INVALID_ARGUMENT",
                         "protected_fill requires a bounded field_id");
  }

  FormFieldRoute route;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    const auto found = form_field_routes_.find(field_id);
    if (form_field_routes_revision_ != RequestRevision(params) ||
        found == form_field_routes_.end()) {
      return ErrorResponse(id, "STALE_FORM_INVENTORY",
                           "refresh the unified form inventory before protected fill");
    }
    route = found->second;
  }
  auto routed_params = params ? params->Copy(false)
                              : CefDictionaryValue::Create();
  routed_params->SetString("field_id", route.renderer_field_id);

  const std::string prepare_response =
      FormCommandResponseForFrame(id, "protected_prepare", routed_params,
                                  route.frame_identifier);
  auto parsed_prepare = CefParseJSON(prepare_response, JSON_PARSER_RFC);
  if (!parsed_prepare || parsed_prepare->GetType() != VTYPE_DICTIONARY) {
    return ErrorResponse(id, "TRANSPORT_UNAVAILABLE",
                         "protected fill preflight returned invalid data");
  }
  auto prepare_root = parsed_prepare->GetDictionary();
  if (!prepare_root->GetBool("ok")) {
    return prepare_response;
  }
  auto prepared = prepare_root->GetDictionary("result");
  if (!prepared || !prepared->GetBool("local_fill_allowed")) {
    const std::string reason =
        prepared ? prepared->GetString("reason").ToString()
                 : "protected_local_fill_not_allowed";
    return ErrorResponse(id, "PERMISSION_DENIED", reason);
  }

#if defined(OS_MAC)
  CefRefPtr<CefBrowser> prompt_browser;
  std::string page_origin;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    prompt_browser = browser_;
    page_origin = TrustedOrigin(current_url_);
  }
  auto prompt = SaccadePromptProtectedValue(
      prompt_browser, page_origin, prepared->GetString("label").ToString());
  if (!prompt.confirmed) {
    auto result = CefDictionaryValue::Create();
    result->SetString("field_id", field_id);
    result->SetString("status", "cancelled");
    result->SetBool("user_confirmed", false);
    result->SetBool("completed", false);
    result->SetBool("raw_value_returned", false);
    result->SetBool("sensitive_values_exposed", false);
    result->SetBool("values_logged", false);
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      result->SetDouble("page_revision", static_cast<double>(page_revision_));
    }
    return Response(id, result);
  }

  auto fill_params = routed_params->Copy(false);
  fill_params->SetString("local_value", prompt.value);
  fill_params->SetBool("user_confirmed", true);
  const std::string fill_response =
      FormCommandResponseForFrame(id, "protected_fill", fill_params,
                                  route.frame_identifier);
  std::fill(prompt.value.begin(), prompt.value.end(), '\0');
  fill_params->Remove("local_value");
  auto fill_value = CefParseJSON(fill_response, JSON_PARSER_RFC);
  auto fill_root = fill_value && fill_value->GetType() == VTYPE_DICTIONARY
                       ? fill_value->GetDictionary()
                       : nullptr;
  auto fill_result = fill_root && fill_root->GetBool("ok")
                         ? fill_root->GetDictionary("result")
                         : nullptr;
  if (fill_result) {
    fill_result->SetString("field_id", field_id);
    return Response(id, fill_result);
  }
  return fill_response;
#else
  return ErrorResponse(id, "TRANSPORT_UNAVAILABLE",
                       "protected local fill is not available on this platform");
#endif
}

std::string SaccadeAdapter::FormExecutePlanResponse(
    int id,
    CefRefPtr<CefDictionaryValue> params) {
  if (!ValidAssignments(params)) {
    return ErrorResponse(id, "INVALID_ARGUMENT",
                         "form execution requires scalar assignments");
  }
  const std::string expected_plan_id =
      params ? params->GetString("expected_plan_id").ToString() : "";
  if (expected_plan_id.empty() || expected_plan_id.size() > 128) {
    return ErrorResponse(id, "INVALID_ARGUMENT",
                         "form execution requires expected_plan_id");
  }
  const uint64_t basis_page_revision = RequestRevision(params);

  const std::string compile_response =
      FormCompilePlanResponse(id, params);
  auto compile_value = CefParseJSON(compile_response, JSON_PARSER_RFC);
  auto compile_root =
      compile_value && compile_value->GetType() == VTYPE_DICTIONARY
          ? compile_value->GetDictionary()
          : nullptr;
  if (!compile_root || !compile_root->GetBool("ok")) {
    return compile_root ? compile_response
                        : ErrorResponse(id, "FORM_COMMAND_FAILED",
                                        "form compile returned invalid data");
  }
  auto compiled = compile_root->GetDictionary("result");
  if (!compiled ||
      compiled->GetString("plan_id").ToString() != expected_plan_id) {
    return ErrorResponse(id, "STALE_FORM_PLAN",
                         "form plan id mismatch; recompile before execution");
  }

  auto assignments = params->GetDictionary("assignments");
  auto eligible = compiled->GetList("eligible");
  auto rejected = compiled->GetList("rejected");
  auto filled = CefListValue::Create();
  auto failed = CefListValue::Create();
  auto preserved = CefListValue::Create();
  auto rejected_copy = CefListValue::Create();
  auto receipts = CefListValue::Create();
  if (rejected) {
    for (size_t index = 0; index < rejected->GetSize(); ++index) {
      auto entry = rejected->GetDictionary(index);
      if (entry) {
        rejected_copy->SetDictionary(rejected_copy->GetSize(),
                                     entry->Copy(false));
      }
    }
  }

  int native_attempt_count = 0;
  for (size_t index = 0; eligible && index < eligible->GetSize(); ++index) {
    auto planned = eligible->GetDictionary(index);
    if (!planned) {
      continue;
    }
    const std::string field_id = planned->GetString("field_id").ToString();
    const std::string type = planned->GetString("type").ToString();
    const bool native_text_type =
        type == "text" || type == "email" || type == "tel" ||
        type == "url" || type == "search" || type == "textarea" ||
        type == "contenteditable" || type == "role_textbox";
    if (!native_text_type || assignments->GetType(field_id) != VTYPE_STRING) {
      auto failure = CefDictionaryValue::Create();
      failure->SetString("field_id", field_id);
      failure->SetString("reason", native_text_type
                                       ? "native_text_value_required"
                                       : "native_input_type_unsupported");
      failed->SetDictionary(failed->GetSize(), failure);
      continue;
    }

    auto type_params = CefDictionaryValue::Create();
    type_params->SetString("field_id", field_id);
    type_params->SetString("text", assignments->GetString(field_id));
    type_params->SetDouble("basis_page_revision",
                           static_cast<double>(basis_page_revision));
    ++native_attempt_count;
    const std::string type_response =
        TypeFieldTextResponse(id, type_params, true);
    type_params->Remove("text");
    auto type_value = CefParseJSON(type_response, JSON_PARSER_RFC);
    auto type_root =
        type_value && type_value->GetType() == VTYPE_DICTIONARY
            ? type_value->GetDictionary()
            : nullptr;
    auto type_result = type_root && type_root->GetBool("ok")
                           ? type_root->GetDictionary("result")
                           : nullptr;
    auto receipt = type_result
                       ? type_result->GetDictionary("native_input_receipt")
                       : nullptr;
    if (!type_result || !type_result->GetBool("receipt_verified") ||
        !receipt || !receipt->GetBool("same_webview")) {
      auto failure = CefDictionaryValue::Create();
      failure->SetString("field_id", field_id);
      failure->SetString("reason", "native_input_receipt_missing");
      if (type_root && type_root->GetType("error") == VTYPE_DICTIONARY) {
        auto error = type_root->GetDictionary("error");
        failure->SetString("error_code", error->GetString("code"));
      }
      failed->SetDictionary(failed->GetSize(), failure);
      continue;
    }
    auto filled_entry = CefDictionaryValue::Create();
    filled_entry->SetString("field_id", field_id);
    filled_entry->SetString("status", "filled_verified");
    filled_entry->SetString("method", "cef_devtools_input_insert_text");
    filled->SetDictionary(filled->GetSize(), filled_entry);
    receipts->SetDictionary(receipts->GetSize(), receipt->Copy(false));
  }

  uint64_t observed_revision = basis_page_revision;
  if (native_attempt_count > 0) {
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      if (page_revision_ == basis_page_revision) {
        ++page_revision_;
        observed_revision = page_revision_;
        ResetPageStateLocked("page changed after native form input");
      } else {
        observed_revision = page_revision_;
      }
    }
    CefPostTask(TID_UI,
                base::BindOnce(&SaccadeAdapter::RefreshCollectorOnUi,
                               base::Unretained(this)));
  }

  const size_t eligible_count = eligible ? eligible->GetSize() : 0;
  const bool receipt_verified =
      native_attempt_count > 0 && failed->GetSize() == 0 &&
      filled->GetSize() == eligible_count &&
      receipts->GetSize() == filled->GetSize();
  const int native_input_receipt_count =
      static_cast<int>(receipts->GetSize());
  auto result = CefDictionaryValue::Create();
  result->SetString("plan_id", expected_plan_id);
  result->SetList("filled", filled);
  result->SetList("preserved", preserved);
  result->SetList("rejected", rejected_copy);
  result->SetList("failed", failed);
  result->SetList("native_input_receipts", receipts);
  result->SetInt("write_attempted_count", native_attempt_count);
  result->SetInt("native_input_receipt_count", native_input_receipt_count);
  result->SetBool("same_webview_native_input", receipt_verified);
  result->SetBool("receipt_verified", receipt_verified);
  result->SetDouble("basis_page_revision",
                    static_cast<double>(basis_page_revision));
  result->SetDouble("page_revision", static_cast<double>(observed_revision));
  result->SetBool("sensitive_values_exposed", false);
  result->SetBool("values_logged", false);
  AppendValueFreeReplay("form_native_execute", result,
                        basis_page_revision, observed_revision);
  return Response(id, result);
}

std::string SaccadeAdapter::TypeFieldTextResponse(
    int id,
    CefRefPtr<CefDictionaryValue> params,
    bool allow_ordinary_native_type) {
  const std::string field_id =
      params ? params->GetString("field_id").ToString() : "";
  const std::string text = params ? params->GetString("text").ToString() : "";
  if (field_id.empty() || field_id.size() > 256 || text.empty()) {
    return ErrorResponse(id, "INVALID_ARGUMENT",
                         "type_field_text requires field_id and non-empty text");
  }
  const std::u16string characters = CefString(text).ToString16();
  if (characters.empty() || characters.size() > 16384) {
    return ErrorResponse(id, "INVALID_ARGUMENT",
                         "type_field_text exceeds the 16384 character limit");
  }

  FormFieldRoute route;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    const auto found = form_field_routes_.find(field_id);
    if (form_field_routes_revision_ != RequestRevision(params) ||
        found == form_field_routes_.end()) {
      return ErrorResponse(id, "STALE_FORM_INVENTORY",
                           "refresh the unified form inventory before typing");
    }
    route = found->second;
  }

  auto focus_params = CefDictionaryValue::Create();
  focus_params->SetString("field_id", route.renderer_field_id);
  focus_params->SetBool("allow_ordinary_native_type",
                        allow_ordinary_native_type);
  focus_params->SetDouble("basis_page_revision",
                          static_cast<double>(RequestRevision(params)));
  const std::string focus_response =
      FormCommandResponseForFrame(id, "focus_native_type", focus_params,
                                  route.frame_identifier);
  auto focus_value = CefParseJSON(focus_response, JSON_PARSER_RFC);
  auto focus_root = focus_value && focus_value->GetType() == VTYPE_DICTIONARY
                        ? focus_value->GetDictionary()
                        : nullptr;
  if (!focus_root || !focus_root->GetBool("ok")) {
    return focus_response;
  }
  auto focus_result = focus_root->GetDictionary("result");
  if (!focus_result || !focus_result->GetBool("native_type_ready")) {
    return ErrorResponse(id, "POLICY_BLOCKED",
                         focus_result
                             ? focus_result->GetString("reason").ToString()
                             : "form field is not ready for native typing");
  }

  int target_browser_id = 0;
  const uint64_t target_revision = RequestRevision(params);
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (text_insert_pending_) {
      return ErrorResponse(id, "CONFLICT", "another text insert is active");
    }
    if (!browser_ || page_revision_ != target_revision) {
      return ErrorResponse(id, "STALE_PAGE_REVISION",
                           "form-field target changed before text insertion");
    }
    target_browser_id = browser_->GetIdentifier();
    text_insert_pending_ = true;
    text_insert_done_ = false;
    text_insert_ok_ = false;
    text_insert_message_id_ = 0;
    text_insert_error_.clear();
    text_insert_registration_ = nullptr;
  }
  CefPostTask(TID_UI,
              base::BindOnce(&SaccadeAdapter::DispatchTextOnUi,
                             base::Unretained(this), characters,
                             target_browser_id, target_revision));

  {
    std::unique_lock<std::mutex> lock(state_mutex_);
    if (!text_insert_cv_.wait_for(lock, std::chrono::seconds(5), [this] {
          return stopping_ || text_insert_done_;
        })) {
      text_insert_pending_ = false;
      text_insert_registration_ = nullptr;
      return ErrorResponse(id, "TIMEOUT", "CEF Input.insertText timed out");
    }
    text_insert_pending_ = false;
    text_insert_registration_ = nullptr;
    if (!text_insert_ok_) {
      return ErrorResponse(id, "FORM_COMMAND_FAILED",
                           text_insert_error_.empty()
                               ? "CEF Input.insertText failed"
                               : text_insert_error_);
    }
  }

  auto verify_params = CefDictionaryValue::Create();
  verify_params->SetString("field_id", route.renderer_field_id);
  verify_params->SetDouble("basis_page_revision",
                           static_cast<double>(RequestRevision(params)));
  verify_params->SetInt("expected_length",
                        static_cast<int>(characters.size()));
  verify_params->SetString("expected_hash", Fnv1aUtf16(characters));
  verify_params->SetString(
      "visible_hash_before",
      focus_result->GetString("visible_hash_before").ToString());

  bool backing_match = false;
  bool visible_changed = false;
  int observed_length = 0;
  for (int attempt = 0; attempt < 20; ++attempt) {
    if (attempt > 0) {
      std::this_thread::sleep_for(std::chrono::milliseconds(25));
    }
    const std::string verify_response =
        FormCommandResponseForFrame(id, "verify_native_type", verify_params,
                                    route.frame_identifier);
    auto verify_value = CefParseJSON(verify_response, JSON_PARSER_RFC);
    auto verify_root =
        verify_value && verify_value->GetType() == VTYPE_DICTIONARY
            ? verify_value->GetDictionary()
            : nullptr;
    if (!verify_root || !verify_root->GetBool("ok")) {
      return verify_response;
    }
    auto verify_result = verify_root->GetDictionary("result");
    if (verify_result) {
      backing_match = verify_result->GetBool("backing_match");
      visible_changed = verify_result->GetBool("visible_changed");
      observed_length = verify_result->GetInt("value_length");
    }
    if (verify_result && verify_result->GetBool("verified")) {
      auto result = CefDictionaryValue::Create();
      result->SetString("field_id", field_id);
      result->SetString("status", "filled_verified");
      result->SetString("method", "cef_devtools_input_insert_text");
      result->SetInt("chars_requested", static_cast<int>(characters.size()));
      result->SetBool("backing_match", true);
      result->SetBool("visible_changed", true);
      result->SetBool("receipt_verified", true);
      result->SetBool("values_logged", false);
      const uint64_t revision = RequestRevision(params);
      auto receipt = CefDictionaryValue::Create();
      receipt->SetString("schema", "saccade.native_input_receipt/1");
      receipt->SetString("kind", "text_input");
      receipt->SetString("backend", "cef_devtools_protocol");
      receipt->SetString("method", "Input.insertText");
      receipt->SetString("field_id", field_id);
      receipt->SetBool("same_webview", true);
      receipt->SetBool("dispatch_acknowledged", true);
      receipt->SetBool("postcondition_verified", true);
      receipt->SetDouble("basis_page_revision",
                         static_cast<double>(revision));
      receipt->SetDouble("page_revision", static_cast<double>(revision));
      receipt->SetBool("value_logged", false);
      result->SetDictionary("native_input_receipt", receipt);
      AppendValueFreeReplay("native_text_typed", result, revision, revision);
      return Response(id, result);
    }
  }
  char detail[192] = {};
  std::snprintf(detail, sizeof(detail),
                "native typing postcondition failed "
                "(backing_match=%s, visible_changed=%s, observed_length=%d)",
                backing_match ? "true" : "false",
                visible_changed ? "true" : "false", observed_length);
  return ErrorResponse(id, "POSTCONDITION_FAILED",
                       detail);
}

std::string SaccadeAdapter::ScreenshotAuditResponse(
    int id,
    CefRefPtr<CefDictionaryValue> params) {
  const std::string policy_response =
      FormCommandResponse(id, "screenshot_policy", params);
  auto parsed_policy = CefParseJSON(policy_response, JSON_PARSER_RFC);
  if (!parsed_policy || parsed_policy->GetType() != VTYPE_DICTIONARY) {
    return ErrorResponse(id, "SCREENSHOT_FAILED",
                         "screenshot policy returned invalid data");
  }
  auto policy_root = parsed_policy->GetDictionary();
  if (!policy_root->GetBool("ok")) {
    return policy_response;
  }
  auto result = policy_root->GetDictionary("result");
  if (!result || !result->GetBool("capture_allowed")) {
    return policy_response;
  }
  const uint64_t basis_page_revision = RequestRevision(params);

  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (screenshot_pending_) {
      return ErrorResponse(id, "SCREENSHOT_BUSY",
                           "another screenshot audit is pending");
    }
    screenshot_pending_ = true;
    screenshot_done_ = false;
    screenshot_ok_ = false;
    screenshot_message_id_ = 0;
    screenshot_error_.clear();
    screenshot_bytes_.clear();
  }
  CefPostTask(TID_UI,
              base::BindOnce(&SaccadeAdapter::CaptureScreenshotOnUi,
                             base::Unretained(this)));

  std::unique_lock<std::mutex> lock(state_mutex_);
  if (!screenshot_cv_.wait_for(lock, std::chrono::seconds(10), [this] {
        return stopping_ || screenshot_done_;
      })) {
    screenshot_pending_ = false;
    screenshot_registration_ = nullptr;
    return ErrorResponse(id, "TIMEOUT", "CEF screenshot audit timed out");
  }
  const bool ok = screenshot_ok_;
  const std::string error = screenshot_error_;
  const uint64_t observed_revision = page_revision_;
  std::vector<unsigned char> bytes = std::move(screenshot_bytes_);
  screenshot_pending_ = false;
  screenshot_done_ = false;
  lock.unlock();
  if (!ok) {
    return ErrorResponse(id, "SCREENSHOT_FAILED",
                         error.empty() ? "CEF screenshot audit failed" : error);
  }
  if (basis_page_revision != observed_revision) {
    return ErrorResponse(id, "STALE_PAGE_REVISION",
                         "page changed during screenshot audit");
  }
  if (bytes.empty() || bytes.size() > 32 * 1024 * 1024) {
    return ErrorResponse(id, "SCREENSHOT_FAILED",
                         "CEF returned an invalid screenshot size");
  }

  const std::string screenshot_path = replay_path_ + ".audit.png";
  const int fd = open(screenshot_path.c_str(),
                      O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC, 0600);
  if (fd < 0) {
    return ErrorResponse(id, "SCREENSHOT_FAILED",
                         "could not create owner-only screenshot artifact");
  }
  fchmod(fd, 0600);
  size_t written = 0;
  while (written < bytes.size()) {
    const ssize_t count =
        write(fd, bytes.data() + written, bytes.size() - written);
    if (count < 0 && errno == EINTR) {
      continue;
    }
    if (count <= 0) {
      break;
    }
    written += static_cast<size_t>(count);
  }
  const bool saved = written == bytes.size() && fsync(fd) == 0;
  close(fd);
  if (!saved) {
    unlink(screenshot_path.c_str());
    return ErrorResponse(id, "SCREENSHOT_FAILED",
                         "owner-only screenshot artifact was incomplete");
  }
  result->SetString("screenshot_path", screenshot_path);
  result->SetInt("screenshot_bytes", static_cast<int>(bytes.size()));
  result->SetString("capture_backend", "cef_page_capture_audit");
  result->SetBool("truth_route_used", false);
  AppendValueFreeReplay("screenshot_saved", result,
                        basis_page_revision, observed_revision);
  return Response(id, result);
}

void SaccadeAdapter::NavigateOnUi(std::string url) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    browser = browser_;
  }
  if (browser && browser->GetMainFrame()) {
    browser->GetMainFrame()->LoadURL(url);
  }
}

void SaccadeAdapter::NavigateHistoryOnUi(std::string action) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    browser = browser_;
  }
  if (!browser) {
    return;
  }
  if (action == "back" && browser->CanGoBack()) {
    browser->GoBack();
  } else if (action == "forward" && browser->CanGoForward()) {
    browser->GoForward();
  } else if (action == "reload") {
    browser->Reload();
  }
}

void SaccadeAdapter::StartReflexOnUi() {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    browser = browser_;
  }
  if (browser && browser->GetMainFrame()) {
    auto message = CefProcessMessage::Create("saccade.reflex.start_v1");
    browser->GetMainFrame()->SendProcessMessage(PID_RENDERER, message);
  }
}

void SaccadeAdapter::FinalizeFormInventoryLocked(FormCommandState& state) {
  state.done = true;
  state.ok = state.successful_responses > 0;
  if (!state.ok) {
    if (state.error.empty()) {
      state.error = "all renderer frame form commands failed";
    }
    return;
  }

  auto original_input = CefParseJSON(state.input_json, JSON_PARSER_RFC);
  auto input = original_input && original_input->GetType() == VTYPE_DICTIONARY
                   ? original_input->GetDictionary()
                   : CefDictionaryValue::Create();
  std::string mode = input->GetString("mode").ToString();
  if (mode != "actionable" && mode != "compact") {
    mode = "full";
  }
  const int requested_offset = input->GetInt("offset");
  const int offset = requested_offset > 0 ? requested_offset : 0;
  int limit = input->GetInt("limit");
  if (limit < 1) {
    limit = mode == "compact" ? 100 : 500;
  }
  limit = std::min(limit, 500);

  std::sort(state.frame_payloads.begin(), state.frame_payloads.end(),
            [](const FormCommandState::FramePayload& left,
               const FormCommandState::FramePayload& right) {
              if (left.depth != right.depth) {
                return left.depth < right.depth;
              }
              return left.dispatch_order < right.dispatch_order;
            });
  const bool multiple_form_frames = state.form_frames_detected > 1;
  auto all_fields = CefListValue::Create();
  auto frame_inventory = CefListValue::Create();
  std::map<std::string, FormFieldRoute> routes;
  int dom_control_count = 0;
  int hidden_control_count = 0;
  int field_count = 0;
  int eligible_count = 0;
  int sensitive_count = 0;
  int existing_value_count = 0;
  int form_frame_index = 0;
  bool any_embedded_frame = false;

  for (const auto& frame_payload : state.frame_payloads) {
    auto parsed = CefParseJSON(frame_payload.payload, JSON_PARSER_RFC);
    auto frame_result = parsed && parsed->GetType() == VTYPE_DICTIONARY
                            ? parsed->GetDictionary()
                            : nullptr;
    if (!frame_result) {
      continue;
    }
    const int frame_field_count = frame_result->GetInt("field_count");
    if (frame_field_count <= 0) {
      continue;
    }
    const int current_frame_index = form_frame_index++;
    const std::string frame_scope = "f" + std::to_string(current_frame_index);
    any_embedded_frame = any_embedded_frame || !frame_payload.is_main;
    dom_control_count += frame_result->GetInt("dom_control_count");
    hidden_control_count += frame_result->GetInt("hidden_control_count");
    field_count += frame_field_count;
    eligible_count += frame_result->GetInt("eligible_count");
    sensitive_count += frame_result->GetInt("sensitive_count");
    existing_value_count += frame_result->GetInt("existing_value_count");

    auto frame_summary = CefDictionaryValue::Create();
    frame_summary->SetInt("frame_index", current_frame_index);
    frame_summary->SetInt("frame_depth", frame_payload.depth);
    frame_summary->SetString("frame_scope",
                             frame_payload.is_main ? "main" : "embedded");
    frame_summary->SetInt("field_count", frame_field_count);
    frame_summary->SetInt("eligible_count",
                          frame_result->GetInt("eligible_count"));
    frame_inventory->SetDictionary(frame_inventory->GetSize(), frame_summary);

    auto fields = frame_result->GetList("fields");
    for (size_t index = 0; fields && index < fields->GetSize(); ++index) {
      auto source = fields->GetDictionary(index);
      if (!source) {
        continue;
      }
      auto field = source->Copy(false);
      const std::string renderer_field_id =
          field->GetString("field_id").ToString();
      if (renderer_field_id.empty()) {
        continue;
      }
      std::string exposed_field_id = renderer_field_id;
      if (multiple_form_frames) {
        exposed_field_id = "frame:" + frame_scope + ":" + renderer_field_id;
        if (exposed_field_id.size() > 256) {
          exposed_field_id =
              "frame:" + frame_scope + ":field:" +
              Fnv1aUtf16(CefString(renderer_field_id).ToString16());
        }
      }
      field->SetString("field_id", exposed_field_id);
      field->SetInt("frame_index", current_frame_index);
      field->SetInt("frame_depth", frame_payload.depth);
      field->SetString("frame_scope",
                       frame_payload.is_main ? "main" : "embedded");
      routes[exposed_field_id] =
          {frame_payload.frame_identifier, renderer_field_id};
      all_fields->SetDictionary(all_fields->GetSize(), field);
    }
  }

  if (form_frame_index == 0) {
    auto selected = CefParseJSON(state.payload, JSON_PARSER_RFC);
    if (!selected || selected->GetType() != VTYPE_DICTIONARY) {
      state.ok = false;
      state.error = "renderer returned invalid fixed form result";
      return;
    }
  }

  auto candidates = CefListValue::Create();
  for (size_t index = 0; index < all_fields->GetSize(); ++index) {
    auto field = all_fields->GetDictionary(index);
    if (!field) {
      continue;
    }
    const bool actionable = field->GetBool("eligible") ||
                            field->GetBool("native_type_eligible");
    if (mode != "actionable" || actionable) {
      candidates->SetDictionary(candidates->GetSize(), field->Copy(false));
    }
  }
  auto page = CefListValue::Create();
  const size_t start = std::min(static_cast<size_t>(offset),
                                candidates->GetSize());
  const size_t end = std::min(start + static_cast<size_t>(limit),
                              candidates->GetSize());
  for (size_t index = start; index < end; ++index) {
    auto field = candidates->GetDictionary(index);
    if (!field) {
      continue;
    }
    if (mode != "compact") {
      page->SetDictionary(page->GetSize(), field->Copy(false));
      continue;
    }
    auto compact = CefDictionaryValue::Create();
    for (const char* key : {"field_id", "label", "type", "owner",
                            "sensitivity", "value_state", "frame_scope"}) {
      compact->SetString(key, field->GetString(key));
    }
    compact->SetInt("frame_index", field->GetInt("frame_index"));
    compact->SetInt("frame_depth", field->GetInt("frame_depth"));
    compact->SetBool("required", field->GetBool("required"));
    compact->SetBool("eligible", field->GetBool("eligible"));
    compact->SetBool("native_type_eligible",
                     field->GetBool("native_type_eligible"));
    auto blocked = field->GetList("blocked_reasons");
    compact->SetString("blocked_reason",
                       blocked && blocked->GetSize() > 0
                           ? blocked->GetString(0)
                           : "");
    page->SetDictionary(page->GetSize(), compact);
  }

  auto result = CefDictionaryValue::Create();
  result->SetString("mode", mode);
  result->SetInt("dom_control_count", dom_control_count);
  result->SetInt("hidden_control_count", hidden_control_count);
  result->SetInt("field_count", field_count);
  result->SetInt("candidate_count", static_cast<int>(candidates->GetSize()));
  result->SetInt("eligible_count", eligible_count);
  result->SetInt("sensitive_count", sensitive_count);
  result->SetInt("existing_value_count", existing_value_count);
  result->SetInt("offset", offset);
  result->SetInt("limit", limit);
  result->SetInt("returned_count", static_cast<int>(page->GetSize()));
  result->SetBool("has_more", end < candidates->GetSize());
  result->SetList("fields", page);
  result->SetList("frame_inventory", frame_inventory);
  result->SetInt("frame_count_scanned", state.expected_responses);
  result->SetInt("frame_response_success_count", state.successful_responses);
  result->SetInt("frame_response_failure_count",
                 state.received_responses - state.successful_responses);
  result->SetInt("frame_response_pending_count",
                 state.expected_responses - state.received_responses);
  result->SetBool("frame_settlement_partial",
                  state.received_responses < state.expected_responses);
  result->SetInt("form_frame_count", form_frame_index);
  if (!state.error.empty()) {
    result->SetString("frame_response_error", state.error);
  }
  result->SetString("frame_scope",
                    form_frame_index > 1
                        ? "composited"
                        : (any_embedded_frame ? "embedded" : "main"));
  result->SetBool("embedded_frame", any_embedded_frame);
  result->SetBool("frame_selection_ambiguous", false);
  result->SetBool("frame_aggregation_complete",
                  state.received_responses >= state.expected_responses);
  auto value = CefValue::Create();
  value->SetDictionary(result);
  state.payload = JsonString(value);
  selected_form_frame_identifier_ = state.best_frame_identifier;
  form_field_routes_ = std::move(routes);
  form_field_routes_revision_ =
      state.received_responses >= state.expected_responses ? page_revision_ : 0;
}

void SaccadeAdapter::SettleFormInventoryOnUi(int request_id) {
  CEF_REQUIRE_UI_THREAD();
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    const auto pending = form_commands_.find(request_id);
    if (pending == form_commands_.end() || pending->second.done ||
        pending->second.command != "inventory") {
      return;
    }
    FinalizeFormInventoryLocked(pending->second);
  }
  form_cv_.notify_all();
}
void SaccadeAdapter::DispatchFormCommandOnUi(int request_id,
                                             std::string command,
                                             std::string input_json) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  std::string selected_frame_identifier;
  std::string target_frame_identifier;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    browser = browser_;
    selected_frame_identifier = selected_form_frame_identifier_;
    const auto pending = form_commands_.find(request_id);
    if (pending != form_commands_.end()) {
      target_frame_identifier = pending->second.target_frame_identifier;
    }
  }
  if (!browser || !browser->GetMainFrame()) {
    std::lock_guard<std::mutex> lock(state_mutex_);
    auto pending = form_commands_.find(request_id);
    if (pending != form_commands_.end()) {
      pending->second.done = true;
      pending->second.ok = false;
      pending->second.error = "browser closed before form command";
    }
    form_cv_.notify_all();
    return;
  }

  std::vector<CefRefPtr<CefFrame>> frames;
  if (command == "inventory") {
    std::vector<CefString> frame_identifiers;
    browser->GetFrameIdentifiers(frame_identifiers);
    for (const auto& frame_identifier : frame_identifiers) {
      auto candidate = browser->GetFrameByIdentifier(frame_identifier);
      if (candidate && candidate->IsValid()) {
        frames.push_back(candidate);
      }
    }
  } else if (!target_frame_identifier.empty()) {
    auto target = browser->GetFrameByIdentifier(target_frame_identifier);
    if (target && target->IsValid()) {
      frames.push_back(target);
    }
  } else if (!selected_frame_identifier.empty()) {
    auto selected = browser->GetFrameByIdentifier(selected_frame_identifier);
    if (selected && selected->IsValid()) {
      frames.push_back(selected);
    }
  }
  if (frames.empty()) {
    frames.push_back(browser->GetMainFrame());
  }

  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    auto pending = form_commands_.find(request_id);
    if (pending == form_commands_.end() || pending->second.done) {
      return;
    }
    pending->second.expected_responses =
        static_cast<int>(frames.size());
    pending->second.frame_dispatch_order.clear();
    for (const auto& target_frame : frames) {
      pending->second.frame_dispatch_order.push_back(
          target_frame->GetIdentifier().ToString());
    }
  }

  for (const auto& target_frame : frames) {
    auto message = CefProcessMessage::Create("saccade.form.request_v1");
    auto arguments = message->GetArgumentList();
    arguments->SetInt(0, request_id);
    arguments->SetString(1, command);
    arguments->SetString(2, input_json);
    target_frame->SendProcessMessage(PID_RENDERER, message);
  }
  if (command == "inventory") {
    CefPostDelayedTask(
        TID_UI,
        base::BindOnce(&SaccadeAdapter::SettleFormInventoryOnUi,
                       base::Unretained(this), request_id),
        1000);
  }
}

void SaccadeAdapter::DispatchTextOnUi(std::u16string text,
                                      int browser_id,
                                      uint64_t page_revision) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    const auto target = browsers_.find(browser_id);
    if (!browser_ || target == browsers_.end() ||
        !browser_->IsSame(target->second) || page_revision_ != page_revision) {
      text_insert_done_ = true;
      text_insert_ok_ = false;
      text_insert_error_ = "rich-editor tab or page changed before insertion";
      text_insert_cv_.notify_all();
      return;
    }
    browser = target->second;
  }
  if (!browser) {
    std::lock_guard<std::mutex> lock(state_mutex_);
    text_insert_done_ = true;
    text_insert_ok_ = false;
    text_insert_error_ = "browser closed before Input.insertText";
    text_insert_cv_.notify_all();
    return;
  }
  browser->GetHost()->SetFocus(true);
  auto host = browser->GetHost();
  auto observer =
      CefRefPtr<SaccadeTextInsertObserver>(new SaccadeTextInsertObserver(this));
  auto registration = host->AddDevToolsMessageObserver(observer);
  auto params = CefDictionaryValue::Create();
  params->SetString("text", CefString(text));
  const int message_id =
      host->ExecuteDevToolsMethod(0, "Input.insertText", params);
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!registration || message_id == 0) {
      text_insert_done_ = true;
      text_insert_ok_ = false;
      text_insert_error_ = "CEF rejected Input.insertText";
      text_insert_registration_ = nullptr;
    } else {
      text_insert_registration_ = registration;
      text_insert_message_id_ = message_id;
    }
  }
  text_insert_cv_.notify_all();
}

void SaccadeAdapter::OnTextInsertResult(int message_id,
                                        bool success,
                                        const void* result,
                                        size_t result_size) {
  CEF_REQUIRE_UI_THREAD();
  std::lock_guard<std::mutex> lock(state_mutex_);
  if (message_id != text_insert_message_id_) {
    return;
  }
  text_insert_done_ = true;
  text_insert_ok_ = success;
  text_insert_error_ = success ? "" : "CEF Input.insertText returned failure";
  text_insert_message_id_ = 0;
  text_insert_registration_ = nullptr;
  text_insert_cv_.notify_all();
}

void SaccadeAdapter::CaptureScreenshotOnUi() {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    browser = browser_;
  }
  if (!browser) {
    std::lock_guard<std::mutex> lock(state_mutex_);
    screenshot_done_ = true;
    screenshot_ok_ = false;
    screenshot_error_ = "browser closed before screenshot audit";
    screenshot_cv_.notify_all();
    return;
  }
  auto host = browser->GetHost();
  auto observer =
      CefRefPtr<SaccadeScreenshotObserver>(new SaccadeScreenshotObserver(this));
  auto registration = host->AddDevToolsMessageObserver(observer);
  auto params = CefDictionaryValue::Create();
  params->SetString("format", "png");
  params->SetBool("fromSurface", true);
  params->SetBool("captureBeyondViewport", false);
  const int message_id =
      host->ExecuteDevToolsMethod(0, "Page.captureScreenshot", params);
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!registration || message_id == 0) {
      screenshot_done_ = true;
      screenshot_ok_ = false;
      screenshot_error_ = "CEF rejected Page.captureScreenshot";
      screenshot_registration_ = nullptr;
    } else if (screenshot_message_id_ == 0) {
      screenshot_registration_ = registration;
      screenshot_message_id_ = message_id;
    } else if (screenshot_message_id_ != message_id) {
      screenshot_done_ = true;
      screenshot_ok_ = false;
      screenshot_error_ = "CEF returned an unexpected screenshot request id";
      screenshot_registration_ = nullptr;
    }
  }
  screenshot_cv_.notify_all();
}

void SaccadeAdapter::OnScreenshotResult(int message_id,
                                        bool success,
                                        const void* result,
                                        size_t result_size) {
  CEF_REQUIRE_UI_THREAD();
  std::vector<unsigned char> decoded;
  std::string error;
  if (!success || !result || result_size == 0 || result_size > 48 * 1024 * 1024) {
    error = "CEF screenshot method returned an error";
  } else {
    const std::string json(static_cast<const char*>(result), result_size);
    auto parsed = CefParseJSON(json, JSON_PARSER_RFC);
    auto dictionary = parsed && parsed->GetType() == VTYPE_DICTIONARY
                          ? parsed->GetDictionary()
                          : nullptr;
    const CefString encoded = dictionary ? dictionary->GetString("data") : "";
    auto binary = encoded.empty() ? nullptr : CefBase64Decode(encoded);
    const size_t size = binary ? binary->GetSize() : 0;
    if (!binary || size == 0 || size > 32 * 1024 * 1024) {
      error = "CEF screenshot payload was invalid";
    } else {
      decoded.resize(size);
      if (binary->GetData(decoded.data(), size, 0) != size) {
        decoded.clear();
        error = "CEF screenshot payload was truncated";
      }
    }
  }

  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!screenshot_pending_ ||
        (screenshot_message_id_ != 0 &&
         message_id != screenshot_message_id_)) {
      return;
    }
    screenshot_message_id_ = message_id;
    screenshot_done_ = true;
    screenshot_ok_ = error.empty();
    screenshot_error_ = error;
    screenshot_bytes_ = std::move(decoded);
    screenshot_registration_ = nullptr;
  }
  screenshot_cv_.notify_all();
}

void SaccadeAdapter::DispatchPointerOnUi(int x,
                                         int y,
                                         std::string action_id,
                                         int browser_id,
                                         uint64_t page_revision,
                                         uint64_t layout_epoch,
                                         bool allow_layout_rebase) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  bool expects_agent_child = false;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    const auto found = browsers_.find(browser_id);
    const bool exact_layout =
        page_revision_ == page_revision && layout_epoch_ == layout_epoch;
    const bool safe_target_layout_rebase =
        allow_layout_rebase && last_layout_page_revision_ == page_revision_;
    if (found != browsers_.end() && browser_ && browser_->IsSame(found->second) &&
        (exact_layout || safe_target_layout_rebase)) {
      browser = found->second;
      const auto action = actions_.find(action_id);
      expects_agent_child =
          action != actions_.end() && action->second.opens_new_context;
      if (expects_agent_child) {
        pending_agent_child_openers_[browser_id] = action->second.destination_url;
      }
    }
  }
  if (!browser) {
    std::lock_guard<std::mutex> lock(state_mutex_);
    dispatched_actions_.erase(action_id);
    dispatched_action_facts_.erase(action_id);
    return;
  }
  CefMouseEvent event;
  event.x = x;
  event.y = y;
  event.modifiers = 0;
  auto host = browser->GetHost();
  host->SendMouseMoveEvent(event, false);
  host->SendMouseClickEvent(event, MBT_LEFT, false, 1);
  host->SendMouseClickEvent(event, MBT_LEFT, true, 1);
  if (expects_agent_child) {
    CefPostDelayedTask(
        TID_UI,
        base::BindOnce(&SaccadeAdapter::ClearPendingAgentChildOpenerOnUi,
                       base::Unretained(this), browser_id),
        10000);
  }
}

void SaccadeAdapter::ClearPendingAgentChildOpenerOnUi(int browser_id) {
  CEF_REQUIRE_UI_THREAD();
  std::lock_guard<std::mutex> lock(state_mutex_);
  pending_agent_child_openers_.erase(browser_id);
}

void SaccadeAdapter::DispatchDragOnUi(int start_x,
                                      int start_y,
                                      int end_x,
                                      int end_y,
                                      std::string action_id,
                                      int browser_id,
                                      uint64_t page_revision,
                                      uint64_t layout_epoch) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    const auto found = browsers_.find(browser_id);
    if (found != browsers_.end() && browser_ && browser_->IsSame(found->second) &&
        page_revision_ == page_revision && layout_epoch_ == layout_epoch) {
      browser = found->second;
    }
  }
  if (!browser) {
    std::lock_guard<std::mutex> lock(state_mutex_);
    dispatched_actions_.erase(action_id);
    dispatched_action_facts_.erase(action_id);
    return;
  }
  auto host = browser->GetHost();
  CefMouseEvent start;
  start.x = start_x;
  start.y = start_y;
  start.modifiers = 0;
  host->SendMouseMoveEvent(start, false);
  host->SendMouseClickEvent(start, MBT_LEFT, false, 1);

  CefMouseEvent end;
  end.x = end_x;
  end.y = end_y;
  end.modifiers = EVENTFLAG_LEFT_MOUSE_BUTTON;
  host->SendMouseMoveEvent(end, false);
  CefPostDelayedTask(
      TID_UI,
      base::BindOnce(&SaccadeAdapter::ReleaseDragOnUi,
                     base::Unretained(this), end_x, end_y, browser_id),
      250);
}

void SaccadeAdapter::ReleaseDragOnUi(int end_x, int end_y, int browser_id) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    const auto found = browsers_.find(browser_id);
    if (found != browsers_.end()) {
      browser = found->second;
    }
  }
  if (!browser) {
    return;
  }
  CefMouseEvent end;
  end.x = end_x;
  end.y = end_y;
  end.modifiers = 0;
  browser->GetHost()->SendMouseClickEvent(end, MBT_LEFT, true, 1);
}

void SaccadeAdapter::CloseOnUi() {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    browser = browser_;
  }
  if (browser) {
    browser->GetHost()->CloseBrowser(false);
  }
}

bool SaccadeAdapter::OpenChromeTabOnUi(std::string url, bool grant_to_agent) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    browser = browser_;
  }
  if (!browser) {
    return false;
  }
  CEF_DECLARE_COMMAND_ID(IDC_NEW_TAB);
  if (!browser->GetHost()->CanExecuteChromeCommand(IDC_NEW_TAB)) {
    return false;
  }
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (grant_to_agent) {
      pending_agent_tab_urls_.push_back(std::move(url));
    } else {
      pending_user_tab_urls_.push_back(std::move(url));
    }
  }
  browser->GetHost()->ExecuteChromeCommand(IDC_NEW_TAB,
                                           CEF_WOD_NEW_FOREGROUND_TAB);
  return true;
}

void SaccadeAdapter::OpenUserTabOnUi(std::string url) {
  CEF_REQUIRE_UI_THREAD();
  OpenChromeTabOnUi(std::move(url), false);
}

void SaccadeAdapter::OpenRoutedTabOnUi(std::string url) {
  CEF_REQUIRE_UI_THREAD();
  bool agent_requested = false;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    const auto pending = std::find_if(
        pending_agent_child_openers_.begin(),
        pending_agent_child_openers_.end(),
        [&url](const auto& entry) { return entry.second == url; });
    if (pending != pending_agent_child_openers_.end()) {
      pending_agent_child_openers_.erase(pending);
      agent_requested = true;
    }
  }
  OpenChromeTabOnUi(std::move(url), agent_requested);
}

void SaccadeAdapter::OpenAgentTabOnUi(std::string url) {
  CEF_REQUIRE_UI_THREAD();
  OpenChromeTabOnUi(std::move(url), true);
}

void SaccadeAdapter::RefreshCollectorOnUi() {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefFrame> frame;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    frame = browser_ ? browser_->GetMainFrame() : nullptr;
  }
  if (frame) {
    frame->SendProcessMessage(
        PID_RENDERER,
        CefProcessMessage::Create("saccade.collector.refresh_v1"));
  }
}

bool SaccadeAdapter::RefreshActionMap(int timeout_ms) {
  uint64_t initial_serial = 0;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!CurrentTabActiveLocked() || !browser_) {
      return false;
    }
    initial_serial = action_map_serial_;
  }
  CefPostTask(TID_UI, base::BindOnce(&SaccadeAdapter::RefreshCollectorOnUi,
                                    base::Unretained(this)));
  std::unique_lock<std::mutex> lock(state_mutex_);
  const bool completed = action_map_cv_.wait_for(
      lock, std::chrono::milliseconds(timeout_ms), [this, initial_serial] {
        return stopping_ || action_map_serial_ > initial_serial;
      });
  return completed && !stopping_ && action_map_serial_ > initial_serial;
}

bool SaccadeAdapter::WriteGrant() {
  std::lock_guard<std::mutex> grant_lock(grant_mutex_);
  auto endpoint = CefDictionaryValue::Create();
  endpoint->SetString("protocol", kProtocol);
  endpoint->SetString("scheme", "unix");
  endpoint->SetString("path", socket_path_);

  auto session_capability = CefDictionaryValue::Create();
  session_capability->SetString("scheme", "saccade_session_bearer_v1");
  session_capability->SetString("token", capability_);

  auto adapter = CefDictionaryValue::Create();
  adapter->SetString("contract_version", kContractVersion);
  adapter->SetString("transport", "owner_only_unix_v1");
  adapter->SetString("provenance", "browser_process");
  adapter->SetBool("page_dom_injected", false);
  adapter->SetBool("sensitive_values_exposed_to_agent", false);
  adapter->SetList("capabilities", CapabilityList());

  auto grant = CefDictionaryValue::Create();
  bool granted = false;
  bool agent_created = false;
  bool paused = false;
  std::string url;
  std::string tab_id;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    granted = CurrentTabGrantedLocked();
    const int browser_id = browser_ ? browser_->GetIdentifier() : 0;
    paused = granted &&
             agent_paused_browser_ids_.find(browser_id) !=
                 agent_paused_browser_ids_.end();
    agent_created = granted &&
                    agent_created_browser_ids_.find(browser_id) !=
                        agent_created_browser_ids_.end();
    if (granted) {
      url = current_url_;
      tab_id = CurrentTabIdLocked();
    }
  }
  grant->SetString("status", granted ? "granted" : "available");
  grant->SetString("grant_type", granted
                                      ? (agent_created ? "agent_created_tab"
                                                       : "current_tab_copilot")
                                      : "tab_broker");
  grant->SetBool("selected_tab_seen", granted);
  grant->SetBool("grant_required", !agent_created);
  grant->SetBool("grant_given", granted);
  grant->SetString("owner", agent_created ? "agent" : "human");
  grant->SetBool("agent_input_grant", granted);
  grant->SetBool("paused", paused);
  grant->SetString("agent_activity",
                   !granted ? "disconnected" : (paused ? "paused" : "idle"));
  grant->SetString("read_grant", granted ? "full_truth" : "none");
  grant->SetString("url", url);
  grant->SetString("tab_id", tab_id);
  grant->SetDictionary("engine_adapter", adapter);
  grant->SetDictionary("control_endpoint", endpoint);
  grant->SetDictionary("control_capability", session_capability);
  auto artifacts = CefDictionaryValue::Create();
  artifacts->SetString("replay", replay_path_);
  artifacts->SetBool("values_logged", false);
  grant->SetDictionary("artifacts", artifacts);

  auto value = CefValue::Create();
  value->SetDictionary(grant);
  const std::string contents = JsonString(value) + "\n";
  const std::string temporary_path = grant_path_ + ".tmp";
  const int fd =
      open(temporary_path.c_str(), O_WRONLY | O_CREAT | O_TRUNC, 0600);
  if (fd < 0) {
    return false;
  }
  fchmod(fd, 0600);
  const bool wrote = WriteAll(fd, contents) && fsync(fd) == 0;
  close(fd);
  if (!wrote || rename(temporary_path.c_str(), grant_path_.c_str()) != 0) {
    unlink(temporary_path.c_str());
    return false;
  }
  if (!WriteCurrentPointer()) {
    unlink(grant_path_.c_str());
    return false;
  }
  return true;
}

bool SaccadeAdapter::CurrentTabGrantedLocked() const {
  return browser_ &&
         agent_granted_browser_ids_.find(browser_->GetIdentifier()) !=
             agent_granted_browser_ids_.end();
}

bool SaccadeAdapter::CurrentTabPausedLocked() const {
  return browser_ &&
         agent_paused_browser_ids_.find(browser_->GetIdentifier()) !=
             agent_paused_browser_ids_.end();
}

bool SaccadeAdapter::CurrentTabActiveLocked() const {
  return CurrentTabGrantedLocked() && !CurrentTabPausedLocked();
}

bool SaccadeAdapter::CurrentTabHasHumanVerificationFailureLocked() const {
  return browser_ &&
         human_verification_failures_.find(browser_->GetIdentifier()) !=
             human_verification_failures_.end();
}

void SaccadeAdapter::RefreshAgentSwitchOnUi() {
  CEF_REQUIRE_UI_THREAD();
  // Chrome Runtime toolbar actions query GetAgentUiState through the bundled
  // native-messaging bridge. Do not mirror the state into a platform titlebar
  // accessory: that places the control in the tab strip instead of beside the
  // address bar and creates a second source of UI truth.
}

bool SaccadeAdapter::WriteCurrentPointer() {
  if (current_pointer_path_.empty()) {
    return true;
  }
  const std::string contents = grant_path_ + "\n";
  const std::string temporary_path =
      current_pointer_path_ + ".tmp." + std::to_string(getpid());
  const int fd = open(temporary_path.c_str(),
                      O_WRONLY | O_CREAT | O_TRUNC | O_NOFOLLOW, 0600);
  if (fd < 0) {
    return false;
  }
  fchmod(fd, 0600);
  const bool wrote = WriteAll(fd, contents) && fsync(fd) == 0;
  close(fd);
  if (!wrote || rename(temporary_path.c_str(), current_pointer_path_.c_str()) !=
                    0) {
    unlink(temporary_path.c_str());
    return false;
  }
  return true;
}

void SaccadeAdapter::RemoveCurrentPointerIfOwned() {
  if (current_pointer_path_.empty()) {
    return;
  }
  const int fd = open(current_pointer_path_.c_str(), O_RDONLY | O_NOFOLLOW);
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
  if (lstat(current_pointer_path_.c_str(), &current) == 0 &&
      current.st_dev == opened.st_dev && current.st_ino == opened.st_ino) {
    unlink(current_pointer_path_.c_str());
  }
}

void SaccadeAdapter::ResetPageStateLocked(const std::string& reason) {
  collector_ready_ = false;
  collector_error_.clear();
  selected_form_frame_identifier_.clear();
  form_field_routes_.clear();
  form_field_routes_revision_ = 0;
  controls_.clear();
  pending_facts_.clear();
  actions_.clear();
  staged_actions_.clear();
  action_scan_generation_ = 0;
  dispatched_actions_.clear();
  dispatched_action_facts_.clear();
  pending_receipts_.clear();
  for (auto& [request_id, command] : form_commands_) {
    if (!command.done) {
      command.done = true;
      command.ok = false;
      command.error = reason;
    }
  }
  if (screenshot_pending_) {
    screenshot_done_ = true;
    screenshot_ok_ = false;
    screenshot_error_ = reason;
  }
}

std::string SaccadeAdapter::CurrentTabIdLocked() const {
  return browser_ ? "cef:" + std::to_string(browser_->GetIdentifier()) : "";
}

void SaccadeAdapter::AppendValueFreeReplay(
    const std::string& event,
    CefRefPtr<CefDictionaryValue> result,
    uint64_t basis_page_revision,
    uint64_t observed_page_revision) {
  if (replay_path_.empty() || !result) {
    return;
  }
  auto record = CefDictionaryValue::Create();
  record->SetString("schema", "saccade-cef-value-free-replay-v1");
  record->SetString("event", event);
  record->SetString("status", "ok");
  record->SetDouble("basis_page_revision",
                    static_cast<double>(basis_page_revision));
  record->SetDouble("page_revision", static_cast<double>(observed_page_revision));
  record->SetBool("values_logged", false);
  for (const char* key : {"field_count", "eligible_count", "sensitive_count",
                          "existing_value_count", "write_attempted_count",
                          "changed_scrollers"}) {
    if (result->HasKey(key) && result->GetType(key) == VTYPE_INT) {
      record->SetInt(key, result->GetInt(key));
    }
  }
  for (const char* key : {"fields", "eligible", "rejected", "filled",
                          "preserved", "failed"}) {
    if (result->GetType(key) == VTYPE_LIST) {
      record->SetInt(std::string(key) + "_count",
                     static_cast<int>(result->GetList(key)->GetSize()));
    }
  }
  if (result->HasKey("capture_allowed")) {
    record->SetBool("capture_allowed", result->GetBool("capture_allowed"));
    record->SetString("reason", result->GetString("reason"));
  }
  if (result->GetType("confirmation") == VTYPE_DICTIONARY) {
    record->SetDictionary("confirmation",
                          result->GetDictionary("confirmation")->Copy(false));
  }
  auto value = CefValue::Create();
  value->SetDictionary(record);
  const std::string line = JsonString(value) + "\n";

  std::lock_guard<std::mutex> lock(replay_mutex_);
  const int fd = open(replay_path_.c_str(),
                      O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
  if (fd < 0) {
    return;
  }
  fchmod(fd, 0600);
  WriteAll(fd, line);
  fsync(fd);
  close(fd);
}
