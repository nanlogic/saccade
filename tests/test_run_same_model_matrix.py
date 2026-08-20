"""Matrix runner must isolate attempts and fail closed on non-PASS reports."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "run_same_model_matrix", ROOT / "scripts/run_same_model_matrix.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

SCRIPTS = ROOT / "scripts"
sys.path.insert(0, str(SCRIPTS))
UNKNOWN_SPEC = importlib.util.spec_from_file_location(
    "run_unknown_same_model_matrix", SCRIPTS / "run_unknown_same_model_matrix.py"
)
UNKNOWN_MODULE = importlib.util.module_from_spec(UNKNOWN_SPEC)
UNKNOWN_SPEC.loader.exec_module(UNKNOWN_MODULE)


class SameModelMatrixRunnerTests(unittest.TestCase):
    def test_unknown_matrix_reuses_exact_task_across_both_orders(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            args = SimpleNamespace(
                runtime=root / "runtime",
                runtime_dir=root / "state",
                fixture_root=root / "fixture-root",
                base_url="http://127.0.0.1:8765/fixtures/benchmarks",
                output=root / "output",
                browser="chrome",
                model="model",
                effort="low",
            )
            with (
                mock.patch.object(UNKNOWN_MODULE.argparse.ArgumentParser, "parse_args",
                                  return_value=args),
                mock.patch.object(UNKNOWN_MODULE, "assert_attached_browser"),
                mock.patch.object(UNKNOWN_MODULE, "prepare_output", return_value=None),
                mock.patch.object(UNKNOWN_MODULE, "summarize", return_value=0),
                mock.patch.object(UNKNOWN_MODULE.secrets, "token_hex",
                                  side_effect=("a" * 24, "b" * 24, "c" * 24)),
                mock.patch.object(UNKNOWN_MODULE.subprocess, "run") as run,
            ):
                self.assertEqual(UNKNOWN_MODULE.main(), 0)

            commands = [call.args[0] for call in run.call_args_list]
            self.assertEqual(len(commands), 6)
            for offset in range(0, 6, 2):
                first, second = commands[offset:offset + 2]
                self.assertEqual(first[first.index("--task") + 1],
                                 second[second.index("--task") + 1])
                self.assertNotEqual(first[first.index("--order") + 1],
                                    second[second.index("--order") + 1])

    def test_browser_preflight_reads_system_capabilities(self) -> None:
        response = {
            "id": 2,
            "result": {"structuredContent": {
                "extension_connected": True,
                "attached_browser": "chrome",
            }},
        }
        completed = SimpleNamespace(stdout=json.dumps(response) + "\n", stderr="", returncode=0)
        with mock.patch.object(MODULE.subprocess, "run", return_value=completed) as run:
            MODULE.assert_attached_browser(Path("/runtime"), Path("/state"), "chrome")
        sent = run.call_args.kwargs["input"]
        self.assertIn('"name": "saccade.system.capabilities"', sent)

    def test_browser_preflight_rejects_wrong_attached_browser(self) -> None:
        response = {
            "id": 2,
            "result": {"structuredContent": {
                "extension_connected": True,
                "attached_browser": "edge",
            }},
        }
        completed = SimpleNamespace(stdout=json.dumps(response) + "\n", stderr="", returncode=0)
        with mock.patch.object(MODULE.subprocess, "run", return_value=completed):
            with self.assertRaises(SystemExit):
                MODULE.assert_attached_browser(Path("/runtime"), Path("/state"), "chrome")

    def test_prior_attempt_is_archived_before_a_new_run(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "matrix"
            output.mkdir()
            (output / "stale.txt").write_text("old", encoding="utf-8")
            archived = MODULE.prepare_output(output)
            self.assertIsNotNone(archived)
            self.assertEqual((archived / "stale.txt").read_text(encoding="utf-8"), "old")
            self.assertEqual(list(output.iterdir()), [])

    def test_invalid_matrix_returns_nonzero(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory) / "task"
            run.mkdir()
            (run / "report.json").write_text(json.dumps({
                "verdict": "INVALID", "evidence_errors": {},
                "lanes": {
                    "saccade": {"passed": False, "elapsed_ms": 1, "tool_calls": 0,
                                "browser_metrics": {}, "usage": {}},
                    "playwright": {"passed": False, "elapsed_ms": 1, "tool_calls": 0,
                                   "browser_metrics": {}, "usage": {}},
                },
            }), encoding="utf-8")
            self.assertEqual(MODULE.summarize(Path(directory)), 1)


if __name__ == "__main__":
    unittest.main()
