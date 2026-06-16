# Saccade Browser Compatibility Ledger

Date: 2026-06-16

This ledger records user-visible browser differences that matter for dogfood or agent reliability.

Status values:

- `open`: measured enough to track, not fixed.
- `investigating`: active reduction or source inspection.
- `fixed`: Saccade code/fixture changed and verified.
- `engine-limited`: likely Servo/web-engine gap; use routing/fallback.
- `page-css`: page or fixture CSS is the source; record, do not patch engine.
- `routed`: not fixed, but product has a clear fallback route.

Severity:

- `P0`: unsafe action map, sensitive-policy risk, or unusable primary workflow.
- `P1`: daily dogfood pain or serious visual/layout mismatch.
- `P2`: visual polish or public-demo clarity.

## Entries

| ID | Severity | Status | Area | Symptom | Evidence | Next Step |
| --- | --- | --- | --- | --- | --- | --- |
| BP-001 | P0 | fixed | CSS Grid / responsive | Narrow `form_controls` used to become action-unsafe; controls and footer shifted/clipped differently from Chrome. | `docs/form_control_width_modes_report.md`; after local CSS workaround, `390px` `form_controls` is no longer action-map red: Chrome hit-test `8/8`, max click escape `1.0px`. | Keep as regression; remaining visual/layout drift is tracked under BP-002/BP-009. |
| BP-002 | P1 | investigating | Native controls | Servo form controls often keep intrinsic widths while Chrome expands controls to fill grid columns. | `docs/form_control_width_modes_report.md`: auto input/textarea stay about `136.5px` wide in Saccade while Chrome expands to `302-440px`; `width:100%` makes rect widths match across grid/flex/block. | Use `width:100%` and `min-width:0` for Saccade-owned forms; keep third-party pages routed until measured. |
| BP-003 | P1 | routed | Browser shell | The legacy dogfood `saccade-shell` GL toolbar now has visible URL text, hit-zones, selection/caret state, and in-place URL editing, but it should not be the product browser UI. Official ServoShell already has an egui toolbar with Back/Forward/Reload/Stop location input, `Cmd+L`, select-all, tabs, and WebView resizing below chrome. | `docs/browser_shell_basics_report.md`; `docs/CURRENT_ACTION_ITEMS.md`; `f84d157`; official source `ports/servoshell/desktop/gui.rs`, `ports/servoshell/window.rs`, and `ports/servoshell/parser.rs`; `cargo test -p saccade_browser`; `cargo check -p saccade_browser -p saccade-shell`. | Stop hand-polishing legacy GL chrome. Use official ServoShell UI as the human browser layer and attach Saccade bridge there; keep legacy shell only as adapter/proof fallback. |
| BP-004 | P0 | investigating | Editors | GitHub/Gist editor looked visible but exposed zero-size/non-actionable body editor to Saccade. Local reduction now distinguishes visible authoring editors from hidden/zero-rect backing fields and from non-authoring search/login controls without leaking editor values. | Manual dogfood; `docs/editor_reduction_report.md`; `docs/gist_editor_probe_report.md`; `selftest-editor-reduction` -> `editors=6 zero_rect=2 sensitive=1 route=usable_ignore_hidden_backing_fields`; shared-profile `https://gist.github.com/new` probe -> `route_login_or_non_authoring_page`, search box only. | Wayne logs in to Gist inside Saccade with `runs/dogfood_profile/default`; rerun `inspect-editors`; route if the only writable body target remains zero-rect. |
| BP-005 | P2 | routed | Public visual proof | Servo browser chrome/page rendering does not look identical to Chrome/Safari on `mouseaccuracy.com`. | Demo parity requirements; Chrome/Safari references exist. | Use Chrome/Safari/Firefox references for public visual proof; keep Servo as action/replay evidence. |
| BP-006 | P2 | open | Fonts / sizing | After HiDPI fix, font scale is much better but native controls and button text still look rough. | Manual screenshots of `form_controls`. | Add font metrics fixture and compare computed font/line-height/text rects. |
| BP-007 | P1 | fixed | Viewport / HiDPI / resize | Window resize used to resize the surface without updating JS/layout viewport; Retina scale made text too small. | `960c66d`; `1280x759 -> 1360x759 -> 1000x668` runtime resize verification. | Keep as regression in productization suite. |
| BP-008 | P1 | open | Window bounds / large viewport | Requested large worker widths can be capped by the current macOS window/display session, making 1600/1920 comparisons invalid. | Width matrix requested `1600x700`; Saccade raw screenshot was `2880x1400`, equivalent to `1440x700 @2x`, while Chrome was `1600x700`. | Add a display-boundary/fullscreen probe and clamp/report invalid widths before using large-width benchmarks. |
| BP-009 | P0 | routed | Native textarea / vertical layout | Default textarea height differs from Chrome and causes cumulative vertical click drift in normal flow. | `docs/textarea_default_height_report.md`: default textarea is `54px` in Chrome and `32px` in Saccade at 768/1280; stacked variants produce max click escape `52px`; explicit heights make own rect heights match but cannot undo prior flow drift. | Use explicit textarea sizing for Saccade-owned pages; after resize, re-audit instead of refreshing; route third-party pages when Chrome/live hit-test shows unsafe drift. |
| BP-010 | P1 | fixed | Auth / profile persistence | Independent workers did not inherit real-site login state; real Gist `/new` probe saw only unauthenticated search UI. | `docs/profile_persistence_report.md`; `cargo run -q -p saccade-shell -- selftest-profile-persistence` -> `cookie_status=present`; worker shutdown now breaks the WebView delegate cycle so Servo writes `cookie_jar.json`. | Use `--profile-dir` for Saccade-owned login handoff; add friendly profile picker and retest authenticated Gist. |
| BP-011 | P1 | investigating | Canvas / WebGL / GL runtime | Saccade's embedded Servo path can emit GL texture warnings and miss canvas/gameplay layers, but the downloaded official Servo.app can run the local game at `http://127.0.0.1:4173/` on the same machine. That points away from a hard Servo-engine impossibility and toward Saccade embedder/config/screenshot-readback/version divergence. | `docs/webgl_runtime_probe_report.md`; `docs/servoshell_source_strategy.md`; previous Saccade runs routed `blocked_missing_gameplay_layer`; Wayne manual check on 2026-06-14: official Servo.app runs the local game; installed Servo.app is ServoShell `0.3.0` while Saccade currently pins crates.io `servo=0.2.0`; `ign.com` also fails in official Servo.app, so IGN is not a Saccade-specific target. | Use official ServoShell source as the browser-productization reference. First try an external agent bridge via `servoshell --webdriver/--devtools`; if that works, fork official ServoShell or upgrade Saccade to a matching git/source Servo. |
| BP-012 | P0 | fixed | Manual pointer input / HiDPI | User-visible clicks can miss or appear inconsistent even when agent `act` can click the same page. | `docs/pointer_input_diagnostic_report.md`; `docs/pointer_input_official_research.md`; `runs/pointer_trace/trace_cgevent_1781475144/summary.json` shows a real click near CSS `(220,245)` arriving as winit raw physical `(440,486)`, while Saccade used to send stored page `(440,486)` to Servo under `hidpi=2.0`. After the fix, CGEvent dogfood trace showed `raw_physical=(330,494)` stored as `stored_page=(165,247)` before `MouseInput`. | Re-run live third-party dogfood clicks with `SACCADE_TRACE_POINTER=1`; if misses remain, split a new stale-cursor AppKit fallback entry because winit `MouseInput` has no position payload. |
| BP-013 | P1 | open | macOS packaging / signing | Downloaded or packaged macOS app can fail to open because signing/notarization is not productized. | Wayne report during dogfood. | Add a packaging/signing checklist and a local unsigned/dev-signed path; do not mix with renderer/input fixes. |
| BP-014 | P2 | routed | Real-site upstream Servo limit | `ign.com` can be slow/unresponsive in Saccade, but the downloaded official Servo.app has the same issue. | Wayne manual check on 2026-06-14; Saccade worker/devmax experiments also timed out before useful probe output. | Do not productize around IGN now. Use it only as an upstream Servo limitation note, not a Saccade release blocker. |
| BP-015 | P1 | investigating | Real-site overlay / performance | `dealmoon.com` is slow in Saccade and appears to show or rely on an overlay that does not display correctly. | Wayne manual dogfood report on 2026-06-16. No official ServoShell comparison captured yet. | First compare downloaded official Servo.app/official ServoShell with legacy `saccade-shell`. If official also fails, route as upstream/site compatibility; if official works, prioritize the ServoShell UI/adapter migration instead of patching the legacy embedder. |

## Entry Template

```text
ID:
Severity:
Status:
Area:
URL/fixture:
Chrome expected:
Saccade observed:
Action-map impact:
Sensitive-policy impact:
Likely source: Saccade adapter | Servo engine | page CSS | unknown
Artifacts:
Next step:
Decision:
```
