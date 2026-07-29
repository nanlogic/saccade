use std::collections::{BTreeMap, BTreeSet};

use saccade_protocol::*;

fn snapshot() -> ObservationSnapshot {
    ObservationSnapshot {
        schema: OBSERVATION_SCHEMA.into(),
        browser_instance_id: "browser-1".into(),
        tab_id: "tab-1".into(),
        document_id: "document-1".into(),
        revision: 1,
        viewport_revision: 1,
        frames: vec![FrameObservation {
            frame_id: "frame-1".into(),
            parent_frame_id: None,
            document_id: "document-1".into(),
            origin: "https://example.test".into(),
            status: FrameStatus::Observed,
        }],
        objects: vec![ObservedObject {
            object_id: "object-1".into(),
            object_revision: 1,
            frame_id: "frame-1".into(),
            kind: ObjectKind::Control,
            role: SemanticRole::TextField,
            document_bounds: Rect {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
            },
            viewport_bounds: None,
            visibility: Visibility::Visible,
            name: Some("Email".into()),
            description: None,
            text: None,
            state: BTreeMap::from([("has_value".into(), "false".into())]),
            affordances: BTreeSet::from([Affordance::Type]),
            transition: Transition::None,
            action_token: Some("a".repeat(43)),
            protected: false,
            loop_class_token: None,
        }],
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

#[test]
fn canonical_observation_is_valid_and_strict() {
    snapshot().validate().unwrap();
    let mut value = serde_json::to_value(snapshot()).unwrap();
    value["selector"] = serde_json::json!("#email");
    assert!(serde_json::from_value::<ObservationSnapshot>(value).is_err());
}

#[test]
fn values_and_unknown_state_fail_closed() {
    let mut value = snapshot();
    value.objects[0]
        .state
        .insert("current_value".into(), "sentinel".into());
    assert_eq!(value.validate(), Err(ObservationError::ValueExposed));
}

#[test]
fn structural_text_is_non_actionable() {
    let mut value = snapshot();
    let object = &mut value.objects[0];
    object.kind = ObjectKind::Text;
    object.role = SemanticRole::Heading;
    object.name = None;
    object.text = Some("Account settings".into());
    object.state = BTreeMap::from([("level".into(), "2".into())]);
    object.affordances.clear();
    object.action_token = None;
    value.validate().unwrap();

    value.objects[0].affordances.insert(Affordance::Click);
    assert_eq!(value.validate(), Err(ObservationError::InvalidActionToken));
}

#[test]
fn migrated_canonical_fixture_is_valid() {
    let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
    let observation: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
    observation.validate().unwrap();
}
