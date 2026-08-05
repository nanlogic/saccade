# Truth-first baseline and public passive observation

Date: 2026-08-02
Status: local engineering evidence; not publication evidence

## Candidate

- Git HEAD: `369522454e6e5073e032fb0af7e56cb3204db13b`
- Pre-test tracked patch SHA-256: `593340d240e096c60865a9c5836b04fbcca9dafb2434e05d37b1b36bc3d73c7b`
- Pre-test worktree-state SHA-256: `035d70106b4a7d447ebc180b4c1632816bea88318408b21255fbb2a18a5c4a03`
- Chrome for Testing: `151.0.7922.47`
- Microsoft Edge: `151.0.4129.59`

The worktree was intentionally not committed or cleaned. Test-harness and
documentation changes made after the fingerprint do not alter the Runtime or
Extension candidate exercised by the browser gate.

## Local result

- Architecture check, Python focused tests, Extension tests, and Rust workspace
  tests passed. The owner-only IPC test required execution outside the sandbox
  and then passed.
- `./scripts/dev.sh test all` passed the same local Truth candidate in Chrome
  and Edge. Both browsers produced pushed-delta, Resource notification,
  15-control projection, and semantic/variant/structure evidence under local
  evidence root `20260802T231022Z`.
- Optional Reference Actuator Chrome and Edge regressions both stopped on
  text-field dispatch with `permission_required`, despite each repair preflight
  reporting Accessibility trusted. This is a shared local actuator permission
  blocker, not a Truth projection failure.

The optional dispatch path was tightened to request and re-preflight permission
inside the actual browser-launched Host after an explicit reference action. Its
stale-token recovery taxonomy was also aligned with Host/MCP errors. macOS still
denied native dispatch, so the actuator remains truthfully blocked rather than
bypassing TCC.

After those isolated changes, the complete Truth-only gate passed again in
Chrome and Edge under evidence root `20260802T232858Z`, confirming that default
startup and the 34/12/6 product route were unaffected.

## Public passive result

These runs used only Saccade Truth in the managed browser. They prove initial
projection and passive delayed-render behavior, not an externally caused
post-action delta.

| Source | Chrome | Edge | Result |
| --- | --- | --- | --- |
| Selenium official form | 22 initial objects | 22 initial objects | expected native controls recognized |
| WAI-ARIA APG radio | 6 radios | 6 radios | initial recognition passed |
| WAI-ARIA APG switch | 1 switch | 1 switch | initial recognition passed |
| WAI-ARIA APG tabs | 4 tabs | 4 tabs | initial recognition passed |
| WAI-ARIA APG menu | 4 menu items | 4 menu items | initial recognition passed |
| DemoQA React form | 19 objects appeared after delayed render | 19 objects appeared after delayed render | delayed projection recovered |
| Angular Material select | component deltas appeared after shell load and reset | example-anchor run produced 4 selects and 4 options in pushed revision 10 | viewport lazy-render diagnosed; Collector projection passed |

Angular Chrome eventually produced one 76-change delta containing 21 `select`
and 29 `option` objects, followed by another 22-change delta. Edge initially
produced the site shell and multiple empty revisions/full resets. A second
Saccade-only run navigated to the same official page's `#select-overview`
fragment, which moved the native viewport without a selector or script. Edge
then pushed revision 10 with 29 changes, including 4 `select` and 4 `option`
objects. This identifies viewport lazy-render as the cause; the remaining fair
task requirement is a general same-tab executor with `scroll`, not an Angular
Collector or site-specific fix.

## Comparison status

The fair runner now returns `BLOCKED` with
`same_tab_executor_unavailable` when no explicit same-tab web-act MCP is
configured, and it does not run Playwright alone. The current environment has
no such executor, so no new Saccade/Playwright performance comparison was run
and no speed claim is supported.

The 63-row generated public denominator keeps every role, variant, boundary,
and lifecycle scenario visible. Because the historical mainstream/uncommon
document was unavailable, `catalog/control_denominator_sources.json` now
provides the replacement classification from WHATWG HTML forms, WAI-ARIA role
definitions, and the W3C ARIA APG. A recovered historical document can be
merged later as another source without removing or hiding rows.
