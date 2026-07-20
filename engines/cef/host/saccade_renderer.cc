// Copyright (c) 2026 Saccade contributors.
// Use of this source code is governed by a BSD-style license.

#include "tests/cefsimple/saccade_renderer.h"

#include <cmath>
#include <cstdlib>
#include <string>

#include "include/cef_process_message.h"
#include "include/cef_v8.h"
#include "include/wrapper/cef_helpers.h"
#include "tests/cefsimple/saccade_form_script.h"

namespace {

constexpr char kNativeEmitName[] = "__saccadeEmitNative";
constexpr char kStartMessage[] = "saccade.reflex.start_v1";
constexpr char kRefreshMessage[] = "saccade.collector.refresh_v1";
constexpr char kFormRequestMessage[] = "saccade.form.request_v1";
constexpr char kFormResponseMessage[] = "saccade.renderer.form_response_v1";

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
  const canonicalIds = new Map();
  const requestedCount = Math.max(
      1, Number(new URLSearchParams(location.search).get('count') || 30));
  let sequence = 0;
  let actionScanGeneration = 0;
  let attached = false;
  let lastLayoutSignature = '';
  let layoutRefreshPending = false;

  const queryAll = Function.call.bind(Document.prototype.querySelectorAll);
  const getById = Function.call.bind(Document.prototype.getElementById);
  const rectFor = Function.call.bind(Element.prototype.getBoundingClientRect);
  const closest = Function.call.bind(Element.prototype.closest);
  const matches = Function.call.bind(Element.prototype.matches);
  const queryOne = Function.call.bind(Element.prototype.querySelector);
  const attr = Function.call.bind(Element.prototype.getAttribute);
  const addEvent = Function.call.bind(EventTarget.prototype.addEventListener);
  const elementAt = Function.call.bind(Document.prototype.elementFromPoint);
  const styleFor = Function.call.bind(window.getComputedStyle, window);
  const actionSelector = '.target:not(.hit), button, a[href], canvas, [role="button"], input[type="button"], input[type="submit"]';
  let pendingInput = null;

  const scanControls = () => {
    nativeEmit('controls_reset', epochNow());
    for (const element of queryAll(document, 'input, textarea, select, [contenteditable="true"]')) {
      const type = String(attr(element, 'type') || element.tagName || 'control').toLowerCase();
      const identity = String(attr(element, 'id') || attr(element, 'name') || type).slice(0, 128);
      const markers = [
        type,
        identity,
        attr(element, 'autocomplete') || '',
        attr(element, 'data-sensitive') || ''
      ].join(' ').toLowerCase();
      const sensitive = /password|passcode|ssn|social.security|credit|card|cvv|cvc|government|passport|tax.id/.test(markers);
      const complete = typeof element.value === 'string' ? element.value.length > 0 : false;
      nativeEmit('control', identity, type, sensitive, complete, epochNow());
    }
  };

  const sensitiveControlValues = () => {
    const values = [];
    for (const element of queryAll(document, 'input, textarea, select')) {
      const type = String(attr(element, 'type') || '').toLowerCase();
      if (type === 'checkbox' || type === 'radio') continue;
      const markers = [type, attr(element, 'autocomplete') || '',
        attr(element, 'name') || '', attr(element, 'id') || '',
        attr(element, 'data-sensitive') || ''].join(' ').toLowerCase();
      if (!/password|passcode|otp|one[-_ ]?time|ssn|social.security|credit|card|cvv|cvc|government|passport|tax.id|signature/.test(markers)) continue;
      const value = String(element.value || '');
      if (value && (value.length >= 4 || type === 'password')) values.push(value);
    }
    return values;
  };

  const redactActionLabel = raw => {
    let value = String(raw || '');
    for (const protectedValue of sensitiveControlValues()) {
      value = value.split(protectedValue).join('[redacted]');
    }
    return value
      .replace(/\b\d{3}-\d{2}-\d{4}\b/g, '[redacted]')
      .replace(/\b(?:\d[ -]*?){13,19}\b/g, '[redacted]');
  };

  const stableHash = value => {
    let hash = 2166136261;
    for (let index = 0; index < value.length; index += 1) {
      hash ^= value.charCodeAt(index);
      hash = Math.imul(hash, 16777619);
    }
    return (hash >>> 0).toString(36);
  };

