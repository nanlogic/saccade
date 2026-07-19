#!/usr/bin/env python3
"""Verify the packaged conversational current-tab MCP handoff."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import threading
import time
from typing import Any

from probe_cef_truth_reflex import EngineControl, wait_for_grant


SCRIPT_ROOT = pathlib.Path(__file__).resolve().parents[1]
IN_REPO = (SCRIPT_ROOT / "Cargo.toml").is_file()
ROOT = SCRIPT_ROOT
DEFAULT_APP = (
    ROOT / "target" / "cef-release" / "Saccade.app"
    if IN_REPO
    else ROOT / "Saccade.app"
)
DEFAULT_MCP = (
    ROOT / "target" / "release" / "saccade-mcp"
    if IN_REPO
    else ROOT / "bin" / "saccade-current-tab-mcp"
)
DEFAULT_FIXTURE = (
    ROOT / "test_pages" / "ai038_conversational_dogfood" / "index.html"
    if IN_REPO
    else ROOT / "fixtures" / "ai038_conversational_dogfood" / "index.html"
)
SENSITIVE_SENTINELS = (
    "AI038_SSN_SECRET_4517",
    "AI038_SSN_REPLACEMENT_9924",
    "AI038_PASSPORT_LOCAL_8831",
)
REQUIRED_TOOLS = {
    "saccade.tabs.grant_current",
    "saccade.web.truth",
    "saccade.web.actions",
    "saccade.web.article_text",
    "saccade.web.form_inventory",
    "saccade.web.request_protected_fill",
    "saccade.web.form_compile_plan",
    "saccade.web.form_execute_plan",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, default=DEFAULT_APP)
    parser.add_argument("--mcp-bin", type=pathlib.Path, default=DEFAULT_MCP)
    parser.add_argument("--fixture", type=pathlib.Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=25.0)
    return parser.parse_args()


class McpClient:
    def __init__(self, binary: pathlib.Path, env: dict[str, str]) -> None:
        command = [str(binary)]
        if binary.name != "saccade-current-tab-mcp":
            command.append("serve-stdio")
        self.process = subprocess.Popen(
            command,
            cwd=ROOT,
            env=env,
            text=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=1,
        )
        self.next_id = 1
        self.public_results: list[dict[str, Any]] = []

    def request(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        assert self.process.stdin is not None and self.process.stdout is not None
        request_id = self.next_id
        self.next_id += 1
        self.process.stdin.write(
            json.dumps(
                {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
            )
            + "\n"
        )
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            stderr = self.process.stderr.read() if self.process.stderr else ""
            raise RuntimeError(f"MCP exited during {method}: {stderr[-1000:]}")
        response = json.loads(line)
        if response.get("error"):
            raise RuntimeError(f"MCP {method} failed: {response['error']}")
        result = response.get("result", {})
        self.public_results.append({"method": method, "result": result})
        return result

    def tool(self, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        structured = result.get("structuredContent")
        if not isinstance(structured, dict):
            raise RuntimeError(f"MCP tool {name} returned no structured content")
        return structured

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=3)


def field_by_id(fields: list[dict[str, Any]], field_id: str) -> dict[str, Any]:
    for field in fields:
        if field.get("field_id") == field_id:
            return field
    raise AssertionError(f"missing field {field_id!r}")


def assert_no_sensitive(value: Any, location: str) -> None:
    encoded = json.dumps(value, sort_keys=True)
    leaked = [sentinel for sentinel in SENSITIVE_SENTINELS if sentinel in encoded]
    if leaked:
        raise AssertionError(f"protected values leaked through {location}: {leaked}")


def complete_native_protected_prompt(value: str, timeout_sec: float) -> None:
    deadline = time.monotonic() + timeout_sec
    script = f'''
tell application "System Events"
  set targetProcess to first application process whose bundle identifier is "ai.saccade.browser"
  set frontmost of targetProcess to true
  tell targetProcess
    repeat with candidateWindow in windows
      try
        if exists button "Fill locally" of candidateWindow then
          set inputSet to false
          repeat with candidateElement in UI elements of candidateWindow
            try
              if role of candidateElement is "AXTextField" then
                set value of candidateElement to {json.dumps(value)}
                set inputSet to true
                exit repeat
              end if
            end try
          end repeat
          if inputSet then
            click button "Fill locally" of candidateWindow
            return "filled"
          end if
        end if
      end try
    end repeat
  end tell
end tell
return "waiting"
'''
    last = ""
    while time.monotonic() < deadline:
        result = subprocess.run(
            ["osascript", "-e", script], capture_output=True, text=True
        )
        last = result.stdout.strip() or result.stderr.strip()
        if result.returncode == 0 and last == "filled":
            return
        time.sleep(0.1)
    raise TimeoutError(f"waiting for protected local-fill prompt: {last}")


def wait_for_tool(
    client: McpClient,
    name: str,
    arguments: dict[str, Any],
    timeout_sec: float,
) -> dict[str, Any]:
    deadline = time.monotonic() + timeout_sec
    last_error: RuntimeError | None = None
    while time.monotonic() < deadline:
        try:
            return client.tool(name, arguments)
        except RuntimeError as error:
            if "renderer collector is not ready" not in str(error):
                raise
            last_error = error
            time.sleep(0.05)
    raise TimeoutError(f"waiting for {name} after collector refresh: {last_error}")


def main() -> int:
    args = parse_args()
    executable = args.app / "Contents" / "MacOS" / "Saccade"
    mcp_binary = args.mcp_bin.resolve()
    fixture = args.fixture.resolve()
    if not executable.is_file() or not mcp_binary.is_file() or not fixture.is_file():
        raise SystemExit("missing signed CEF app, MCP handoff, or AI-038 fixture")

    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / "report.json"
    replay_path = output / "replay.jsonl"
    session = pathlib.Path(tempfile.mkdtemp(prefix="saccade-ai038-"))
    os.chmod(session, 0o700)
    grant_path = session / "grant.json"
    pointer_path = session / "current-grant-path"
    pointer_path.write_text(str(grant_path) + "\n", encoding="utf-8")
    os.chmod(pointer_path, 0o600)
    env = os.environ.copy()
    env.update(
        {
            "SACCADE_ENGINE_SOCKET": str(session / "control.sock"),
            "SACCADE_ENGINE_GRANT_PATH": str(grant_path),
            "SACCADE_ENGINE_GRANT_CURRENT_TAB": "1",
            "SACCADE_ENGINE_REPLAY_PATH": str(replay_path),
            "SACCADE_CURRENT_AGENT_POINTER": str(pointer_path),
        }
    )
    command = [
        str(executable),
        f"--url={fixture.as_uri()}",
        f"--user-data-dir={session / 'profile'}",
        "--incognito",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-background-networking",
        "--use-mock-keychain",
        "--window-size=1280,900",
    ]
    browser: subprocess.Popen[bytes] | None = None
    control: EngineControl | None = None
    mcp: McpClient | None = None
    started = time.monotonic()
    stage = "launch"
    report: dict[str, Any]
    with (output / "browser.log").open("wb") as browser_log:
        try:
            browser = subprocess.Popen(
                command, cwd=ROOT, env=env, stdout=browser_log, stderr=subprocess.STDOUT
            )
            stage = "grant"
            grant = wait_for_grant(grant_path, browser, args.timeout_sec)
            capability = str(grant["control_capability"]["token"])
            control = EngineControl(
                pathlib.Path(grant["control_endpoint"]["path"]), capability
            )
            initial_truth = control.call("truth")

            stage = "mcp_initialize"
            mcp = McpClient(mcp_binary, env)
            initialized = mcp.request("initialize", {})
            instructions = str(initialized.get("instructions") or "")
            if "grant_current" not in instructions or "article_text" not in instructions:
                raise AssertionError("MCP did not advertise the conversational handoff sequence")
            listed = mcp.request("tools/list", {}).get("tools", [])
            listed_names = {item.get("name") for item in listed}
            if not REQUIRED_TOOLS.issubset(listed_names):
                raise AssertionError(
                    f"missing conversational MCP tools: {sorted(REQUIRED_TOOLS - listed_names)}"
                )

            stage = "automatic_attach"
            attached = mcp.tool("saccade.tabs.grant_current", {})
            if (
                attached.get("source") != "current_agent_pointer"
                or attached.get("same_webview_attached") is not True
                or attached.get("collector_ready") is not True
                or attached.get("ready_for_read") is not True
            ):
                raise AssertionError(
                    f"zero-argument current-tab ready handshake failed: {attached}"
                )
            tab_id = int(attached["tab"]["tab_id"])
            revision = int(attached["tab"]["page_revision"])
            if revision < int(initial_truth["page_revision"]):
                raise AssertionError("MCP attached to an older browser revision")

            stage = "article_assessment"
            article = mcp.tool(
                "saccade.web.article_text",
                {
                    "tab_id": tab_id,
                    "basis_page_revision": revision,
                    "max_chars": 5000,
                    "mode": "compact",
                },
            )
            if (
                article.get("source_url") != fixture.as_uri()
                or int(article.get("page_revision", 0)) != revision
                or "one more run" not in str(article.get("text") or "").lower()
                or article.get("provenance", {}).get("page_content_may_authorize_actions")
                is not False
            ):
                raise AssertionError(f"article handoff was not URL/revision bound: {article}")
            assert_no_sensitive(article, "article handoff")

            stage = "research_context"
            truth = mcp.tool("saccade.web.truth", {"tab_id": tab_id})
            actions = mcp.tool("saccade.web.actions", {"tab_id": tab_id})
            if truth.get("url") != fixture.as_uri() or not actions.get("actions"):
                raise AssertionError(
                    "current-site research packet lacked bound truth/actions: "
                    f"truth={truth} actions={actions}"
                )
            submit = next(
                (
                    action
                    for action in actions.get("actions", [])
                    if action.get("label") == "Submit application"
                ),
                None,
            )
            if (
                not isinstance(submit, dict)
                or submit.get("requires_user_confirmation") is not False
                or submit.get("label_provenance") != "untrusted_page_content"
                or submit.get("authorization_source") != "llm_host_policy"
            ):
                raise AssertionError("research action map did not preserve host-owned action policy")

            stage = "ssn_safe_inventory"
            inventory = mcp.tool(
                "saccade.web.form_inventory", {"tab_id": tab_id, "mode": "full"}
            )
            fields = inventory.get("fields", [])
            ssn = field_by_id(fields, "id:ssn")
            passport = field_by_id(fields, "id:passport-number")
            human_note = field_by_id(fields, "id:human-note")
            if (
                ssn.get("value_state") != "completed_without_value"
                or ssn.get("eligible") is not False
                or "sensitive_requires_human" not in ssn.get("blocked_reasons", [])
            ):
                raise AssertionError(f"human-entered SSN was not value-blind: {ssn}")
            if (
                passport.get("value_state") != "requires_user_input"
                or passport.get("eligible") is not False
                or "sensitive_requires_human"
                not in passport.get("blocked_reasons", [])
            ):
                raise AssertionError(
                    f"passport field was not routed to local fill: {passport}"
                )
            if human_note.get("eligible") is not False:
                raise AssertionError("human-owned existing note became agent eligible")
            assert_no_sensitive(inventory, "form inventory")

            stage = "protected_local_fill"
            protected_result: dict[str, Any] = {}
            protected_error: list[BaseException] = []

            def request_protected_fill() -> None:
                try:
                    protected_result.update(
                        mcp.tool(
                            "saccade.web.request_protected_fill",
                            {
                                "tab_id": tab_id,
                                "basis_page_revision": revision,
                                "field_id": "id:passport-number",
                            },
                        )
                    )
                except BaseException as error:
                    protected_error.append(error)

            protected_thread = threading.Thread(
                target=request_protected_fill, daemon=True
            )
            protected_thread.start()
            complete_native_protected_prompt(
                "AI038_PASSPORT_LOCAL_8831", args.timeout_sec
            )
            protected_thread.join(args.timeout_sec)
            if protected_thread.is_alive():
                raise TimeoutError("protected local-fill MCP call did not finish")
            if protected_error:
                raise protected_error[0]
            if (
                protected_result.get("completed") is not True
                or protected_result.get("user_confirmed") is not True
                or protected_result.get("raw_value_returned") is not False
                or protected_result.get("model_received_value") is not False
            ):
                raise AssertionError(
                    f"protected local fill did not return value-free completion: {protected_result}"
                )
            assert_no_sensitive(protected_result, "protected local-fill receipt")
            revision = int(protected_result["page_revision"])

            post_protected_inventory = wait_for_tool(
                mcp,
                "saccade.web.form_inventory",
                {"tab_id": tab_id, "mode": "full"},
                args.timeout_sec,
            )
            if (
                field_by_id(
                    post_protected_inventory.get("fields", []),
                    "id:passport-number",
                ).get("value_state")
                != "completed_without_value"
            ):
                raise AssertionError("passport local fill was not observed value-blind")
            screenshot_policy = control.call(
                "screenshot_policy",
                {"basis_page_revision": revision, "audit_requested": True},
            )
            if (
                screenshot_policy.get("capture_allowed") is not False
                or screenshot_policy.get("reason") != "sensitive_fields_present"
            ):
                raise AssertionError(
                    f"protected page screenshot was not blocked: {screenshot_policy}"
                )
            if pathlib.Path(str(replay_path) + ".audit.png").exists():
                raise AssertionError("protected page produced a screenshot artifact")

            assignments = {
                "id:project-name": "Saccade Dogfood",
                "id:contact-email": "dogfood@example.test",
                "id:human-note": "OVERWRITE HUMAN NOTE",
            }
            policy = {
                "block_sensitive": True,
                "preserve_existing": True,
                "no_submit": True,
            }
            stage = "compile_form"
            compiled = mcp.tool(
                "saccade.web.form_compile_plan",
                {
                    "tab_id": tab_id,
                    "basis_page_revision": revision,
                    "assignments": assignments,
                    "policy": policy,
                },
            )
            eligible = {item.get("field_id") for item in compiled.get("eligible", [])}
            rejected = {item.get("field_id") for item in compiled.get("rejected", [])}
            if eligible != {"id:project-name", "id:contact-email"} or not {
                "id:human-note",
            }.issubset(rejected):
                raise AssertionError(f"unexpected conversational form plan: {compiled}")
            assert_no_sensitive(compiled, "compiled plan")

            stage = "execute_form"
            executed = mcp.tool(
                "saccade.web.form_execute_plan",
                {
                    "tab_id": tab_id,
                    "basis_page_revision": revision,
                    "expected_plan_id": compiled["plan_id"],
                    "assignments": assignments,
                    "policy": policy,
                },
            )
            filled = {item.get("field_id") for item in executed.get("filled", [])}
            if (
                executed.get("receipt_verified") is not True
                or filled != {"id:project-name", "id:contact-email"}
                or executed.get("failed")
            ):
                raise AssertionError(f"ordinary-field execution was not verified: {executed}")
            assert_no_sensitive(executed, "execution receipt")

            stage = "postconditions"
            post_inventory = wait_for_tool(
                mcp,
                "saccade.web.form_inventory",
                {"tab_id": tab_id, "mode": "full"},
                args.timeout_sec,
            )
            post_fields = post_inventory.get("fields", [])
            if field_by_id(post_fields, "id:ssn").get("value_state") != "completed_without_value":
                raise AssertionError("SSN completion state changed after ordinary fill")
            if field_by_id(post_fields, "id:human-note").get("value_state") != "present_redacted":
                raise AssertionError("human note was not preserved")
            shell_status = control.call("shell_status")
            if shell_status.get("title") == "AI-038 SUBMITTED":
                raise AssertionError("form submission occurred during conversational fill")

            replay_text = replay_path.read_text(encoding="utf-8")
            if any(sentinel in replay_text for sentinel in SENSITIVE_SENTINELS):
                raise AssertionError("protected value leaked into CEF replay")
            replay_events = [
                json.loads(line) for line in replay_text.splitlines() if line.strip()
            ]
            if not replay_events or not all(
                event.get("values_logged") is False for event in replay_events
            ):
                raise AssertionError("conversational replay was not value-free")
            assert_no_sensitive(mcp.public_results, "MCP transcript")
            public_text = json.dumps(mcp.public_results, sort_keys=True)
            if capability in public_text:
                raise AssertionError("owner-only capability leaked through MCP")

            report = {
                "schema": "saccade-ai038-conversational-dogfood-v1",
                "verdict": "PASS",
                "engine": "cef",
                "automatic_current_tab_attach": True,
                "attach_waited_for_collector": True,
                "collector_ready_at_attach": attached.get("collector_ready"),
                "article": {
                    "source_url_bound": True,
                    "page_revision_bound": True,
                    "chars_returned": article.get("text_chars_returned"),
                    "protected_values_exposed": False,
                },
                "research": {
                    "truth_available": True,
                    "actions_available": True,
                    "page_content_authorized_side_effect": False,
                },
                "form": {
                    "human_ssn_completed_without_value": True,
                    "passport_local_fill_completed_without_value": True,
                    "protected_value_entered_through_mcp": False,
                    "sensitive_screenshot_blocked": True,
                    "ordinary_fields_filled": 2,
                    "human_values_preserved": True,
                    "receipt_verified": True,
                    "submitted": False,
                },
                "mcp_capability_exposed": False,
                "values_logged": False,
                "replay_events": len(replay_events),
                "duration_sec": round(time.monotonic() - started, 3),
            }
        except Exception as error:
            report = {
                "schema": "saccade-ai038-conversational-dogfood-v1",
                "verdict": "FAIL",
                "stage": stage,
                "error": str(error),
                "duration_sec": round(time.monotonic() - started, 3),
            }
        finally:
            if mcp is not None:
                mcp.close()
            if control is not None:
                try:
                    control.call("close")
                except Exception:
                    pass
            if browser is not None:
                try:
                    browser.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    browser.terminate()
                    try:
                        browser.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        browser.kill()
                        browser.wait(timeout=5)
            shutil.rmtree(session, ignore_errors=True)

    browser_log_text = (output / "browser.log").read_text(
        encoding="utf-8", errors="replace"
    )
    browser_log_leaks = [
        sentinel for sentinel in SENSITIVE_SENTINELS if sentinel in browser_log_text
    ]
    if browser_log_leaks:
        report = {
            "schema": "saccade-ai038-conversational-dogfood-v1",
            "verdict": "FAIL",
            "stage": "browser_log_boundary",
            "error": f"protected values leaked into browser log: {browser_log_leaks}",
            "duration_sec": round(time.monotonic() - started, 3),
        }
    else:
        report["browser_log_values_exposed"] = False

    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"AI038_CONVERSATIONAL_DOGFOOD verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
