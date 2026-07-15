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
  const std::array<const char*, 5> capabilities = {
      "ping", "shell_status", "navigate", "pause", "close"};
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
      refresh_grant = started_;
    }
  }
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
  result->SetString("tab_identity", "visible-primary");
  auto value = CefValue::Create();
  value->SetDictionary(result);
  return JsonString(value);
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
