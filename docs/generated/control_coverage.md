# Generated Control Coverage

> Generated from `catalog/truth_inventory.json`, `catalog/controls.json`, and `catalog/development_evidence.json`; do not edit by hand.

## Complete Truth inventory

Protocol roles: 34. Reusable variants: 12. Structural/push boundaries: 6.

| Kind | ID | Protocol role | Status | Gate |
| --- | --- | --- | --- | --- |
| role | button | button | implemented | control |
| role | link | link | implemented | control |
| role | text_field | text_field | implemented | control |
| role | search_field | search_field | implemented | control |
| role | text_area | text_area | implemented | control |
| role | content_editable | content_editable | implemented | control |
| role | checkbox | checkbox | implemented | control |
| role | radio | radio | implemented | control |
| role | switch | switch | implemented | control |
| role | select | select | implemented | control |
| role | option | option | implemented | semantic |
| role | file_input | file_input | implemented | control |
| role | spin_button | spin_button | implemented | control |
| role | tab | tab | implemented | control |
| role | menu_item | menu_item | implemented | control |
| role | reflex_target | reflex_target | implemented | control |
| role | heading | heading | implemented | semantic |
| role | paragraph | paragraph | implemented | semantic |
| role | list_item | list_item | implemented | semantic |
| role | cell | cell | implemented | semantic |
| role | alert | alert | implemented | semantic |
| role | status | status | implemented | semantic |
| role | image | image | implemented | semantic |
| role | frame | frame | implemented_metadata | structure |
| role | text | text | implemented | semantic |
| role | list | list | implemented | semantic |
| role | table | table | implemented | semantic |
| role | row | row | implemented | semantic |
| role | slider | slider | implemented | semantic |
| role | label | label | implemented | semantic |
| role | generic_control | generic_control | implemented | semantic |
| role | opaque_surface | opaque_surface | implemented | semantic |
| role | restricted_document | restricted_document | implemented | semantic |
| role | unknown | unknown | reserved | negative |
| variant | date | text_field | implemented | semantic |
| variant | time | text_field | implemented | semantic |
| variant | month | text_field | implemented | semantic |
| variant | week | text_field | implemented | semantic |
| variant | datetime_local | text_field | implemented | semantic |
| variant | color | text_field | implemented | semantic |
| variant | drag_source | generic_control | implemented | semantic |
| variant | drop_target | generic_control | implemented_observation_only | semantic |
| variant | built_in_pdf | restricted_document | implemented_opaque | semantic |
| variant | native_listbox | select | implemented | control |
| variant | aria_listbox | select | implemented | control |
| variant | aria_combobox | select | implemented | control |
| boundary | same_origin_frame | — | implemented | structure |
| boundary | restricted_frame | — | implemented | structure |
| boundary | open_shadow_root | — | implemented | structure |
| boundary | closed_shadow_root | — | opaque | structure |
| boundary | stream_gap_reset | — | implemented | push |
| boundary | resource_notification | — | implemented | push |

## Reference-capable control modules

These 15 rows are the optional Reference Actuator subset, not the complete Truth Layer.

## Reference Actuator evidence summary

Implemented: 15. Chrome + Edge fixture: 13. Chrome + Edge external: 1. Publishable: 0.

Chrome / Edge values are shown in that order.
External status requires two independent traceable public sources per control and browser.

| Control | Implemented | Fixture C / E | External C / E | Release C / E |
| --- | --- | --- | --- | --- |
| button | yes | passed / passed | pending / pending | pending / pending |
| text_field | yes | passed / passed | pending / pending | pending / pending |
| link | yes | passed / passed | pending / pending | pending / pending |
| search_field | yes | passed / passed | pending / pending | pending / pending |
| text_area | yes | passed / passed | pending / pending | pending / pending |
| content_editable | yes | passed / passed | pending / pending | pending / pending |
| spin_button | yes | passed / passed | pending / pending | pending / pending |
| checkbox | yes | passed / passed | pending / pending | pending / pending |
| radio | yes | passed / passed | passed / passed | pending / pending |
| switch | yes | passed / passed | pending / pending | pending / pending |
| select | yes | passed / passed | pending / pending | pending / pending |
| reflex_target | yes | passed / pending | pending / pending | pending / pending |
| tab | yes | passed / passed | pending / pending | pending / pending |
| menu_item | yes | passed / passed | pending / pending | pending / pending |
| file_input | yes | passed / pending | pending / pending | pending / pending |

`Fixture` and `External` are local development evidence. `Release` stays pending until a signed release candidate passes.

## Public case inventory

| Control | Declared cases | Sources | Implementations |
| --- | ---: | --- | --- |
| button | 0 | gap | gap |
| text_field | 1 | Selenium | native_html |
| link | 0 | gap | gap |
| search_field | 0 | gap | gap |
| text_area | 1 | Selenium | native_html |
| content_editable | 0 | gap | gap |
| spin_button | 0 | gap | gap |
| checkbox | 1 | Selenium | native_html |
| radio | 2 | Selenium, W3C WAI-ARIA APG | aria, native_html |
| switch | 1 | W3C WAI-ARIA APG | aria |
| select | 1 | Selenium | native_html |
| reflex_target | 0 | gap | gap |
| tab | 1 | W3C WAI-ARIA APG | aria |
| menu_item | 1 | W3C WAI-ARIA APG | aria |
| file_input | 0 | gap | gap |

## Module details

| Control | Family | Safe state | Affordance | Chrome | Edge | Status |
| --- | --- | --- | --- | --- | --- | --- |
| button | button | enabled, pressed, expanded | click | pending | pending | implementation |
| text_field | editable | has_value, enabled, required, readonly, invalid | type | pending | pending | implementation |
| link | navigation | enabled, current, expanded | click | pending | pending | implementation |
| search_field | editable | has_value, enabled, required, readonly, invalid | type | pending | pending | implementation |
| text_area | editable | has_value, enabled, required, readonly, invalid | type | pending | pending | implementation |
| content_editable | editable | has_value, readonly | type | pending | pending | implementation |
| spin_button | editable | has_value, enabled, required, readonly, invalid | type | pending | pending | implementation |
| checkbox | toggle | checked, enabled, required, invalid | click | pending | pending | implementation |
| radio | toggle | checked, enabled, required, invalid | click | pending | pending | implementation |
| switch | toggle | checked, enabled | click | pending | pending | implementation |
| select | choice | has_value, enabled, required, invalid, expanded | click, select | pending | pending | implementation |
| reflex_target | reflex | enabled, reflex_occurrence | click | pending | pending | implementation |
| tab | navigation | selected, enabled | click | pending | pending | implementation |
| menu_item | navigation | expanded, enabled | click | pending | pending | implementation |
| file_input | file | has_value, enabled, required | upload | pending | pending | implementation |

No row is `publishable` until current Chrome and Edge artifacts pass for the same release candidate.
