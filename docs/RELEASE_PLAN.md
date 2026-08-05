# Developer Preview release plan

## Release target

The first public build is a live semantic Truth Layer for current macOS Chrome
and Edge. A tester installs one signed app, confirms one store Extension,
selects a Profile, opens or shares a tab, and sees one full view followed by
browser-pushed semantic deltas. Default installation and use require no
Accessibility permission.

The preview targets the complete machine inventory: 34 protocol Truth roles,
12 reusable variants, and 6 structural/push boundaries. The 15-family Reference
Actuator catalog is optional development tooling and is not the release Truth
surface. Inventory entries remain `implementation` until one frozen candidate
passes the release gates. Windows follows as a separate candidate.

## Product gates

- Default MCP exposes exactly capabilities, tab list/open, and `truth.read`.
- Capabilities `/5` identify `truth_layer`, push/resources, and
  `execution_owner: agent_client`; default views contain no action authority.
- Full→delta, Profile bans, dynamic replacement, delayed render, same-origin
  iframe, open Shadow DOM, stream gap/reset, and unsolicited Resource updates
  pass in Chrome and Edge from the same commit.
- Each common control has public recognition/state evidence from two independent
  sites; every family covers multiple implementation types where applicable.
- Codex and Claude each act with their own tool in the same managed browser tab,
  and Saccade passively reports the resulting delta. Inability to share the same
  browser instance is reported as incompatible, not hidden by a fallback.
- Canvas/WebGL, closed shadow roots, cross-origin frames, browser-owned dialogs,
  built-in PDF, and semantically weak pages report explicit limitations.
- Clean install, upgrade, Host restart, browser restart, Profile preservation,
  and uninstall pass without Accessibility.
- Signed/notarized macOS app, Chrome Web Store and Edge Add-ons packages,
  checksums, five-minute quickstart, and redacted diagnostics are ready.

## Reference Actuator gate

The source and developer package may retain `reference-actuator-mcp` for
regression, benchmarks, and clients without execution. It is not configured by
default. Its stale/replay, focus, coverage, native/soft, form-fill, reflex, and
receipt tests must remain green; any receipt is labeled
`reference_actuator`. Its Accessibility and input-policy state cannot appear in
default capabilities or installation.

## Candidate evidence

Freeze one commit, Runtime and Extension versions, Chrome and Edge versions,
then run the complete Truth fixture and public-site matrices in clean profiles.
Store initial transfer size, delta size, revisions, observed transitions,
failures, and limitations. Do not record editable contents, file paths,
locators, coordinates, cookies, or browser storage. Publish the full
denominator and never combine best results from different commits.

Playwright may be an isolated semantic comparison lane beginning from the same
unknown URL and task. It is not a Saccade production dependency or fallback.
Reference Actuator benchmarks must be labeled historical/reference and cannot
support claims about default execution.

The required core comparison contains at least a native HTML form, a React
dynamic page, and an Angular or Vue multi-control page. The Saccade lane uses
Truth plus the Agent client's own web-act tool in the same tab; it never uses
the Reference Actuator. The Playwright lane uses official Playwright MCP.
Neither lane receives selectors, control names, page structure, prepared
scripts, or state from the other lane. Retain completion, initial discovery
time, initial bytes and token estimate, delta latency, post-action
re-observation count, stale/replacement recovery, tool calls, total time, and
every failure reason.

## Work order

1. Finish source-diverse public compatibility evidence across Selenium,
   WAI-ARIA APG, Angular Material, an official Vue library, Web Components,
   dynamic replacement, delayed rendering, and frames.
2. Run the three core-product Playwright comparisons above.
3. Complete lifecycle evidence for loading, disappearance, overlays/modals,
   infinite scroll, sortable tables, dialogs, slow resources, upload/download,
   large rearrangements, and viewport changes.
4. Freeze and package the release candidate only after those evidence gates.

## Tester package and launch

- signed DMG, checksums, store links, supported browser versions;
- README quickstart, architecture, Profile example, Truth coverage table;
- public full→delta demo and same-tab external web-act demo;
- honest limitations and GitHub issue templates for install and incorrect truth.

Primary launch is GitHub Release plus Show HN. Follow-up venues may include
Lobsters, relevant open-source/LocalLLaMA communities, DEV, X, LinkedIn, and
Product Hunt after checking their current rules. Launch copy explains the
continuous compiler, compact deltas, same-tab integration requirement, raw
evidence, and known gaps—not arbitrary-site or safety guarantees.

Wayne approves the candidate after reviewing installer, demo, evidence,
limitations, and launch copy. Local development evidence never promotes a
Catalog row to `publishable`.
