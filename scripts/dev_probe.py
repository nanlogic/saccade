#!/usr/bin/env python3
"""Exercise Saccade through its MCP stdio route and save value-free evidence."""

from __future__ import annotations

import argparse
import json
import os
import select
import subprocess
import time
import urllib.parse
from pathlib import Path
from typing import Any


EDITABLE_INPUTS = (
    ("text_field", "Email", "SACCADE-DEV-EMAIL-Ω"),
    ("search_field", "Search", "SACCADE-DEV-SEARCH-Ω"),
    ("text_area", "Notes", "SACCADE DEV LINE ONE\nLINE TWO Ω"),
    ("content_editable", "Draft", "SACCADE-DEV-DRAFT-Ω"),
    ("spin_button", "Quantity", "7319"),
)
TEXT_SENTINELS = tuple(value for _role, _name, value in EDITABLE_INPUTS)
FIXTURE_SENTINELS = (
    "FIXTURE-SEARCH-SECRET",
    "FIXTURE-NOTES-SECRET",
    "FIXTURE-DRAFT-SECRET",
    "8675309",
    "FIXTURE-HIDDEN-TEXT-SECRET",
    "FIXTURE-NESTED-TEXT-SECRET",
)
REDACTED_VALUES = TEXT_SENTINELS + FIXTURE_SENTINELS
ACCURACY_TARGET_COUNT = 24
ACCURACY_WINDOW_PHASES = (
    (1, "baseline", 24, 52, 800, 747),
    (9, "moved", 60, 90, 760, 700),
    (17, "moved_and_resized", 120, 70, 640, 680),
)
ACCURACY_LAYOUTS = ("buttons", "canvas")
ACCURACY_DIFFICULTIES = ("ordinary", "hard")
MOUSE_BACKENDS = ("native", "soft")
SOFTWARE_PREFERRED_ROLES = {
    "button", "link", "checkbox", "radio", "switch", "tab", "menu_item", "reflex_target",
}
MOUSEACCURACY_DIFFICULTY_VALUES = ("Easy", "Normal", "Hard", "Insane")
MOUSEACCURACY_SIZE_VALUES = ("Large", "Medium", "Small", "Tiny")
ACCURACY_SIZES = {
    "buttons": {
        "ordinary": (32, 40, 48),
        "hard": (24, 32, 40),
    },
    "canvas": {
        "ordinary": (14, 18, 22),
        "hard": (10, 14, 18),
    },
}


def redact_editable_values(value: str) -> str:
    for sentinel in REDACTED_VALUES:
        value = value.replace(sentinel, "[editable content removed]")
    return value


