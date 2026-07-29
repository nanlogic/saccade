# Generated Control Coverage

> Generated from `catalog/controls.json`; do not edit by hand.

| Control | Family | Affordance | Native primitive | Verifier | Chrome | Edge | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| button | button | click | primary_click | button_effect | pending | pending | implementation |
| text_field | editable | type | unicode_text | has_value | pending | pending | implementation |
| link | navigation | click | primary_click | document_transition | pending | pending | implementation |
| search_field | editable | type | unicode_text | has_value | pending | pending | implementation |
| text_area | editable | type | unicode_text | has_value | pending | pending | implementation |
| content_editable | editable | type | unicode_text | has_value | pending | pending | implementation |
| spin_button | editable | type | unicode_text | has_value | pending | pending | implementation |
| checkbox | toggle | click | primary_click | checked_transition | pending | pending | implementation |
| radio | toggle | click | primary_click | checked_transition | pending | pending | implementation |
| switch | toggle | click | primary_click | checked_transition | pending | pending | implementation |
| select | choice | select | select_option | option_selected | pending | pending | implementation |
| reflex_target | reflex | click | primary_click | target_advanced | pending | pending | implementation |
| tab | navigation | click | primary_click | selected_transition | pending | pending | implementation |
| menu_item | navigation | click | primary_click | expanded_transition | pending | pending | implementation |
| file_input | file | upload | file_chooser | has_file | pending | pending | implementation |

No row is `publishable` until current Chrome and Edge artifacts pass for the same release candidate.
