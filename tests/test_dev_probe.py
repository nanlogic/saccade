import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("dev_probe", ROOT / "scripts/dev_probe.py")
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class DevProbeTests(unittest.TestCase):
    def test_materializer_folds_recursive_updated_object_patches(self) -> None:
        client = MODULE.Mcp.__new__(MODULE.Mcp)
        client.agent_views = {}
        client.materialize_view({
            "schema": "saccade.agent-view/1",
            "mode": "full",
            "browser_instance_id": "browser-1",
            "tab_id": "tab-1",
            "document_id": "document-1",
            "revision": 1,
            "viewport_revision": 1,
            "object_defaults": {"visibility": "visible"},
            "objects": [{
                "object_id": "o1", "role": "checkbox", "visibility": "offscreen",
                "state": {"checked": "false", "enabled": "true"},
                "document_bounds": {"x": 10, "y": 20, "width": 30},
            }],
            "coverage": {}, "limitations": [], "gap": False,
        })
        folded = client.materialize_view({
            "schema": "saccade.agent-view/1",
            "mode": "delta",
            "browser_instance_id": "browser-1",
            "tab_id": "tab-1",
            "document_id": "document-1",
            "revision": 2,
            "viewport_revision": 2,
            "object_defaults": {"visibility": "visible"},
            "changes": [{
                "kind": "updated", "object_id": "o1",
                "patch": {
                    "visibility": None,
                    "state": {"checked": "true"},
                    "document_bounds": {"x": 15},
                },
            }],
            "coverage": {}, "limitations": [], "gap": False,
        })
        item = folded["objects"][0]
        self.assertEqual(item["visibility"], "visible")
        self.assertEqual(item["state"], {"checked": "true", "enabled": "true"})
        self.assertEqual(item["document_bounds"], {"x": 15, "y": 20, "width": 30})

    def test_materializer_appends_bounded_full_pages(self) -> None:
        client = MODULE.Mcp.__new__(MODULE.Mcp)
        client.agent_views = {}
        base = {
            "schema": "saccade.agent-view/1",
            "mode": "full",
            "tab_id": "tab-1",
            "document_id": "document-1",
            "revision": 7,
            "viewport_revision": 7,
            "object_defaults": {"protected": False},
            "coverage": {},
            "limitations": [],
            "gap": False,
        }
        first = client.materialize_view({
            **base,
            "page": {"index": 1, "count": 2, "complete": False},
            "objects": [{"object_id": "o1", "role": "button"}],
        })
        self.assertEqual([item["object_id"] for item in first["objects"]], ["o1"])
        complete = client.materialize_view({
            **base,
            "page": {"index": 2, "count": 2, "complete": True},
            "objects": [{"object_id": "o2", "role": "link"}],
        })
        self.assertEqual(
            [item["object_id"] for item in complete["objects"]],
            ["o1", "o2"],
        )
        self.assertFalse(complete["objects"][1]["protected"])

    def test_diagnostic_catalog_expands_frozen_pages_and_details(self) -> None:
        client = MODULE.Mcp.__new__(MODULE.Mcp)
        client.agent_views = {}
        replies = iter([
            {
                "schema": "saccade.agent-view/1", "mode": "catalog",
                "tab_id": "tab-1", "document_id": "document-1", "revision": 9,
                "entries": [{"object_id": "o2", "role": "link"}],
                "page": {"complete": True},
            },
            {
                "schema": "saccade.agent-view/1", "mode": "details",
                "tab_id": "tab-1", "document_id": "document-1", "revision": 9,
                "object_defaults": {"protected": False},
                "objects": [
                    {"object_id": "o1", "role": "button"},
                    {"object_id": "o2", "role": "link"},
                ],
            },
        ])
        calls = []

        def raw_tool(name, arguments, timeout=35.0):  # noqa: ANN001, ARG001
            calls.append((name, arguments))
            return next(replies)

        client.raw_tool = raw_tool
        expanded = client.materialize_catalog({
            "schema": "saccade.agent-view/1", "mode": "catalog",
            "browser_instance_id": "browser-1", "tab_id": "tab-1",
            "document_id": "document-1", "revision": 9, "viewport_revision": 3,
            "entries": [{"object_id": "o1", "role": "button"}],
            "page": {"complete": False}, "coverage": {}, "limitations": [],
        })
        self.assertEqual([item["object_id"] for item in expanded["objects"]], ["o1", "o2"])
        self.assertTrue(all(item["protected"] is False for item in expanded["objects"]))
        self.assertEqual(calls[0], ("truth.read", {"tab_id": "tab-1"}))
        self.assertEqual(calls[1][1]["object_ids"], ["o1", "o2"])
        self.assertTrue(expanded["catalog_expanded_for_diagnostics"])

    def test_mouseaccuracy_waits_for_client_rendered_settings(self) -> None:
        shell = {"tab_id": "tab-1", "objects": [{"role": "heading", "name": None}]}
        ready = {
            "tab_id": "tab-1",
            "objects": [
                {"role": "button", "name": "Decrease Normal"},
                {"role": "button", "name": "Increase Normal"},
                {"role": "button", "name": "Decrease Medium"},
                {"role": "button", "name": "Increase Medium"},
            ],
        }

        class FakeMcp:
            def tool(self, name, arguments):  # noqa: ANN001
                self.call = (name, arguments)
                return ready

        client = FakeMcp()
        self.assertIs(MODULE.wait_mouseaccuracy_settings(client, shell, timeout=1.0), ready)
        self.assertEqual(client.call, ("web.observe", {"tab_id": "tab-1"}))

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
            "prepared action failed identity, focus, geometry, visibility, or topmost revalidation",
        ):
            self.assertTrue(MODULE.is_stale_action_error(RuntimeError(detail)), detail)
        self.assertFalse(MODULE.is_stale_action_error(RuntimeError("permission_required")))


if __name__ == "__main__":
    unittest.main()
