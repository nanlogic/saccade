"""Same-model fair benchmark driver: contract tests."""

from __future__ import annotations

import importlib.util
import json
import os
import sys
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "benchmark_same_model_fair", ROOT / "scripts/benchmark_same_model_fair.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

TASK = ROOT / "benchmarks/tasks/selenium_web_form.json"
LOCAL_TASK = ROOT / "benchmarks/tasks/local_form.json"
LOCAL_FIXTURE = ROOT / "fixtures/benchmarks/form.html"


def assistant(identifier: str, name: str, payload: dict | None = None) -> str:
    return json.dumps({"type": "assistant", "message": {"content": [
        {"type": "tool_use", "id": identifier, "name": name, "input": payload or {}}]}})


def result(identifier: str, content) -> str:
    return json.dumps({"type": "user", "message": {"content": [
        {"type": "tool_result", "tool_use_id": identifier, "content": content}]}})


def final(completed: bool = True, summary: str = "done", usage: dict | None = None) -> str:
    return json.dumps({
        "type": "result", "subtype": "success", "is_error": False,
        "result": json.dumps({"completed": completed, "summary": summary}),
        "usage": usage if usage is not None else {"input_tokens": 1200, "output_tokens": 90},
    })


class SameModelDriver(unittest.TestCase):
    def test_local_form_comparison_has_no_external_submission(self) -> None:
        task = MODULE.load_task(LOCAL_TASK)
        fixture = LOCAL_FIXTURE.read_text(encoding="utf-8")
        self.assertTrue(task["url"].startswith("http://127.0.0.1:8765/"))
        self.assertIn("BENCHMARK-COMPLETE", task["success"]["tool_output_contains"])
        self.assertIn("event.preventDefault()", fixture)
        self.assertIn("BENCHMARK-COMPLETE", fixture)

    def test_both_lanes_use_one_claude_binary_and_one_model(self) -> None:
        task = MODULE.load_task(TASK)
        common = dict(task=task, model="claude-opus-5", runtime=Path("/rt"),
                      runtime_dir=Path("/rtdir"), playwright_package="@playwright/mcp@0.0.79")
        saccade = MODULE.lane_command("saccade", **common)
        playwright = MODULE.lane_command("playwright", **common)
        for command in (saccade, playwright):
            self.assertEqual(command[0], "claude")
            self.assertIn("--model", command)
            self.assertEqual(command[command.index("--model") + 1], "claude-opus-5")
            self.assertIn("--strict-mcp-config", command)
            self.assertIn("--output-format", command)
            self.assertEqual(command[command.index("--output-format") + 1], "stream-json")

    def test_each_lane_connects_only_its_own_browser_mcp(self) -> None:
        task = MODULE.load_task(TASK)
        common = dict(task=task, model=None, runtime=Path("/rt"), runtime_dir=Path("/rtdir"),
                      playwright_package="@playwright/mcp@0.0.79")
        saccade = MODULE.lane_command("saccade", **common)
        playwright = MODULE.lane_command("playwright", **common)
        saccade_servers = json.loads(saccade[saccade.index("--mcp-config") + 1])["mcpServers"]
        playwright_servers = json.loads(playwright[playwright.index("--mcp-config") + 1])["mcpServers"]
        self.assertEqual(list(saccade_servers), ["saccade"])
        self.assertEqual(list(playwright_servers), ["playwright"])
        self.assertIn("@playwright/mcp@0.0.79", playwright_servers["playwright"]["args"])
        self.assertEqual(saccade_servers["saccade"]["env"]["SACCADE_RUNTIME_DIR"], "/rtdir")
        # Neither lane gets a client-owned executor: Saccade executes through
        # saccade.act and Playwright through its own tools, so the comparison
        # varies the engine and nothing else.
        self.assertNotIn("--chrome", saccade)
        self.assertNotIn("--chrome", playwright)

    def test_playwright_package_comes_from_the_lock(self) -> None:
        lock = MODULE.load_playwright_lock()
        self.assertEqual(lock["package"], "@playwright/mcp")
        self.assertEqual(lock["version"], "0.0.79")
        self.assertTrue(lock["online_latest_verified"])

    def test_both_lanes_get_the_same_url_goal_and_success_condition(self) -> None:
        task = MODULE.load_task(TASK)
        saccade = MODULE.prompt_for(task, "saccade")
        playwright = MODULE.prompt_for(task, "playwright")
        for prompt in (saccade, playwright):
            self.assertIn(task["url"], prompt)
            self.assertIn(task["task"], prompt)
            for needle in task["success"]["tool_output_contains"]:
                self.assertIn(needle, prompt)

    def test_saccade_prompt_uses_working_set_then_delta(self) -> None:
        prompt = MODULE.prompt_for(MODULE.load_task(TASK), "saccade")
        self.assertIn("saccade.system.capabilities once", prompt)
        self.assertIn("browser_family=chrome", prompt)
        self.assertIn("mode=full", prompt)
        self.assertIn("top-level min_objects", prompt)
        self.assertIn("query={roles:", prompt)
        self.assertIn("compact_rows/1", prompt)
        self.assertIn("saccade.act steps batch", prompt)
        self.assertIn("next_basis_revision", prompt)
        self.assertIn("never retry an ambiguous side effect", prompt)
        self.assertNotIn("text_any", prompt)
        self.assertNotIn("visible_only", prompt)
        self.assertNotIn("frame_scope", prompt)
        self.assertNotIn("view_mode", prompt)

    def test_wrapper_timestamps_every_request_and_return(self) -> None:
        stdout = "\n".join([
            assistant("a", "mcp__saccade__saccade_truth_read", {"view_mode": "index"}),
            result("a", "x" * 10),
            assistant("b", "mcp__claude-in-chrome__computer"),
            result("b", "clicked"),
            final(),
        ])
        parsed = MODULE.trace_events(stdout, "saccade", MODULE.time.monotonic())
        self.assertEqual(len(parsed["trace"]), 2)
        for call in parsed["trace"]:
            self.assertIsInstance(call["requested_ms"], float)
            self.assertIsInstance(call["returned_ms"], float)
            self.assertIsInstance(call["duration_ms"], float)
            self.assertGreaterEqual(call["returned_ms"], call["requested_ms"])
            self.assertIsInstance(call["response_bytes"], int)

    def test_tokens_are_read_from_stream_json_usage(self) -> None:
        stdout = "\n".join([assistant("a", "truth.read"), result("a", "v"),
                            final(usage={"input_tokens": 4321, "output_tokens": 77})])
        parsed = MODULE.trace_events(stdout, "saccade", MODULE.time.monotonic())
        self.assertEqual(parsed["usage"]["input_tokens"], 4321)
        self.assertEqual(parsed["usage"]["output_tokens"], 77)

    def test_discovery_bytes_accumulate_index_and_every_region(self) -> None:
        trace = [
            {"role": "navigate", "response_bytes": 50, "view_mode": None},
            {"role": "observe", "response_bytes": 100, "view_mode": "index"},
            {"role": "observe", "response_bytes": 400, "view_mode": "region"},
            {"role": "observe", "response_bytes": 500, "view_mode": "region"},
            {"role": "execute", "response_bytes": 10, "view_mode": None},
            {"role": "observe", "response_bytes": 9999, "view_mode": "full"},
        ]
        found = MODULE.discovery_bytes(trace)
        # index + both regions, and nothing after the first execution call
        self.assertEqual(found["initial_transfer_bytes"], 1000)
        self.assertEqual(found["discovery_observation_calls"], 3)
        self.assertEqual(found["discovery_view_modes"], ["index", "region", "region"])

    def test_query_is_recorded_as_a_working_set_discovery_mode(self) -> None:
        self.assertEqual(
            MODULE.view_mode_of({"query": {"roles": ["text_field"]}}),
            "working_set",
        )

    def test_delta_latency_is_execution_return_to_next_observation(self) -> None:
        trace = [
            {"role": "execute", "returned_ms": 1000.0},
            {"role": "observe", "returned_ms": 1042.5},
            {"role": "execute", "returned_ms": 2000.0},
            {"role": "observe", "returned_ms": 2011.0},
        ]
        self.assertEqual(MODULE.delta_latencies(trace), [42.5, 11.0])

    def test_saccade_act_inline_transition_is_detected_without_an_extra_read(self) -> None:
        content = {
            "verified": False,
            "relevant_delta": {"schema": "saccade.action-delta/1",
                               "changed_steps": [0]},
        }
        self.assertTrue(MODULE.carries_truth_transition(
            "mcp__saccade__saccade_act", content
        ))
        self.assertFalse(MODULE.carries_truth_transition(
            "mcp__playwright__browser_click", content
        ))

    def test_object_transition_metadata_is_not_mistaken_for_inline_truth(self) -> None:
        content = {"objects": [{"transition": "none"}], "mode": "delta"}
        self.assertFalse(MODULE.carries_truth_transition(
            "mcp__saccade__saccade_act", content
        ))

    def test_classify_recognises_real_mcp_prefixed_tool_names(self) -> None:
        """Live traces carry mcp__<server>__<tool> with underscores, not bare dotted names."""
        self.assertEqual(MODULE.classify("mcp__saccade__saccade_truth_read", "saccade"), "observe")
        self.assertEqual(MODULE.classify("mcp__saccade__saccade_tabs_open", "saccade"), "navigate")
        self.assertEqual(MODULE.classify("mcp__saccade__saccade_tabs_list", "saccade"), "navigate")
        self.assertEqual(
            MODULE.classify("mcp__claude-in-chrome__computer", "saccade"), "execute")
        self.assertEqual(
            MODULE.classify("mcp__playwright__browser_click", "playwright"), "execute")

    def test_claude_in_chrome_readonly_tools_do_not_end_discovery(self) -> None:
        """The 'chrome' in the server segment must not make every tool an execution."""
        for tool in ("mcp__claude-in-chrome__tabs_context_mcp",
                     "mcp__claude-in-chrome__list_connected_browsers"):
            self.assertNotEqual(MODULE.classify(tool, "saccade"), "execute", tool)

    def test_playwright_find_counts_as_observation(self) -> None:
        """browser_find is the observation tool the locked 0.0.79 server actually exposes."""
        self.assertEqual(MODULE.classify("mcp__playwright__browser_find", "playwright"), "observe")
        self.assertEqual(MODULE.classify("mcp__playwright__browser_snapshot", "playwright"),
                         "observe")

    def test_navigation_is_not_page_payload_on_either_lane(self) -> None:
        """Navigation confirmations carry no page facts, so neither lane may bank them."""
        self.assertEqual(
            MODULE.classify("mcp__playwright__browser_navigate", "playwright"), "navigate")
        self.assertEqual(MODULE.classify("mcp__saccade__saccade_tabs_open", "saccade"), "navigate")

    def test_harness_tools_are_never_browser_evidence(self) -> None:
        for lane in ("saccade", "playwright"):
            self.assertEqual(MODULE.classify("ToolSearch", lane), "other")

    def test_real_trace_yields_discovery_bytes_and_delta_latency(self) -> None:
        """End-to-end guard: a realistic prefixed trace must produce both metrics."""
        trace = [
            {"tool": "ToolSearch", "response_bytes": 373, "view_mode": None, "returned_ms": 6.0},
            {"tool": "mcp__saccade__saccade_tabs_open", "response_bytes": 72,
             "view_mode": None, "returned_ms": 14.0},
            {"tool": "mcp__saccade__saccade_truth_read", "response_bytes": 2037,
             "view_mode": "auto", "returned_ms": 22.0},
            {"tool": "mcp__claude-in-chrome__computer", "response_bytes": 92,
             "view_mode": None, "returned_ms": 68.0},
            {"tool": "mcp__saccade__saccade_truth_read", "response_bytes": 500,
             "view_mode": "region", "returned_ms": 75.0},
        ]
        for call in trace:
            call["role"] = MODULE.classify(call["tool"], "saccade")
        found = MODULE.discovery_bytes(trace)
        self.assertEqual(found["initial_transfer_bytes"], 2037)
        self.assertEqual(found["discovery_observation_calls"], 1)
        self.assertEqual(MODULE.delta_latencies(trace), [7.0])

    def test_missing_required_evidence_is_invalid(self) -> None:
        complete = {
            "timing": {"clock_source": "wrapper_monotonic", "elapsed_ms": 10.0},
            "usage": {"input_tokens": 5, "output_tokens": 2},
            "tool_calls": 3,
            "browser_metrics": {"initial_transfer_bytes": 100,
                                "action_return_to_delta_read_ms": 12.0,
                                "dynamic_replacement_recoveries": 0},
        }
        self.assertEqual(MODULE.lane_evidence_errors(complete), [])
        for field, expected in (
            ("input_tokens", "model_input_tokens_missing"),
            ("output_tokens", "model_output_tokens_missing"),
        ):
            broken = json.loads(json.dumps(complete))
            broken["usage"].pop(field)
            self.assertIn(expected, MODULE.lane_evidence_errors(broken))
        for field, expected in (
            ("initial_transfer_bytes", "discovery_transfer_bytes_missing"),
            ("action_return_to_delta_read_ms", "delta_latency_missing"),
            ("dynamic_replacement_recoveries", "dynamic_replacement_recovery_count_missing"),
        ):
            broken = json.loads(json.dumps(complete))
            broken["browser_metrics"].pop(field)
            self.assertIn(expected, MODULE.lane_evidence_errors(broken))

    def test_order_is_proven_by_timestamps(self) -> None:
        lanes = {
            "saccade": {"timing": {"started_at": "2026-08-17T10:00:00Z",
                                   "completed_at": "2026-08-17T10:01:00Z"}},
            "playwright": {"timing": {"started_at": "2026-08-17T10:02:00Z",
                                      "completed_at": "2026-08-17T10:03:00Z"}},
        }
        self.assertEqual(MODULE.validate_order(lanes, "saccade-first"), [])
        self.assertTrue(MODULE.validate_order(lanes, "playwright-first"))

    def test_a_lane_that_never_reached_its_mcp_is_named_not_credited(self) -> None:
        parsed = MODULE.trace_events(final(completed=False), "playwright", MODULE.time.monotonic())
        self.assertEqual(parsed["trace"], [])

    def test_stream_event_stamps_are_preserved_instead_of_recreated_after_exit(self) -> None:
        parsed = MODULE.trace_events([
            (10.0, assistant("a", "truth.read")),
            (45.5, result("a", "truth")),
            (50.0, final()),
        ], "saccade", MODULE.time.monotonic())
        self.assertEqual(parsed["trace"][0]["requested_ms"], 10.0)
        self.assertEqual(parsed["trace"][0]["returned_ms"], 45.5)
        self.assertEqual(parsed["trace"][0]["duration_ms"], 35.5)

    def test_streaming_reader_stamps_lines_when_they_arrive(self) -> None:
        started = time.monotonic()
        stdout, stderr, returncode, timed_out, stamped = MODULE.run_streaming(
            [sys.executable, "-c", "import time; print('one', flush=True); time.sleep(.05); print('two', flush=True)"],
            2, os.environ.copy(), started,
        )
        self.assertEqual((returncode, timed_out, stderr), (0, False, ""))
        self.assertEqual(stdout.splitlines(), ["one", "two"])
        self.assertGreaterEqual(stamped[1][0] - stamped[0][0], 30)

    def test_streaming_reader_splits_tail_lines_after_fast_exit(self) -> None:
        started = time.monotonic()
        stdout, stderr, returncode, timed_out, stamped = MODULE.run_streaming(
            [sys.executable, "-c", "print('one'); print('two'); print('three')"],
            2, os.environ.copy(), started,
        )
        self.assertEqual((returncode, timed_out, stderr), (0, False, ""))
        self.assertEqual(stdout.splitlines(), ["one", "two", "three"])
        self.assertEqual([line for _, line in stamped], ["one", "two", "three"])

    def test_auth_failure_is_not_misclassified_as_browser_mcp_failure(self) -> None:
        task = MODULE.load_task(TASK)
        original = MODULE.run_streaming
        try:
            MODULE.run_streaming = lambda *args, **kwargs: (
                final(False, "Not logged in · Please run /login"), "", 1, False,
                [(1.0, final(False, "Not logged in · Please run /login"))],
            )
            with __import__("tempfile").TemporaryDirectory() as directory:
                lane = MODULE.run_lane(
                    "saccade", task, None, Path("/rt"), Path("/rtdir"),
                    "@playwright/mcp@0.0.79", Path(directory),
                )
            self.assertEqual(lane["failure_reason"], "claude_cli_not_authenticated")
        finally:
            MODULE.run_streaming = original

    def test_tasks_carry_no_selector_or_site_logic(self) -> None:
        for path in sorted((ROOT / "benchmarks/tasks").glob("*.json")):
            serialized = json.dumps(MODULE.load_task(path)).casefold()
            for forbidden in ("#my-", "[name=", "button[type", "xpath", "selector"):
                self.assertNotIn(forbidden, serialized, path.name)


