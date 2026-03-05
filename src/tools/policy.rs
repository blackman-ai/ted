// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Policy-based permission rules.
//!
//! This module loads and evaluates `permissions.toml` rules from:
//! - `~/.ted/permissions.toml` (user scope)
//! - `<project>/.ted/permissions.toml` (project scope)
//!
//! Rules are evaluated in source order, with project rules appended after user
//! rules so project policy can override user policy.

use std::path::{Path, PathBuf};

use glob::Pattern;
use serde::{Deserialize, Serialize};

use crate::config::Settings;
use crate::error::{Result, TedError};

/// Effect to apply when a permission rule matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyEffect {
    /// Explicitly allow the action without prompting.
    Allow,
    /// Require an interactive prompt (default behavior).
    Ask,
    /// Explicitly deny the action.
    Deny,
}

fn default_policy_effect() -> PolicyEffect {
    PolicyEffect::Ask
}

/// Source of a matched rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicySource {
    User(PathBuf),
    Project(PathBuf),
}

/// A concrete rule match result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyMatch {
    pub effect: PolicyEffect,
    pub reason: Option<String>,
    pub source: PolicySource,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PermissionPolicyFile {
    /// Optional policy pack includes. Relative paths resolve from this file's directory.
    #[serde(default)]
    include: Vec<String>,

    #[serde(default)]
    rules: Vec<PermissionRule>,

    /// Lock-mode rules that are evaluated after normal rules and cannot be
    /// bypassed by later non-lock matches.
    #[serde(default)]
    lock_rules: Vec<PermissionRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PermissionRule {
    #[serde(default = "default_policy_effect")]
    effect: PolicyEffect,

    /// Glob patterns for tool names (e.g. `shell`, `file_*`).
    #[serde(default)]
    tools: Vec<String>,

    /// Glob patterns for command text. Primarily useful for shell requests.
    #[serde(default)]
    commands: Vec<String>,

    /// Glob patterns for affected paths.
    #[serde(default)]
    paths: Vec<String>,

    /// Optional destructive flag matcher.
    #[serde(default)]
    destructive: Option<bool>,

    /// Optional explanation for why this rule exists.
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Clone)]
struct PermissionRuleEntry {
    rule: PermissionRule,
    source: PolicySource,
}

/// Merged permission policy across scopes.
#[derive(Debug, Clone, Default)]
pub struct PermissionPolicy {
    rules: Vec<PermissionRuleEntry>,
    lock_rules: Vec<PermissionRuleEntry>,
}

impl PermissionPolicy {
    /// Load policy from the default user/project locations.
    pub fn load_for_workspace(
        working_directory: &Path,
        project_root: Option<&Path>,
    ) -> Result<Self> {
        let user_path = Settings::permissions_policy_path();
        let project_base = project_root.unwrap_or(working_directory);
        let project_path = Settings::project_permissions_policy_path(project_base);

        Self::load_from_paths(&user_path, Some(&project_path))
    }

    /// Load policy from explicit user/project paths.
    pub fn load_from_paths(user_path: &Path, project_path: Option<&Path>) -> Result<Self> {
        let mut merged = PermissionPolicy::default();
        let mut visited = std::collections::HashSet::new();

        let (user_rules, user_lock_rules) = Self::load_file(
            user_path,
            PolicySource::User(user_path.to_path_buf()),
            &mut visited,
        )?;
        merged.rules.extend(user_rules);
        merged.lock_rules.extend(user_lock_rules);

        if let Some(path) = project_path {
            let (project_rules, project_lock_rules) = Self::load_file(
                path,
                PolicySource::Project(path.to_path_buf()),
                &mut visited,
            )?;
            merged.rules.extend(project_rules);
            merged.lock_rules.extend(project_lock_rules);
        }

        Ok(merged)
    }

