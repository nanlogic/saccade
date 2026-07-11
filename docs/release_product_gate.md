# Release Product Gate

This is the smallest repeatable gate for the local Saccade evaluation kit. It
checks the V3 MCP contract implementation and the end-to-end MCP selftest in a
fresh run directory.

Run from the repository root:

```bash
scripts/release_product_gate.sh
```

Override the output directory when comparing runs:

```bash
SACCADE_PRODUCT_GATE_OUT=runs/product_gate/manual_01 \
  scripts/release_product_gate.sh
```

The gate requires:

- `cargo test -p saccade-mcp` to pass;
- MCP selftest to report a JSON artifact;
- the selftest artifact to report registered tools, tab scoping, local
  development audit, and policy gate success.
- the generic form boundary fixture to verify a receipt, reject stale and
  unsafe requests, preserve existing human values, block sensitive fields, and
  write no field values to evidence.

The gate does not claim that every third-party website works. Site-specific
compatibility, WebGL, authentication-provider behavior, and release signing
remain separate evidence gates. macOS `GLD_TEXTURE_INDEX_2D` warnings are kept
in the run's stderr log and are recorded as known runtime warnings rather than
silently discarded.
