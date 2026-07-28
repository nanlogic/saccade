//! Shared closed-loop authority used by the separate native-host and MCP modes.

#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::BTreeSet;

use saccade_control_sdk::{verify_with_documents, NativePrimitive, Registry, RegistryError};
use saccade_protocol::{
    ActionPayload, ActionReceipt, ActionRequest, ActionValidationError, DispatchStatus,
    ObservationError, ObservationSnapshot, PostconditionStatus, PreparedAction, Visibility,
};
use thiserror::Error;

pub mod mcp;
pub mod native_messaging;
pub mod owner_ipc;
pub mod platform_input;
pub mod profile;
pub mod session;

pub trait NativeInput: Send {
    fn execute(
        &mut self,
        primitive: NativePrimitive,
        prepared: &PreparedAction,
        payload: &ActionPayload,
        selection_name: Option<&str>,
    ) -> DispatchStatus;
}

pub trait ObservationSource {
    fn current_observation(&mut self) -> Result<ObservationSnapshot, ClosedLoopError>;
    fn settled_observation(
        &mut self,
        after_revision: u64,
    ) -> Result<(ObservationSnapshot, bool), ClosedLoopError>;
}

#[derive(Debug, Error, PartialEq)]
pub enum ClosedLoopError {
    #[error(transparent)]
    Action(#[from] ActionValidationError),
    #[error(transparent)]
    Observation(#[from] ObservationError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("request identity or revision is stale")]
    Stale,
    #[error("action token is missing, mismatched, or already used")]
    InvalidToken,
    #[error(
        "prepared action failed identity, focus, geometry, visibility, or topmost revalidation"
    )]
    InvalidPreparation,
    #[error("settled observation source failed: {0}")]
    ObservationSource(String),
}

pub struct ClosedLoopEngine {
    registry: Registry,
    consumed_tokens: BTreeSet<String>,
}

impl ClosedLoopEngine {
    pub fn new(registry: Registry) -> Self {
        Self {
            registry,
            consumed_tokens: BTreeSet::new(),
        }
    }

    pub fn builtin() -> Result<Self, RegistryError> {
        Ok(Self::new(Registry::builtin()?))
    }

    pub fn execute(
        &mut self,
        request: &ActionRequest,
        before: &ObservationSnapshot,
        prepared: &PreparedAction,
        native: &mut dyn NativeInput,
        observations: &mut dyn ObservationSource,
    ) -> Result<ActionReceipt, ClosedLoopError> {
        request.validate()?;
        before.validate()?;
        if request.browser_instance_id != before.browser_instance_id
            || request.tab_id != before.tab_id
            || request.document_id != before.document_id
            || request.basis_revision != before.revision
        {
            return Err(ClosedLoopError::Stale);
        }
        if self.consumed_tokens.contains(&request.action_token) {
            return Err(ClosedLoopError::InvalidToken);
        }
        let target = before
            .objects
            .iter()
            .find(|object| object.action_token.as_deref() == Some(&request.action_token))
            .ok_or(ClosedLoopError::InvalidToken)?;
        let module = self.registry.resolve(target, request.operation)?;
        if target.visibility == Visibility::Hidden
            || prepared.browser_instance_id != request.browser_instance_id
            || prepared.tab_id != request.tab_id
            || prepared.document_id != request.document_id
            || prepared.basis_revision != request.basis_revision
            || prepared.viewport_revision != before.viewport_revision
            || prepared.object_id != target.object_id
            || prepared.action_token != request.action_token
            || prepared.operation != request.operation
            || !prepared.screen_bounds.is_valid()
            || prepared.screen_bounds.width == 0.0
            || prepared.screen_bounds.height == 0.0
            || !prepared.visible
            || !prepared.topmost
            || !prepared.focus_verified
            || (request.operation == saccade_protocol::ActionOperation::Select
                && prepared.selection_index.is_none())
        {
            return Err(ClosedLoopError::InvalidPreparation);
        }

        // Preparing may scroll, focus, and yield to the browser. Recheck the
        // authoritative identity and revision immediately before native input.
        let current = observations.current_observation()?;
        current.validate()?;
        let target_is_current = current.objects.iter().any(|object| {
            object.object_id == target.object_id
                && object.action_token.as_deref() == Some(&request.action_token)
                && object.affordances == target.affordances
        });
        if current.browser_instance_id != request.browser_instance_id
            || current.tab_id != request.tab_id
            || current.document_id != request.document_id
            || current.revision != request.basis_revision
            || current.viewport_revision != prepared.viewport_revision
            || !target_is_current
        {
            return Err(ClosedLoopError::Stale);
        }

        // Single use begins immediately before the only side effect.
        self.consumed_tokens.insert(request.action_token.clone());
        let selection_name = match &request.payload {
            ActionPayload::Select { option_object_id } => before
                .objects
                .iter()
                .find(|object| object.object_id == *option_object_id)
                .and_then(|object| object.name.as_deref()),
            _ => None,
        };
        let dispatch_status = native.execute(
            module.native_primitive,
            prepared,
            &request.payload,
            selection_name,
        );
        let (after, observed_settled) = observations.settled_observation(request.basis_revision)?;
        after.validate()?;
        if after.browser_instance_id != before.browser_instance_id || after.tab_id != before.tab_id
        {
            return Err(ClosedLoopError::Stale);
        }
        let fresh = after.document_id != before.document_id || after.revision > before.revision;
        let postcondition = if dispatch_status == DispatchStatus::AcceptedByOs && fresh {
            verify_with_documents(module, before, target, &request.payload, &after)
        } else if dispatch_status == DispatchStatus::AcceptedByOs {
            PostconditionStatus::VisibleStateUnchanged
        } else {
            PostconditionStatus::Unverified
        };
        Ok(ActionReceipt {
            browser_instance_id: before.browser_instance_id.clone(),
            tab_id: before.tab_id.clone(),
            document_id: before.document_id.clone(),
            basis_revision: before.revision,
            prepared_revision: prepared.basis_revision,
            post_revision: after.revision,
            action_token: request.action_token.clone(),
            operation: request.operation,
            dispatch_status,
            postcondition,
            settled: observed_settled && fresh,
            post_action_observation: after,
        })
    }
}
