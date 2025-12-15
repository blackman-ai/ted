// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Permission system for tools
//!
//! Handles requesting and managing permissions for tool actions.

use crossterm::style::{Color, ResetColor, SetForegroundColor};
use crossterm::ExecutableCommand;
use std::io::{self, Write};

/// Request for permission to perform an action
#[derive(Debug, Clone)]
pub struct PermissionRequest {
    /// Name of the tool requesting permission
    pub tool_name: String,
    /// Human-readable description of the action
    pub action_description: String,
    /// Paths that will be affected
    pub affected_paths: Vec<String>,
    /// Whether this action is destructive/dangerous
    pub is_destructive: bool,
}

/// Response to a permission request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionResponse {
    /// Allow this specific action
    Allow,
    /// Deny this specific action
    Deny,
    /// Allow all actions for this tool in this session
    AllowAll,
    /// Allow all actions for all tools in this session (trust mode)
    TrustAll,
}

/// Permission manager
pub struct PermissionManager {
    /// Tools that have been granted "allow all" permission
    allowed_tools: std::collections::HashSet<String>,
    /// Whether trust mode is enabled
    trust_mode: bool,
    /// Whether to auto-approve read operations
    auto_approve_reads: bool,
}

impl PermissionManager {
    /// Create a new permission manager
    pub fn new() -> Self {
        Self {
            allowed_tools: std::collections::HashSet::new(),
            trust_mode: false,
            auto_approve_reads: true,
        }
    }

    /// Create with trust mode enabled
    pub fn with_trust_mode() -> Self {
        Self {
            allowed_tools: std::collections::HashSet::new(),
            trust_mode: true,
            auto_approve_reads: true,
        }
    }

    /// Check if a tool needs permission
    pub fn needs_permission(&self, tool_name: &str) -> bool {
        if self.trust_mode {
            return false;
        }
        if self.allowed_tools.contains(tool_name) {
            return false;
        }
        // Auto-approve reads
        if self.auto_approve_reads
            && (tool_name == "file_read" || tool_name == "glob" || tool_name == "grep")
        {
            return false;
        }
        true
    }

    /// Request permission from the user
    pub fn request_permission(
        &mut self,
        request: &PermissionRequest,
    ) -> io::Result<PermissionResponse> {
        let mut stdout = io::stdout();

        // Display the request
        println!();
        stdout.execute(SetForegroundColor(Color::Yellow))?;
        print!("⚠ ");
        stdout.execute(ResetColor)?;

        println!("Tool '{}' wants to:", request.tool_name);
        println!("  {}", request.action_description);

        if !request.affected_paths.is_empty() {
            println!("  Affected paths:");
            for path in &request.affected_paths {
                if request.is_destructive {
                    stdout.execute(SetForegroundColor(Color::Red))?;
                }
                println!("    - {}", path);
                stdout.execute(ResetColor)?;
            }
        }

        // Prompt for response
        println!();
        print!("Allow? [y]es / [n]o / [a]llow all for this tool / [t]rust all: ");
        stdout.flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let response = match input.trim().to_lowercase().as_str() {
            "y" | "yes" => PermissionResponse::Allow,
            "n" | "no" => PermissionResponse::Deny,
            "a" | "allow" => {
                self.allowed_tools.insert(request.tool_name.clone());
                PermissionResponse::AllowAll
            }
            "t" | "trust" => {
                self.trust_mode = true;
                PermissionResponse::TrustAll
            }
            _ => PermissionResponse::Deny,
        };

        println!();
        Ok(response)
    }

    /// Enable trust mode
    pub fn enable_trust_mode(&mut self) {
        self.trust_mode = true;
    }

