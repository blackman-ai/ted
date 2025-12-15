// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Shell command execution tool
//!
//! Executes shell commands with timeout and safety checks.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use crate::error::Result;
use crate::indexer::extract_paths_from_text;
use crate::llm::provider::ToolDefinition;
use crate::tools::{PermissionRequest, SchemaBuilder, Tool, ToolContext, ToolResult};

/// Tool for executing shell commands
pub struct ShellTool {
    /// Patterns that are always blocked
    blocked_patterns: HashSet<String>,
    /// Default timeout in seconds
    default_timeout: u64,
}

impl ShellTool {
    /// Create a new shell tool with default settings
    pub fn new() -> Self {
        let mut blocked_patterns = HashSet::new();

        // Block dangerous commands
        blocked_patterns.insert("rm -rf /".to_string());
        blocked_patterns.insert("rm -rf /*".to_string());
        blocked_patterns.insert("mkfs".to_string());
        blocked_patterns.insert(":(){:|:&};:".to_string()); // Fork bomb
        blocked_patterns.insert("> /dev/sda".to_string());
        blocked_patterns.insert("dd if=/dev/zero of=/dev".to_string());

        Self {
            blocked_patterns,
            default_timeout: 120,
        }
    }

    /// Check if a command is blocked
    fn is_blocked(&self, command: &str) -> bool {
        let lower = command.to_lowercase();
        self.blocked_patterns.iter().any(|p| lower.contains(p))
    }
}

