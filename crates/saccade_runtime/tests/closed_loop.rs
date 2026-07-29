use std::collections::BTreeSet;

use saccade_control_sdk::NativePrimitive;
use saccade_protocol::*;
use saccade_runtime::*;

struct Native {
    calls: Vec<NativePrimitive>,
    status: DispatchStatus,
}
impl NativeInput for Native {
    fn execute(
        &mut self,
        primitive: NativePrimitive,
        _: &PreparedAction,
        _: &ActionPayload,
        _: Option<&str>,
    ) -> DispatchStatus {
        self.calls.push(primitive);
        self.status
    }
}

struct Source {
    current: Option<ObservationSnapshot>,
    post: Option<ObservationSnapshot>,
}
impl ObservationSource for Source {
    fn current_observation(&mut self) -> Result<ObservationSnapshot, ClosedLoopError> {
        self.current
            .take()
            .ok_or_else(|| ClosedLoopError::ObservationSource("missing fixture".into()))
    }

    fn settled_observation(
        &mut self,
        _: u64,
    ) -> Result<(ObservationSnapshot, bool), ClosedLoopError> {
        self.post
            .take()
            .map(|snapshot| (snapshot, true))
            .ok_or_else(|| ClosedLoopError::ObservationSource("missing fixture".into()))
    }
}

fn source(current: Option<ObservationSnapshot>, post: Option<ObservationSnapshot>) -> Source {
    Source { current, post }
}

fn object(
    id: &str,
    role: SemanticRole,
    affordance: Affordance,
    state: &[(&str, &str)],
) -> ObservedObject {
    ObservedObject {
        object_id: id.into(),
        object_revision: 1,
        frame_id: "frame-1".into(),
        kind: ObjectKind::Control,
        role,
        document_bounds: Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 24.0,
        },
        viewport_bounds: None,
        visibility: Visibility::Visible,
        name: Some(id.into()),
        description: None,
        text: None,
        state: state
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
        affordances: BTreeSet::from([affordance]),
        transition: Transition::None,
        action_token: Some(format!("{:a<43}", id)),
        loop_class_token: None,
        protected: false,
    }
}

fn snapshot(revision: u64, objects: Vec<ObservedObject>) -> ObservationSnapshot {
    ObservationSnapshot {
        schema: OBSERVATION_SCHEMA.into(),
        browser_instance_id: "browser-1".into(),
        tab_id: "tab-1".into(),
        document_id: "document-1".into(),
        revision,
        viewport_revision: 1,
        frames: vec![FrameObservation {
            frame_id: "frame-1".into(),
            parent_frame_id: None,
            document_id: "document-1".into(),
            origin: "https://fixture.test".into(),
            status: FrameStatus::Observed,
        }],
        objects,
        changes: vec![],
        coverage: ObservationCoverage {
            source: "dom_extension".into(),
            observed_frame_count: 1,
            restricted_frame_count: 0,
            truncated: false,
        },
        limitations: vec![],
        gap: false,
    }
}

fn prepared(target: &ObservedObject, operation: ActionOperation) -> PreparedAction {
    PreparedAction {
        browser_instance_id: "browser-1".into(),
        tab_id: "tab-1".into(),
        document_id: "document-1".into(),
        basis_revision: 1,
        viewport_revision: 1,
        object_id: target.object_id.clone(),
        action_token: target.action_token.clone().unwrap(),
        operation,
        screen_bounds: target.document_bounds,
        visible: true,
        topmost: true,
        focus_verified: true,
        selection_index: (operation == ActionOperation::Select).then_some(1),
    }
}

fn request(
    target: &ObservedObject,
    operation: ActionOperation,
    payload: ActionPayload,
) -> ActionRequest {
    ActionRequest {
        browser_instance_id: "browser-1".into(),
        tab_id: "tab-1".into(),
        document_id: "document-1".into(),
        basis_revision: 1,
        action_token: target.action_token.clone().unwrap(),
        operation,
        payload,
    }
}

