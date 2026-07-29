#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use saccade_control_sdk::NativePrimitive;
use saccade_protocol::{ActionOperation, ActionPayload, DispatchStatus, PreparedAction};

use crate::NativeInput;

#[cfg(any(target_os = "macos", target_os = "windows", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeStep {
    PrimaryClick,
    UnicodeText,
    ChoicePopupDelay,
    ChoiceTypeahead,
    Return,
    PostActionDelay,
}

/// Audited finite adapter. Control modules cannot call platform code directly.
pub struct PlatformInput;

impl NativeInput for PlatformInput {
    fn execute(
        &mut self,
        primitive: NativePrimitive,
        prepared: &PreparedAction,
        payload: &ActionPayload,
        selection_name: Option<&str>,
    ) -> DispatchStatus {
        if !primitive_matches(primitive, prepared.operation, payload) {
            return DispatchStatus::Rejected;
        }
        dispatch(prepared, payload, selection_name).unwrap_or(DispatchStatus::Rejected)
    }
}

fn primitive_matches(
    primitive: NativePrimitive,
    operation: ActionOperation,
    payload: &ActionPayload,
) -> bool {
    matches!(
        (primitive, operation, payload),
        (
            NativePrimitive::PrimaryClick,
            ActionOperation::Click,
            ActionPayload::None
        ) | (
            NativePrimitive::UnicodeText,
            ActionOperation::Type,
            ActionPayload::Text { .. }
        ) | (
            NativePrimitive::SelectOption,
            ActionOperation::Select,
            ActionPayload::Select { .. }
        )
    )
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn event_plan(
    prepared: &PreparedAction,
    payload: &ActionPayload,
    selection_name: Option<&str>,
) -> anyhow::Result<Vec<NativeStep>> {
    match (prepared.operation, payload) {
        (ActionOperation::Click, ActionPayload::None) => Ok(vec![NativeStep::PrimaryClick]),
        (ActionOperation::Type, ActionPayload::Text { .. }) => {
            Ok(vec![NativeStep::PrimaryClick, NativeStep::UnicodeText])
        }
        (ActionOperation::Select, ActionPayload::Select { .. }) => {
            prepared
                .selection_index
                .ok_or_else(|| anyhow::anyhow!("select preparation has no option index"))?;
            if !matches!(selection_name, Some(name) if !name.is_empty()) {
                anyhow::bail!("select preparation has no visible option name");
            }
            Ok(vec![
                NativeStep::PrimaryClick,
                NativeStep::ChoicePopupDelay,
                NativeStep::ChoiceTypeahead,
                NativeStep::Return,
                NativeStep::PostActionDelay,
            ])
        }
        _ => anyhow::bail!("operation and native payload do not match"),
    }
}

pub fn dispatch(
    prepared: &PreparedAction,
    payload: &ActionPayload,
    selection_name: Option<&str>,
) -> anyhow::Result<DispatchStatus> {
    if !prepared.visible || !prepared.topmost || !prepared.focus_verified {
        return Ok(DispatchStatus::FocusMismatch);
    }
    if !prepared.screen_bounds.is_valid()
        || prepared.screen_bounds.width <= 0.0
        || prepared.screen_bounds.height <= 0.0
    {
        anyhow::bail!("extension returned invalid prepared geometry");
    }
    #[cfg(target_os = "macos")]
    return macos::dispatch(prepared, payload, selection_name);
    #[cfg(target_os = "windows")]
    return windows::dispatch(prepared, payload, selection_name);
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let _ = (prepared, payload, selection_name);
        Ok(DispatchStatus::Unsupported)
    }
}

pub fn accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    return macos::accessibility_trusted();
    #[cfg(not(target_os = "macos"))]
    true
}

pub fn request_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    return macos::request_accessibility();
    #[cfg(not(target_os = "macos"))]
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_operation_pairs_are_finite() {
        assert!(primitive_matches(
            NativePrimitive::PrimaryClick,
            ActionOperation::Click,
            &ActionPayload::None
        ));
        assert!(!primitive_matches(
            NativePrimitive::PrimaryClick,
            ActionOperation::Type,
            &ActionPayload::Text { text: "x".into() }
        ));
        assert!(!primitive_matches(
            NativePrimitive::UnicodeText,
            ActionOperation::Click,
            &ActionPayload::None
        ));
    }

    #[test]
    fn textfield_plan_clicks_the_prepared_center_before_unicode() {
        let prepared = prepared_text_field();
        assert_eq!(
            event_plan(
                &prepared,
                &ActionPayload::Text {
                    text: "value".into()
                },
                None,
            )
            .unwrap(),
            vec![NativeStep::PrimaryClick, NativeStep::UnicodeText]
        );
    }

    #[test]
    fn select_plan_waits_for_the_native_popup_before_keyboard_selection() {
        let mut prepared = prepared_text_field();
        prepared.operation = ActionOperation::Select;
        prepared.selection_index = Some(2);
        assert_eq!(
            event_plan(
                &prepared,
                &ActionPayload::Select {
                    option_object_id: "option-2".into()
                },
                Some("Blue"),
            )
            .unwrap(),
            vec![
                NativeStep::PrimaryClick,
                NativeStep::ChoicePopupDelay,
                NativeStep::ChoiceTypeahead,
                NativeStep::Return,
                NativeStep::PostActionDelay
            ]
        );
    }

    fn prepared_text_field() -> PreparedAction {
        PreparedAction {
            browser_instance_id: "browser-1".into(),
            tab_id: "tab-1".into(),
            document_id: "document-1".into(),
            basis_revision: 1,
            viewport_revision: 1,
            object_id: "field-1".into(),
            action_token: "token.0123456789abcdef0123456789abcdef".into(),
            operation: ActionOperation::Type,
            screen_bounds: saccade_protocol::Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 24.0,
            },
            visible: true,
            topmost: true,
            focus_verified: true,
            selection_index: None,
        }
    }
}