  const canonicalActionKey = (element, role, label, rect) => {
    if (role === 'link') {
      const href = String(element.href || attr(element, 'href') || '');
      return `link|${href}|${label.toLowerCase()}`;
    }
    const explicit = String(attr(element, 'data-saccade-action-key') ||
      attr(element, 'id') || attr(element, 'name') || '');
    if (explicit) return `${role}|explicit|${explicit}`;
    if (role === 'button' && closest(element, 'form')) {
      const form = closest(element, 'form');
      return `button|form|${String(form.action || location.href)}|${label.toLowerCase()}`;
    }
    const geometry = [rect.left, rect.top, rect.width, rect.height]
      .map(value => Math.round(value)).join('|');
    return `${role}|${label.toLowerCase()}|${geometry}`;
  };

  const layoutSignature = () => {
    const viewport = globalThis.visualViewport;
    const parts = [
      innerWidth, innerHeight, scrollX, scrollY, devicePixelRatio,
      viewport ? viewport.width : 0,
      viewport ? viewport.height : 0,
      viewport ? viewport.offsetLeft : 0,
      viewport ? viewport.offsetTop : 0,
      viewport ? viewport.scale : 1,
      document.documentElement ? document.documentElement.scrollWidth : 0,
      document.documentElement ? document.documentElement.scrollHeight : 0
    ].map(value => Math.round(Number(value || 0) * 100) / 100);
    let count = 0;
    for (const element of queryAll(document, actionSelector)) {
      if (count++ >= 256) break;
      const rect = rectFor(element);
      parts.push(element.tagName, Math.round(rect.left), Math.round(rect.top),
                 Math.round(rect.width), Math.round(rect.height));
    }
    return parts.join('|');
  };

  const scanActions = () => {
    const generation = ++actionScanGeneration;
    const seen = new Set();
    nativeEmit('actions_begin', generation, epochNow());
    for (const element of queryAll(document, actionSelector)) {
      if (element.disabled || attr(element, 'aria-disabled') === 'true') continue;
      const rect = rectFor(element);
      if (!Number.isFinite(rect.left) || !Number.isFinite(rect.top) ||
          rect.width <= 0 || rect.height <= 0) continue;
      if (innerWidth > 0 && innerHeight > 0 &&
          (rect.right <= 0 || rect.bottom <= 0 || rect.left >= innerWidth ||
           rect.top >= innerHeight)) continue;
      const style = styleFor(element);
      if (style.display === 'none' || style.visibility === 'hidden' ||
          style.pointerEvents === 'none' || Number(style.opacity) === 0) continue;
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;
      const topmost = elementAt(document, centerX, centerY);
      if (!topmost || closest(topmost, actionSelector) !== element) continue;
      const role = matches(element, '.target:not(.hit)')
          ? 'target'
          : (matches(element, 'a[href]')
              ? 'link'
              : (matches(element, 'canvas') ? 'surface' : 'button'));
      const labelledBy = String(attr(element, 'aria-labelledby') || '')
          .split(/\s+/).filter(Boolean)
          .map(id => getById(document, id)?.textContent || '').join(' ');
      const descendantAlt =
          (queryOne(element, 'img[alt]') &&
            attr(queryOne(element, 'img[alt]'), 'alt')) || '';
      const label = redactActionLabel(
          attr(element, 'aria-label') || labelledBy ||
          attr(element, 'title') || element.innerText ||
          element.textContent || descendantAlt || element.value ||
          attr(element, 'id') || attr(element, 'name') || role)
          .replace(/\s+/g, ' ').trim().slice(0, 128);
      const canonicalKey = canonicalActionKey(element, role, label, rect);
      if (seen.has(canonicalKey)) continue;
      seen.add(canonicalKey);
      let actionId = canonicalIds.get(canonicalKey);
      if (!actionId) {
        actionId = `${role}-${stableHash(canonicalKey)}-${++sequence}`;
        canonicalIds.set(canonicalKey, actionId);
      }
      ids.set(element, actionId);
      const opensNewContext = role === 'link' &&
          String(attr(element, 'target') || '').toLowerCase() === '_blank';
      const destinationUrl = role === 'link'
          ? String(element.href || attr(element, 'href') || '').slice(0, 2048)
          : '';
      nativeEmit('action', actionId, role, label, rect.left, rect.top,
                 rect.width, rect.height, opensNewContext, epochNow(), generation,
                 destinationUrl);
    }
    nativeEmit('actions_end', generation, epochNow());
  };

  const refreshLayout = () => {
    const signature = layoutSignature();
    if (lastLayoutSignature && signature !== lastLayoutSignature) {
      nativeEmit('layout_changed', innerWidth, innerHeight,
                 devicePixelRatio, epochNow());
    }
    lastLayoutSignature = signature;
    scanActions();
  };

