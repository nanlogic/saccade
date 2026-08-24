//! Shared closed-loop authority used by the separate native-host and MCP modes.

#![deny(unsafe_op_in_unsafe_fn)]

use std::collections::BTreeSet;

use saccade_control_sdk::{
    verify_strategy_with_documents, InputPolicy, NativePrimitive, Registry, RegistryError,
};
use saccade_protocol::{
    ActionPayload, ActionReceipt, ActionRequest, ActionValidationError, DispatchStatus,
    ObservationError, ObservationSnapshot, PostconditionStatus, PreparedAction, Visibility,
};
use thiserror::Error;

pub mod browser_wake;
pub mod input_policy;
pub mod mcp;
pub mod native_messaging;
pub mod owner_ipc;
pub mod platform_input;
pub mod profile;
pub mod session;

pub trait NativeInput: Send {
    fn requires_physical_hit_testing(&self) -> bool {
        true
    }

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
        sufficient: &mut dyn FnMut(&ObservationSnapshot) -> bool,
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
    #[error("post-dispatch observation identity changed")]
    PostDispatchIdentityMismatch,
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

    pub fn input_policy(
        &self,
        role: saccade_protocol::SemanticRole,
    ) -> Result<InputPolicy, RegistryError> {
        self.registry.input_policy(role)
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
        // Software input performs its authoritative visibility/topmost/focus
        // revalidation inside the Extension immediately after scrolling and
        // immediately before dispatch. Its first prepare is deliberately
        // geometry-passive so scrolling cannot advance the observation and
        // stale the command against itself. Physical input still requires the
        // complete prepared geometry here.
        let software_dispatch = !native.requires_physical_hit_testing();
        let native_reflex_rebase = !software_dispatch
            && target.role == saccade_protocol::SemanticRole::ReflexTarget
            && prepared.basis_revision >= request.basis_revision;
        if (!software_dispatch && target.visibility == Visibility::Hidden)
            || prepared.browser_instance_id != request.browser_instance_id
            || prepared.tab_id != request.tab_id
            || prepared.document_id != request.document_id
            || (!software_dispatch
                && !native_reflex_rebase
                && prepared.basis_revision != request.basis_revision)
            || (software_dispatch && prepared.basis_revision < request.basis_revision)
            || (!software_dispatch
                && !native_reflex_rebase
                && prepared.viewport_revision != before.viewport_revision)
            || prepared.object_id != target.object_id
            || prepared.action_token != request.action_token
            || prepared.operation != request.operation
            || (!software_dispatch && !prepared.screen_bounds.is_valid())
            || (!software_dispatch && prepared.screen_bounds.width == 0.0)
            || (!software_dispatch && prepared.screen_bounds.height == 0.0)
            || (!software_dispatch && !prepared.visible)
            || (!software_dispatch && !prepared.topmost)
            || (!software_dispatch && !prepared.focus_verified)
            || (request.operation == saccade_protocol::ActionOperation::Select
                && prepared.selection_index.is_none())
        {
            return Err(ClosedLoopError::InvalidPreparation);
        }

        // Preparing may scroll, focus, and yield to the browser. Recheck the
        // authoritative identity and revision immediately before input. A
        // physical reflex prepare is itself the Extension's final synchronous
        // identity, topmost, focus, viewport, and geometry revalidation. An
        // additional observation round trip would retain those semantics but
        // dispatch at the previous position of a continuously moving target.
        if !native_reflex_rebase {
            let current = observations.current_observation()?;
            current.validate()?;
            let target_is_current = current.objects.iter().any(|object| {
                object.object_id == target.object_id
                    && object.action_token.as_deref() == Some(&request.action_token)
                    && object.affordances == target.affordances
                    && (!software_dispatch || software_semantics_unchanged(target, object))
            });
            if current.browser_instance_id != request.browser_instance_id
                || current.tab_id != request.tab_id
                || current.document_id != request.document_id
                || (!software_dispatch && current.revision != request.basis_revision)
                || (software_dispatch && current.revision < prepared.basis_revision)
                || (!software_dispatch && current.viewport_revision != prepared.viewport_revision)
                || !target_is_current
            {
                return Err(ClosedLoopError::Stale);
            }
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
        if !matches!(
            dispatch_status,
            DispatchStatus::AcceptedByOs | DispatchStatus::AcceptedBySoftware
        ) {
            // A bounded prepare/dispatch rejection has no accepted side effect.
            // Release the reservation so a machine-readable retry_safe result
            // is truthful; accepted actions remain single-use.
            self.consumed_tokens.remove(&request.action_token);
        }
        let mut sufficient = |candidate: &ObservationSnapshot| {
            verify_strategy_with_documents(
                module.verifier,
                before,
                target,
                &request.payload,
                candidate,
            ) == PostconditionStatus::Verified
        };
        let (after, observed_settled) =
            observations.settled_observation(request.basis_revision, &mut sufficient)?;
        after.validate()?;
        if after.browser_instance_id != before.browser_instance_id || after.tab_id != before.tab_id
        {
            return Err(ClosedLoopError::PostDispatchIdentityMismatch);
        }
        let fresh = after.document_id != before.document_id || after.revision > before.revision;
        let accepted = matches!(
            dispatch_status,
            DispatchStatus::AcceptedByOs | DispatchStatus::AcceptedBySoftware
        );
        let postcondition = if accepted && fresh {
            verify_strategy_with_documents(
                module.verifier,
                before,
                target,
                &request.payload,
                &after,
            )
        } else if accepted {
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
            failure_stage: None,
            failure_code: None,
            retry_safe: None,
            local_wait_ms: None,
            postcondition,
            settled: observed_settled && fresh,
            post_action_observation: after,
        })
    }
}

/// Geometry and visibility may change while the Extension performs its local
/// actionability wait. Every semantic or authority-bearing field must remain
/// identical; software dispatch may never use this allowance to rebind.
fn software_semantics_unchanged(
    before: &saccade_protocol::ObservedObject,
    current: &saccade_protocol::ObservedObject,
) -> bool {
    current.frame_id == before.frame_id
        && current.kind == before.kind
        && current.role == before.role
        && current.name == before.name
        && current.description == before.description
        && current.text == before.text
        && current.navigation_target == before.navigation_target
        && current.navigation_disposition == before.navigation_disposition
        && current.state == before.state
        && current.affordances == before.affordances
        && current.transition == before.transition
        && current.loop_class_token == before.loop_class_token
        && current.protected == before.protected
}
