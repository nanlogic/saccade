# AI-027 GitHub UI Canary

Date: 2026-07-06
Status: active canary; local overlay reduction complete

## Why This Exists

GitHub looks simple but exercises many product-critical browser surfaces:
authenticated session reuse, SPA hydration, popovers, sticky headers, resize,
CodeMirror, action maps, and user-owned side effects.

This canary separates four classes of problems:

- Saccade product UI bugs that we should fix directly.
- Servo web-compat gaps that need upstream/runtime fixes or narrow shims.
- Dogfood harness mistakes, such as filling the wrong authoring surface.
- Acceptable fallbacks that do not block the human-in-loop draft workflow.

## Runs

Public read-only smoke matrix:

```text
dist/saccade-dogfood-current/run-public-site-smoke-matrix ai027_github_canary_extended --matrix extended --timeout-sec 35
```

Result:

```text
ok=true
report=dist/saccade-dogfood-ai027-loading-ready-20260706/runs/public_site_matrix/ai027_github_canary_extended/report.json
github_servo_repo: ok=true elapsed=5.681s actions=15 same_webview_control=true graceful_shutdown=true
gist_discover: ok=true elapsed=4.163s actions=32 same_webview_control=true graceful_shutdown=true
```

GitHub dropdown geometry with the Saccade userscript shim:

```text
python3 scripts/probe_github_dropdown_geometry.py \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell \
  --profile-dir runs/dogfood_profile/default \
  --url https://gist.github.com/starred \
  --userscripts-dir scripts/userscripts \
  --wait-for-auth-sec 8 \
  --timeout-sec 45 \
  --page-ready-sec 18 \
  --sizes 1200x760,900x700,1200x760 \
  --output-dir runs/ai027_github_ui_canary/dropdown_starred_hit_target_20260706
```

Result:

```text
classification=fail
auth_wait.status=profile_seen
profile click=true
menuWithinViewport=true
horizontalOverflow=0
verticalOverflow=0
signOutHit=false
```

The added hit-target instrumentation shows that the visible Sign out row does
not receive the pointer hit. The center point hits underlying gist-list content:

```text
phase_0_1200x760 hit path:
div.gist-snippet > div.gist-snippet-meta... > ul... > li.d-inline-block

phase_1_900x700 hit path:
div.repository-content.gist-content > div.gist-snippet > ... > div.flex-order-1...
```

That means the current failure is not login, not menu geometry, and not simple
right-edge clipping. It is an overlay hit-test/stacking/compositing issue:
the menu is visible and inside the viewport, but hit testing still targets
content behind it.

API-only comparison:

```text
runs/ai027_github_ui_canary/api_only_no_userscript_port7096_20260706/report.json
runs/ai027_github_ui_canary/api_only_userscript_20260706/report.json
```

Without the userscript shim, GitHub still lacks the required APIs:

```text
intersectionObserver='undefined'
documentPrototypeAdoptedStyleSheets=false
shadowRootPrototypeAdoptedStyleSheets=false
```

With the userscript shim, the API probe passes:

```text
intersectionObserver=function
documentPrototypeAdoptedStyleSheets=true
shadowRootPrototypeAdoptedStyleSheets=true
saccadeCompatShim.kind=saccade_github_compat_shim_v0
```

The earlier `api_only_no_userscript_20260706` run is excluded because it
collided on the WebDriver port while another probe was running.

Local overlay hit-test reduction:

```text
python3 scripts/probe_overlay_hit_test.py \
  --sizes 1200x760,900x700,1200x760 \
  --timeout-sec 30 \
  --output-dir runs/ai027_github_ui_canary/overlay_hit_test_matrix_with_primer_20260706
```

Result:

```text
ok=true
passed=30
failed=0
chrome: passed=15 failed=0
servo: passed=15 failed=0
report=runs/ai027_github_ui_canary/overlay_hit_test_matrix_with_primer_20260706/report.json
```

The local fixture covers a visible account dropdown over ordinary list
content, fixed positioning, absolute positioning, static child menu content,
transformed underlay, and a Primer-like wrapper hierarchy:

```text
action-menu > focus-group > button
Overlay > Overlay-body > action-list > div > ul[role=menu].ActionListWrap
```

In every case, both Chrome and source ServoShell hit `#sign-out`, record one
menu click, and record zero underlay clicks.

