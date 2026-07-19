# AI-039 Native browser chrome

## User-visible problem

The AI-038 package opened a content-only CEF Views window. The page rendered,
but there was no tab strip, address entry, Back, Forward, or Reload surface.
That made the package an agent-capable web container rather than a usable
dogfood browser.

## Product decision

Saccade now defaults to pinned CEF's native Chrome-style window. It supplies
the standard tab strip, new-tab button, location bar, Back, Forward,
Reload/Stop, browser menu, history state, and normal keyboard focus behavior.
Saccade does not maintain a second custom toolbar.

Both human launch paths use the same surface:

- opening `Saccade.app` directly gives a normal browser UI but no agent grant;
- `bin/open-saccade <URL>` gives the same browser UI and explicitly grants the
  current visible tab to the owner-only local bridge.

The old Views surface remains available only through the explicit
`--use-views` diagnostic switch.

## Dogfood controls

- Click the location bar or press Command-L, type a URL, and press Return.
- Use the visible Back, Forward, and Reload/Stop buttons.
- Use the `+` button for a new tab.
- For LLM collaboration, start the session through `bin/open-saccade`.

## Evidence

- Visual evidence: `runs/dogfood/ai039_browser_chrome_20260715/browser_chrome.png`
- Native agent regression:
  `runs/dogfood/ai039_native_agent_regression_final_20260715/report.json`
- Packaged native agent regression:
  `runs/dogfood/ai039_packaged_native_agent_20260715/report.json`
- Pinned CEF source contract: native `CefBrowserHost::CreateBrowser` with
  Chrome runtime creates the fully styled Chrome UI window.

The native regression passed automatic current-tab attach, bounded article
reading, current-site action context, value-blind populated SSN handling, two
verified ordinary-field writes, human-value preservation, and no submit.

## Milestone report

MILESTONE: AI-039 native browser chrome
VERDICT: PASS
CHROME: tabs + new tab + location + back + forward + reload/stop visible
AGENT: AI-038 native regression PASS
PACKAGE: dist/saccade-cef-dogfood-ai039-native-chrome-20260715
PRIVACY: incognito + mock Keychain test; no saved profile touched
SIGNING: Developer ID, bundle ai.saccade.browser, Team 48KK2UWXQM
