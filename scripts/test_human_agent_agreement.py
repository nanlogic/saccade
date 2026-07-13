#!/usr/bin/env python3

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))

from human_agent_agreement import (  # noqa: E402
    analyze_agreement,
    compare_screenshots,
    write_fact_overlay,
)


def action(
    action_id: str,
    label: str,
    left: float,
    top: float,
    width: float = 100,
    height: float = 30,
    **overrides,
):
    value = {
        "action_id": action_id,
        "label": label,
        "kind": "click",
        "enabled": True,
        "visible": True,
        "offscreen": False,
        "blocked_by": None,
        "rect": {
            "left": left,
            "top": top,
            "right": left + width,
            "bottom": top + height,
            "width": width,
            "height": height,
        },
        "sensitivity": {"kind": "none", "completion_state": "not_sensitive"},
    }
    value.update(overrides)
    return value


def truth(actions, *, revision=7, width=800, height=600):
    return {
        "url": "https://fixture.invalid/form",
        "page_revision": revision,
        "viewport": {"width": width, "height": height, "devicePixelRatio": 1},
        "actions": actions,
    }


def hit_test(*states):
    return {
        "results": [
            {"action_id": f"act_{index}", "ok": state}
            for index, state in enumerate(states)
        ]
    }


class AgreementGateTest(unittest.TestCase):
    def test_missing_action_inventory_is_incomplete_evidence(self):
        report = analyze_agreement(
            {"url": "https://fixture.invalid/form"},
            {"url": "https://fixture.invalid/form"},
        )
        self.assertFalse(report["ok"])
        self.assertFalse(report["evidence_complete"])
        self.assertEqual(report["verdict"], "INCOMPLETE_EVIDENCE")
        self.assertIn(
            "AGREEMENT_ACTION_INVENTORY_MISSING",
            {item["code"] for item in report["reasons"]},
        )

    def test_green_agreement(self):
        reference = truth(
            [action("name", "Name", 10, 20), action("save", "Save", 10, 70)]
        )
        observed = truth(
            [action("name", "Name", 11, 20), action("save", "Save", 10, 71)]
        )
        report = analyze_agreement(
            reference,
            observed,
            hit_test=hit_test(True, True),
            visual_metrics={
                "dimension_match": True,
                "observed_nonblank": True,
                "diff_ratio": 0.01,
            },
        )
        self.assertTrue(report["ok"])
        self.assertEqual(report["verdict"], "PASS_ACTION_GREEN")
        self.assertEqual(report["metrics"]["visible_control_recall"], 1.0)
        self.assertEqual(report["metrics"]["actionable_precision"], 1.0)

    def test_missing_hidden_duplicate_and_actionability_route(self):
        reference = truth(
            [
                action("name", "Name", 10, 20),
                action("email", "Email", 10, 70),
                action("save", "Save", 10, 120),
            ]
        )
        observed = truth(
            [
                action("name", "Name", 10, 20),
                action("name_copy", "Name", 10, 20),
                action("save", "Save", 10, 120, enabled=False),
                action(
                    "backing",
                    "Backing editor",
                    0,
                    0,
                    width=0,
                    height=0,
                    visible=False,
                    enabled=True,
                ),
            ]
        )
        report = analyze_agreement(reference, observed, hit_test=hit_test(True))
        codes = {item["code"] for item in report["reasons"]}
        self.assertFalse(report["ok"])
        self.assertEqual(report["verdict"], "ROUTE_COMPATIBILITY")
        self.assertIn("AGREEMENT_MISSING_VISIBLE_FACT", codes)
        self.assertIn("AGREEMENT_EXTRA_VISIBLE_FACT", codes)
        self.assertIn("AGREEMENT_ACTIONABILITY_MISMATCH", codes)
        self.assertIn("AGREEMENT_DUPLICATE_ACTION", codes)
        self.assertIn("AGREEMENT_HIDDEN_ACTION", codes)

    def test_geometry_hit_test_and_revision_route(self):
        reference = truth([action("save", "Save", 10, 20)], revision=7)
        observed = truth([action("save", "Save", 400, 300)], revision=8)
        report = analyze_agreement(
            reference,
            observed,
            hit_test=hit_test(False),
            visual_metrics={
                "dimension_match": True,
                "observed_nonblank": True,
                "diff_ratio": 0.02,
            },
        )
        codes = {item["code"] for item in report["reasons"]}
        self.assertIn("AGREEMENT_GEOMETRY_ESCAPE", codes)
        self.assertIn("AGREEMENT_HIT_TEST_MISMATCH", codes)
        self.assertIn("AGREEMENT_REVISION_MISMATCH", codes)
        self.assertFalse(report["matches"][0]["geometry_ok"])
        self.assertFalse(report["matches"][0]["hit_test_ok"])

    def test_values_do_not_enter_report(self):
        sentinel = "SACCADE_SECRET_SENTINEL_9483"
        reference_action = action("ssn", "SSN (government_or_tax_id)", 10, 20)
        observed_action = action("ssn", "SSN (government_or_tax_id)", 10, 20)
        reference_action["value"] = sentinel
        observed_action["value"] = sentinel
        report = analyze_agreement(
            truth([reference_action]), truth([observed_action]), hit_test=hit_test(True)
        )
        self.assertNotIn(sentinel, json.dumps(report, sort_keys=True))
        self.assertFalse(report["privacy"]["field_values_included"])

    def test_screenshot_metrics_and_safe_overlay(self):
        try:
            from PIL import Image
        except ImportError:
            self.skipTest("Pillow unavailable")
        with tempfile.TemporaryDirectory() as raw_dir:
            output_dir = Path(raw_dir)
            reference_path = output_dir / "reference.png"
            observed_path = output_dir / "observed.png"
            overlay_path = output_dir / "overlay.png"
            Image.new("RGB", (200, 120), "white").save(reference_path)
            observed_image = Image.new("RGB", (200, 120), "white")
            for x in range(10, 111):
                for y in range(20, 51):
                    observed_image.putpixel((x, y), (20, 80, 180))
            observed_image.save(observed_path)
            metrics = compare_screenshots(reference_path, observed_path)
            self.assertTrue(metrics["dimension_match"])
            self.assertTrue(metrics["observed_nonblank"])
            observed_truth = truth([action("name", "Name", 10, 20)], width=200, height=120)
            report = analyze_agreement(
                observed_truth,
                observed_truth,
                hit_test=hit_test(True),
                visual_metrics=metrics,
            )
            write_fact_overlay(observed_path, observed_truth, report, overlay_path)
            self.assertTrue(overlay_path.exists())


if __name__ == "__main__":
    unittest.main()
