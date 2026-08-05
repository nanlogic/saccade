# Truth Layer latency and completeness baseline

Date: 2026-08-03

This report measures the current local release candidate through the complete
page mutation → Extension compiler → Native Host → Runtime → MCP return route.
The fixture embeds a same-machine epoch timestamp in each semantic mutation;
the probe samples the clock after the matching delta returns from MCP.

It is a local conformance and performance baseline, not proof that every public
website has the same latency or that inaccessible browser content has internal
semantics.

## Final Chrome and Edge gate

Evidence root: `~/Library/Application Support/Saccade Dev/evidence/20260803T211030Z`.

| Scenario | Chrome | Edge | Gate |
| --- | ---: | ---: | ---: |
| Initial full (~200 projected objects) | 162.204 ms | 45.638 ms | ≤500 ms |
| Single-object delta p95 (20 samples) | 29.026 ms | 15.536 ms | ≤50 ms |
| 10-object simultaneous batch p95 | 9.625 ms | 12.748 ms | ≤100 ms |
| 100-object simultaneous batch p95 | 60.298 ms | 17.085 ms | ≤500 ms |
| Lifecycle maximum (remove/replace/reorder) | 82.599 ms | 14.392 ms | ≤250 ms |

Both browsers observed all 134 expected markers with zero missing markers,
duplicates, or empty semantic deltas. The removal and replacement each emitted
the required disappearance, and reordering retained all 100 object identities.

## Tail behavior

An earlier Chrome run on the same candidate measured single-object p95 at
28.617 ms but a 100-object batch at 380.580 ms. It still reported 134/134
markers with zero omissions. This shows that ordinary interaction latency is
currently in the tens of milliseconds while a large simultaneous structural
batch can experience material machine-load tail latency.

The gate therefore uses separate workload limits instead of presenting one
aggregate percentile as ordinary interaction latency. A future optimization
target is to reduce Chrome batch tail variance without weakening complete
projection or adding a site-specific fast path.

## Clean-profile alternating matrix

A 10-round matrix on 2026-08-04 created a new disposable profile for each
browser in each round and alternated Chrome-first/Edge-first order. Evidence is
under `20260804T203930Z/latency-matrix`.

| Scenario | Chrome p95 | Edge p95 |
| --- | ---: | ---: |
| Initial full | 49.053 ms | 80.260 ms |
| Single-object delta (200 samples/browser) | 16.511 ms | 18.965 ms |
| 10-object batch (100 samples/browser) | 15.742 ms | 16.421 ms |
| 100-object batch (1,000 samples/browser) | 21.082 ms | 22.204 ms |
| Remove | 16.113 ms | 16.411 ms |
| Replace | 17.161 ms | 16.570 ms |
| Reorder | 15.347 ms | 15.449 ms |

All 20 browser runs passed with zero missing markers, duplicates, or empty
deltas. First/second position changed the delta percentiles only slightly. The
earlier Chrome 60–380 ms batch tail did not reproduce with disposable profiles;
the evidence points to retained development-profile/browser-state noise rather
than an inherent Chrome collector penalty.

## Canvas and WebGL observation boundary

The deterministic fixture now performs a real Canvas 2D draw and a real WebGL
clear, while updating an application-supplied accessible semantic companion on
the same `opaque_surface`. A two-round clean-profile matrix on 2026-08-04 is
under `20260804T204640Z/latency-matrix`.

| Scenario | Chrome p95 | Edge p95 |
| --- | ---: | ---: |
| Canvas draw + semantic companion delta | 13.400 ms | 10.466 ms |
| WebGL clear + semantic companion delta | 8.813 ms | 10.597 ms |

Both browsers returned the expected semantic changes with zero missing,
duplicate, or empty deltas. This proves fast observation when the application
exposes a revalidatable semantic companion. It deliberately does not claim that
Saccade can infer arbitrary object identity or meaning from pixels alone. A
pure-pixel Canvas or WebGL surface remains honestly opaque.

## React and Angular evidence boundary

Public-page runs already exercise React dynamic replacement on DemoQA and
Angular Material state/lazy-render behavior. They establish that the collector
can preserve useful role, name, state, identity, and pushed-delta behavior
through those framework update patterns. They do not provide a trustworthy
page-mutation timestamp controlled by the fixture, so their latency can only be
reported as external-action-return to matching-delta-read time. It must not be
mixed with the deterministic mutation-to-MCP measurements above.

The next public gate should retain this separation: framework pages prove
compatibility and semantic completeness; controlled fixtures prove strict
pipeline latency. Any React or Angular failure must be repaired as a general
DOM/ARIA, identity, lifecycle, frame, or visibility cause—never with a
framework-name or site-URL branch.

## Supported conclusion

For the defined local denominator, Saccade has demonstrated millisecond-scale
full and delta delivery with zero silent omissions. The evidence supports a
stronger statement for ordinary changes: single-object p95 was below 30 ms in
Chrome and below 16 ms in Edge in the final run. It does not support a universal
latency or compatibility claim across arbitrary public pages.
