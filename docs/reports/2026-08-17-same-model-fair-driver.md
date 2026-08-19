# Same-model fair benchmark driver

Date: 2026-08-17. Status: driver complete and unit-tested; the matrix has **not**
been run, because the local `claude` CLI is still `Not logged in`.

This replaces the confounded comparison in
`reports/2026-08-17-fair-benchmark-matrix.md`, where the Saccade lane was an
interactive Claude session and the Playwright lane was `codex exec`. That matrix
could not measure Saccade-lane tokens or delta latency, and its two lanes did
not share a model.

## What changed

`scripts/benchmark_same_model_fair.py` drives **both** lanes with one `claude -p`
binary, one `--model`, one prompt template, one URL, one natural-language goal
and one success condition. Only the connected browser MCP differs:

| | Saccade lane | Playwright lane |
| --- | --- | --- |
| Observation | Saccade MCP `truth.read` | `@playwright/mcp@0.0.79` snapshot |
| Execution | Claude in Chrome (`--chrome`) in the same Saccade tab | the same Playwright MCP |
| MCP wiring | `--mcp-config` + `--strict-mcp-config` | `--mcp-config` + `--strict-mcp-config` |

The Playwright package string is read from
`benchmarks/playwright-mcp.lock.json`; it is never hard-coded in the driver.

## Measurement

- **Clock.** Every tool request and every tool return is stamped against the
  wrapper's own `time.monotonic()` as the stream-json event is read, giving
  `requested_ms`, `returned_ms` and `duration_ms` per call. `clock_source` is
  `wrapper_monotonic`; anything else is rejected.
- **Tokens.** `input_tokens` and `output_tokens` come from the Claude
  stream-json `result` event's `usage`. Both must be positive.
- **Delta latency.** `action_return_to_delta_read_ms` is measured from an
  execution tool's return to the next observation call's return, inside one
  process. Every sample is retained in `delta_latency_samples_ms`.
- **Discovery bytes.** `initial_transfer_bytes` accumulates **every**
  observation payload before the first executable action — an `index` read plus
  all subsequent `region` reads, not just the smallest response. The modes the
  model actually chose are recorded in `discovery_view_modes`.
- **View mode is free.** The Saccade prompt names `auto`, `full`, `index` and
  `region` and states that no mode is required or discouraged. A test asserts
  the prompt forces none of them.

## Validity

A run is `INVALID` when any lane is missing `wrapper_monotonic` timing, positive
end-to-end elapsed, positive input tokens, positive output tokens, positive
discovery bytes, a positive tool-call count, a numeric delta latency, or a
replacement-recovery count — or when the lane timestamps do not prove the
requested order. Nothing is estimated or back-filled. A lane that exits with
`Not logged in` before its first tool is reported as
`claude_cli_not_authenticated`. Other zero-tool failures remain
`browser_mcp_unavailable_no_tool_calls`, so authentication and browser plumbing
cannot be confused or credited to the other lane.

## Verification so far

The same-model driver and matrix runner have 18 tests covering: one binary and one
model for both lanes; each lane connecting only its own MCP, with `--chrome`
only on the Saccade lane; the Playwright version coming from the lock; identical
URL, goal and success condition; no forced view mode; request/return timestamps;
token extraction; cumulative index-plus-region discovery bytes; delta latency;
every missing-field `INVALID` path; timestamp-proven order; and the tasks
carrying no selector or site logic.

The wrapper reads stdout incrementally and timestamps each JSONL line when it
arrives. It does not stamp buffered output after process exit. Before a new
matrix, the runner moves an existing output directory to a timestamped
`.previous-*` sibling. Any non-PASS report makes the matrix command exit
nonzero.

An end-to-end dry run produced a well-formed `INVALID` report with
`order_errors: []` and identical error sets for both lanes, each lane's `final`
carrying `Not logged in · Please run /login`. The pipeline is therefore proven
up to the login boundary.

## Running it after login

```sh
python3 scripts/run_same_model_matrix.py \
  --runtime "$HOME/Applications/Saccade Dev Runtime.app/Contents/MacOS/saccade-runtime" \
  --runtime-dir "$HOME/Library/Application Support/Saccade Dev/runtime" \
  --model claude-opus-5 \
  --output "$HOME/Library/Application Support/Saccade Dev/evidence/20260817-same-model"
```

That runs 3 tasks × 2 orders and prints the matrix. Re-print later with
`--summarize-only`.

## Scope

The benchmark hardening changes only the driver, runner, tests, CI, and reports.
A separate MCP instruction correction removed the obsolete requirement to read
full Truth first; that rebuilt the Runtime but did not change the Extension,
Host, protocol, Collector, Profile, or Extension candidate. Browser denominator
evidence therefore remains current.

## Remaining blocker

`claude -p` exits `Not logged in`. Login needs an interactive terminal and a
browser OAuth flow, so it is Wayne's step. Until then no same-model comparison
exists, and the prior matrix's only comparable metric — initial payload, which
favored Playwright — still stands uncontested.
