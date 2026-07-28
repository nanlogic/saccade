# Saccade Control Runtime contributor instructions

Read `docs/FINAL_ARCHITECTURE.md`, `docs/PROFILE_ARCHITECTURE.md`, and
`docs/extension_observation_contract.md` before changing browser, Host,
protocol, MCP, Profile, control-module, input, download, or packaging behavior.

## One production route

Chrome/Edge Extension → Native Messaging Host mode → owner-only local IPC →
MCP mode. Do not add CEF, Servo, Playwright, CDP, or unregistered fallback
routes. Profile filtering must remain inside this route.

## Product invariants

- Existing wire schemas remain `saccade.observation/1` and
  `saccade-extension-host/1` until an explicit version decision is recorded.
- Ordinary users install one signed product: DMG on macOS or Setup on Windows,
  plus one browser-store extension confirmation.
- The Runtime may share one executable, but Native Messaging and MCP retain
  separate modes, framing, lifecycles, and protected-data boundaries.
- Every supported control follows observe → prepare → revalidate → native
  execute → reobserve → verify → receipt/failure.
- The current v1 schemas never send Agents locators, arbitrary coordinates,
  editable values, protected values, cookies, or storage. Do not change that
  behavior without an explicit version decision.
- Control modules may request only allowlisted native-input primitives and
  declarative verification rules. They cannot execute arbitrary Host code.
- Profiles contain only `name`, Agent-facing `behavior`, and control `ban`
  entries. Profile filtering must not alter a control module or its closed loop.
- Common controls require full closed-loop Chrome and Edge evidence. Uncommon
  controls require truthful basic recognition and explicit limitations.
- Arbitrary Canvas/WebGL remains opaque unless an audited semantic bridge or a
  separately approved detector capability supplies revalidatable objects.

## Migration rule

Do not copy the old repository wholesale. Move one approved component at a
time according to `docs/MIGRATION_MANIFEST.md`, preserve its tests, and record
the source commit/path. CEF and Servo remain historical research, not runtime
dependencies.

## Change discipline

- Treat `docs/PROFILE_ARCHITECTURE.md` as a public normative design document.
  `catalog/profile.schema.json` is its machine-readable schema.
  A change to its boundary must update `docs/FINAL_ARCHITECTURE.md`,
  `docs/extension_observation_contract.md`, and `docs/decisions.md` in the same
  review and keep the architecture gate green. A worker may draft the change;
  the supervising agent owns the final cross-document decision and diff review.
- Keep the Control Catalog machine-readable and generate the public coverage
  matrix from it once the generator exists.
- Do not label a control `Publishable` without current Chrome and Edge
  artifacts for the same route and release candidate.
- Run the narrowest relevant checks after each migration. Add the full required
  check list when the Rust/Extension skeleton is introduced.
