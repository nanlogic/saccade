# CEF Day 1 Report

Date: 2026-07-13
Result: PASS with one Day 2 profile-root follow-up

## Built Artifact

- App: `target/cef-release/Saccade.app`
- Architecture: Apple Silicon arm64
- CEF: `150.0.11+gb887805+chromium-150.0.7871.115`
- Chromium: `150.0.7871.115`
- Sandbox: enabled by the official CEF build
- Bundle: main app plus the five required upstream helper apps
- Size: 330 MB staged app, 659 MB including the incremental build tree
- Signing: the official local build's linker signatures only; distribution
  signing and notarization are not done

`engines/cef/cef.lock.json` records the exact CEF and Chromium revisions,
official archive URLs, published SHA-1 values, and measured SHA-256 values.
The build copies the CEF license and Chromium credits into the app bundle.

## Implementation Choice

Day 1 builds the official standard-distribution `cefsimple` target and stages
that tested bundle as `Saccade.app`. The outer bundle name and identifier are
`Saccade` and `ai.saccade.browser`; the executable and helper names remain
upstream values for this gate.

Several custom target/package variants rendered correctly but hung during
`CefShutdown()` after their final helper exited. An outer-only rebrand of the
official target launched and quit cleanly. We therefore kept the official
macOS lifecycle and deferred custom host code to the Day 2 adapter work.

The standard distribution was selected over the minimal archive because it
contains the complete official sample resources and bundle recipe used by the
passing baseline. Both archive identities remain pinned, but only the standard
package is fetched by the Day 1 build script.

## Runtime Evidence

All checks below used the Release app. CDP was enabled explicitly only as a
test probe to read measurements and capture screenshots; neither build nor run
scripts enable remote debugging by default, and CDP is not the production
agent interface.

| Gate | Measured result |
| --- | --- |
| Local fixture | Loaded with `devicePixelRatio=2`; WebGL reported `WebGL 2.0 (OpenGL ES 3.0 Chromium)` |
| Resize | At 800 px viewport the grid measured two 368 px columns; after a 700 px window resize it reflowed to one 652 px column |
| GitHub public | Loaded `https://github.com/` to `readyState=complete`, 5,825 visible text characters, and 146 links; screenshot matched the current public Chromium layout |
| Local game | `Blend or Die - Prototype` ran at `http://127.0.0.1:4173/`; the 1600x1136 backing canvas rendered enemies, drops, HUD, and the player at an 800x568 CSS viewport |
| External WebGL | `https://get.webgl.org/` reported support and visibly rendered the rotating cube |
| Normal profile | A localStorage marker survived a clean quit and relaunch in the same named profile |
| Incognito | It could not see the normal marker; its own marker disappeared after quit; the temporary session directory count returned from 1 to 0 |
| Lifecycle | Final normal and incognito runs each exited in about one second after orderly macOS termination |

Screenshots used for visual inspection were written to `/tmp` and were not
added to repository evidence. The local game server was stopped after the
gate.

## Profile Follow-up

The Day 1 launcher uses an owner-only named `--user-data-dir` for normal mode
and a disposable owner-only directory for incognito. Persistence and deletion
were both measured. The unchanged official sample still logs CEF's warning
that `CefSettings.root_cache_path` uses its default.

Two narrowly scoped attempts to set CEF `cache_path` or only
`root_cache_path` removed the warning but caused shutdown to hang in a macOS
Keychain path. Ad-hoc re-signing the framework, helpers, and staged outer app
also made clean shutdown hang, including with a fresh profile. Those attempts
were removed. Day 2 must set the profile root in
the dedicated Saccade CEF host and pass both persistence and clean-shutdown
gates before concurrent named profiles are claimed. Day 1 supports one active
browser process at a time. Signing remains a Day 5 gate and must use the final
bundle/helper identities rather than mutating this upstream-lifecycle probe.

## Reproduce

```sh
engines/cef/scripts/fetch_macos.sh
engines/cef/scripts/build_macos.sh

engines/cef/scripts/run_macos.sh normal https://example.com
engines/cef/scripts/run_macos.sh incognito https://example.com
```

Build tools used: CMake 4.4.0, Ninja 1.13.2, Xcode 26.3 / AppleClang 17.
