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
        _: &mut dyn FnMut(&ObservationSnapshot) -> bool,
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
        navigation_target: None,
        navigation_disposition: None,
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
        geometry: None,
        frames: vec![FrameObservation {
            frame_id: "frame-1".into(),
            parent_frame_id: None,
            document_id: "document-1".into(),
            document_url: Some("https://fixture.test/page".into()),
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
fn deferred_button_verifies_new_visible_dialog_title() {
    let mut before_target = object(
        "submit",
        SemanticRole::Button,
        Affordance::Click,
        &[("enabled", "true")],
    );
    before_target.transition = Transition::DeferredContentPossible;
    let mut dialog_title = object(
        "dialog-title",
        SemanticRole::Heading,
        Affordance::Click,
        &[("level", "2")],
    );
    dialog_title.kind = ObjectKind::Text;
    dialog_title.affordances.clear();
    dialog_title.action_token = None;
    dialog_title.text = Some("Thanks for submitting the form".into());
    let before = snapshot(1, vec![before_target.clone()]);
    let after = snapshot(2, vec![before_target.clone(), dialog_title]);
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
fn link_requires_a_document_transition() {
    let mut link = object(
        "project-link",
        SemanticRole::Link,
        Affordance::Click,
        &[("enabled", "true")],
    );
    link.kind = ObjectKind::Link;
    link.transition = Transition::NavigationPossible;
    let before = snapshot(1, vec![link.clone()]);
    let mut after = snapshot(2, vec![]);
    after.document_id = "document-2".into();
    after.frames[0].document_id = "document-2".into();
    let (receipt, calls) = run(
        before,
        after,
        request(&link, ActionOperation::Click, ActionPayload::None),
        prepared(&link, ActionOperation::Click),
    );
    assert_eq!(receipt.postcondition, PostconditionStatus::Verified);
    assert_eq!(calls, vec![NativePrimitive::PrimaryClick]);
}

#[test]
fn file_input_verifies_boolean_state_without_receipting_path() {
    let input = object(
        "upload",
        SemanticRole::FileInput,
        Affordance::Upload,
        &[("enabled", "true"), ("has_value", "false")],
    );
    let mut after_input = input.clone();
    after_input.state.insert("has_value".into(), "true".into());
    let supplied = "/tmp/private-release-name.pdf";
    let (receipt, calls) = run(
        snapshot(1, vec![input.clone()]),
        snapshot(2, vec![after_input]),
        request(
            &input,
            ActionOperation::Upload,
            ActionPayload::File {
                path: supplied.into(),
            },
        ),
        prepared(&input, ActionOperation::Upload),
    );
    assert_eq!(receipt.postcondition, PostconditionStatus::Verified);
    assert_eq!(calls, vec![NativePrimitive::FileChooser]);
    assert!(!serde_json::to_string(&receipt).unwrap().contains(supplied));
}

#[test]
fn reflex_target_verifies_only_when_the_same_loop_class_advances() {
    let mut before_target = object(
        "reflex",
        SemanticRole::ReflexTarget,
        Affordance::Click,
        &[("enabled", "true"), ("reflex_occurrence", "0")],
    );
    before_target.name = None;
    before_target.loop_class_token = Some("loop.0123456789abcdef0123456789abcdef0123456789".into());
    let mut after_target = before_target.clone();
    after_target.document_bounds.x += 40.0;
    after_target
        .state
        .insert("reflex_occurrence".into(), "1".into());
    let (receipt, calls) = run(
        snapshot(1, vec![before_target.clone()]),
        snapshot(2, vec![after_target]),
        request(&before_target, ActionOperation::Click, ActionPayload::None),
        prepared(&before_target, ActionOperation::Click),
    );
    assert_eq!(receipt.postcondition, PostconditionStatus::Verified);
    assert_eq!(calls, vec![NativePrimitive::PrimaryClick]);
}

#[test]
fn native_reflex_may_rebase_only_across_semantically_identical_revisions() {
    let mut target = object(
        "reflex-rebase",
        SemanticRole::ReflexTarget,
        Affordance::Click,
        &[("enabled", "true"), ("reflex_occurrence", "0")],
    );
    target.name = None;
    target.loop_class_token = Some("loop.0123456789abcdef0123456789abcdef0123456789".into());
    let before = snapshot(1, vec![target.clone()]);
    let current = snapshot(2, vec![target.clone()]);
    let mut after_target = target.clone();
    after_target
        .state
        .insert("reflex_occurrence".into(), "1".into());
    let mut prep = prepared(&target, ActionOperation::Click);
    prep.basis_revision = 2;
    let mut engine = ClosedLoopEngine::builtin().unwrap();
    let mut native = Native {
        calls: vec![],
        status: DispatchStatus::AcceptedByOs,
    };
    let receipt = engine
        .execute(
            &request(&target, ActionOperation::Click, ActionPayload::None),
            &before,
            &prep,
            &mut native,
            &mut source(Some(current), Some(snapshot(3, vec![after_target]))),
        )
        .unwrap();
    assert_eq!(receipt.postcondition, PostconditionStatus::Verified);
    assert_eq!(native.calls, vec![NativePrimitive::PrimaryClick]);

    let mut changed = target.clone();
    changed
        .state
        .insert("reflex_occurrence".into(), "99".into());
    let mut engine = ClosedLoopEngine::builtin().unwrap();
    let mut native = Native {
        calls: vec![],
        status: DispatchStatus::AcceptedByOs,
    };
    assert_eq!(
        engine.execute(
            &request(&target, ActionOperation::Click, ActionPayload::None),
            &before,
            &prep,
            &mut native,
            &mut source(Some(snapshot(2, vec![changed])), None),
        ),
        Err(ClosedLoopError::Stale)
    );
    assert!(native.calls.is_empty());
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
fn radio_switch_tab_and_menu_item_use_role_specific_transitions() {
    for (role, state_key) in [
        (SemanticRole::Radio, "checked"),
        (SemanticRole::Switch, "checked"),
        (SemanticRole::Tab, "selected"),
        (SemanticRole::MenuItem, "expanded"),
    ] {
        let before_target = object(
            "new-control",
            role,
            Affordance::Click,
            &[("enabled", "true"), (state_key, "false")],
        );
        let mut after_target = before_target.clone();
        after_target.state.insert(state_key.into(), "true".into());
        let (receipt, calls) = run(
            snapshot(1, vec![before_target.clone()]),
            snapshot(2, vec![after_target]),
            request(&before_target, ActionOperation::Click, ActionPayload::None),
            prepared(&before_target, ActionOperation::Click),
        );
        assert_eq!(receipt.postcondition, PostconditionStatus::Verified);
        assert_eq!(calls, vec![NativePrimitive::PrimaryClick]);
    }
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
