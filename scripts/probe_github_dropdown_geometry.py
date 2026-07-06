#!/usr/bin/env python3
"""Geometry-only GitHub account dropdown probe for ServoShell.

This probe is intentionally screenshot-free and value-free. It records only
viewport sizes, element rectangles, hit-test booleans, and auth route status.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SERVOSHELL = Path(
    "/Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell"
)
DEFAULT_PROFILE = ROOT / "runs/dogfood_profile/default"


GEOMETRY_JS = r"""
(() => {
  function rectOf(el) {
    const rect = el.getBoundingClientRect();
    return {
      left: Math.round(rect.left * 100) / 100,
      top: Math.round(rect.top * 100) / 100,
      right: Math.round(rect.right * 100) / 100,
      bottom: Math.round(rect.bottom * 100) / 100,
      width: Math.round(rect.width * 100) / 100,
      height: Math.round(rect.height * 100) / 100
    };
  }

  function styleOf(el) {
    if (!el) return null;
    const style = getComputedStyle(el);
    return {
      display: style.display,
      visibility: style.visibility,
      opacity: style.opacity,
      pointerEvents: style.pointerEvents,
      position: style.position,
      zIndex: style.zIndex,
      overflow: style.overflow,
      overflowX: style.overflowX,
      overflowY: style.overflowY,
      transform: style.transform,
      contain: style.contain,
      isolation: style.isolation
    };
  }

  function visible(el) {
    if (!el) return false;
    const rect = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return rect.width > 0 && rect.height > 0 &&
      rect.right > 0 && rect.bottom > 0 &&
      rect.left < innerWidth && rect.top < innerHeight &&
      style.display !== "none" &&
      style.visibility !== "hidden" &&
      Number(style.opacity || "1") > 0.01;
  }

  function cssPath(el) {
    if (!el) return "";
    const parts = [];
    let current = el;
    for (let depth = 0; current && depth < 4; depth += 1) {
      let part = current.tagName ? current.tagName.toLowerCase() : "node";
      if (current.id) part += "#" + current.id;
      const cls = String(current.className || "").trim().split(/\s+/).filter(Boolean).slice(0, 3);
      if (cls.length) part += "." + cls.join(".");
      parts.unshift(part);
      current = current.parentElement;
    }
    return parts.join(">");
  }

  function scoreProfileCandidate(el) {
    if (!visible(el)) return -1;
    const rect = el.getBoundingClientRect();
    let score = 0;
    const aria = String(el.getAttribute("aria-label") || "").toLowerCase();
    const title = String(el.getAttribute("title") || "").toLowerCase();
    const className = String(el.className || "").toLowerCase();
    const text = String(el.textContent || "").trim().toLowerCase();
    const haystack = aria + " " + title + " " + className + " " + text;
    const rightish = rect.right >= innerWidth - 320;
    const profileish = /profile|account|avatar|user/.test(haystack);
    if (rect.top <= 180) score += 5;
    if (rightish) score += 5;
    if (rect.width >= 16 && rect.width <= 90 && rect.height >= 16 && rect.height <= 90) score += 2;
    if (el.querySelector("img, svg")) score += 2;
    if (profileish) score += 4;
    if (!rightish && !profileish) score -= 6;
    if (/sign in|sign up|search|command palette/.test(haystack)) score -= 8;
    return score;
  }

  function eligibleProfileCandidate(el) {
    if (!visible(el)) return false;
    const rect = el.getBoundingClientRect();
    const aria = String(el.getAttribute("aria-label") || "").toLowerCase();
    const title = String(el.getAttribute("title") || "").toLowerCase();
    const className = String(el.className || "").toLowerCase();
    const text = String(el.textContent || "").trim().toLowerCase();
    const haystack = aria + " " + title + " " + className + " " + text;
    const rightish = rect.right >= innerWidth - 320;
    const profileish = /profile|account|avatar|user/.test(haystack);
    const hasAvatarImage = !!el.querySelector("img");
    const disqualifier =
      /sign in|sign up|search|command palette|cookie|consent|two-factor|authentication|session|alternative|method/.test(haystack);
    return !disqualifier && (profileish || (rightish && hasAvatarImage));
  }

  function findProfileButton() {
    const explicit = Array.from(document.querySelectorAll(
      'button[aria-label], summary[aria-label], [role="button"][aria-label], button, summary, [role="button"]'
    ));
    const ranked = explicit
      .map((el) => ({ el, score: scoreProfileCandidate(el), rect: rectOf(el) }))
      .filter((item) => item.score >= 5 && eligibleProfileCandidate(item.el))
      .sort((a, b) => b.score - a.score || b.rect.right - a.rect.right);
    return ranked[0] || null;
  }

  function floatingCandidates() {
    const elements = Array.from(document.querySelectorAll(
      '[role="menu"], [role="dialog"], [role="listbox"], .dropdown-menu, details[open] > *, nav, ul, div'
    ));
    return elements
      .filter(visible)
      .map((el) => {
        const rect = rectOf(el);
        const style = getComputedStyle(el);
        const area = rect.width * rect.height;
        const isFloating =
          ["absolute", "fixed", "sticky"].includes(style.position) ||
          el.matches('[role="menu"], [role="dialog"], .dropdown-menu, details[open] > *');
        let score = 0;
        if (rect.top <= 240) score += 4;
        if (rect.right >= innerWidth - 520) score += 4;
        if (rect.width >= 120 && rect.width <= 520) score += 3;
        if (rect.height >= 80 && rect.height <= 900) score += 3;
        if (["absolute", "fixed", "sticky"].includes(style.position)) score += 2;
        if (el.matches('[role="menu"], [role="dialog"], .dropdown-menu, details[open] > *')) score += 4;
        if (area > 10000) score += 1;
        if (!isFloating) score = -1;
        return {
          tag: el.tagName ? el.tagName.toLowerCase() : "",
          role: el.getAttribute("role") || "",
          path: cssPath(el),
          rect,
          style: styleOf(el),
          position: style.position,
          score,
          area: Math.round(area)
        };
      })
      .filter((item) => item.score >= 7)
      .sort((a, b) => b.score - a.score || b.area - a.area)
      .slice(0, 5);
  }

  function findSignOutRect() {
    const item = Array.from(document.querySelectorAll("a, button"))
      .find((el) => visible(el) && /sign\s*out/i.test(String(el.textContent || "")));
    return item ? rectOf(item) : null;
  }

  function findSignOutElement() {
    return Array.from(document.querySelectorAll("a, button"))
      .find((el) => visible(el) && /sign\s*out/i.test(String(el.textContent || ""))) || null;
  }

  const bodyText = String(document.body ? document.body.innerText || "" : "");
  const authRouteHint = /\/login|\/sessions\/|\/session|\/signup|two-factor/.test(location.pathname);
  const profile = findProfileButton();
  const candidates = floatingCandidates();
  const menu = candidates[0] || null;
  const signOutElement = findSignOutElement();
  const signOutRect = signOutElement ? rectOf(signOutElement) : null;
  const menuRect = menu ? menu.rect : null;
  const signOutHitTarget = signOutRect
    ? (() => {
        const x = Math.min(innerWidth - 1, Math.max(0, signOutRect.left + signOutRect.width / 2));
        const y = Math.min(innerHeight - 1, Math.max(0, signOutRect.top + signOutRect.height / 2));
        const hit = document.elementFromPoint(x, y);
        const clickable = hit ? hit.closest("a, button, [role='menuitem'], [role='option']") : null;
        return {
          x: Math.round(x * 100) / 100,
          y: Math.round(y * 100) / 100,
          tag: hit && hit.tagName ? hit.tagName.toLowerCase() : "",
          role: hit ? hit.getAttribute("role") || "" : "",
          path: hit ? cssPath(hit) : "",
          clickablePath: clickable ? cssPath(clickable) : "",
          clickableTag: clickable && clickable.tagName ? clickable.tagName.toLowerCase() : "",
          clickableRole: clickable ? clickable.getAttribute("role") || "" : ""
        };
      })()
    : null;
  const signOutHit = !!signOutHitTarget && !!signOutHitTarget.clickablePath;
  const signOutShimTarget = signOutRect &&
    window.__saccadeGithubAccountMenuPointerShim &&
    typeof window.__saccadeGithubAccountMenuPointerShim.probePoint === "function"
      ? window.__saccadeGithubAccountMenuPointerShim.probePoint(
          signOutHitTarget ? signOutHitTarget.x : signOutRect.left + signOutRect.width / 2,
          signOutHitTarget ? signOutHitTarget.y : signOutRect.top + signOutRect.height / 2
        )
      : null;
  const signOutShimHit = !!signOutShimTarget && signOutShimTarget.found;

  return {
    url: location.origin + location.pathname,
    titleLength: document.title.length,
    readyState: document.readyState,
    browserApiFeatures: {
      saccadeCompatShim: typeof window.__saccadeCompatShim === "object" ? window.__saccadeCompatShim : null,
      intersectionObserver: typeof window.IntersectionObserver,
      resizeObserver: typeof window.ResizeObserver,
      cssStyleSheet: typeof window.CSSStyleSheet,
      cssStyleSheetReplaceSync: typeof (window.CSSStyleSheet && window.CSSStyleSheet.prototype && window.CSSStyleSheet.prototype.replaceSync),
      documentAdoptedStyleSheets: typeof document.adoptedStyleSheets,
      documentPrototypeAdoptedStyleSheets: typeof Document !== "undefined" && "adoptedStyleSheets" in Document.prototype,
      shadowRootPrototypeAdoptedStyleSheets: typeof ShadowRoot !== "undefined" && "adoptedStyleSheets" in ShadowRoot.prototype,
      customElements: typeof window.customElements
    },
    authRouteHint,
    loggedOutHint: /sign\s*in|sign\s*up/i.test(bodyText),
    route: authRouteHint
      ? "logged_out_or_auth_required"
      : (profile
          ? "profile_button_seen"
          : (/sign\s*in|sign\s*up/i.test(bodyText) ? "logged_out_or_auth_required" : "profile_button_not_found")),
    viewport: {
      innerWidth,
      innerHeight,
      devicePixelRatio,
      documentClientWidth: document.documentElement.clientWidth,
      documentClientHeight: document.documentElement.clientHeight,
      bodyClientWidth: document.body ? document.body.clientWidth : null,
      bodyScrollWidth: document.body ? document.body.scrollWidth : null
    },
    profileButton: profile ? {
      score: profile.score,
      rect: profile.rect,
      path: cssPath(profile.el),
      ariaExpanded: profile.el.getAttribute("aria-expanded") || ""
    } : null,
    menuCandidates: candidates,
    selectedMenu: menu,
    signOutRect,
    signOutStyle: styleOf(signOutElement),
    signOutHit,
    signOutHitTarget,
    signOutShimHit,
    signOutShimTarget,
    menuWithinViewport: !!menuRect &&
      menuRect.left >= 0 &&
      menuRect.top >= 0 &&
      menuRect.right <= innerWidth &&
      menuRect.bottom <= innerHeight,
    horizontalOverflow: menuRect ? Math.max(0, menuRect.right - innerWidth) : null,
    verticalOverflow: menuRect ? Math.max(0, menuRect.bottom - innerHeight) : null
  };
})()
"""


COMPAT_SHIM_JS = r"""
(() => {
  const installed = {};

  if (typeof window.IntersectionObserver === "undefined") {
    class SaccadeIntersectionObserver {
      constructor(callback, options) {
        this.callback = callback;
        this.options = options || {};
        this.targets = new Set();
      }
      observe(target) {
        if (!target) return;
        this.targets.add(target);
        const rect = target.getBoundingClientRect ? target.getBoundingClientRect() : null;
        const entry = {
          time: Date.now(),
          target,
          rootBounds: null,
          boundingClientRect: rect,
          intersectionRect: rect,
          isIntersecting: true,
          intersectionRatio: 1
        };
        setTimeout(() => {
          try { this.callback([entry], this); } catch (_) {}
        }, 0);
      }
      unobserve(target) {
        this.targets.delete(target);
      }
      disconnect() {
        this.targets.clear();
      }
      takeRecords() {
        return [];
      }
    }
    window.IntersectionObserver = SaccadeIntersectionObserver;
    installed.intersectionObserver = true;
  } else {
    installed.intersectionObserver = false;
  }

  if (typeof window.CSSStyleSheet === "undefined") {
    window.CSSStyleSheet = class SaccadeCSSStyleSheet {
      constructor() {
        this.cssText = "";
      }
      replaceSync(text) {
        this.cssText = String(text || "");
      }
      replace(text) {
        this.replaceSync(text);
        return Promise.resolve(this);
      }
    };
    installed.cssStyleSheet = true;
    installed.cssStyleSheetReplaceSync = true;
  } else {
    installed.cssStyleSheet = false;
    if (typeof window.CSSStyleSheet.prototype.replaceSync === "undefined") {
      window.CSSStyleSheet.prototype.replaceSync = function replaceSync(text) {
        this.__saccadeCssText = String(text || "");
      };
      installed.cssStyleSheetReplaceSync = true;
    } else {
      installed.cssStyleSheetReplaceSync = false;
    }
    if (typeof window.CSSStyleSheet.prototype.replace === "undefined") {
      window.CSSStyleSheet.prototype.replace = function replace(text) {
        this.replaceSync(text);
        return Promise.resolve(this);
      };
      installed.cssStyleSheetReplace = true;
    } else {
      installed.cssStyleSheetReplace = false;
    }
  }

  function installAdoptedStyleSheets(proto, label) {
    if (!proto) return false;
    if (Object.prototype.hasOwnProperty.call(proto, "adoptedStyleSheets")) return false;
    const existing = Object.getOwnPropertyDescriptor(proto, "adoptedStyleSheets");
    if (existing) return false;
    const store = new WeakMap();
    Object.defineProperty(proto, "adoptedStyleSheets", {
      configurable: true,
      enumerable: true,
      get() {
        return store.get(this) || [];
      },
      set(value) {
        store.set(this, Array.from(value || []));
      }
    });
    installed[label] = true;
    return true;
  }

  installed.documentAdoptedStyleSheets =
    typeof Document !== "undefined" && installAdoptedStyleSheets(Document.prototype, "documentAdoptedStyleSheets");
  installed.shadowRootAdoptedStyleSheets =
    typeof ShadowRoot !== "undefined" && installAdoptedStyleSheets(ShadowRoot.prototype, "shadowRootAdoptedStyleSheets");

  const report = {
    installed,
    features: {
      intersectionObserver: typeof window.IntersectionObserver,
      cssStyleSheet: typeof window.CSSStyleSheet,
      cssStyleSheetReplaceSync: typeof (window.CSSStyleSheet && window.CSSStyleSheet.prototype && window.CSSStyleSheet.prototype.replaceSync),
      documentAdoptedStyleSheets: typeof document.adoptedStyleSheets,
      documentPrototypeAdoptedStyleSheets: typeof Document !== "undefined" && "adoptedStyleSheets" in Document.prototype,
      shadowRootPrototypeAdoptedStyleSheets: typeof ShadowRoot !== "undefined" && "adoptedStyleSheets" in ShadowRoot.prototype
    },
    timing: "after_ready_webdriver_execute"
  };
  window.__saccadeCompatShim = report;
  return report;
})()
"""


CLICK_PROFILE_JS = r"""
(() => {
  function visible(el) {
    if (!el) return false;
    const rect = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return rect.width > 0 && rect.height > 0 &&
      rect.right > 0 && rect.bottom > 0 &&
      rect.left < innerWidth && rect.top < innerHeight &&
      style.display !== "none" &&
      style.visibility !== "hidden" &&
      Number(style.opacity || "1") > 0.01;
  }

  function scoreProfileCandidate(el) {
    if (!visible(el)) return -1;
    const rect = el.getBoundingClientRect();
    let score = 0;
    const aria = String(el.getAttribute("aria-label") || "").toLowerCase();
    const title = String(el.getAttribute("title") || "").toLowerCase();
    const className = String(el.className || "").toLowerCase();
    const text = String(el.textContent || "").trim().toLowerCase();
    const haystack = aria + " " + title + " " + className + " " + text;
    const rightish = rect.right >= innerWidth - 320;
    const profileish = /profile|account|avatar|user/.test(haystack);
    if (rect.top <= 180) score += 5;
    if (rightish) score += 5;
    if (rect.width >= 16 && rect.width <= 90 && rect.height >= 16 && rect.height <= 90) score += 2;
    if (el.querySelector("img, svg")) score += 2;
    if (profileish) score += 4;
    if (!rightish && !profileish) score -= 6;
    if (/sign in|sign up|search|command palette/.test(haystack)) score -= 8;
    return score;
  }

  function eligibleProfileCandidate(el) {
    if (!visible(el)) return false;
    const rect = el.getBoundingClientRect();
    const aria = String(el.getAttribute("aria-label") || "").toLowerCase();
    const title = String(el.getAttribute("title") || "").toLowerCase();
    const className = String(el.className || "").toLowerCase();
    const text = String(el.textContent || "").trim().toLowerCase();
    const haystack = aria + " " + title + " " + className + " " + text;
    const rightish = rect.right >= innerWidth - 320;
    const profileish = /profile|account|avatar|user/.test(haystack);
    const hasAvatarImage = !!el.querySelector("img");
    const disqualifier =
      /sign in|sign up|search|command palette|cookie|consent|two-factor|authentication|session|alternative|method/.test(haystack);
    return !disqualifier && (profileish || (rightish && hasAvatarImage));
  }

  const ranked = Array.from(document.querySelectorAll(
    'button[aria-label], summary[aria-label], [role="button"][aria-label], button, summary, [role="button"]'
  ))
    .map((el) => ({ el, score: scoreProfileCandidate(el) }))
    .filter((item) => item.score >= 5 && eligibleProfileCandidate(item.el))
    .sort((a, b) => b.score - a.score);
  const profile = ranked[0] ? ranked[0].el : null;
  if (!profile) return { clicked: false, reason: "profile_button_not_found" };
  profile.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true, view: window }));
  profile.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, cancelable: true, view: window }));
  profile.click();
  return { clicked: true, score: ranked[0].score };
})()
"""


def unix_ms() -> int:
    return int(time.time() * 1000)


def webdriver_request(port: int, method: str, path: str, payload=None, timeout: float = 10.0):
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    headers = {"Content-Type": "application/json"} if payload is not None else {}
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        data=data,
        headers=headers,
        method=method,
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        text = response.read().decode("utf-8", "replace")
        return response.status, json.loads(text) if text else None


def wait_for_status(port: int, proc: subprocess.Popen, timeout_sec: float):
    deadline = time.monotonic() + timeout_sec
    last_error = None
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f"servoshell exited before WebDriver was ready: {proc.returncode}")
        try:
            return webdriver_request(port, "GET", "/status", timeout=0.5)
        except Exception as error:  # noqa: BLE001
            last_error = repr(error)
            time.sleep(0.25)
    raise TimeoutError(f"WebDriver status was not ready; last_error={last_error}")


def value_session_id(response) -> str:
    value = response.get("value") if isinstance(response, dict) else None
    if isinstance(value, dict) and isinstance(value.get("sessionId"), str):
        return value["sessionId"]
    if isinstance(response.get("sessionId"), str):
        return response["sessionId"]
    raise RuntimeError(f"new session response did not include a session id: {response}")


def execute(port: int, session_id: str, script: str):
    expression = script.strip()
    _, body = webdriver_request(
        port,
        "POST",
        f"/session/{session_id}/execute/sync",
        {"script": f"return ({expression});", "args": []},
    )
    return body.get("value") if isinstance(body, dict) else body


def set_window_rect(port: int, session_id: str, width: int, height: int):
    _, body = webdriver_request(
        port,
        "POST",
        f"/session/{session_id}/window/rect",
        {"width": width, "height": height},
    )
    time.sleep(0.7)
    return body


def navigate_to(port: int, session_id: str, url: str):
    _, body = webdriver_request(
        port,
        "POST",
        f"/session/{session_id}/url",
        {"url": url},
    )
    return body


def wait_for_ready(port: int, session_id: str, timeout_sec: float):
    deadline = time.monotonic() + timeout_sec
    last = None
    while time.monotonic() < deadline:
        last = execute(
            port,
            session_id,
            "(() => ({readyState: document.readyState, body: !!document.body, url: location.origin + location.pathname}))()",
        )
        if last.get("body") and last.get("readyState") in {"interactive", "complete"}:
            return last
        time.sleep(0.3)
    return last


def wait_for_auth(port: int, session_id: str, timeout_sec: float):
    deadline = time.monotonic() + timeout_sec
    last = None
    samples = 0
    while time.monotonic() < deadline:
        last = execute(port, session_id, GEOMETRY_JS)
        samples += 1
        if last.get("route") == "profile_button_seen":
            return {"status": "profile_seen", "samples": samples, "last": last}
        time.sleep(1.0)
    return {"status": "timeout", "samples": samples, "last": last}


def phase_probe(port: int, session_id: str, width: int, height: int, label: str):
    rect_response = set_window_rect(port, session_id, width, height)
    before = execute(port, session_id, GEOMETRY_JS)
    click = execute(port, session_id, CLICK_PROFILE_JS)
    time.sleep(0.6)
    after = execute(port, session_id, GEOMETRY_JS)
    return {
        "label": label,
        "requestedOuterRect": {"width": width, "height": height},
        "webdriverRectResponse": rect_response,
        "before": before,
        "click": click,
        "after": after,
    }


def finish_servoshell(port: int, session_id: str | None, proc: subprocess.Popen) -> dict:
    report = {"attempted": False, "route": "webdriver_servo_shutdown"}
    if session_id:
        report["attempted"] = True
        try:
            status, body = webdriver_request(port, "DELETE", f"/session/{session_id}/servo/shutdown", timeout=3)
            report["ok"] = True
            report["status"] = status
            report["body"] = body
        except Exception as error:  # noqa: BLE001
            report["ok"] = False
            report["error"] = repr(error)
    else:
        report["skipped"] = "missing_session_id"

    try:
        stdout, stderr = proc.communicate(timeout=10)
        report["termination"] = "graceful_servo_shutdown"
    except subprocess.TimeoutExpired:
        report["timed_out"] = True
        if session_id:
            try:
                webdriver_request(port, "DELETE", f"/session/{session_id}", timeout=3)
            except Exception as error:  # noqa: BLE001
                report["delete_session_error"] = repr(error)
        if proc.poll() is None:
            proc.terminate()
        try:
            stdout, stderr = proc.communicate(timeout=3)
            report["termination"] = "sigterm_after_shutdown_timeout"
        except subprocess.TimeoutExpired:
            proc.kill()
            stdout, stderr = proc.communicate()
            report["termination"] = "sigkill_after_shutdown_timeout"
    report["returncode"] = proc.returncode
    report["stdout_head"] = stdout.splitlines()[:80]
    report["stderr_head"] = stderr.splitlines()[:120]
    return report


def classify(phases: list[dict]) -> tuple[str, list[str]]:
    failures = []
    routes = [phase.get("after", {}).get("route") for phase in phases]
    logged_out_hints = [bool(phase.get("after", {}).get("loggedOutHint")) for phase in phases]
    if all(route == "logged_out_or_auth_required" for route in routes) or (
        not any(route == "profile_button_seen" for route in routes) and any(logged_out_hints)
    ):
        return "auth_required", ["GitHub profile button was not visible; route looked logged out"]
    if not any(route == "profile_button_seen" for route in routes):
        return "profile_not_found", [f"profile route states: {routes}"]

    for phase in phases:
        after = phase.get("after") or {}
        label = phase.get("label")
        if after.get("route") != "profile_button_seen":
            failures.append(f"{label}: route={after.get('route')}")
            continue
        if not phase.get("click", {}).get("clicked"):
            failures.append(f"{label}: profile click did not dispatch")
        if not after.get("selectedMenu"):
            failures.append(f"{label}: no floating menu candidate after click")
        if not after.get("signOutRect"):
            failures.append(f"{label}: sign-out action was not visible after profile click")
        if not after.get("menuWithinViewport"):
            failures.append(
                f"{label}: menu overflow h={after.get('horizontalOverflow')} v={after.get('verticalOverflow')}"
            )
        if after.get("signOutRect") and not after.get("signOutHit") and not after.get("signOutShimHit"):
            failures.append(f"{label}: sign-out center is not hittable and shim target is missing")

    return ("pass" if not failures else "fail"), failures


def classify_api_probe(probe: dict, expect_shim: bool) -> tuple[str, list[str]]:
    failures = []
    features = probe.get("browserApiFeatures") or {}
    shim = features.get("saccadeCompatShim")
    if expect_shim:
        if not isinstance(shim, dict):
            failures.append("saccade compat shim marker missing")
        elif shim.get("kind") != "saccade_github_compat_shim_v0":
            failures.append(f"unexpected shim marker kind: {shim.get('kind')!r}")
        else:
            account_shim = (shim.get("installed") or {}).get("githubAccountMenuPointerShim")
            if not isinstance(account_shim, dict):
                failures.append("github account menu pointer shim marker missing")
            elif account_shim.get("kind") != "saccade_github_account_menu_pointer_shim_v1":
                failures.append(f"unexpected account menu shim marker kind: {account_shim.get('kind')!r}")
    if features.get("intersectionObserver") != "function":
        failures.append(f"intersectionObserver={features.get('intersectionObserver')!r}")
    if features.get("cssStyleSheetReplaceSync") != "function":
        failures.append(f"cssStyleSheetReplaceSync={features.get('cssStyleSheetReplaceSync')!r}")
    if features.get("documentPrototypeAdoptedStyleSheets") is not True:
        failures.append(
            f"documentPrototypeAdoptedStyleSheets={features.get('documentPrototypeAdoptedStyleSheets')!r}"
        )
    if features.get("shadowRootPrototypeAdoptedStyleSheets") is not True:
        failures.append(
            f"shadowRootPrototypeAdoptedStyleSheets={features.get('shadowRootPrototypeAdoptedStyleSheets')!r}"
        )
    return ("pass" if not failures else "fail"), failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--servoshell", type=Path, default=DEFAULT_SERVOSHELL)
    parser.add_argument("--profile-dir", type=Path, default=DEFAULT_PROFILE)
    parser.add_argument("--url", default="https://gist.github.com/starred")
    parser.add_argument("--port", type=int, default=7095)
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--timeout-sec", type=float, default=35.0)
    parser.add_argument("--page-ready-sec", type=float, default=12.0)
    parser.add_argument(
        "--wait-for-auth-sec",
        type=float,
        default=0.0,
        help="Keep the visible window open this long waiting for a human login/profile button.",
    )
    parser.add_argument(
        "--sizes",
        default="900x700,1200x740,900x700",
        help="Comma-separated outer window sizes to probe.",
    )
    parser.add_argument(
        "--compat-shim",
        action="store_true",
        help=(
            "Experimental: inject minimal IntersectionObserver/adoptedStyleSheets "
            "polyfills after page readiness before probing. This is diagnostic only; "
            "it is too late for scripts that failed during initial module evaluation."
        ),
    )
    parser.add_argument(
        "--userscripts-dir",
        type=Path,
        help=(
            "Experimental: pass ServoShell --userscripts=<dir>. This is closer to a "
            "preload path than --compat-shim and should apply to new documents."
        ),
    )
    parser.add_argument(
        "--api-only",
        action="store_true",
        help="Skip dropdown geometry and only verify the compatibility API surface.",
    )
    args = parser.parse_args()

    output_dir = args.output_dir or ROOT / "runs/servoshell_ui" / f"github_dropdown_{unix_ms()}"
    output_dir.mkdir(parents=True, exist_ok=True)
    args.profile_dir.mkdir(parents=True, exist_ok=True)
    userscripts_dir = args.userscripts_dir.expanduser().resolve() if args.userscripts_dir else None
    if userscripts_dir and not userscripts_dir.is_dir():
        parser.error(f"--userscripts-dir must be an existing directory: {userscripts_dir}")

    sizes = []
    for token in args.sizes.split(","):
        width, height = token.lower().split("x", 1)
        sizes.append((int(width), int(height)))

    cmd = [
        str(args.servoshell),
        f"--webdriver={args.port}",
        f"--config-dir={args.profile_dir}",
    ]
    if userscripts_dir:
        cmd.append(f"--userscripts={userscripts_dir}")
    cmd.append(args.url)
    proc = subprocess.Popen(
        cmd,
        cwd=str(ROOT),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    report = {
        "servoshell": str(args.servoshell),
        "url": args.url,
        "profile_dir": str(args.profile_dir),
        "port": args.port,
        "cmd": cmd,
        "output_dir": str(output_dir),
        "policy": {
            "screenshots": "disabled",
            "text_values_logged": False,
            "sensitive_inputs_read": False,
            "compat_shim": "after_ready_webdriver_execute" if args.compat_shim else "none",
            "userscripts_dir": str(userscripts_dir) if userscripts_dir else None,
            "api_only": args.api_only,
        },
        "sizes": [{"width": width, "height": height} for width, height in sizes],
        "phases": [],
    }
    session_id = None
    classification = "error"
    failures = []
    try:
        status_code, status_body = wait_for_status(args.port, proc, args.timeout_sec)
        report["status"] = {"code": status_code, "body": status_body}
        _, body = webdriver_request(
            args.port,
            "POST",
            "/session",
            {"capabilities": {"alwaysMatch": {"browserName": "servo"}}},
        )
        session_id = value_session_id(body)
        report["session_id"] = session_id
        report["navigate"] = navigate_to(args.port, session_id, args.url)
        report["ready"] = wait_for_ready(args.port, session_id, args.page_ready_sec)
        if args.compat_shim:
            report["compat_shim"] = execute(args.port, session_id, COMPAT_SHIM_JS)
        skip_phases = False
        if args.api_only:
            report["api_probe"] = execute(args.port, session_id, GEOMETRY_JS)
            classification, failures = classify_api_probe(
                report["api_probe"],
                expect_shim=bool(userscripts_dir or args.compat_shim),
            )
            skip_phases = True
        elif args.wait_for_auth_sec > 0:
            report["auth_wait"] = wait_for_auth(args.port, session_id, args.wait_for_auth_sec)
            if report["auth_wait"].get("status") != "profile_seen":
                classification = "auth_required"
                failures = ["GitHub profile button was not visible before auth wait timeout"]
                skip_phases = True

        if not skip_phases:
            for index, (width, height) in enumerate(sizes):
                report["phases"].append(
                    phase_probe(args.port, session_id, width, height, f"phase_{index}_{width}x{height}")
                )

            classification, failures = classify(report["phases"])
        report["classification"] = classification
        report["failures"] = failures
    except Exception as error:  # noqa: BLE001
        failures = [repr(error)]
        report["error"] = repr(error)
        report["classification"] = classification
        report["failures"] = failures
    finally:
        report["process"] = finish_servoshell(args.port, session_id, proc)
        report["returncode"] = report["process"].get("returncode")
        report["stdout_head"] = report["process"].get("stdout_head")
        report["stderr_head"] = report["process"].get("stderr_head")
        report["ok"] = classification == "pass"
        report_path = output_dir / "report.json"
        report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    print(
        "GITHUB_DROPDOWN_GEOMETRY "
        f"classification={classification} "
        f"ok={str(classification == 'pass').lower()} "
        f"report={output_dir / 'report.json'}"
    )
    return 0 if classification in {"pass", "auth_required"} else 1


if __name__ == "__main__":
    sys.exit(main())
