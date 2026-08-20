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

ASSEMBLE_SPEC = importlib.util.spec_from_file_location(
    "assemble_setup_release", ROOT / "scripts/assemble_setup_release.py"
)
ASSEMBLE = importlib.util.module_from_spec(ASSEMBLE_SPEC)
assert ASSEMBLE_SPEC.loader is not None
ASSEMBLE_SPEC.loader.exec_module(ASSEMBLE)

VERIFY_SPEC = importlib.util.spec_from_file_location(
    "verify_published_setup_release", ROOT / "scripts/verify_published_setup_release.py"
)
VERIFY = importlib.util.module_from_spec(VERIFY_SPEC)
assert VERIFY_SPEC.loader is not None
VERIFY_SPEC.loader.exec_module(VERIFY)


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

    def test_signed_architecture_drafts_assemble_into_nanlogic_release(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            drafts = []
            for platform in ("darwin-arm64", "darwin-x64"):
                runtime = root / f"runtime-{platform}"
                runtime.write_text(
                    "#!/bin/sh\nprintf '%s\\n' '{\"mcp_contract_hash\":\""
                    + ("a" * 64)
                    + "\"}'\n"
                )
                runtime.chmod(0o755)
                result = MODULE.build(
                    runtime,
                    platform,
                    root / platform,
                    signed=True,
                )
                drafts.append(Path(result["manifest"]))
            output = root / "release"
            result = ASSEMBLE.assemble(
                drafts,
                output,
                base_url="https://github.com/nanlogic/saccade/releases/download/v0.1.0",
                allowed_origins=["chrome-extension://abcdefghijklmnopabcdefghijklmnop/"],
            )
            manifest = json.loads(Path(result["manifest"]).read_text())
            self.assertTrue(manifest["published"])
            self.assertEqual(manifest["publisher"]["organization"], "Nanlogic")
            self.assertEqual(set(manifest["artifacts"]), {"darwin-arm64", "darwin-x64"})
            self.assertTrue(all(item["signed"] for item in manifest["artifacts"].values()))
            VERIFY.verify(Path(result["manifest"]), "v0.1.0", output)
            arm64 = output / "saccade-runtime-0.1.0-darwin-arm64"
            arm64.write_bytes(arm64.read_bytes() + b"changed")
            with self.assertRaisesRegex(ValueError, "checksum differs"):
                VERIFY.verify(Path(result["manifest"]), "v0.1.0", output)

    def test_assembly_rejects_unsigned_runtime(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / "runtime"
            runtime.write_text(
                "#!/bin/sh\nprintf '%s\\n' '{\"mcp_contract_hash\":\""
                + ("a" * 64)
                + "\"}'\n"
            )
            runtime.chmod(0o755)
            result = MODULE.build(runtime, "darwin-arm64", root / "draft")
            with self.assertRaisesRegex(ValueError, "not signed"):
                ASSEMBLE.assemble(
                    [Path(result["manifest"])],
                    root / "release",
                    base_url="https://github.com/nanlogic/saccade/releases/download/v0.1.0",
                    allowed_origins=["chrome-extension://abcdefghijklmnopabcdefghijklmnop/"],
                )


if __name__ == "__main__":
    unittest.main()
