# 2026-08-19 heavy public-sites dogfood

Runtime and Extension candidate:
`4c5ff8e20489e72b75437c8772a37e27341d2d40327bfe2d3de1c03232be6d54`
(`0.3.22`). Browser: Chrome. All pages were temporary Agent-owned tabs and were
closed after read-only testing.

## Sites

- IGN
- Best Buy
- GitHub
- NanMesh
- Nanlogic
- Mythcastera

Fresh Runtime MCP processes opened every site, received canonical Truth, and
then received a later delta under the same document identity with no gap. This
proved that the current Runtime does not repeat a full page for routine later
reads. A long-lived Codex session still attached to an older MCP process did
repeat full responses on IGN and Best Buy; restarting the MCP client is required
after installing a new Runtime.

Immediate unqueried reads frequently reached the eager revision-1 Snapshot
before page hydration. The next pushed delta arrived about 0.2–0.4 seconds later
and contained the page objects. No information was lost, but a discovery Agent
should use a bounded semantic query with `min_objects` when it already knows a
target role or label. Those queries wait locally for useful hydration instead
of making the model poll.

Meaningful bounded working sets were obtained from Best Buy, GitHub, NanMesh,
Nanlogic, and Mythcastera. GitHub exposed the signed-in Dashboard/Home headings;
Best Buy exposed search, cart, Shop, Deals, Support & Services, Top Deals, and
Deal of the Day; the three company sites exposed their current headings and
product copy.

## IGN failure and fix

IGN initially returned zero objects for a correct root-only query even though
an all-frame query showed 204 total objects and 145 matching links/buttons/text.
Its top frame was incorrectly marked `root:false`. Runtime's old rule returned a
default root only when `frames.len() == 1`; IGN has same-origin child frames.

Runtime now selects the unique `FrameObservation` with no `parent_frame_id`.
After rebuilding, the identical real IGN root query returned `settled:true`,
125 matches, and one root frame containing 182 objects. The bounded sample
included current article links such as “GTA 6 Gameplay and Map Appear to Leak
Online”, “Mortal Shell 2 Review”, and IGN navigation. No site-specific selector
or fallback was introduced.

## Honest boundaries

IGN video objects remained `opaque_video`, Nanlogic's visual surface remained
`opaque_canvas`, and no screenshot or coordinate route was used. These are
truthful limitations, not missing DOM Truth. This run tested reading and delta
delivery only; it performed no login, purchase, account, or page action.

## Independent Claude read

Claude Opus 5 low then repeated the six-site read with a strict MCP configuration
containing only Saccade. The trace contained 20 calls: one tool discovery, one
capabilities check, six tab opens, six bounded working-set reads, and six tab
closes. Claude reported `completed:true` with current tool-output evidence for
all sites, including IGN Game Scope, Best Buy Deal of the Day, the signed-in
GitHub Dashboard, the NanMesh protocol-layer heading, Nanlogic's NaNDesk CTA,
and Mythcastera's world-section navigation. It used no Claude-in-Chrome,
Playwright, generic search, screenshot, coordinate, selector, or shell route.
