# Codex vs Claude Agent Comparison Plan

Updated: 2026-06-13

## Goal

Compare Codex and Claude on the same Saccade gauntlet gates with the same prompt shape, same task commands, and the same normalized result schema.

This is an agent-layer comparison. It answers:

- Can the agent run the correct Saccade gate without drifting?
- Did the gate pass?
- How much wall time did it take?
- How many LLM events and tokens were reported by the CLI?
- Did it preserve useful artifact paths for replay/debugging?

It is separate from the Playwright/Chrome/Saccade engine baseline.

## Files

- `eval/agent_compare/tasks.json`: canonical task list.
- `eval/agent_compare/result_schema.json`: structured agent final response schema.
- `scripts/agent_compare.py`: task listing, dry-run plan, real runner, result parser, and SVG chart generator.

## Current Task Coverage

The first suite covers the local gates we already trust:

- Trusted Tabs runtime.
- Login handoff.
- Sensitive truth redaction.
- Full user-flow handoff.
- Native input probe.
- Browser session worker.
- MCP end-to-end selftest.
- DEVMAX static fixtures.
- DEVMAX Servo fixtures.
- FORMMAX scrolling capacity fixture plus validation.
- Chrome reference capture.
- Visual parity fixtures.
- MOUSEMAX local arena replay.

## Commands

List tasks:

```bash
python3 scripts/agent_compare.py list-tasks
```

Create a run plan without launching agents:

```bash
python3 scripts/agent_compare.py run --agent both --tasks all
```

Run a small first real comparison:

```bash
python3 scripts/agent_compare.py run --agent both --tasks trusted_tabs_runtime safety_truth_redaction --execute
```

Run the full comparison only when the machine is ready for long agent sessions:

```bash
python3 scripts/agent_compare.py run --agent both --tasks all --execute
```

Regenerate charts from an existing run directory:

```bash
python3 scripts/agent_compare.py report runs/agent_compare/<run_id>
```

## Outputs

Each run directory contains:

- `plan.json` and `plan.md`.
- `results.jsonl`.
- `summary.json`.
- `summary.md`.
- `charts/success_rate_by_agent.svg`.
- `charts/wall_time_by_task.svg`.
- `charts/tokens_by_task.svg`.
- `charts/llm_events_by_task.svg`.
- `charts/failures_by_suite.svg`.
- `raw/` CLI outputs for auditability when real runs are executed.

## Safety Notes

The runner defaults to a dry plan. Real agent execution requires `--execute`.

Saccade browser selftests bind localhost. The Codex runner therefore defaults to `--codex-sandbox danger-full-access`; this avoids measuring Codex's filesystem/network sandbox instead of Saccade's test result.

The runner does not pass full bypass permission flags unless `--dangerous` is explicitly set. Keep that off for the first benchmark pass. The task prompts tell agents not to edit source files or change git state.

Token fields are recorded only when the CLI exposes them. Missing token data stays `null`; the report does not invent estimates.