if __name__ == "__main__":
    unittest.main()


class EngineRouteContract(unittest.TestCase):
    """The Saccade lane must use the agent-client claim route and prove same-tab."""

    def setUp(self):
        self.task = json.loads(TASK.read_text(encoding="utf-8"))

    def test_saccade_prompt_pins_truth_observation_and_act_execution(self):
        prompt = MODULE.prompt_for(self.task, "saccade")
        for needle in ("saccade.truth.read", "saccade.act", "object_id",
                       "Never pass a coordinate", "never take a screenshot",
                       "external_execution_required", "strictly"):
            self.assertIn(needle, prompt)

    def test_saccade_lane_has_no_claude_browser_or_chrome_flag(self):
        command = MODULE.lane_command(
            "saccade", self.task, "claude-opus-5", Path("/rt"), Path("/rtd"),
            "@playwright/mcp@0.0.79", "low",
        )
        serialized = " ".join(command).casefold()
        self.assertNotIn("claude-in-chrome", serialized)
        self.assertNotIn("--chrome", command)
        self.assertEqual(command[command.index("--allowedTools") + 1], "mcp__saccade__*")

    def test_both_lanes_receive_identical_model_and_effort(self):
        commands = {
            lane: MODULE.lane_command(lane, self.task, "claude-opus-5", Path("/rt"),
                                      Path("/rtd"), "@playwright/mcp@1.0.0", "low")
            for lane in ("saccade", "playwright")
        }
        for lane, command in commands.items():
            self.assertIn("--model", command, lane)
            self.assertEqual(command[command.index("--model") + 1], "claude-opus-5", lane)
            self.assertIn("--effort", command, lane)
            self.assertEqual(command[command.index("--effort") + 1], "low", lane)

    def test_each_lane_explicitly_allows_only_its_strict_mcp_namespace(self):
        for lane in MODULE.LANES:
            command = MODULE.lane_command(
                lane, self.task, "sonnet", Path("/runtime"), Path("/state"),
                "@playwright/mcp@0.0.79", "low", "edge",
            )
            self.assertIn("--allowedTools", command)
            self.assertEqual(
                command[command.index("--allowedTools") + 1], f"mcp__{lane}__*", lane
            )

    def test_chrome_tab_creation_is_navigation_not_discovery_payload(self):
        for tool in ("mcp__claude-in-chrome__tabs_create_mcp",
                     "mcp__claude-in-chrome__navigate",
                     "mcp__claude-in-chrome__tabs_context_mcp"):
            self.assertEqual(MODULE.classify(tool, "saccade"), "navigate", tool)

    def _trace(self, **overrides):
        trace = [
            {"sequence": 1, "tool": "mcp__saccade__saccade_tabs_open", "role": "navigate",
             "claim_evidence": {"claim": "arm"}, "claim_result": {"claim": "armed"}},
            {"sequence": 2, "tool": "mcp__claude-in-chrome__tabs_create_mcp", "role": "navigate",
             "claim_evidence": None, "claim_result": None},
            {"sequence": 3, "tool": "mcp__saccade__saccade_tabs_open", "role": "navigate",
             "claim_evidence": {"claim": "confirm", "tab_id": "77", "claim_id_prefix": "claim.abc…"},
             "claim_result": {"claim": "confirmed", "tab_id": "77",
                              "provenance": "agent_client", "opened": "false"}},
            {"sequence": 4, "tool": "mcp__saccade__saccade_tabs_list", "role": "navigate",
             "claim_evidence": None,
             "claim_result": {"provenance": "agent_client", "ownership": "agent",
                              "observation_ready": "true"}},
            {"sequence": 5, "tool": "mcp__saccade__saccade_truth_read", "role": "observe",
             "claim_evidence": {"tab_id": "77"}, "claim_result": None},
            {"sequence": 6, "tool": "mcp__claude-in-chrome__computer", "role": "execute",
             "claim_evidence": {"tab_id": "77"}, "claim_result": None},
        ]
        trace.extend(overrides.get("extra", []))
        return trace

    def test_full_claim_route_on_one_tab_is_accepted(self):
        proof = MODULE.claim_proof(self._trace())
        self.assertTrue(proof["armed"])
        self.assertTrue(proof["client_created_tab"])
        self.assertTrue(proof["same_tab"])
        self.assertFalse(proof["claimless_tabs_open_used"])
        self.assertEqual(proof["confirmed"]["provenance"], "agent_client")
        # Only a truncated prefix may be retained, never the full single-use token.
        serialized = json.dumps(proof)
        self.assertIn("claim_id_prefix", serialized)
        self.assertNotRegex(serialized, r"claim\.[0-9a-f]{40,}")

    def test_execution_on_a_different_tab_breaks_same_tab_proof(self):
        trace = self._trace()
        trace[-1]["claim_evidence"] = {"tab_id": "999"}
        self.assertFalse(MODULE.claim_proof(trace)["same_tab"])

    def test_claimless_tabs_open_is_detected(self):
        trace = self._trace()
        trace.append({"sequence": 7, "tool": "mcp__saccade__saccade_tabs_open",
                      "role": "navigate", "claim_evidence": {}, "claim_result": {}})
        self.assertTrue(MODULE.claim_proof(trace)["claimless_tabs_open_used"])

    def _lane(self, proof):
        return {"timing": {"clock_source": "wrapper_monotonic", "elapsed_ms": 10.0},
                "usage": {"input_tokens": 5, "output_tokens": 5},
                "browser_metrics": {"initial_transfer_bytes": 10,
                                    "action_return_to_delta_read_ms": 1.0,
                                    "dynamic_replacement_recoveries": 0},
                "tool_calls": 6, "route_proof": proof}

    def _engine_trace(self):
        return [
            {"sequence": 1, "tool": "mcp__saccade__saccade_tabs_open", "role": "navigate"},
            {"sequence": 2, "tool": "mcp__saccade__saccade_truth_read", "role": "observe"},
            {"sequence": 3, "tool": "mcp__saccade__saccade_act", "role": "execute"},
        ]

    def test_a_pure_engine_route_has_no_evidence_errors(self):
        proof = MODULE.route_proof(self._engine_trace())
        self.assertTrue(proof["pure_engine_route"])
        self.assertEqual(proof["act_calls"], 1)
        self.assertEqual(MODULE.lane_evidence_errors(self._lane(proof)), [])

    def test_reaching_for_a_client_browser_tool_invalidates_the_lane(self):
        trace = self._engine_trace()
        trace.append({"sequence": 4, "tool": "mcp__claude-in-chrome__computer", "role": "execute"})
        errors = MODULE.lane_evidence_errors(self._lane(MODULE.route_proof(trace)))
        self.assertIn("foreign_browser_tool_used", errors)

    def test_a_lane_that_never_executed_through_act_is_invalid(self):
        trace = [c for c in self._engine_trace() if "saccade_act" not in c["tool"]]
        errors = MODULE.lane_evidence_errors(self._lane(MODULE.route_proof(trace)))
        self.assertIn("saccade_act_execution_missing", errors)

    def test_playwright_lane_is_not_subject_to_claim_proof(self):
        self.assertEqual(MODULE.lane_evidence_errors(self._lane(None)), [])


