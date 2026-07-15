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

#include <array>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <utility>

#include "include/base/cef_callback.h"
#include "include/cef_devtools_message_observer.h"
#include "include/cef_parser.h"
#include "include/wrapper/cef_closure_task.h"
#include "include/wrapper/cef_helpers.h"

namespace {

constexpr char kProtocol[] = "saccade-engine-control-v1";
constexpr char kContractVersion[] = "1.0";

std::string JsonString(CefRefPtr<CefValue> value) {
  return CefWriteJSON(value, JSON_WRITER_DEFAULT).ToString();
}

CefRefPtr<CefListValue> CapabilityList() {
  auto list = CefListValue::Create();
  const std::array<const char*, 20> capabilities = {
      "ping",        "shell_status", "navigate",    "pause",
      "close",       "truth",        "actions",     "next_fact",
      "act",         "next_receipt", "reflex_start", "form_inventory",
      "inspect_fields", "form_compile_plan", "form_execute_plan",
      "screenshot_policy", "screenshot_audit", "form_reveal_more",
      "article_text", "type_field_text"};
  list->SetSize(capabilities.size());
  for (size_t index = 0; index < capabilities.size(); ++index) {
    list->SetString(index, capabilities[index]);
  }
  return list;
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
  if (!params || !params->HasKey("field_ids")) {
    return true;
  }
  if (params->GetType("field_ids") != VTYPE_LIST) {
    return false;
  }
  auto fields = params->GetList("field_ids");
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
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    const int browser_id = browser->GetIdentifier();
    browsers_[browser_id] = browser;
    browser_roles_[browser_id] = {
        .is_popup = browser->IsPopup(),
        .opener_id = browser->GetHost()->GetOpenerIdentifier(),
    };
    if (!browser_) {
      browser_ = browser;
      if (browser->GetMainFrame()) {
        current_url_ = browser->GetMainFrame()->GetURL().ToString();
      }
    }
  }
  ConfigureIfRequested();
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
    current_url_ = frame ? frame->GetURL().ToString() : "";
    current_title_.clear();
    ++page_revision_;
    ResetPageStateLocked("visible tab changed while command was pending");
    refresh_grant = started_;
  }
  fact_cv_.notify_all();
  receipt_cv_.notify_all();
  form_cv_.notify_all();
  screenshot_cv_.notify_all();
  if (refresh_grant) {
    WriteGrant();
  }
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
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (browser_ && browser_->IsSame(browser)) {
      current_url_ = url.ToString();
      ++page_revision_;
      ResetPageStateLocked("page changed while form command was pending");
      refresh_grant = started_;
    }
  }
  form_cv_.notify_all();
  if (refresh_grant) {
    WriteGrant();
  }
}

