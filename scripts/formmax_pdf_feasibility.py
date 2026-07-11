#!/usr/bin/env python3

import json
import os
import sys
from pathlib import Path

try:
    from pypdf import PdfReader, PdfWriter
    from reportlab.lib.pagesizes import letter
    from reportlab.pdfgen import canvas
except ModuleNotFoundError:
    bundled_python = (
        Path.home()
        / ".cache"
        / "codex-runtimes"
        / "codex-primary-runtime"
        / "dependencies"
        / "python"
        / "bin"
        / "python3"
    )
    if Path(sys.executable) != bundled_python and bundled_python.exists():
        os.execv(str(bundled_python), [str(bundled_python), __file__, *sys.argv[1:]])
    raise


ROOT = Path(__file__).resolve().parents[1]
OUT_DIR = ROOT / "runs" / "formmax" / "pdf_feasibility"
SOURCE_DIR = OUT_DIR / "source"


SENSITIVE_RULES = {
    "password": "password",
    "otp": "one-time code",
    "ssn": "government identifier",
    "social security": "government identifier",
    "tax": "government identifier",
    "routing": "banking information",
    "bank": "banking information",
    "card": "payment card",
    "medical": "medical identifier",
    "signature": "signature field",
    "attestation": "legal attestation",
    "consent": "consent field",
}

SYNTHETIC_FILL_VALUES = {
    "full_name": "Ada Lovelace",
    "capacity_mw": "4.20",
}


def classify_field(name):
    text = name.lower().replace("_", " ")
    for needle, reason in SENSITIVE_RULES.items():
        if needle in text:
            return {"sensitive": True, "reason": reason}
    return {"sensitive": False, "reason": None}


def create_acroform_pdf(path):
    c = canvas.Canvas(str(path), pagesize=letter)
    c.setTitle("FORMMAX AcroForm Fixture")
    c.setFont("Helvetica", 12)
    c.drawString(72, 744, "FORMMAX AcroForm Fixture")

    fields = [
        ("full_name", "Full name", 700),
        ("capacity_mw", "Capacity MW", 660),
        ("tax_id", "Tax ID", 620),
        ("signature", "Authorized signature", 540),
    ]
    for name, label, y in fields:
        c.drawString(72, y + 6, label)
        c.acroForm.textfield(
            name=name,
            x=240,
            y=y,
            width=220,
            height=22,
            borderStyle="inset",
            forceBorder=True,
        )

    c.drawString(72, 584, "Legal attestation")
    c.acroForm.checkbox(
        name="legal_attestation",
        x=240,
        y=580,
        size=16,
        borderStyle="solid",
        forceBorder=True,
    )
    c.drawString(262, 584, "I attest that this capacity plan is accurate.")
    c.save()


def create_flat_pdf(path):
    c = canvas.Canvas(str(path), pagesize=letter)
    c.setTitle("FORMMAX Flat PDF Fixture")
    c.setFont("Helvetica", 12)
    c.drawString(72, 744, "FORMMAX Flat PDF Fixture")
    c.rect(235, 695, 230, 28)
    c.drawString(72, 704, "Full name")
    c.rect(235, 655, 230, 28)
    c.drawString(72, 664, "Capacity MW")
    c.drawString(72, 604, "This PDF has no AcroForm fields.")
    c.save()


def field_report(path):
    reader = PdfReader(str(path))
    fields = reader.get_fields() or {}
    def clean_value(value):
        if value is None:
            return ""
        text = str(value)
        return "" if text in {"/Off", "Off"} else text

    return {
        name: {
            "value": clean_value(data.get("/V")),
            "classification": classify_field(name),
        }
        for name, data in fields.items()
    }


def fill_non_sensitive_pdf(source, output):
    reader = PdfReader(str(source))
    writer = PdfWriter()
    writer.clone_document_from_reader(reader)
    if hasattr(writer, "set_need_appearances_writer"):
        writer.set_need_appearances_writer(True)
    writer.update_page_form_field_values(writer.pages[0], SYNTHETIC_FILL_VALUES)
    with output.open("wb") as fh:
        writer.write(fh)


def main():
    SOURCE_DIR.mkdir(parents=True, exist_ok=True)
    acroform = SOURCE_DIR / "fillable_acroform.pdf"
    flat = SOURCE_DIR / "flat_no_fields.pdf"
    filled = OUT_DIR / "filled_non_sensitive.pdf"

    create_acroform_pdf(acroform)
    create_flat_pdf(flat)
    fill_non_sensitive_pdf(acroform, filled)

    acro_fields = field_report(acroform)
    filled_fields = field_report(filled)
    flat_fields = field_report(flat)

    sensitive_names = [
        name
        for name, info in acro_fields.items()
        if info["classification"]["sensitive"]
    ]
    filled_sensitive = [
        name
        for name in sensitive_names
        if filled_fields.get(name, {}).get("value")
    ]
    non_sensitive_filled_fields = sorted(
        name
        for name, info in filled_fields.items()
        if not info["classification"]["sensitive"] and info["value"]
    )

    failures = []
    if len(acro_fields) < 5:
        failures.append(f"expected at least 5 AcroForm fields, got {len(acro_fields)}")
    if not sensitive_names:
        failures.append("expected sensitive AcroForm fields")
    if filled_sensitive:
        failures.append(f"sensitive fields were filled: {', '.join(filled_sensitive)}")
    if set(non_sensitive_filled_fields) != {"full_name", "capacity_mw"}:
        failures.append(
            f"unexpected non-sensitive filled fields: {non_sensitive_filled_fields}"
        )
    if flat_fields:
        failures.append(f"flat PDF exposed fields: {sorted(flat_fields)}")

    result = {
        "fixture": "formmax_pdf_feasibility",
        "verdict": "PASS" if not failures else "FAIL",
        "artifacts": {
            "acroform_pdf": str(acroform),
            "flat_pdf": str(flat),
            "filled_non_sensitive_pdf": str(filled),
        },
        "cases": {
            "acroform": {
                "field_count": len(acro_fields),
                "sensitive_fields": sensitive_names,
                "non_sensitive_filled_fields": non_sensitive_filled_fields,
                "non_sensitive_filled_count": len(non_sensitive_filled_fields),
                "field_values_logged": False,
            },
            "flat_pdf": {
                "field_count": len(flat_fields),
                "reported_mode": "no_fillable_fields",
            },
            "browser_pdf_viewer": {
                "reported_mode": "unsupported_in_current_harness",
                "recommended_path": "programmatic_acroform_fill",
            },
        },
        "failures": failures,
    }

    serialized_result = json.dumps(result, sort_keys=True)
    leaked_values = [
        name
        for name, value in SYNTHETIC_FILL_VALUES.items()
        if value in serialized_result
    ]
    if leaked_values:
        result["failures"].append(
            f"result leaked synthetic values for fields: {', '.join(leaked_values)}"
        )
        result["verdict"] = "FAIL"

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    (OUT_DIR / "result.json").write_text(json.dumps(result, indent=2), encoding="utf-8")

    if failures:
        print(f"FORMMAX PDF FEASIBILITY FAIL failures={len(failures)}")
        for failure in failures:
            print(failure)
        raise SystemExit(1)

    print(
        "FORMMAX PDF FEASIBILITY PASS "
        f"acroform_fields={len(acro_fields)} "
        f"sensitive_fields={len(sensitive_names)} "
        "result=runs/formmax/pdf_feasibility/result.json"
    )


if __name__ == "__main__":
    main()
