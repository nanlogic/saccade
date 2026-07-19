use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{CssPoint, CssRect, Ns, ViewportInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameObservation {
    pub frame_id: u64,
    pub t_paint_ns: Ns,
    pub t_readback_ns: Ns,
    pub viewport: ViewportInfo,
    pub game_area_css: CssRect,
    pub pixels: PixelRegion,
    pub dom_rects: Option<Vec<DomRectObs>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PixelRegion {
    pub w: u32,
    pub h: u32,
    pub rgba: Arc<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomRectObs {
    pub label: String,
    pub rect_css: CssRect,
    pub t_obs_ns: Ns,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TargetId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TabId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabOwner {
    Human,
    Agent,
}

impl Serialize for TabOwner {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::Human => "human",
            Self::Agent => "agent",
        })
    }
}

impl<'de> Deserialize<'de> for TabOwner {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match canonical_token(&value).as_str() {
            "human" => Ok(Self::Human),
            "agent" => Ok(Self::Agent),
            _ => Err(de::Error::unknown_variant(&value, &["human", "agent"])),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadGrant {
    None,
    VisibleSummaryOnly,
    FullTruth,
}

impl Serialize for ReadGrant {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::None => "none",
            Self::VisibleSummaryOnly => "visible_summary_only",
            Self::FullTruth => "full_truth",
        })
    }
}

impl<'de> Deserialize<'de> for ReadGrant {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match canonical_token(&value).as_str() {
            "none" => Ok(Self::None),
            "visiblesummaryonly" | "visible_summary_only" => Ok(Self::VisibleSummaryOnly),
            "fulltruth" | "full_truth" => Ok(Self::FullTruth),
            _ => Err(de::Error::unknown_variant(
                &value,
                &["none", "visible_summary_only", "full_truth"],
            )),
        }
    }
}

fn canonical_token(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|ch| *ch != '-' && *ch != ' ')
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabVisualMarker {
    pub border: bool,
    pub badge: String,
    pub color_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub tab_id: TabId,
    pub owner: TabOwner,
    pub url: String,
    pub title: Option<String>,
    pub read_grant: ReadGrant,
    pub page_revision: u64,
    pub visual_marker: TabVisualMarker,
}

impl TabInfo {
    pub fn agent_input_allowed(&self) -> bool {
        self.owner == TabOwner::Agent
    }

