# Saccade Browser Productization Plan

Date: 2026-06-16

## Goal

Make Saccade comfortable enough to dogfood as Wayne's default browser layer for Saccade work, while keeping claims honest:

- Saccade should expose the same browser facts and action targets the agent will use.
- The user-visible shell should feel like a practical browser: URL, navigation, loading, focus, readable sizing, and recoverable controls.
- Servo does not need to become Chrome, but Chrome differences must be measured, classified, and routed.

2026-06-16 pivot: product browser UI should come from official ServoShell, not
the legacy `saccade-shell` GL toolbar. The legacy shell remains useful for
adapter proof and safety experiments, but the human-facing browser should keep
official ServoShell's egui address bar, tabs, toolbar commands, and WebView
layout behavior intact while Saccade supplies the agent/safety/replay bridge.

## Research Notes

- Servo CSS Grid support is still relatively new. Servo announced Grid as experimental in late 2024 and exposed it through `layout.grid.enabled`.
- Servo 0.2.0 includes recent form/focus improvements such as `<select multiple>`, input/textarea `selectionchange`, `activeElement`, focus/blur related target, form submission, and desktop focus/IME fixes. This is encouraging, but not a Chrome parity claim.
- CSS Grid percentage and `fr` behavior depends on container definiteness and intrinsic sizing. Percent tracks can become `auto` when the grid container depends on its tracks; standalone `fr` implies an automatic minimum.
- `min-width:auto` on grid/flex items can use min-content sizing, which is a common reason 50%/100% layouts and long form controls overflow or appear narrower than expected.
- HTML form controls involve user-agent rendering and intrinsic/native behavior. Inputs can be treated as replaced/button-like controls in parts of the rendering model, so exact cross-engine appearance and natural sizes are not guaranteed.
- Web Platform Tests are the right upstream compatibility oracle. Local fixtures should be small WPT-like reductions before we decide whether a difference is ours, Servo's, or page CSS.

Sources:

- Servo Grid announcement: https://servo.org/blog/2024/12/09/this-month-in-servo/
- Servo 0.2.0 forms/focus notes: https://servo.org/blog/2026/05/31/april-in-servo/
- MDN `grid-template-columns`: https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/grid-template-columns
- MDN `min-width`: https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/min-width
- WHATWG HTML rendering/form-control notes: https://html.spec.whatwg.org/multipage/rendering.html
- Web Platform Tests: https://web-platform-tests.org/

## Product Principles

1. Do not hide renderer truth. Do not inject arbitrary CSS into third-party pages just to make Servo look like Chrome.
2. Fix Saccade adapter bugs first: viewport, DPR, resize, input routing, focus, screenshots, and action maps.
3. For engine differences, add a minimal fixture and ledger entry before choosing a workaround.
4. Use `chrome-reference` for UI-design review, public screenshots, and pixel-sensitive judgement.
5. Use `servo-modern` for agent action, safety, replay, and local dogfood unless a red compatibility gate routes elsewhere.

## Profile And Session Product Model

Saccade has two audiences in the same browser: the human owner and the agent
assistant. Browser profile data belongs to the human owner. The agent never gets
raw cookies, password-manager data, local/session storage dumps, or sensitive
field values; it only receives current-session, explicitly granted,
policy-redacted truth/action/control surfaces.

Modes:

- Normal profile: default daily browser mode. Login cookies, site storage, and
  history-like browser state persist in a Saccade profile, currently
  `runs/dogfood_profile/default` for local dogfood builds. This should feel like
  Chrome/Safari profile reuse: close and reopen the Saccade browser, and normal
  sites can remain logged in when the provider allows it.
- Incognito / Ephemeral profile: future explicit mode for untrusted checks,
  logged-out comparison, and throwaway browsing. It should use a temporary
  profile directory and delete it on close. Agent grants can still exist inside
  the incognito session, but nothing should persist after shutdown.
- Named profiles: future UX for `default`, `work`, `test`, or project-specific
  profiles. A visible chrome badge/picker should show the active profile and
  whether the current tab is Human-only or Copilot-granted.

Safety rules:

