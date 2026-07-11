# AI-031 Generic Form Inventory and Plan

Date: 2026-07-11
Status: local failure recovery and two public test-form gates passed

## Result

The official ServoShell bridge now exposes three value-free control methods:

- `form_inventory` discovers ordinary and sensitive fields on the current page;
- `form_compile_plan` validates proposed assignments against a fixed page
  revision and returns eligible/rejected field IDs without assignment values;
- `form_execute_plan` requires the unchanged revision and compiled plan ID,
  writes only eligible fields, verifies each postcondition, preserves existing
  fields, and returns repair metadata without values or submit.

The same three methods are available through the engine-neutral MCP surface as
`saccade.web.form_inventory`, `saccade.web.form_compile_plan`, and
`saccade.web.form_execute_plan` after a user grants the current visible tab.

The inventory reports stable field ID, selector hash, type, label and confidence,
owner, sensitivity class, required/visible/enabled/readonly state, option count,
redacted completion state, eligibility, and block reasons. It does not return raw
field values or select option contents.

The plan rejects sensitive fields, human-owned fields, existing values, hidden or
disabled controls, unsupported types, ambiguous labels, unstable identities,
unknown field IDs, and stale page revisions. It performs no writes and cannot
submit the form. Execution rejects a stale revision, wrong plan ID, unsafe policy,
and structured/non-scalar assignments.

## Adversarial gate

Fixture: `test_pages/form_plan/index.html`

The fixture contains 17 controls, including ordinary text/select/number/date/
checkbox/contenteditable fields, existing user values, human ownership, SSN,
password, hidden and disabled controls, a file input, duplicate names, an
ambiguous label, and a display-hidden field.

Command:

```text
python3 scripts/probe_generic_form_plan.py \
  --output-dir runs/formmax/generic_execute_mcp_ai031_20260711_final
```

Observed:

```text
GENERIC FORM EXECUTION PASS fields=17 eligible=6 filled=6 preserved=4 rejected=12
```

The 12 rejections include the 11 ineligible fixture fields plus an explicit
unknown field request. The positive path fills and verifies all six eligible
fields through MCP, preserves four existing values, reports zero failed/repair
items, and advances the page revision once. The probe also proves stale revision,
wrong plan ID, explicit safe-policy, and scalar-only assignment rejection. It
scans the full output tree for three assignment sentinels; no sentinel appeared.

Artifact:

- `runs/formmax/generic_execute_mcp_ai031_20260711_final/report.json`
- `runs/formmax/generic_execute_mcp_ai031_20260711_final/bridge/control/replay.jsonl`

## Regression

The existing high-volume path remains green:

```text
SACCADE_SERVOSHELL_FORMMAX PASS rows=96 pages=2 filled=672
blocked_sensitive=3 receipt_verified=true
```

Artifact:

- `runs/servoshell_adapter/formmax_ai031_executor_regression_20260711/result.json`
- `runs/servoshell_adapter/formmax_ai031_executor_regression_20260711/replay.jsonl`

The focused Rust tests pass: 7 ServoShell tests and 3 MCP tests. The broader
`saccade-mcp selftest` did not produce a final aggregate report during this run;
it exited while exercising the existing GL/browser matrix after repeated
`GLD_TEXTURE_INDEX_2D` warnings. The focused MCP current-tab gate above remains
green, but the broad selftest is not counted as passed.

## Failed-postcondition gate

`test_pages/form_repair/index.html` deliberately uppercases one assigned value
inside its `input` handler. The executor attempted two writes: one verified and
one returned `postcondition_mismatch`. The result correctly has
`receipt_verified=false` and a non-looping `human_review_or_remap` repair. Any
write attempt now advances the page revision, so the original plan becomes
stale even when the page rejects or normalizes the requested value.

The fixture also preserved an existing value, left the SSN empty, attached
through MCP to the same visible WebView, and produced zero sentinel matches in
the output tree.

Artifact:

- `runs/formmax/generic_repair_ai031_20260711/report.json`

## Public test forms

Both pages are explicitly published for browser automation practice. Saccade
used the official ServoShell bridge and engine-neutral MCP tools, filled only
ordinary empty scalar controls, and never submitted either form.

| Page | Ready | Discovered | Selected | Verified | Repair | Response chars |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Selenium Web Form | 3.214 s | 16 | 5 | 5 | 0 | 12,695 |
| EvilTester Text Inputs | 4.068 s | 229 | 5 | 5 | 0 | 108,881 |

Both receipts verified, the same WebView was attached, and output-tree sentinel
scans were clean. The larger EvilTester response is useful evidence that the
inventory needs a compact/paged mode before broad token-efficiency claims.

Artifacts:

- `runs/formmax/public_selenium_web_form_ai031_20260711_final/report.json`
- `runs/formmax/public_eviltester_text_inputs_ai031_20260711/report.json`

The EvilTester legacy URL `/styled/basic-html-form-test.html` did not produce a
bridge-ready event within 45 seconds. The current documented Text Inputs route
passed; a 12-second structured confirmation is recorded at
`runs/formmax/public_eviltester_legacy_timeout_ai031_20260711/report.json`. The
legacy timeout remains a compatibility observation, not a success.

Remote pages remain gated. MCP accepts them only from an explicit official
ServoShell or Chrome compatibility artifact with exact runtime/transport
metadata and a loopback same-WebView control endpoint. Direct remote URL grants
remain blocked.

## Next slice

Package the three MCP tools in the next dogfood release, add compact/paged
inventory output, then run one human-reviewed draft on a non-sensitive real
workflow without submit.

This checkpoint proves the generic surface on local adversarial fixtures and two
public automation test pages. It does not claim arbitrary third-party form
compatibility or permission to submit.
