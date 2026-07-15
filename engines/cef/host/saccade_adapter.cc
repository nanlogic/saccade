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
#include <cstring>
#include <utility>

#include "include/base/cef_callback.h"
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
  const std::array<const char*, 11> capabilities = {
      "ping",        "shell_status", "navigate",    "pause",
      "close",       "truth",        "actions",     "next_fact",
      "act",         "next_receipt", "reflex_start"};
  list->SetSize(capabilities.size());
  for (size_t index = 0; index < capabilities.size(); ++index) {
    list->SetString(index, capabilities[index]);
  }
  return list;
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

}  // namespace

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
    if (browser_) {
      return;
    }
    browser_ = browser;
    if (browser->GetMainFrame()) {
      current_url_ = browser->GetMainFrame()->GetURL().ToString();
    }
  }
  StartIfRequested();
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
      collector_ready_ = false;
      collector_error_.clear();
      controls_.clear();
      pending_facts_.clear();
      actions_.clear();
      dispatched_actions_.clear();
      pending_receipts_.clear();
      refresh_grant = started_;
    }
  }
  if (refresh_grant) {
    WriteGrant();
  }
  auto refresh = CefProcessMessage::Create("saccade.collector.refresh_v1");
  frame->SendProcessMessage(PID_RENDERER, refresh);
}

void SaccadeAdapter::OnTitleChanged(CefRefPtr<CefBrowser> browser,
                                    const CefString& title) {
  CEF_REQUIRE_UI_THREAD();
  std::lock_guard<std::mutex> lock(state_mutex_);
  if (browser_ && browser_->IsSame(browser)) {
    current_title_ = title.ToString();
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

  if (name == "saccade.renderer.ready_v1" && arguments->GetSize() == 1) {
    {
      std::lock_guard<std::mutex> lock(state_mutex_);
      collector_ready_ = true;
    }
    fact_cv_.notify_all();
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
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    if (!browser_ || !browser_->IsSame(browser)) {
      return;
    }
    browser_ = nullptr;
  }
  Stop();
}

void SaccadeAdapter::StartIfRequested() {
  const char* socket_path = getenv("SACCADE_ENGINE_SOCKET");
  const char* grant_path = getenv("SACCADE_ENGINE_GRANT_PATH");
  const char* granted = getenv("SACCADE_ENGINE_GRANT_CURRENT_TAB");
  if (!socket_path || !grant_path || !granted || std::string(granted) != "1") {
    return;
  }

  std::lock_guard<std::mutex> lock(state_mutex_);
  if (started_) {
    return;
  }
  socket_path_ = socket_path;
  grant_path_ = grant_path;
  capability_ = RandomCapability();
  if (capability_.empty()) {
    return;
  }
  started_ = true;
  stopping_ = false;
  server_thread_ = std::thread(&SaccadeAdapter::Serve, this);
}

void SaccadeAdapter::Stop() {
  if (!started_) {
    return;
  }
  stopping_ = true;
  fact_cv_.notify_all();
  receipt_cv_.notify_all();
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
  result->SetString("tab_identity", "visible-primary");
  auto value = CefValue::Create();
  value->SetDictionary(result);
  return JsonString(value);
}

CefRefPtr<CefDictionaryValue> SaccadeAdapter::TruthResult() {
  auto result = CefDictionaryValue::Create();
  auto fields = CefListValue::Create();
  std::lock_guard<std::mutex> lock(state_mutex_);
  result->SetString("tab_id", "visible-primary");
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
  result->SetString("tab_id", "visible-primary");
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

void SaccadeAdapter::NavigateOnUi(std::string url) {
  CEF_REQUIRE_UI_THREAD();
  CefRefPtr<CefBrowser> browser;
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    browser = browser_;
    current_url_ = url;
    ++page_revision_;
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
  }
  grant->SetDictionary("engine_adapter", adapter);
  grant->SetDictionary("control_endpoint", endpoint);
  grant->SetDictionary("control_capability", session_capability);

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
