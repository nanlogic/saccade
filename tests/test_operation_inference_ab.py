import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location(
    "run_operation_inference_ab", ROOT / "scripts/run_operation_inference_ab.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def event(arguments):
    return {
        "type": "item.completed",
        "item": {
            "type": "mcp_tool_call", "tool": "saccade.act", "arguments": arguments,
        },
    }


class OperationInferenceABTests(unittest.TestCase):
    def test_explicit_lane_requires_operation_on_every_action(self):
        good = [event({"actions": [
            {"object_id": "o1", "operation": "type", "value": "x"},
            {"object_id": "o2", "operation": "click"},
        ]})]
        self.assertTrue(MODULE.operation_compliance(good, "explicit")["compliant"])
        bad = [event({"actions": [{"object_id": "o1", "operation": "click"}, {"object_id": "o2"}]})]
        self.assertFalse(MODULE.operation_compliance(bad, "explicit")["compliant"])

    def test_inferred_lane_rejects_any_operation_field(self):
        good = [event({"actions": [
            {"object_id": "o1", "value": "x"},
            {"object_id": "o2"},
        ]})]
        result = MODULE.operation_compliance(good, "inferred")
        self.assertTrue(result["compliant"])
        self.assertEqual(result["operation_fields"], 0)
        self.assertFalse(MODULE.operation_compliance(
            [event({"object_id": "o1", "operation": "click"})], "inferred"
        )["compliant"])

    def test_no_act_call_is_not_compliant(self):
        self.assertFalse(MODULE.operation_compliance([], "inferred")["compliant"])

    def test_any_failed_tool_call_invalidates_the_run(self):
        events = [event({"object_id": "o1"})]
        events.append({
            "type": "item.completed",
            "item": {"type": "mcp_tool_call", "tool": "saccade.truth.read", "status": "failed"},
        })
        compliance = MODULE.operation_compliance(events, "inferred")
        self.assertEqual(compliance["failed_tool_calls"], 1)
        summary = {
            "passed": True, "elapsed_ms": 1, "usage": {"input_tokens": 1, "output_tokens": 1},
            "browser_metrics": {"initial_transfer_bytes": 1},
        }
        self.assertIn("tool_call_failed", MODULE.evidence_errors(summary, compliance))

    def test_aggregate_uses_only_valid_pairs(self):
        metrics = {
            "elapsed_ms": 10, "input_tokens": 20, "output_tokens": 5,
            "tool_calls": 3, "initial_transfer_bytes": 100, "transcript_bytes": 200,
            "post_initial_reobservation_calls": 0, "stale_events": 0,
        }
        pairs = [{"valid": True, "runs": {
            "explicit": {"metrics": metrics},
            "inferred": {"metrics": {**metrics, "output_tokens": 4}},
        }}]
        result = MODULE.aggregate(pairs)
        self.assertEqual(result["valid_pairs"], 1)
        self.assertEqual(result["deltas"]["output_tokens"]["percent_inferred_minus_explicit"], -20.0)


if __name__ == "__main__":
    unittest.main()
