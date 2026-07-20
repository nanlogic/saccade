// Copyright (c) 2026 Saccade contributors.

#ifndef SACCADE_CEF_HOST_SACCADE_AGENT_SWITCH_WIN_H_
#define SACCADE_CEF_HOST_SACCADE_AGENT_SWITCH_WIN_H_

#include <string>

#include "include/cef_browser.h"

struct SaccadeProtectedValuePromptResult {
  bool confirmed = false;
  std::string value;
};

void SaccadeUpdateAgentSwitch(CefRefPtr<CefBrowser> browser, int state);
void SaccadeShowHumanVerificationFailure(CefRefPtr<CefBrowser> browser,
                                         const std::string& provider);
SaccadeProtectedValuePromptResult SaccadePromptProtectedValue(
    CefRefPtr<CefBrowser> browser,
    const std::string& page_origin,
    const std::string& field_label);

#endif  // SACCADE_CEF_HOST_SACCADE_AGENT_SWITCH_WIN_H_
