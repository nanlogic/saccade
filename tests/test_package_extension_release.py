import hashlib
import importlib.util
import json
import os
import shutil
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
        with tempfile.TemporaryDirectory() as directory:
            extension = Path(directory) / "extension"
            shutil.copytree(ROOT / "extension", extension)
            manifest_path = extension / "manifest.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["name"] = "Saccade Extension (Development)"
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "development name"):
                MODULE.package(extension, Path(directory) / "out")

    def test_checked_in_production_candidate_can_be_packaged(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            archive = MODULE.package(ROOT / "extension", Path(directory))
            self.assertTrue(archive.is_file())
            self.assertEqual(archive.name, "saccade-extension-0.3.24.zip")
            with MODULE.zipfile.ZipFile(archive) as packaged:
                self.assertFalse(any(name.startswith("tests/") for name in packaged.namelist()))

    def test_package_is_reproducible_across_source_timestamps(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            extension = root / "extension"
            shutil.copytree(ROOT / "extension", extension)
            first = MODULE.package(extension, root / "first").read_bytes()
            for path in extension.rglob("*"):
                if path.is_file():
                    os.utime(path, (1_800_000_000, 1_800_000_000))
            second = MODULE.package(extension, root / "second").read_bytes()
            self.assertEqual(hashlib.sha256(first).digest(), hashlib.sha256(second).digest())
            self.assertEqual(first, second)

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
