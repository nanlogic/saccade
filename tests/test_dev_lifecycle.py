import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


class DevelopmentLifecycleTests(unittest.TestCase):
    def test_attach_installs_candidate_extension_for_ordinary_chrome(self) -> None:
        script = (Path(__file__).parents[1] / "scripts" / "dev.sh").read_text(
            encoding="utf-8"
        )
        body = script.split("attach_existing_chrome() {", 1)[1].split("\n}", 1)[0]
        self.assertIn("install_runtime", body)
        self.assertIn("install_native_manifest", body)
        self.assertIn("install_extension", body)
        self.assertIn("start_fixture", body)
        self.assertIn("refresh_attached_native_hosts", body)
        self.assertIn("verify_attached_extension_candidate", body)
        self.assertIn("saccade.tabs.open", body)
        self.assertIn("Agent On automatically", body)
        self.assertIn("Agent-Off tabs remain private", body)
        self.assertNotIn("click the Saccade toolbar icon", body)

    def test_installed_extension_candidate_is_content_addressed(self) -> None:
        root = Path(__file__).parents[1]
        with tempfile.TemporaryDirectory() as temporary:
            temporary = Path(temporary)
            extension = temporary / "extension"
            expected = temporary / "expected.json"
            shutil.copytree(root / "extension", extension)
            subprocess.run(
                [
                    "python3",
                    str(root / "scripts" / "write_extension_candidate.py"),
                    "--extension-root",
                    str(extension),
                    "--expected",
                    str(expected),
                ],
                check=True,
                capture_output=True,
                text=True,
            )
            candidate = json.loads(expected.read_text(encoding="utf-8"))
            checked_in = json.loads(
                (root / "extension" / "candidate.json").read_text(encoding="utf-8")
            )
            self.assertEqual(candidate["schema"], "saccade.extension-candidate/1")
            manifest = json.loads((extension / "manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(candidate["version"], manifest["version"])
            self.assertEqual(len(candidate["id"]), 64)
            self.assertEqual(candidate, checked_in)
            self.assertEqual(
                candidate,
                json.loads((extension / "candidate.json").read_text(encoding="utf-8")),
            )
            identity = (extension / "src" / "candidate_identity.js").read_text(
                encoding="utf-8"
            )
            self.assertIn(candidate["id"], identity)

    def test_dev_install_derives_a_separate_development_candidate(self) -> None:
        script = (Path(__file__).parents[1] / "scripts" / "dev.sh").read_text(
            encoding="utf-8"
        )
        body = script.split("install_extension() {", 1)[1].split("\n}", 1)[0]
        self.assertIn('manifest.get("name") != "Saccade"', body)
        self.assertIn('manifest["name"] = "Saccade Extension (Development)"', body)
        self.assertIn(
            'if cmp -s "$source_expected" "$RUNTIME_DIR/expected-extension-candidate.json"',
            body,
        )
        self.assertNotIn("source and installed Extension candidates diverged", body)

    def test_attach_refreshes_only_saccade_native_host_processes(self) -> None:
        script = (Path(__file__).parents[1] / "scripts" / "dev.sh").read_text(
            encoding="utf-8"
        )
        body = script.split("refresh_attached_native_hosts() {", 1)[1].split(
            "\n}", 1
        )[0]
        self.assertIn('pgrep -f "$RUNTIME chrome-extension://"', body)
        self.assertIn('ps -p "$attached_host_pid" -o command=', body)
        self.assertIn('"$RUNTIME chrome-extension://"*', body)
        self.assertNotIn("pkill", body)

    def test_runtime_install_repairs_a_stale_or_replaced_binary(self) -> None:
        script = (Path(__file__).parents[1] / "scripts" / "dev.sh").read_text(
            encoding="utf-8"
        )
        body = script.split("install_runtime() {", 1)[1].split("\n}", 1)[0]
        self.assertIn('"$STATE_DIR/runtime-installed.sha256"', body)
        self.assertIn('actual_runtime_hash=$(shasum -a 256 "$RUNTIME"', body)
        self.assertIn('[ "$actual_runtime_hash" != "$recorded_runtime_hash" ]', body)
        self.assertIn(
            'shasum -a 256 "$RUNTIME" | awk \'{print $1}\' > "$STATE_DIR/runtime-installed.sha256"',
            body,
        )

    def test_managed_release_gates_suspend_only_ordinary_chrome_host(self) -> None:
        script = (Path(__file__).parents[1] / "scripts" / "dev.sh").read_text(
            encoding="utf-8"
        )
        suspend = script.split("suspend_ordinary_chrome_native_host() {", 1)[1].split(
            "\n}", 1
        )[0]
        restore = script.split("restore_ordinary_chrome_native_host() {", 1)[1].split(
            "\n}", 1
        )[0]
        self.assertIn('"$HOST_DIR_CHROME/com.nanlogic.saccade.dev.json"', suspend)
        self.assertIn("refresh_attached_native_hosts", suspend)
        self.assertNotIn("HOST_DIR_EDGE", suspend)
        self.assertIn('mv "$suspended_manifest" "$chrome_manifest"', restore)
        self.assertIn("restore_ordinary_chrome_native_host", script)
        down = script.split("down() {", 1)[1].split("\n}", 1)[0]
        self.assertIn("restore_ordinary_chrome_native_host", down)
        self.assertIn(
            'SACCADE_SUSPEND_ORDINARY_CHROME_HOST=1 up "$lifecycle_browser"',
            script,
        )
        self.assertIn(
            'SACCADE_SUSPEND_ORDINARY_CHROME_HOST=1 up "$truth_browser"',
            script,
        )
        self.assertIn(
            'SACCADE_SUSPEND_ORDINARY_CHROME_HOST=1 up "$public_truth_browser"',
            script,
        )

    def test_managed_profiles_enable_unpacked_extension_developer_mode(self) -> None:
        script = (Path(__file__).parents[1] / "scripts" / "dev.sh").read_text(
            encoding="utf-8"
        )
        clean = script.split("mark_browser_profile_clean() {", 1)[1].split("\n}", 1)[0]
        self.assertIn('["developer_mode"] = True', clean)
        self.assertIn('safebrowsing["enabled"] = True', clean)
        self.assertIn('safebrowsing["enhanced"] = False', clean)
        self.assertIn('--load-extension="$EXTENSION_ROOT"', script)
        self.assertIn('"$start_browser_app" "$EXTENSION_BOOTSTRAP_URL" --args', script)

    def test_public_truth_diagnostic_is_not_the_agent_closed_loop(self) -> None:
        script = (Path(__file__).parents[1] / "scripts" / "dev.sh").read_text(
            encoding="utf-8"
        )
        readme = (Path(__file__).parents[1] / "README.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("write_candidate_manifest.py", script)
        self.assertIn("public_truth_route()", script)
        self.assertIn("probe_public_truth.py", script)
        self.assertIn("observation regression diagnostic, not Codex dogfood", readme)
        self.assertIn("Agent\nclient's own same-tab browser tool", readme)

    def test_denominator_command_binds_truth_and_lifecycle_to_one_stamp(self) -> None:
        script = (Path(__file__).parents[1] / "scripts" / "dev.sh").read_text(
            encoding="utf-8"
        )
        body = script.split("denominator_all() {", 1)[1].split("\n}", 1)[0]
        self.assertIn('truth_test_all "$denominator_stamp"', body)
        self.assertIn('lifecycle_all "$denominator_stamp"', body)
        self.assertIn("summarize_denominator_evidence.py", body)
        self.assertIn("denominator-63.json", body)

    def test_wait_for_mcp_requires_the_live_extension_not_only_initialize(self) -> None:
        probe = (Path(__file__).parents[1] / "scripts" / "dev_probe.py").read_text(
            encoding="utf-8"
        )
        body = probe.split("def wait_for_mcp(", 1)[1].split("\ndef wait_observation", 1)[0]
        self.assertIn('client.tool("system.capabilities", {})', body)
        self.assertIn('capabilities.get("extension_connected") is not True', body)
        self.assertIn("client.close()", body)


if __name__ == "__main__":
    unittest.main()
