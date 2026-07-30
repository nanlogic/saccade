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

    pub fn remember(
        &mut self,
        page: String,
        role: SemanticRole,
        control: String,
        backend: LearnedBackend,
        evidence: PolicyEvidence,
    ) -> Result<bool> {
        let next = InputPolicyRule {
            page,
            role,
            control,
            backend,
            evidence,
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
            )
            .unwrap();
        let reloaded = LocalInputPolicy::load(dir.path()).unwrap();
        assert_eq!(
            reloaded.backend_for(
                "https://example.test/settings",
                SemanticRole::Button,
                "SAVE   CHANGES"
            ),
            Some(LearnedBackend::Native)
        );
        assert!(!fs::read_to_string(dir.path().join("input-policy.json"))
            .unwrap()
            .contains("secret"));
    }
}
