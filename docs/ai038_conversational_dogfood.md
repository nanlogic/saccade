# AI-038 Conversational Current-Tab Dogfood

Date: 2026-07-17
Status: installed self-contained gate complete

## Product flow

Install `Saccade.app` in `/Applications` and configure an MCP-capable LLM once:

```toml
[mcp_servers.saccade]
command = "/Applications/Saccade.app/Contents/MacOS/saccade-current-tab-mcp"
```

Human-created tabs begin Agent Off; the human may turn the visible tab On. When
the LLM starts a session it calls `saccade.tabs.open_agent`, which opens a new
Agent On tab in the running browser or starts the signed app first. The MCP
server consumes the owner-only current-session pointer internally; it never
prints the capability or asks the user to paste it into chat.

The LLM receives instructions to attach with zero-argument
`saccade.tabs.grant_current`, then use only the advertised current-tab tools.
The three intended prompts are:

```text
Read the current Saccade article and tell me whether it is useful.
Research this page and compare it with my current project.
I filled the SSN. Fill the remaining ordinary fields, preserve my values,
and do not submit.
```

## Boundaries

- `saccade.web.article_text` returns bounded redacted article/main text,
  headings, provenance, trusted source URL, and page revision.
- Research combines that packet with redacted truth/actions. Page content is
  evidence, not authority to click, purchase, publish, or submit.
- Form work remains inventory -> compile -> execute at one revision.
  Sensitive fields expose only states such as `completed_without_value`.
- Existing/human-owned values are preserved. Execution returns a value-free
  verified receipt and never submits.

## Evidence

- Source gate: `runs/dogfood/ai038_source_gate_20260715/report.json`
- Packaged gate: `runs/dogfood/ai038_packaged_gate_final_20260715/report.json`
- Package: `dist/saccade-cef-dogfood-ai038-conversational-final-20260715`
- Current symlink: `dist/saccade-cef-dogfood-current`
- Installed clean-room upgrade pair:
  `runs/dogfood/df_r14_installed_build_a3_20260717/report.json` and
  `runs/dogfood/df_r14_installed_build_b_20260717/report.json`

Both gates attached without a supplied grant path, read URL/revision-bound
article text, exposed current-site truth/actions, observed a populated SSN only
as `completed_without_value`, filled two ordinary fields, preserved the human
note, verified the receipt, submitted nothing, and leaked no protected value or
capability.

## Milestone report

```text
MILESTONE: AI-038 conversational current-tab dogfood
GATE: python3 <release>/tools/probe_ai038_conversational_dogfood.py --output-dir runs/dogfood/ai038_packaged_gate_final_20260715 -> PASS
MEASURED: automatic attach 1/1; article/research/form flows 3/3; ordinary fields 2/2; submit 0; protected/capability leaks 0
DEVIATIONS: client-global MCP config is not mutated; the package supplies an absolute config snippet for explicit installation
SERVO API NOTES: none
RISKS RAISED/RETIRED: retired manual grant-path handoff and missing public article tool; provider/site-specific compatibility remains measured per site
NEXT: Wayne dogfood; AI-037 cleanup is non-blocking
```