class Mcp:
    def __init__(self, runtime: Path, runtime_dir: Path) -> None:
        environment = os.environ.copy()
        environment["SACCADE_RUNTIME_DIR"] = str(runtime_dir)
        self.process = subprocess.Popen(
            [str(runtime), "mcp"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
            env=environment,
        )
        self.next_id = 1
        try:
            self.initialize = self.rpc("initialize", {})
        except Exception:
            self.close()
            raise

    def rpc(self, method: str, params: dict[str, Any], timeout: float = 35.0) -> dict[str, Any]:
        request_id = self.next_id
        self.next_id += 1
        assert self.process.stdin is not None
        assert self.process.stdout is not None
        self.process.stdin.write(
            json.dumps({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params})
            + "\n"
        )
        self.process.stdin.flush()
        ready, _, _ = select.select([self.process.stdout], [], [], timeout)
        if not ready:
            raise RuntimeError(f"MCP timed out during {method}")
        line = self.process.stdout.readline()
        if not line:
            stderr = self.process.stderr.read() if self.process.stderr else ""
            raise RuntimeError(f"MCP exited during {method}: {stderr.strip()}")
        response = json.loads(line)
        if response.get("id") != request_id:
            raise RuntimeError(f"MCP returned the wrong response id during {method}")
        if "error" in response:
            raise RuntimeError(response["error"].get("message", str(response["error"])))
        return response["result"]

    def tool(self, name: str, arguments: dict[str, Any], timeout: float = 35.0) -> dict[str, Any]:
        result = self.rpc(
            "tools/call",
            {"name": f"saccade.{name}", "arguments": arguments},
            timeout=timeout,
        )
        return result["structuredContent"]

    def close(self) -> None:
        if self.process.stdin:
            try:
                self.process.stdin.close()
            except BrokenPipeError:
                pass
        if self.process.poll() is None:
            self.process.terminate()
        try:
            self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.process.kill()


def wait_for_mcp(runtime: Path, runtime_dir: Path, timeout: float = 30.0) -> Mcp:
    deadline = time.monotonic() + timeout
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            return Mcp(runtime, runtime_dir)
        except Exception as error:  # noqa: BLE001
            last_error = error
            time.sleep(0.25)
    raise RuntimeError(f"Saccade MCP did not become ready: {last_error}")


def wait_observation(mcp: Mcp, tab_id: str, timeout: float = 20.0) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            observation = mcp.tool("web.observe", {"tab_id": tab_id})
            if observation.get("objects"):
                return observation
        except Exception as error:  # noqa: BLE001
            last_error = error
        time.sleep(0.1)
    raise RuntimeError(f"fixture observation did not arrive: {last_error}")


def named(observation: dict[str, Any], role: str, name: str) -> dict[str, Any]:
    matches = named_items(observation, role, name)
    if matches:
        return matches[0]
    raise RuntimeError(f"observation has no {role} named {name!r}")


def named_items(observation: dict[str, Any], role: str, name: str) -> list[dict[str, Any]]:
    return [
        item for item in observation["objects"]
        if item.get("role") == role and item.get("name") == name
    ]


def stable_observation(mcp: Mcp, tab_id: str, timeout: float = 5.0) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    previous = mcp.tool("web.observe", {"tab_id": tab_id})
    while time.monotonic() < deadline:
        time.sleep(0.25)
        current = mcp.tool("web.observe", {"tab_id": tab_id})
        if (
            current["document_id"] == previous["document_id"]
            and current["revision"] == previous["revision"]
        ):
            return current
        previous = current
    raise RuntimeError("observation did not reach a stable revision")


def act(
    mcp: Mcp,
    observation: dict[str, Any],
    role: str,
    name: str,
    operation: str,
    payload_for: Any,
    backend: str = "auto",
    expected_backend: str | None = None,
) -> tuple[dict[str, Any], dict[str, Any]]:
    last_error: Exception | None = None
    for _attempt in range(8):
        observation = stable_observation(mcp, observation["tab_id"])
        target = named(observation, role, name)
        request = {
            "browser_instance_id": observation["browser_instance_id"],
            "tab_id": observation["tab_id"],
            "document_id": observation["document_id"],
            "basis_revision": observation["revision"],
            "action_token": target["action_token"],
            "operation": operation,
            "payload": payload_for(observation),
        }
        try:
            tool = {
                "auto": "web.act",
                "native": "web.act_native",
                "soft": "web.act_soft",
            }[backend]
            receipt = mcp.tool(tool, request, timeout=40.0)
            if receipt.get("dispatch_status") == "stale_before_dispatch":
                observation = receipt["post_action_observation"]
                continue
            break
        except Exception as error:
            last_error = error
            if "stale action basis" not in str(error) and "not current" not in str(error):
                raise
            observation = stable_observation(mcp, observation["tab_id"])
    else:
        raise RuntimeError(
            f"{role} {name!r} {operation} stayed stale after fresh observations: {last_error}"
        )
    effective_backend = expected_backend or backend
    software = effective_backend == "soft" or (
        effective_backend == "auto" and role in SOFTWARE_PREFERRED_ROLES
    )
    expected_dispatch = "accepted_by_software" if software else "accepted_by_os"
    if receipt.get("dispatch_status") != expected_dispatch:
        raise RuntimeError(
            f"{role} {name!r} {operation} dispatch failed: {receipt.get('dispatch_status')}"
        )
    if receipt.get("postcondition") != "verified":
        raise RuntimeError(
            f"{role} {name!r} {operation} postcondition failed: {receipt.get('postcondition')}"
        )
    return receipt, receipt["post_action_observation"]


def open_fixture(mcp: Mcp, url: str) -> dict[str, Any]:
    opened = mcp.tool("tabs.open", {"url": url, "active": True})
    return wait_observation(mcp, opened["tab_id"])


def adaptive_input_policy(mcp: Mcp, base_url: str) -> dict[str, Any]:
    url = urllib.parse.urljoin(base_url, "adaptive_input.html")
    observation = open_fixture(mcp, url)
    first_receipt: dict[str, Any] | None = None
    for _attempt in range(8):
        observation = stable_observation(mcp, observation["tab_id"])
        target = named(observation, "button", "Trusted only")
        request = {
            "browser_instance_id": observation["browser_instance_id"],
            "tab_id": observation["tab_id"],
            "document_id": observation["document_id"],
            "basis_revision": observation["revision"],
            "action_token": target["action_token"],
            "operation": "click",
            "payload": {"kind": "none"},
        }
        first_receipt = mcp.tool("web.act", request, timeout=40.0)
        observation = first_receipt["post_action_observation"]
        if first_receipt.get("dispatch_status") == "stale_before_dispatch":
            continue
        break
    if first_receipt is None or first_receipt.get("dispatch_status") != "accepted_by_software":
        raise RuntimeError("adaptive input did not try the registered software default first")
    if first_receipt.get("postcondition") not in {"visible_state_unchanged", "unverified"}:
        raise RuntimeError("adaptive fixture unexpectedly accepted an untrusted software click")

    policy = mcp.tool("input_policy.list", {})
    learned = [
        rule for rule in policy.get("rules", [])
        if rule.get("page") == url
        and rule.get("role") == "button"
        and rule.get("control") == "Trusted only"
    ]
    if len(learned) != 1 or learned[0].get("backend") != "native":
        raise RuntimeError("unverified software receipt did not create a page-local native rule")

    observation = stable_observation(mcp, observation["tab_id"])
    target = named(observation, "button", "Trusted only")
    diagnostic_request = {
        "browser_instance_id": observation["browser_instance_id"],
        "tab_id": observation["tab_id"],
        "document_id": observation["document_id"],
        "basis_revision": observation["revision"],
        "action_token": target["action_token"],
        "operation": "click",
        "payload": {"kind": "none"},
    }
    try:
        mcp.tool("web.act_soft", diagnostic_request, timeout=10.0)
    except Exception as error:  # noqa: BLE001
        if "user-local input policy requires native" not in str(error):
            raise
        soft_override_rejected = True
    else:
        raise RuntimeError("diagnostic software input bypassed a learned native rule")

    second_receipt, _observation = act(
        mcp,
        observation,
        "button",
        "Trusted only",
        "click",
        lambda _: {"kind": "none"},
        "auto",
        "native",
    )
    return {
        "page": url,
        "first_receipt": first_receipt,
        "learned_rule": learned[0],
        "soft_override_rejected": soft_override_rejected,
        "next_receipt": second_receipt,
        "no_same_token_retry": first_receipt["action_token"] != second_receipt["action_token"],
    }


def validate_editable_projection(observation: dict[str, Any]) -> None:
    expected_state = {
        "search_field": {"has_value", "enabled", "required", "readonly", "invalid"},
        "text_area": {"has_value", "enabled", "required", "readonly", "invalid"},
        "content_editable": {"has_value", "readonly"},
        "spin_button": {"has_value", "enabled", "required", "readonly", "invalid"},
    }
    for role, name, _text in EDITABLE_INPUTS[1:]:
        item = named(observation, role, name)
        if set(item.get("state", {})) != expected_state[role]:
            raise RuntimeError(f"{role} exposed an unexpected state surface")
        if item.get("affordances") != ["type"] or not item.get("action_token"):
            raise RuntimeError(f"{role} did not expose its closed-loop type action")
    for role, name in (
        ("search_field", "Read-only search"),
        ("text_area", "Read-only notes"),
        ("content_editable", "Read-only draft"),
        ("spin_button", "Read-only quantity"),
    ):
        item = named(observation, role, name)
        if item.get("affordances") or item.get("action_token"):
            raise RuntimeError(f"read-only {role} exposed an action")


def validate_structural_projection(observation: dict[str, Any]) -> None:
    expected = {
        ("heading", "Catalog controls"),
        ("paragraph", "This page proves native control loops and bounded structural reading."),
        ("list_item", "Observe the current page"),
        ("list_item", "Act through native input"),
        ("cell", "Evidence"),
        ("cell", "Chrome and Edge"),
        ("alert", "Fixture ready"),
        ("status", "No actions yet"),
    }
    projected = {(item.get("role"), item.get("text")) for item in observation.get("objects", [])}
    missing = expected - projected
    if missing:
        raise RuntimeError(f"structural reading omitted {sorted(missing)!r}")
    heading = next(item for item in observation["objects"] if item.get("role") == "heading")
    if heading.get("state") != {"level": "1"}:
        raise RuntimeError("heading level was not projected")
    alert = next(item for item in observation["objects"] if item.get("role") == "alert")
    if alert.get("state") != {"busy": "false"}:
        raise RuntimeError("alert busy state was not projected")
    for item in observation.get("objects", []):
        if item.get("kind") != "text":
            continue
        if item.get("affordances") or item.get("action_token") or item.get("name"):
            raise RuntimeError("structural text exposed an action or duplicate accessible name")


def controls(mcp: Mcp, url: str, browser: str) -> dict[str, Any]:
    capabilities = mcp.tool("system.capabilities", {})
    observation = open_fixture(mcp, url)
    initial = observation
    validate_editable_projection(initial)
    validate_structural_projection(initial)
    image = named(initial, "image", "Gear Up cover")
    if image.get("description") != "Semantic identity: gear-up-cover-v2.1":
        raise RuntimeError("image semantic identity bridge was not projected")
    if image.get("affordances") or image.get("action_token"):
        raise RuntimeError("image identity observation exposed an action")
    receipts: list[dict[str, Any]] = []

    button_basis = observation
    button_target = named(observation, "button", "Save")
    receipt, observation = act(mcp, observation, "button", "Save", "click", lambda _: {"kind": "none"})
    receipts.append(receipt)
    stale_request = {
        "browser_instance_id": button_basis["browser_instance_id"],
        "tab_id": button_basis["tab_id"],
        "document_id": button_basis["document_id"],
        "basis_revision": button_basis["revision"],
        "action_token": button_target["action_token"],
        "operation": "click",
        "payload": {"kind": "none"},
    }
    try:
        mcp.tool("web.act", stale_request, timeout=10.0)
    except Exception:
        stale_token_rejected = True
    else:
        raise RuntimeError("a consumed action token was accepted twice")
    for role, name, supplied_text in EDITABLE_INPUTS:
        receipt, observation = act(
            mcp,
            observation,
            role,
            name,
            "type",
            lambda _current, text=supplied_text: {"kind": "text", "text": text},
        )
        receipts.append(receipt)
    receipt, observation = act(
        mcp,
        observation,
        "checkbox",
        "Remember me",
        "click",
        lambda _: {"kind": "none"},
    )
    receipts.append(receipt)
    receipt, observation = act(
        mcp,
        observation,
        "radio",
        "Fast plan",
        "click",
        lambda _: {"kind": "none"},
    )
    receipts.append(receipt)
    if named(observation, "radio", "Eco plan").get("state", {}).get("checked") != "false":
        raise RuntimeError("radio group did not preserve native exclusivity")
    receipt, observation = act(
        mcp,
        observation,
        "switch",
        "Notifications",
        "click",
        lambda _: {"kind": "none"},
    )
    receipts.append(receipt)
    receipt, observation = act(
        mcp,
        observation,
        "tab",
        "Details",
        "click",
        lambda _: {"kind": "none"},
    )
    receipts.append(receipt)
    receipt, observation = act(
        mcp,
        observation,
        "menu_item",
        "More actions",
        "click",
        lambda _: {"kind": "none"},
    )
    receipts.append(receipt)
    receipt, observation = act(
        mcp,
        observation,
        "select",
        "Color",
        "select",
        lambda current: {
            "kind": "select",
            "option_object_id": named(current, "option", "Blue")["object_id"],
        },
    )
    receipts.append(receipt)
    link_observation = open_fixture(mcp, urllib.parse.urljoin(url, "link.html"))
    receipt, _link_observation = act(
        mcp,
        link_observation,
        "link",
        "Open button fixture",
        "click",
        lambda _: {"kind": "none"},
    )
    receipts.append(receipt)
    choice_url = urllib.parse.urljoin(url, "listbox_combobox.html")
    choice_observation = open_fixture(mcp, choice_url)
    choice_observation = stable_observation(mcp, choice_observation["tab_id"])
    named(choice_observation, "option", "Denver")
    urgent_options = named_items(choice_observation, "option", "Urgent")
    if len(urgent_options) != 2 or urgent_options[0]["object_id"] == urgent_options[1]["object_id"]:
        raise RuntimeError("duplicate ARIA options did not preserve distinct object identity")
    receipt, choice_observation = act(
        mcp,
        choice_observation,
        "select",
        "Priority",
        "select",
        lambda current: {
            "kind": "select",
            "option_object_id": named_items(current, "option", "Urgent")[-1]["object_id"],
        },
    )
    receipts.append(receipt)
    selected_urgent = [
        item.get("state", {}).get("selected")
        for item in named_items(choice_observation, "option", "Urgent")
    ]
    if selected_urgent != ["false", "true"]:
        raise RuntimeError("duplicate option identity did not select only the requested object")
    receipt, choice_observation = act(
        mcp,
        choice_observation,
        "select",
        "City",
        "select",
        lambda current: {
            "kind": "select",
            "option_object_id": named(current, "option", "Denver")["object_id"],
        },
    )
    receipts.append(receipt)
    if named(choice_observation, "select", "City").get("state", {}).get("expanded") != "false":
        raise RuntimeError("combobox popup did not settle closed after selection")
    adaptive_policy = adaptive_input_policy(mcp, url)
    evidence = {
        "mode": "controls",
        "browser": browser,
        "capabilities": capabilities,
        "initial_observation": initial,
        "choice_observation": choice_observation,
        "receipts": receipts,
        "adaptive_input_policy": adaptive_policy,
        "stale_token_rejected": stale_token_rejected,
    }
    serialized_evidence = json.dumps(evidence)
    leaked_classes = [
        index for index, sentinel in enumerate(REDACTED_VALUES)
        if sentinel in serialized_evidence
    ]
    if leaked_classes:
        raise RuntimeError(f"editable contents leaked into evidence classes {leaked_classes}")
    return evidence


def profile(mcp: Mcp, url: str, browser: str) -> dict[str, Any]:
    instructions = mcp.initialize.get("instructions", "")
    if "Saccade profile integration test." not in instructions:
        raise RuntimeError("Profile behavior was not placed in MCP instructions")
    observation = open_fixture(mcp, url)
    if any(item.get("name") == "Save" for item in observation["objects"]):
        raise RuntimeError("Profile-banned Save control reached MCP")
    return {
        "mode": "profile",
        "browser": browser,
        "initialize": mcp.initialize,
        "observation": observation,
    }


def reflex(
    mcp: Mcp,
    url: str,
    browser: str,
    mouse_backend: str,
    max_actions: int,
    timeout_ms: int,
) -> dict[str, Any]:
    settings: dict[str, Any] | None = None
    if url.rstrip("/") == "https://mouseaccuracy.com/game":
        settings = configure_mouseaccuracy(mcp)
    opened = mcp.tool("tabs.open", {"url": url, "active": True})
    wait_observation(mcp, opened["tab_id"])
    report = mcp.tool(
        "web.reflex.run",
        {
            "tab_id": opened["tab_id"],
            "input_backend": mouse_backend,
            "max_actions": max_actions,
            "timeout_ms": timeout_ms,
        },
        timeout=timeout_ms / 1000 + 20.0,
    )
    return {
        "mode": "reflex",
        "browser": browser,
        "url": url,
        "mouse_backend": mouse_backend,
        "settings": settings,
        "passed": report.get("actions", 0) > 0 and report.get("failures") == 0,
        "report": report,
    }


def mouseaccuracy_setting_value(
    observation: dict[str, Any], values: tuple[str, ...]
) -> str:
    names = {item.get("name") for item in observation.get("objects", [])}
    for value in values:
        if f"Decrease {value}" in names and f"Increase {value}" in names:
            return value
    raise RuntimeError(f"MouseAccuracy setting is not one of the audited values: {values}")


def drive_mouseaccuracy_setting(
    mcp: Mcp,
    observation: dict[str, Any],
    direction: str,
    values: tuple[str, ...],
) -> tuple[str, list[str]]:
    current = mouseaccuracy_setting_value(observation, values)
    transitions = [current]
    for _step in range(len(values) + 2):
        if current == values[-1]:
            return current, transitions
        for attempt in range(5):
            observation = stable_observation(mcp, observation["tab_id"])
            current = mouseaccuracy_setting_value(observation, values)
            target = named(observation, "button", f"{direction} {current}")
            request = {
                "browser_instance_id": observation["browser_instance_id"],
                "tab_id": observation["tab_id"],
                "document_id": observation["document_id"],
                "basis_revision": observation["revision"],
                "action_token": target["action_token"],
                "operation": "click",
                "payload": {"kind": "none"},
            }
            try:
                receipt = mcp.tool("web.act", request, timeout=15.0)
                observation = receipt["post_action_observation"]
                break
            except Exception as error:  # noqa: BLE001
                if attempt == 4 or "stale" not in str(error):
                    raise
        next_value = mouseaccuracy_setting_value(observation, values)
        current = next_value
        transitions.append(current)
    raise RuntimeError(
        f"MouseAccuracy setting did not reach audited endpoint {values[-1]}: {transitions}"
    )


def configure_mouseaccuracy(mcp: Mcp) -> dict[str, Any]:
    observation = open_fixture(mcp, "https://mouseaccuracy.com/")
    difficulty, difficulty_transitions = drive_mouseaccuracy_setting(
        mcp, observation, "Increase", MOUSEACCURACY_DIFFICULTY_VALUES
    )
    observation = stable_observation(mcp, observation["tab_id"])
    size, size_transitions = drive_mouseaccuracy_setting(
        mcp, observation, "Decrease", MOUSEACCURACY_SIZE_VALUES
    )
    return {
        "difficulty": difficulty,
        "target_size": size,
        "difficulty_transitions": difficulty_transitions,
        "target_size_transitions": size_transitions,
        "verified_highest": difficulty == MOUSEACCURACY_DIFFICULTY_VALUES[-1]
        and size == MOUSEACCURACY_SIZE_VALUES[-1],
    }


def accuracy_size(layout: str, difficulty: str, index: int) -> int:
    row = (index - 1) // 3
    column = (index - 1) % 3
    sizes = ACCURACY_SIZES[layout][difficulty]
    return sizes[(row + column) % len(sizes)]


def accuracy_url(base_url: str, layout: str, difficulty: str) -> str:
    parsed = urllib.parse.urlsplit(base_url)
    query = dict(urllib.parse.parse_qsl(parsed.query, keep_blank_values=True))
    query["layout"] = layout
    query["difficulty"] = difficulty
    return urllib.parse.urlunsplit((
        parsed.scheme,
        parsed.netloc,
        parsed.path,
        urllib.parse.urlencode(query),
        parsed.fragment,
    ))


def set_managed_window_geometry(
    window_pid: int, phase: tuple[int, str, int, int, int, int]
) -> None:
    _start, _name, x, y, width, height = phase
    script = """
on run argv
  set targetPid to item 1 of argv as integer
  set windowX to item 2 of argv as integer
  set windowY to item 3 of argv as integer
  set windowWidth to item 4 of argv as integer
  set windowHeight to item 5 of argv as integer
  tell application "System Events"
    set targetProcess to first application process whose unix id is targetPid
    tell front window of targetProcess
      set position to {windowX, windowY}
      set size to {windowWidth, windowHeight}
    end tell
  end tell
end run
"""
    result = subprocess.run(
        [
            "osascript",
            "-e",
            script,
            str(window_pid),
            str(x),
            str(y),
            str(width),
            str(height),
        ],
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(f"managed browser window geometry failed: {result.stderr.strip()}")
    time.sleep(0.35)


def mouse_accuracy(
    mcp: Mcp,
    url: str,
    browser: str,
    window_pid: int | None,
    layout: str,
    difficulty: str,
    mouse_backend: str,
) -> dict[str, Any]:
    if window_pid is None or window_pid <= 0:
        raise RuntimeError("mouse accuracy requires the exact managed browser PID")
    if mouse_backend == "soft" and layout != "canvas":
        raise RuntimeError("soft mouse accuracy is restricted to the reflex-target canvas fixture")
    phase = ACCURACY_WINDOW_PHASES[0]
    set_managed_window_geometry(window_pid, phase)
    url = accuracy_url(url, layout, difficulty)
    observation = open_fixture(mcp, url)
    trials: list[dict[str, Any]] = []
    for index in range(1, ACCURACY_TARGET_COUNT + 1):
        for candidate in ACCURACY_WINDOW_PHASES:
            if candidate[0] == index and candidate != phase:
                phase = candidate
                set_managed_window_geometry(window_pid, phase)
                break
        name = f"Accuracy {index:02d}"
        started = time.monotonic()
        try:
            receipt, observation = act(
                mcp,
                observation,
                "reflex_target" if layout == "canvas" else "button",
                name,
                "click",
                lambda _: {"kind": "none"},
                mouse_backend,
            )
            trial = {
                "target": name,
                "size_css_px": accuracy_size(layout, difficulty, index),
                "window_phase": phase[1],
                "hit": True,
                "dispatch_status": receipt["dispatch_status"],
                "postcondition": receipt["postcondition"],
            }
        except Exception as error:  # noqa: BLE001
            trial = {
                "target": name,
                "size_css_px": accuracy_size(layout, difficulty, index),
                "window_phase": phase[1],
                "hit": False,
                "error": redact_editable_values(str(error)),
            }
        trial["round_trip_ms"] = round((time.monotonic() - started) * 1000, 1)
        trials.append(trial)

    hits = sum(1 for trial in trials if trial["hit"])
    return {
        "mode": "mouse_accuracy",
        "browser": browser,
        "definition": f"Mouse-accuracy gate on {layout} layout ({difficulty}) with { 'soft' if mouse_backend == 'soft' else 'native' } center click.",
        "layout": layout,
        "difficulty": difficulty,
        "mouse_backend": mouse_backend,
        "url": url,
        "attempts": len(trials),
        "hits": hits,
        "misses": len(trials) - hits,
        "accuracy_percent": round(hits * 100 / len(trials), 2),
        "passed": hits == len(trials),
        "window_phases": [
            {
                "name": item[1],
                "starts_at_target": item[0],
                "position": [item[2], item[3]],
                "size": [item[4], item[5]],
            }
            for item in ACCURACY_WINDOW_PHASES
        ],
        "trials": trials,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["controls", "profile", "mouse_accuracy", "reflex"])
    parser.add_argument("--browser", choices=["chrome", "edge"], required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--url", required=True)
    parser.add_argument("--accuracy-layout", choices=ACCURACY_LAYOUTS, default="buttons")
    parser.add_argument("--accuracy-difficulty", choices=ACCURACY_DIFFICULTIES, default="ordinary")
    parser.add_argument("--mouse-backend", choices=MOUSE_BACKENDS, default="native")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--window-pid", type=int)
    parser.add_argument("--max-actions", type=int, default=500)
    parser.add_argument("--timeout-ms", type=int, default=30_000)
    args = parser.parse_args()

    try:
        mcp = wait_for_mcp(args.runtime.resolve(), args.runtime_dir.resolve())
        try:
            if args.mode == "controls":
                evidence = controls(mcp, args.url, args.browser)
            elif args.mode == "profile":
                evidence = profile(mcp, args.url, args.browser)
            elif args.mode == "mouse_accuracy":
                evidence = mouse_accuracy(
                    mcp,
                    args.url,
                    args.browser,
                    args.window_pid,
                    args.accuracy_layout,
                    args.accuracy_difficulty,
                    args.mouse_backend,
                )
            else:
                evidence = reflex(
                    mcp,
                    args.url,
                    args.browser,
                    args.mouse_backend,
                    args.max_actions,
                    args.timeout_ms,
                )
        finally:
            mcp.close()
        result = {"ok": evidence.get("passed", True), **evidence}
    except Exception as error:
        result = {
            "ok": False,
            "mode": args.mode,
            "browser": args.browser,
            "error": redact_editable_values(str(error)),
        }
    encoded = json.dumps(result, indent=2)
    if any(sentinel in encoded for sentinel in REDACTED_VALUES):
        raise RuntimeError("editable contents leaked into evidence")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(encoded + "\n", encoding="utf-8")
    print(json.dumps({"ok": result["ok"], "mode": args.mode, "evidence": str(args.output)}))
    if not result["ok"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
