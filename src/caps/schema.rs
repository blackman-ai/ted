// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Cap schema definition
//!
//! Defines the structure of a cap file and its components.

use serde::{Deserialize, Serialize};

/// A cap (capability/persona) definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cap {
    /// Unique name of the cap
    pub name: String,

    /// Human-readable description
    #[serde(default)]
    pub description: String,

    /// Version string (semver recommended)
    #[serde(default = "default_version")]
    pub version: String,

    /// Priority for ordering (higher = applied later)
    #[serde(default)]
    pub priority: i32,

    /// List of cap names this cap extends
    #[serde(default)]
    pub extends: Vec<String>,

    /// Tool permission configuration
    #[serde(default)]
    pub tool_permissions: CapToolPermissions,

    /// System prompt to prepend to conversations
    #[serde(default)]
    pub system_prompt: String,

    /// Model preferences (optional override)
    #[serde(default)]
    pub model: Option<CapModelPreferences>,

    /// Whether this cap is a built-in
    #[serde(skip)]
    pub is_builtin: bool,

    /// Source path (for user-defined caps)
    #[serde(skip)]
    pub source_path: Option<std::path::PathBuf>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

impl Cap {
    /// Create a new cap with the given name
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: String::new(),
            version: default_version(),
            priority: 0,
            extends: Vec::new(),
            tool_permissions: CapToolPermissions::default(),
            system_prompt: String::new(),
            model: None,
            is_builtin: false,
            source_path: None,
        }
    }

    /// Set the description
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = description.to_string();
        self
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, prompt: &str) -> Self {
        self.system_prompt = prompt.to_string();
        self
    }

    /// Set the priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Add parent caps to extend
    pub fn extends(mut self, parents: &[&str]) -> Self {
        self.extends = parents.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Mark as built-in
    pub fn builtin(mut self) -> Self {
        self.is_builtin = true;
        self
    }

    /// Set tool permissions
    pub fn with_tool_permissions(mut self, perms: CapToolPermissions) -> Self {
        self.tool_permissions = perms;
        self
    }
}

