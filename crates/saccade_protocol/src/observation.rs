use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::OBSERVATION_SCHEMA;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= 0.0
            && self.height >= 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Affordance {
    Click,
    Hover,
    Focus,
    Type,
    Scroll,
    Drag,
    Select,
    Upload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Text,
    Control,
    Link,
    Image,
    Frame,
    OpaqueSurface,
    RestrictedDocument,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticRole {
    Text,
    Heading,
    Paragraph,
    List,
    ListItem,
    Table,
    Row,
    Cell,
    Alert,
    Status,
    Button,
    Link,
    TextField,
    SearchField,
    TextArea,
    ContentEditable,
    Checkbox,
    Radio,
    Switch,
    Select,
    Option,
    FileInput,
    Slider,
    SpinButton,
    Tab,
    MenuItem,
    Label,
    GenericControl,
    ReflexTarget,
    Image,
    Frame,
    OpaqueSurface,
    RestrictedDocument,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Visible,
    Offscreen,
    Hidden,
    PartiallyOccluded,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    None,
    DeferredContentPossible,
    NavigationPossible,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedObject {
    pub object_id: String,
    pub object_revision: u64,
    pub frame_id: String,
    pub kind: ObjectKind,
    pub role: SemanticRole,
    pub document_bounds: Rect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport_bounds: Option<Rect>,
    pub visibility: Visibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation_target: Option<String>,
    /// Present only when following this link cannot be verified from this
    /// document's URL: `download`, or `new_context` for a link that opens a
    /// new browsing context. Absent means an ordinary same-context navigation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation_disposition: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub state: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub affordances: BTreeSet<Affordance>,
    pub transition: Transition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_class_token: Option<String>,
    pub protected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameStatus {
    Observed,
    RestrictedPermission,
    AmbiguousTransform,
    Unavailable,
}

/// Top-level viewport geometry, read by the Collector in the same collect that
/// produced this revision. It exists so an Agent never has to infer the viewport
/// from a full-width element. `device_pixel_ratio` is descriptive only:
/// `saccade.act` never converts coordinates, and no Saccade execution path may
/// depend on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationGeometry {
    /// Always `css_px`.
    pub unit: String,
    /// Always `content_viewport`: the space `viewport_bounds` are expressed in.
    pub coordinate_space: String,
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub device_pixel_ratio: f64,
    /// Advances only when geometry itself changes. A DOM-only or URL-only
    /// change leaves it untouched.
    pub viewport_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameObservation {
    pub frame_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_frame_id: Option<String>,
    pub document_id: String,
    /// Full document URL including path, query and fragment, read by the
    /// Collector in the same collect that produced this revision. Anchor
    /// navigation verification needs the fragment. Absent for restricted
    /// frames, whose URL the Extension cannot observe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_url: Option<String>,
    pub origin: String,
    pub status: FrameStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitationKind {
    RestrictedFrame,
    AmbiguousFrameTransform,
    ClosedShadowRoot,
    OpaqueCanvas,
    OpaqueWebgl,
    OpaqueVideo,
    BuiltInPdf,
    BrowserRestrictedPage,
    Truncated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limitation {
    pub kind: LimitationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationCoverage {
    pub source: String,
    pub observed_frame_count: u32,
    pub restricted_frame_count: u32,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Appeared,
    Updated,
    Disappeared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationChange {
    pub kind: ChangeKind,
    pub object_id: String,
    pub object_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationSnapshot {
    pub schema: String,
    pub browser_instance_id: String,
    pub tab_id: String,
    pub document_id: String,
    pub revision: u64,
    pub viewport_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<ObservationGeometry>,
    pub frames: Vec<FrameObservation>,
    pub objects: Vec<ObservedObject>,
    #[serde(default)]
    pub changes: Vec<ObservationChange>,
    pub coverage: ObservationCoverage,
    #[serde(default)]
    pub limitations: Vec<Limitation>,
    #[serde(default)]
    pub gap: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ObservationError {
    #[error("wrong observation schema")]
    WrongSchema,
    #[error("missing or oversized identity")]
    InvalidIdentity,
    #[error("revision must be non-zero")]
    InvalidRevision,
    #[error("duplicate object, frame, or token")]
    DuplicateIdentity,
    #[error("invalid geometry")]
    InvalidBounds,
    #[error("protected or editable value was exposed")]
    ValueExposed,
    #[error("object kind and semantic role are inconsistent")]
    InvalidSemanticRole,
    #[error("navigation target is invalid or belongs to a non-link object")]
    InvalidNavigationTarget,
    #[error("action token lacks an affordance")]
    InvalidActionToken,
    #[error("coverage is inconsistent")]
    InvalidCoverage,
    #[error("a gap must carry a full snapshot, not deltas")]
    GapWithChanges,
    #[error("semantic changes are internally inconsistent")]
    InvalidChanges,
    #[error("observation exceeded a protocol limit")]
    LimitExceeded,
}

impl ObservationSnapshot {
    pub const MAX_OBJECTS: usize = 10_000;
    pub const MAX_FRAMES: usize = 512;
    pub const MAX_TEXT_BYTES: usize = 2 * 1024 * 1024;
    pub const MAX_NAME_BYTES: usize = 2_048;
    pub const MAX_DESCRIPTION_BYTES: usize = 4_096;

    pub fn validate(&self) -> Result<(), ObservationError> {
        if self.schema != OBSERVATION_SCHEMA {
            return Err(ObservationError::WrongSchema);
        }
        for identity in [&self.browser_instance_id, &self.tab_id, &self.document_id] {
            validate_identity(identity)?;
        }
        if self.revision == 0 || self.viewport_revision == 0 {
            return Err(ObservationError::InvalidRevision);
        }
        if self.gap && !self.changes.is_empty() {
            return Err(ObservationError::GapWithChanges);
        }
        if self.objects.len() > Self::MAX_OBJECTS || self.frames.len() > Self::MAX_FRAMES {
            return Err(ObservationError::LimitExceeded);
        }
        if self.coverage.source != "dom_extension" {
            return Err(ObservationError::InvalidCoverage);
        }

        let mut frame_ids = BTreeSet::new();
        let mut observed = 0;
        let mut restricted = 0;
        for frame in &self.frames {
            validate_identity(&frame.frame_id)?;
            validate_identity(&frame.document_id)?;
            if !frame_ids.insert(&frame.frame_id) {
                return Err(ObservationError::DuplicateIdentity);
            }
            if frame.status == FrameStatus::Observed {
                observed += 1;
            } else {
                restricted += 1;
            }
        }
        if observed != self.coverage.observed_frame_count
            || restricted != self.coverage.restricted_frame_count
            || self.coverage.truncated
                != self
                    .limitations
                    .iter()
                    .any(|item| item.kind == LimitationKind::Truncated)
        {
            return Err(ObservationError::InvalidCoverage);
        }

        let mut object_ids = BTreeSet::new();
        let mut tokens = BTreeSet::new();
        let mut disclosed_bytes = 0usize;
        for object in &self.objects {
            validate_identity(&object.object_id)?;
            validate_identity(&object.frame_id)?;
            if object.object_revision == 0 || !frame_ids.contains(&object.frame_id) {
                return Err(ObservationError::InvalidIdentity);
            }
            if !object_ids.insert(&object.object_id) {
                return Err(ObservationError::DuplicateIdentity);
            }
            if !object.document_bounds.is_valid()
                || object.viewport_bounds.is_some_and(|rect| !rect.is_valid())
            {
                return Err(ObservationError::InvalidBounds);
            }
            if !role_matches_kind(object.kind, object.role) {
                return Err(ObservationError::InvalidSemanticRole);
            }
            disclosed_bytes += object.name.as_ref().map_or(0, String::len)
                + object.description.as_ref().map_or(0, String::len)
                + object.text.as_ref().map_or(0, String::len);
            if object
                .name
                .as_ref()
                .is_some_and(|value| value.len() > Self::MAX_NAME_BYTES)
                || object
                    .description
                    .as_ref()
                    .is_some_and(|value| value.len() > Self::MAX_DESCRIPTION_BYTES)
            {
                return Err(ObservationError::LimitExceeded);
            }
            if object.kind != ObjectKind::Text && object.text.is_some() {
                return Err(ObservationError::ValueExposed);
            }
            if let Some(target) = &object.navigation_target {
                if object.role != SemanticRole::Link
                    || object.transition != Transition::NavigationPossible
                    || target.len() > 8_192
                    || target.chars().any(char::is_control)
                    || !valid_navigation_target(target)
                {
                    return Err(ObservationError::InvalidNavigationTarget);
                }
                disclosed_bytes += target.len();
            }
            if object.kind == ObjectKind::Text
                && (!object.affordances.is_empty() || object.action_token.is_some())
            {
                return Err(ObservationError::InvalidActionToken);
            }
            for key in object.state.keys() {
                if !allowed_state(key) {
                    return Err(ObservationError::ValueExposed);
                }
            }
            if let Some(token) = &object.action_token {
                validate_token(token)?;
                if object.affordances.is_empty() || !tokens.insert(token) {
                    return Err(ObservationError::InvalidActionToken);
                }
            }
            if let Some(token) = &object.loop_class_token {
                validate_token(token)?;
            }
        }
        if disclosed_bytes > Self::MAX_TEXT_BYTES {
            return Err(ObservationError::LimitExceeded);
        }
        let mut changed_ids = BTreeSet::new();
        for change in &self.changes {
            if !changed_ids.insert(&change.object_id) {
                return Err(ObservationError::InvalidChanges);
            }
            let current = self
                .objects
                .iter()
                .find(|object| object.object_id == change.object_id);
            match change.kind {
                ChangeKind::Appeared | ChangeKind::Updated
                    if current
                        .is_some_and(|object| object.object_revision == change.object_revision) => {
                }
                ChangeKind::Disappeared if current.is_none() && change.object_revision > 0 => {}
                _ => return Err(ObservationError::InvalidChanges),
            }
        }
        Ok(())
    }
}

fn allowed_state(key: &str) -> bool {
    matches!(
        key,
        "has_value"
            | "checked"
            | "enabled"
            | "selected"
            | "expanded"
            | "required"
            | "readonly"
            | "pressed"
            | "current"
            | "invalid"
            | "busy"
            | "modal"
            | "level"
            | "reflex_target"
            | "reflex_occurrence"
    )
}

fn valid_navigation_target(value: &str) -> bool {
    let Some((scheme, remainder)) = value.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "http" | "https") {
        return false;
    }
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    !authority.is_empty() && !authority.contains('@')
}

fn role_matches_kind(kind: ObjectKind, role: SemanticRole) -> bool {
    match kind {
        ObjectKind::Text => matches!(
            role,
            SemanticRole::Text
                | SemanticRole::Heading
                | SemanticRole::Paragraph
                | SemanticRole::List
                | SemanticRole::ListItem
                | SemanticRole::Table
                | SemanticRole::Row
                | SemanticRole::Cell
                | SemanticRole::Alert
                | SemanticRole::Status
        ),
        ObjectKind::Control => matches!(
            role,
            SemanticRole::Button
                | SemanticRole::TextField
                | SemanticRole::SearchField
                | SemanticRole::TextArea
                | SemanticRole::ContentEditable
                | SemanticRole::Checkbox
                | SemanticRole::Radio
                | SemanticRole::Switch
                | SemanticRole::Select
                | SemanticRole::Option
                | SemanticRole::FileInput
                | SemanticRole::Slider
                | SemanticRole::SpinButton
                | SemanticRole::Tab
                | SemanticRole::MenuItem
                | SemanticRole::Label
                | SemanticRole::GenericControl
                | SemanticRole::ReflexTarget
        ),
        ObjectKind::Link => role == SemanticRole::Link,
        ObjectKind::Image => role == SemanticRole::Image,
        ObjectKind::Frame => role == SemanticRole::Frame,
        ObjectKind::OpaqueSurface => role == SemanticRole::OpaqueSurface,
        ObjectKind::RestrictedDocument => role == SemanticRole::RestrictedDocument,
        ObjectKind::Unknown => role == SemanticRole::Unknown,
    }
}

fn validate_identity(value: &str) -> Result<(), ObservationError> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        Err(ObservationError::InvalidIdentity)
    } else {
        Ok(())
    }
}

fn validate_token(value: &str) -> Result<(), ObservationError> {
    if (32..=512).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_graphic()) {
        Ok(())
    } else {
        Err(ObservationError::InvalidActionToken)
    }
}
