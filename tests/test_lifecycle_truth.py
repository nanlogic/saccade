import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location("probe_lifecycle_truth", ROOT / "scripts/probe_lifecycle_truth.py")
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class LifecycleTruthTests(unittest.TestCase):
    def test_fixture_declares_every_probe_marker(self) -> None:
        fixture = (ROOT / "fixtures/structural/lifecycle_gauntlet.html").read_text(encoding="utf-8")
        for marker in MODULE.EXPECTED_MARKERS:
            self.assertIn(marker.removeprefix("LC|"), fixture)

    def test_probe_keeps_execution_outside_runtime(self) -> None:
        source = (ROOT / "scripts/probe_lifecycle_truth.py").read_text(encoding="utf-8")
        self.assertIn('"execution_owner": "agent_client"', source)
        self.assertIn('"stimulus": "page_driven_fixture"', source)
        self.assertNotIn("reference-actuator", source)
        self.assertNotIn("saccade.reference", source)

    def test_probe_closes_only_its_agent_owned_tab_and_retires_truth(self) -> None:
        source = (ROOT / "scripts/probe_lifecycle_truth.py").read_text(encoding="utf-8")
        self.assertIn('raw_tool(mcp, "tabs.list", {})', source)
        self.assertIn('listed_open_tab.get("ownership")', source)
        self.assertIn('raw_tool(mcp, "tabs.close", {"tab_id": tab_id})', source)
        self.assertIn('rejected_tool(mcp, "tabs.close", {"tab_id": tab_id})', source)
        self.assertIn('rejected_tool(mcp, "truth.read", {"tab_id": tab_id})', source)
        self.assertIn('"tab_lifecycle": lifecycle_cleanup', source)

    def test_slow_resource_is_a_real_delayed_http_response(self) -> None:
        server = (ROOT / "scripts" / "fixture_server.py").read_text(encoding="utf-8")
        self.assertIn("slow_resource_payload.html", server)
        self.assertIn("time.sleep(1.5)", server)

    def test_probe_retries_only_the_explicit_browser_window_startup_race(self) -> None:
        source = (ROOT / "scripts/dev_probe.py").read_text(encoding="utf-8")
        self.assertIn('"operation timed out"', source)
        self.assertIn("timeout: float = 20.0", source)
        self.assertNotIn("except RuntimeError:\n", source)


if __name__ == "__main__":
    unittest.main()
