# AI-044 Playwright MCP parity benchmark

Date: 2026-07-18

Status: matched open-and-read protocol wedge passes. The broader Build 55 Codex
matrix is documented in `docs/ai044_agent_browser_complete_report.md`; it adds
a protected-form safety win but still does not justify a universal speed or
token claim.

## Question

Can the signed Saccade product open a public page and return its main text with
lower model-facing token cost and lower wall time than the official Playwright
MCP package?

## Compared task

Each implementation opened `https://example.com` and returned the page's main
text. The success assertion required `Example Domain` in the returned text.
Five sequential iterations ran for the optimized structured lane. Playwright's
default full-snapshot mode received a separate two-iteration observation.

- Saccade: signed build 49, installed-product MCP, 20 advertised tools,
  `saccade.tabs.open_agent` followed by default-minimal
  `saccade.web.article_text`.
- Playwright: official `@playwright/mcp@latest` package, MCP server version
  `1.62.0-alpha-1783623505000`, 24 advertised tools,
  `browser_navigate`/`browser_tabs` followed by `browser_evaluate`.
- Playwright structured best case used `--snapshot-mode none`. This gives
  Playwright the lower-output configuration instead of forcing screenshots or
  default accessibility snapshots into every iteration.
- Both ran on the same Apple Silicon macOS host and public URL. Saccade used its
  headed signed CEF product; Playwright used its documented headless Chrome
  launch. Exact Chromium build parity was not asserted.

## Token accounting

Text and JSON used `o200k_base`. Counts include complete MCP tool result
envelopes. Cold-context totals also include every advertised tool schema.
Common user/model prompt text was omitted equally.

