use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ObservationSnapshot, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionOperation {
    Click,
    Hover,
    Focus,
    Type,
    Scroll,
    Drag,
    Select,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionPayload {
    None,
    Text { text: String },
    Scroll { delta_x: f64, delta_y: f64 },
    Drag { delta_x: f64, delta_y: f64 },
    Select { option_object_id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRequest {
    pub browser_instance_id: String,
    pub tab_id: String,
    pub document_id: String,
    pub basis_revision: u64,
    pub action_token: String,
    pub operation: ActionOperation,
    pub payload: ActionPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ActionValidationError {
    #[error("action identity or token is invalid")]
    InvalidIdentity,
    #[error("action operation and payload do not match")]
    PayloadMismatch,
    #[error("action payload exceeds its limit")]
    PayloadLimit,
    #[error("loop limits are outside the allowed range")]
    InvalidLoopLimits,
}

impl ActionRequest {
    pub fn validate(&self) -> Result<(), ActionValidationError> {
        if [&self.browser_instance_id, &self.tab_id, &self.document_id]
            .iter()
            .any(|value| {
                value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
            })
            || !(32..=512).contains(&self.action_token.len())
            || !self
                .action_token
                .bytes()
                .all(|byte| byte.is_ascii_graphic())
            || self.basis_revision == 0
        {
            return Err(ActionValidationError::InvalidIdentity);
        }
        let payload_matches = matches!(
            (&self.operation, &self.payload),
            (
                ActionOperation::Click | ActionOperation::Hover | ActionOperation::Focus,
                ActionPayload::None
            ) | (ActionOperation::Type, ActionPayload::Text { .. })
                | (ActionOperation::Scroll, ActionPayload::Scroll { .. })
                | (ActionOperation::Drag, ActionPayload::Drag { .. })
                | (ActionOperation::Select, ActionPayload::Select { .. })
        );
        if !payload_matches {
            return Err(ActionValidationError::PayloadMismatch);
        }
        match &self.payload {
            ActionPayload::Text { text } if text.len() > 1024 * 1024 => {
                Err(ActionValidationError::PayloadLimit)
            }
            ActionPayload::Scroll { delta_x, delta_y }
            | ActionPayload::Drag { delta_x, delta_y }
                if !delta_x.is_finite()
                    || !delta_y.is_finite()
                    || delta_x.abs() > 100_000.0
                    || delta_y.abs() > 100_000.0 =>
            {
                Err(ActionValidationError::PayloadLimit)
            }
            ActionPayload::Select { option_object_id }
                if option_object_id.is_empty() || option_object_id.len() > 256 =>
            {
                Err(ActionValidationError::PayloadLimit)
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedAction {
    pub browser_instance_id: String,
    pub tab_id: String,
    pub document_id: String,
    pub basis_revision: u64,
    pub viewport_revision: u64,
    pub object_id: String,
    pub action_token: String,
    pub operation: ActionOperation,
    pub screen_bounds: Rect,
    pub visible: bool,
    pub topmost: bool,
    pub focus_verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_index: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchStatus {
    AcceptedByOs,
    AcceptedBySoftware,
    StaleBeforeDispatch,
    PermissionRequired,
    FocusMismatch,
    Unsupported,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostconditionStatus {
    Verified,
    VisibleStateUnchanged,
    Unverified,
    TargetInvalidated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionReceipt {
    pub browser_instance_id: String,
    pub tab_id: String,
    pub document_id: String,
    pub basis_revision: u64,
    pub prepared_revision: u64,
    pub post_revision: u64,
    pub action_token: String,
    pub operation: ActionOperation,
    pub dispatch_status: DispatchStatus,
    pub postcondition: PostconditionStatus,
    pub settled: bool,
    pub post_action_observation: ObservationSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopLimits {
    pub timeout_ms: u32,
    pub max_actions: u32,
    pub min_interval_ms: u32,
}

impl Default for LoopLimits {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000,
            max_actions: 500,
            min_interval_ms: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopStartRequest {
    pub browser_instance_id: String,
    pub tab_id: String,
    pub document_id: String,
    pub basis_revision: u64,
    pub prototype_action_token: String,
    pub operation: ActionOperation,
    #[serde(default)]
    pub start_action: bool,
    #[serde(default)]
    pub limits: LoopLimits,
}

impl LoopStartRequest {
    pub fn validate(&self) -> Result<(), ActionValidationError> {
        if self.operation != ActionOperation::Click {
            return Err(ActionValidationError::PayloadMismatch);
        }
        ActionRequest {
            browser_instance_id: self.browser_instance_id.clone(),
            tab_id: self.tab_id.clone(),
            document_id: self.document_id.clone(),
            basis_revision: self.basis_revision,
            action_token: self.prototype_action_token.clone(),
            operation: self.operation,
            payload: ActionPayload::None,
        }
        .validate()?;
        if self.limits.timeout_ms == 0
            || self.limits.timeout_ms > 30_000
            || self.limits.max_actions == 0
            || self.limits.max_actions > 500
            || self.limits.min_interval_ms > 5_000
        {
            return Err(ActionValidationError::InvalidLoopLimits);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopState {
    Running,
    Completed,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopActionReceipt {
    pub sequence: u32,
    pub basis_revision: u64,
    pub post_revision: u64,
    pub action_token: String,
    pub dispatch_status: DispatchStatus,
    pub observation_to_dispatch_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoopReport {
    pub loop_id: String,
    pub state: LoopState,
    pub actions: u32,
    pub failures: u32,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub max_latency_ms: f64,
    #[serde(default)]
    pub receipts: Vec<LoopActionReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    Requested,
    InProgress,
    Complete,
    Cancelled,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DownloadRecord {
    pub download_id: String,
    pub browser_instance_id: String,
    pub tab_id: String,
    pub state: DownloadState,
    pub filename: String,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}
