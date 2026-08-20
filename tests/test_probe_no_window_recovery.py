import unittest
from pathlib import Path


class NoWindowRecoveryProbeTests(unittest.TestCase):
    def test_probe_runs_exactly_two_cycles_through_saccade(self) -> None:
        body = (Path(__file__).parents[1] / "scripts/probe_no_window_recovery.py").read_text()
        self.assertIn("for cycle in range(1, 3)", body)
        loop = body[body.index("for cycle in range(1, 3)") :]
        self.assertLess(loop.index("close_test_windows"), loop.index("open_when_browser_ready"))
        self.assertIn('"started_without_normal_window": True', body)
        self.assertIn("open_when_browser_ready(mcp", body)
        self.assertIn('mcp.tool("tabs.close"', body)
        self.assertIn('mcp.tool("system.capabilities"', body)
        self.assertNotIn("playwright", body.casefold())
        self.assertNotIn("cdp", body.casefold())


if __name__ == "__main__":
    unittest.main()
