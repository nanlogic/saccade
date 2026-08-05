import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location("probe_truth_latency", ROOT / "scripts/probe_truth_latency.py")
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)
MATRIX_SPEC = importlib.util.spec_from_file_location(
    "summarize_truth_latency_matrix", ROOT / "scripts/summarize_truth_latency_matrix.py"
)
MATRIX = importlib.util.module_from_spec(MATRIX_SPEC)
assert MATRIX_SPEC.loader is not None
MATRIX_SPEC.loader.exec_module(MATRIX)


class TruthLatencyTests(unittest.TestCase):
    def test_nearest_rank_percentiles_and_empty_metrics(self) -> None:
        self.assertEqual(MODULE.percentile(list(range(1, 101)), 0.95), 95)
        self.assertEqual(MODULE.latency_metrics([])["samples"], 0)
        self.assertIsNone(MODULE.latency_metrics([])["p95_ms"])

    def test_marker_requires_scenario_item_and_epoch(self) -> None:
        self.assertIsNotNone(MODULE.MARKER.match("LT|single|1|1234.5"))
        self.assertIsNone(MODULE.MARKER.match("LT|single|1"))

    def test_matrix_metrics_use_nearest_rank(self) -> None:
        self.assertEqual(MATRIX.metrics(list(range(1, 101)))["p95_ms"], 95)
        self.assertEqual(MATRIX.metrics([])["samples"], 0)


if __name__ == "__main__":
    unittest.main()
