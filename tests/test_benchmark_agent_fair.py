import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


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

    def test_lanes_use_the_same_agent_shell_and_only_one_browser_mcp(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            common = {
                "model": "same-model",
                "workdir": Path(directory),
                "runtime": Path("/runtime"),
                "runtime_dir": Path("/runtime-dir"),
                "playwright_package": "@playwright/mcp@test",
            }
            saccade = MODULE.lane_command("saccade", **common)
            playwright = MODULE.lane_command("playwright", **common)
        self.assertIn("mcp_servers.saccade.command=\"/runtime\"", saccade)
        self.assertFalse(any("playwright" in item for item in saccade))
        self.assertIn('mcp_servers.playwright.command="npx"', playwright)
        self.assertFalse(any("mcp_servers.saccade" in item for item in playwright))
        for command in (saccade, playwright):
            self.assertIn("--ignore-user-config", command)
            self.assertIn("shell_tool", command)
            self.assertIn("same-model", command)
            self.assertTrue(any('default_tools_approval_mode="approve"' in item for item in command))

    def test_lane_context_authorizes_only_its_browser(self) -> None:
        task = MODULE.load_task(ROOT / "benchmarks/tasks/selenium_web_form.json")
        self.assertIn("Saccade as the only browser route", MODULE.prompt_for(task, "saccade"))
        playwright = MODULE.prompt_for(task, "playwright")
        self.assertIn("explicitly authorizes Playwright", playwright)
        self.assertIn("Saccade is intentionally unavailable", playwright)

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


if __name__ == "__main__":
    unittest.main()
