#!/usr/bin/env python3

from __future__ import annotations

import argparse
import importlib.util
import json
import pathlib
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = pathlib.Path(__file__).with_name("build_reflex_evidence_pack.py")
SPEC = importlib.util.spec_from_file_location("build_reflex_evidence_pack", SCRIPT)
assert SPEC and SPEC.loader
PACKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKER)


def pass_report() -> dict:
    return {
        "schema": "saccade-reflex-run-test-v1",
        "verdict": "PASS",
        "completed": True,
        "completion_policy": "mouseaccuracy_results_truth_v1",
        "verified_target_receipts": 3,
        "final_hits": 3,
        "final_misses": 0,
        "duration_ms": 1000,
        "benchmark_truth": {
            "source": "same_webview_article_text_v1",
            "source_url": "https://mouseaccuracy.com/results?private=1#result",
            "target_efficiency_pct": 100,
            "targets_hit": 3,
            "targets_total": 3,
            "click_accuracy_pct": 100,
            "clicks_hit": 3,
            "clicks_total": 3,
            "total_score": 300,
            "verified_receipt_count_matches_hits": True,
        },
        "latency_ms": {"p50": 4.2, "p95": 6.8, "max": 7.1},
        "agent_layer": {
            "required": True,
            "bound": True,
            "route": "same_webview_control_v1",
            "input_route": "native_cef_input",
            "receipt_verification": "matching_action_id_applied_v1",
            "llm_calls_in_hot_loop": 0,
            "screenshot_fallback_used": False,
            "external_input_fallback_used": False,
            "fail_closed": True,
        },
    }


class ReflexEvidencePackTests(unittest.TestCase):
    def test_sanitize_redacts_capabilities_paths_and_url_queries(self) -> None:
        value = {
            "control_capability": {"token": "do-not-publish"},
            "url": "https://example.com/game?session=private#score",
            "path": str(pathlib.Path.home() / "private" / "run.json"),
        }
        sanitized = PACKER.sanitize(value)
        self.assertEqual(sanitized["control_capability"], "[REDACTED]")
        self.assertEqual(sanitized["url"], "https://example.com/game")
        self.assertEqual(sanitized["path"], "$HOME/private/run.json")

    def test_output_file_is_rejected_without_iterdir_error(self) -> None:
        with tempfile.TemporaryDirectory(prefix="saccade-reflex-output-test-") as raw:
            output = pathlib.Path(raw) / "occupied"
            output.write_text("not a directory")
            with self.assertRaisesRegex(RuntimeError, "not a directory"):
                PACKER.ensure_clean_output(output)

    def test_validate_accepts_strict_mouseaccuracy_pass(self) -> None:
        PACKER.validate_pass_report(pass_report())

    def test_validate_rejects_receipt_mismatch(self) -> None:
        report = pass_report()
        report["benchmark_truth"]["targets_hit"] = 2
        with self.assertRaisesRegex(RuntimeError, "full-score proof"):
            PACKER.validate_pass_report(report)

    @unittest.skipUnless(shutil.which("ffmpeg") and shutil.which("ffprobe"), "ffmpeg required")
    def test_integration_builds_media_and_manifest(self) -> None:
        with tempfile.TemporaryDirectory(prefix="saccade-reflex-pack-test-") as raw:
            root = pathlib.Path(raw)
            run_dir = root / "run"
            output = root / "evidence"
            run_dir.mkdir()
            (run_dir / "report.json").write_text(json.dumps(pass_report()))
            (run_dir / "replay.jsonl").write_text(
                json.dumps(
                    {
                        "event": "pointer_applied",
                        "verified": True,
                        "capability_token": "private",
                    }
                )
                + "\n"
            )
            master = root / "master.mp4"
            subprocess.run(
                [
                    shutil.which("ffmpeg"),
                    "-nostdin",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=640x360:rate=30",
                    "-t",
                    "1.2",
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                    str(master),
                ],
                check=True,
            )
            args = argparse.Namespace(
                run_dir=run_dir,
                master_video=master,
                output_dir=output,
                build="test",
                title="Test reflex run",
                expected_game_duration_sec=1.0,
                preview_start_sec=0.0,
                preview_duration_sec=0.5,
                commit="0" * 40,
                platform_label="test-platform",
                allow_fail=False,
                max_gif_mib=8.0,
                ffmpeg=shutil.which("ffmpeg"),
                ffprobe=shutil.which("ffprobe"),
            )
            PACKER.package(args)
            expected = {
                "README.md",
                "SHA256SUMS",
                "embed.html",
                "environment.json",
                "manifest.json",
                "reflex-full.mp4",
                "reflex-loop.mp4",
                "reflex-loop.webm",
                "reflex-master.mp4",
                "reflex-poster.jpg",
                "reflex-readme.gif",
                "replay.jsonl",
                "report.json",
            }
            self.assertEqual({path.name for path in output.iterdir()}, expected)
            replay = json.loads((output / "replay.jsonl").read_text())
            self.assertEqual(replay["capability_token"], "[REDACTED]")
            manifest = json.loads((output / "manifest.json").read_text())
            self.assertEqual(manifest["schema"], PACKER.PACK_SCHEMA)
            self.assertGreater(len(manifest["files"]), 8)


if __name__ == "__main__":
    unittest.main()