    fn load_file(
        path: &Path,
        source: PolicySource,
        visited: &mut std::collections::HashSet<PathBuf>,
    ) -> Result<(Vec<PermissionRuleEntry>, Vec<PermissionRuleEntry>)> {
        if !path.exists() {
            return Ok((Vec::new(), Vec::new()));
        }

        let canonical_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if !visited.insert(canonical_path.clone()) {
            tracing::warn!(
                target: "ted.tools.policy",
                path = %canonical_path.display(),
                "Skipping already loaded policy include to avoid recursive loop"
            );
            return Ok((Vec::new(), Vec::new()));
        }

        let raw = std::fs::read_to_string(path)?;
        let parsed: PermissionPolicyFile = toml::from_str(&raw).map_err(|err| {
            TedError::Config(format!(
                "Failed to parse permission policy '{}': {}",
                path.display(),
                err
            ))
        })?;

        let mut rules = Vec::new();
        let mut lock_rules = Vec::new();

        let include_base = path.parent().unwrap_or(Path::new("."));
        for include in parsed.include {
            let include_path = PathBuf::from(&include);
            let resolved_path = if include_path.is_absolute() {
                include_path
            } else {
                include_base.join(include_path)
            };

            let (included_rules, included_lock_rules) =
                Self::load_file(&resolved_path, source.clone(), visited)?;
            rules.extend(included_rules);
            lock_rules.extend(included_lock_rules);
        }

        rules.extend(
            parsed
                .rules
                .into_iter()
                .map(|rule| PermissionRuleEntry {
                    rule,
                    source: source.clone(),
                })
                .collect::<Vec<_>>(),
        );

        lock_rules.extend(
            parsed
                .lock_rules
                .into_iter()
                .map(|rule| PermissionRuleEntry {
                    rule,
                    source: source.clone(),
                })
                .collect::<Vec<_>>(),
        );

        Ok((rules, lock_rules))
    }

    /// Evaluate policy for a tool action.
    ///
    /// Returns the last matching rule across merged scopes.
    pub fn evaluate(
        &self,
        tool_name: &str,
        action_description: &str,
        affected_paths: &[String],
        is_destructive: bool,
    ) -> Option<PolicyMatch> {
        let mut matched: Option<PolicyMatch> = None;
        let command_text = extract_command_text(action_description).unwrap_or(action_description);

        for entry in &self.rules {
            if !matches_patterns(&entry.rule.tools, tool_name) {
                continue;
            }
            if !matches_patterns(&entry.rule.commands, command_text) {
                continue;
            }
            if !matches_paths(&entry.rule.paths, affected_paths) {
                continue;
            }
            if !matches_destructive(entry.rule.destructive, is_destructive) {
                continue;
            }

            matched = Some(PolicyMatch {
                effect: entry.rule.effect,
                reason: entry.rule.reason.clone(),
                source: entry.source.clone(),
            });
        }

        let mut matched_lock: Option<PolicyMatch> = None;
        for entry in &self.lock_rules {
            if !matches_patterns(&entry.rule.tools, tool_name) {
                continue;
            }
            if !matches_patterns(&entry.rule.commands, command_text) {
                continue;
            }
            if !matches_paths(&entry.rule.paths, affected_paths) {
                continue;
            }
            if !matches_destructive(entry.rule.destructive, is_destructive) {
                continue;
            }

            matched_lock = Some(PolicyMatch {
                effect: entry.rule.effect,
                reason: entry.rule.reason.clone(),
                source: entry.source.clone(),
            });
        }

        matched_lock.or(matched)
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.lock_rules.is_empty()
    }
}