GitHub dropdown computed-style follow-up:

```text
runs/ai027_github_ui_canary/dropdown_starred_style_probe_20260706/report.json
```

Result:

```text
classification=fail
signOutHit=false
menuStyle.pointerEvents=auto
menuStyle.visibility=visible
menuStyle.opacity=1
signOutStyle.pointerEvents=auto
signOutStyle.visibility=visible
signOutStyle.opacity=1
```

The hit target still lands on underlying gist content. The computed style says
the menu and Sign out row are visible and pointer-enabled, so this is not a
simple `pointer-events: none`, hidden, transparent, or offscreen condition.

Narrow GitHub account-menu pointer shim:

```text
runs/ai027_github_ui_canary/api_only_account_menu_shim_clean_20260706/report.json
runs/ai027_github_ui_canary/dropdown_starred_account_menu_shim_20260706/report.json
```

Result:

```text
api-only classification=pass
githubAccountMenuPointerShim.kind=saccade_github_account_menu_pointer_shim_v1
dropdown classification=pass
failures=[]
native signOutHit=false
signOutShimHit=true
```

This does not claim native Servo hit-testing is fixed. It proves the userscript
can identify the already-visible account-menu row at the pointer coordinate
when Servo's native `elementFromPoint` still lands on underlying page content.
The shim is scoped to `github.com` / `gist.github.com`, records counters only,
does not read field values, and does not fire the Sign out action during the
probe.

## Current Conclusion

GitHub public read-only pages are usable through the current dogfood bridge.
The Saccade GitHub userscript shim is necessary and effective for the known
`IntersectionObserver` and `adoptedStyleSheets` gaps.

### Live New Issue Observation, 2026-07-11

The logged-in release-browser dogfood flow exposed a separate GitHub surface
that is more important than the account menu. On the GitHub Dashboard, the
visible `Create issue` control opened GitHub Copilot's `/create-issue` command
surface, not a repository Issue form. GitHub displayed `Error sending message`;
Saccade did not send a message or create an Issue.

Direct navigation to the user's real repository New Issue URL reached a page
titled `New Issue`. The human sees an Issue form. The same Saccade truth layer
reported a different, safety-relevant reality:

```text
form controls discovered: 22
eligible generic fields: 0
visible writable editors: 0
visible authoring editors: 0
editor candidates with zero rect: 8
sensitive fields: 0
writes: 0
```

The controls were hidden query-builder, feedback, token, and backing-editor
candidates. The generic planner refused all of them because they were hidden,
unstable, ambiguous, or unsupported. This is a safety pass: filling one by CSS
selector would risk targeting a backing field rather than the editor the person
can see.

Product decision: GitHub New Issue is a P1 Servo compatibility canary. Keep
Servo as the default browser route, but use the explicit Chrome compatibility
route for a measured GitHub issue draft today. The Saccade grant, redaction,
policy, receipts, and replay contract remain the same across that engine route.
Do not represent GitHub New Issue as Servo-native form support until a canary
proves visible title/body editors, safe inventory, no-submit fill, and replay.

Chrome compatibility verification on the same logged-in URL reached `New Issue`
with 112 actions, including a visible `Add a title` input (`958x30`) and visible
`Markdown value` textarea (`958x452`). No values, cookies, storage, or
screenshots were exported. This proves the fallback restores the human-visible
form surface; it does not yet claim generic third-party issue filling through
the compatibility bridge.

The remaining GitHub profile dropdown native hit-test failure is a live GitHub
state or GitHub/Primer integration bug, not a general ServoShell overlay
failure and not a basic Primer-like wrapper failure. The current narrow
userscript workaround can route already-visible account-menu rows by pointer
coordinate. Do not treat it as an auth/session failure. Do not block Gist or
issue draft fill on this account-menu bug, but keep it as a product-quality
canary.

## Next Slice

1. Treat the local overlay/Primer-like hit-test path as green.
2. Treat the GitHub account-menu userscript route as a measured workaround,
   not a native engine fix.
3. Optional human dogfood: manually click a harmless account-menu item, such as
   profile/help, to confirm the event-route feels right. Avoid using Sign out
   as the manual test unless intentionally logging out.
4. Retry the real logged-in GitHub issue/discussion draft gate only through
   Chrome compatibility until the Servo New Issue editor canary becomes green.
