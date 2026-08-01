# Generated Control Coverage

> Generated from `catalog/controls.json` and `catalog/development_evidence.json`; do not edit by hand.

## Evidence summary

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

| Control | Family | Affordance | Input policy | Primitive | Verifier | Chrome | Edge | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| button | button | click | software_preferred | primary_click | button_effect | pending | pending | implementation |
| text_field | editable | type | native_required | unicode_text | has_value | pending | pending | implementation |
| link | navigation | click | software_preferred | primary_click | document_transition | pending | pending | implementation |
| search_field | editable | type | native_required | unicode_text | has_value | pending | pending | implementation |
| text_area | editable | type | native_required | unicode_text | has_value | pending | pending | implementation |
| content_editable | editable | type | native_required | unicode_text | has_value | pending | pending | implementation |
| spin_button | editable | type | native_required | unicode_text | has_value | pending | pending | implementation |
| checkbox | toggle | click | software_preferred | primary_click | checked_transition | pending | pending | implementation |
| radio | toggle | click | software_preferred | primary_click | checked_transition | pending | pending | implementation |
| switch | toggle | click | software_preferred | primary_click | checked_transition | pending | pending | implementation |
| select | choice | click, select | software_preferred | select_option | option_selected | pending | pending | implementation |
| reflex_target | reflex | click | software_preferred | primary_click | target_advanced | pending | pending | implementation |
| tab | navigation | click | software_preferred | primary_click | selected_transition | pending | pending | implementation |
| menu_item | navigation | click | software_preferred | primary_click | expanded_transition | pending | pending | implementation |
| file_input | file | upload | native_required | file_chooser | has_file | pending | pending | implementation |

No row is `publishable` until current Chrome and Edge artifacts pass for the same release candidate.
