import importlib.util
import json
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location(
    "external_dogfood", ROOT / "scripts/external_dogfood.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class ExternalDogfoodTests(unittest.TestCase):
    def test_manifest_is_declarative_and_has_no_execution_escape_hatch(self) -> None:
        cases = MODULE.load_cases(ROOT / "catalog/external_cases.json")
        self.assertGreaterEqual(len(cases), 9)
        serialized = json.dumps(cases).casefold()
        for forbidden in ("selector", "xpath", "locator", "coordinate", "javascript"):
            self.assertNotIn(forbidden, serialized)

    def test_error_taxonomy_is_stable(self) -> None:
        self.assertEqual(MODULE.classify_error(RuntimeError("observation has no button"), False), "not_observed")
        self.assertEqual(MODULE.classify_error(RuntimeError("unsupported operation"), True), "unsupported")
        self.assertEqual(MODULE.classify_error(RuntimeError("stale action basis"), True), "prepare_rejected")
        self.assertEqual(MODULE.classify_error(RuntimeError("native dispatch failed"), True), "dispatch_failed")

    def test_compact_view_does_not_persist_authority_or_geometry(self) -> None:
        view = {
            "document_id": "d",
            "revision": 3,
            "coverage": {},
            "limitations": [],
            "objects": [{
                "object_id": "o1", "role": "text_field", "name": "Email",
                "action_token": "secret", "bounds": {"x": 1}, "state": {"has_value": "false"},
            }],
        }
        serialized = json.dumps(MODULE.compact_view(view))
        self.assertNotIn("action_token", serialized)
        self.assertNotIn("bounds", serialized)


if __name__ == "__main__":
    unittest.main()
