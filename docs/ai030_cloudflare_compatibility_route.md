# AI-030 Cloudflare Compatibility Route

Date: 2026-07-09

## Result

Saccade now has an explicit compatibility route for public sites that the
primary Servo engine cannot load. Servo remains the default engine and the
millisecond reflex/truth proof. Compatibility mode uses a visible Chrome
window, a dedicated persistent profile, and the existing redacted Saccade
truth probe.

This is engine routing, not browser spoofing. It does not solve CAPTCHAs,
conceal automation, export cookies/storage, or expose sensitive field values.

## Diagnosis

`https://www.gameuidatabase.com/gameData.php?id=2444` was tested through four
separate paths:

| Path | Result |
| --- | --- |
| Current ServoShell bridge/WebDriver | Cloudflare `Just a moment...` |
| Source ServoShell in-process bridge, no WebDriver | Blocked after 15-20s; `navigator.webdriver=false` |
| Same source build plus the post-base module Blob lifetime claim | Still blocked after 20s |
| Fresh headless Chrome reference capture | Also blocked |
| Visible Chrome compatibility route with dedicated persistent profile | Passed twice |

The no-WebDriver run rules out WebDriver as the only trigger. Servo issue
`servo/servo#34320` remains open and describes the same Cloudflare Turnstile
failure; the related Blob URL lifetime work under `servo/servo#25226` is also
not a complete solution for this page.

The experimental one-file Blob patch was removed after the negative gate. No
UA spoofing or challenge bypass was added.

## Measured Compatibility Pass

First visible compatibility run:

- title: `Game UI Database - Botany Manor`
- elapsed: `4773ms`
- actions: `364`
- `navigator.webdriver=false`
- challenge observed: no
- cookie/storage/sensitive-value export: no
- evidence: `runs/chrome_compat/gameuidatabase_20260709/report.json`

Immediate repeat with the same dedicated profile:

- elapsed: `3022ms`
- actions: `364`
- evidence: `runs/chrome_compat/gameuidatabase_repeat_20260709/report.json`

Packaged wrapper smoke:

- current kit: `dist/saccade-dogfood-ai030-compat-20260709`;
- `open-saccade-compat` reached the same detail page in `3539ms` with 364
  actions and remained open in live-follow mode;
- the ready gate requires a stable `complete` state and keeps waiting if a SPA
  temporarily returns to `interactive` during settle;
- evidence:
  `dist/saccade-dogfood-ai030-compat-20260709/runs/chrome_compat/compat_20260709-182125/report.json`.

Live-follow gate:

- opened the Botany Manor detail page with `364` actions;
- navigated the same visible window to the Game UI Database home page;
- the redacted live report updated to title `Game UI Database 2.0 | Welcome`
  and `961` actions;
- `truth_stale=false` after navigation completed;
- evidence: `runs/chrome_compat/gameuidatabase_follow_check/report.json`.

Sensitive/query redaction gate:

- loaded the local sensitive fixture with SSN, card, password, and a query/fragment secret;
- none of the literal values appeared in `report.json` or `truth.json`;
- report URL, truth URL, and stored command URL exclude query and fragment;
- raw Chrome stderr is not persisted by the compatibility route;
- evidence: `runs/chrome_compat/sensitive_query_redaction_gate/report.json`.

Window-close invalidation gate:

- while open, live truth reported the local fixture normally;
- after Ctrl-C/window shutdown, `truth.json` was removed and the report changed
  to `ok=false`, `route=browser_closed`, `truth_stale=true`;
- evidence: `runs/chrome_compat/close_invalidation_gate/report.json`.

The corresponding negative Servo evidence is
`runs/nonwebdriver_page_status/gameuidatabase_blob_claim_release/report.json`.

## Use

Source checkout, one-shot gate:

```bash
python3 scripts/chrome_compat_cdp.py \
  'https://www.gameuidatabase.com/gameData.php?id=2444' \
  runs/chrome_compat/manual \
  --profile-dir runs/chrome_compat_profile/default
```

Packaged dogfood, live human browser plus current-page truth:

```bash
dist/saccade-dogfood-current/open-saccade-compat \
  'https://www.gameuidatabase.com/gameData.php?id=2444'
```

Explicit current-tab co-pilot grant for an agent session:

```bash
SACCADE_COMPAT_GRANT_CURRENT=1 \
  dist/saccade-dogfood-current/open-saccade-compat \
  'https://example.com'
```

This writes `current_tab_compat_grant.json` inside the dogfood directory. Give
that artifact path to `saccade.tabs.grant_current`; it is not a cookie export
or a reusable credential. The packaged local control gate passed using the
release MCP binary: `runs/chrome_compat_mcp/ai030b_packaged_fill/report.json`.

The packaged wrapper uses `--keep-open`. While the user navigates, it refreshes
`report.json` and `truth.json`. During loading, provider challenge, or browser
loss, it removes current truth and writes `truth_stale=true` instead of serving
the previous page as current fact.

## AI-030B Current-Tab Bridge

AI-030B attaches the existing engine-neutral MCP current-tab protocol to the
same visible compatibility window. It is an explicit Human grant, not an
ambient connection to a Chrome profile:

- `saccade.tabs.grant_current` attaches only to the loopback endpoint written
  by `--grant-current-tab` / `SACCADE_COMPAT_GRANT_CURRENT=1`.
- `saccade.web.truth`, `actions`, `act`, and named browser navigation operate
  on that visible tab. Low-risk clicks use Chrome browser input, not
  page-injected `element.click()`.
- Every accepted control action writes a sanitized control report and replay.
- `saccade.web.fill_agent_fields` is deliberately narrower than generic web
  fill: it accepts only visible, non-sensitive fields explicitly marked
  `data-owner="agent"` (or `data-saccade-owner="agent"`), and refuses existing
  values rather than overwriting them.
- `saccade.web.inspect_fields` returns ownership, sensitivity, and completion
  state only. It never returns values.

Local fixture gate: a visible agent-owned note field filled successfully;
inspection returned `completed_without_value`; a human-owned SSN field was
rejected as `sensitive_field`; and artifact scans found neither the fill text
nor the fixture SSN value. A second write to the now-nonempty note was rejected
as `already_has_user_value`. Evidence:
`runs/chrome_compat_mcp/ai030b_fill_note/report.json`,
`runs/chrome_compat_mcp/ai030b_fill_sensitive_reject/report.json`, and
`runs/chrome_compat/ai030b_fill_fixture/control/replay.jsonl`.

The measured Game UI Database page also attached through the same grant
surface with 364 redacted actions, without clicking an unmeasured third-party
control. Evidence:
`runs/chrome_compat_mcp/ai030b_gameuidatabase_attach/report.json`.

## Boundary And Next Step

AI-030A closes public read/navigation compatibility for this measured site.
AI-030B closes the current-tab MCP surface for truth, low-risk actions,
navigation, replay, and explicit agent-owned controls. Generic third-party
form filling, FORMMAX live fill, and a trusted compatibility-engine badge are
still separate work; they must be measured before a broader claim.

Servo continues to own MOUSEMAX/local-game reflex and normal compatible-site
dogfood. Compatibility mode is explicit and should be selected only after a
measured Servo blocker or when mainstream-browser visual parity is the task.
