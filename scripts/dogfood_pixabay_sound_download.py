#!/usr/bin/env python3
"""Download one fixed Pixabay sound through the installed Saccade MCP."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import time
from typing import Any

from probe_ai038_conversational_dogfood import McpClient


ROOT = pathlib.Path(__file__).resolve().parents[1]
MCP = pathlib.Path(
    "/Applications/Saccade.app/Contents/MacOS/saccade-current-tab-mcp"
)
PIXABAY_URL = (
    "https://pixabay.com/sound-effects/"
    "film-special-effects-calm-elegant-logo-519008/"
)
EXPECTED_ORIGIN = "https://pixabay.com"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--timeout-sec", type=float, default=45.0)
    return parser.parse_args()


def action_label(action: dict[str, Any]) -> str:
    return str(action.get("label") or "").strip()


def download_action(actions: list[dict[str, Any]]) -> dict[str, Any] | None:
    ranked: list[tuple[int, dict[str, Any]]] = []
    for action in actions:
        label = " ".join(action_label(action).lower().split())
        if label == "free download":
            ranked.append((0, action))
        elif label == "download":
            ranked.append((1, action))
        elif label.startswith("download ") or label.endswith(" download"):
            ranked.append((2, action))
    ranked.sort(key=lambda item: item[0])
    return ranked[0][1] if ranked else None


def complete_receipt(receipts: dict[str, Any]) -> dict[str, Any] | None:
    for receipt in receipts.get("downloads", []):
        if receipt.get("status") == "complete":
            return receipt
    return None


def main() -> int:
    args = parse_args()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / "report.json"
    started = time.monotonic()
    stage = "initialize"
    mcp: McpClient | None = None
    clicked: list[dict[str, Any]] = []
    report: dict[str, Any]
    try:
        if not MCP.is_file():
            raise RuntimeError(f"installed Saccade MCP is missing: {MCP}")
        mcp = McpClient(MCP, os.environ.copy())
        mcp.request("initialize", {})

        stage = "open_agent_tab"
        opened = mcp.tool("saccade.tabs.open_agent", {"url": PIXABAY_URL})
        tab = opened.get("tab") or {}
        tab_id = int(tab["tab_id"])

        deadline = time.monotonic() + args.timeout_sec
        last_actions: dict[str, Any] = {}
        last_receipts: dict[str, Any] = {}
        completed: dict[str, Any] | None = None
        while time.monotonic() < deadline and len(clicked) < 3:
            stage = "inspect_actions"
            try:
                last_actions = mcp.tool("saccade.web.actions", {"tab_id": tab_id})
            except RuntimeError:
                time.sleep(0.5)
                continue
            candidate = download_action(last_actions.get("actions", []))
            if candidate is None:
                time.sleep(0.5)
                continue

            stage = "click_download"
            basis = int(
                candidate.get("basis_page_revision")
                or last_actions.get("page_revision")
            )
            mcp.tool(
                "saccade.web.act",
                {
                    "tab_id": tab_id,
                    "action_id": candidate["action_id"],
                    "basis_page_revision": basis,
                },
            )
            clicked.append({"label": action_label(candidate), "basis_revision": basis})

            receipt_deadline = min(deadline, time.monotonic() + 6.0)
            while time.monotonic() < receipt_deadline:
                stage = "download_receipt"
                last_receipts = mcp.tool(
                    "saccade.downloads.list", {"tab_id": tab_id}
                )
                completed = complete_receipt(last_receipts)
                if completed is not None:
                    break
                time.sleep(0.25)
            if completed is not None:
                break
            time.sleep(0.75)

        if completed is None:
            visible_download_labels = [
                action_label(action)
                for action in last_actions.get("actions", [])
                if "download" in action_label(action).lower()
            ][:10]
            raise RuntimeError(
                "no completed Pixabay download receipt; "
                f"download_actions={visible_download_labels!r}, "
                f"receipts={last_receipts.get('download_count', 0)!r}"
            )

        if completed.get("source_origin") != EXPECTED_ORIGIN:
            raise RuntimeError(f"unexpected download origin: {completed!r}")
        if (
            completed.get("full_path_exposed") is not False
            or completed.get("contents_exposed") is not False
            or completed.get("auto_executed") is not False
        ):
            raise RuntimeError("download receipt broadened local file authority")

        report = {
            "schema": "saccade-pixabay-live-download-v1",
            "verdict": "PASS",
            "page": PIXABAY_URL,
            "clicked": clicked,
            "file_name": completed.get("file_name"),
            "mime_type": completed.get("mime_type"),
            "source_origin": completed.get("source_origin"),
            "received_bytes": completed.get("received_bytes"),
            "status": completed.get("status"),
            "full_path_exposed": False,
            "file_contents_exposed": False,
            "auto_executed": False,
            "duration_sec": round(time.monotonic() - started, 3),
        }
    except Exception as error:
        report = {
            "schema": "saccade-pixabay-live-download-v1",
            "verdict": "FAIL",
            "stage": stage,
            "error": str(error),
            "clicked": clicked,
            "full_path_exposed": False,
            "file_contents_exposed": False,
            "duration_sec": round(time.monotonic() - started, 3),
        }
    finally:
        if mcp is not None:
            mcp.close()

    report_path.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"PIXABAY_SOUND_DOWNLOAD verdict={report['verdict']} "
        f"report={report_path}"
    )
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
