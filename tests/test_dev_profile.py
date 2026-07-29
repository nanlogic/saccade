import json
import tempfile
import unittest
from pathlib import Path

from scripts.dev_profile import install, validate


class DevelopmentProfileTests(unittest.TestCase):
    def test_install_preserves_three_fields_and_unicode(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source.json"
            destination = root / "runtime" / "profile.json"
            expected = {"name": "聪明的野蛮人 eco", "behavior": "直接推进。", "ban": []}
            source.write_text(json.dumps(expected, ensure_ascii=False), encoding="utf-8")
            self.assertEqual(install(source, destination), expected)
            self.assertEqual(json.loads(destination.read_text(encoding="utf-8")), expected)
            self.assertEqual(destination.stat().st_mode & 0o777, 0o600)

    def test_unknown_fields_and_empty_conditions_fail(self) -> None:
        with self.assertRaises(ValueError):
            validate({"name": "bad", "behavior": "", "ban": [], "extra": True})
        with self.assertRaises(ValueError):
            validate({"name": "bad", "behavior": "", "ban": [{"control": "Save", "condition": ""}]})


if __name__ == "__main__":
    unittest.main()
