# AI-044 Saccade vs Playwright MCP complete Agent-browser report

Date: 2026-07-18

Status: protocol benchmark passes; matched Codex end-to-end matrix is complete;
Claude cross-host smoke is pending account access and is not required for the
same-model Playwright comparison.

## Executive verdict

Saccade has a defensible win at the browser protocol boundary and a stronger
protected-data boundary. The current evidence does **not** support a universal
claim that every end-to-end LLM task is faster or uses fewer total tokens than
Playwright MCP.

- Protocol open-and-read: Saccade was 75.1% faster warm, returned 41.1% fewer
  marginal model-facing tokens, and used 50.0% fewer cold-context tokens.
- Matched Codex task matrix: after excluding four runs where Codex attached no
  MCP server at all, Saccade completed 7/7 attached runs; Playwright completed
  5/7 attached runs.
- Protected form: Saccade completed 3/3 with zero Passport-value exposure.
  Playwright completed only 1/3 under the same hard success rule because two
  accessibility snapshots returned the prefilled Passport value to the model.
- Screenshot use: neither browser needed a screenshot in the structured
  end-to-end matrix. The separate visual measurement shows that Playwright's
  1280x720 screenshot costs an estimated 920 GPT-5.6 image tokens plus 158 MCP
  metadata tokens, 1,078 tokens total.
- Claude: useful for a future cross-host compatibility claim, but not needed to
  determine Saccade vs Playwright under one controlled model. The available
  Claude Code CLI required Max/Pro or an API key, so no Claude result is mixed
  into the Codex matrix.

## Compared systems

Primary end-to-end matrix:

- Host/model surface: Codex CLI `0.145.0-alpha.18`, fresh ephemeral session per
  run.
- Saccade: signed macOS build 55, bundle `ai.saccade.browser`, CEF product MCP.
- Playwright: official `@playwright/mcp` package `0.0.78`, isolated headless
  Chrome.
- Host: Apple Silicon macOS 26.5.2.
- Network: localhost fixtures only. Each run received a unique URL and state.

Both browsers were given task-scoped least-authority tools. Playwright's
`browser_run_code_unsafe` and evaluation tools were intentionally excluded
from the primary matrix because Saccade has no equivalent arbitrary-code
authority. An earlier unrestricted smoke is retained as secondary evidence;
in that lane Playwright used arbitrary JavaScript to fill and verify the form
in one call.

## Tasks and success oracle

Each browser ran three independent tasks three times, for 18 fresh Agent
sessions total:

1. Read a structured article and return its verification phrase.
2. Fill three ordinary fields in a dynamic form, do not submit it, do not
   change the prefilled Passport field, and never expose the Passport sentinel
   to the model-visible transcript.
3. Click a normal `target=_blank` link and read the destination verification
   code from the newly opened tab.

The model's final claim was non-authoritative. The fixture server independently
checked requested routes, final form values, protected-value mutation, submit
events, and transcript leakage. Raw logs were sanitized before being written.

## Formal matrix result

The generated report says `PARTIAL`: 12 passed, six were initially recorded as
failed. Inspection separates those six into four host-attachment blocks and
two genuine task-policy failures:

| Browser | Attempts | MCP attached | Verified pass | Task-policy fail | Host blocked | Protected exposure |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Saccade | 9 | 7 | 7 | 0 | 2 | 0 |
| Playwright MCP | 9 | 7 | 5 | 2 | 2 | 2 |

The four zero-tool runs produced no page request and explicitly reported that
the configured browser MCP was absent from that Codex session. Both products
were affected twice, so these runs are retained as host reliability evidence
but are not attributed to either browser's task implementation. The runner now
classifies this signature as `blocked` in future runs.

The two Playwright policy failures were not inferred. Its sanitized raw
`browser_snapshot` results contain the marker
`[PROTECTED_SENTINEL_REDACTED]` at the value of the Passport textbox. The
unsanitized value was detected in memory before log redaction, so those runs
correctly fail the protected-data rule even though the ordinary fields were
filled.

## Performance by task

Performance rows include verified passes only. A single passing sample is
reported as `n=1`, not treated as a stable median.

| Task | Browser | Pass samples | Median wall time | Median total reported tokens | Median uncached tokens | Median tool calls |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Article read | Saccade | 3 | 22,158 ms | 79,083 | 19,477 | 3 |
| Article read | Playwright | 3 | 19,226 ms | 78,825 | 19,433 | 3 |
| Protected form | Saccade | 3 | 42,598 ms | 119,078 | 39,076 | 5 |
| Protected form | Playwright | 1 | 42,893 ms | 152,093 | 25,117 | 8 |
| New-tab navigation | Saccade | 1 | 40,565 ms | 133,526 | 38,806 | 7 |
| New-tab navigation | Playwright | 1 | 36,649 ms | 129,477 | 36,805 | 6 |

Interpretation:

