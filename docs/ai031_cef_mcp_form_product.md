# AI-031 CEF MCP Form Product Gate

Date: 2026-07-15
Status: passed

The signed CEF browser now exposes its revision-bound form planner through the
stable public MCP tools:

- `saccade.web.form_inventory`
- `saccade.web.form_compile_plan`
- `saccade.web.form_execute_plan`

The gate launches the CEF tab, consumes its owner-only grant with
`saccade-mcp serve-stdio`, discovers 16 visible/actionable form records, fills
six ordinary fields, preserves an existing human value, blocks SSN and
password assignments, verifies the renderer receipt, and does not submit.
Neither the MCP response nor replay contains assignment values.

Command:

```sh
python3 scripts/probe_cef_mcp_form_plan.py \
  --output-dir runs/cef_day5/mcp_form_product_gate_final
```

Evidence: `runs/cef_day5/mcp_form_product_gate_final/report.json`.

The run also exposed an engine-neutral boundary bug: CEF JSON encodes page
revisions as integer-like doubles such as `2.0`. MCP now accepts finite,
non-negative, integral JSON numbers while still rejecting fractional or
negative revisions.
