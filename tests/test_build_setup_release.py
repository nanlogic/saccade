import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "build_setup_release", ROOT / "scripts/build_setup_release.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class BuildSetupReleaseTests(unittest.TestCase):
    def test_draft_has_real_checksum_but_cannot_claim_publication(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / "runtime"
            runtime.write_text(
                "#!/bin/sh\nprintf '%s\\n' '{\"mcp_contract_hash\":\""
                + ("a" * 64)
                + "\"}'\n"
            )
            runtime.chmod(0o755)
            result = MODULE.build(runtime, "darwin-arm64", root / "out")
            manifest = json.loads(Path(result["manifest"]).read_text())
            self.assertFalse(manifest["published"])
            self.assertFalse(manifest["artifacts"]["darwin-arm64"]["signed"])
            self.assertIsNone(manifest["artifacts"]["darwin-arm64"]["url"])
            self.assertEqual(manifest["artifacts"]["darwin-arm64"]["sha256"], result["sha256"])
            self.assertEqual(manifest["native_host"]["allowed_origins"], [])
            self.assertEqual(manifest["mcp_contract_hash"], "a" * 64)

    def test_draft_rejects_a_runtime_without_a_contract_identity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / "runtime"
            runtime.write_text("#!/bin/sh\nprintf '%s\\n' '{}'\n")
            runtime.chmod(0o755)
            with self.assertRaisesRegex(ValueError, "mcp_contract_hash"):
                MODULE.build(runtime, "darwin-arm64", root / "out")


if __name__ == "__main__":
    unittest.main()