- Login, 2FA, CAPTCHA, password, payment, signing, account security, and
  destructive actions remain human-owned.
- Agent attach is a tab/session grant, not ownership of the browser profile.
- Redacted reports and replay artifacts must not include raw cookies or
  sensitive values. The profile directory itself is local browser data and must
  stay out of git/backups unless intentionally managed.
- Clearing or switching profiles should be an explicit user action with visible
  state, not a hidden wrapper side effect.

## Workstreams

### P0 - Measurement Harness

Add a browser productization compare suite, separate from public demo packs.

Fixtures to add:

- `grid_percent_100_50`: `width:100%`, `width:50%`, `grid-template-columns: 50% 50%`, `1fr 1fr`, `minmax(0, 1fr)`, nested percentage grids.
- `grid_min_auto`: long tokens, form controls, `min-width:auto` versus `min-width:0`, overflow detection.
- `form_intrinsic_controls`: input/select/date/number/button/checkbox/radio/textarea with fixed, percent, auto, and grid/flex containers.
- `responsive_breakpoints`: media queries at 390, 768, 1000, 1280, 1920 CSS px.
- `sticky_fixed_transform`: sticky headers, fixed overlays, transforms, hit-testing.
- `editors`: textarea, contenteditable, CodeMirror-like DOM, Monaco-like DOM, GitHub/Gist reduction.

Metrics:

- Chrome/Saccade screenshots and diff.
- `window.innerWidth/innerHeight`, DPR, visual viewport if available.
- Computed styles for `display`, grid templates, gaps, widths, min/max widths, overflow, font, line-height.
- Element rects for controls and probes.
- Action map delta and Chrome-side non-mutating hit-test.
- Overflow classification: none, clipped, horizontal-scroll, offscreen-action.

Done when:

- Each fixture has a `case_result.json`.
- Each case is classified as `PASS_ACTION_GREEN`, `YELLOW_VISUAL`, `YELLOW_CONTROL`, `FAIL_LAYOUT`, `FAIL_ACTION_MAP`, or `ENGINE_LIMIT_RECORDED`.

### P1 - Browser Shell Basics

Implement the minimum shell that makes Saccade usable for daily dogfood:

- URL bar with current URL.
- Back, Forward, Reload/Stop.
- Page title and loading/error state.
- Focus indicator for page versus chrome controls.
- Escape closes active select/popover-like embedder state where possible.
- Stable resize behavior, already partially done by HiDPI/runtime viewport fix.

Done when:

- Wayne can open a URL, navigate back/forward, reload, resize, and recover focus without terminal commands.
- The shell does not visually squeeze page content with side panels or hidden control overlays.

Current state:

- First stage shipped in `docs/browser_shell_basics_report.md`: the native window title now exposes current URL, page title, load state, Back/Forward availability, and Reload shortcut without squeezing the page.
- Existing shortcuts: `Cmd+L` opens a keyboard address command, `Cmd+R` reloads, `Cmd+[` goes back, and `Cmd+]` goes forward.
- Current-tab co-pilot grant shortcut: `Cmd+Shift+G` marks the visible dogfood
  browser tab as granted, updates the title to `copilot=granted`, and writes a
  grant artifact at `runs/current_tab_grants/latest.json`.
- Dogfood grant artifacts now include a loopback `control_endpoint`. MCP can
  ping that endpoint and prove it is talking to the same live dogfood WebView.
  Shell status/navigation, redacted truth, action-map reads, safe agent-owned
  field fill, redacted field inspect, safe non-side-effect act, and local
  FORMMAX long-form fill now use that same endpoint when present;
  submit/external side effects still require user confirmation.
- Mouse Back/Forward buttons now navigate browser history when available.
- Page mouse press now recovers from active address-entry/select shell modes and still forwards the click to Servo.
- Native toolbar overlay v0 now shows and consumes hit-zones for Back,
  Forward, Reload, address command, and Copilot grant without injecting DOM.
- The toolbar address strip now paints shell-owned URL/placeholder text,
  secure/search icon, active focus/error state, caret, and a loading indicator
  line without injecting DOM into the page.
