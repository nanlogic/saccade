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
        geometry: Some(ObservationGeometry {
            unit: "css_px".into(),
            coordinate_space: "content_viewport".into(),
            viewport_width: 1200.0,
            viewport_height: 665.0,
            scroll_x: 0.0,
            scroll_y: 412.0,
            device_pixel_ratio: 2.0,
            viewport_revision: 18,
        }),
        frames: vec![FrameObservation {
            frame_id: "frame-1".into(),
            parent_frame_id: None,
            document_id: "document-1".into(),
            document_url: Some("https://example.test/page?q=1#section".into()),
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
            navigation_target: None,
            navigation_disposition: None,
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
fn link_navigation_target_is_http_semantic_truth_only() {
    let mut value = snapshot();
    let object = &mut value.objects[0];
    object.kind = ObjectKind::Link;
    object.role = SemanticRole::Link;
    object.affordances = BTreeSet::from([Affordance::Click]);
    object.transition = Transition::NavigationPossible;
    object.navigation_target = Some("https://example.test/source?q=darkwood".into());
    value.validate().unwrap();

    value.objects[0].navigation_target = Some("javascript:alert(1)".into());
    assert_eq!(
        value.validate(),
        Err(ObservationError::InvalidNavigationTarget)
    );

    value.objects[0].navigation_target = Some("https://user:secret@example.test/".into());
    assert_eq!(
        value.validate(),
        Err(ObservationError::InvalidNavigationTarget)
    );

    value.objects[0].navigation_target = Some("https://example.test/source".into());
    value.objects[0].role = SemanticRole::Button;
    value.objects[0].kind = ObjectKind::Control;
    assert_eq!(
        value.validate(),
        Err(ObservationError::InvalidNavigationTarget)
    );
}

#[test]
fn source_delta_must_reference_consistent_current_truth() {
    let mut value = snapshot();
    value.changes.push(ObservationChange {
        kind: ChangeKind::Updated,
        object_id: "missing-object".into(),
        object_revision: 1,
    });
    assert_eq!(value.validate(), Err(ObservationError::InvalidChanges));

    let mut value = snapshot();
    value.changes.push(ObservationChange {
        kind: ChangeKind::Disappeared,
        object_id: "object-1".into(),
        object_revision: 1,
    });
    assert_eq!(value.validate(), Err(ObservationError::InvalidChanges));
}

#[test]
fn migrated_canonical_fixture_is_valid() {
    let fixture = include_str!("../../../tests/protocol/canonical_observation.json");
    let observation: ObservationSnapshot = serde_json::from_str(fixture).unwrap();
    observation.validate().unwrap();
}

#[test]
fn document_url_round_trips_with_path_query_and_fragment() {
    // Anchor verification compares the full URL, so the fragment must survive
    // the wire exactly. A dropped fragment would silently break same-document
    // link verification.
    let encoded = serde_json::json!({
        "frame_id": "frame-1",
        "document_id": "document-1",
        "document_url": "https://example.test/docs/page?q=1&r=2#section-3",
        "origin": "https://example.test",
        "status": "observed"
    });
    let frame: FrameObservation = serde_json::from_value(encoded.clone()).expect("decodes");
    assert_eq!(
        frame.document_url.as_deref(),
        Some("https://example.test/docs/page?q=1&r=2#section-3")
    );
    assert_eq!(serde_json::to_value(&frame).expect("encodes"), encoded);
}

#[test]
fn document_url_is_additive_and_may_be_absent() {
    // saccade.observation/1 is unchanged: a frame without the field still
    // decodes, and a restricted frame has no observable URL.
    let frame: FrameObservation = serde_json::from_value(serde_json::json!({
        "frame_id": "restricted.frame-2",
        "document_id": "restricted.frame-2",
        "origin": "",
        "status": "restricted_permission"
    }))
    .expect("decodes without document_url");
    assert_eq!(frame.document_url, None);
    let reencoded = serde_json::to_value(&frame).expect("encodes");
    assert!(reencoded.get("document_url").is_none(), "absent field must not be emitted");
}

#[test]
fn geometry_is_additive_and_reports_the_css_viewport_directly() {
    // An Agent must never have to infer the viewport from a full-width element.
    let encoded = serde_json::json!({
        "unit": "css_px",
        "coordinate_space": "content_viewport",
        "viewport_width": 1200.0,
        "viewport_height": 665.0,
        "scroll_x": 0.0,
        "scroll_y": 412.0,
        "device_pixel_ratio": 2.0,
        "viewport_revision": 18
    });
    let geometry: ObservationGeometry = serde_json::from_value(encoded.clone()).expect("decodes");
    assert_eq!(geometry.unit, "css_px");
    assert_eq!(geometry.coordinate_space, "content_viewport");
    assert_eq!(geometry.viewport_width, 1200.0);
    assert_eq!(serde_json::to_value(&geometry).expect("encodes"), encoded);
}

#[test]
fn a_snapshot_without_geometry_still_decodes() {
    // saccade.observation/1 is unchanged; geometry is additive.
    let mut value = serde_json::to_value(snapshot()).expect("encodes");
    value.as_object_mut().expect("object").remove("geometry");
    let snapshot: ObservationSnapshot = serde_json::from_value(value).expect("decodes");
    assert!(snapshot.geometry.is_none());
}

#[test]
fn navigation_disposition_is_additive_and_only_present_when_unverifiable() {
    // Absent means an ordinary same-context navigation Saccade can verify.
    let mut plain = snapshot();
    plain.objects[0].navigation_disposition = None;
    let encoded = serde_json::to_value(&plain.objects[0]).expect("encodes");
    assert!(encoded.get("navigation_disposition").is_none());
    for disposition in ["download", "new_context"] {
        let decoded: ObservedObject = serde_json::from_value(serde_json::json!({
            "object_id": "o1",
            "frame_id": "frame-1",
            "object_revision": 1,
            "kind": "control",
            "role": "link",
            "document_bounds": {"x":0.0,"y":0.0,"width":10.0,"height":10.0},
            "visibility": "visible",
            "navigation_target": "https://example.test/file",
            "navigation_disposition": disposition,
            "affordances": ["click"],
            "transition": "navigation_possible",
            "protected": false
        }))
        .expect("decodes");
        assert_eq!(decoded.navigation_disposition.as_deref(), Some(disposition));
    }
}
