import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("dev_probe", ROOT / "scripts/dev_probe.py")
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class DevProbeTests(unittest.TestCase):
    def test_frame_gate_is_truth_only(self) -> None:
        source = (ROOT / "scripts/dev_probe.py").read_text(encoding="utf-8")
        frame_gate = source[source.index("def frames_and_shadow"):source.index("\ndef reflex")]
        self.assertNotIn("act(", frame_gate)
        self.assertNotIn("receipts", frame_gate)
        self.assertIn('"execution_owner": "agent_client"', frame_gate)
        self.assertIn('reference=args.mode not in {"frames", "profile"}', source)

    def test_reference_stale_taxonomy_matches_host_and_mcp_errors(self) -> None:
        for detail in (
            "stale action basis",
            "request identity or revision is stale",
            "action token is not current for operation",
            "action token is not present in the current Profile-filtered observation",
            "action token is stale or absent from this Agent's current Truth Layer",
            "tab observation is not current",
        ):
            self.assertTrue(MODULE.is_stale_action_error(RuntimeError(detail)), detail)
        self.assertFalse(MODULE.is_stale_action_error(RuntimeError("permission_required")))


if __name__ == "__main__":
    unittest.main()
