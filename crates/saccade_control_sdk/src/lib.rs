//! Declarative Control Catalog and audited Registry for bundled control families.

#![deny(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use saccade_protocol::{
    ActionOperation, ActionPayload, Affordance, ObservationSnapshot, ObservedObject,
    PostconditionStatus, SemanticRole,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const BUILTIN_CATALOG: &str = include_str!("../../../catalog/controls.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlCatalog {
    pub catalog_version: u32,
    pub controls: Vec<ControlDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlDefinition {
    pub id: String,
    pub role: SemanticRole,
    pub implementation_family: ImplementationFamily,
    pub safe_state: BTreeSet<String>,
    pub affordances: BTreeSet<Affordance>,
    pub native_primitive: NativePrimitive,
    pub input_policy: InputPolicy,
    pub verifier: Verifier,
    pub limitations: Vec<String>,
    pub fixtures: Vec<String>,
    pub evidence: BrowserEvidence,
    pub publication_status: PublicationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationFamily {
    Button,
    Navigation,
    Reflex,
    Editable,
    Toggle,
    Choice,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativePrimitive {
    PrimaryClick,
    UnicodeText,
    SelectOption,
    FileChooser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputPolicy {
    SoftwarePreferred,
    NativeRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verifier {
    ButtonEffect,
    DocumentTransition,
    TargetAdvanced,
    HasValue,
    CheckedTransition,
    SelectedTransition,
    ExpandedTransition,
    OptionSelected,
    HasFile,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserEvidence {
    pub chrome: EvidenceStatus,
    pub edge: EvidenceStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Pending,
    Passed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationStatus {
    Implementation,
    Publishable,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("invalid catalog: {0}")]
    InvalidCatalog(String),
    #[error("no registered module for role")]
    UnsupportedRole,
    #[error("operation is not advertised by this control")]
    UnsupportedOperation,
    #[error("protected controls reject Agent-provided text")]
    ProtectedValue,
    #[error("control is disabled or readonly")]
    Unavailable,
}

pub struct Registry {
    modules: BTreeMap<SemanticRole, ControlDefinition>,
}

impl Registry {
    pub fn builtin() -> Result<Self, RegistryError> {
        let catalog: ControlCatalog = serde_json::from_str(BUILTIN_CATALOG)
            .map_err(|error| RegistryError::InvalidCatalog(error.to_string()))?;
        Self::from_catalog(catalog)
    }

    pub fn from_catalog(catalog: ControlCatalog) -> Result<Self, RegistryError> {
        if catalog.catalog_version != 1 {
            return Err(RegistryError::InvalidCatalog(
                "catalog_version must be 1".into(),
            ));
        }
        let mut modules = BTreeMap::new();
        for definition in catalog.controls {
            validate_definition(&definition)?;
            if modules.insert(definition.role, definition).is_some() {
                return Err(RegistryError::InvalidCatalog("duplicate role".into()));
            }
        }
        let expected = BTreeSet::from([
            SemanticRole::Button,
            SemanticRole::Link,
            SemanticRole::TextField,
            SemanticRole::SearchField,
            SemanticRole::TextArea,
            SemanticRole::ContentEditable,
            SemanticRole::SpinButton,
            SemanticRole::Checkbox,
            SemanticRole::Radio,
            SemanticRole::Switch,
            SemanticRole::Select,
            SemanticRole::Tab,
            SemanticRole::MenuItem,
            SemanticRole::ReflexTarget,
            SemanticRole::FileInput,
        ]);
        if modules.keys().copied().collect::<BTreeSet<_>>() != expected {
            return Err(RegistryError::InvalidCatalog(
                "Catalog must contain exactly the implemented modules".into(),
            ));
        }
        Ok(Self { modules })
    }

    pub fn resolve(
        &self,
        object: &ObservedObject,
        operation: ActionOperation,
    ) -> Result<&ControlDefinition, RegistryError> {
        let definition = self
            .modules
            .get(&object.role)
            .ok_or(RegistryError::UnsupportedRole)?;
        let affordance =
            operation_affordance(operation).ok_or(RegistryError::UnsupportedOperation)?;
        if !definition.affordances.contains(&affordance)
            || !object.affordances.contains(&affordance)
        {
            return Err(RegistryError::UnsupportedOperation);
        }
        if object.protected && operation == ActionOperation::Type {
            return Err(RegistryError::ProtectedValue);
        }
        if state_is(object, "enabled", false) || state_is(object, "readonly", true) {
            return Err(RegistryError::Unavailable);
        }
        Ok(definition)
    }

    pub fn input_policy(&self, role: SemanticRole) -> Result<InputPolicy, RegistryError> {
        self.modules
            .get(&role)
            .map(|definition| definition.input_policy)
            .ok_or(RegistryError::UnsupportedRole)
    }
}

fn validate_definition(definition: &ControlDefinition) -> Result<(), RegistryError> {
    if definition.id.is_empty() || definition.fixtures.is_empty() {
        return Err(RegistryError::InvalidCatalog(
            "id and fixtures are required".into(),
        ));
    }
    if definition.publication_status == PublicationStatus::Publishable
        && (definition.evidence.chrome != EvidenceStatus::Passed
            || definition.evidence.edge != EvidenceStatus::Passed)
    {
        return Err(RegistryError::InvalidCatalog(
            "publishable requires current Chrome and Edge evidence".into(),
        ));
    }
    let expected = match definition.role {
        SemanticRole::Button => (
            ImplementationFamily::Button,
            NativePrimitive::PrimaryClick,
            Verifier::ButtonEffect,
        ),
        SemanticRole::Link => (
            ImplementationFamily::Navigation,
            NativePrimitive::PrimaryClick,
            Verifier::DocumentTransition,
        ),
        SemanticRole::TextField
        | SemanticRole::SearchField
        | SemanticRole::TextArea
        | SemanticRole::ContentEditable
        | SemanticRole::SpinButton => (
            ImplementationFamily::Editable,
            NativePrimitive::UnicodeText,
            Verifier::HasValue,
        ),
        SemanticRole::Checkbox | SemanticRole::Radio | SemanticRole::Switch => (
            ImplementationFamily::Toggle,
            NativePrimitive::PrimaryClick,
            Verifier::CheckedTransition,
        ),
        SemanticRole::Tab => (
            ImplementationFamily::Navigation,
            NativePrimitive::PrimaryClick,
            Verifier::SelectedTransition,
        ),
        SemanticRole::MenuItem => (
            ImplementationFamily::Navigation,
            NativePrimitive::PrimaryClick,
            Verifier::ExpandedTransition,
        ),
        SemanticRole::Select => (
            ImplementationFamily::Choice,
            NativePrimitive::SelectOption,
            Verifier::OptionSelected,
        ),
        SemanticRole::ReflexTarget => (
            ImplementationFamily::Reflex,
            NativePrimitive::PrimaryClick,
            Verifier::TargetAdvanced,
        ),
        SemanticRole::FileInput => (
            ImplementationFamily::File,
            NativePrimitive::FileChooser,
            Verifier::HasFile,
        ),
        _ => {
            return Err(RegistryError::InvalidCatalog(
                "role has no audited control module".into(),
            ))
        }
    };
    if (
        definition.implementation_family,
        definition.native_primitive,
        definition.verifier,
    ) != expected
    {
        return Err(RegistryError::InvalidCatalog(
            "module boundary does not match audited role".into(),
        ));
    }
    if definition.input_policy == InputPolicy::SoftwarePreferred
        && definition.native_primitive != NativePrimitive::PrimaryClick
    {
        return Err(RegistryError::InvalidCatalog(
            "software_preferred requires the finite primary_click primitive".into(),
        ));
    }
    Ok(())
}

fn operation_affordance(operation: ActionOperation) -> Option<Affordance> {
    match operation {
        ActionOperation::Click => Some(Affordance::Click),
        ActionOperation::Type => Some(Affordance::Type),
        ActionOperation::Select => Some(Affordance::Select),
        ActionOperation::Upload => Some(Affordance::Upload),
        _ => None,
    }
}

fn state_is(object: &ObservedObject, key: &str, expected: bool) -> bool {
    object.state.get(key).map(String::as_str) == Some(if expected { "true" } else { "false" })
}

pub fn verify(
    definition: &ControlDefinition,
    before: &ObservedObject,
    payload: &ActionPayload,
    after: &ObservationSnapshot,
) -> PostconditionStatus {
    let after_target = after
        .objects
        .iter()
        .find(|object| object.object_id == before.object_id);
    match definition.verifier {
        Verifier::ButtonEffect => {
            let Some(after_target) = after_target else {
                return PostconditionStatus::TargetInvalidated;
            };
            if ["pressed", "expanded"]
                .iter()
                .any(|key| before.state.get(*key) != after_target.state.get(*key))
                || before.name != after_target.name
            {
                PostconditionStatus::Verified
            } else {
                PostconditionStatus::Unverified
            }
        }
        Verifier::DocumentTransition => PostconditionStatus::Unverified,
        Verifier::HasValue => {
            let Some(after_target) = after_target else {
                return PostconditionStatus::TargetInvalidated;
            };
            match payload {
                ActionPayload::Text { text }
                    if !text.is_empty() && state_is(after_target, "has_value", true) =>
                {
                    PostconditionStatus::Verified
                }
                _ => PostconditionStatus::VisibleStateUnchanged,
            }
        }
        Verifier::CheckedTransition => {
            let Some(after_target) = after_target else {
                return PostconditionStatus::TargetInvalidated;
            };
            if before.state.get("checked") != after_target.state.get("checked") {
                PostconditionStatus::Verified
            } else {
                PostconditionStatus::VisibleStateUnchanged
            }
        }
        Verifier::SelectedTransition => {
            let Some(after_target) = after_target else {
                return PostconditionStatus::TargetInvalidated;
            };
            if before.state.get("selected") != after_target.state.get("selected")
                && state_is(after_target, "selected", true)
            {
                PostconditionStatus::Verified
            } else {
                PostconditionStatus::VisibleStateUnchanged
            }
        }
        Verifier::ExpandedTransition => {
            let Some(after_target) = after_target else {
                return PostconditionStatus::TargetInvalidated;
            };
            if before.state.get("expanded") != after_target.state.get("expanded") {
                PostconditionStatus::Verified
            } else {
                PostconditionStatus::VisibleStateUnchanged
            }
        }
        Verifier::OptionSelected => {
            let ActionPayload::Select { option_object_id } = payload else {
                return PostconditionStatus::Unverified;
            };
            if after
                .objects
                .iter()
                .find(|object| object.object_id == *option_object_id)
                .is_some_and(|option| state_is(option, "selected", true))
            {
                PostconditionStatus::Verified
            } else {
                PostconditionStatus::VisibleStateUnchanged
            }
        }
        Verifier::TargetAdvanced => {
            if after.objects.iter().any(|object| {
                object.role == SemanticRole::ReflexTarget
                    && object.loop_class_token == before.loop_class_token
                    && object.state.get("reflex_occurrence")
                        != before.state.get("reflex_occurrence")
            }) {
                PostconditionStatus::Verified
            } else {
                PostconditionStatus::VisibleStateUnchanged
            }
        }
        Verifier::HasFile => {
            let Some(after_target) = after_target else {
                return PostconditionStatus::TargetInvalidated;
            };
            if state_is(after_target, "has_value", true) {
                PostconditionStatus::Verified
            } else {
                PostconditionStatus::VisibleStateUnchanged
            }
        }
    }
}

pub fn verify_with_documents(
    definition: &ControlDefinition,
    before_snapshot: &ObservationSnapshot,
    before: &ObservedObject,
    payload: &ActionPayload,
    after: &ObservationSnapshot,
) -> PostconditionStatus {
    if matches!(
        definition.verifier,
        Verifier::ButtonEffect | Verifier::DocumentTransition
    ) && before_snapshot.document_id != after.document_id
    {
        return PostconditionStatus::Verified;
    }
    verify(definition, before, payload, after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_catalog_backed_and_not_publishable() {
        let registry = Registry::builtin().unwrap();
        assert_eq!(registry.modules.len(), 15);
        assert!(registry
            .modules
            .values()
            .all(|definition| definition.publication_status == PublicationStatus::Implementation));
        assert_eq!(
            registry.input_policy(SemanticRole::Button).unwrap(),
            InputPolicy::SoftwarePreferred
        );
        assert_eq!(
            registry.input_policy(SemanticRole::TextField).unwrap(),
            InputPolicy::NativeRequired
        );
    }
}
