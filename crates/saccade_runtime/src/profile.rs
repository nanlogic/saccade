//! Human-readable Agent behavior and control-ban Profile.

use std::fs;
use std::path::Path;

use saccade_protocol::{ObjectKind, ObservationSnapshot, ObservedObject};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_NAME_BYTES: usize = 128;
const MAX_BEHAVIOR_BYTES: usize = 16 * 1024;
const MAX_BAN_RULES: usize = 512;
const MAX_MATCH_BYTES: usize = 2 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub behavior: String,
    pub ban: Vec<BanRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BanRule {
    pub control: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("failed to read Profile: {0}")]
    Read(#[from] std::io::Error),
    #[error("invalid Profile JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid Profile: {0}")]
    Invalid(&'static str),
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "default".into(),
            behavior: String::new(),
            ban: Vec::new(),
        }
    }
}

impl Profile {
    pub fn load(runtime_dir: &Path) -> Result<Self, ProfileError> {
        let path = runtime_dir.join("profile.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let profile: Self = serde_json::from_slice(&fs::read(path)?)?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), ProfileError> {
        if normalized(&self.name).is_empty() || self.name.len() > MAX_NAME_BYTES {
            return Err(ProfileError::Invalid("name is empty or too long"));
        }
        if self.behavior.len() > MAX_BEHAVIOR_BYTES {
            return Err(ProfileError::Invalid("behavior is too long"));
        }
        if self.ban.len() > MAX_BAN_RULES {
            return Err(ProfileError::Invalid("ban has too many rules"));
        }
        for rule in &self.ban {
            if normalized(&rule.control).is_empty() || rule.control.len() > MAX_MATCH_BYTES {
                return Err(ProfileError::Invalid("ban control is empty or too long"));
            }
            if rule.condition.as_ref().is_some_and(|condition| {
                normalized(condition).is_empty() || condition.len() > MAX_MATCH_BYTES
            }) {
                return Err(ProfileError::Invalid("ban condition is empty or too long"));
            }
        }
        Ok(())
    }

    pub fn filter_observation(&self, snapshot: &mut ObservationSnapshot) {
        let banned_ids = snapshot
            .objects
            .iter()
            .filter(|object| self.bans(object))
            .map(|object| object.object_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        if banned_ids.is_empty() {
            return;
        }
        snapshot
            .objects
            .retain(|object| !banned_ids.contains(&object.object_id));
        snapshot
            .changes
            .retain(|change| !banned_ids.contains(&change.object_id));
        snapshot
            .limitations
            .retain(|limitation| match limitation.object_id.as_ref() {
                Some(object_id) => !banned_ids.contains(object_id),
                None => true,
            });
    }

    pub fn bans(&self, object: &ObservedObject) -> bool {
        if !matches!(object.kind, ObjectKind::Control | ObjectKind::Link) {
            return false;
        }
        let Some(name) = object.name.as_deref() else {
            return false;
        };
        let normalized_name = normalized(name);
        let associated_text = normalized(&object.description.as_ref().map_or_else(
            || name.to_string(),
            |description| format!("{name} {description}"),
        ));
        self.ban.iter().any(|rule| {
            normalized(&rule.control) == normalized_name
                && match rule.condition.as_ref() {
                    Some(condition) => associated_text.contains(&normalized(condition)),
                    None => true,
                }
        })
    }
}

fn normalized(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use saccade_protocol::{
        Affordance, ObjectKind, ObservedObject, Rect, SemanticRole, Transition, Visibility,
    };

    use super::*;

    fn control(name: &str, description: Option<&str>) -> ObservedObject {
        ObservedObject {
            object_id: "control-1".into(),
            object_revision: 1,
            frame_id: "frame-1".into(),
            kind: ObjectKind::Control,
            role: SemanticRole::Button,
            document_bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
            viewport_bounds: None,
            visibility: Visibility::Visible,
            name: Some(name.into()),
            description: description.map(str::to_string),
            text: None,
            state: BTreeMap::new(),
            affordances: BTreeSet::from([Affordance::Click]),
            transition: Transition::None,
            action_token: Some("token.0123456789abcdef0123456789abcdef".into()),
            loop_class_token: None,
            protected: false,
        }
    }

    #[test]
    fn ban_without_condition_matches_normalized_control_name() {
        let profile = Profile {
            name: "test".into(),
            behavior: String::new(),
            ban: vec![BanRule {
                control: "  DELETE   ACCOUNT ".into(),
                condition: None,
            }],
        };
        assert!(profile.bans(&control("Delete Account", None)));
        let mut unavailable = control("delete account", None);
        unavailable.action_token = None;
        assert!(profile.bans(&unavailable));
    }

    #[test]
    fn condition_matches_associated_text_without_case_sensitivity() {
        let profile = Profile {
            name: "test".into(),
            behavior: String::new(),
            ban: vec![BanRule {
                control: "Continue".into(),
                condition: Some("PAYMENT required".into()),
            }],
        };
        assert!(profile.bans(&control(
            "continue",
            Some("Payment   Required before checkout")
        )));
        assert!(!profile.bans(&control("continue", Some("Read the next page"))));
    }

    #[test]
    fn unknown_fields_and_empty_conditions_are_rejected() {
        assert!(serde_json::from_str::<Profile>(
            r#"{"name":"x","behavior":"","ban":[],"guard":"off"}"#
        )
        .is_err());
        let profile = Profile {
            name: "x".into(),
            behavior: String::new(),
            ban: vec![BanRule {
                control: "Continue".into(),
                condition: Some("  ".into()),
            }],
        };
        assert!(profile.validate().is_err());
    }

    #[test]
    fn load_uses_default_or_strict_runtime_profile() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Profile::load(dir.path()).unwrap(), Profile::default());
        std::fs::write(
            dir.path().join("profile.json"),
            r#"{"name":"work","behavior":"Stay concise.","ban":[]}"#,
        )
        .unwrap();
        let loaded = Profile::load(dir.path()).unwrap();
        assert_eq!(loaded.name, "work");
        assert_eq!(loaded.behavior, "Stay concise.");
    }
}
