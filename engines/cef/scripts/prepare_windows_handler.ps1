[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Header,
  [Parameter(Mandatory = $true)][string]$Implementation
)

$ErrorActionPreference = 'Stop'

function Replace-Exact {
  param([ref]$Text, [string]$Old, [string]$New, [string]$Label)
  $Old = $Old.Replace("`r`n", "`n")
  $New = $New.Replace("`r`n", "`n")
  if (-not $Text.Value.Contains($Old)) {
    throw "Windows handler transform lost expected $Label fragment"
  }
  $Text.Value = $Text.Value.Replace($Old, $New)
}

$headerText = [System.IO.File]::ReadAllText($Header).Replace("`r`n", "`n")
if (-not $headerText.Contains('GetDownloadHandler()')) {
  Replace-Exact ([ref]$headerText) @'
#include "include/cef_client.h"
'@ @'
#include "include/cef_client.h"
#include "include/cef_download_handler.h"
#include "include/cef_focus_handler.h"
#include "include/cef_request_handler.h"
#include "include/cef_resource_request_handler.h"
'@ 'includes'
  Replace-Exact ([ref]$headerText) @'
class SimpleHandler : public CefClient,
                      public CefDisplayHandler,
                      public CefLifeSpanHandler,
                      public CefLoadHandler {
'@ @'
class SimpleHandler : public CefClient,
                      public CefDisplayHandler,
                      public CefDownloadHandler,
                      public CefLifeSpanHandler,
                      public CefLoadHandler,
                      public CefFocusHandler,
                      public CefRequestHandler,
                      public CefResourceRequestHandler {
'@ 'inheritance'
  Replace-Exact ([ref]$headerText) @'
  CefRefPtr<CefDisplayHandler> GetDisplayHandler() override { return this; }
  CefRefPtr<CefLifeSpanHandler> GetLifeSpanHandler() override { return this; }
  CefRefPtr<CefLoadHandler> GetLoadHandler() override { return this; }

  // CefDisplayHandler methods:
'@ @'
  CefRefPtr<CefDisplayHandler> GetDisplayHandler() override { return this; }
  CefRefPtr<CefDownloadHandler> GetDownloadHandler() override { return this; }
  CefRefPtr<CefLifeSpanHandler> GetLifeSpanHandler() override { return this; }
  CefRefPtr<CefLoadHandler> GetLoadHandler() override { return this; }
  CefRefPtr<CefFocusHandler> GetFocusHandler() override { return this; }
  CefRefPtr<CefRequestHandler> GetRequestHandler() override { return this; }
  bool OnProcessMessageReceived(CefRefPtr<CefBrowser> browser,
                                CefRefPtr<CefFrame> frame,
                                CefProcessId source_process,
                                CefRefPtr<CefProcessMessage> message) override;

  // CefDownloadHandler methods.
  bool OnBeforeDownload(CefRefPtr<CefBrowser> browser,
                        CefRefPtr<CefDownloadItem> download_item,
                        const CefString& suggested_name,
                        CefRefPtr<CefBeforeDownloadCallback> callback) override;
  void OnDownloadUpdated(CefRefPtr<CefBrowser> browser,
                         CefRefPtr<CefDownloadItem> download_item,
                         CefRefPtr<CefDownloadItemCallback> callback) override;

  // CefDisplayHandler methods:
'@ 'client methods'
  Replace-Exact ([ref]$headerText) @'
  void OnTitleChange(CefRefPtr<CefBrowser> browser,
                     const CefString& title) override;
'@ @'
  void OnTitleChange(CefRefPtr<CefBrowser> browser,
                     const CefString& title) override;
  void OnAddressChange(CefRefPtr<CefBrowser> browser,
                       CefRefPtr<CefFrame> frame,
                       const CefString& url) override;
'@ 'display methods'
  Replace-Exact ([ref]$headerText) @'
  void OnBeforeClose(CefRefPtr<CefBrowser> browser) override;

  // CefLoadHandler methods:
'@ @'
  void OnBeforeClose(CefRefPtr<CefBrowser> browser) override;
  bool OnBeforePopup(
      CefRefPtr<CefBrowser> browser,
      CefRefPtr<CefFrame> frame,
      int popup_id,
      const CefString& target_url,
      const CefString& target_frame_name,
      CefLifeSpanHandler::WindowOpenDisposition target_disposition,
      bool user_gesture,
      const CefPopupFeatures& popupFeatures,
      CefWindowInfo& windowInfo,
      CefRefPtr<CefClient>& client,
      CefBrowserSettings& settings,
      CefRefPtr<CefDictionaryValue>& extra_info,
      bool* no_javascript_access) override;

  // CefRequestHandler methods.
  bool OnOpenURLFromTab(
      CefRefPtr<CefBrowser> browser,
      CefRefPtr<CefFrame> frame,
      const CefString& target_url,
      CefRequestHandler::WindowOpenDisposition target_disposition,
      bool user_gesture) override;
  CefRefPtr<CefResourceRequestHandler> GetResourceRequestHandler(
      CefRefPtr<CefBrowser> browser,
      CefRefPtr<CefFrame> frame,
      CefRefPtr<CefRequest> request,
      bool is_navigation,
      bool is_download,
      const CefString& request_initiator,
      bool& disable_default_handling) override;

  // CefResourceRequestHandler methods.
  void OnResourceLoadComplete(CefRefPtr<CefBrowser> browser,
                              CefRefPtr<CefFrame> frame,
                              CefRefPtr<CefRequest> request,
                              CefRefPtr<CefResponse> response,
                              URLRequestStatus status,
                              int64_t received_content_length) override;

  // CefLoadHandler methods:
  void OnLoadingStateChange(CefRefPtr<CefBrowser> browser,
                            bool isLoading,
                            bool canGoBack,
                            bool canGoForward) override;
'@ 'life/request methods'
  Replace-Exact ([ref]$headerText) @'
  void ShowMainWindow();
'@ @'
  // CefFocusHandler methods.
  void OnGotFocus(CefRefPtr<CefBrowser> browser) override;

  void ShowMainWindow();
'@ 'focus method'
  [System.IO.File]::WriteAllText($Header, $headerText,
    [System.Text.UTF8Encoding]::new($false))
}

$cc = [System.IO.File]::ReadAllText($Implementation).Replace("`r`n", "`n")
if (-not $cc.Contains('OnResourceLoadComplete(')) {
  Replace-Exact ([ref]$cc) @'
#include <sstream>
#include <string>
'@ @'
#include <algorithm>
#include <cctype>
#include <cstdlib>
#include <sstream>
#include <string>
'@ 'implementation standard includes'
  Replace-Exact ([ref]$cc) @'
#include "include/wrapper/cef_helpers.h"
'@ @'
#include "include/wrapper/cef_helpers.h"
#include "tests/cefsimple/saccade_adapter.h"
'@ 'adapter include'
  Replace-Exact ([ref]$cc) @'
std::string GetDataURI(const std::string& data, const std::string& mime_type) {
  return "data:" + mime_type + ";base64," +
         CefURIEncode(CefBase64Encode(data.data(), data.size()), false)
             .ToString();
}

}  // namespace
'@ @'
std::string GetDataURI(const std::string& data, const std::string& mime_type) {
  return "data:" + mime_type + ";base64," +
         CefURIEncode(CefBase64Encode(data.data(), data.size()), false)
             .ToString();
}

bool IsSaccadeTabDisposition(cef_window_open_disposition_t disposition) {
  return disposition == CEF_WOD_NEW_FOREGROUND_TAB ||
         disposition == CEF_WOD_NEW_BACKGROUND_TAB;
}

std::string SafeSuggestedDownloadName(const CefString& suggested_name) {
  std::string name = suggested_name.ToString();
  const size_t separator = name.find_last_of("/\\");
  if (separator != std::string::npos) name = name.substr(separator + 1);
  return name.empty() || name == "." || name == ".." ? "download" : name;
}

bool HostMatchesDomain(const std::string& host, const std::string& domain) {
  return host == domain ||
         (host.size() > domain.size() &&
          host.compare(host.size() - domain.size(), domain.size(), domain) == 0 &&
          host[host.size() - domain.size() - 1] == '.');
}

std::string HumanVerificationProviderForRequest(CefRefPtr<CefRequest> request) {
  if (!request) return "";
  CefURLParts parts;
  if (!CefParseURL(request->GetURL(), parts)) return "";
  std::string host = CefString(&parts.host).ToString();
  const std::string path = CefString(&parts.path).ToString();
  std::transform(host.begin(), host.end(), host.begin(),
                 [](unsigned char value) { return std::tolower(value); });
  if (path.find("/fc/gt2/public_key/") == std::string::npos) return "";
  if (HostMatchesDomain(host, "arkoselabs.com") ||
      HostMatchesDomain(host, "octocaptcha.com")) return "Arkose Labs";
  return "";
}

}  // namespace
'@ 'helpers'
  Replace-Exact ([ref]$cc) @'
SimpleHandler* SimpleHandler::GetInstance() {
  return g_instance;
}

void SimpleHandler::OnTitleChange
'@ @'
SimpleHandler* SimpleHandler::GetInstance() {
  return g_instance;
}

bool SimpleHandler::OnProcessMessageReceived(
    CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame,
    CefProcessId source_process, CefRefPtr<CefProcessMessage> message) {
  CEF_REQUIRE_UI_THREAD();
  return SaccadeAdapter::GetInstance()->OnRendererMessage(
      browser, frame, source_process, message);
}

bool SimpleHandler::OnBeforeDownload(
    CefRefPtr<CefBrowser> browser, CefRefPtr<CefDownloadItem> download_item,
    const CefString& suggested_name,
    CefRefPtr<CefBeforeDownloadCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  SaccadeAdapter::GetInstance()->OnDownloadUpdated(browser, download_item);
  const char* forced_directory = std::getenv("SACCADE_DOWNLOAD_DIR");
  if (forced_directory && forced_directory[0] != '\0') {
    std::string path(forced_directory);
    if (path.back() != '/' && path.back() != '\\') path.push_back('\\');
    path.append(SafeSuggestedDownloadName(suggested_name));
    callback->Continue(path, false);
    return true;
  }
  return false;
}

void SimpleHandler::OnDownloadUpdated(
    CefRefPtr<CefBrowser> browser, CefRefPtr<CefDownloadItem> download_item,
    CefRefPtr<CefDownloadItemCallback> callback) {
  CEF_REQUIRE_UI_THREAD();
  SaccadeAdapter::GetInstance()->OnDownloadUpdated(browser, download_item);
}

void SimpleHandler::OnTitleChange
'@ 'client callbacks'
  Replace-Exact ([ref]$cc) @'
  } else if (is_alloy_style_) {
    // Set the title of the window using platform APIs.
    PlatformTitleChange(browser, title);
  }
}