fn extract_command_text(action_description: &str) -> Option<&str> {
    action_description
        .strip_prefix("Execute:")
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn matches_destructive(filter: Option<bool>, actual: bool) -> bool {
    filter.map(|expected| expected == actual).unwrap_or(true)
}

fn matches_paths(path_patterns: &[String], affected_paths: &[String]) -> bool {
    if path_patterns.is_empty() {
        return true;
    }

    if affected_paths.is_empty() {
        return false;
    }

    path_patterns.iter().any(|pattern| {
        affected_paths
            .iter()
            .any(|path| matches_pattern(pattern, path))
    })
}

fn matches_patterns(patterns: &[String], value: &str) -> bool {
    if patterns.is_empty() {
        return true;
    }

    patterns
        .iter()
        .any(|pattern| matches_pattern(pattern, value))
}

fn matches_pattern(pattern: &str, value: &str) -> bool {
    Pattern::new(pattern)
        .map(|compiled| compiled.matches(value))
        .unwrap_or_else(|_| pattern == value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_policy_evaluates_allow_for_matching_shell_command() {
        let policy = PermissionPolicy {
            rules: vec![PermissionRuleEntry {
                rule: PermissionRule {
                    effect: PolicyEffect::Allow,
                    tools: vec!["shell".to_string()],
                    commands: vec!["cargo *".to_string()],
                    paths: Vec::new(),
                    destructive: None,
                    reason: Some("safe cargo workflow".to_string()),
                },
                source: PolicySource::User(PathBuf::from("/tmp/user-policy")),
            }],
            lock_rules: Vec::new(),
        };

        let result = policy.evaluate("shell", "Execute: cargo test --all", &[], false);
        let matched = result.expect("expected policy match");
        assert_eq!(matched.effect, PolicyEffect::Allow);
        assert_eq!(matched.reason, Some("safe cargo workflow".to_string()));
    }

    #[test]
    fn test_policy_path_pattern_matching() {
        let policy = PermissionPolicy {
            rules: vec![PermissionRuleEntry {
                rule: PermissionRule {
                    effect: PolicyEffect::Deny,
                    tools: vec!["file_edit".to_string()],
                    commands: Vec::new(),
                    paths: vec!["secrets/**".to_string()],
                    destructive: None,
                    reason: None,
                },
                source: PolicySource::Project(PathBuf::from("/tmp/project-policy")),
            }],
            lock_rules: Vec::new(),
        };

        let result = policy.evaluate(
            "file_edit",
            "Edit file: secrets/prod.env",
            &[String::from("secrets/prod.env")],
            true,
        );
        assert_eq!(result.expect("match").effect, PolicyEffect::Deny);
    }

    #[test]
    fn test_policy_destructive_filter() {
        let policy = PermissionPolicy {
            rules: vec![PermissionRuleEntry {
                rule: PermissionRule {
                    effect: PolicyEffect::Deny,
                    tools: vec!["shell".to_string()],
                    commands: vec!["git push*".to_string()],
                    paths: Vec::new(),
                    destructive: Some(true),
                    reason: None,
                },
                source: PolicySource::Project(PathBuf::from("/tmp/project-policy")),
            }],
            lock_rules: Vec::new(),
        };

        let non_destructive = policy.evaluate("shell", "Execute: git push", &[], false);
        assert!(non_destructive.is_none());

        let destructive = policy.evaluate("shell", "Execute: git push", &[], true);
        assert_eq!(destructive.expect("match").effect, PolicyEffect::Deny);
    }

    #[test]
    fn test_project_policy_overrides_user_policy_with_last_match() {
        let temp = tempdir().unwrap();
        let user_policy = temp.path().join("user-permissions.toml");
        let project_policy = temp.path().join("project-permissions.toml");

        std::fs::write(
            &user_policy,
            r#"
[[rules]]
effect = "deny"
tools = ["shell"]
commands = ["cargo *"]
"#,
        )
        .unwrap();

        std::fs::write(
            &project_policy,
            r#"
[[rules]]
effect = "allow"
tools = ["shell"]
commands = ["cargo *"]
"#,
        )
        .unwrap();

        let merged =
            PermissionPolicy::load_from_paths(&user_policy, Some(&project_policy)).unwrap();
        let matched = merged
            .evaluate("shell", "Execute: cargo test", &[], false)
            .expect("expected rule");

        assert_eq!(matched.effect, PolicyEffect::Allow);
        match matched.source {
            PolicySource::Project(path) => assert_eq!(path, project_policy),
            _ => panic!("expected project source"),
        }
    }

    #[test]
    fn test_load_missing_files_is_empty_policy() {
        let temp = tempdir().unwrap();
        let user_path = temp.path().join("missing-user.toml");
        let project_path = temp.path().join("missing-project.toml");
        let policy = PermissionPolicy::load_from_paths(&user_path, Some(&project_path)).unwrap();
        assert!(policy.is_empty());
    }

    #[test]
    fn test_policy_include_loads_relative_policy_pack() {
        let temp = tempdir().unwrap();
        let user_policy = temp.path().join("permissions.toml");
        let pack_dir = temp.path().join("packs");
        let pack_policy = pack_dir.join("shared.toml");
        std::fs::create_dir_all(&pack_dir).unwrap();

        std::fs::write(
            &user_policy,
            r#"
include = ["packs/shared.toml"]
"#,
        )
        .unwrap();

        std::fs::write(
            &pack_policy,
            r#"
[[rules]]
effect = "allow"
tools = ["shell"]
commands = ["cargo *"]
"#,
        )
        .unwrap();

        let policy = PermissionPolicy::load_from_paths(&user_policy, None).unwrap();
        let matched = policy
            .evaluate("shell", "Execute: cargo test", &[], false)
            .expect("expected included rule to match");
        assert_eq!(matched.effect, PolicyEffect::Allow);
    }

    #[test]
    fn test_lock_rules_override_last_match_from_regular_rules() {
        let temp = tempdir().unwrap();
        let user_policy = temp.path().join("permissions.toml");
        std::fs::write(
            &user_policy,
            r#"
[[rules]]
effect = "allow"
tools = ["shell"]
commands = ["git push --force*"]

[[lock_rules]]
effect = "deny"
tools = ["shell"]
commands = ["git push --force*"]
reason = "force push is always blocked"
"#,
        )
        .unwrap();

        let policy = PermissionPolicy::load_from_paths(&user_policy, None).unwrap();
        let matched = policy
            .evaluate("shell", "Execute: git push --force-with-lease", &[], true)
            .expect("expected lock rule match");
        assert_eq!(matched.effect, PolicyEffect::Deny);
        assert_eq!(
            matched.reason,
            Some("force push is always blocked".to_string())
        );
    }
}
