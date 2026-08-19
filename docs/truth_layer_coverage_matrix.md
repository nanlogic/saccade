# Truth Layer coverage

The machine-readable source of truth is
[`catalog/truth_inventory.json`](../catalog/truth_inventory.json). It accounts
for every protocol semantic role, every implemented roadmap variant, and every
structural boundary. A role may not exist only in prose.

## Current complete local gate

The protocol defines 34 roles:

- 15 interactive roles: `button`, `link`, `text_field`, `search_field`,
  `text_area`, `content_editable`, `checkbox`, `radio`, `switch`, `select`,
  `file_input`, `spin_button`, `tab`, `menu_item`, and `reflex_target`;
- 17 additional semantic roles: `option`, `heading`, `paragraph`, `text`,
  `list`, `list_item`, `table`, `row`, `cell`, `alert`, `status`, `image`,
  `slider`, `label`, `generic_control`, `opaque_surface`, and
  `restricted_document`;
- `frame`, which uses the structural metadata gate;
- reserved `unknown`, which uses a negative non-emission gate and must never
  appear in Agent output.

The 12 reusable variants are `date`, `time`, `month`, `week`,
`datetime_local`, `color`, `native_listbox`, `aria_listbox`, `aria_combobox`,
`drag_source`, `drop_target`, and `built_in_pdf`. They reuse a semantic role
instead of creating arbitrary per-HTML-element roles.

The 6 structural/push boundaries are `same_origin_frame`, `restricted_frame`,
`open_shadow_root`, `closed_shadow_root`, `stream_gap_reset`, and
`resource_notification`.

Opaque Canvas2D, WebGL, video, and restricted document surfaces are emitted as
bounded objects with explicit limitations. Their pixels or internals are not
claimed as semantic controls. Same-origin frames and open Shadow DOM compose
into Truth; restricted frames and marked closed-shadow boundaries remain
limited or opaque.

`./scripts/dev.sh test all` must prove in both Chrome and Edge:

- safe initial projection and no public action authority;
- a real Extension-produced delta for every positive role and variant;
- current `document_bounds` and `viewport_bounds` for projected objects plus a
  pushed update when an object's geometry changes;
- full→delta continuity and unsolicited Resource notification;
- frame and Shadow boundaries;
- non-emission of the reserved `unknown` role;
- absence of editable contents, locators, arbitrary-coordinate action
  authority, and action tokens from evidence.

## Reference Actuator boundary

`catalog/controls.json` currently retains the 16 families with an audited
Reference Actuator implementation. Its native primitives and verifiers live in
`catalog/reference_actuators.json`. That smaller list is not the Truth Layer
coverage list and must never be presented as the total number of supported
semantic objects.

## Legacy gauntlet relationship

The reviewed legacy `SACCADE_EVALUATION_GAUNTLET_v1` also lists page behaviors:
dynamic loading, disappearing elements, dialogs, upload/download, infinite
scroll, sortable tables, drag/drop, overlay blocking, and slow resources.
Those are lifecycle or application scenarios, not additional control roles.
They receive separate public-site and lifecycle evidence; they do not inflate
the semantic role count or justify copying the retired browser engine.

Local fixture success remains `implementation` evidence. `publishable` still
requires the same frozen release candidate in current Chrome and Edge plus the
required independent public-page evidence.

The precise current claim is: the complete local Truth inventory and the
two-browser pushed-delta framework gate pass. `./scripts/dev.sh denominator`
binds that inventory to the 11 lifecycle rows and emits all 63 results for one
candidate. The current split is 56 local passes and 7 truthful limitations:
opaque surfaces, restricted documents, the reserved unknown role, the
observation-only drop target, built-in PDF, restricted frames, and closed
Shadow DOM. The inventory does not establish universal modern-web compatibility
or superiority over Playwright.
