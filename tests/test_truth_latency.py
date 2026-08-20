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

    def test_probe_retains_folded_delivery_batch_evidence(self) -> None:
        source = (ROOT / "scripts/probe_truth_latency.py").read_text()
        self.assertIn('"delivery_batches": delivery_batches', source)
        self.assertIn('"change_count": len(changes)', source)

    def test_sequential_markers_use_distinct_stable_objects(self) -> None:
        fixture = (ROOT / "fixtures/structural/truth_latency.html").read_text()
        self.assertIn("node.id = `single-${sequence}`", fixture)
        self.assertIn("appendChild(node)", fixture)
        self.assertNotIn("getElementById('single').textContent = stamp", fixture)

    def test_dialog_and_live_status_markers_are_public_truth_cases(self) -> None:
        fixture = (ROOT / "fixtures/structural/truth_latency.html").read_text()
        probe = (ROOT / "scripts/probe_truth_latency.py").read_text()
        self.assertIn("stamp('dialog', 'text')", fixture)
        self.assertIn('style="display: contents"', fixture)
        self.assertIn("stamp('live', 'status')", fixture)
        self.assertIn('"dialog:text", "live:status"', probe)

    def test_matrix_metrics_use_nearest_rank(self) -> None:
        self.assertEqual(MATRIX.metrics(list(range(1, 101)))["p95_ms"], 95)
        self.assertEqual(MATRIX.metrics([])["samples"], 0)


if __name__ == "__main__":
    unittest.main()
