#!/usr/bin/env python3
"""Build a redacted human/agent agreement report from browser evidence."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
ROOT = SCRIPT_DIR.parents[0]
LIB_DIR = SCRIPT_DIR / "lib"
if not (LIB_DIR / "human_agent_agreement.py").exists():
    LIB_DIR = ROOT / "scripts" / "lib"
sys.path.insert(0, str(LIB_DIR))

from human_agent_agreement import (  # noqa: E402
    analyze_agreement,
    compare_screenshots,
    write_fact_overlay,
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare a user-visible reference with Saccade truth/actions."
    )
    parser.add_argument("--reference-truth", required=True, type=Path)
    parser.add_argument("--observed-truth", required=True, type=Path)
    parser.add_argument("--hit-test", type=Path)
    parser.add_argument("--reference-screenshot", type=Path)
    parser.add_argument("--observed-screenshot", type=Path)
    parser.add_argument("--visual-metrics", type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument(
        "--strict-visual",
        action="store_true",
        help="Route on a high screenshot diff instead of recording a visual warning.",
    )
    parser.add_argument(
        "--safe-visual-artifact",
        action="store_true",
        help="Write a screenshot overlay only when the caller has established that no protected values are present.",
    )
    parser.add_argument(
        "--allow-route",
        action="store_true",
        help="Exit zero even when the report recommends compatibility/block; useful for evidence generation.",
    )
    return parser.parse_args()


def load_json(path: Path | None) -> dict:
    if path is None:
        return {}
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"expected JSON object: {path}")
    return value


def main() -> int:
    args = parse_args()
    reference_truth = load_json(args.reference_truth)
    observed_truth = load_json(args.observed_truth)
    hit_test = load_json(args.hit_test) if args.hit_test else None
    visual_metrics = load_json(args.visual_metrics) if args.visual_metrics else None
    if visual_metrics is None and args.reference_screenshot and args.observed_screenshot:
        visual_metrics = compare_screenshots(
            args.reference_screenshot, args.observed_screenshot
        )

    report = analyze_agreement(
        reference_truth,
        observed_truth,
        hit_test=hit_test,
        visual_metrics=visual_metrics,
        strict_visual=args.strict_visual,
    )
    args.output_dir.mkdir(parents=True, exist_ok=True)
    artifacts = {
        "reference_truth": str(args.reference_truth),
        "observed_truth": str(args.observed_truth),
        "hit_test": str(args.hit_test) if args.hit_test else None,
        "reference_screenshot": str(args.reference_screenshot)
        if args.reference_screenshot
        else None,
        "observed_screenshot": str(args.observed_screenshot)
        if args.observed_screenshot
        else None,
        "overlay": None,
    }
    if args.safe_visual_artifact and args.observed_screenshot:
        overlay = write_fact_overlay(
            args.observed_screenshot,
            observed_truth,
            report,
            args.output_dir / "fact_overlay.png",
        )
        artifacts["overlay"] = str(overlay)
    elif args.observed_screenshot:
        artifacts["overlay_skipped_reason"] = "safe_visual_artifact_not_asserted"
    report["artifacts"] = artifacts
    report_path = args.output_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(
        "HUMAN_AGENT_AGREEMENT "
        f"verdict={report['verdict']} ok={str(report['ok']).lower()} "
        f"recall={report['metrics']['visible_control_recall']:.3f} "
        f"precision={report['metrics']['actionable_precision']:.3f} "
        f"route={report['recommended_route']} report={report_path}"
    )
    return 0 if report["ok"] or args.allow_route else 2


if __name__ == "__main__":
    raise SystemExit(main())