fn run(
    before: ObservationSnapshot,
    after: ObservationSnapshot,
    request: ActionRequest,
    prepared: PreparedAction,
) -> (ActionReceipt, Vec<NativePrimitive>) {
    let mut engine = ClosedLoopEngine::builtin().unwrap();
    let mut native = Native {
        calls: vec![],
        status: DispatchStatus::AcceptedByOs,
    };
    let current = before.clone();
    let receipt = engine
        .execute(
            &request,
            &before,
            &prepared,
            &mut native,
            &mut source(Some(current), Some(after)),
        )
        .unwrap();
    (receipt, native.calls)
}

#[test]
fn button_click_requires_a_semantic_effect() {
    let before_target = object(
        "button",
        SemanticRole::Button,
        Affordance::Click,
        &[("enabled", "true"), ("pressed", "false")],
    );
    let mut after_target = before_target.clone();
    after_target.state.insert("pressed".into(), "true".into());
    let before = snapshot(1, vec![before_target.clone()]);
    let after = snapshot(2, vec![after_target]);
    let (receipt, calls) = run(
        before,
        after,
        request(&before_target, ActionOperation::Click, ActionPayload::None),
        prepared(&before_target, ActionOperation::Click),
    );
    assert_eq!(receipt.postcondition, PostconditionStatus::Verified);
    assert_eq!(calls, vec![NativePrimitive::PrimaryClick]);
}

#[test]
fn text_field_verifies_has_value_without_disclosing_contents() {
    let before_target = object(
        "field",
        SemanticRole::TextField,
        Affordance::Type,
        &[("enabled", "true"), ("has_value", "false")],
    );
    let mut after_target = before_target.clone();
    after_target.state.insert("has_value".into(), "true".into());
    let (receipt, calls) = run(
        snapshot(1, vec![before_target.clone()]),
        snapshot(2, vec![after_target]),
        request(
            &before_target,
            ActionOperation::Type,
            ActionPayload::Text {
                text: "sentinel-secret".into(),
            },
        ),
        prepared(&before_target, ActionOperation::Type),
    );
    assert_eq!(receipt.postcondition, PostconditionStatus::Verified);
    let wire = serde_json::to_string(&receipt).unwrap();
    assert!(!wire.contains("sentinel-secret"));
    assert_eq!(calls, vec![NativePrimitive::UnicodeText]);
}

#[test]
fn editable_family_reuses_native_type_and_redacted_has_value_verification() {
    for (role, id) in [
        (SemanticRole::SearchField, "search"),
        (SemanticRole::TextArea, "notes"),
        (SemanticRole::ContentEditable, "draft"),
        (SemanticRole::SpinButton, "quantity"),
    ] {
        let before_target = object(
            id,
            role,
            Affordance::Type,
            &[("enabled", "true"), ("has_value", "false")],
        );
        let mut after_target = before_target.clone();
        after_target.state.insert("has_value".into(), "true".into());
        let supplied = if role == SemanticRole::SpinButton {
            "7319"
        } else {
            "editable-sentinel-Ω"
        };
        let (receipt, calls) = run(
            snapshot(1, vec![before_target.clone()]),
            snapshot(2, vec![after_target]),
            request(
                &before_target,
                ActionOperation::Type,
                ActionPayload::Text {
                    text: supplied.into(),
                },
            ),
            prepared(&before_target, ActionOperation::Type),
        );
        assert_eq!(receipt.postcondition, PostconditionStatus::Verified);
        assert!(!serde_json::to_string(&receipt).unwrap().contains(supplied));
        assert_eq!(calls, vec![NativePrimitive::UnicodeText]);
    }
}

#[test]
fn checkbox_verifies_checked_transition() {
    let before_target = object(
        "check",
        SemanticRole::Checkbox,
        Affordance::Click,
        &[("enabled", "true"), ("checked", "false")],
    );
    let mut after_target = before_target.clone();
    after_target.state.insert("checked".into(), "true".into());
    let (receipt, _) = run(
        snapshot(1, vec![before_target.clone()]),
        snapshot(2, vec![after_target]),
        request(&before_target, ActionOperation::Click, ActionPayload::None),
        prepared(&before_target, ActionOperation::Click),
    );
    assert_eq!(receipt.postcondition, PostconditionStatus::Verified);
}