  const scheduleLayoutRefresh = () => {
    if (layoutRefreshPending) return;
    layoutRefreshPending = true;
    requestAnimationFrame(() => {
      layoutRefreshPending = false;
      safeRun('layout_refresh', refreshLayout);
      nativeEmit('ready', epochNow());
    });
  };

  const attach = () => {
    if (attached || !document.documentElement) return;
    attached = true;
    const observer = new MutationObserver(scheduleLayoutRefresh);
    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['class', 'style']
    });
    addEvent(window, 'resize', scheduleLayoutRefresh, true);
    addEvent(window, 'scroll', scheduleLayoutRefresh, true);
    if (globalThis.visualViewport) {
      addEvent(globalThis.visualViewport, 'resize', scheduleLayoutRefresh, true);
      addEvent(globalThis.visualViewport, 'scroll', scheduleLayoutRefresh, true);
    }
    if (typeof ResizeObserver === 'function') {
      const resizeObserver = new ResizeObserver(scheduleLayoutRefresh);
      resizeObserver.observe(document.documentElement);
    }
    safeRun('scan_controls', scanControls);
    lastLayoutSignature = layoutSignature();
    safeRun('scan_actions', scanActions);
    nativeEmit('ready', epochNow());
  };

  globalThis.__saccadeCollectorRefresh = () => {
    safeRun('scan_controls', scanControls);
    safeRun('layout_refresh', refreshLayout);
    nativeEmit('ready', epochNow());
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
    } else if (kind == "controls_reset" && arguments.size() == 2 &&
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
    } else if ((kind == "actions_begin" || kind == "actions_end") &&
               arguments.size() == 3 && NumberArgument(arguments, 1) &&
               NumberArgument(arguments, 2)) {
      output->SetInt(0, arguments[1]->GetIntValue());
      output->SetDouble(1, arguments[2]->GetDoubleValue());
    } else if (kind == "layout_changed" && arguments.size() == 5 &&
               NumberArgument(arguments, 1) && NumberArgument(arguments, 2) &&
               NumberArgument(arguments, 3) && NumberArgument(arguments, 4)) {
      output->SetDouble(0, arguments[1]->GetDoubleValue());
      output->SetDouble(1, arguments[2]->GetDoubleValue());
      output->SetDouble(2, arguments[3]->GetDoubleValue());
      output->SetDouble(3, arguments[4]->GetDoubleValue());
    } else if (kind == "action" && arguments.size() == 12 &&
               arguments[1]->IsString() && arguments[2]->IsString() &&
               arguments[3]->IsString() && NumberArgument(arguments, 4) &&
               NumberArgument(arguments, 5) && NumberArgument(arguments, 6) &&
               NumberArgument(arguments, 7) && arguments[8]->IsBool() &&
               NumberArgument(arguments, 9) && NumberArgument(arguments, 10) &&
               arguments[11]->IsString()) {
      output->SetString(0, arguments[1]->GetStringValue());
      output->SetString(1, arguments[2]->GetStringValue());
      output->SetString(2, arguments[3]->GetStringValue());
      for (size_t index = 4; index <= 7; ++index) {
        const double value = arguments[index]->GetDoubleValue();
        if (!std::isfinite(value)) {
          exception = "non-finite Saccade geometry";
          return true;
        }
        output->SetDouble(index - 1, value);
      }
      output->SetBool(7, arguments[8]->GetBoolValue());
      output->SetDouble(8, arguments[9]->GetDoubleValue());
      output->SetInt(9, arguments[10]->GetIntValue());
      output->SetString(10, arguments[11]->GetStringValue());
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

void RunFormCommand(CefRefPtr<CefFrame> frame,
                    CefRefPtr<CefV8Context> context,
                    CefRefPtr<CefV8Value> function,
                    int request_id,
                    const std::string& command,
                    const std::string& input_json) {
  auto response = CefProcessMessage::Create(kFormResponseMessage);
  auto output = response->GetArgumentList();
  output->SetInt(0, request_id);

  if (!context || !function || !function->IsFunction() || !context->Enter()) {
    output->SetBool(1, false);
    output->SetString(2, "renderer context unavailable");
    frame->SendProcessMessage(PID_BROWSER, response);
    return;
  }

  CefV8ValueList arguments;
  arguments.push_back(CefV8Value::CreateString(command));
  arguments.push_back(CefV8Value::CreateString(input_json));
  CefRefPtr<CefV8Value> result =
      function->ExecuteFunctionWithContext(context, nullptr, arguments);
  if (!result || !result->IsString()) {
    output->SetBool(1, false);
    output->SetString(2, "fixed form command failed");
  } else {
    output->SetBool(1, true);
    output->SetString(2, result->GetStringValue());
  }
  context->Exit();
  frame->SendProcessMessage(PID_BROWSER, response);
}

}  // namespace

