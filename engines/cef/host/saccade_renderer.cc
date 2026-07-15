// Copyright (c) 2026 Saccade contributors.
// Use of this source code is governed by a BSD-style license.

#include "tests/cefsimple/saccade_renderer.h"

#include <cmath>
#include <cstdlib>
#include <string>

#include "include/cef_process_message.h"
#include "include/cef_v8.h"
#include "include/wrapper/cef_helpers.h"

namespace {

constexpr char kNativeEmitName[] = "__saccadeEmitNative";
constexpr char kStartMessage[] = "saccade.reflex.start_v1";
constexpr char kRefreshMessage[] = "saccade.collector.refresh_v1";

constexpr char kCollectorScript[] = R"SACCADE_JS(
(() => {
  const nativeEmit = globalThis.__saccadeEmitNative;
  delete globalThis.__saccadeEmitNative;
  if (typeof nativeEmit !== 'function') return false;

  const epochNow = () => performance.timeOrigin + performance.now();
  const safeRun = (stage, callback) => {
    try {
      callback();
    } catch (_) {
      nativeEmit('collector_error', stage, epochNow());
    }
  };
  const ids = new WeakMap();
  const requestedCount = Math.max(
      1, Number(new URLSearchParams(location.search).get('count') || 30));
  let sequence = 0;
  let attached = false;

  const queryAll = Function.call.bind(Document.prototype.querySelectorAll);
  const rectFor = Function.call.bind(Element.prototype.getBoundingClientRect);
  const closest = Function.call.bind(Element.prototype.closest);
  const matches = Function.call.bind(Element.prototype.matches);
  const addEvent = Function.call.bind(EventTarget.prototype.addEventListener);
  const styleFor = Function.call.bind(window.getComputedStyle, window);
  const actionSelector = '.target:not(.hit), button, a[href], [role="button"], input[type="button"], input[type="submit"]';
  let pendingInput = null;

  const scanControls = () => {
    for (const element of queryAll(document, 'input, textarea, select, [contenteditable="true"]')) {
      const type = String(element.getAttribute('type') || element.tagName || 'control').toLowerCase();
      const identity = String(element.getAttribute('id') || element.getAttribute('name') || type).slice(0, 128);
      const markers = [
        type,
        identity,
        element.getAttribute('autocomplete') || '',
        element.getAttribute('data-sensitive') || ''
      ].join(' ').toLowerCase();
      const sensitive = /password|passcode|ssn|social.security|credit|card|cvv|cvc|government|passport|tax.id/.test(markers);
      const complete = typeof element.value === 'string' ? element.value.length > 0 : false;
      nativeEmit('control', identity, type, sensitive, complete, epochNow());
    }
  };

  const scanActions = () => {
    for (const element of queryAll(document, actionSelector)) {
      if (element.disabled || element.getAttribute('aria-disabled') === 'true') continue;
      const rect = rectFor(element);
      if (!Number.isFinite(rect.left) || !Number.isFinite(rect.top) ||
          rect.width <= 0 || rect.height <= 0) continue;
      if (innerWidth > 0 && innerHeight > 0 &&
          (rect.right <= 0 || rect.bottom <= 0 || rect.left >= innerWidth ||
           rect.top >= innerHeight)) continue;
      const style = styleFor(element);
      if (style.display === 'none' || style.visibility === 'hidden' ||
          style.pointerEvents === 'none' || Number(style.opacity) === 0) continue;
      const role = matches(element, '.target:not(.hit)')
          ? 'target'
          : (matches(element, 'a[href]') ? 'link' : 'button');
      const label = String(
          element.getAttribute('aria-label') || element.getAttribute('title') ||
          element.innerText || element.textContent || element.value ||
          element.getAttribute('id') ||
          element.getAttribute('name') || role)
          .replace(/\s+/g, ' ').trim().slice(0, 128);
      let actionId = ids.get(element);
      if (!actionId) {
        actionId = `${role}-${++sequence}`;
        ids.set(element, actionId);
      }
      nativeEmit('action', actionId, role, label, rect.left, rect.top,
                 rect.width, rect.height, epochNow());
    }
  };

  const attach = () => {
    if (attached || !document.documentElement) return;
    attached = true;
    nativeEmit('ready', epochNow());
    const observer = new MutationObserver(() => safeRun('scan_actions', scanActions));
    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['class', 'style']
    });
    safeRun('scan_controls', scanControls);
    safeRun('scan_actions', scanActions);
  };