impl Default for ShellTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "shell".to_string(),
            description: "Execute a shell command and return the output. Commands run in the working directory with a timeout.".to_string(),
            input_schema: SchemaBuilder::new()
                .string("command", "The shell command to execute", true)
                .integer("timeout", "Timeout in seconds (default: 120, max: 600)", false)
                .build(),
        }
    }

    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let command = input["command"].as_str().ok_or_else(|| {
            crate::error::TedError::InvalidInput("command is required".to_string())
        })?;

        let timeout_secs = input["timeout"]
            .as_u64()
            .unwrap_or(self.default_timeout)
            .min(600); // Max 10 minutes

        // Check for blocked commands
        if self.is_blocked(command) {
            return Ok(ToolResult::error(
                tool_use_id,
                "This command has been blocked for safety reasons.",
            ));
        }

        // Execute the command
        let result = timeout(
            Duration::from_secs(timeout_secs),
            Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(&context.working_directory)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let exit_code = output.status.code().unwrap_or(-1);

                let mut result_text = String::new();

                // Add exit code
                result_text.push_str(&format!("Exit code: {}\n", exit_code));

                // Add stdout if present
                if !stdout.is_empty() {
                    result_text.push_str("\n--- stdout ---\n");
                    // Truncate if too long
                    if stdout.len() > 30000 {
                        result_text.push_str(&stdout[..30000]);
                        result_text.push_str("\n... (output truncated)");
                    } else {
                        result_text.push_str(&stdout);
                    }
                }

                // Add stderr if present
                if !stderr.is_empty() {
                    result_text.push_str("\n--- stderr ---\n");
                    if stderr.len() > 10000 {
                        result_text.push_str(&stderr[..10000]);
                        result_text.push_str("\n... (output truncated)");
                    } else {
                        result_text.push_str(&stderr);
                    }
                }

                // Extract file paths from output and emit recall events
                // This captures files mentioned by commands like ls, find, cat, etc.
                let project_root = context.project_root.as_deref();
                let mut found_paths: Vec<PathBuf> = Vec::new();

                // Extract from stdout
                found_paths.extend(extract_paths_from_text(&stdout, project_root));
                // Extract from stderr (errors often mention file paths)
                found_paths.extend(extract_paths_from_text(&stderr, project_root));

                // Deduplicate
                let unique_paths: Vec<PathBuf> = found_paths
                    .into_iter()
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();

                if !unique_paths.is_empty() {
                    context.emit_search_match(unique_paths);
                }

                if exit_code == 0 {
                    Ok(ToolResult::success(tool_use_id, result_text))
                } else {
                    // Non-zero exit is still a "success" in terms of execution,
                    // but we include the error in the output
                    Ok(ToolResult::success(tool_use_id, result_text))
                }
            }
            Ok(Err(e)) => Ok(ToolResult::error(
                tool_use_id,
                format!("Failed to execute command: {}", e),
            )),
            Err(_) => Ok(ToolResult::error(
                tool_use_id,
                format!("Command timed out after {} seconds", timeout_secs),
            )),
        }
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let command = input["command"].as_str().unwrap_or("unknown");

        // Determine if this is a destructive command
        let is_destructive = command.contains("rm ")
            || command.contains("mv ")
            || command.contains("git push")
            || command.contains("git reset")
            || command.contains("npm publish")
            || command.contains("cargo publish");

        Some(PermissionRequest {
            tool_name: "shell".to_string(),
            action_description: format!("Execute: {}", command),
            affected_paths: vec![],
            is_destructive,
        })
    }

    fn requires_permission(&self) -> bool {
        true // Shell commands always require permission
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn create_test_context(temp_dir: &TempDir) -> ToolContext {
        ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            Uuid::new_v4(),
            true,
        )
    }

    #[test]
    fn test_tool_name() {
        let tool = ShellTool::new();
        assert_eq!(tool.name(), "shell");
    }

    #[test]
    fn test_tool_definition() {
        let tool = ShellTool::new();
        let def = tool.definition();
        assert_eq!(def.name, "shell");
        assert!(def.description.contains("Execute"));
    }

    #[test]
    fn test_requires_permission() {
        let tool = ShellTool::new();
        assert!(tool.requires_permission());
    }

    #[test]
    fn test_default() {
        let tool = ShellTool::default();
        assert_eq!(tool.name(), "shell");
    }

    #[test]
    fn test_is_blocked() {
        let tool = ShellTool::new();
        assert!(tool.is_blocked("rm -rf /"));
        assert!(tool.is_blocked("rm -rf /*"));
        assert!(tool.is_blocked("mkfs.ext4 /dev/sda"));
        assert!(tool.is_blocked(":(){:|:&};:"));
        assert!(!tool.is_blocked("ls -la"));
        assert!(!tool.is_blocked("echo hello"));
    }

    #[test]
    fn test_permission_request() {
        let tool = ShellTool::new();
        let input = serde_json::json!({"command": "echo hello"});
        let request = tool.permission_request(&input).unwrap();
        assert_eq!(request.tool_name, "shell");
        assert!(request.action_description.contains("echo hello"));
        assert!(!request.is_destructive);
    }

    #[test]
    fn test_permission_request_destructive() {
        let tool = ShellTool::new();
        let input = serde_json::json!({"command": "rm file.txt"});
        let request = tool.permission_request(&input).unwrap();
        assert!(request.is_destructive);

        let input = serde_json::json!({"command": "git push origin main"});
        let request = tool.permission_request(&input).unwrap();
        assert!(request.is_destructive);
    }

    #[tokio::test]
    async fn test_execute_simple_command() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"command": "echo hello"}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let output = result.output_text();
        assert!(output.contains("hello"));
        assert!(output.contains("Exit code: 0"));
    }

    #[tokio::test]
    async fn test_execute_command_with_exit_code() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"command": "exit 42"}),
                &context,
            )
            .await
            .unwrap();

        // Non-zero exit is still a "success" in terms of execution
        assert!(!result.is_error());
        assert!(result.output_text().contains("Exit code: 42"));
    }

    #[tokio::test]
    async fn test_execute_command_with_stderr() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"command": "echo error >&2"}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("stderr"));
        assert!(result.output_text().contains("error"));
    }

    #[tokio::test]
    async fn test_execute_blocked_command() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"command": "rm -rf /"}),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("blocked"));
    }

    #[tokio::test]
    async fn test_execute_missing_command() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_in_working_directory() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test_file.txt"), "content").unwrap();

        let tool = ShellTool::new();
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"command": "ls"}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("test_file.txt"));
    }

    #[tokio::test]
    async fn test_execute_with_timeout() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_test_context(&temp_dir);

        // Use a very short timeout
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "command": "sleep 10",
                    "timeout": 1
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("timed out"));
    }

    #[tokio::test]
    async fn test_execute_multi_line_output() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"command": "printf 'line1\\nline2\\nline3'"}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let output = result.output_text();
        assert!(output.contains("line1"));
        assert!(output.contains("line2"));
        assert!(output.contains("line3"));
    }
}
