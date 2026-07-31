# Fair Agent comparison: Saccade and Playwright

Date: 2026-07-31

Page: Selenium official `web-form.html`

Agent: Codex `gpt-5.6-terra`

## Result

Both products completed and independently observed `Received!` in two runs
with reversed lane order. Saccade used fewer browser calls and fewer input
tokens. Playwright completed faster. This result supports Saccade's protocol
and context-efficiency claim on this task; it does not support a general speed
or superiority claim.

| Lane | Passes | Mean browser calls | Mean input tokens | Mean elapsed |
| --- | ---: | ---: | ---: | ---: |
| Saccade | 2/2 | 5.5 | 100,228 | 43.126 s |
| Playwright | 2/2 | 9.0 | 162,352 | 33.620 s |

Relative to Playwright, Saccade used 38.9% fewer browser calls and 38.3% fewer
input tokens, while taking 28.3% longer. Saccade's output and reasoning tokens
were higher, so the next optimization target is schema-following and action
planning rather than Truth Layer size alone.

Elapsed time is directional, not a controlled browser-engine microbenchmark:
Saccade used the managed headed Chrome session, while Playwright MCP created an
isolated headless Chrome context. The fair controls here are Agent knowledge,
model, task, prohibited shortcuts, proof requirement, and accounting boundary.

## Fair-start rules

- Each lane ran in a separate ephemeral `codex exec` process with the same model.
- Each process received the same URL and natural-language goal.
- Saccade exposed only Saccade MCP; Playwright exposed only Playwright MCP.
- Shell, web search, apps, subagents, selectors, XPath, DOM queries, JavaScript,
  coordinates, screenshots, and remembered site structure were prohibited.
- Navigation, first observation/snapshot, planning, failed calls, actions,
  verification, elapsed time, and model usage all counted.
- A model statement was insufficient: browser tool output had to contain
  `Received!`.
- Editable values were redacted from saved JSONL, including URL-encoded forms.

## Recorded steps

Saccade performed `tabs.open`, one initial `web.observe`, one local
`web.form.fill`, and a separate verified Submit action. Each run contained one
recoverable malformed `web.act` attempt; the reverse-order run also first tried
to include Submit in the form plan, which correctly rejected non-form-plan
clicks. Total calls were five and six.

Playwright navigated, obtained semantic snapshots, filled the form, checked
controls as needed, clicked Submit, and resnapshotted the confirmation. Total
calls were eleven and seven. No selectors were supplied by the benchmark.

## Evidence

Local value-redacted evidence is retained outside Git:

- `20260731T1215Z/fair-agent-selenium-saccade-first/report.json`
- `20260731T1218Z/fair-agent-selenium-playwright-first/report.json`
- Each directory also contains both complete JSONL transcripts and stderr logs.

An earlier correctly isolated run before the select/focus fixes is retained as
failure evidence. An even earlier run with approval/routing mistakes is invalid
setup evidence and is excluded from product results.

## Interpretation

The former selector-predeclared oracle measured execution after a human had
already discovered the page. It remains useful for implementation regression,
but it is not a fair browser-Agent comparison. The primary benchmark now starts
at the unknown page and charges both systems for discovery through proof.
