# AI-040 — Per-tab Agent consent and LLM-owned tabs

## Outcome

Saccade now separates an available browser broker from permission to inspect or
control a tab. Human-created tabs start **Agent Off**. The browser-owned
titlebar control changes only the visible tab to **Agent On** or back to Off.
No page DOM is injected to implement the control.

An LLM uses `saccade.tabs.open_agent`. If Saccade is already running, the
broker opens a new foreground tab in that process. If it is not running, the
packaged MCP launcher starts the signed app. Only the LLM-created tab begins
On; existing Human tabs are never silently enabled.

## Authorization state machine

- Direct app launch: broker available, first Human tab Off.
- Human `+` tab: Off.
- Human presses `Agent Off`: that visible tab becomes On.
- Human presses `Agent On`, or MCP pauses the tab: that tab becomes Off and
  pending page state is discarded.
- LLM `open_agent`: dedicated Agent-owned tab starts On.
- Navigation keeps the current tab's state.
- Closing a tab destroys its state.
- Off tabs expose neither URL/title nor truth/actions/form surfaces. Those
  calls return `CONSENT_REQUIRED`.

The compatibility launcher reads only the embedded bundle id and Team ID. It
does not request the Developer ID private key at runtime; private-key access is
a packaging-time operation only.

## Trust boundaries

- The owner-only process broker and bearer capability are not a read grant.
- The switch lives in browser chrome, not untrusted page content.
- An LLM can create its own On tab but cannot turn a Human tab On.
- Passwords, SSNs, payment values, cookies, browser storage, and Keychain
  material remain outside the agent contract.

## Pinned CEF mapping

- Existing-process new tab:
  `CefBrowserHost::ExecuteChromeCommand(IDC_NEW_TAB,
  CEF_WOD_NEW_FOREGROUND_TAB)` from the pinned CEF 150 headers.
- CEF does not expose an API to insert a custom item into the Chrome toolbar.
  macOS therefore uses an `NSTitlebarAccessoryViewController`, keyed to the
  focused CEF browser id.

## Done-when evidence

- Incremental pinned CEF `cefsimple` build passes.
- `cargo test -p saccade-mcp` passes.
- Signed package must show the Saccade icon and `Agent Off/On` control.
- Direct launch must publish an Off broker grant.
- MCP LLM launch must either reuse the process or start it, then publish an
  `agent_created_tab` grant and return live truth.

## Section 16 report

- Milestone: AI-040 per-tab Agent consent.
- Scope: browser broker, per-tab state, browser-owned switch, LLM open/reuse,
  icon/branding, packaging and bounded cleanup only.
- Servo pin: unchanged.
- Hot loop: unchanged.
- Real-site budget: no automated real-site run required for this milestone.
- Residual risk: titlebar accessory geometry is macOS/CEF-version-specific and
  must be visually rechecked when the pinned CEF version changes.