/// Tool permission configuration for a cap
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapToolPermissions {
    /// Tools to enable (empty = all enabled)
    #[serde(default)]
    pub enable: Vec<String>,

    /// Tools to disable
    #[serde(default)]
    pub disable: Vec<String>,

    /// Whether to require confirmation for edits
    #[serde(default = "default_true")]
    pub require_edit_confirmation: bool,

    /// Whether to require confirmation for shell commands
    #[serde(default = "default_true")]
    pub require_shell_confirmation: bool,

    /// Patterns for auto-approved paths (glob patterns)
    #[serde(default)]
    pub auto_approve_paths: Vec<String>,

    /// Shell commands that are blocked
    #[serde(default)]
    pub blocked_commands: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl CapToolPermissions {
    /// Create permissive permissions (no confirmations needed)
    pub fn permissive() -> Self {
        Self {
            enable: Vec::new(),
            disable: Vec::new(),
            require_edit_confirmation: false,
            require_shell_confirmation: false,
            auto_approve_paths: Vec::new(),
            blocked_commands: Vec::new(),
        }
    }

    /// Create restrictive permissions (all confirmations required)
    pub fn restrictive() -> Self {
        Self {
            enable: Vec::new(),
            disable: vec![
                "shell".to_string(),
                "file_write".to_string(),
                "file_edit".to_string(),
            ],
            require_edit_confirmation: true,
            require_shell_confirmation: true,
            auto_approve_paths: Vec::new(),
            blocked_commands: vec![
                "rm -rf".to_string(),
                "sudo".to_string(),
                "chmod".to_string(),
            ],
        }
    }

    /// Merge with another permissions config (other takes precedence)
    pub fn merge(&self, other: &CapToolPermissions) -> CapToolPermissions {
        let mut result = self.clone();

        // Merge enable lists (union)
        for tool in &other.enable {
            if !result.enable.contains(tool) {
                result.enable.push(tool.clone());
            }
        }

        // Merge disable lists (union)
        for tool in &other.disable {
            if !result.disable.contains(tool) {
                result.disable.push(tool.clone());
            }
        }

        // Later cap's preferences take precedence for booleans
        result.require_edit_confirmation = other.require_edit_confirmation;
        result.require_shell_confirmation = other.require_shell_confirmation;

        // Merge auto-approve paths
        for path in &other.auto_approve_paths {
            if !result.auto_approve_paths.contains(path) {
                result.auto_approve_paths.push(path.clone());
            }
        }

        // Merge blocked commands
        for cmd in &other.blocked_commands {
            if !result.blocked_commands.contains(cmd) {
                result.blocked_commands.push(cmd.clone());
            }
        }

        result
    }

    /// Check if a tool is enabled
    pub fn is_tool_enabled(&self, tool_name: &str) -> bool {
        // If disable list is not empty and contains the tool, it's disabled
        if !self.disable.is_empty() && self.disable.contains(&tool_name.to_string()) {
            return false;
        }

        // If enable list is not empty, tool must be in it
        if !self.enable.is_empty() {
            return self.enable.contains(&tool_name.to_string());
        }

        // Default: all tools enabled
        true
    }
}

/// Model preferences for a cap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapModelPreferences {
    /// Preferred model name
    #[serde(default)]
    pub preferred_model: Option<String>,

    /// Temperature override
    #[serde(default)]
    pub temperature: Option<f32>,

    /// Max tokens override
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cap_creation() {
        let cap = Cap::new("test")
            .with_description("A test cap")
            .with_system_prompt("You are a test assistant.")
            .with_priority(10)
            .extends(&["base"]);

        assert_eq!(cap.name, "test");
        assert_eq!(cap.priority, 10);
        assert_eq!(cap.extends, vec!["base"]);
    }

    #[test]
    fn test_cap_new_defaults() {
        let cap = Cap::new("my-cap");
        assert_eq!(cap.name, "my-cap");
        assert_eq!(cap.description, "");
        assert_eq!(cap.version, "1.0.0");
        assert_eq!(cap.priority, 0);
        assert!(cap.extends.is_empty());
        assert_eq!(cap.system_prompt, "");
        assert!(cap.model.is_none());
        assert!(!cap.is_builtin);
        assert!(cap.source_path.is_none());
    }

    #[test]
    fn test_cap_with_description() {
        let cap = Cap::new("test").with_description("Test description");
        assert_eq!(cap.description, "Test description");
    }

    #[test]
    fn test_cap_with_system_prompt() {
        let cap = Cap::new("test").with_system_prompt("System prompt content");
        assert_eq!(cap.system_prompt, "System prompt content");
    }

    #[test]
    fn test_cap_with_priority() {
        let cap = Cap::new("test").with_priority(50);
        assert_eq!(cap.priority, 50);
    }

    #[test]
    fn test_cap_extends() {
        let cap = Cap::new("test").extends(&["base", "security"]);
        assert_eq!(cap.extends, vec!["base", "security"]);
    }

    #[test]
    fn test_cap_builtin() {
        let cap = Cap::new("test").builtin();
        assert!(cap.is_builtin);
    }

    #[test]
    fn test_cap_with_tool_permissions() {
        let perms = CapToolPermissions::permissive();
        let cap = Cap::new("test").with_tool_permissions(perms.clone());
        assert!(!cap.tool_permissions.require_edit_confirmation);
        assert!(!cap.tool_permissions.require_shell_confirmation);
    }

    #[test]
    fn test_cap_chained_builders() {
        let cap = Cap::new("full-test")
            .with_description("Full test")
            .with_system_prompt("Prompt")
            .with_priority(100)
            .extends(&["base"])
            .builtin()
            .with_tool_permissions(CapToolPermissions::restrictive());

        assert_eq!(cap.name, "full-test");
        assert_eq!(cap.description, "Full test");
        assert_eq!(cap.system_prompt, "Prompt");
        assert_eq!(cap.priority, 100);
        assert_eq!(cap.extends, vec!["base"]);
        assert!(cap.is_builtin);
        assert!(cap.tool_permissions.require_edit_confirmation);
    }

    #[test]
    fn test_tool_permissions_default() {
        // Note: Default derive uses bool default (false) for confirmations
        // but serde deserialize uses default_true for these fields
        let perms = CapToolPermissions::default();
        assert!(perms.enable.is_empty());
        assert!(perms.disable.is_empty());
        // Default derive gives false for bools
        assert!(!perms.require_edit_confirmation);
        assert!(!perms.require_shell_confirmation);
        assert!(perms.auto_approve_paths.is_empty());
        assert!(perms.blocked_commands.is_empty());
    }

    #[test]
    fn test_tool_permissions_serde_default() {
        // serde uses default_true for confirmation fields
        let toml = r#"
enable = []
"#;
        let perms: CapToolPermissions = toml::from_str(toml).unwrap();
        assert!(perms.require_edit_confirmation);
        assert!(perms.require_shell_confirmation);
    }

    #[test]
    fn test_tool_permissions_permissive() {
        let perms = CapToolPermissions::permissive();
        assert!(!perms.require_edit_confirmation);
        assert!(!perms.require_shell_confirmation);
    }

    #[test]
    fn test_tool_permissions_restrictive() {
        let perms = CapToolPermissions::restrictive();
        assert!(perms.require_edit_confirmation);
        assert!(perms.require_shell_confirmation);
        assert!(perms.disable.contains(&"shell".to_string()));
        assert!(perms.disable.contains(&"file_write".to_string()));
        assert!(perms.disable.contains(&"file_edit".to_string()));
        assert!(perms.blocked_commands.contains(&"rm -rf".to_string()));
        assert!(perms.blocked_commands.contains(&"sudo".to_string()));
        assert!(perms.blocked_commands.contains(&"chmod".to_string()));
    }

    #[test]
    fn test_tool_permissions_merge() {
        let base = CapToolPermissions {
            enable: vec!["file_read".to_string()],
            disable: Vec::new(),
            require_edit_confirmation: true,
            require_shell_confirmation: true,
            auto_approve_paths: Vec::new(),
            blocked_commands: vec!["rm -rf".to_string()],
        };

        let child = CapToolPermissions {
            enable: vec!["shell".to_string()],
            disable: Vec::new(),
            require_edit_confirmation: false,
            require_shell_confirmation: true,
            auto_approve_paths: vec!["src/**".to_string()],
            blocked_commands: Vec::new(),
        };

        let merged = base.merge(&child);
        assert!(merged.enable.contains(&"file_read".to_string()));
        assert!(merged.enable.contains(&"shell".to_string()));
        assert!(!merged.require_edit_confirmation);
        assert!(merged.auto_approve_paths.contains(&"src/**".to_string()));
        assert!(merged.blocked_commands.contains(&"rm -rf".to_string()));
    }

    #[test]
    fn test_tool_permissions_merge_with_disable() {
        let base = CapToolPermissions {
            enable: vec!["file_read".to_string()],
            disable: vec!["shell".to_string()],
            ..Default::default()
        };

        let child = CapToolPermissions {
            enable: Vec::new(),
            disable: vec!["file_write".to_string()],
            ..Default::default()
        };

        let merged = base.merge(&child);
        assert!(merged.disable.contains(&"shell".to_string()));
        assert!(merged.disable.contains(&"file_write".to_string()));
    }

    #[test]
    fn test_tool_permissions_merge_no_duplicates() {
        let base = CapToolPermissions {
            enable: vec!["file_read".to_string()],
            disable: vec!["shell".to_string()],
            auto_approve_paths: vec!["src/**".to_string()],
            blocked_commands: vec!["rm -rf".to_string()],
            ..Default::default()
        };

        let child = CapToolPermissions {
            enable: vec!["file_read".to_string()],          // duplicate
            disable: vec!["shell".to_string()],             // duplicate
            auto_approve_paths: vec!["src/**".to_string()], // duplicate
            blocked_commands: vec!["rm -rf".to_string()],   // duplicate
            ..Default::default()
        };

        let merged = base.merge(&child);
        assert_eq!(
            merged.enable.iter().filter(|x| *x == "file_read").count(),
            1
        );
        assert_eq!(merged.disable.iter().filter(|x| *x == "shell").count(), 1);
        assert_eq!(
            merged
                .auto_approve_paths
                .iter()
                .filter(|x| *x == "src/**")
                .count(),
            1
        );
        assert_eq!(
            merged
                .blocked_commands
                .iter()
                .filter(|x| *x == "rm -rf")
                .count(),
            1
        );
    }

    #[test]
    fn test_is_tool_enabled_default() {
        let perms = CapToolPermissions::default();
        assert!(perms.is_tool_enabled("file_read"));
        assert!(perms.is_tool_enabled("shell"));
        assert!(perms.is_tool_enabled("any_tool"));
    }

    #[test]
    fn test_is_tool_enabled_with_enable_list() {
        let perms = CapToolPermissions {
            enable: vec!["file_read".to_string(), "file_edit".to_string()],
            ..Default::default()
        };

        assert!(perms.is_tool_enabled("file_read"));
        assert!(perms.is_tool_enabled("file_edit"));
        assert!(!perms.is_tool_enabled("shell"));
        assert!(!perms.is_tool_enabled("other"));
    }

    #[test]
    fn test_is_tool_enabled_with_disable_list() {
        let perms = CapToolPermissions {
            disable: vec!["shell".to_string()],
            ..Default::default()
        };

        assert!(perms.is_tool_enabled("file_read"));
        assert!(perms.is_tool_enabled("file_edit"));
        assert!(!perms.is_tool_enabled("shell"));
    }

    #[test]
    fn test_is_tool_enabled_disable_takes_precedence() {
        let perms = CapToolPermissions {
            enable: vec!["shell".to_string()],
            disable: vec!["shell".to_string()],
            ..Default::default()
        };

        // Disable takes precedence over enable
        assert!(!perms.is_tool_enabled("shell"));
    }

    #[test]
    fn test_toml_parsing() {
        let toml = r#"
name = "test-cap"
description = "A test capability"
version = "1.0.0"
priority = 10
extends = ["base"]

[tool_permissions]
enable = ["file_read", "file_edit"]
require_edit_confirmation = false

system_prompt = """
You are a helpful assistant.
"""
"#;

        let cap: Cap = toml::from_str(toml).unwrap();
        assert_eq!(cap.name, "test-cap");
        assert_eq!(cap.priority, 10);
        assert!(!cap.tool_permissions.require_edit_confirmation);
    }

    #[test]
    fn test_toml_parsing_minimal() {
        let toml = r#"
name = "minimal"
"#;

        let cap: Cap = toml::from_str(toml).unwrap();
        assert_eq!(cap.name, "minimal");
        assert_eq!(cap.version, "1.0.0");
        assert!(cap.extends.is_empty());
    }

    #[test]
    fn test_toml_parsing_with_model_preferences() {
        let toml = r#"
name = "model-test"

[model]
preferred_model = "claude-opus-4-5-20250514"
temperature = 0.7
max_tokens = 4096
"#;

        let cap: Cap = toml::from_str(toml).unwrap();
        assert!(cap.model.is_some());
        let model = cap.model.unwrap();
        assert_eq!(
            model.preferred_model,
            Some("claude-opus-4-5-20250514".to_string())
        );
        assert_eq!(model.temperature, Some(0.7));
        assert_eq!(model.max_tokens, Some(4096));
    }

    #[test]
    fn test_cap_serialization_roundtrip() {
        let cap = Cap::new("roundtrip")
            .with_description("Test roundtrip")
            .with_priority(5);

        let toml_str = toml::to_string(&cap).unwrap();
        let parsed: Cap = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.name, "roundtrip");
        assert_eq!(parsed.description, "Test roundtrip");
        assert_eq!(parsed.priority, 5);
    }

    #[test]
    fn test_cap_debug_and_clone() {
        let cap = Cap::new("debug-test").with_priority(10);
        let debug_str = format!("{:?}", cap);
        assert!(debug_str.contains("debug-test"));

        let cloned = cap.clone();
        assert_eq!(cloned.name, "debug-test");
        assert_eq!(cloned.priority, 10);
    }

    #[test]
    fn test_tool_permissions_debug_and_clone() {
        let perms = CapToolPermissions::permissive();
        let debug_str = format!("{:?}", perms);
        assert!(debug_str.contains("require_edit_confirmation"));

        let cloned = perms.clone();
        assert_eq!(
            cloned.require_edit_confirmation,
            perms.require_edit_confirmation
        );
    }

    #[test]
    fn test_cap_model_preferences_debug_and_clone() {
        let model = CapModelPreferences {
            preferred_model: Some("test".to_string()),
            temperature: Some(0.5),
            max_tokens: Some(1000),
        };

        let debug_str = format!("{:?}", model);
        assert!(debug_str.contains("test"));

        let cloned = model.clone();
        assert_eq!(cloned.preferred_model, Some("test".to_string()));
    }

    #[test]
    fn test_tool_permissions_merge_blocked_commands() {
        let base = CapToolPermissions {
            blocked_commands: vec!["rm -rf".to_string(), "dd".to_string()],
            ..Default::default()
        };

        let other = CapToolPermissions {
            blocked_commands: vec!["dd".to_string(), "mkfs".to_string()], // dd is duplicate
            ..Default::default()
        };

        let merged = base.merge(&other);

        // Should have all unique blocked commands
        assert!(merged.blocked_commands.contains(&"rm -rf".to_string()));
        assert!(merged.blocked_commands.contains(&"dd".to_string()));
        assert!(merged.blocked_commands.contains(&"mkfs".to_string()));
        // dd should only appear once
        assert_eq!(
            merged
                .blocked_commands
                .iter()
                .filter(|c| *c == "dd")
                .count(),
            1
        );
    }

    #[test]
    fn test_tool_permissions_merge_auto_approve_paths() {
        let base = CapToolPermissions {
            auto_approve_paths: vec!["/tmp".to_string(), "/var/log".to_string()],
            ..Default::default()
        };

        let other = CapToolPermissions {
            auto_approve_paths: vec!["/var/log".to_string(), "/home".to_string()], // /var/log is duplicate
            ..Default::default()
        };

        let merged = base.merge(&other);

        assert!(merged.auto_approve_paths.contains(&"/tmp".to_string()));
        assert!(merged.auto_approve_paths.contains(&"/var/log".to_string()));
        assert!(merged.auto_approve_paths.contains(&"/home".to_string()));
        // /var/log should only appear once
        assert_eq!(
            merged
                .auto_approve_paths
                .iter()
                .filter(|p| *p == "/var/log")
                .count(),
            1
        );
    }

    #[test]
    fn test_tool_permissions_merge_all_fields() {
        let base = CapToolPermissions {
            enable: vec!["tool_a".to_string()],
            disable: vec!["tool_x".to_string()],
            require_edit_confirmation: true,
            require_shell_confirmation: true,
            auto_approve_paths: vec!["/tmp".to_string()],
            blocked_commands: vec!["cmd1".to_string()],
        };

        let other = CapToolPermissions {
            enable: vec!["tool_b".to_string()],
            disable: vec!["tool_y".to_string()],
            require_edit_confirmation: false,
            require_shell_confirmation: false,
            auto_approve_paths: vec!["/home".to_string()],
            blocked_commands: vec!["cmd2".to_string()],
        };

        let merged = base.merge(&other);

        // Enable/disable lists are merged (union)
        assert!(merged.enable.contains(&"tool_a".to_string()));
        assert!(merged.enable.contains(&"tool_b".to_string()));
        assert!(merged.disable.contains(&"tool_x".to_string()));
        assert!(merged.disable.contains(&"tool_y".to_string()));

        // Booleans take the other cap's value
        assert!(!merged.require_edit_confirmation);
        assert!(!merged.require_shell_confirmation);

        // Paths and commands are merged
        assert!(merged.auto_approve_paths.contains(&"/tmp".to_string()));
        assert!(merged.auto_approve_paths.contains(&"/home".to_string()));
        assert!(merged.blocked_commands.contains(&"cmd1".to_string()));
        assert!(merged.blocked_commands.contains(&"cmd2".to_string()));
    }
}