    pub fn agent_truth_allowed(&self) -> bool {
        self.owner == TabOwner::Agent || self.read_grant != ReadGrant::None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TargetSource {
    PixelDetector,
    DomRect,
    CanvasObserve,
    Fused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetCandidate {
    pub center_css: CssPoint,
    pub bbox_css: CssRect,
    pub radius_css: f32,
    pub source: TargetSource,
    pub confidence: f32,
    pub evidence: TargetEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TargetEvidence {
    PixelComponent {
        area_px: u32,
        fill_ratio: f32,
        contrast: f32,
        temporal_delta: f32,
    },
    DomBox {
        label: String,
    },
    CanvasDraw {
        kind: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedTarget {
    pub id: TargetId,
    pub frame_id: u64,
    pub first_seen_ns: Ns,
    pub last_seen_ns: Ns,
    pub center_css: CssPoint,
    pub bbox_css: CssRect,
    pub radius_css: f32,
    pub confidence: f32,
    pub source: TargetSource,
    pub clicked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameFrameReport {
    pub frame_id: u64,
    pub t_report_ns: Ns,
    pub game_area_css: CssRect,
    pub targets: Vec<RenderedTarget>,
    pub detector_ms: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MotorAction {
    Click {
        target_id: TargetId,
        point_css: CssPoint,
        frame_id: u64,
    },
    Noop {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputBackendKind {
    ServoInternal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickReceipt {
    pub click_id: u64,
    pub target_id: TargetId,
    pub point_css: CssPoint,
    pub frame_id: u64,
    pub t_target_first_seen_ns: Ns,
    pub t_decided_ns: Ns,
    pub t_move_sent_ns: Ns,
    pub t_down_sent_ns: Ns,
    pub t_up_sent_ns: Ns,
    pub backend: InputBackendKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClickOutcome {
    Hit,
    Miss,
    Unknown,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrackerEvent {
    Appeared { target: RenderedTarget },
    Updated { target: RenderedTarget },
    Disappeared { target_id: TargetId, t_obs_ns: Ns },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub click_id: u64,
    pub target_id: TargetId,
    pub outcome: ClickOutcome,
    pub t_verified_ns: Ns,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreState {
    pub hits: u32,
    pub misses: u32,
    pub time_remaining_s: Option<f32>,
    pub finished: bool,
    pub t_obs_ns: Ns,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifficultyConfig {
    pub spawn_speed: String,
    pub target_size: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCounters {
    pub hits: u32,
    pub misses: u32,
    pub targets_seen: u32,
    pub clicks_sent: u32,
    pub unknown_verifications: u32,
    pub false_positive_clicks: u32,
    pub stale_clicks: u32,
    pub expired_unclicked: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccuracySummary {
    pub median_click_error_css_px: f32,
    pub max_click_error_css_px: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorUsage {
    #[serde(rename = "PixelDetector")]
    pub pixel_detector: u32,
    #[serde(rename = "DomRect")]
    pub dom_rect: u32,
    #[serde(rename = "CanvasObserve")]
    pub canvas_observe: u32,
    #[serde(rename = "Fused")]
    pub fused: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_owner_and_read_grant_accept_case_variants_but_emit_canonical_lowercase() {
        let info: TabInfo = serde_json::from_value(serde_json::json!({
            "tab_id": 7,
            "owner": "AGENT",
            "url": "https://example.com",
            "title": "Example",
            "read_grant": "FullTruth",
            "page_revision": 3,
            "visual_marker": {
                "border": true,
                "badge": "Agent",
                "color_name": "green"
            }
        }))
        .unwrap();
        assert_eq!(info.owner, TabOwner::Agent);
        assert_eq!(info.read_grant, ReadGrant::FullTruth);
        let encoded = serde_json::to_value(&info).unwrap();
        assert_eq!(encoded["owner"], "agent");
        assert_eq!(encoded["read_grant"], "full_truth");

        assert_eq!(
            serde_json::from_value::<ReadGrant>(serde_json::json!("FULL_TRUTH")).unwrap(),
            ReadGrant::FullTruth
        );
        assert_eq!(
            serde_json::from_value::<ReadGrant>(serde_json::json!("visible-summary-only")).unwrap(),
            ReadGrant::VisibleSummaryOnly
        );
        assert_eq!(
            serde_json::from_value::<TabOwner>(serde_json::json!("Human")).unwrap(),
            TabOwner::Human
        );
    }

    #[test]
    fn frame_observation_serde_round_trip_preserves_pixels_and_units() {
        let observation = FrameObservation {
            frame_id: 9,
            t_paint_ns: 1_000,
            t_readback_ns: 2_000,
            viewport: ViewportInfo {
                width_css: 1280.0,
                height_css: 800.0,
                device_scale_factor: 2.0,
                page_zoom: 1.0,
            },
            game_area_css: CssRect {
                x: 10.0,
                y: 20.0,
                w: 300.0,
                h: 200.0,
            },
            pixels: PixelRegion {
                w: 2,
                h: 1,
                rgba: Arc::new(vec![255, 0, 0, 255, 0, 255, 0, 255]),
            },
            dom_rects: Some(vec![DomRectObs {
                label: "target tTiny".into(),
                rect_css: CssRect {
                    x: 42.0,
                    y: 84.0,
                    w: 8.0,
                    h: 8.0,
                },
                t_obs_ns: 1_500,
            }]),
        };

        let json = serde_json::to_string(&observation).unwrap();
        let decoded: FrameObservation = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.frame_id, observation.frame_id);
        assert_eq!(decoded.viewport.device_scale_factor, 2.0);
        assert_eq!(decoded.game_area_css.x, 10.0);
        assert_eq!(&*decoded.pixels.rgba, &*observation.pixels.rgba);
        assert_eq!(
            decoded.dom_rects.unwrap()[0].label,
            observation.dom_rects.unwrap()[0].label
        );
    }

    #[test]
    fn motor_action_noop_reason_round_trips() {
        let action = MotorAction::Noop {
            reason: "no fresh targets".into(),
        };

        let json = serde_json::to_string(&action).unwrap();
        let decoded: MotorAction = serde_json::from_str(&json).unwrap();

        match decoded {
            MotorAction::Noop { reason } => assert_eq!(reason, "no fresh targets"),
            MotorAction::Click { .. } => panic!("decoded wrong motor action variant"),
        }
    }
}
