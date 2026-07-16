#!/usr/bin/env python3
"""Run DOCMAX local AcroForm and optional public blank-PDF gates."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import subprocess
import sys
import urllib.request

try:
    import pypdf  # noqa: F401
    import reportlab  # noqa: F401
except ModuleNotFoundError:
    bundled_python = (
        pathlib.Path.home()
        / ".cache"
        / "codex-runtimes"
        / "codex-primary-runtime"
        / "dependencies"
        / "python"
        / "bin"
        / "python3"
    )
    if pathlib.Path(sys.executable) != bundled_python and bundled_python.exists():
        os.execv(str(bundled_python), [str(bundled_python), __file__, *sys.argv[1:]])
    raise

from formmax_pdf_feasibility import create_acroform_pdf, create_flat_pdf


ROOT = pathlib.Path(__file__).resolve().parents[1]
DOCMAX = pathlib.Path(__file__).resolve().with_name("docmax_pdf.py")
if not DOCMAX.exists():
    DOCMAX = ROOT / "scripts" / "docmax_pdf.py"


def run(*arguments: str) -> None:
    subprocess.run([sys.executable, str(DOCMAX), *arguments], cwd=ROOT, check=True)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--public-url")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    output = args.output_dir.resolve()
    source = output / "source"
    source.mkdir(parents=True, exist_ok=True)
    input_pdf = source / "fillable_acroform.pdf"
    flat_pdf = source / "flat.pdf"
    create_acroform_pdf(input_pdf)
    create_flat_pdf(flat_pdf)
    assignments = output / "assignments.json"
    assignments.write_text(
        json.dumps(
            {
                "full_name": "Ada Lovelace",
                "capacity_mw": "4.20",
                "tax_id": "000-00-0000",
                "signature": "Synthetic Signature",
                "legal_attestation": True,
            }
        )
        + "\n"
    )
    replay = output / "replay.jsonl"
    local_inventory = output / "local_inventory.json"
    fill_report = output / "fill_report.json"
    filled_pdf = output / "filled_non_sensitive.pdf"
    run(
        "inspect",
        "--input",
        str(input_pdf),
        "--report",
        str(local_inventory),
        "--replay",
        str(replay),
    )
    run(
        "fill",
        "--input",
        str(input_pdf),
        "--assignments",
        str(assignments),
        "--output",
        str(filled_pdf),
        "--report",
        str(fill_report),
        "--replay",
        str(replay),
    )
    flat_report = output / "flat_inventory.json"
    run(
        "inspect",
        "--input",
        str(flat_pdf),
        "--report",
        str(flat_report),
        "--replay",
        str(replay),
    )
    fill = json.loads(fill_report.read_text())
    flat = json.loads(flat_report.read_text())
    failures: list[str] = []
    if fill.get("filled_count") != 2 or fill.get("blocked_count") != 3:
        failures.append(f"unexpected fill boundary: {fill}")
    if fill.get("receipt_verified") is not True or fill.get("values_logged") is not False:
        failures.append("local fill receipt or value boundary failed")
    if flat.get("mode") != "no_fillable_fields":
        failures.append("flat PDF was not routed as non-fillable")

    public: dict[str, object] | None = None
    if args.public_url:
        public_pdf = source / "public_blank.pdf"
        request = urllib.request.Request(args.public_url, headers={"User-Agent": "Saccade-DOCMAX/1.0"})
        with urllib.request.urlopen(request, timeout=25) as response:
            if response.headers.get_content_type() != "application/pdf":
                raise RuntimeError("public source did not return application/pdf")
            public_pdf.write_bytes(response.read())
        public_report = output / "public_inventory.json"
        run(
            "inspect",
            "--input",
            str(public_pdf),
            "--report",
            str(public_report),
            "--replay",
            str(replay),
        )
        inventory = json.loads(public_report.read_text())
        public = {
            "url": args.public_url,
            "mode": inventory.get("mode"),
            "pages": inventory.get("pages"),
            "field_count": inventory.get("field_count"),
            "class_counts": inventory.get("class_counts"),
            "values_logged": False,
        }
        if int(inventory.get("field_count", 0)) <= 0:
            failures.append("public blank PDF exposed no AcroForm fields")

    replay_text = replay.read_text()
    for value in ("Ada Lovelace", "4.20", "000-00-0000", "Synthetic Signature"):
        if value in replay_text:
            failures.append("DOCMAX replay leaked an assignment value")
    result = {
        "schema": "saccade-docmax-gate-v1",
        "verdict": "PASS" if not failures else "FAIL",
        "local": {
            "field_count": fill.get("field_count"),
            "filled_count": fill.get("filled_count"),
            "blocked_count": fill.get("blocked_count"),
            "receipt_verified": fill.get("receipt_verified"),
            "flat_mode": flat.get("mode"),
        },
        "public_blank": public,
        "output_pdf": str(filled_pdf),
        "values_logged": False,
        "failures": failures,
    }
    report = output / "report.json"
    report.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(f"DOCMAX_GATE verdict={result['verdict']} report={report}")
    return 0 if result["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
