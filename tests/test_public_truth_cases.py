import importlib.util
import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "generate_public_truth_cases", ROOT / "scripts/generate_public_truth_cases.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class PublicTruthCasesTests(unittest.TestCase):
    def test_denominator_covers_inventory_and_lifecycle_without_hidden_skips(self) -> None:
        document = MODULE.render()
        MODULE.validate(document)
        self.assertEqual(document["summary"], {
            "roles": 34,
            "variants": 12,
            "boundaries": 6,
            "lifecycle_scenarios": 11,
            "total": 63,
        })
        self.assertTrue(all(row["outcome"] in document["outcomes"] for row in document["items"]))
        self.assertTrue(all(row["reason"] for row in document["items"]))

    def test_standards_denominator_and_historical_actuator_evidence_are_explicit(self) -> None:
        document = MODULE.render()
        source = next(row for row in document["source_documents"] if row["id"] == "standards_mainstream_uncommon_controls")
        self.assertEqual(source["status"], "merged")
        self.assertEqual(next(row for row in document["items"] if row["id"] == "role:button")["classification"], "mainstream_control")
        self.assertEqual(next(row for row in document["items"] if row["id"] == "variant:drop_target")["classification"], "uncommon_control")
        text_field = next(row for row in document["items"] if row["id"] == "role:text_field")
        self.assertEqual(text_field["outcome"], "blocked")
        self.assertIn("historical_reference_only", {row["status"] for row in text_field["sources"]})

    def test_checked_in_manifest_matches_generator(self) -> None:
        checked_in = json.loads((ROOT / "catalog/public_truth_cases.json").read_text(encoding="utf-8"))
        self.assertEqual(checked_in, MODULE.render())

    def test_checked_in_manifest_matches_schema(self) -> None:
        manifest = json.loads((ROOT / "catalog/public_truth_cases.json").read_text(encoding="utf-8"))
        schema = json.loads((ROOT / "catalog/public_truth_cases.schema.json").read_text(encoding="utf-8"))
        item_properties = schema["properties"]["items"]["items"]["properties"]
        classifications = set(item_properties["classification"]["enum"])
        source_statuses = set(item_properties["sources"]["items"]["properties"]["status"]["enum"])
        self.assertTrue({row["classification"] for row in manifest["items"]} <= classifications)
        self.assertTrue({
            source["status"] for row in manifest["items"] for source in row["sources"]
        } <= source_statuses)


if __name__ == "__main__":
    unittest.main()