- MCP now exposes named shell navigation as `saccade.browser.navigate` for
  already-granted same-WebView dogfood tabs, covering
  status/navigate/reload/back/forward and updating session URL/title/revision
  from shell receipts.
- Product direction: do not continue hand-polishing the legacy GL toolbar into a
  full browser. Official ServoShell already has egui browser chrome that resizes
  the WebView below the toolbar and includes location input, Back/Forward,
  Reload/Stop, tabs, `Cmd+L`, and select-all-on-focus. Saccade should attach
  its bridge to that path, falling back to a thin official ServoShell fork only
  if WebDriver cannot satisfy safety/native-input/reflex gates.

### P2 - CSS Layout Compatibility

Investigate one layout class at a time:

1. Grid enabled/off baseline.
2. 100% and 50% width in block, flex, and grid containers.
3. `fr` tracks and automatic minimum sizes.
4. `min-width:0` / `overflow` fixes for local app CSS versus genuine engine differences.
5. Media query and container-query behavior.
6. Font metrics and line-height deltas.

Fix policy:

- If Saccade adapter caused it, fix code and add regression.
- If the fixture CSS is invalid or incomplete, fix the fixture and record why.
- If Servo differs from Chrome but actions remain valid, mark yellow and route visual review to Chrome.
- If action coordinates are unsafe, mark red and route the page to Chrome/reference until fixed.

### P3 - Control And Editor Behavior

Expand native input and control coverage:

- Text, number, date, checkbox, radio, textarea, select single/multiple.
- Keyboard editing: selection, backspace/delete, arrows, tab, enter, paste where possible.
- Focus and blur events.
- Contenteditable insert and verification.
- GitHub/Gist editor reduction, because the real dogfood run exposed zero-rect/non-focus editor behavior.

Done when:

- We can explain each control as native-pass, safe-fallback, or engine-limited.
- `form_controls` no longer hides control differences inside one giant yellow verdict; each control class is separately reported.

### P4 - Real-Site Dogfood

Use only safe, non-destructive flows:

- GitHub/Gist draft creation without publishing unless user explicitly clicks final submit.
- Forum/comment draft pages with user-owned publish confirmation.
- Login handoff with Google/GitHub session reuse.
- High-ad/content-heavy pages to test stability and block policy.

Rules:

- Never publish, pay, sign, or submit legally meaningful content without user confirmation.
- Sensitive fields stay human-only; agent receives status, not value.
- Record whether the failure is visual, action-map, auth/session, editor, network, or policy.
- Canvas/WebGL/game pages are now a P1 dogfood blocker. Try Saccade first for evidence, but if Saccade emits GL unsupported/texture warnings, misses gameplay/canvas layers, or becomes too slow to judge, stop it and route to Chrome/reference. Do not change the app's CSS or game code to work around Saccade's canvas/compositor/runtime issue.

Current auth/session state:

- `--profile-dir` lets dogfood browser and browser-session-worker share Saccade-owned cookies/storage across processes.
- `selftest-profile-persistence` proves one worker can write a persistent cookie and a second worker can read it from the same profile.
- This does not import external browser cookies. Real Google/GitHub login should be done inside Saccade, then reused through the same profile dir.
- Current local dogfood wrappers default to stable normal-profile storage at
  `runs/dogfood_profile/default`, so rebuilt kits do not force a new login by
  changing to a fresh per-kit `dist/.../profile/default` directory.
- Product backlog: add visible profile mode UI, `--incognito` / ephemeral
  wrapper support, named profiles, and a user-confirmed clear-profile command.

Browser chrome architecture note:

- Pinned Servo `WebView::resize` resizes every WebView sharing the same `RenderingContext`; a WebView is always as large as its rendering context.
- Therefore a Chrome-like visible toolbar should not be implemented as a second same-context WebView. It needs a native overlay, separate rendering context, or offscreen composition path that preserves the page viewport/action map.

### P5 - Routing And Fallback

Add an explicit page decision:

```text
servo-modern        usable for action/safety/replay
chrome-reference    required for visual/UI-design judgement
chrome-live         future: use Chrome engine for pages Servo cannot operate
unsupported         record and ask for user/adapter work
```

