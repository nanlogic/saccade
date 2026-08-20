#!/usr/bin/env python3
"""Audit public/client-owned evidence without promoting local or actuator runs."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]


def load(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def audit(evidence_paths: list[Path]) -> dict[str, Any]:
    denominator = load(ROOT / "catalog/public_truth_cases.json")
    candidate = load(ROOT / "extension/candidate.json")
    accepted: dict[str, list[dict[str, Any]]] = {}
    rejected: list[dict[str, str]] = []
    for path in evidence_paths:
        evidence = load(path)
        reason = None
        if evidence.get("schema") != "saccade.public-client-evidence/1":
            reason = "not_client_owned_public_evidence"
        elif evidence.get("execution_owner") not in {"codex", "claude"}:
            reason = "execution_owner_is_not_supported_client"
        elif evidence.get("same_browser_tab") is not True:
            reason = "same_browser_tab_not_proven"
        elif evidence.get("extension_candidate") != candidate:
            reason = "candidate_mismatch"
        elif evidence.get("browser") not in {"chrome", "edge"}:
            reason = "unsupported_browser"
        elif evidence.get("outcome") not in {"pass", "truthful_limitation"}:
            reason = "evidence_did_not_pass"
        elif not evidence.get("source") or not evidence.get("url") or not evidence.get("denominator_id"):
            reason = "source_identity_missing"
        if reason:
            rejected.append({"path": str(path), "reason": reason})
            continue
        accepted.setdefault(evidence["denominator_id"], []).append(evidence)

    rows = []
    for item in denominator["items"]:
        records = accepted.get(item["id"], [])
        sources = {(row["source"], row["url"]) for row in records}
        browsers = {row["browser"] for row in records}
        required_sources = 1 if item["kind"] in {"boundary", "lifecycle"} else 2
        complete = len(sources) >= required_sources and browsers == {"chrome", "edge"}
        outcomes = {row["outcome"] for row in records}
        outcome = "blocked"
        if complete:
            outcome = "truthful_limitation" if outcomes == {"truthful_limitation"} else "pass"
        rows.append({
            "id": item["id"],
            "required_independent_sources": required_sources,
            "accepted_independent_sources": len(sources),
            "browsers": sorted(browsers),
            "outcome": outcome,
            "reason": "public_client_evidence_complete" if complete else "public_client_evidence_incomplete",
        })
    promoted = sum(row["outcome"] != "blocked" for row in rows)
    return {
        "schema": "saccade.public-evidence-audit/1",
        "extension_candidate": candidate,
        "policy": "Only Codex/Claude same-tab public evidence counts. Local fixtures and Reference Actuator reports are rejected.",
        "summary": {"total": len(rows), "promoted": promoted, "blocked": len(rows) - promoted},
        "rejected_evidence": rejected,
        "rows": rows,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", action="append", default=[], type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = audit([path.resolve() for path in args.evidence])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result["summary"]))
    return 0 if result["summary"]["blocked"] == 0 else 2


if __name__ == "__main__":
    raise SystemExit(main())