- Article read is effectively tied on token use; Playwright was faster in this
  cold, independent-session Agent sample.
- The verified protected-form sample favors Saccade on total reported tokens
  and task success, but not uncached tokens. Playwright has only one valid
  sample because the other two exposed the protected value.
- New-tab performance has only one attached pass per browser, so no marketing
  speed or token conclusion should be drawn from it yet.
- Agent CLI token totals include prompt, tool schemas, tool results, reasoning,
  and output as reported by the host. Cached and uncached totals are shown
  separately because cache behavior varied materially across fresh sessions.

## Protocol-level result

The earlier five-run `example.com` open-and-read benchmark isolates browser
tool overhead from model sampling and cache variance:

| Metric | Saccade | Optimized Playwright MCP | Result |
| --- | ---: | ---: | ---: |
| Warm p50 open + main-text read | 162.755 ms | 654.004 ms | Saccade 75.1% lower |
| Median structured task result | 132 tokens | 224 tokens | Saccade 41.1% fewer |
| All tool schemas + first task | 2,120 tokens | 4,242 tokens | Saccade 50.0% fewer |
| Task plus one 1280x720 screenshot | 132 tokens | 1,302 tokens | Saccade 89.9% fewer |

This is the correct evidence for the narrow “faster tool and smaller protocol
context” claim. It is not evidence that model inference time will always be
lower on every workflow.

## Product fixes validated during the matrix

- Build 51 made a successful `form_execute_plan` receipt final verification.
  The model stopped re-inventorying and re-inspecting completed fields; the
  Saccade form path fell from eight calls to five.
- Builds 52-54 marked `target=_blank` actions as new-context actions and made
  only the exact Agent-clicked destination inherit Agent On. Human-created tabs
  remain Off by default.
- Build 55 made `web.act` wait for the exact Agent child destination to become
  readable and return its new revision. The next read can use that revision
  directly without listing tabs, granting a human tab, or guessing a URL.
- The independent Build 55 tab-default regression passed: Human-created tabs
  still start Off, while MCP-created Agent tabs start On. The installed-product
  cleanroom also passed with no repository dependency or forbidden source path.
- The benchmark runner gained task-scoped tool allowlists, independent fixture
  validation, protected-sentinel detection, sanitized raw logs, screenshot
  image-token estimation, cached/uncached accounting, and environment version
  capture.

## Claims allowed now

Recommended protocol claim:

> In our published macOS open-and-read protocol benchmark, Saccade used 41%
> fewer per-task model-facing tokens and 75% less warm wall time than the
> optimized official Playwright MCP configuration. Including all advertised
> tool schemas, Saccade used 50% fewer cold-context tokens.

Recommended protected-form claim:

> In a three-run Codex protected-form test, Saccade completed all three tasks
> without exposing the prefilled Passport value. Playwright MCP exposed that
> value through its accessibility snapshot in two of three runs under the same
> test rule.

Do not currently claim “Saccade always beats Playwright,” “always uses fewer
end-to-end tokens,” or “is always faster.” The real-Agent corpus shows a
protocol win and a safety/success win, not a universal latency win.

## Claude decision

Claude is not required to close this comparison because using the same Codex
host for both browsers removes model choice as a confounder. A Claude run would
answer a different question: whether the installed Saccade MCP behaves the
same under a second Agent host.

When Claude Code access is available, run the exact same task matrix against
both Saccade and Playwright. One pass is a compatibility smoke; three matched
repeats are needed before adding Claude to marketing evidence. There is no
reason to purchase a Claude subscription only for the current Playwright
comparison.

## Evidence

- Protocol report:
  `runs/benchmarks/playwright_parity_build49_evaluate_20260718/report.json`
- Installed-product cleanroom:
  `runs/dogfood/df_playwright_parity_build49_cleanroom_20260718/report.json`
- Build 55 formal 18-session matrix:
  `runs/benchmarks/agent_browser_e2e_build55_codex_least_authority_r3_20260718/report.json`
- Build 55 generated Markdown:
  `runs/benchmarks/agent_browser_e2e_build55_codex_least_authority_r3_20260718/report.md`
- Build 55 navigation regression:
  `runs/benchmarks/agent_browser_e2e_build55_codex_nav_least_authority_20260718/report.json`
- Build 55 Human-Off/Agent-On regression:
  `runs/dogfood/df_build55_tab_defaults_20260718/report.json`
- Build 55 installed-product cleanroom:
  `runs/dogfood/df_build55_installed_cleanroom_20260718/report.json`
- Benchmark runner: `scripts/benchmark_agent_browser_e2e.py`
- Task definitions: `eval/agent_browser_e2e/tasks.json`
- Structured result schema: `eval/agent_browser_e2e/result_schema.json`

Primary capability references:
[Playwright MCP](https://github.com/microsoft/playwright-mcp),
[Anthropic Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code/cli-usage),
and [OpenAI image-input token rules](https://developers.openai.com/api/docs/guides/images-vision).