The product should tell the user why it routed:

- "Saccade can act safely here, but Chrome is the visual reference."
- "Servo action map is unsafe for this page; using Chrome/reference mode."
- "This editor exposes zero-size action targets; user focus handoff required."

## Initial Backlog

Canonical active queue: `docs/CURRENT_ACTION_ITEMS.md`.

| ID | Issue | Current Evidence | Next Step |
| --- | --- | --- | --- |
| BP-001 | Narrow `form_controls` window overflows right column and can become action-unsafe | Fixed for the local fixture: after strict local form CSS, 390px `form_controls` has Chrome hit-test `8/8` and max click escape `1.0px` | Keep as regression |
| BP-002 | Native form controls have large rect deltas versus Chrome | Width modes report: auto input/textarea stay about `136.5px` in Saccade while Chrome expands to `302-440px`; `width:100%` makes rect widths match | Use `width:100%` plus `min-width:0` in Saccade-owned forms; route third-party pages by measured action safety |
| BP-003 | Browser shell lacks product-grade browser chrome | Legacy GL toolbar has usable URL text/hit-zones, but official ServoShell source already provides egui browser chrome with address bar, tabs, Back/Forward, Reload/Stop, and WebView resize below toolbar. See `docs/browser_shell_basics_report.md` and `docs/servoshell_adapter_migration_plan.md` | Route product browser UI to official ServoShell; attach Saccade bridge there instead of further polishing legacy GL toolbar |
| BP-004 | GitHub/Gist body editor visible but not focusable/actionable | Local editor reduction passes with route `usable_ignore_hidden_backing_fields`: visible contenteditable and CodeMirror-like shell have positive rects; hidden backing fields produce `zero_rect_count=2`; sensitive textarea is counted without value leakage | Inspect authenticated real Gist again and route if the writable body remains zero-rect |
| BP-005 | MouseAccuracy public demo still needs mainstream visual reference | Chrome/Safari references exist, Firefox missing | Keep Servo evidence separate from Chrome visual proof |
| BP-006 | Font metrics and control text sizing still rough | Manual screenshots after HiDPI fix | Add font/line-height fixture and Chrome/Saccade metrics |
| BP-008 | Large viewport requests can exceed the actual worker window bounds | Width matrix: requested 1600px, Saccade captured 1440 CSS px while Chrome captured 1600 CSS px | Add display-boundary/fullscreen probe before using 1600/1920 as gates |
| BP-009 | Default textarea height causes vertical click drift | Textarea report: default textarea is `54px` in Chrome and `32px` in Saccade at 768/1280; stacked variants produce max click escape `52px`; explicit heights make own rects match | Use explicit local textarea sizing; re-audit after resize; route unsafe third-party pages |
| BP-010 | Independent Saccade workers did not inherit logged-in real-site session | `docs/profile_persistence_report.md`; `cargo run -q -p saccade-shell -- selftest-profile-persistence` proves shared `--profile-dir` cookie persistence across worker processes after fixing WebView shutdown cycle | Use persistent Saccade profile for authenticated real-site dogfood; add friendly profile picker later |
| BP-011 | Canvas/WebGL/GL texture path blocks default dogfood for games/canvas-heavy pages | `docs/webgl_runtime_probe_report.md`; `scripts/probe_webgl_game_runtime.py` routes the live local game as `blocked_missing_gameplay_layer`; page-side Canvas2D backing has foreground-like pixels in red Saccade runs, while the audit screenshot loses them; `present()` before manual readback did not fix the reductions | Park active debugging for now; keep live-game and Canvas2D reductions as red gates, route canvas-heavy judgement to Chrome/reference, and resume later with Servo `WebView::take_screenshot()` comparison |

## Acceptance Order

1. Land this plan and ledger.
2. Implement P1 browser shell basics.
3. Keep the local form CSS workaround from `docs/form_control_width_modes_report.md` as a regression.
4. Add a classifier bucket for cumulative vertical drift from default control sizing.
5. Re-test GitHub/Gist editor and record BP-004 real-site result; local reduction is now in place.
6. Only then resume broad real-site dogfood.
