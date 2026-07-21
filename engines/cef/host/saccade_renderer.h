// Copyright (c) 2026 Saccade contributors.
// Use of this source code is governed by a BSD-style license.

#ifndef SACCADE_CEF_HOST_SACCADE_RENDERER_H_
#define SACCADE_CEF_HOST_SACCADE_RENDERER_H_

#include <string>
#include <map>
#include <utility>

#include "include/cef_app.h"
#include "include/cef_render_process_handler.h"

// Renderer-process half of the bounded Day 3 truth/reflex bridge.
class SaccadeRendererApp : public CefApp, public CefRenderProcessHandler {
 public:
  SaccadeRendererApp() = default;

  CefRefPtr<CefRenderProcessHandler> GetRenderProcessHandler() override {
    return this;
  }

  void OnContextCreated(CefRefPtr<CefBrowser> browser,
                        CefRefPtr<CefFrame> frame,
                        CefRefPtr<CefV8Context> context) override;
  void OnContextReleased(CefRefPtr<CefBrowser> browser,
                         CefRefPtr<CefFrame> frame,
                         CefRefPtr<CefV8Context> context) override;
  bool OnProcessMessageReceived(CefRefPtr<CefBrowser> browser,
                                CefRefPtr<CefFrame> frame,
                                CefProcessId source_process,
                                CefRefPtr<CefProcessMessage> message) override;

 private:
  struct FormCommandClosure {
    CefRefPtr<CefV8Context> context;
    CefRefPtr<CefV8Value> function;
  };

  using FormCommandKey = std::pair<int, std::string>;
  std::map<FormCommandKey, FormCommandClosure> form_command_closures_;

  IMPLEMENT_REFCOUNTING(SaccadeRendererApp);
};

#endif  // SACCADE_CEF_HOST_SACCADE_RENDERER_H_
