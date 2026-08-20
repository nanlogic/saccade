import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "package_extension_release", ROOT / "scripts/package_extension_release.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class PackageExtensionReleaseTests(unittest.TestCase):
    def test_development_manifest_cannot_be_packaged_for_store(self) -> None:
        with self.assertRaisesRegex(ValueError, "development name"):
            MODULE.package(ROOT / "extension", Path(tempfile.mkdtemp()))

    def test_exact_production_candidate_is_packaged(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            extension = Path(directory) / "extension"
            output = Path(directory) / "out"
            (extension / "src").mkdir(parents=True)
            (extension / "manifest.json").write_text(
                json.dumps({"manifest_version": 3, "name": "Saccade", "version": "1.2.3"})
            )
            (extension / "src/service_worker.js").write_text("// worker\n")
            candidate = {
                "schema": "saccade.extension-candidate/1",
                "id": MODULE.candidate_id(extension),
                "version": "1.2.3",
            }
            (extension / "candidate.json").write_text(json.dumps(candidate))
            (extension / "src/candidate_identity.js").write_text("// generated\n")
            archive = MODULE.package(extension, output)
            self.assertTrue(archive.is_file())
            self.assertEqual(archive.name, "saccade-extension-1.2.3.zip")


if __name__ == "__main__":
    unittest.main()