  globalThis.__saccadeCollectorRefresh = () => {
    nativeEmit('ready', epochNow());
    safeRun('scan_controls', scanControls);
    safeRun('scan_actions', scanActions);
    return true;
  };

  addEvent(document, 'mousedown', event => {
    const target = event.target instanceof Element ? closest(event.target, actionSelector) : null;
    const actionId = target ? ids.get(target) : null;
    if (!actionId) return;
    pendingInput = {actionId, clientX: event.clientX, clientY: event.clientY};
  }, true);

  addEvent(document, 'mouseup', () => {
    if (!pendingInput) return;
    const {actionId, clientX, clientY} = pendingInput;
    pendingInput = null;
    const truth = document.getElementById('truth');
    const text = truth ? String(truth.textContent || '') : '';
    const hits = Number((/hits=(\d+)/.exec(text) || [0, 0])[1]);
    const misses = Number((/misses=(\d+)/.exec(text) || [0, 0])[1]);
    const finished = /finished=true/.test(text) || hits >= requestedCount;
    nativeEmit('receipt', actionId, clientX, clientY, hits, misses, finished, epochNow());
  }, true);

  if (document.documentElement) {
    attach();
  } else {
    addEvent(document, 'readystatechange', attach, true);
    addEvent(document, 'DOMContentLoaded', attach, true);
  }
  return true;
})()
)SACCADE_JS";

bool NumberArgument(const CefV8ValueList& arguments, size_t index) {
  return index < arguments.size() && arguments[index] &&
         (arguments[index]->IsInt() || arguments[index]->IsUInt() ||
          arguments[index]->IsDouble());
}

class EmitHandler : public CefV8Handler {
 public:
  bool Execute(const CefString& name,
               CefRefPtr<CefV8Value> object,
               const CefV8ValueList& arguments,
               CefRefPtr<CefV8Value>& retval,
               CefString& exception) override {
    CEF_REQUIRE_RENDERER_THREAD();
    auto context = CefV8Context::GetCurrentContext();
    auto frame = context ? context->GetFrame() : nullptr;
    if (name != kNativeEmitName || !frame || !frame->IsMain() ||
        arguments.empty() || !arguments[0]->IsString()) {
      exception = "invalid Saccade renderer emission";
      return true;
    }

    const std::string kind = arguments[0]->GetStringValue().ToString();
    auto message = CefProcessMessage::Create("saccade.renderer." + kind + "_v1");
    auto output = message->GetArgumentList();
    if (kind == "ready" && arguments.size() == 2 &&
        NumberArgument(arguments, 1)) {
      output->SetDouble(0, arguments[1]->GetDoubleValue());
    } else if (kind == "control" && arguments.size() == 6 &&
               arguments[1]->IsString() && arguments[2]->IsString() &&
               arguments[3]->IsBool() && arguments[4]->IsBool() &&
               NumberArgument(arguments, 5)) {
      output->SetString(0, arguments[1]->GetStringValue());
      output->SetString(1, arguments[2]->GetStringValue());
      output->SetBool(2, arguments[3]->GetBoolValue());
      output->SetBool(3, arguments[4]->GetBoolValue());
      output->SetDouble(4, arguments[5]->GetDoubleValue());
    } else if (kind == "action" && arguments.size() == 9 &&
               arguments[1]->IsString() && arguments[2]->IsString() &&
               arguments[3]->IsString() && NumberArgument(arguments, 4) &&
               NumberArgument(arguments, 5) && NumberArgument(arguments, 6) &&
               NumberArgument(arguments, 7) && NumberArgument(arguments, 8)) {
      output->SetString(0, arguments[1]->GetStringValue());
      output->SetString(1, arguments[2]->GetStringValue());
      output->SetString(2, arguments[3]->GetStringValue());
      for (size_t index = 4; index <= 8; ++index) {
        const double value = arguments[index]->GetDoubleValue();
        if (!std::isfinite(value)) {
          exception = "non-finite Saccade geometry";
          return true;
        }
        output->SetDouble(index - 1, value);
      }
    } else if (kind == "collector_error" && arguments.size() == 3 &&
               arguments[1]->IsString() && NumberArgument(arguments, 2)) {
      output->SetString(0, arguments[1]->GetStringValue());
      output->SetDouble(1, arguments[2]->GetDoubleValue());
    } else if (kind == "receipt" && arguments.size() == 8 &&
               arguments[1]->IsString() && NumberArgument(arguments, 2) &&
               NumberArgument(arguments, 3) && NumberArgument(arguments, 4) &&
               NumberArgument(arguments, 5) && arguments[6]->IsBool() &&
               NumberArgument(arguments, 7)) {
      output->SetString(0, arguments[1]->GetStringValue());
      output->SetDouble(1, arguments[2]->GetDoubleValue());
      output->SetDouble(2, arguments[3]->GetDoubleValue());
      output->SetInt(3, arguments[4]->GetIntValue());
      output->SetInt(4, arguments[5]->GetIntValue());
      output->SetBool(5, arguments[6]->GetBoolValue());
      output->SetDouble(6, arguments[7]->GetDoubleValue());
    } else {
      exception = "unsupported Saccade renderer emission";
      return true;
    }

    frame->SendProcessMessage(PID_BROWSER, message);
    retval = CefV8Value::CreateBool(true);
    return true;
  }

