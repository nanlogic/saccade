# Test-first public compatibility progress

Date: 2026-08-03
Status: engineering evidence; Playwright comparison intentionally blocked

## Candidate and local gate

- Git HEAD: `369522454e6e5073e032fb0af7e56cb3204db13b`
- Tracked patch SHA-256 before this run:
  `b48c9dd088adf7ed210e5ec01314362e6b67a1c091f3ffcd75e25a7f9af84e75`
- Worktree-state SHA-256 before this run:
  `64968fb5793388481eab8be20d8133ade5234d71815adf7ee6cf0adc099b6d1e`
- Chrome and Edge Truth evidence root: `20260803T011502Z`

Rust workspace tests, formatting, clippy, 18 Extension tests, 19 Python tests,
and the architecture gate passed. The owner-only IPC test passed outside the
sandbox. The same source candidate passed the complete Chrome and Edge 34-role,
12-variant, and 6-boundary Truth/pushed-delta gate.

## Additional public passive evidence

These checks used only Saccade in the managed browsers and took no page action.
They prove initial or delayed projection, not an action-caused state delta.

| Official source | Chrome | Edge | Result |
| --- | --- | --- | --- |
| PrimeVue Select | 27 selects | 27 selects | initial Vue projection passed |
| Shoelace Select | 18 selects appeared after component upgrade | 18 selects appeared after component upgrade | open Web Components delayed projection passed |
| W3C APG grouped listbox | 1 select, 11 options | 1 select, 11 options | ARIA listbox projection passed |
| W3C APG autocomplete combobox | select with bounded expanded/value state | same | ARIA combobox projection passed |
| W3C APG modal dialog | closed-page trigger visible | closed-page trigger visible | opening/disappearance delta still requires client action |

Raw redacted development evidence is under
`/private/tmp/saccade-public-20260803`. It is intentionally not committed as
release evidence because the candidate is not frozen and no external action was
taken.

## Same-tab execution result

The dedicated ChatGPT Chrome extension was absent, but Codex Computer Use's
already-authorized `Any App` route successfully targeted the exact Saccade
Chrome for Testing application. On Selenium's official form, Codex clicked the
unchecked `Default checkbox`; Saccade independently pushed revision 2 with one
`updated` object: role `checkbox`, name `Default checkbox`, and
`checked: true`. Saccade Runtime requested no Accessibility permission and the
Reference Actuator was not used.

The fair runner now imports a retained `saccade-client-native-lane/1` trajectory
that proves Chrome, the Saccade browser instance, tab identity, task, and lane
order. It no longer configures or accepts an external web-act MCP. Without that
trajectory it blocks both lanes and does not run Playwright alone.

The Codex same-instance Truth → native action → pushed delta preflight therefore
passes without an extra browser extension. Claude is deferred by product
decision and is not part of this engineering comparison round. Claude Desktop
was inspected and reported `For your security, sign in again to keep using
Claude`; no credentials were requested or handled by Saccade or Codex.

## Cross-document pushed Truth fix

The first retained Selenium task exposed a generic wait bug: after form submit,
the new document restarted its revision at 1, so a wait based only on the old
document's revision 15 could time out even though the Extension had pushed the
new page. The MCP adapter now privately binds `after_revision` to the document
identity in its current Agent view. The Host wait returns when either that
document changes or the revision increases. Public MCP inputs and both wire
schemas remain unchanged.

A focused regression covers revision 15 on `document-1` transitioning to
revision 1 on `document-2`. The real Extension → Host → Runtime → MCP path was
then retested with Codex Computer Use submitting Selenium's official form. It
delivered intermediate same-document deltas followed by a new-document full
Truth at revision 1 containing heading `Form submitted` and paragraph
`Received!`. Retained raw evidence:
`/private/tmp/saccade-navigation-reset-fixed.json`.

## Angular public-site drift

Angular Material remained intermittent. In one earlier run the official page
rendered its examples and Saccade projected selects/options. In the latest
Chrome run both Saccade and Chrome's independent accessibility tree contained
only the documentation shell; the component examples did not render. Because
the browser itself lacked the target content, this run is classified
`public-site drift/intermittent render`, not a Collector defect. A speculative
hydration timer change was tested and removed because it did not affect the
browser-side failure.

## Public action and lifecycle evidence

Codex Computer Use drove the exact Saccade Chrome tab while Saccade remained
observation-only. Retained evidence under
`/private/tmp/saccade-public-20260803` proves:

| Public implementation | Result |
| --- | --- |
| W3C APG radio | `Deep dish` updated to `checked:true` |
| W3C APG switch | `Notifications` updated to `checked:true` |
| W3C APG tabs | `Carl Andersen` updated to `selected:true` |
| W3C APG menubar | `About` updated to `expanded:true`; child menu items appeared |
| W3C APG modal dialog | dialog fields/buttons appeared on open and disappeared on close |
| PrimeVue Select | overlay options appeared; `English` selected; closed combobox retained `has_value:true` |

The APG menu also exposed a Codex Computer Use limitation: its Accessibility
menu remained the active client tree after the browser switched tabs. A clean
browser restart recovered it. This is classified `client same-tab
incompatibility`, not a Collector failure.

## PrimeVue detached-overlay fix

PrimeVue exposed a generic state-projection defect. When its portal listbox was
removed after selection, the same stable select object incorrectly regressed
from `has_value:true` to `has_value:false`. The Collector now retains only the
boolean fact that a choice element had an explicitly selected option. It clears
that fact when a subsequently observed option set explicitly contains no
selection. No selected label or editable value is retained.

A local fixture removes a selected combobox popup after first observation. The
Chrome and Edge semantic gates require its `has_value` state to remain true.
The repaired candidate passed the complete dual-browser gate at evidence root
`20260803T172317Z`, and the real PrimeVue trace no longer contains the false
state regression.

## Redirect authorization race fix

Repeated `loading` events during `primevue.org` → `primevue.dev` navigation
could clear a tab session while an authorization promise for the same final URL
was still running. Later authorization calls reused that promise but did not
recreate the deleted session, so otherwise valid Collector observations were
rejected and `tabs.open` timed out.

Collector configuration now acknowledges before scheduling its first full
compile, and same-URL authorization rechecks session existence after the
in-flight promise settles. If navigation cleared the session, authorization is
rerun for the current URL. The temporary longer open timeout was removed;
Host/MCP retain their original 20/25 second bounds. Three consecutive public
redirect runs produced first Truth successfully. The final same-candidate
Chrome and Edge inventory gate passed at evidence root `20260803T173846Z`.