class SuccessEvidenceContract(unittest.TestCase):
    """Success must come from tool output, never from the model's own final JSON."""

    def _stream(self, tool_body: str, final_json: str) -> str:
        return "\n".join([
            json.dumps({"type": "assistant", "message": {"content": [
                {"type": "tool_use", "id": "t1",
                 "name": "mcp__playwright__browser_find", "input": {}}]}}),
            json.dumps({"type": "user", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": tool_body}]}}),
            json.dumps({"type": "result", "subtype": "success", "is_error": False,
                        "result": final_json,
                        "usage": {"input_tokens": 10, "output_tokens": 5}}),
        ])

    def test_tool_output_is_captured_separately_from_the_final_message(self):
        parsed = MODULE.trace_events(
            self._stream("page shows Received!", '{"completed": true}'),
            "playwright", time.monotonic())
        self.assertIn("Received!", parsed["tool_output"])
        # The trace itself carries only metadata and must not be used as evidence.
        self.assertNotIn("Received!", MODULE.compact(parsed["trace"]))

    def test_model_claiming_success_without_tool_output_is_not_evidence(self):
        parsed = MODULE.trace_events(
            self._stream("page shows nothing useful",
                         '{"completed": true, "summary": "I saw Received!"}'),
            "playwright", time.monotonic())
        # The needle appears only in the model's own words, so it must not count.
        self.assertNotIn("received!", parsed["tool_output"].casefold())

    def test_needle_present_in_tool_output_counts(self):
        parsed = MODULE.trace_events(
            self._stream("heading Received!", '{"completed": true}'),
            "playwright", time.monotonic())
        self.assertIn("received!", parsed["tool_output"].casefold())


class FinalReplyParsing(unittest.TestCase):
    """The same model fences its JSON on some runs; that must not change a verdict."""

    def test_a_fenced_json_reply_is_read_the_same_as_a_bare_one(self):
        bare = '{"completed": true, "summary": "done"}'
        for text in (bare, f"```json\n{bare}\n```", f"```\n{bare}\n```"):
            self.assertEqual(json.loads(MODULE.unfenced(text))["completed"], True, text)

    def test_a_non_json_reply_still_fails_closed(self):
        with self.assertRaises(json.JSONDecodeError):
            json.loads(MODULE.unfenced("I could not finish the task."))

    def test_prose_before_a_fenced_final_object_is_not_a_lane_failure(self):
        text = ('The tool output proves success.\n\n```json\n'
                '{"completed": true, "summary": "done"}\n```')
        self.assertEqual(MODULE.final_json(text), {"completed": True, "summary": "done"})

    def test_prose_before_a_trailing_final_object_is_not_a_lane_failure(self):
        text = ('Success confirmed from tool output.\n\n'
                '{"completed": true, "summary": "done"}')
        self.assertEqual(MODULE.final_json(text), {"completed": True, "summary": "done"})

    def test_final_object_must_have_the_requested_typed_fields(self):
        self.assertIsNone(MODULE.final_json('```json\n{"completed": "yes"}\n```'))


class InfrastructureFailures(unittest.TestCase):
    """An API-side failure must never be scored as a browsing result."""

    def test_an_overload_reply_is_missing_evidence_not_a_lane_failure(self):
        lane = {"timing": {"clock_source": "wrapper_monotonic", "elapsed_ms": 200619.0},
                "usage": {}, "browser_metrics": {}, "tool_calls": 0,
                "infrastructure_failure": "api error: 529"}
        errors = MODULE.lane_evidence_errors(lane)
        self.assertEqual(errors, ["infrastructure_failure:api error: 529"])

    def test_a_clean_reply_reports_no_infrastructure_failure(self):
        self.assertIsNone(MODULE.infrastructure_failure('{"completed": true}'))
        self.assertEqual(MODULE.infrastructure_failure("API Error: 529 Overloaded"), "api error: 529")
        self.assertEqual(
            MODULE.infrastructure_failure("API Error: Rate limit reached"),
            "rate limit reached",
        )
        self.assertEqual(
            MODULE.infrastructure_failure("You've hit your limit · resets 12:20am"),
            "account_usage_limit",
        )
