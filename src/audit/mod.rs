// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Local append-only audit logging.
//!
//! This module currently stores permission decisions to JSONL for local review
//! and compliance workflows.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::config::Settings;
use crate::error::Result;
use crate::tools::{PolicyEffect, PolicySource};

/// Permission decision recorded to the audit log.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    AutoAllow,
    PolicyAllow,
    PolicyDeny,
    PromptAllow,
    PromptDeny,
    PromptAllowAll,
    PromptTrustAll,
    PromptError,
}

/// Append-only event describing a permission decision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionAuditEvent {
    pub timestamp: DateTime<Utc>,
    pub tool_name: String,
    pub action_description: String,
    #[serde(default)]
    pub affected_paths: Vec<String>,
    pub is_destructive: bool,
    pub decision: PermissionDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_effect: Option<PolicyEffect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_response: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl PermissionAuditEvent {
    /// Build a base event with current timestamp.
    pub fn new(
        tool_name: String,
        action_description: String,
        affected_paths: Vec<String>,
        is_destructive: bool,
        decision: PermissionDecision,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            tool_name,
            action_description,
            affected_paths,
            is_destructive,
            decision,
            policy_effect: None,
            policy_scope: None,
            policy_path: None,
            policy_reason: None,
            user_response: None,
            note: None,
        }
    }

    /// Attach policy match context to this event.
    pub fn with_policy_match(mut self, effect: PolicyEffect, source: &PolicySource) -> Self {
        self.policy_effect = Some(effect);
        match source {
            PolicySource::User(path) => {
                self.policy_scope = Some("user".to_string());
                self.policy_path = Some(path.display().to_string());
            }
            PolicySource::Project(path) => {
                self.policy_scope = Some("project".to_string());
                self.policy_path = Some(path.display().to_string());
            }
        }
        self
    }
}

/// Append-only JSONL log for permission decisions.
#[derive(Debug, Clone)]
pub struct PermissionAuditLog {
    path: PathBuf,
}

impl Default for PermissionAuditLog {
    fn default() -> Self {
        Self::new(Settings::permissions_audit_log_path())
    }
}

impl PermissionAuditLog {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, event: &PermissionAuditEvent) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string(event)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(json.as_bytes())?;
        file.write_all(b"\n")?;
        Ok(())
    }

    pub fn read_recent(&self, limit: usize) -> Result<Vec<PermissionAuditEvent>> {
        if limit == 0 || !self.path.exists() {
            return Ok(Vec::new());
        }

        let raw = std::fs::read_to_string(&self.path)?;
        let mut parsed: Vec<PermissionAuditEvent> = raw
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return None;
                }
                match serde_json::from_str::<PermissionAuditEvent>(trimmed) {
                    Ok(event) => Some(event),
                    Err(err) => {
                        tracing::warn!(
                            target: "ted.audit.permissions",
                            error = %err,
                            "Skipping malformed permission audit log line"
                        );
                        None
                    }
                }
            })
            .collect();

        if parsed.len() > limit {
            let keep_start = parsed.len() - limit;
            parsed.drain(0..keep_start);
        }

        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_permission_audit_append_and_read_recent() {
        let temp = tempdir().unwrap();
        let log_path = temp.path().join("permissions.jsonl");
        let log = PermissionAuditLog::new(log_path.clone());

        let mut first = PermissionAuditEvent::new(
            "shell".to_string(),
            "Execute: cargo test".to_string(),
            vec![],
            false,
            PermissionDecision::PolicyAllow,
        );
        first.note = Some("safe local command".to_string());

        let second = PermissionAuditEvent::new(
            "file_edit".to_string(),
            "Edit file: secrets/prod.env".to_string(),
            vec!["secrets/prod.env".to_string()],
            true,
            PermissionDecision::PolicyDeny,
        );

        log.append(&first).unwrap();
        log.append(&second).unwrap();

        let recent = log.read_recent(1).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].tool_name, "file_edit");
        assert_eq!(recent[0].decision, PermissionDecision::PolicyDeny);
        assert!(log_path.exists());
    }

    #[test]
    fn test_permission_audit_read_recent_handles_missing_file() {
        let temp = tempdir().unwrap();
        let log = PermissionAuditLog::new(temp.path().join("missing.jsonl"));
        let events = log.read_recent(20).unwrap();
        assert!(events.is_empty());
    }
}