    /// Check if trust mode is enabled
    pub fn is_trust_mode(&self) -> bool {
        self.trust_mode
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_request_creation() {
        let request = PermissionRequest {
            tool_name: "file_write".to_string(),
            action_description: "Write to file".to_string(),
            affected_paths: vec!["/tmp/test.txt".to_string()],
            is_destructive: false,
        };

        assert_eq!(request.tool_name, "file_write");
        assert_eq!(request.affected_paths.len(), 1);
        assert!(!request.is_destructive);
    }

    #[test]
    fn test_permission_request_destructive() {
        let request = PermissionRequest {
            tool_name: "shell".to_string(),
            action_description: "Execute rm -rf command".to_string(),
            affected_paths: vec!["/home/user/".to_string()],
            is_destructive: true,
        };

        assert!(request.is_destructive);
    }

    #[test]
    fn test_permission_response_variants() {
        assert_eq!(PermissionResponse::Allow, PermissionResponse::Allow);
        assert_eq!(PermissionResponse::Deny, PermissionResponse::Deny);
        assert_eq!(PermissionResponse::AllowAll, PermissionResponse::AllowAll);
        assert_eq!(PermissionResponse::TrustAll, PermissionResponse::TrustAll);
        assert_ne!(PermissionResponse::Allow, PermissionResponse::Deny);
    }

    #[test]
    fn test_permission_manager_new() {
        let manager = PermissionManager::new();
        assert!(!manager.trust_mode);
        assert!(manager.auto_approve_reads);
        assert!(manager.allowed_tools.is_empty());
    }

    #[test]
    fn test_permission_manager_default() {
        let manager = PermissionManager::default();
        assert!(!manager.trust_mode);
    }

    #[test]
    fn test_permission_manager_with_trust_mode() {
        let manager = PermissionManager::with_trust_mode();
        assert!(manager.trust_mode);
        assert!(manager.is_trust_mode());
    }

    #[test]
    fn test_needs_permission_trust_mode() {
        let manager = PermissionManager::with_trust_mode();
        // In trust mode, nothing needs permission
        assert!(!manager.needs_permission("file_write"));
        assert!(!manager.needs_permission("shell"));
        assert!(!manager.needs_permission("anything"));
    }

    #[test]
    fn test_needs_permission_read_tools_auto_approved() {
        let manager = PermissionManager::new();
        // Read tools are auto-approved
        assert!(!manager.needs_permission("file_read"));
        assert!(!manager.needs_permission("glob"));
        assert!(!manager.needs_permission("grep"));
    }

    #[test]
    fn test_needs_permission_write_tools_require_permission() {
        let manager = PermissionManager::new();
        // Write tools need permission
        assert!(manager.needs_permission("file_write"));
        assert!(manager.needs_permission("file_edit"));
        assert!(manager.needs_permission("shell"));
    }

    #[test]
    fn test_needs_permission_allowed_tool() {
        let mut manager = PermissionManager::new();
        manager.allowed_tools.insert("file_write".to_string());
        // Once a tool is in the allowed set, it doesn't need permission
        assert!(!manager.needs_permission("file_write"));
        // But other tools still do
        assert!(manager.needs_permission("shell"));
    }

    #[test]
    fn test_enable_trust_mode() {
        let mut manager = PermissionManager::new();
        assert!(!manager.is_trust_mode());
        manager.enable_trust_mode();
        assert!(manager.is_trust_mode());
    }

    #[test]
    fn test_is_trust_mode() {
        let manager1 = PermissionManager::new();
        assert!(!manager1.is_trust_mode());

        let manager2 = PermissionManager::with_trust_mode();
        assert!(manager2.is_trust_mode());
    }

    #[test]
    fn test_permission_request_clone() {
        let request = PermissionRequest {
            tool_name: "test".to_string(),
            action_description: "test action".to_string(),
            affected_paths: vec!["/path".to_string()],
            is_destructive: true,
        };

        let cloned = request.clone();
        assert_eq!(cloned.tool_name, request.tool_name);
        assert_eq!(cloned.action_description, request.action_description);
        assert_eq!(cloned.affected_paths, request.affected_paths);
        assert_eq!(cloned.is_destructive, request.is_destructive);
    }

    #[test]
    fn test_permission_request_debug() {
        let request = PermissionRequest {
            tool_name: "test".to_string(),
            action_description: "test action".to_string(),
            affected_paths: vec![],
            is_destructive: false,
        };

        let debug_str = format!("{:?}", request);
        assert!(debug_str.contains("PermissionRequest"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_permission_response_copy() {
        let response = PermissionResponse::Allow;
        let copied = response;
        assert_eq!(response, copied);
    }

    #[test]
    fn test_permission_response_debug() {
        let response = PermissionResponse::Deny;
        let debug_str = format!("{:?}", response);
        assert!(debug_str.contains("Deny"));
    }

    #[test]
    fn test_permission_request_empty_paths() {
        let request = PermissionRequest {
            tool_name: "shell".to_string(),
            action_description: "Run a command".to_string(),
            affected_paths: vec![],
            is_destructive: false,
        };

        assert!(request.affected_paths.is_empty());
    }

    #[test]
    fn test_permission_request_multiple_paths() {
        let request = PermissionRequest {
            tool_name: "shell".to_string(),
            action_description: "Copy files".to_string(),
            affected_paths: vec![
                "/path/one".to_string(),
                "/path/two".to_string(),
                "/path/three".to_string(),
            ],
            is_destructive: false,
        };

        assert_eq!(request.affected_paths.len(), 3);
    }

    #[test]
    fn test_permission_manager_multiple_allowed_tools() {
        let mut manager = PermissionManager::new();
        manager.allowed_tools.insert("file_write".to_string());
        manager.allowed_tools.insert("shell".to_string());
        manager.allowed_tools.insert("file_edit".to_string());

        assert!(!manager.needs_permission("file_write"));
        assert!(!manager.needs_permission("shell"));
        assert!(!manager.needs_permission("file_edit"));
        assert!(manager.needs_permission("some_other_tool"));
    }

    // ===== Additional PermissionManager Tests =====

    #[test]
    fn test_permission_manager_auto_approve_reads_disabled() {
        let mut manager = PermissionManager::new();
        manager.auto_approve_reads = false;

        // With auto_approve_reads disabled, read tools need permission
        assert!(manager.needs_permission("file_read"));
        assert!(manager.needs_permission("glob"));
        assert!(manager.needs_permission("grep"));
    }

    #[test]
    fn test_permission_manager_allowed_tool_takes_precedence() {
        let mut manager = PermissionManager::new();
        // Even if auto_approve_reads is false, explicitly allowed tools don't need permission
        manager.auto_approve_reads = false;
        manager.allowed_tools.insert("file_read".to_string());

        assert!(!manager.needs_permission("file_read"));
    }

    #[test]
    fn test_permission_manager_trust_mode_overrides_all() {
        let mut manager = PermissionManager::new();
        manager.auto_approve_reads = false;
        manager.enable_trust_mode();

        // Trust mode overrides everything
        assert!(!manager.needs_permission("file_read"));
        assert!(!manager.needs_permission("file_write"));
        assert!(!manager.needs_permission("shell"));
        assert!(!manager.needs_permission("dangerous_tool"));
    }

    #[test]
    fn test_permission_request_with_many_paths() {
        let request = PermissionRequest {
            tool_name: "shell".to_string(),
            action_description: "Copy multiple files".to_string(),
            affected_paths: vec![
                "/path/1".to_string(),
                "/path/2".to_string(),
                "/path/3".to_string(),
                "/path/4".to_string(),
                "/path/5".to_string(),
            ],
            is_destructive: false,
        };

        assert_eq!(request.affected_paths.len(), 5);
    }

    #[test]
    fn test_permission_response_clone() {
        let response1 = PermissionResponse::Allow;
        let response2 = response1;
        assert_eq!(response1, response2);

        let response3 = PermissionResponse::AllowAll;
        let response4 = response3;
        assert_eq!(response3, response4);
    }

    #[test]
    fn test_permission_response_all_variants_distinct() {
        let variants = [
            PermissionResponse::Allow,
            PermissionResponse::Deny,
            PermissionResponse::AllowAll,
            PermissionResponse::TrustAll,
        ];

        // Each variant should be distinct from the others
        for i in 0..variants.len() {
            for j in 0..variants.len() {
                if i != j {
                    assert_ne!(variants[i], variants[j]);
                }
            }
        }
    }

    #[test]
    fn test_permission_manager_clear_and_readd_tools() {
        let mut manager = PermissionManager::new();

        // Add a tool
        manager.allowed_tools.insert("file_write".to_string());
        assert!(!manager.needs_permission("file_write"));

        // Clear all allowed tools
        manager.allowed_tools.clear();
        assert!(manager.needs_permission("file_write"));

        // Re-add the tool
        manager.allowed_tools.insert("file_write".to_string());
        assert!(!manager.needs_permission("file_write"));
    }

    #[test]
    fn test_permission_manager_case_sensitive() {
        let mut manager = PermissionManager::new();
        manager.allowed_tools.insert("File_Write".to_string());

        // Tool names are case-sensitive
        assert!(!manager.needs_permission("File_Write"));
        assert!(manager.needs_permission("file_write"));
        assert!(manager.needs_permission("FILE_WRITE"));
    }

    #[test]
    fn test_permission_request_special_characters_in_paths() {
        let request = PermissionRequest {
            tool_name: "shell".to_string(),
            action_description: "Execute command".to_string(),
            affected_paths: vec![
                "/path/with spaces/file.txt".to_string(),
                "/path/with'quotes/file.txt".to_string(),
                "/path/with\"doublequotes\"/file.txt".to_string(),
                "/path/with$dollar/file.txt".to_string(),
            ],
            is_destructive: true,
        };

        assert_eq!(request.affected_paths.len(), 4);
        assert!(request.is_destructive);
    }

    #[test]
    fn test_permission_manager_default_impl() {
        let manager1 = PermissionManager::default();
        let manager2 = PermissionManager::new();

        // Both should have the same initial state
        assert_eq!(manager1.trust_mode, manager2.trust_mode);
        assert_eq!(manager1.auto_approve_reads, manager2.auto_approve_reads);
        assert_eq!(manager1.allowed_tools.len(), manager2.allowed_tools.len());
    }

    #[test]
    fn test_permission_request_unicode() {
        let request = PermissionRequest {
            tool_name: "file_write".to_string(),
            action_description: "Write file with unicode: 你好世界 🌍".to_string(),
            affected_paths: vec!["/path/to/文件.txt".to_string()],
            is_destructive: false,
        };

        assert!(request.action_description.contains("🌍"));
        assert!(request.affected_paths[0].contains("文件"));
    }

    // ===== Response Parsing Tests =====

    /// Helper to parse permission response from string (mirrors logic in request_permission)
    fn parse_response(input: &str) -> PermissionResponse {
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => PermissionResponse::Allow,
            "n" | "no" => PermissionResponse::Deny,
            "a" | "allow" => PermissionResponse::AllowAll,
            "t" | "trust" => PermissionResponse::TrustAll,
            _ => PermissionResponse::Deny,
        }
    }

    #[test]
    fn test_parse_response_yes() {
        assert_eq!(parse_response("y"), PermissionResponse::Allow);
        assert_eq!(parse_response("yes"), PermissionResponse::Allow);
        assert_eq!(parse_response("Y"), PermissionResponse::Allow);
        assert_eq!(parse_response("YES"), PermissionResponse::Allow);
        assert_eq!(parse_response("Yes"), PermissionResponse::Allow);
    }

    #[test]
    fn test_parse_response_no() {
        assert_eq!(parse_response("n"), PermissionResponse::Deny);
        assert_eq!(parse_response("no"), PermissionResponse::Deny);
        assert_eq!(parse_response("N"), PermissionResponse::Deny);
        assert_eq!(parse_response("NO"), PermissionResponse::Deny);
        assert_eq!(parse_response("No"), PermissionResponse::Deny);
    }

    #[test]
    fn test_parse_response_allow_all() {
        assert_eq!(parse_response("a"), PermissionResponse::AllowAll);
        assert_eq!(parse_response("allow"), PermissionResponse::AllowAll);
        assert_eq!(parse_response("A"), PermissionResponse::AllowAll);
        assert_eq!(parse_response("ALLOW"), PermissionResponse::AllowAll);
        assert_eq!(parse_response("Allow"), PermissionResponse::AllowAll);
    }

    #[test]
    fn test_parse_response_trust_all() {
        assert_eq!(parse_response("t"), PermissionResponse::TrustAll);
        assert_eq!(parse_response("trust"), PermissionResponse::TrustAll);
        assert_eq!(parse_response("T"), PermissionResponse::TrustAll);
        assert_eq!(parse_response("TRUST"), PermissionResponse::TrustAll);
        assert_eq!(parse_response("Trust"), PermissionResponse::TrustAll);
    }

    #[test]
    fn test_parse_response_unknown_defaults_to_deny() {
        assert_eq!(parse_response(""), PermissionResponse::Deny);
        assert_eq!(parse_response("x"), PermissionResponse::Deny);
        assert_eq!(parse_response("unknown"), PermissionResponse::Deny);
        assert_eq!(parse_response("maybe"), PermissionResponse::Deny);
        assert_eq!(parse_response("123"), PermissionResponse::Deny);
        assert_eq!(parse_response("!@#"), PermissionResponse::Deny);
    }

    #[test]
    fn test_parse_response_with_whitespace() {
        assert_eq!(parse_response("  y  "), PermissionResponse::Allow);
        assert_eq!(parse_response("\ny\n"), PermissionResponse::Allow);
        assert_eq!(parse_response("\t y \t"), PermissionResponse::Allow);
        assert_eq!(parse_response("  yes  "), PermissionResponse::Allow);
    }

    #[test]
    fn test_parse_response_partial_words() {
        // Only exact matches should work
        assert_eq!(parse_response("ye"), PermissionResponse::Deny);
        assert_eq!(parse_response("yess"), PermissionResponse::Deny);
        assert_eq!(parse_response("nn"), PermissionResponse::Deny);
        assert_eq!(parse_response("all"), PermissionResponse::Deny); // Not "allow"
        assert_eq!(parse_response("tru"), PermissionResponse::Deny);
    }

    // ===== Permission Manager State Machine Tests =====

    #[test]
    fn test_permission_manager_workflow() {
        let mut manager = PermissionManager::new();

        // Initially, write tools need permission
        assert!(manager.needs_permission("file_write"));

        // After allowing a tool, it no longer needs permission
        manager.allowed_tools.insert("file_write".to_string());
        assert!(!manager.needs_permission("file_write"));

        // Other tools still need permission
        assert!(manager.needs_permission("shell"));

        // After enabling trust mode, nothing needs permission
        manager.enable_trust_mode();
        assert!(!manager.needs_permission("shell"));
        assert!(!manager.needs_permission("any_tool"));
    }

    #[test]
    fn test_permission_manager_read_vs_write_tools() {
        let manager = PermissionManager::new();

        // Read tools are auto-approved by default
        let read_tools = ["file_read", "glob", "grep"];
        for tool in &read_tools {
            assert!(
                !manager.needs_permission(tool),
                "{} should be auto-approved",
                tool
            );
        }

        // Write tools require permission
        let write_tools = ["file_write", "file_edit", "shell"];
        for tool in &write_tools {
            assert!(
                manager.needs_permission(tool),
                "{} should require permission",
                tool
            );
        }
    }

    #[test]
    fn test_permission_manager_trust_mode_overrides() {
        let mut manager = PermissionManager::new();

        // Disable auto_approve_reads to make the test more explicit
        manager.auto_approve_reads = false;

        // Now read tools need permission
        assert!(manager.needs_permission("file_read"));

        // Enable trust mode
        manager.enable_trust_mode();

        // Trust mode overrides everything
        assert!(!manager.needs_permission("file_read"));
        assert!(!manager.needs_permission("file_write"));
        assert!(!manager.needs_permission("any_dangerous_tool"));
    }

    #[test]
    fn test_permission_request_format_verification() {
        // Verify the structure of a permission request for display purposes
        let request = PermissionRequest {
            tool_name: "shell".to_string(),
            action_description: "Execute: rm -rf /tmp/test".to_string(),
            affected_paths: vec!["/tmp/test".to_string()],
            is_destructive: true,
        };

        // These fields are used in the display
        assert!(!request.tool_name.is_empty());
        assert!(!request.action_description.is_empty());
        assert!(request.is_destructive);
        assert!(!request.affected_paths.is_empty());
    }

    #[test]
    fn test_permission_manager_allowed_tools_isolation() {
        let mut manager = PermissionManager::new();

        // Add some allowed tools
        manager.allowed_tools.insert("tool_a".to_string());
        manager.allowed_tools.insert("tool_b".to_string());

        // Verify they are independent
        assert!(!manager.needs_permission("tool_a"));
        assert!(!manager.needs_permission("tool_b"));
        assert!(manager.needs_permission("tool_c"));

        // Removing one doesn't affect the other
        manager.allowed_tools.remove("tool_a");
        assert!(manager.needs_permission("tool_a"));
        assert!(!manager.needs_permission("tool_b"));
    }

    #[test]
    fn test_permission_response_state_transitions() {
        let mut manager = PermissionManager::new();

        // Simulate responding with AllowAll
        let tool_name = "file_write".to_string();
        manager.allowed_tools.insert(tool_name.clone());

        // Now this tool is allowed
        assert!(!manager.needs_permission(&tool_name));
        assert_eq!(manager.allowed_tools.len(), 1);

        // Simulate responding with TrustAll
        manager.trust_mode = true;

        // Now everything is allowed
        assert!(manager.is_trust_mode());
        assert!(!manager.needs_permission("any_tool"));
    }

    #[test]
    fn test_permission_manager_empty_tool_name() {
        let manager = PermissionManager::new();

        // Empty tool name should require permission (not a read tool)
        assert!(manager.needs_permission(""));
    }

    #[test]
    fn test_permission_manager_whitespace_tool_name() {
        let mut manager = PermissionManager::new();

        // Tool names with whitespace are valid (though unusual)
        manager.allowed_tools.insert("tool with space".to_string());
        assert!(!manager.needs_permission("tool with space"));
        assert!(manager.needs_permission("tool_with_space")); // Different name
    }

    #[test]
    fn test_permission_request_many_affected_paths() {
        let request = PermissionRequest {
            tool_name: "shell".to_string(),
            action_description: "Batch operation".to_string(),
            affected_paths: (0..100).map(|i| format!("/path/{}", i)).collect(),
            is_destructive: false,
        };

        assert_eq!(request.affected_paths.len(), 100);
    }

    #[test]
    fn test_permission_manager_new_vs_default() {
        let new_manager = PermissionManager::new();
        let default_manager = PermissionManager::default();

        // Both should have the same initial state
        assert_eq!(new_manager.trust_mode, default_manager.trust_mode);
        assert_eq!(
            new_manager.auto_approve_reads,
            default_manager.auto_approve_reads
        );
        assert_eq!(
            new_manager.allowed_tools.len(),
            default_manager.allowed_tools.len()
        );
    }

    #[test]
    fn test_permission_manager_with_trust_mode_is_trust() {
        let manager = PermissionManager::with_trust_mode();

        // Should start with trust mode enabled
        assert!(manager.trust_mode);
        assert!(manager.is_trust_mode());

        // Everything should be allowed
        assert!(!manager.needs_permission("anything"));
    }
}
