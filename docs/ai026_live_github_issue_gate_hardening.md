# AI-026 Live GitHub Issue Gate Hardening

Date: 2026-07-06
Status: complete for gate hardening; real logged-in issue draft still pending

## What Happened

During the first visible GitHub issue dogfood attempt, the browser opened on
`https://github.com/` and Wayne clicked Saccade's Copilot/Profile chrome. The
visible page showed:

```text
Servo crashed!
Request body stream has already been closed while trying to connect to the request body stream.
```

Because the first command was not attached to a true interactive stdin, the
manual gate read EOF and continued. The harness then inspected GitHub Dashboard,
misclassified the visible GitHub Copilot textarea as a body draft target, filled
only the body slot, rejected the title/description slot, and still reported
`ok=true`.

No submit/publish/create action was clicked, and the artifact leak check stayed
green, but the run is invalid as a real GitHub issue measurement.

Invalid evidence, kept for debugging:

```text
runs/ai026_live_github_issue/github_issue_visible_20260706/report.json
```

## Fix

`scripts/run_ai020_live_draft.py` now has three additional safeguards:

1. Manual gate EOF is fatal.
   If `--manual-gate` is used from a non-interactive command and stdin returns
   EOF, the run fails before any fill.

2. Issue/discussion profiles have prefill URL gates.
   `github_issue` requires:

   ```text
   ^https://github\.com/[^/]+/[^/]+/issues/new(?:[/?#]|$)
   ```

   `github_discussion` requires:

   ```text
   ^https://github\.com/[^/]+/[^/]+/discussions/new(?:[/?#]|$)
   ```

   Local fixture URLs are exempt so regression pages still run.

3. Issue/discussion profiles require all requested slots.
   If title/body are requested, both `description` and `body` slots must be
   filled for the run to pass.

## Verification

Manual-gate EOF regression:

```text
runs/ai026_live_github_issue/manual_gate_eof_regression_20260706/report.json
ok=false
error=manual gate received EOF before human confirmation
fill=null
```

Wrong-page prefill gate regression:

```text
runs/ai026_live_github_issue/example_prefill_gate_20260706/report.json
ok=false
error=prefill gate failed: url_mismatch; no_visible_authoring_editor
fill=null
```

Positive local issue fixture:

```text
runs/ai026_live_github_issue/local_issue_prefill_gate_positive_20260706/report.json
ok=true
prefill_gate.ok=true
required_field_check.ok=true
filled_slots=body, description
submit_attempted=false
value_leak_check.ok=true
```

## Boundary

This closes the harness safety bug exposed by the live GitHub attempt. It does
not close the real logged-in GitHub issue/discussion measurement yet.

Next retry:

1. Launch visible Saccade with `--manual-gate` from an interactive terminal.
2. Human logs in and navigates to a real `https://github.com/<owner>/<repo>/issues/new...`
   URL.
3. Harness verifies the URL before fill.
4. Harness fills title/body only.
5. Human reviews and owns final submit.
