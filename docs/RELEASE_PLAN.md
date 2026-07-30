# Developer Preview release plan

## Release target

The first public build targets macOS with current Chrome and Edge. A tester
installs one signed app, confirms one browser Extension, selects a Profile, and
runs a public-page proof without building the repository.

The preview will ship the 15 current Catalog controls. Bounded page reading and
ARIA listbox/combobox source are implemented and passed paired managed-browser
development proof in run `20260729T225249Z`. Frozen release-candidate and public
page evidence remain release blockers. Slider, date/time variants, color, and
drag and drop can follow after the preview if the coverage table names those
gaps.

Windows follows as a separate signed candidate. The macOS preview must not
imply Windows support.

## Product gates

- Prove bounded heading, paragraph, list-item, table-cell, alert, and status
  projection in managed Chrome and Edge. Report frame and opaque-surface limits.
- Prove listbox and combobox option identity, popup settling, disabled choices,
  duplicate names, and dynamic options in managed Chrome and Edge.
- Prove the Extension popup share/revoke flow in Chrome and Edge, including
  unsupported pages, browser restart, and immediate token invalidation.
- Produce a signed and notarized macOS app, Native Messaging manifests, and
  Chrome Web Store and Edge Add-ons builds.
- Prove clean install, upgrade, repair, browser restart, Host restart, and
  uninstall on a test account. Upgrade and repair must preserve the user's
  Profile and local input-policy log; uninstall must state whether that log is
  retained or removed.
- Prove automatic Registry selection, user-remembered native exceptions, and
  receipt-backed software-to-native learning in both browsers. Confirm that an
  unverified software dispatch never triggers a same-token native retry and
  that the log contains no values, locators, coordinates, or URL query data.
- Publish a five-minute quickstart and one command that produces a redacted
  diagnostic bundle.

## Release-candidate data

Freeze one source commit, Runtime version, Extension version, Chrome version,
and Edge version before measurement. Store that identity beside each artifact.

Run the following gates again from clean browser profiles:

- every Catalog fixture in Chrome and Edge;
- at least two independent public implementations for each common control;
- authenticated dogfood for link and file input, plus the MouseAccuracy reflex
  run;
- stale, replay, focus, covered, navigation, Profile-ban, and value-leak tests;
- action latency p50, p95, and p99 with success and failure counts;
- the Playwright semantic oracle after Saccade passes independently.

Publish the full denominator. Failed sites stay in the report with a reason.
Do not combine results from different commits or select the best run.

## Tester package

- signed DMG and checksums;
- store Extension links and supported browser versions;
- `README` quickstart, architecture, Profile example, and coverage table;
- a public fixture command and a public-site comparison command;
- limitations for protected values, browser-owned dialogs, frames, PDF,
  Canvas/WebGL, and unsupported controls;
- GitHub issue templates for install failures, incorrect observations, and
  action receipts.

## Launch material

Prepare a short screen recording that shows installation, observation, native
input, a verified receipt, and one truthful rejection. Publish the raw evidence
table and the Playwright comparison method beside the video.

The launch post should explain the problem, the single execution route, the
closed-loop receipt, measured results, and known limits. Avoid claims about
arbitrary websites or safety guarantees beyond the published gates.

## Distribution

Primary launch:

- GitHub Release with source, artifacts, checksums, and evidence;
- Show HN with a technical demo and direct repository link.

Follow-up posts:

- Lobsters, if an existing member submits it under the site's rules;
- relevant Reddit communities such as LocalLLaMA and opensource after checking
  each community's self-promotion rules;
- a technical article on the project site or DEV Community;
- X and LinkedIn posts pointing to the evidence and demo;
- Product Hunt after the install flow works for non-repository testers.

Check each site's current posting and self-promotion rules from its official
pages before scheduling. Space posts across several days so maintainers can
answer issues and repair onboarding failures.

## Release decision

Wayne approves the candidate after reviewing the installer, demo, evidence,
limitations, and launch copy. Publishable Catalog rows require signed-candidate
Chrome and Edge evidence. Local development runs cannot promote them.
