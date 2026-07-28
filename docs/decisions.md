# Architecture decisions

## 2026-07-27 — One final product route

Accepted: Saccade is an open closed-loop control runtime for authenticated
Chrome/Edge tabs. The only production route is Extension → Native Host mode →
owner-only local IPC → MCP mode.

## 2026-07-27 — One runtime executable, separate modes

Accepted: Native Host and MCP share runtime code and one shipped executable but
retain separate process modes, stdin/stdout framing, launch lifecycles, and
protected-data boundaries.

## 2026-07-27 — Control SDK and generated public coverage

Accepted: control families are modular and implement one closed-loop contract.
A machine-readable Catalog becomes the source for the public support matrix,
fixtures, conformance checks, and evidence links. End users do not install the
SDK; reviewed modules are bundled with releases.

## 2026-07-27 — Platform delivery

Accepted: macOS ships a signed/notarized DMG and Windows ships a signed Setup.
They share Extension, protocol, Catalog, modules, and release version while
using platform-specific system input, signing, permissions, and registration.

## 2026-07-27 — Historical engines

Rejected for production: CEF and Servo browser shells. Their algorithms,
fixtures, and evidence may be studied or selectively ported, but they cannot
re-enter the default runtime or create an alternate browser route.

## 2026-07-27 — Coverage and verification

Accepted: common controls require full closed-loop proof; uncommon controls
require truthful basic recognition and explicit limitations. OS input delivery
or a page revision change alone is not a verified semantic action.

## 2026-07-28: Profiles provide behavior and ban named controls

Accepted: a public Profile contains `name`, Agent-facing `behavior`, and a
`ban` list. Each ban entry names a control and may include a text `condition`.
Matching ignores case and whitespace differences. A missing condition bans the
named control. A present condition bans it only when the control's associated
text contains that condition.

Accepted: control modules retain one closed loop and do not read Profile data.
The Native Host filters banned controls before MCP exposure and accepts action
tokens only from its current filtered observation. Human control remains
available. `catalog/profile.schema.json` defines the public JSON.

Accepted: `saccade.observation/1` and `saccade-extension-host/1` remain
unchanged. The Runtime exposes the active Profile name and behavior through
`saccade.system.capabilities` as `saccade.capabilities/4`.
