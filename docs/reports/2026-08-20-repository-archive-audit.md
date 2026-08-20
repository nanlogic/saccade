# Nanlogic Saccade repository archive audit

Status: owner-approved archival completed on 2026-08-20.

| Repository | Default branch final SHA | State | Open PRs | Releases | Recent Actions | Packages |
| --- | --- | --- | ---: | --- | --- | --- |
| `nanlogic/cef-saccade` | `803fe341dffd5b6b18eb009301b8d9a61a83a329` | public fork, archived 2026-08-20 | 0 | none | none | public page shows package onboarding/no published package |
| `nanlogic/chromium-saccade` | `24a198fe772b9f4ad2bc8ecfc5331734dccc7f43` | public fork, archived 2026-08-20 | 0 | none | none | public page shows package onboarding/no published package |
| `nanlogic/saccade-legacy` | `8c4defb3f8b0ed9b0cb4cb6ff522f9a550ddb76b` | private, already archived | 2 | `v0.1.0-alpha.1` prerelease | none | private package inventory unavailable to the current token |

The two public repositories are unmodified upstream CEF and Chromium forks
from the abandoned embedded-browser direction. They were archived with
Wayne's explicit approval without deleting history or affecting the Extension
→ Native Host → Runtime → MCP product route.

`nanlogic/saccade-legacy` was already archived before this audit. Its open PRs
are #6, “Clarify open-source release and HN launch,” and #8, “Align Windows
Build 98 release integrity.” They remain historical and must not be merged into
the current product wholesale. The repository stays readable through the
current `legacy` Git remote for one-component-at-a-time migration.

`nanlogic/saccade` is the current workspace origin and is not an archive
candidate. It remains the sole company source and release repository.
