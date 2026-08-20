import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "generate_long_horizon_benchmark", ROOT / "scripts/generate_long_horizon_benchmark.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class LongHorizonGeneratorTests(unittest.TestCase):
    def test_same_seed_is_deterministic_and_oracle_has_both_decisions(self) -> None:
        records = MODULE.records_for("deterministic-seed", 50)
        self.assertEqual(records, MODULE.records_for("deterministic-seed", 50))
        self.assertEqual(len(records), 50)
        self.assertEqual({record["expected"] for record in records}, {"approve", "reject"})
        for record in records:
            expected = "approve" if record["risk"] <= 45 and record["evidence"] else "reject"
            self.assertEqual(record["expected"], expected)

    def test_all_modes_create_unknown_declarative_tasks_without_selectors(self) -> None:
        for mode in MODULE.MODES:
            pages, task = MODULE.build(
                "fresh-seed", 5, mode,
                f"http://127.0.0.1:8765/fixtures/benchmarks/long/{mode}/index.html",
            )
            self.assertEqual(task["generation"]["mode"], mode)
            self.assertEqual(task["generation"]["length"], 5)
            self.assertNotIn("selector", task["task"].casefold())
            self.assertNotIn("#card", task["task"])
            expected_pages = 5 if mode == "navigation" else 1
            self.assertEqual(len(pages), expected_pages)
            combined = "".join(pages.values())
            self.assertIn("aria-pressed", combined)
            self.assertIn(task["success"]["tool_output_contains"][0], combined)

    def test_lengths_are_the_fixed_break_even_curve(self) -> None:
        self.assertEqual(MODULE.LENGTHS, (1, 5, 10, 25, 50))

    def test_interrupted_matrix_resumes_only_frozen_pass_evidence(self) -> None:
        source = (ROOT / "scripts/run_long_horizon_matrix.py").read_text(encoding="utf-8")
        self.assertIn('parser.add_argument("--resume"', source)
        self.assertIn('prior.get("verdict") == "PASS"', source)
        self.assertIn("resume found multiple generated tasks", source)
        self.assertNotIn('prior.get("verdict") in', source)
        self.assertIn('preflight_fixture(task["url"])', source)
        self.assertIn("response.status == 200", source)


if __name__ == "__main__":
    unittest.main()
