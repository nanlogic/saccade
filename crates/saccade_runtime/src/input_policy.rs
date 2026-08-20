//! User-local input-backend learning. This state never enters Profile or Git.

use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use saccade_protocol::SemanticRole;
use serde::{Deserialize, Serialize};

const SCHEMA: &str = "saccade.input-policy/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearnedBackend {
    Software,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEvidence {
    VerifiedSoftwareReceipt,
    UnverifiedSoftwareReceipt,
    UserRememberedNative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputPolicyRule {
    pub page: String,
    pub role: SemanticRole,
    pub control: String,
    pub backend: LearnedBackend,
    pub evidence: PolicyEvidence,
    /// Extension candidate the rule was learned under. A native escalation is
    /// a conclusion about one software implementation, so it must not outlive
    /// it: rules from another generation are ignored rather than binding a
    /// software pipe that has since changed. Absent on rules written before
    /// generations were recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub software_generation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputPolicyFile {
    schema: String,
    rules: Vec<InputPolicyRule>,
}

pub struct LocalInputPolicy {
    path: PathBuf,
    rules: Vec<InputPolicyRule>,
}

impl LocalInputPolicy {
    pub fn load(runtime_dir: &Path) -> Result<Self> {
        let path = runtime_dir.join("input-policy.json");
        if !path.exists() {
            return Ok(Self {
                path,
                rules: Vec::new(),
            });
        }
        let file: InputPolicyFile = serde_json::from_slice(
            &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
        )?;
        if file.schema != SCHEMA {
            bail!("input policy file uses an unsupported schema");
        }
        for rule in &file.rules {
            validate_rule(rule)?;
        }
        Ok(Self {
            path,
            rules: file.rules,
        })
    }

    /// The learned backend as the Reference Actuator has always resolved it.
    ///
    /// The Reference Actuator is the audited actuation path: it may escalate to
    /// native, so a remembered rule is simply the backend to use. Shipping a new
    /// Extension is not evidence about that path and must never silently move a
    /// control it had already settled on native back to software, so this
    /// deliberately does not consider the software generation.
    pub fn backend_for(
        &self,
        page: &str,
        role: SemanticRole,
        control: &str,
    ) -> Option<LearnedBackend> {
        self.rules
            .iter()
            .find(|rule| rule_matches(rule, page, role, control))
            .map(|rule| rule.backend)
    }

    /// The learned backend for the public `saccade.act` route, which is
    /// software-only and hands off to the Agent client instead of escalating.
    ///
    /// Here a native rule means "stop, this is not doable in software". That is
    /// a conclusion about one software implementation, so it must not outlive
    /// it: once a different Extension generation is answering, an automatically
    /// learned native conclusion is discarded and software is tried again, or
    /// software typing could never be improved. A native backend the *user*
    /// chose is a decision about what they want rather than a conclusion about
    /// a pipe, so it keeps binding across every generation.
    pub fn public_act_backend_for(
        &self,
        page: &str,
        role: SemanticRole,
        control: &str,
        generation: Option<&str>,
    ) -> Option<LearnedBackend> {
        let rule = self
            .rules
            .iter()
            .find(|rule| rule_matches(rule, page, role, control))?;
        if rule.backend == LearnedBackend::Native
            && rule.evidence != PolicyEvidence::UserRememberedNative
            && rule.software_generation.as_deref() != generation
        {
            return None;
        }
        Some(rule.backend)
    }

    pub fn remember(
        &mut self,
        page: String,
        role: SemanticRole,
        control: String,
        backend: LearnedBackend,
        evidence: PolicyEvidence,
        software_generation: Option<String>,
    ) -> Result<bool> {
        let next = InputPolicyRule {
            page,
            role,
            control,
            backend,
            evidence,
            software_generation,
        };
        validate_rule(&next)?;
        if let Some(rule) = self
            .rules
            .iter_mut()
            .find(|rule| rule_matches(rule, &next.page, next.role, &next.control))
        {
            if *rule == next {
                return Ok(false);
            }
            *rule = next;
        } else {
            self.rules.push(next);
            self.rules.sort_by(|left, right| {
                (&left.page, left.role, canonical(&left.control)).cmp(&(
                    &right.page,
                    right.role,
                    canonical(&right.control),
                ))
            });
        }
        self.save()?;
        Ok(true)
    }

    pub fn rules(&self) -> &[InputPolicyRule] {
        &self.rules
    }

    fn save(&self) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("input policy path has no parent")?;
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        let temporary = self.path.with_extension("json.tmp");
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(&InputPolicyFile {
            schema: SCHEMA.into(),
            rules: self.rules.clone(),
        })?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(temporary, &self.path)?;
        Ok(())
    }
}

