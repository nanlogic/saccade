# AI-027 GitHub UI Canary

Date: 2026-07-06
Status: active canary; first measurement complete

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

## Current Conclusion

GitHub public read-only pages are usable through the current dogfood bridge.
The Saccade GitHub userscript shim is necessary and effective for the known
`IntersectionObserver` and `adoptedStyleSheets` gaps.

The remaining GitHub profile dropdown failure is a separate overlay hit-test
bug. Do not treat it as an auth/session failure. Do not block Gist or issue
draft fill on this account-menu bug, but keep it as a product-quality canary.

## Next Slice

1. Build a local overlay hit-test reduction:
   a visible menu layered over normal content, with `elementFromPoint` expected
   to return the menu item at multiple widths.
2. Run the reduction in Chrome and source ServoShell.
3. If the reduction fails only in ServoShell, decide between:
   a narrow GitHub userscript workaround for account-menu pointer events, or
   source-fork hit-test investigation.
4. After the overlay canary is classified, retry the real logged-in GitHub
   issue/discussion draft gate from AI-026.