void SaccadeAdapter::OnTitleChanged(CefRefPtr<CefBrowser> browser,
                                    const CefString& title) {
  CEF_REQUIRE_UI_THREAD();
  std::lock_guard<std::mutex> lock(state_mutex_);
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

bool SaccadeAdapter::OnRendererMessage(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    CefProcessId source_process,
    CefRefPtr<CefProcessMessage> message) {
  CEF_REQUIRE_UI_THREAD();
  if (source_process != PID_RENDERER || !frame || !frame->IsMain() ||
      !browser_ || !browser_->IsSame(browser) || !message ||
      !message->IsValid()) {
    return false;
  }

  const std::string name = message->GetName().ToString();
  if (name.rfind("saccade.renderer.", 0) != 0) {
    return false;
  }
  auto arguments = message->GetArgumentList();
  if (!arguments) {
    return true;
  }

  if (name == "saccade.renderer.form_response_v1" &&
      arguments->GetSize() == 3) {
    const int request_id = arguments->GetInt(0);
    const bool ok = arguments->GetBool(1);
    const std::string payload = arguments->GetString(2).ToString();
    bool refresh = false;
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      auto pending = form_commands_.find(request_id);
      if (pending == form_commands_.end() || pending->second.done) {
        return true;
      }
      pending->second.done = true;
      pending->second.ok = ok;
      if (ok && payload.size() <= 1024 * 1024) {
        pending->second.payload = payload;
        if (pending->second.command == "execute" ||
            pending->second.command == "reveal_more") {
          auto parsed = CefParseJSON(payload, JSON_PARSER_RFC);
          const char* counter = pending->second.command == "execute"
                                    ? "write_attempted_count"
                                    : "changed_scrollers";
          if (parsed && parsed->GetType() == VTYPE_DICTIONARY &&
              parsed->GetDictionary()->GetInt(counter) > 0) {
            ++page_revision_;
            ResetPageStateLocked(
                "page changed while form command was pending");
            refresh = true;
          }
        }
      } else {
        pending->second.ok = false;
        pending->second.error = ok ? "renderer form response was too large"
                                   : "fixed renderer form command failed";
      }
    }
    form_cv_.notify_all();
    if (refresh) {
      frame->SendProcessMessage(
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

  if (name == "saccade.renderer.action_v1" && arguments->GetSize() == 8) {
    TargetFact fact;
    fact.action_id = arguments->GetString(0).ToString();
    fact.role = arguments->GetString(1).ToString();
    fact.label = arguments->GetString(2).ToString();
    fact.left = arguments->GetDouble(3);
    fact.top = arguments->GetDouble(4);
    fact.width = arguments->GetDouble(5);
    fact.height = arguments->GetDouble(6);
    fact.renderer_epoch_ms = arguments->GetDouble(7);
    if (fact.action_id.empty() || fact.action_id.size() > 128 ||
        (fact.role != "target" && fact.role != "button" &&
         fact.role != "link") ||
        fact.label.size() > 128 ||
        !std::isfinite(fact.left) || !std::isfinite(fact.top) ||
        !std::isfinite(fact.width) || !std::isfinite(fact.height) ||
        !std::isfinite(fact.renderer_epoch_ms) || fact.width <= 0 ||
        fact.height <= 0 || fact.width > 4096 || fact.height > 4096 ||
        std::abs(fact.left) > 100000 || std::abs(fact.top) > 100000) {
      return true;
    }
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      fact.page_revision = page_revision_;
      const bool is_new = actions_.find(fact.action_id) == actions_.end();
      if (is_new) {
        if (pending_facts_.size() >= 256) {
          pending_facts_.pop_front();
        }
        pending_facts_.push_back(fact);
      }
      actions_[fact.action_id] = fact;
      while (actions_.size() > 256) {
        actions_.erase(actions_.begin());
      }
    }
    fact_cv_.notify_one();
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
      const auto action = actions_.find(receipt.action_id);
      if (action == actions_.end() || !std::isfinite(receipt.client_x) ||
          !std::isfinite(receipt.client_y) ||
          !std::isfinite(receipt.renderer_epoch_ms) ||
          dispatched_actions_.erase(receipt.action_id) != 1) {
        return true;
      }
      receipt.basis_page_revision = action->second.page_revision;
      receipt.observed_page_revision = page_revision_;
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
    if (browser_ && browser_->IsSame(browser)) {
      browser_ = browsers_.empty() ? nullptr : browsers_.begin()->second;
      next_frame = browser_ ? browser_->GetMainFrame() : nullptr;
      current_url_ = next_frame ? next_frame->GetURL().ToString() : "";
      current_title_.clear();
      ++page_revision_;
      ResetPageStateLocked("visible tab closed while command was pending");
      refresh_grant = started_ && browser_;
    }
    stop = browsers_.empty();
  }
  fact_cv_.notify_all();
  receipt_cv_.notify_all();
  form_cv_.notify_all();
  screenshot_cv_.notify_all();
  if (refresh_grant) {
    WriteGrant();
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
  bool auto_grant = false;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!configured_) {
      socket_path_ = socket_path;
      grant_path_ = grant_path;
      const char* replay_path = getenv("SACCADE_ENGINE_REPLAY_PATH");
      replay_path_ = replay_path && replay_path[0] != '\0'
                         ? replay_path
                         : grant_path_ + ".replay.jsonl";
      configured_ = true;
    }
    const char* granted = getenv("SACCADE_ENGINE_GRANT_CURRENT_TAB");
    auto_grant = granted && std::string(granted) == "1";
  }
  if (auto_grant) {
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
    paused_ = false;
    started_ = true;
    stopping_ = false;
    server_thread_ = std::thread(&SaccadeAdapter::Serve, this);
  }
}

SaccadeAdapter::AgentUiState SaccadeAdapter::ToggleAgentForVisibleTab() {
  CEF_REQUIRE_UI_THREAD();
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!configured_ || !browser_) {
      return AgentUiState::kUnavailable;
    }
    if (started_) {
      paused_ = !paused_;
      return paused_ ? AgentUiState::kPaused : AgentUiState::kOn;
    }
  }
  StartBridge();
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
  return paused_ ? AgentUiState::kPaused : AgentUiState::kOn;
}