void SaccadeRendererApp::OnContextCreated(CefRefPtr<CefBrowser> browser,
                                          CefRefPtr<CefFrame> frame,
                                          CefRefPtr<CefV8Context> context) {
  CEF_REQUIRE_RENDERER_THREAD();
  const char* reflex_gate = std::getenv("SACCADE_REFLEX_GATE");
  const char* current_tab_grant =
      std::getenv("SACCADE_ENGINE_GRANT_CURRENT_TAB");
  const char* broker = std::getenv("SACCADE_ENGINE_BROKER");
  const bool enabled =
      (reflex_gate && std::string(reflex_gate) == "1") ||
      (current_tab_grant && std::string(current_tab_grant) == "1") ||
      (broker && std::string(broker) == "1");
  if (!frame->IsMain() || !enabled) {
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
  CefRefPtr<CefV8Value> form_function;
  CefRefPtr<CefV8Exception> form_exception;
  if (context->Eval(kSaccadeFormCommandScript,
                    "saccade://renderer/form_command.js", 1,
                    form_function, form_exception) &&
      form_function && form_function->IsFunction()) {
    form_command_closures_[browser->GetIdentifier()] =
        FormCommandClosure{context, form_function};
  }
}

void SaccadeRendererApp::OnContextReleased(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    CefRefPtr<CefV8Context> context) {
  CEF_REQUIRE_RENDERER_THREAD();
  if (!frame->IsMain()) {
    return;
  }
  const auto current = form_command_closures_.find(browser->GetIdentifier());
  if (current != form_command_closures_.end() && current->second.context &&
      current->second.context->IsSame(context)) {
    form_command_closures_.erase(current);
  }
}

bool SaccadeRendererApp::OnProcessMessageReceived(
    CefRefPtr<CefBrowser> browser,
    CefRefPtr<CefFrame> frame,
    CefProcessId source_process,
    CefRefPtr<CefProcessMessage> message) {
  CEF_REQUIRE_RENDERER_THREAD();
  if (source_process != PID_BROWSER || !frame->IsMain()) {
    return false;
  }
  if (message->GetName() == kFormRequestMessage) {
    auto arguments = message->GetArgumentList();
    if (!arguments || arguments->GetSize() != 3) {
      return true;
    }
    const auto closure = form_command_closures_.find(browser->GetIdentifier());
    RunFormCommand(frame,
                   closure == form_command_closures_.end()
                       ? nullptr
                       : closure->second.context,
                   closure == form_command_closures_.end()
                       ? nullptr
                       : closure->second.function,
                   arguments->GetInt(0),
                   arguments->GetString(1).ToString(),
                   arguments->GetString(2).ToString());
    return true;
  }
  if (message->GetName() != kStartMessage &&
      message->GetName() != kRefreshMessage) {
    return false;
  }
  auto context = frame->GetV8Context();
  if (!context || !context->Enter()) {
    return true;
  }
  const bool refresh = message->GetName() == kRefreshMessage;
  if (refresh) {
    auto global = context->GetGlobal();
    auto refresh_function =
        global->GetValue("__saccadeCollectorRefresh");
    if (!refresh_function || !refresh_function->IsFunction()) {
      auto handler = CefRefPtr<EmitHandler>(new EmitHandler());
      global->SetValue(kNativeEmitName,
                       CefV8Value::CreateFunction(kNativeEmitName, handler),
                       V8_PROPERTY_ATTRIBUTE_DONTENUM);
      CefRefPtr<CefV8Value> install_result;
      CefRefPtr<CefV8Exception> install_exception;
      context->Eval(kCollectorScript, "saccade://renderer/collector.js", 1,
                    install_result, install_exception);
      CefRefPtr<CefV8Value> form_function;
      CefRefPtr<CefV8Exception> form_exception;
      if (context->Eval(kSaccadeFormCommandScript,
                        "saccade://renderer/form_command.js", 1,
                        form_function, form_exception) &&
          form_function && form_function->IsFunction()) {
        form_command_closures_[browser->GetIdentifier()] =
            FormCommandClosure{context, form_function};
      }
    }
  }
  CefRefPtr<CefV8Value> result;
  CefRefPtr<CefV8Exception> exception;
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
