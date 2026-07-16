#!/usr/bin/env python3
"""Inspect and safely fill AcroForm PDFs with value-free evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import sys
import tempfile
from typing import Any

try:
    from pypdf import PdfReader, PdfWriter
except ModuleNotFoundError as error:
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
    raise SystemExit("DOCMAX requires pypdf") from error


SENSITIVE_RULES = {
    "password": "password",
    "passcode": "password",
    "one time code": "one_time_code",
    "otp": "one_time_code",
    "social security": "government_identifier",
    "ssn": "government_identifier",
    "tax id": "government_identifier",
    "taxpayer identification": "government_identifier",
    "tin": "government_identifier",
    "ein": "government_identifier",
    "routing": "banking_information",
    "bank account": "banking_information",
    "account number": "banking_information",
    "credit card": "payment_card",
    "card number": "payment_card",
    "medical": "medical_identifier",
    "signature": "signature",
    "attestation": "legal_attestation",
    "certification": "legal_attestation",
    "consent": "consent",
}

ORDINARY_RULES = (
    "name",
    "address",
    "street",
    "city",
    "state",
    "province",
    "postal",
    "zip",
    "country",
    "email",
    "phone",
    "date",
    "capacity",
    "description",
    "title",
    "company",
    "organization",
    "business",
)


def clean_text(value: Any) -> str:
    if value is None:
        return ""
    return " ".join(str(value).replace("_", " ").split())


def field_label(field_id: str, data: dict[str, Any]) -> str:
    for key in ("/TU", "/TM", "/T"):
        label = clean_text(data.get(key))
        if label:
            return label
    return clean_text(field_id)


def classify_field(field_id: str, data: dict[str, Any]) -> dict[str, Any]:
    label = field_label(field_id, data)
    haystack = f"{clean_text(field_id)} {label}".lower()
    for needle, reason in SENSITIVE_RULES.items():
        if needle in haystack:
            return {"class": "sensitive", "reason": reason, "label": label}
    if any(needle in haystack for needle in ORDINARY_RULES):
        return {"class": "ordinary", "reason": None, "label": label}
    return {
        "class": "unknown_requires_human",
        "reason": "unclassified_pdf_field",
        "label": label,
    }


def value_present(value: Any) -> bool:
    text = clean_text(value)
    return bool(text and text not in {"/Off", "Off"})


def field_type(data: dict[str, Any]) -> str:
    return {
        "/Tx": "text",
        "/Btn": "button",
        "/Ch": "choice",
        "/Sig": "signature",
    }.get(str(data.get("/FT") or ""), "unknown")


def inspect_pdf(path: pathlib.Path) -> tuple[dict[str, Any], dict[str, Any]]:
    reader = PdfReader(str(path))
    fields = reader.get_fields() or {}
    inventory: list[dict[str, Any]] = []
    classes = {"ordinary": 0, "sensitive": 0, "unknown_requires_human": 0}
    for field_id, raw in fields.items():
        data = dict(raw)
        classification = classify_field(field_id, data)
        classes[classification["class"]] += 1
        inventory.append(
            {
                "field_id": field_id,
                "label": classification["label"],
                "type": field_type(data),
                "classification": classification["class"],
                "reason": classification["reason"],
                "value_state": "present_redacted"
                if value_present(data.get("/V"))
                else "empty",
            }
        )
    result = {
        "schema": "saccade-docmax-inventory-v1",
        "status": "ok",
        "mode": "acroform" if fields else "no_fillable_fields",
        "pages": len(reader.pages),
        "field_count": len(fields),
        "class_counts": classes,
        "fields": inventory,
        "sensitive_values_exposed": False,
        "values_logged": False,
        "source_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
    }
    return result, fields


def write_json(path: pathlib.Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def append_replay(path: pathlib.Path, event: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps({**event, "values_logged": False}, sort_keys=True) + "\n")


def inspect_command(args: argparse.Namespace) -> dict[str, Any]:
    inventory, _ = inspect_pdf(args.input)
    write_json(args.report, inventory)
    append_replay(
        args.replay,
        {
            "event": "pdf_inventory",
            "field_count": inventory["field_count"],
            "mode": inventory["mode"],
            "report": str(args.report),
        },
    )
    return inventory


def fill_command(args: argparse.Namespace) -> dict[str, Any]:
    assignments = json.loads(args.assignments.read_text())
    if not isinstance(assignments, dict) or not assignments:
        raise ValueError("assignments must be a non-empty JSON object")
    inventory, fields = inspect_pdf(args.input)
    if not fields:
        raise ValueError("PDF has no AcroForm fields")

    allowed: dict[str, Any] = {}
    blocked: list[dict[str, str]] = []
    for field_id in sorted(assignments):
        value = assignments[field_id]
        if not isinstance(value, (str, int, float, bool)) or value is None:
            blocked.append({"field_id": field_id, "reason": "unsupported_value_type"})
            continue
        raw = fields.get(field_id)
        if raw is None:
            blocked.append({"field_id": field_id, "reason": "field_not_found"})
            continue
        data = dict(raw)
        classification = classify_field(field_id, data)
        if classification["class"] != "ordinary":
            blocked.append({"field_id": field_id, "reason": classification["reason"]})
            continue
        if value_present(data.get("/V")):
            blocked.append({"field_id": field_id, "reason": "preserve_existing_value"})
            continue
        if field_type(data) not in {"text", "choice", "button"}:
            blocked.append({"field_id": field_id, "reason": "unsupported_field_type"})
            continue
        allowed[field_id] = value

    reader = PdfReader(str(args.input))
    writer = PdfWriter()
    writer.clone_document_from_reader(reader)
    if hasattr(writer, "set_need_appearances_writer"):
        writer.set_need_appearances_writer(True)
    for page in writer.pages:
        writer.update_page_form_field_values(page, allowed, auto_regenerate=False)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        dir=args.output.parent, prefix=f".{args.output.name}.", delete=False
    ) as handle:
        temporary = pathlib.Path(handle.name)
        writer.write(handle)
    os.replace(temporary, args.output)

    output_reader = PdfReader(str(args.output))
    output_fields = output_reader.get_fields() or {}
    failed: list[dict[str, str]] = []
    for field_id, expected in allowed.items():
        observed = output_fields.get(field_id, {}).get("/V")
        if clean_text(observed) != clean_text(expected):
            failed.append({"field_id": field_id, "reason": "postcondition_mismatch"})
    for item in inventory["fields"]:
        if item["classification"] == "sensitive":
            source_value = fields[item["field_id"]].get("/V")
            output_value = output_fields.get(item["field_id"], {}).get("/V")
            if clean_text(source_value) != clean_text(output_value):
                failed.append({"field_id": item["field_id"], "reason": "sensitive_changed"})

    result = {
        "schema": "saccade-docmax-fill-v1",
        "status": "ok" if not failed else "failed",
        "source_sha256": inventory["source_sha256"],
        "output_sha256": hashlib.sha256(args.output.read_bytes()).hexdigest(),
        "field_count": inventory["field_count"],
        "requested_count": len(assignments),
        "filled_count": len(allowed),
        "filled_fields": sorted(allowed),
        "blocked_count": len(blocked),
        "blocked_fields": blocked,
        "failed_count": len(failed),
        "failed_fields": failed,
        "protected_fields_changed": False,
        "receipt_verified": not failed,
        "submitted": False,
        "sensitive_values_exposed": False,
        "values_logged": False,
        "artifacts": {"output_pdf": str(args.output)},
    }
    encoded = json.dumps(result, sort_keys=True)
    leaked = [
        field_id
        for field_id, value in assignments.items()
        if clean_text(value) and clean_text(value) in encoded
    ]
    if leaked:
        raise AssertionError(f"DOCMAX report would leak assignment values for {leaked}")
    write_json(args.report, result)
    append_replay(
        args.replay,
        {
            "event": "pdf_fill_verified" if not failed else "pdf_fill_failed",
            "requested_count": len(assignments),
            "filled_count": len(allowed),
            "blocked_count": len(blocked),
            "failed_count": len(failed),
            "report": str(args.report),
        },
    )
    return result


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    inspect_parser = subparsers.add_parser("inspect")
    inspect_parser.add_argument("--input", type=pathlib.Path, required=True)
    inspect_parser.add_argument("--report", type=pathlib.Path, required=True)
    inspect_parser.add_argument("--replay", type=pathlib.Path, required=True)
    fill_parser = subparsers.add_parser("fill")
    fill_parser.add_argument("--input", type=pathlib.Path, required=True)
    fill_parser.add_argument("--assignments", type=pathlib.Path, required=True)
    fill_parser.add_argument("--output", type=pathlib.Path, required=True)
    fill_parser.add_argument("--report", type=pathlib.Path, required=True)
    fill_parser.add_argument("--replay", type=pathlib.Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    result = inspect_command(args) if args.command == "inspect" else fill_command(args)
    print(
        f"DOCMAX_{args.command.upper()} status={result['status']} "
        f"fields={result['field_count']} report={args.report}"
    )
    return 0 if result["status"] == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(main())
