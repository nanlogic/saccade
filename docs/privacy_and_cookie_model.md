# Saccade privacy, Cookie, and site-data model

Date: 2026-07-18

## What the browser does

Saccade uses Chromium/CEF's normal network and profile behavior. A persistent
normal profile stores cookies and site data so websites can keep the user
signed in. When a page makes an authenticated request, Chromium may send the
applicable cookies directly to that website. Saccade does not copy cookies from
Chrome, Safari, Firefox, or another Saccade profile.

On macOS, Chromium Safe Storage uses a Keychain-backed encryption key for
persistent cookies, login state, and saved browser credentials. The signed
Saccade app may need one first-use Keychain approval. If the user explicitly
locks the login Keychain, macOS must authenticate again before CEF can read the
key; Saccade does not bypass that system security boundary. The Agent bridge
never receives the key, and test-only mock Keychain is never used for a saved
product profile.

Incognito mode uses a disposable profile. Its cookies and site data exist only
for that browser session and the temporary profile is removed after exit.

## What an Agent can and cannot do

An Agent working in an Agent On tab can use the visible authenticated session
indirectly: the browser may send the website's cookies while loading pages or
performing a user-authorized page action. This is how the Agent can work on a
site where the user already signed in without receiving login credentials.

The MCP contract does not expose:

- raw cookies or cookie-jar files;
- localStorage, sessionStorage, IndexedDB, or cache dumps;
- Keychain secrets or password-manager data;
- browser control capabilities; or
- protected field values.

Agent Off is a browser-layer read and control gate. An Off tab is omitted from
Agent discovery and its URL, content, cookies, and storage are not returned.

Cookie-consent banners remain ordinary website UI. Saccade does not silently
accept them or invent a global legal preference. The user or LLM host decides
whether to interact with a banner; Saccade still enforces the tab grant,
revision, target, input, protected-value, and receipt boundaries.

## User controls

- Persistent profile: the default normal mode under
  `~/Library/Application Support/Saccade/CEF/Profiles/<name>`.
- Incognito profile: launch with `SACCADE_PROFILE_MODE=incognito` through the
  packaged `bin/open-saccade` command.
- Inspect profile metadata without values: `bin/profile-status`.
- Preview a full profile deletion: `bin/clear-profile --dry-run`.
- Delete a full profile after quitting Saccade: `bin/clear-profile --yes`.

When only the app is installed, the equivalent signed in-app commands are
`/Applications/Saccade.app/Contents/MacOS/saccade-profile-status` and
`/Applications/Saccade.app/Contents/MacOS/saccade-clear-profile`.

Full profile deletion signs sites out and removes that profile's cookies,
browser storage, cache, history, and saved site state. The command reports only
file counts and byte totals. It rejects invalid profile names, symlinked profile
paths, and deletion while Saccade is running.

Site-specific Cookie controls are delegated to Chromium's Settings UI. Saccade
does not claim a separate site-data manager until that UI path has a dedicated
installed-build regression.

## Release requirements

A public build must keep this document, the CEF license, Chromium credits,
exact engine versions, and the Saccade privacy boundary in its release bundle.
Regression evidence must show normal-profile persistence, incognito cleanup,
safe profile deletion, and zero raw Cookie/storage values in MCP output and
replay.
