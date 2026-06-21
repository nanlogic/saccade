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

Then added a reusable ServoShell userscript path:

```bash
--userscripts-dir scripts/userscripts
```

The userscript file is:

```text
scripts/userscripts/github_compat_shim.js
```

`saccade-servoshell bridge` now also accepts:

```bash
--userscripts-dir scripts/userscripts
```

The bridge resolves the directory to an absolute path before launching
ServoShell. The dogfood release script copies these scripts into
`$KIT/userscripts/`; wrappers keep the layer disabled by default and enable it
only when `SACCADE_SERVOSHELL_USERSCRIPTS_DIR` is set.

ServoShell already exposes an official CLI hook:

```text
--userscripts=<your/directory>
```

Source inspection shows ServoShell reads every file in that directory, sorts the
paths, wraps each file as a `UserScript`, and evaluates those scripts from
`components/script/dom/userscripts.rs` when the document `<head>` is bound. This
is not a guaranteed-before-every-inline-script browser extension model, but it is
closer to a preload hook than post-ready WebDriver injection and it applies to
new documents.

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

Official Servo.app userscript run:

```bash
python3 scripts/probe_github_dropdown_geometry.py \
  --servoshell /Applications/Servo.app/Contents/MacOS/servoshell \
  --profile-dir runs/dogfood_profile/default \
  --wait-for-auth-sec 12 \
  --output-dir runs/servoshell_ui/github_dropdown_official_userscript_abs_20260620 \
  --port 7168 \
  --userscripts-dir scripts/userscripts
```

Local API-only userscript gate:

```bash
python3 scripts/probe_github_dropdown_geometry.py \
  --servoshell /Users/waynema/Documents/GitHub/servo-saccade-upstream/target/release/servoshell \
  --url file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/browser_session/index.html \
  --userscripts-dir scripts/userscripts \
  --api-only \
  --output-dir runs/servoshell_ui/userscript_api_only_local_nav_20260621 \
  --port 7170
```

## Result

Artifacts:

```text
runs/servoshell_ui/github_dropdown_source_compat_shim_20260620/report.json
runs/servoshell_ui/github_dropdown_official_compat_shim_20260620/report.json
runs/servoshell_ui/github_dropdown_official_userscript_abs_20260620/report.json
runs/servoshell_bridge/userscript_local_smoke_abs_20260620/report.json
runs/servoshell_ui/userscript_api_only_local_nav_20260621/report.json
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

Official Servo.app userscript result:

```text
classification=auth_required
ready.url=https://gist.github.com/starred
policy.userscripts_dir=/Users/waynema/Documents/GitHub/SACCADE/scripts/userscripts
auth_wait.last.browserApiFeatures.saccadeCompatShim.kind=saccade_github_compat_shim_v0
auth_wait.last.browserApiFeatures.intersectionObserver=function
auth_wait.last.browserApiFeatures.documentPrototypeAdoptedStyleSheets=true
auth_wait.last.browserApiFeatures.shadowRootPrototypeAdoptedStyleSheets=true
stderr_errors for adoptedStyleSheets/IntersectionObserver: none in summary
termination=graceful_servo_shutdown
```

Saccade bridge local userscript launch result:

```text
ok=true
launch.userscripts_dir=/Users/waynema/Documents/GitHub/SACCADE/scripts/userscripts
termination=graceful_servo_shutdown
```

Local API-only userscript gate result:

```text
classification=pass
ok=true
api_probe.browserApiFeatures.saccadeCompatShim.kind=saccade_github_compat_shim_v0
api_probe.browserApiFeatures.saccadeCompatShim.href=file:///Users/waynema/Documents/GitHub/SACCADE/test_pages/browser_session/index.html
api_probe.browserApiFeatures.intersectionObserver=function
api_probe.browserApiFeatures.documentPrototypeAdoptedStyleSheets=true
api_probe.browserApiFeatures.shadowRootPrototypeAdoptedStyleSheets=true
termination=graceful_servo_shutdown
```

One early userscript run used a relative `--userscripts=scripts/userscripts`
path. ServoShell loaded the userscript in `about:blank`, but the target URL did
not navigate. The probe now resolves `--userscripts-dir` to an absolute path,
rejects missing directories because ServoShell's help text asks for a full path,
and explicitly navigates the WebDriver session to the requested URL before
probing.

The shim can work in the current document. In the source-release run, GitHub
redirected to a new login document and the shim disappeared; process stderr
still showed modules failing before or outside the shimmed document. In the
official Servo.app run, the shim stayed installed in the Gist document and
suppressed the missing-API stderr pattern, but the profile button was not
visible because the profile was not logged in for that run. The userscript run
is stronger than the post-ready shim: the marker is present in the target Gist
document and the missing-API stderr pattern is absent, but it still needs a
logged-in geometry run before product promotion.

The API-only gate is the no-human regression test for this layer. It does not
prove GitHub menu geometry; it proves the launch path can install the expected
compat marker and browser APIs in a target document without screenshots or value
reads.

## Conclusion

There is a plausible fix path, but it is not proven as a product fix yet.

Viable routes:

1. **ServoShell userscript preload hook**: inject a controlled compatibility
   script with `--userscripts=<absolute-dir>`. This is already available in
   ServoShell and can be Saccade-owned as a launch-time compatibility layer.
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
pass. If Wayne is available to log in, run the official Servo.app userscript
probe in the same window/profile and compare dropdown geometry.
```

Dogfood command for the logged-in geometry run:

```bash
SACCADE_SERVOSHELL_USERSCRIPTS_DIR=/Users/waynema/Documents/GitHub/SACCADE/scripts/userscripts \
  dist/saccade-dogfood-current/open-saccade https://gist.github.com/starred
```