void SimpleHandler::OnAfterCreated
'@ @'
  } else if (is_alloy_style_) {
    // Set the title of the window using platform APIs.
    PlatformTitleChange(browser, title);
  }
  SaccadeAdapter::GetInstance()->OnTitleChanged(browser, title);
}

void SimpleHandler::OnAddressChange(CefRefPtr<CefBrowser> browser,
                                    CefRefPtr<CefFrame> frame,
                                    const CefString& url) {
  CEF_REQUIRE_UI_THREAD();
  SaccadeAdapter::GetInstance()->OnAddressChanged(browser, frame, url);
}

void SimpleHandler::OnAfterCreated
'@ 'display callbacks'
  Replace-Exact ([ref]$cc) @'
  browser_list_.push_back(browser);
}

bool SimpleHandler::DoClose
'@ @'
  browser_list_.push_back(browser);
  SaccadeAdapter::GetInstance()->OnBrowserCreated(browser);
}

bool SimpleHandler::DoClose
'@ 'browser created'
  Replace-Exact ([ref]$cc) @'
  return false;
}

void SimpleHandler::OnBeforeClose
'@ @'
  return false;
}

bool SimpleHandler::OnBeforePopup(
    CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame, int popup_id,
    const CefString& target_url, const CefString& target_frame_name,
    CefLifeSpanHandler::WindowOpenDisposition target_disposition,
    bool user_gesture, const CefPopupFeatures& popupFeatures,
    CefWindowInfo& windowInfo, CefRefPtr<CefClient>& client,
    CefBrowserSettings& settings, CefRefPtr<CefDictionaryValue>& extra_info,
    bool* no_javascript_access) {
  CEF_REQUIRE_UI_THREAD();
  if (IsSaccadeTabDisposition(target_disposition) &&
      !target_url.ToString().empty()) {
    SaccadeAdapter::GetInstance()->OpenRoutedTabOnUi(target_url.ToString());
    return true;
  }
  return false;
}

