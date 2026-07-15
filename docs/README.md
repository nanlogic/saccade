# Saccade Docs Start Here

This directory contains both current planning documents and historical evidence.
Use this file as the navigation entry point.

## Current Source Of Truth

Read these first:

1. `docs/PRODUCT_CONTRACT.md` - the engine-neutral product definition and six
   non-negotiable guarantees.
2. `docs/CURRENT_ACTION_ITEMS.md` - the canonical short execution queue.
3. `docs/SACCADE_ADVANTAGES_AND_IMPROVEMENT_CHECKLIST.md` - product strengths,
   evidence status, improvement queue, release gates, and claim boundaries.
4. `docs/CURRENT_PLAN.md` - the broad product plan; milestone labels may lag the
   execution queue.
5. `docs/next_plan_v5_tracker.md` - normalized historical status and evidence map.
6. `docs/browser_compat_ledger.md` - known browser/product compatibility gaps.
7. `docs/decisions.md` - append-only decision ledger.

## Current Active Direction

Saccade is now past the original MOUSEMAX proof. The active product direction is:

```text
browser truth -> redacted action map -> verified action -> replay
```

The current P0 product gates are:

```text
AI-033 agent safety measurement
AI-031 real user-granted form drafts
AI-032 DOCMAX AcroForm inspect/fill/verify
```

Goal:

```text
Ship a human-supervised form and document copilot on top of the browser fact,
redaction, verified-action, and replay contract. Compatibility work continues
when it blocks these workflows; unsupported routes remain explicit.
```

## Document Groups

### Product Plan

- `docs/PRODUCT_CONTRACT.md`
- `docs/CURRENT_PLAN.md`
- `docs/CURRENT_ACTION_ITEMS.md`
- `docs/SACCADE_ADVANTAGES_AND_IMPROVEMENT_CHECKLIST.md`
- `docs/next_plan_v5_tracker.md`
- `docs/SACCADE_EVALUATION_GAUNTLET_v1.md`
- `docs/evaluation_gauntlet_execution_plan.md`

### Vendor Integration

- `docs/VENDOR_INTEGRATION_READINESS_PLAN.md`
- `docs/integration_contract_v1.md`
- `docs/integration_examples/`
- `docs/release_inventory.md`
- `docs/render_preflight.md`
- `docs/ai034_github_human_agent_agreement.md`

### Browser Productization

- `docs/browser_compat_ledger.md`
- `docs/browser_shell_basics_report.md`
- `docs/browser_productization_plan.md`
- `docs/servoshell_adapter_migration_plan.md`
- `docs/servoshell_source_strategy.md`

### Human + Agent Safety

- `docs/tabs_runtime_profile.md`
- `docs/login_handoff_profile.md`
- `docs/user_flow_selftest_report.md`
- `docs/safety_truth_profile.md`
- `docs/profile_persistence_report.md`
- `docs/browser_session_report.md`

### Practical Workflow Gates

- `docs/m10_formmax_fixture_report.md`
- `docs/m11_pdf_sensitive_report.md`
- `docs/formmax_practical_eval_plan.md`
- `docs/devmax_n2_report.md`
- `docs/mcp_skeleton_report.md`

### Reflex / Benchmark Evidence

- `docs/local_game_reflex_gate.md`
- `docs/browser_fact_stream.md`
- `docs/reflex_live_interface.md`
- `docs/m7_benchmark_report.md`
- `docs/m8_replay_visualization_report.md`
- `docs/m9_release_repro_report.md`

### Historical Background

- `docs/roadmap.md`
- `docs/SACCADE_NEXT_PLAN_v5.md`
- `docs/viability_review.md`
- `SACCADE_BUILD_SPEC_v4.md`

These are still useful for context, but they are not the current execution
source of truth.

## Cleanup Rule

Do not delete evidence reports just because they are old. Most of them contain
artifact paths, commands, or measured failures. Prefer adding a clear pointer
from current docs over moving files and breaking references.