pub fn page_scope(url: &str) -> Result<String> {
    let (scheme, remainder) = url
        .split_once("://")
        .context("page URL has no HTTP scheme")?;
    if !matches!(scheme, "http" | "https") {
        bail!("page URL must use HTTP or HTTPS");
    }
    let clean = remainder.split(['?', '#']).next().unwrap_or_default();
    let (authority, path) = clean.split_once('/').unwrap_or((clean, ""));
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if host.is_empty() {
        bail!("page URL has no host");
    }
    Ok(format!(
        "{scheme}://{host}/{}",
        path.trim_start_matches('/')
    ))
}

fn rule_matches(rule: &InputPolicyRule, page: &str, role: SemanticRole, control: &str) -> bool {
    rule.page == page && rule.role == role && canonical(&rule.control) == canonical(control)
}

fn canonical(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn validate_rule(rule: &InputPolicyRule) -> Result<()> {
    if page_scope(&rule.page)? != rule.page {
        bail!("input policy page is not normalized");
    }
    if canonical(&rule.control).is_empty() || rule.control.len() > 512 {
        bail!("input policy control name is invalid");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_scope_drops_credentials_query_and_fragment() {
        assert_eq!(
            page_scope("https://user:secret@Example.test/settings?id=secret#section").unwrap(),
            "https://Example.test/settings"
        );
    }

    fn seed(
        dir: &std::path::Path,
        backend: LearnedBackend,
        evidence: PolicyEvidence,
        generation: Option<&str>,
    ) -> LocalInputPolicy {
        let mut memory = LocalInputPolicy::load(dir).unwrap();
        memory
            .remember(
                "https://example.test/settings".into(),
                SemanticRole::TextField,
                "Full name".into(),
                backend,
                evidence,
                generation.map(str::to_string),
            )
            .unwrap();
        LocalInputPolicy::load(dir).unwrap()
    }

    const PAGE: &str = "https://example.test/settings";
    const CONTROL: &str = "Full name";

    #[test]
    fn local_receipt_memory_is_strict_persistent_and_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let mut memory = LocalInputPolicy::load(dir.path()).unwrap();
        memory
            .remember(
                "https://example.test/settings".into(),
                SemanticRole::Button,
                "Save changes".into(),
                LearnedBackend::Native,
                PolicyEvidence::UnverifiedSoftwareReceipt,
                Some("candidate-a".into()),
            )
            .unwrap();
        let reloaded = LocalInputPolicy::load(dir.path()).unwrap();
        // Persistent, and matched without regard to case or run of whitespace.
        assert_eq!(
            reloaded.backend_for(PAGE, SemanticRole::Button, "SAVE   CHANGES"),
            Some(LearnedBackend::Native)
        );
        assert!(!fs::read_to_string(dir.path().join("input-policy.json"))
            .unwrap()
            .contains("secret"));
    }

    #[test]
    fn the_reference_actuator_selection_never_changes_with_the_software_generation() {
        let dir = tempfile::tempdir().unwrap();
        let memory = seed(
            dir.path(),
            LearnedBackend::Native,
            PolicyEvidence::UnverifiedSoftwareReceipt,
            Some("candidate-a"),
        );
        // The Reference Actuator had already settled this control on native.
        // Shipping a new Extension says nothing about that path, so its answer
        // must be identical no matter which generation is answering.
        assert_eq!(
            memory.backend_for(PAGE, SemanticRole::TextField, CONTROL),
            Some(LearnedBackend::Native)
        );
    }

    #[test]
    fn public_act_is_not_permanently_suppressed_by_an_older_generation() {
        let dir = tempfile::tempdir().unwrap();
        let memory = seed(
            dir.path(),
            LearnedBackend::Native,
            PolicyEvidence::UnverifiedSoftwareReceipt,
            Some("candidate-a"),
        );
        // Same generation: the conclusion still describes the running pipe.
        assert_eq!(
            memory.public_act_backend_for(
                PAGE,
                SemanticRole::TextField,
                CONTROL,
                Some("candidate-a")
            ),
            Some(LearnedBackend::Native)
        );
        // A different generation: the conclusion was about a pipe that is no
        // longer running, so software must be tried again.
        assert_eq!(
            memory.public_act_backend_for(
                PAGE,
                SemanticRole::TextField,
                CONTROL,
                Some("candidate-b")
            ),
            None
        );
        // The Reference Actuator is untouched by that reasoning.
        assert_eq!(
            memory.backend_for(PAGE, SemanticRole::TextField, CONTROL),
            Some(LearnedBackend::Native)
        );
    }

    #[test]
    fn a_user_remembered_native_choice_binds_on_both_routes_in_every_generation() {
        let dir = tempfile::tempdir().unwrap();
        let memory = seed(
            dir.path(),
            LearnedBackend::Native,
            PolicyEvidence::UserRememberedNative,
            Some("candidate-a"),
        );
        assert_eq!(
            memory.backend_for(PAGE, SemanticRole::TextField, CONTROL),
            Some(LearnedBackend::Native)
        );
        for generation in [Some("candidate-a"), Some("candidate-b"), None] {
            assert_eq!(
                memory.public_act_backend_for(PAGE, SemanticRole::TextField, CONTROL, generation),
                Some(LearnedBackend::Native),
                "the user's own choice must survive generation {generation:?}"
            );
        }
    }

    #[test]
    fn a_rule_written_before_generations_were_recorded_is_read_safely() {
        let dir = tempfile::tempdir().unwrap();
        // Exactly the on-disk shape written by a build that predates
        // software_generation: the field is simply absent.
        fs::write(
            dir.path().join("input-policy.json"),
            r#"{"schema":"saccade.input-policy/1","rules":[{"page":"https://example.test/settings","role":"text_field","control":"Full name","backend":"native","evidence":"unverified_software_receipt"}]}"#,
        )
        .unwrap();
        let memory = LocalInputPolicy::load(dir.path()).unwrap();
        // An old file is still a valid file, and the Reference Actuator keeps
        // honoring it exactly as before.
        assert_eq!(
            memory.backend_for(PAGE, SemanticRole::TextField, CONTROL),
            Some(LearnedBackend::Native)
        );
        // On the public route the rule names no generation, so it cannot match
        // the running one and must not suppress software forever.
        assert_eq!(
            memory.public_act_backend_for(
                PAGE,
                SemanticRole::TextField,
                CONTROL,
                Some("candidate-a")
            ),
            None
        );
    }

    #[test]
    fn a_learned_software_rule_is_never_generation_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let memory = seed(
            dir.path(),
            LearnedBackend::Software,
            PolicyEvidence::UnverifiedSoftwareReceipt,
            Some("candidate-a"),
        );
        // Only native conclusions expire with the pipe that produced them.
        assert_eq!(
            memory.public_act_backend_for(
                PAGE,
                SemanticRole::TextField,
                CONTROL,
                Some("candidate-b")
            ),
            Some(LearnedBackend::Software)
        );
        assert_eq!(
            memory.backend_for(PAGE, SemanticRole::TextField, CONTROL),
            Some(LearnedBackend::Software)
        );
    }
}
