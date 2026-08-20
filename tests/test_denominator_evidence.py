import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "summarize_denominator_evidence", ROOT / "scripts/summarize_denominator_evidence.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class DenominatorEvidenceTests(unittest.TestCase):
    def test_every_row_has_a_local_evidence_route(self) -> None:
        denominator = MODULE.load(ROOT / "catalog/public_truth_cases.json")
        controls = {row["role"] for row in MODULE.load(ROOT / "catalog/truth_inventory.json")["roles"] if row["gate"] == "control"}
        self.assertEqual(controls, MODULE.CONTROL_ROLES)
        self.assertEqual(len(denominator["items"]), 63)
        self.assertEqual(len(MODULE.LIMITED), 7)

    def test_lifecycle_mapping_is_complete(self) -> None:
        denominator = MODULE.load(ROOT / "catalog/public_truth_cases.json")
        targets = {row["target"] for row in denominator["items"] if row["kind"] == "lifecycle"}
        source = (ROOT / "scripts/summarize_denominator_evidence.py").read_text(encoding="utf-8")
        for target in targets:
            self.assertIn(f'"{target}"', source)

    def test_local_pass_does_not_promote_publication(self) -> None:
        source = (ROOT / "scripts/summarize_denominator_evidence.py").read_text(encoding="utf-8")
        self.assertIn('"publication_outcome": item["outcome"]', source)
        self.assertIn("still requires declared public/client-owned evidence", source)


if __name__ == "__main__":
    unittest.main()
