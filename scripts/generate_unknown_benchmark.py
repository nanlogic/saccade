#!/usr/bin/env python3
"""Generate a fresh semantic browser task that neither benchmark lane can memorize."""

from __future__ import annotations

import argparse
import html
import json
import random
from pathlib import Path


KINDS = ("native", "reveal", "replace")
WORDS = (
    "amber", "cedar", "delta", "ember", "fable", "glacier", "harbor", "indigo",
    "juniper", "kepler", "lumen", "marble", "nectar", "orbit", "prairie", "quartz",
    "raven", "saffron", "tundra", "velvet", "willow", "xenon", "yarrow", "zephyr",
)


def token(rng: random.Random, count: int = 2) -> str:
    return " ".join(rng.sample(WORDS, count)).title()


def build(kind: str, seed: str, url: str) -> tuple[str, dict]:
    if kind not in KINDS:
        raise ValueError(f"unknown kind: {kind}")
    rng = random.Random(seed)
    first_label, second_label = token(rng), token(rng)
    first_value, second_value = token(rng, 3), token(rng, 3)
    select_label, check_label = token(rng), token(rng)
    options = rng.sample([token(rng) for _ in range(6)], 3)
    selected = rng.choice(options)
    marker = "PROOF-" + "-".join(rng.sample([w.upper() for w in WORDS], 3))
    reveal_style = "display:none" if kind == "reveal" else ""
    replacement = kind == "replace"
    config = {
        "kind": kind,
        "firstLabel": first_label,
        "secondLabel": second_label,
        "firstValue": first_value,
        "secondValue": second_value,
        "selectLabel": select_label,
        "checkLabel": check_label,
        "options": options,
        "selected": selected,
        "marker": marker,
    }
    config_json = json.dumps(config, ensure_ascii=False).replace("</", "<\\/")
    page = f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Generated unknown semantic task</title>
<style>body{{font:16px system-ui;margin:32px}}main{{display:grid;gap:18px;max-width:620px}}
label{{display:grid;gap:6px}}input,select,button{{font:inherit;min-height:38px}}</style></head>
<body><main><h1>One-time generated task</h1>
<label>{html.escape(first_label)}<input id="first" type="text"></label>
<label><input id="gate" type="checkbox"> {html.escape(check_label)}</label>
<section id="dynamic" style="{reveal_style}">
  <label>{html.escape(second_label)}<input id="second" type="text"></label>
  <label>{html.escape(select_label)}<select id="choice">
    <option value="">Choose</option>
    {''.join(f'<option>{html.escape(option)}</option>' for option in options)}
  </select></label>
</section>
<button id="submit" type="button">Validate generated task</button><div id="result" role="status"></div>
</main><script>
const C={config_json};
const gate=document.getElementById('gate'); const dynamic=document.getElementById('dynamic');
gate.addEventListener('change',()=>{{
  if(C.kind==='reveal') dynamic.style.display=gate.checked?'':'none';
  if(C.kind==='replace' && gate.checked){{
    const old=document.getElementById('second'); const fresh=old.cloneNode(true);
    fresh.value=''; old.replaceWith(fresh);
  }}
}});
document.getElementById('submit').addEventListener('click',()=>{{
  const ok=document.getElementById('first').value===C.firstValue && gate.checked &&
    document.getElementById('second').value===C.secondValue &&
    document.getElementById('choice').selectedOptions[0]?.textContent===C.selected;
  document.getElementById('result').textContent=ok?C.marker:'NOT VERIFIED';
}});
</script></body></html>
"""
    dynamic_note = {
        "native": "The controls are all initially visible.",
        "reveal": f"Selecting {check_label} reveals the remaining controls.",
        "replace": f"Selecting {check_label} replaces the later text control before it is filled.",
    }[kind]
    task = {
        "schema": "saccade-agent-benchmark-task/1",
        "name": f"generated-unknown-{kind}-{seed[:8]}",
        "url": url,
        "task": (
            f"This page was generated after the model was chosen. Enter {first_value!r} in "
            f"{first_label!r}, select {check_label!r}, then enter {second_value!r} in "
            f"{second_label!r}, choose {selected!r} in {select_label!r}, and validate the task. "
            + dynamic_note
        ),
        "success": {"tool_output_contains": [marker]},
        "redact": [first_value, second_value],
        "timeout_seconds": 300,
        "generation": {"kind": kind, "seed": seed},
    }
    return page, task


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kind", choices=KINDS, required=True)
    parser.add_argument("--seed", required=True)
    parser.add_argument("--url", required=True)
    parser.add_argument("--html", type=Path, required=True)
    parser.add_argument("--task", type=Path, required=True)
    args = parser.parse_args()
    page, task = build(args.kind, args.seed, args.url)
    args.html.parent.mkdir(parents=True, exist_ok=True)
    args.task.parent.mkdir(parents=True, exist_ok=True)
    args.html.write_text(page, encoding="utf-8")
    args.task.write_text(json.dumps(task, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"kind": args.kind, "name": task["name"], "url": args.url}))


if __name__ == "__main__":
    main()
