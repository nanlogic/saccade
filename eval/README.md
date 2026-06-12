# Saccade Evaluation Pack

This directory is the future home for reproducible gauntlet runs.

Canonical spec:

- `docs/SACCADE_EVALUATION_GAUNTLET_v1.md`
- `docs/evaluation_gauntlet_execution_plan.md`

Planned layout:

```text
eval/
  00_mousemax_release/
  01_ui_torture/
  02_devmax/
  03_formmax/
  04_threadmax/
  05_webarena/
  06_workarena/
  07_pdf/
  08_trusted_tabs_safety/
  09_chrome_adapter/
  10_baselines/
```

Every conquered target should emit:

- `run.json`
- `replay.jsonl`
- `summary.md`
- `before.png`
- `after.png`
- optional click map, screenshot crops, and video

Minimum result schema:

```json
{
  "verdict": "pass|fail|blocked",
  "engine": "servo|chrome|hybrid",
  "target": "target name or URL",
  "task_id": "stable task id",
  "actions_attempted": 0,
  "actions_verified": 0,
  "policy_blocks": [],
  "human_confirmations": [],
  "errors": [],
  "replay_file": "replay.jsonl"
}
```
