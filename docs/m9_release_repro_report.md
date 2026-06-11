# Saccade M9 Release Repro Report

Date: 2026-06-11

## Result

M9 adds a one-command release artifact validator.

Command:

```bash
scripts/validate_m9_release.sh runs/real/run_1781193985
```

The script checks:

- `cargo check -p mousemax`
- replay summary can be recomputed
- `click_map.png` can be regenerated from `replay.jsonl`
- `validate-run` passes with `--require-click-map`
- before, after, and click-map PNG files exist and have valid image headers

## Scope

This does not replace the final Linux/X11 rerun. It packages the known macOS M7 artifact into a reproducible local validation command.

M9 is complete when the release artifact command passes on the chosen artifact directory.

Observed output:

```text
M9 RELEASE VALIDATION PASS run=runs/real/run_1781193985
```
