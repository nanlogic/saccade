import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "audit_public_evidence", ROOT / "scripts/audit_public_evidence.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class PublicEvidenceAuditTests(unittest.TestCase):
    def test_empty_evidence_keeps_all_63_rows_blocked(self) -> None:
        result = MODULE.audit([])
        self.assertEqual(result["summary"], {"total": 63, "promoted": 0, "blocked": 63})

    def test_reference_actuator_report_cannot_promote(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "actuator.json"
            path.write_text(json.dumps({"schema": "saccade.public-truth-evidence/1"}))
            result = MODULE.audit([path])
            self.assertEqual(result["summary"]["promoted"], 0)
            self.assertEqual(result["rejected_evidence"][0]["reason"], "not_client_owned_public_evidence")


if __name__ == "__main__":
    unittest.main()
