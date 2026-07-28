# Saccade contributor instructions

Read `docs/FINAL_ARCHITECTURE.md` and
`docs/extension_observation_contract.md` before changing browser, Host,
protocol, MCP, control-module, input, download, or packaging behavior. Read
`docs/PROFILE_ARCHITECTURE.md` before changing Profile loading or filtering.

## One production route

Chrome/Edge Extension → Native Messaging Host mode → owner-only local IPC →
MCP mode. Do not add CEF, Servo, Playwright, CDP, visual-coordinate, or other
fallback execution routes.

## Product invariants

- Keep wire schemas at `saccade.observation/1` and
  `saccade-extension-host/1` until an explicit version decision lands.
- Ship one signed product: a DMG on macOS or Setup on Windows, plus one
  browser-store Extension confirmation.
- Keep Native Host and MCP modes separate in framing, lifecycle, and
  protected-data boundaries even when one executable supplies both.
- Every supported control follows observe → prepare → revalidate → native
  execute → reobserve → verify → receipt or failure.
- Agents never receive locators, arbitrary coordinates, editable values,
  protected values, cookies, or browser storage.
- Control modules request finite native-input primitives and declarative
  verification rules. They cannot execute arbitrary Host code.
- Profile filtering stays outside control modules and cannot weaken their
  closed loop.
- Common controls require current Chrome and Edge proof for the same release
  candidate before the Catalog marks them `publishable`.
- Uncommon controls require truthful recognition and explicit limitations.
- Keep arbitrary Canvas/WebGL opaque unless an approved semantic bridge
  supplies revalidatable objects.

## Migration

Use the private `nanlogic/saccade-legacy` archive only as a reviewed source.
Move one approved component at a time according to
`docs/MIGRATION_MANIFEST.md`, preserve its tests, and record its source commit
and path. Do not copy the old tree or its monolithic classifiers.

## Changes and checks

- Keep the Control Catalog machine-readable and regenerate the public coverage
  table after each Catalog change.
- Treat `docs/PROFILE_ARCHITECTURE.md` as normative. A Profile boundary change
  must update `docs/FINAL_ARCHITECTURE.md`,
  `docs/extension_observation_contract.md`, and `docs/decisions.md` in the same
  review.
- Add one focused fixture and closed-loop test for each control behavior.
- Run the narrowest checks while editing. Run the complete list from
  `README.md` before merging a control family or changing a contract.
- Keep local browser profiles, evidence, credentials, signing material, and
  protected values out of Git.
