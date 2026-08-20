#!/usr/bin/env python3
"""Generate a post-model, oracle-checked review queue for delta break-even tests."""

from __future__ import annotations

import argparse
import html
import json
import random
from pathlib import Path
from urllib.parse import urljoin


MODES = ("same_identity", "replacement", "navigation")
LENGTHS = (1, 5, 10, 25, 50)
WORDS = ("amber", "cedar", "delta", "ember", "fable", "glacier", "harbor", "indigo", "juniper", "kepler", "lumen", "marble")


def records_for(seed: str, length: int) -> list[dict[str, object]]:
    rng = random.Random(seed)
    records = []
    for index in range(length):
        risk = rng.randint(1, 99)
        evidence = rng.choice((True, False))
        records.append({
            "id": f"{rng.choice(WORDS).upper()}-{rng.randrange(1000, 9999)}-{index + 1}",
            "risk": risk,
            "evidence": evidence,
            "expected": "approve" if risk <= 45 and evidence else "reject",
        })
    return records


def shell(body: str, script: str) -> str:
    return f"""<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Generated review queue</title><style>body{{font:16px system-ui;margin:32px}}main{{max-width:640px;display:grid;gap:16px}}article{{border:1px solid #bbb;padding:20px;border-radius:12px}}button{{font:inherit;min-height:42px;margin-right:10px}}[role=status]{{min-height:24px}}</style></head>
<body><main><h1>One-time review queue</h1>{body}<div id="status" role="status"></div></main><script>{script}</script></body></html>"""


def card(record: dict[str, object], index: int, length: int) -> str:
    evidence = "present" if record["evidence"] else "missing"
    return f"""<article id="card"><h2>Record {index + 1} of {length}</h2>
<p>Record ID: <strong id="record-id">{html.escape(str(record['id']))}</strong></p>
<p>Risk score: <strong id="risk">{record['risk']}</strong></p><p>Evidence: <strong id="evidence">{evidence}</strong></p>
<button id="approve" type="button" aria-pressed="false">Approve record</button>
<button id="reject" type="button" aria-pressed="false">Reject record</button></article>"""


def build(seed: str, length: int, mode: str, url: str) -> tuple[dict[str, str], dict[str, object]]:
    if mode not in MODES:
        raise ValueError(f"unknown mode: {mode}")
    if length not in LENGTHS:
        raise ValueError(f"length must be one of {LENGTHS}")
    records = records_for(seed, length)
    marker = f"QUEUE-PROOF-{seed[:10].upper()}-{mode.upper()}-{length}"
    pages: dict[str, str] = {}
    if mode != "navigation":
        config = json.dumps({"records": records, "marker": marker, "mode": mode}, separators=(",", ":"))
        script = f"""const C={config};let i=0,errors=0;
function render(){{const r=C.records[i];if(!r){{document.getElementById('card')?.remove();document.getElementById('status').textContent=C.marker;return;}}
const markup={json.dumps(card(records[0], 0, length))};
if(C.mode==='replacement'&&i>0){{const holder=document.createElement('div');holder.innerHTML=markup;document.getElementById('card').replaceWith(holder.firstElementChild);bind();}}
document.querySelector('#card h2').textContent=`Record ${{i+1}} of ${{C.records.length}}`;document.getElementById('record-id').textContent=r.id;document.getElementById('risk').textContent=r.risk;document.getElementById('evidence').textContent=r.evidence?'present':'missing';}}
function choose(choice,event){{const r=C.records[i];if(choice!==r.expected){{errors++;document.getElementById('status').textContent=`Incorrect choice; errors ${{errors}}`;return;}}
event.currentTarget.setAttribute('aria-pressed',event.currentTarget.getAttribute('aria-pressed')==='true'?'false':'true');document.getElementById('status').textContent=`Accepted ${{r.id}}`;i++;render();}}
function bind(){{document.getElementById('approve').onclick=(e)=>choose('approve',e);document.getElementById('reject').onclick=(e)=>choose('reject',e);}}bind();render();"""
        pages["index.html"] = shell(card(records[0], 0, length), script)
    else:
        for index, record in enumerate(records):
            next_url = urljoin(url, f"step-{index + 2:03}.html") if index + 1 < length else ""
            config = json.dumps({"record": record, "marker": marker, "next": next_url}, separators=(",", ":"))
            script = f"""const C={config};function choose(choice,event){{if(choice!==C.record.expected){{document.getElementById('status').textContent='Incorrect choice';return;}}
event.currentTarget.setAttribute('aria-pressed','true');if(C.next)location.href=C.next;else{{document.getElementById('card').remove();document.getElementById('status').textContent=C.marker;}}}}
document.getElementById('approve').onclick=(e)=>choose('approve',e);document.getElementById('reject').onclick=(e)=>choose('reject',e);"""
            name = "index.html" if index == 0 else f"step-{index + 1:03}.html"
            pages[name] = shell(card(record, index, length), script)
    task = {
        "schema": "saccade-agent-benchmark-task/1",
        "name": f"generated-review-{mode}-{length}-{seed[:8]}",
        "url": url,
        "task": (
            f"Process all {length} records in the one-time review queue. For each current record, "
            "choose 'Approve record' only when its visible risk score is at most 45 and Evidence is present; "
            "otherwise choose 'Reject record'. Continue until the queue proves completion. Read each newly "
            "visible record; do not assume future records."
        ),
        "success": {"tool_output_contains": [marker]},
        "redact": [],
        "timeout_seconds": max(300, length * 20),
        "generation": {"seed": seed, "length": length, "mode": mode},
    }
    return pages, task


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", required=True)
    parser.add_argument("--length", required=True, type=int, choices=LENGTHS)
    parser.add_argument("--mode", required=True, choices=MODES)
    parser.add_argument("--url", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    pages, task = build(args.seed, args.length, args.mode, args.url)
    args.output.mkdir(parents=True, exist_ok=True)
    for name, page in pages.items():
        (args.output / name).write_text(page, encoding="utf-8")
    (args.output / "task.json").write_text(json.dumps(task, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"pages": len(pages), "task": task["name"]}))


if __name__ == "__main__":
    main()
