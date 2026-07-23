# Public Web And Article Backlog

Status values:

- **Draft now:** enough source material exists for a draft, but publication may
  still depend on public evidence.
- **Evidence first:** do not draft measured claims until the named gate passes.
- **Release first:** wait for a supported public build.

## Core web pages

| Priority | Page | Status | Required proof or input |
| --- | --- | --- | --- |
| P0 | Home | Draft now | Product definition, experimental status, repository URL, one public demo |
| P0 | Evidence index | Evidence first | First sanitized reflex pack and iframe report |
| P0 | Security and privacy | Draft now | Product contract, protected-value boundary, SECURITY contact |
| P0 | Compatibility and limits | Draft now | Current macOS/Windows matrix, iframe status, known routed surfaces |
| P1 | How it works | Draft now | Grant-to-receipt diagram and one value-free replay example |
| P1 | Install and quick start | Release first | Signed public artifact, checksums, clean-machine verification |
| P1 | Changelog | Release first | Frozen version scheme and public release commit |

## Article queue

| Order | Working title | Status | Publication gate |
| --- | --- | --- | --- |
| 1 | Building a browser agent that can prove what it clicked | Draft now | Real reflex evidence pack, final iframe status, live repository/evidence links |
| 2 | The iframe fields a human could see and our agent refused to guess | Evidence first | Reproduce the three-field page on current macOS and Windows builds; publish inventory, plan, receipts, and failure/fix result |
| 3 | What a 15-second browser game can prove | Evidence first | Uncut run, page score, matching native receipts, value-free replay, build and commit |
| 4 | Keeping passwords and OTPs outside an agent's context | Evidence first | Independent sentinel leak audit across MCP output, replay, errors, and screenshots |
| 5 | Revision-bound form filling in a shared browser tab | Evidence first | Two real user-granted drafts, preserve-existing result, partial-failure recovery |
| 6 | Saccade and Playwright MCP on the same browser task | Evidence first | Frozen task, model, environment, sample size, raw reports, and explicit non-claims |
| 7 | Why Saccade moved its product browser to CEF | Draft now | Engine timeline, current architecture diagram, compatibility measurements |
| 8 | Shipping the same agent-browser contract on macOS and Windows | Release first | Signed build matrix, installer/profile tests, MCP parity, known platform differences |

## Recommended first series

Publish the first three pieces as one evidence story:

1. The architecture article explains revision-bound actions and receipts.
2. The iframe report shows a useful fail-closed result or its measured fix.
3. The reflex report shows the local fact-to-native-input loop with an uncut
   demonstration and machine-readable proof.

Keep the protected-value article separate. Its release gate needs an
independent leak audit; fixture results alone do not support a broad privacy
claim.

## Editorial rules

- One primary claim per article.
- Link every public number to a dated report.
- Put limitations in the article body, not a footnote.
- Use the product build and commit that produced the evidence.
- Link to original sources for comparisons and standards.
- Keep video, screenshots, and GIFs in an illustrative role.
- Mark corrections with a date and preserve the original claim context.
- Do not turn internal milestone names into public product versions.
