import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "summarize_fair_matrix", ROOT / "scripts/summarize_fair_matrix.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class FairMatrixTests(unittest.TestCase):
    def test_incomplete_matrix_cannot_authorize_public_claims(self) -> None:
        result = MODULE.summarize([])
        self.assertEqual(result["status"], "BLOCKED")
        self.assertFalse(result["public_comparison_claims_authorized"])
        self.assertEqual(sum(item.startswith("missing_cell:") for item in result["errors"]), 6)


if __name__ == "__main__":
    unittest.main()
