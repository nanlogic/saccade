# GitHub Dropdown Compatibility Shim Probe

Date: 2026-06-20
Status: diagnostic

## Question

Can Saccade fix the GitHub/Gist profile dropdown overflow seen in official
ServoShell without waiting for upstream Servo engine support?

## Experiment

Added a diagnostic-only switch to `scripts/probe_github_dropdown_geometry.py`:

```bash
--compat-shim
```

The shim is injected after page readiness through WebDriver `execute/sync`. It
adds minimal JS implementations for:

- `window.IntersectionObserver`
- `Document.prototype.adoptedStyleSheets`
- `ShadowRoot.prototype.adoptedStyleSheets`
- `CSSStyleSheet.prototype.replace/replaceSync` when absent

Source-release run:

```bash
python3 scripts/probe_github_dropdown_geometry.py \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell \
  --profile-dir runs/dogfood_profile/default \
  --wait-for-auth-sec 12 \
  --output-dir runs/servoshell_ui/github_dropdown_source_compat_shim_20260620 \
  --port 7165 \
  --compat-shim
```

Official Servo.app run:

```bash
python3 scripts/probe_github_dropdown_geometry.py \
  --servoshell /Applications/Servo.app/Contents/MacOS/servoshell \
  --profile-dir runs/dogfood_profile/default \
  --wait-for-auth-sec 12 \
  --output-dir runs/servoshell_ui/github_dropdown_official_compat_shim_20260620 \
  --port 7166 \
  --compat-shim
```

## Result

Artifacts:

```text
runs/servoshell_ui/github_dropdown_source_compat_shim_20260620/report.json
runs/servoshell_ui/github_dropdown_official_compat_shim_20260620/report.json
```

Source-release result:

```text
classification=auth_required
initial compat_shim.features.intersectionObserver=function
initial compat_shim.features.documentPrototypeAdoptedStyleSheets=true
initial compat_shim.features.shadowRootPrototypeAdoptedStyleSheets=true
auth_wait.last.url=https://github.com/login
auth_wait.last.browserApiFeatures.intersectionObserver=undefined
auth_wait.last.browserApiFeatures.documentPrototypeAdoptedStyleSheets=false
```

Official Servo.app result:

```text
classification=auth_required
initial compat_shim.features.intersectionObserver=function
initial compat_shim.features.documentPrototypeAdoptedStyleSheets=true
auth_wait.last.url=https://gist.github.com/starred
auth_wait.last.browserApiFeatures.intersectionObserver=function
auth_wait.last.browserApiFeatures.documentPrototypeAdoptedStyleSheets=true
stderr_errors for adoptedStyleSheets/IntersectionObserver: none in summary
```

The shim can work in the current document. In the source-release run, GitHub
redirected to a new login document and the shim disappeared; process stderr
still showed modules failing before or outside the shimmed document. In the
official Servo.app run, the shim stayed installed in the Gist document and
suppressed the missing-API stderr pattern, but the profile button was not
visible because the profile was not logged in for that run.

## Conclusion

There is a plausible fix path, but it is not proven as a product fix yet.

Viable routes:

1. **ServoShell preload hook**: inject a controlled compatibility script before
   every document/module evaluation. This could be Saccade-owned if the source
   ServoShell fork exposes a safe preload mechanism.
2. **Servo upstream/API work**: implement or upstream enough
   `IntersectionObserver` / constructable stylesheet / adopted stylesheet
   support for GitHub/Primer.
3. **Site-specific post-load patch**: possibly useful for a same-document
   official Servo.app session, but not reliable across redirects or modules
   that failed before injection. It requires one logged-in same-window test
   before promotion.

Product decision for now:

```text
Keep GitHub account/profile dropdown parity routed until a logged-in shim run
proves geometry. Keep same-session GitHub editor/draft flows, which already
pass. If Wayne is available to log in, run the official Servo.app shim probe in
the same window/profile and compare dropdown geometry.
```
