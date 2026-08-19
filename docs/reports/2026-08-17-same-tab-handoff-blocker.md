# Same-tab handoff blocker: `claude -p --chrome` cannot adopt a foreign tab

Date: 2026-08-17. Candidate `0.3.22`
(`c34eb1214f470328dde1758c9e9367e6dc6e9f7a9ad142027a6b6af69dd66c7f`), live
identity equal to the expected identity. `execution_owner: agent_client`,
`reference_actuator_active: false`. No Extension change was made.

## Verdict

The same-model fair benchmark's Saccade lane fails for a reason outside Saccade.
Claude in Chrome, when driven from a `claude -p --chrome` subprocess, can only
act on tabs it created itself through `tabs_create_mcp`. It cannot adopt a tab
another process opened, **with or without an MCP tab group**. Saccade Truth, the
Saccade MCP route, `tab_id` propagation and candidate identity are all healthy.

The benchmark results from `20260817-same-model` remain **INVALID**. No
performance or superiority claim is authorized.

## What the failing lane actually shows

Raw trace:
`~/Library/Application Support/Saccade Dev/evidence/20260817-same-model/angular_material_select-saccade-first/saccade.jsonl`
(retained unmodified).

| Step | Observation |
| --- | --- |
| `saccade.tabs.open` | `{"observation_ready":true,"opened":true,"tab_id":"1680322987"}` |
| `saccade.truth.read` | index and region views returned normally |
| `claude-in-chrome computer` | called with the identical `tabId: 1680322987` |
| result | `Couldn't determine which page this action targets.` |
| `tabs_context_mcp` | `{"availableTabs":[{"tabId":1680322986,...,"url":"chrome://newtab/"}],"tabGroupId":1378097960}` |
| retry on `1680322984` | same error |

So the correct `tab_id` was produced by Saccade and delivered to Claude
in Chrome unchanged. The refusal happens entirely inside Claude in Chrome.

## Hypotheses tested

### H1 — the tab must exist before the subprocess starts ("pre-open"). Disproven.

`scripts/run_claude_same_tab.py` now opens the tab through the ordinary Saccade
MCP stdio protocol *before* launching `claude`, and names that exact `tab_id` in
the prompt. Evidence:
`~/Library/Application Support/Saccade Dev/evidence/20260817-preopen-probe-1.json`

Saccade tab `1680323000` was open and active before the subprocess started. The
subprocess still reported:

```text
tabGroupId 1378097960, availableTabs [1680322986, 1680322991]
computer(tabId 1680323000) -> Couldn't determine which page this action targets.
```

The MCP tab group **persists between runs** — it is the same group id
`1378097960` seen in the morning's failing benchmark — and creating it never
adopts a pre-existing active tab. Ordering is therefore irrelevant.

A separate interactive check confirmed the same thing directly: with no group
present, opening a Saccade tab and then calling `tabs_context_mcp` with
`createIfEmpty: true` produced a brand-new `chrome://newtab/` and left the
Saccade tab outside.

### H2 — the tab group is the gate, so removing it should help. Disproven.

Evidence:
`~/Library/Application Support/Saccade Dev/evidence/20260817-preopen-probe-2.json`

The subprocess closed every tab in its own group. `tabs_close_mcp` then reported:

```text
No MCP tab group exists. Nothing to close.
```

With **no group at all**, every page call on the Saccade tab still failed with
`Couldn't determine which page this action targets.` — across `computer`, `find`
and `read_page`, and after `select_browser`. Group membership is not the gate.

### Consequence for the proposed Extension fix

The suggested repair — have `saccade.tabs.open` inherit Claude's current
`tabGroupId` — **would not work**, because H2 shows an ungrouped Saccade tab is
refused just the same. It was therefore not attempted, which is the right
outcome on the merits as well:

- `extension/manifest.json` grants `["tabs","nativeMessaging","storage","alarms"]`
  only, with no tab-group capability.
- Saccade has no non-heuristic way to identify "Claude's group". Recognizing it
  would be client-specific detection; joining whatever group the active tab sits
  in would silently drop Agent-owned tabs into arbitrary user tab groups.

Per the standing rule — if the correct group cannot be reliably identified, stop
and report rather than guess — no Extension change was made. `openAgentTab` in
`extension/src/service_worker.js` is untouched.

## Why the earlier closed loop passed

`docs/reports/2026-08-17-claude-same-tab-closed-loop.md` passed in an ordinary
**attach-mode** Claude Code session, not a `-p --chrome` subprocess. Re-verified
today in attach mode: with a group present, `computer` on an out-of-group
Saccade tab still succeeded and executed on the Saccade `tab_id`.

The discriminator is the client mode, not the group:

| Mode | Foreign tab addressable |
| --- | --- |
| attach-mode Claude Code session | yes, by `tab_id`, group or no group |
| `claude -p --chrome` subprocess | no, group or no group |

## The actual double bind

```text
Saccade-created tab   Agent On, Truth readable   Claude -p cannot act on it
Claude-created tab    Claude can act on it       Saccade Agent Off by default
```

Both halves are correct behaviour. Saccade's default of Agent Off for tabs it
did not create is the authorization boundary and must not be widened: it would
mean anything Claude opens becomes readable without consent.

The sanctioned bridge already exists and works — `ui.tab.share` from the Saccade
popup marks a tab `user_shared` and `observation_ready`. Two tabs in Claude's
group (`1680322986`, `1680322991`) were in exactly that state today. It requires
one human click per tab by design, so it does not automate the benchmark.

## Open decision for the owner

Unblocking the automated lane needs a product decision, not a bug fix:

1. Accept a one-time human share per benchmark tab (keeps every boundary,
   defeats full automation).
2. Add an explicit, single-`tab_id`, session-only authorization MCP verb so a
   client can hand one named tab to Saccade. New protocol surface; must never
   scan or bulk-authorize.
3. Drive the benchmark's Saccade lane from an attach-mode client instead of a
   `-p --chrome` subprocess, where the loop already demonstrably works.

Option 2 is the only one that both automates and preserves consent, and it is a
protocol change that is out of scope here.

## Harness changes made

Only benchmark harness files were touched. No Collector, Truth projection,
control module, Runtime, Host, MCP schema, observation schema, Profile,
protected-value boundary, candidate identity, setup path or Reference Actuator
change. No Extension or Runtime reinstall is required.

- `scripts/run_claude_same_tab.py` — pre-opens the target tab over the normal
  Saccade MCP stdio protocol, names that `tab_id` in the prompt, forbids
  navigation and duplicate tabs, records Claude's execution `tabId`s and verbatim
  Chrome errors, and always closes the tab it opened.
- `tests/test_run_claude_same_tab.py` — covers the above.

### One correctness fix worth noting

The probe originally treated a Truth **revision** advance as proof the click
landed. On this fixture that is wrong: `pushed_delta.html` pushes its own
`Browser cycle` status updates, so revision moved `1 → 118` in a run where
nothing was clicked. The probe now requires a `pressed` state transition on the
`Toggle signal` button. Under the old rule probe 2 would have been scored PASS.