void SaccadeAdapter::Stop() {
  if (!started_) {
    return;
  }
  stopping_ = true;
  fact_cv_.notify_all();
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
  bool has_initial_url = false;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    has_initial_url = !current_url_.empty();
  }
  if (has_initial_url && !WriteGrant()) {
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
  if (method == "truth") {
    return Response(id, TruthResult());
  }
  if (method == "actions") {
    return Response(id, ActionsResult());
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
  if (method == "form_inventory") {
    return FormCommandResponse(id, "inventory",
                               request->GetDictionary("params"));
  }
  if (method == "inspect_fields") {
    return FormCommandResponse(id, "inspect",
                               request->GetDictionary("params"));
  }
  if (method == "form_compile_plan") {
    return FormCommandResponse(id, "compile",
                               request->GetDictionary("params"));
  }
  if (method == "form_execute_plan") {
    return FormCommandResponse(id, "execute",
                               request->GetDictionary("params"));
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
  if (method == "reflex_start") {
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      if (paused_) {
        return ErrorResponse(id, "PERMISSION_DENIED", "agent is paused");
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
      if (paused_) {
        return ErrorResponse(id, "PERMISSION_DENIED", "agent is paused");
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
  if (method == "pause") {
    auto result = CefDictionaryValue::Create();
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      paused_ = true;
      result->SetBool("paused", true);
      result->SetDouble("page_revision", static_cast<double>(page_revision_));
    }
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
  result->SetString("url", current_url_);
  result->SetString("title", current_title_);
  result->SetDouble("page_revision", static_cast<double>(page_revision_));
  result->SetBool("paused", paused_);
  result->SetBool("collector_ready", collector_ready_);
  result->SetString("collector_error", collector_error_);
  result->SetString("tab_identity", CurrentTabIdLocked());
  result->SetInt("browser_count", static_cast<int>(browsers_.size()));
  int popup_count = 0;
  for (const auto& [browser_id, role] : browser_roles_) {
    if (role.is_popup) {
      ++popup_count;
    }
  }
  result->SetInt("popup_count", popup_count);
  const int current_id = browser_ ? browser_->GetIdentifier() : 0;
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

CefRefPtr<CefDictionaryValue> SaccadeAdapter::TruthResult() {
  auto result = CefDictionaryValue::Create();
  auto fields = CefListValue::Create();
  std::lock_guard<std::mutex> lock(state_mutex_);
  result->SetString("tab_id", CurrentTabIdLocked());
  result->SetString("url", current_url_);
  result->SetDouble("page_revision", static_cast<double>(page_revision_));
  result->SetBool("collector_ready", collector_ready_);
  result->SetString("collector_error", collector_error_);
  result->SetBool("sensitive_values_exposed", false);
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
  actions->SetSize(actions_.size());
  size_t index = 0;
  for (const auto& [action_id, fact] : actions_) {
    auto action = CefDictionaryValue::Create();
    action->SetString("action_id", action_id);
    action->SetString("kind", "pointer_click");
    action->SetString("role", fact.role);
    action->SetString("label", fact.label);
    action->SetDouble("basis_page_revision",
                      static_cast<double>(fact.page_revision));
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
  result->SetDouble("page_revision", static_cast<double>(fact.page_revision));
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
  result->SetDouble("client_x", receipt.client_x);
  result->SetDouble("client_y", receipt.client_y);
  result->SetInt("hits", receipt.hits);
  result->SetInt("misses", receipt.misses);
  result->SetBool("finished", receipt.finished);
  result->SetDouble("renderer_epoch_ms", receipt.renderer_epoch_ms);
  return Response(id, result);
}

std::string SaccadeAdapter::ActResponse(
    int id,
    CefRefPtr<CefDictionaryValue> params) {
  const std::string action_id =
      params ? params->GetString("action_id").ToString() : "";
  const uint64_t basis_page_revision = RequestRevision(params);
  TargetFact fact;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (paused_) {
      return ErrorResponse(id, "PERMISSION_DENIED", "agent is paused");
    }
    if (basis_page_revision == 0 || basis_page_revision != page_revision_) {
      return ErrorResponse(id, "STALE_PAGE_REVISION",
                           "action basis does not match current page");
    }
    const auto action = actions_.find(action_id);
    if (action == actions_.end() ||
        action->second.page_revision != basis_page_revision) {
      return ErrorResponse(id, "INVALID_ARGUMENT", "unknown action id");
    }
    if (!dispatched_actions_.insert(action_id).second) {
      return ErrorResponse(id, "INVALID_ARGUMENT",
                           "action id is already awaiting a receipt");
    }
    fact = action->second;
  }
  const int x = static_cast<int>(std::lround(fact.left + fact.width / 2.0));
  const int y = static_cast<int>(std::lround(fact.top + fact.height / 2.0));
  CefPostTask(TID_UI, base::BindOnce(&SaccadeAdapter::DispatchPointerOnUi,
                                    base::Unretained(this), x, y));
  auto result = CefDictionaryValue::Create();
  result->SetString("action_id", action_id);
  result->SetString("status", "accepted");
  result->SetDouble("basis_page_revision",
                    static_cast<double>(basis_page_revision));
  return Response(id, result);
}

std::string SaccadeAdapter::FormCommandResponse(
    int id,
    const std::string& command,
    CefRefPtr<CefDictionaryValue> params) {
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

  const bool revision_required = command != "inventory";
  const uint64_t basis_page_revision = RequestRevision(params);
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (paused_) {
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

  auto input = CefValue::Create();
  input->SetDictionary(params ? params->Copy(false)
                              : CefDictionaryValue::Create());
  const std::string input_json = JsonString(input);
  const int request_id = next_form_request_id_.fetch_add(1);
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    FormCommandState state;
    state.command = command;
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
  result->SetDouble("basis_page_revision",
                    static_cast<double>(state.basis_page_revision));
  result->SetDouble("page_revision", static_cast<double>(observed_revision));
  result->SetBool("sensitive_values_exposed", false);
  AppendValueFreeReplay("form_" + command, result,
                        state.basis_page_revision, observed_revision);
  return Response(id, result);
}

std::string SaccadeAdapter::TypeFieldTextResponse(
    int id,
    CefRefPtr<CefDictionaryValue> params) {
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

  auto focus_params = CefDictionaryValue::Create();
  focus_params->SetString("field_id", field_id);
  focus_params->SetDouble("basis_page_revision",
                          static_cast<double>(RequestRevision(params)));
  const std::string focus_response =
      FormCommandResponse(id, "focus_native_type", focus_params);
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
                             : "rich editor is not ready for native typing");
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
                           "rich-editor target changed before text insertion");
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
  verify_params->SetString("field_id", field_id);
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
        FormCommandResponse(id, "verify_native_type", verify_params);
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

void SaccadeAdapter::DispatchFormCommandOnUi(int request_id,
                                             std::string command,
                                             std::string input_json) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    browser = browser_;
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
  auto message = CefProcessMessage::Create("saccade.form.request_v1");
  auto arguments = message->GetArgumentList();
  arguments->SetInt(0, request_id);
  arguments->SetString(1, command);
  arguments->SetString(2, input_json);
  browser->GetMainFrame()->SendProcessMessage(PID_RENDERER, message);
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

void SaccadeAdapter::DispatchPointerOnUi(int x, int y) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    browser = browser_;
  }
  if (!browser) {
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
  grant->SetString("status", "granted");
  grant->SetString("grant_type", "current_tab_copilot");
  grant->SetBool("selected_tab_seen", true);
  grant->SetBool("grant_required", true);
  grant->SetBool("grant_given", true);
  grant->SetString("owner", "Human");
  grant->SetBool("agent_input_grant", true);
  grant->SetString("read_grant", "full_truth");
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    grant->SetString("url", current_url_);
    grant->SetString("tab_id", CurrentTabIdLocked());
  }
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
  return true;
}

void SaccadeAdapter::ResetPageStateLocked(const std::string& reason) {
  collector_ready_ = false;
  collector_error_.clear();
  controls_.clear();
  pending_facts_.clear();
  actions_.clear();
  dispatched_actions_.clear();
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