#[test]
fn select_verifies_the_named_option_identity() {
    let select = object(
        "select",
        SemanticRole::Select,
        Affordance::Select,
        &[("enabled", "true"), ("has_value", "false")],
    );
    let mut option = object(
        "option",
        SemanticRole::Option,
        Affordance::Focus,
        &[("selected", "false"), ("enabled", "true")],
    );
    option.affordances.clear();
    option.action_token = None;
    let mut after_option = option.clone();
    after_option.state.insert("selected".into(), "true".into());
    let payload = ActionPayload::Select {
        option_object_id: option.object_id.clone(),
    };
    let (receipt, calls) = run(
        snapshot(1, vec![select.clone(), option]),
        snapshot(2, vec![select.clone(), after_option]),
        request(&select, ActionOperation::Select, payload),
        prepared(&select, ActionOperation::Select),
    );
    assert_eq!(receipt.postcondition, PostconditionStatus::Verified);
    assert_eq!(calls, vec![NativePrimitive::SelectOption]);
}

#[test]
fn stale_preparation_and_replay_fail_before_second_native_dispatch() {
    let target = object(
        "check",
        SemanticRole::Checkbox,
        Affordance::Click,
        &[("enabled", "true"), ("checked", "false")],
    );
    let before = snapshot(1, vec![target.clone()]);
    let mut changed = target.clone();
    changed.state.insert("checked".into(), "true".into());
    let after = snapshot(2, vec![changed]);
    let req = request(&target, ActionOperation::Click, ActionPayload::None);
    let prep = prepared(&target, ActionOperation::Click);
    let mut engine = ClosedLoopEngine::builtin().unwrap();
    let mut native = Native {
        calls: vec![],
        status: DispatchStatus::AcceptedByOs,
    };
    engine
        .execute(
            &req,
            &before,
            &prep,
            &mut native,
            &mut source(Some(before.clone()), Some(after)),
        )
        .unwrap();
    assert_eq!(
        engine.execute(&req, &before, &prep, &mut native, &mut source(None, None)),
        Err(ClosedLoopError::InvalidToken)
    );
    assert_eq!(native.calls.len(), 1);

    let mut changed_during_prepare = before.clone();
    changed_during_prepare.revision = 2;
    assert_eq!(
        ClosedLoopEngine::builtin().unwrap().execute(
            &req,
            &before,
            &prep,
            &mut native,
            &mut source(Some(changed_during_prepare), None)
        ),
        Err(ClosedLoopError::Stale)
    );
    assert_eq!(native.calls.len(), 1);

    let mut stale = req;
    stale.basis_revision = 2;
    assert_eq!(
        ClosedLoopEngine::builtin().unwrap().execute(
            &stale,
            &before,
            &prep,
            &mut native,
            &mut source(None, None)
        ),
        Err(ClosedLoopError::Stale)
    );
}

#[test]
fn protected_text_and_lost_focus_fail_closed() {
    let mut target = object(
        "field",
        SemanticRole::TextField,
        Affordance::Type,
        &[("enabled", "true"), ("has_value", "false")],
    );
    target.protected = true;
    let req = request(
        &target,
        ActionOperation::Type,
        ActionPayload::Text {
            text: "never".into(),
        },
    );
    let mut prep = prepared(&target, ActionOperation::Type);
    let mut native = Native {
        calls: vec![],
        status: DispatchStatus::AcceptedByOs,
    };
    assert_eq!(
        ClosedLoopEngine::builtin().unwrap().execute(
            &req,
            &snapshot(1, vec![target.clone()]),
            &prep,
            &mut native,
            &mut source(None, None)
        ),
        Err(ClosedLoopError::Registry(
            saccade_control_sdk::RegistryError::ProtectedValue
        ))
    );
    target.protected = false;
    prep.focus_verified = false;
    assert_eq!(
        ClosedLoopEngine::builtin().unwrap().execute(
            &req,
            &snapshot(1, vec![target]),
            &prep,
            &mut native,
            &mut source(None, None)
        ),
        Err(ClosedLoopError::InvalidPreparation)
    );
    assert!(native.calls.is_empty());
}
