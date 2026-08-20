import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "benchmark_agent_fair", ROOT / "scripts/benchmark_agent_fair.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class FairBenchmarkTests(unittest.TestCase):
    def test_task_is_declarative_and_contains_no_selector_answer(self) -> None:
        task = MODULE.load_task(ROOT / "benchmarks/tasks/selenium_web_form.json")
        serialized = json.dumps(task)
        for forbidden in ("#my-", "[name=", "button[type", "xpath", "selector"):
            self.assertNotIn(forbidden, serialized.casefold())

    def test_multi_target_read_uses_one_structural_and_action_working_set(self) -> None:
        task = MODULE.load_task(ROOT / "benchmarks/tasks/heavy_public/mythcastera_homepage.json")
        prompt = MODULE.prompt_for(task, "saccade")
        self.assertIn("read-only goal naming multiple distinct labels", prompt)
        self.assertIn('roles:["heading","paragraph","list_item","link","button","status"]', prompt)
        self.assertIn("do not let one actionable target suppress", prompt)

    def test_playwright_lock_is_official_and_exact(self) -> None:
        lock = MODULE.load_playwright_lock()
        self.assertEqual(lock["package"], "@playwright/mcp")
        self.assertEqual(lock["version"], "0.0.79")
        self.assertTrue(lock["online_latest_verified"])

    def test_lanes_use_the_same_agent_shell_and_only_one_browser_mcp(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            common = {
                "model": "same-model",
                "effort": "low",
                "workdir": Path(directory),
                "runtime": Path("/runtime"),
                "runtime_dir": Path("/runtime-dir"),
                "playwright_package": "@playwright/mcp@test",
            }
            saccade = MODULE.lane_command("saccade", **common)
            playwright = MODULE.lane_command("playwright", **common)
        self.assertIn("mcp_servers.saccade.command=\"/runtime\"", saccade)
        self.assertTrue(
            any("SACCADE_BENCHMARK_FRESH_INPUT_POLICY" in item for item in saccade)
        )
        self.assertFalse(any("playwright" in item for item in saccade))
        self.assertIn('mcp_servers.playwright.command="npx"', playwright)
        self.assertFalse(any("mcp_servers.saccade" in item for item in playwright))
        for command in (saccade, playwright):
            self.assertIn("--ignore-user-config", command)
            self.assertIn("shell_tool", command)
            self.assertIn("same-model", command)
            self.assertTrue(any('default_tools_approval_mode="approve"' in item for item in command))

    def test_saccade_lane_never_configures_an_execution_mcp(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            saccade = MODULE.lane_command(
                "saccade", "same-model", "low", Path(directory), Path("/runtime"),
                Path("/runtime-dir"), "@playwright/mcp@test",
            )
        self.assertFalse(any("web_act" in item or "reference" in item for item in saccade))

    def test_lane_context_authorizes_only_its_browser(self) -> None:
        task = MODULE.load_task(ROOT / "benchmarks/tasks/selenium_web_form.json")
        saccade = MODULE.prompt_for(task, "saccade")
        self.assertIn("Saccade as the only browser route", saccade)
        self.assertIn("Execute only with saccade.act", saccade)
        self.assertIn("exactly one initial saccade.truth.read", saccade)
        self.assertIn("bounded nearby decision text", saccade)
        self.assertIn('roles:["button"]', saccade)
        self.assertIn('frame_scope:"root"', saccade)
        self.assertIn("text_any:[...]", saccade)
        self.assertIn("Do not issue another initial query", saccade)
        self.assertIn("fold any receipt transition immediately", saccade)
        self.assertIn("make one plain truth.read with after_revision", saccade)
        self.assertIn("do not query again", saccade)
        self.assertIn("omit operation from every", saccade)
        explicit = MODULE.prompt_for(task, "saccade", "explicit")
        self.assertIn("include operation in every", explicit)
        self.assertNotIn("omit operation from every", explicit)
        playwright = MODULE.prompt_for(task, "playwright")
        self.assertIn("explicitly authorizes Playwright", playwright)
        self.assertIn("Saccade is intentionally unavailable", playwright)

    def test_unknown_operation_mode_fails_closed(self) -> None:
        task = MODULE.load_task(ROOT / "benchmarks/tasks/selenium_web_form.json")
        with self.assertRaisesRegex(ValueError, "explicit or inferred"):
            MODULE.prompt_for(task, "saccade", "guess")

    def test_saved_transcript_redacts_editable_values(self) -> None:
        self.assertEqual(
            MODULE.redact_text('value SACCADE FAIR LINE ONE\\nLINE TWO Ω', ["SACCADE FAIR LINE ONE\nLINE TWO Ω"]),
            "value [REDACTED_EDITABLE]",
        )
        self.assertEqual(
            MODULE.redact_text(
                "?message=SACCADE+FAIR+LINE+ONE%0ALINE+TWO+%CE%A9",
                ["SACCADE FAIR LINE ONE\nLINE TWO Ω"],
            ),
            "?message=[REDACTED_EDITABLE]",
        )
        value = "SACCADE FAIR LINE ONE\nLINE TWO Ω"
        self.assertEqual(
            MODULE.redact_text("nested SACCADE FAIR LINE ONE\\\\nLINE TWO Ω", [value]),
            "nested [REDACTED_EDITABLE]",
        )
        self.assertEqual(
            MODULE.redact_text("rendered SACCADE FAIR LINE ONE LINE TWO Ω", [value]),
            "rendered [REDACTED_EDITABLE]",
        )

    def test_browser_metrics_report_trace_deltas_and_stale_recovery(self) -> None:
        metrics = MODULE.browser_metrics([
            {"type": "mcp_tool_call", "name": "saccade.truth.read", "result": {"mode": "catalog"}},
            {"type": "mcp_tool_call", "name": "saccade.truth.read", "result": {"mode": "details"}},
            {"type": "mcp_tool_call", "name": "saccade.truth.read", "result": {"mode": "delta", "dispatch_status": "stale_before_dispatch"}},
        ])
        self.assertEqual(metrics["full_views"], 0)
        self.assertEqual(metrics["working_set_views"], 0)
        self.assertEqual(metrics["catalog_views"], 1)
        self.assertEqual(metrics["detail_views"], 1)
        self.assertEqual(metrics["delta_views"], 1)
        self.assertEqual(metrics["stale_events"], 1)
        self.assertEqual(metrics["observe_or_snapshot_calls"], 3)
        self.assertEqual(metrics["post_initial_reobservation_calls"], 2)
        self.assertGreater(metrics["discovery"]["transfer_bytes"], 0)
        self.assertEqual(metrics["steady_state"]["delta_views"], 1)
        self.assertEqual(metrics["stability"]["stale"], 1)
        self.assertGreater(metrics["initial_transfer_bytes"], 0)
        self.assertIsNone(metrics["action_return_to_delta_read_ms"])
        self.assertEqual([row["sequence"] for row in metrics["trace"]], [1, 2, 3])

    def test_browser_metrics_count_actual_transition_views_not_capability_text(self) -> None:
        metrics = MODULE.browser_metrics([
            {
                "type": "mcp_tool_call",
                "name": "saccade.system.capabilities",
                "result": {"structured_content": {"help": '"transition" "mode":"delta"'}},
            },
            {
                "type": "mcp_tool_call",
                "name": "saccade.act",
                "result": {
                    "structured_content": {
                        "verified": True,
                        "transition": {"mode": "delta", "changes": []},
                    }
                },
            },
        ])
        self.assertEqual(metrics["steady_state"]["transition_views"], 1)
        self.assertEqual(metrics["delta_views"], 1)

    def test_timeout_can_be_reported_as_a_failed_lane(self) -> None:
        task = MODULE.load_task(ROOT / "benchmarks/tasks/selenium_web_form.json")
        summary = MODULE.lane_summary("saccade", 180000, 124, [], "timed out", task, True)
        self.assertFalse(summary["passed"])
        self.assertTrue(summary["timed_out"])
        self.assertEqual(summary["returncode"], 124)
        self.assertEqual(summary["infrastructure"]["failure"], "timeout")

    def test_echoed_success_text_cannot_override_failed_model_verdict(self) -> None:
        needle = "TARGET-WAS-NOT-FOUND"
        task = {"success": {"tool_output_contains": [needle]}}
        events = [
            {
                "type": "item.completed",
                "item": {
                    "type": "mcp_tool_call",
                    "name": "browser.find",
                    "arguments": {"text": needle},
                    "result": {"content": [{"type": "text", "text": f'No matches found for "{needle}".'}]},
                },
            },
            {
                "type": "item.completed",
                "item": {
                    "type": "agent_message",
                    "text": json.dumps({"completed": False, "summary": "Target was not found."}),
                },
            },
        ]
        summary = MODULE.lane_summary("playwright", 10, 0, events, "", task)
        self.assertFalse(summary["passed"])
        self.assertTrue(summary["model_report_consistent"])
        self.assertFalse(summary["success_evidence"][needle])

    def test_structured_query_echo_is_not_positive_evidence(self) -> None:
        needle = "Pizza"
        task = {"success": {"tool_output_contains": [needle]}}
        events = [
            {
                "type": "item.completed",
                "item": {
                    "type": "mcp_tool_call",
                    "name": "saccade.truth.read",
                    "result": {
                        "structured_content": {
                            "query": {"text_any": [needle]},
                            "objects": [{"role": "heading", "text": "Basic select"}],
                        }
                    },
                },
            },
            {
                "type": "item.completed",
                "item": {
                    "type": "agent_message",
                    "text": json.dumps({"completed": False, "summary": "Option absent."}),
                },
            },
        ]
        summary = MODULE.lane_summary("saccade", 10, 0, events, "", task)
        self.assertFalse(summary["success_evidence"][needle])
        self.assertFalse(summary["passed"])

    def test_model_usage_separates_cached_and_non_cached_input(self) -> None:
        self.assertEqual(
            MODULE.normalized_model_usage({
                "input_tokens": 1000, "cached_input_tokens": 700, "output_tokens": 80,
            }),
            {
                "input_tokens": 1000, "cached_input_tokens": 700,
                "non_cached_input_tokens": 300, "output_tokens": 80,
            },
        )

    def test_infrastructure_failure_never_scores_as_a_lane_loss(self) -> None:
        self.assertEqual(
            MODULE.infrastructure_failure(1, False, [], "API Error: 529 Overloaded"),
            "api_529_overloaded",
        )
        self.assertEqual(
            MODULE.infrastructure_failure(1, False, [], "Not logged in · Please run /login"),
            "agent_authentication",
        )

    def test_saccade_lane_requires_the_fresh_contract_hash_in_tool_output(self) -> None:
        task = MODULE.load_task(ROOT / "benchmarks/tasks/selenium_web_form.json")
        summary = MODULE.lane_summary(
            "saccade", 1, 0, [], "", task, False, "a" * 64,
        )
        self.assertFalse(summary["contract_hash_valid"])
        self.assertEqual(summary["failure_reason"], "stale_mcp_contract_or_registry")

    def test_positive_oracle_does_not_override_failed_agent_completion(self) -> None:
        marker = "QUEUE-PROOF-INDEPENDENT-ORACLE"
        task = {"success": {"tool_output_contains": [marker]}}
        events = [
            {"type": "item.completed", "item": {
                "type": "mcp_tool_call", "tool": "browser_click",
                "result": {"content": [{"type": "text", "text": marker}]},
            }},
            {"type": "item.completed", "item": {
                "type": "agent_message",
                "text": json.dumps({"completed": False, "summary": "misread proof"}),
            }},
        ]
        summary = MODULE.lane_summary("playwright", 1, 0, events, "", task)
        self.assertFalse(summary["passed"])
        self.assertFalse(summary["model_report_consistent"])

    def test_run_lane_executes_saccade_with_codex(self) -> None:
        task = MODULE.load_task(ROOT / "benchmarks/tasks/selenium_web_form.json")
        with tempfile.TemporaryDirectory() as directory, patch("scripts.benchmark_agent_fair.subprocess.run") as run:
            run.return_value = SimpleNamespace(stdout="", stderr="", returncode=1)
            result = MODULE.run_lane(
                "saccade", task, None, "low", Path("/runtime"), Path("/runtime-dir"),
                "@playwright/mcp@test", Path(directory),
            )
            self.assertEqual(result["lane"], "saccade")
            run.assert_called_once()
            self.assertIn('mcp_servers.saccade.command="/runtime"', run.call_args.args[0])
            self.assertEqual(
                run.call_args.kwargs["env"]["SACCADE_BENCHMARK_FRESH_INPUT_POLICY"],
                "1",
            )

    def test_client_native_evidence_requires_same_chrome_tab_and_order(self) -> None:
        task = MODULE.load_task(ROOT / "benchmarks/tasks/selenium_web_form.json")
        evidence = {
            "schema": "saccade-client-native-lane/1",
            "task": {"name": task["name"], "url": task["url"]},
            "order": "saccade-first",
            "client": "codex",
            "browser": {
                "family": "chrome", "same_saccade_instance": True, "same_tab": True,
                "browser_instance_id": "browser-1", "tab_id": "tab-1",
            },
            "truth": {"browser_instance_id": "browser-1", "tab_id": "tab-1"},
            "timing": {
                "started_at": "2026-08-03T10:00:00Z", "completed_at": "2026-08-03T10:01:00Z",
                "clock_source": "client_monotonic", "elapsed_ms": 60000,
            },
            "summary": {
                "lane": "saccade", "passed": True, "failure_reason": None,
                "usage": {"input_tokens": 100}, "tool_calls": 3,
                "browser_metrics": {
                    "initial_transfer_bytes": 1000,
                    "action_return_to_delta_read_ms": 20,
                    "dynamic_replacement_recoveries": 0,
                },
            },
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evidence.json"
            path.write_text(json.dumps(evidence))
            self.assertTrue(MODULE.load_client_native_evidence(path, task, "saccade-first")["passed"])
            evidence["browser"]["same_tab"] = False
            path.write_text(json.dumps(evidence))
            with self.assertRaisesRegex(ValueError, "same.*tab|tab boundary"):
                MODULE.load_client_native_evidence(path, task, "saccade-first")

    def test_incomplete_measurement_is_invalid_evidence(self) -> None:
        lane = {
            "lane": "saccade", "timing": {}, "usage": {}, "tool_calls": 0,
            "browser_metrics": {},
        }
        self.assertEqual(
            MODULE.lane_evidence_errors(lane),
            [
                "trusted_monotonic_clock_missing", "end_to_end_elapsed_ms_missing",
                "model_input_tokens_missing", "initial_transfer_bytes_missing",
                "tool_call_count_missing",
            ],
        )

    def test_lane_order_requires_non_overlapping_timestamped_execution(self) -> None:
        saccade = {"timing": {"started_at": "2026-08-03T10:00:00Z", "completed_at": "2026-08-03T10:01:00Z"}}
        playwright = {"timing": {"started_at": "2026-08-03T10:02:00Z", "completed_at": "2026-08-03T10:03:00Z"}}
        MODULE.validate_lane_order(saccade, playwright, "saccade-first")
        with self.assertRaisesRegex(ValueError, "playwright-first"):
            MODULE.validate_lane_order(saccade, playwright, "playwright-first")

    def test_public_matrix_resumes_only_pass_reports(self) -> None:
        source = (ROOT / "scripts/run_public_agent_fair_matrix.py").read_text(encoding="utf-8")
        self.assertIn('prior.get("verdict") == "PASS"', source)
        self.assertIn('TASK_ROOT.glob("*.json")', source)
        self.assertIn("for order in ORDERS", source)

if __name__ == "__main__":
    unittest.main()