 private:
  IMPLEMENT_REFCOUNTING(EmitHandler);
};

}  // namespace

void SaccadeRendererApp::OnContextCreated(CefRefPtr<CefBrowser> browser,
                                          CefRefPtr<CefFrame> frame,
                                          CefRefPtr<CefV8Context> context) {
  CEF_REQUIRE_RENDERER_THREAD();
  const char* reflex_gate = std::getenv("SACCADE_REFLEX_GATE");
  if (!frame->IsMain() || !reflex_gate || std::string(reflex_gate) != "1") {
    return;
  }

  auto global = context->GetGlobal();
  auto handler = CefRefPtr<EmitHandler>(new EmitHandler());
  global->SetValue(kNativeEmitName,
                   CefV8Value::CreateFunction(kNativeEmitName, handler),
                   V8_PROPERTY_ATTRIBUTE_DONTENUM);
  CefRefPtr<CefV8Value> result;
  CefRefPtr<CefV8Exception> exception;
  context->Eval(kCollectorScript, "saccade://renderer/collector.js", 1,
                result, exception);
}

bool SaccadeRendererApp::OnProcessMessageReceived(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    CefProcessId source_process,
    CefRefPtr<CefProcessMessage> message) {
  CEF_REQUIRE_RENDERER_THREAD();
  if (source_process != PID_BROWSER || !frame->IsMain() ||
      (message->GetName() != kStartMessage &&
       message->GetName() != kRefreshMessage)) {
    return false;
  }
  auto context = frame->GetV8Context();
  if (!context || !context->Enter()) {
    return true;
  }
  CefRefPtr<CefV8Value> result;
  CefRefPtr<CefV8Exception> exception;
  const bool refresh = message->GetName() == kRefreshMessage;
  context->Eval(refresh
                    ? "typeof window.__saccadeCollectorRefresh === 'function' && "
                      "window.__saccadeCollectorRefresh()"
                    : "typeof window.__saccadeStart === 'function' && "
                      "window.__saccadeStart()",
                refresh ? "saccade://renderer/refresh.js"
                        : "saccade://renderer/start.js",
                1, result, exception);
  context->Exit();
  return true;
}
