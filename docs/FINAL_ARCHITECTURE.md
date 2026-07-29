# Saccade final architecture

Status: accepted direction, 2026-07-27.

## Product objective

Saccade is an open closed-loop browser control runtime for authenticated tabs.
Agents connect through MCP. Contributors extend browser-control coverage
through a declarative Control Catalog, audited control-family modules,
conformance fixtures, and evidence.

## The single route

```text
Agent
  → MCP mode
  → owner-only local IPC
  → Native Host mode
  → Native Messaging
  → Chrome/Edge Extension
  → agent-owned or explicitly shared tab
```

There is no production CEF/Servo shell and no Playwright, CDP, screenshot,
vision, or page-script fallback. The Runtime executes only strategies compiled
into its Registry. The current v1 Registry contains no Agent-provided
direct-coordinate strategy.

## Product layers

### Runtime

One cross-platform executable supplies separate modes:

```text
saccade-runtime native-host
saccade-runtime mcp
saccade-runtime doctor
saccade-runtime repair
```

The modes share protocol, session, control-registry, verifier, audit, download,
and packaging libraries. They do not share stdin/stdout: Chrome owns the Native
Messaging channel and each Agent owns its MCP channel.

The Runtime validates identity, revision, focus, topmost state, and geometry,
dispatches native input, waits for fresh observations, and records receipts.
It also loads the three-field Profile described in `PROFILE_ARCHITECTURE.md`.
The Profile supplies behavior text to the Agent and bans named controls from
the Agent surface.

### Truth Layer

The Truth Layer is the evidence boundary. It defines semantic objects, runtime
identity, revision, provenance, affordances, target representations,
limitations, disclosure, and receipts. The current v1 schema uses opaque action
tokens and withholds locators, coordinates, editable values, and protected
values. The active Profile may remove named controls from this projection.

### Control SDK

The SDK contains:

- a machine-readable Control Catalog;
- control-family module contracts and registry;
- allowlisted native-input primitives;
- declarative postcondition verifiers;
- fixtures and Chrome/Edge conformance runners;
- evidence and public-matrix generators.

End users do not install the SDK. Initially, new modules enter releases only
through reviewed source contributions. Runtime-downloaded arbitrary code is
out of scope.

## Closed-loop contract

Every actionable control instance follows:

```text
discover
  → observe
  → prepare
  → revalidate
  → execute real OS input
  → reobserve
  → verify a control-specific postcondition
  → receipt | failed | limited
```

An OS-accepted input event is not automatically a successful control action.
For example, checkbox success requires a checked-state transition; link success
may require a document transition or an agent-owned new tab. If the semantic
postcondition cannot be proved, the receipt says delivered/unverified rather
than successful.

## Control-module boundary

Each cataloged control declares its safe state, affordances, implementation
family, native primitive, verifier, limitations, fixtures, and browser evidence.
Similar controls share code. Agents continue to use generic observe/act tools;
the Registry dispatches the correct module from the opaque token.

Modules cannot send arbitrary code to the Host. The Host exposes a finite set of
native primitives such as pointer movement, allowed buttons, wheel, allowlisted
key chords, Unicode text, selection, and bounded drag. The current v1 route uses
opaque action tokens.

Modules own semantic interpretation and the control-specific closed loop. They
execute through registered primitives and report what occurred. Profile
filtering happens outside the modules and cannot change their native action or
verification logic.

The Profile contains `name`, Agent-facing `behavior`, and a `ban` list. The
Runtime filters banned controls before MCP exposure and rejects action tokens
that are absent from the filtered current observation. The current
`saccade.observation/1` and `saccade-extension-host/1` meanings remain unchanged.

## Coverage target

- Common controls: full semantic observation, registered action,
  control-specific verification, fail-closed and redaction evidence, and
  current Chrome/Edge proof.
- Uncommon controls: at least truthful recognition, safe state/bounds, and an
  explicit limitation for every unverified action or semantic.
- Outside the 95% core: browser/OS chrome, arbitrary closed-shadow internals,
  PDF form internals, and arbitrary Canvas/WebGL/custom widgets without audited
  semantics remain opaque or restricted.

Canvas/WebGL support prioritizes application-provided semantic bridges. Pixel
or visual detectors, if later approved, produce short-lived candidates with
explicit provenance; a changed image is not a semantic success receipt.

## Platform delivery

- macOS: signed and notarized DMG containing `Saccade.app`; CoreGraphics input;
  one Accessibility confirmation; automatic Native Messaging and MCP repair.
- Windows: signed Setup; `SendInput`; automatic Native Messaging registration,
  MCP configuration, diagnostics, and repair.
- Browser: one shared Extension source; store identifiers may differ between
  Chrome Web Store and Edge Add-ons.

Both platform assets, the Extension, Catalog, public matrix, source commit,
SBOM, SHA-256 values, and machine-readable release manifest share one release
version.

## First proof slice

The first Catalog + Registry + Runtime slice contains button, text field,
checkbox, and select. The SDK v1 module contract froze after all four passed
stale, permission, focus, topmost, redaction, native-input, postcondition,
receipt, Chrome, and Edge development gates on the same source candidate.
Catalog publication still requires signed-product and release evidence.

The next Catalog extension adds search field, textarea, contenteditable, and
spin button. These modules reuse the finite Unicode-text primitive and
`has_value` verifier but keep role-specific safe projections. Paired managed
Chrome and Edge development run `20260729T043308Z` verified all eight current
actionable controls. This extension changes neither Profile fields nor the two
v1 wire schemas; its Catalog rows remain `implementation` pending release
evidence.

## Non-goals

- A second embedded browser product.
- One MCP tool per control.
- Arbitrary downloaded Host modules.
- Silent or unregistered execution strategies.
- Restoring retired engines to the default workspace.
- Calling a revision change alone a verified semantic action.