void SimpleHandler::OnBeforeClose
'@ 'popup callback'
  Replace-Exact ([ref]$cc) @'
void SimpleHandler::OnBeforeClose(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();

'@ @'
void SimpleHandler::OnBeforeClose(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();
  SaccadeAdapter::GetInstance()->OnBrowserClosed(browser);

'@ 'browser closed'
  Replace-Exact ([ref]$cc) @'
void SimpleHandler::OnLoadError(CefRefPtr<CefBrowser> browser,
'@ @'
bool SimpleHandler::OnOpenURLFromTab(
    CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame,
    const CefString& target_url,
    CefRequestHandler::WindowOpenDisposition target_disposition,
    bool user_gesture) {
  CEF_REQUIRE_UI_THREAD();
  if (IsSaccadeTabDisposition(target_disposition) &&
      !target_url.ToString().empty()) {
    SaccadeAdapter::GetInstance()->OpenRoutedTabOnUi(target_url.ToString());
    return true;
  }
  return false;
}

CefRefPtr<CefResourceRequestHandler> SimpleHandler::GetResourceRequestHandler(
    CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame,
    CefRefPtr<CefRequest> request, bool is_navigation, bool is_download,
    const CefString& request_initiator, bool& disable_default_handling) {
  return HumanVerificationProviderForRequest(request).empty() ? nullptr : this;
}

void SimpleHandler::OnResourceLoadComplete(
    CefRefPtr<CefBrowser> browser, CefRefPtr<CefFrame> frame,
    CefRefPtr<CefRequest> request, CefRefPtr<CefResponse> response,
    URLRequestStatus status, int64_t received_content_length) {
  const std::string provider = HumanVerificationProviderForRequest(request);
  if (!provider.empty()) {
    SaccadeAdapter::GetInstance()->OnHumanVerificationResourceResult(
        browser, provider, response ? response->GetStatus() : 0,
        static_cast<int>(status));
  }
}

void SimpleHandler::OnLoadingStateChange(CefRefPtr<CefBrowser> browser,
                                         bool isLoading, bool canGoBack,
                                         bool canGoForward) {
  CEF_REQUIRE_UI_THREAD();
  if (!isLoading) SaccadeAdapter::GetInstance()->OnLoadCompleted(browser);
}

void SimpleHandler::OnLoadError(CefRefPtr<CefBrowser> browser,
'@ 'request/load callbacks'
  Replace-Exact ([ref]$cc) @'
void SimpleHandler::ShowMainWindow() {
'@ @'
void SimpleHandler::OnGotFocus(CefRefPtr<CefBrowser> browser) {
  CEF_REQUIRE_UI_THREAD();
  SaccadeAdapter::GetInstance()->OnBrowserFocused(browser);
}

void SimpleHandler::ShowMainWindow() {
'@ 'focus callback'
  [System.IO.File]::WriteAllText($Implementation, $cc,
    [System.Text.UTF8Encoding]::new($false))
}
