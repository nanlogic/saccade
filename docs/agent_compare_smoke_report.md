# Codex vs Claude Agent Compare Smoke Report

Updated: 2026-06-13

Run:

- `runs/agent_compare/run_1781365508552/summary.md`

Scope:

- `trusted_tabs_runtime`
- `safety_truth_redaction`

Result:

| Agent | Records | Passed | Pass rate | Wall time | Tokens |
| --- | ---: | ---: | ---: | ---: | ---: |
| Codex | 2 | 2 | 100% | 43.3s | 60,932 |
| Claude | 2 | 0 | 0% | 4.1s | unavailable |

Interpretation:

- Codex successfully ran both Saccade gates through the benchmark harness.
- Claude did not reach the tasks on this machine. Claude Code returned 403: subscription access is disabled for this organization and it needs an Anthropic API key or admin enablement.
- This is a harness/auth smoke, not yet a fair model-vs-model result.

Artifacts:

- Summary: `runs/agent_compare/run_1781365508552/summary.md`
- Normalized records: `runs/agent_compare/run_1781365508552/results.jsonl`
- Charts: `runs/agent_compare/run_1781365508552/charts/`

Next:

Configure Claude Code access, then rerun:

```bash
python3 scripts/agent_compare.py run --agent both --tasks trusted_tabs_runtime safety_truth_redaction --execute
```

After that passes for both agents, run the full suite:

```bash
python3 scripts/agent_compare.py run --agent both --tasks all --execute
```