Playwright does not always require a screenshot. The structured comparison is
therefore reported without image cost. A separate visual lane calls
`browser_take_screenshot` and adds both non-image MCP metadata tokens and the
image tokens. For GPT-5.6 original/auto detail, the recorded 1280x720 PNG uses
`ceil(width/32) * ceil(height/32) = 920` image tokens. Its result metadata adds
158 text tokens, for 1,078 screenshot-result tokens total. This follows the
[OpenAI image-input token rules](https://developers.openai.com/api/docs/guides/images-vision).

## Result

| Metric | Saccade | Playwright MCP | Saccade result |
| --- | ---: | ---: | ---: |
| Warm p50 open + main-text read | 162.755 ms | 654.004 ms | 75.1% lower time |
| Median structured task result | 132 tokens | 224 tokens | 41.1% fewer |
| Cold context: all tool schemas + first task | 2,120 tokens | 4,242 tokens | 50.0% fewer |
| Structured task plus one 1280x720 screenshot | 132 tokens | 1,302 tokens | 89.9% fewer |

The optimized Playwright result is the primary comparison. Its default full
snapshot mode was larger at a 262.5-token median task result in the two-run
observation.

Evidence:

- `runs/benchmarks/playwright_parity_build49_evaluate_20260718/report.json`
- `runs/dogfood/df_playwright_parity_build49_cleanroom_20260718/report.json`
- `scripts/benchmark_playwright_parity.py`

The signed build also passed the repository-free installed-product cleanroom:
20 product tools, dynamic-form readiness, minimal/compact/full response gates,
and no logged values or forbidden repository paths.

## Product change that unlocked parity

Default article and form reads now return minimal task state while preserving
revision binding, Agent On enforcement, redaction, protected-value isolation,
and opt-in compact/full evidence modes. `open_agent` now returns only the
routing state the next call needs: readiness, ownership, tab ID, revision, and
whether the existing browser was reused. Grant paths, control endpoints,
capabilities, requested URLs, and verbose diagnostics stay out of model context.

## Resize and Canvas control wedge

Signed Build 57 closes the stale-coordinate failure first reproduced on
SimpleMMO. The renderer now pushes a separate layout epoch for resize, scroll,
zoom, device-scale and observed target-geometry changes. Immediately before
native input, the MCP obtains a fresh action map and performs one bounded local
semantic rebase only if the same stable action ID still exists. It then requires
a matching verified native-input receipt. A disappeared, covered or ambiguous
target is rejected before input instead of reporting optimistic success.

The source and packaged native macOS matrices used real window resizing and no
screenshots. Both proved:

- a responsive DOM action moved, rebased locally and returned a verified
  receipt;
- a stable Canvas-surface center action moved, rebased locally and returned a
  verified receipt; and
- a desktop-only action disappeared and received no native input.

The packaged run measured 288.002 ms from completion of the macOS resize command
to observed layout invalidation, 5.551 ms for DOM rebase plus receipt and 2.717
ms for Canvas rebase plus receipt. Evidence:
`runs/dogfood/df_build57_layout_epoch_source_20260718/report.json` and
`runs/dogfood/df_build57_layout_epoch_packaged_20260718/report.json`.

A signed Build 57 live SimpleMMO rerun then used the pre-resize `Show Chat`
request after narrowing the native window. Saccade rebased revision 37/layout
36 to revision 38/layout 37 and returned a verified receipt. The same old map's
`Register Account` action had disappeared and was rejected before native input.
Evidence: `runs/dogfood/df_build57_resize_live_simplemmo_20260718/report.json`.

The defensible comparison is narrow. Playwright's official
[locators](https://playwright.dev/docs/locators) resolve an up-to-date DOM
element at action time, so responsive DOM controls are not a Saccade-only win.
Playwright MCP's official [vision mode](https://playwright.dev/mcp/vision-mode),
however, uses screenshot-derived coordinate mouse tools, and Playwright's
[Mouse API](https://playwright.dev/docs/api/class-mouse) dispatches the supplied
viewport coordinates. If layout changes after observation, the host/model must
observe again or risk an obsolete coordinate. Saccade's scoped advantage is the
browser-native layout notification, local rebase and verified receipt without
another screenshot or LLM turn.

This fixture proves geometry rebase for a stable Canvas surface. It does not yet
prove semantic rediscovery of arbitrary targets drawn *inside* a Canvas/WebGL
scene; that requires browser-native game/pixel truth and remains a separate
gate.

## Allowed claim

Use this exact scope until the broader corpus closes:

> In our published macOS `example.com` open-and-read benchmark, Saccade used
> 41% fewer per-task model-facing tokens and 75% less warm wall time than the
> optimized official Playwright MCP configuration. Including all tool schemas,
> Saccade used 50% fewer cold-context tokens.

When discussing visual control, add:

> Playwright's 1280x720 screenshot added 920 image tokens plus 158 result
> metadata tokens in this benchmark; Saccade completed this structured reading
> task without a screenshot.

For the measured resize/coordinate wedge, use:

> In signed Build 57's native resize gates and live SimpleMMO rerun, Saccade
> locally rebased the same surviving semantic action and verified the native
> input without another screenshot or LLM turn; actions removed by the new
> layout were rejected before input. This result applies to coordinate and
> stable Canvas-surface workflows, not Playwright's DOM locator path or
> arbitrary targets drawn inside Canvas/WebGL.

Do not shorten this to “Saccade always beats Playwright.” A broader claim still
requires multiple task classes, sites, authenticated state, action completion,
recovery, repeated runs, and matched success criteria. Playwright remains a
cross-browser testing framework; this benchmark compares its MCP Agent-browser
surface only. Official capability references:
[Playwright MCP](https://github.com/microsoft/playwright-mcp) and
[Playwright CLI](https://github.com/microsoft/playwright-cli).

## Rerun

```sh
PYTHONPATH=/private/tmp/saccade-benchmark-deps \
  /usr/bin/python3 scripts/benchmark_playwright_parity.py \
  --saccade-app /private/tmp/Saccade-build49-parity.app \
  --output-dir runs/benchmarks/playwright_parity_build49_evaluate_20260718 \
  --iterations 5
```

The script requires `tiktoken` on the temporary `PYTHONPATH`, network access,
and permission to launch both browser processes.
