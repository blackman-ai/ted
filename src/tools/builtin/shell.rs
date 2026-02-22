// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Shell command execution tool
//!
//! Executes shell commands with timeout and safety checks.

use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
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
        blocked_patterns.insert("sudo ".to_string());
        blocked_patterns.insert("shutdown".to_string());
        blocked_patterns.insert("reboot".to_string());
        blocked_patterns.insert("poweroff".to_string());
        blocked_patterns.insert("halt".to_string());
        blocked_patterns.insert("init 0".to_string());
        blocked_patterns.insert("init 6".to_string());

        Self {
            blocked_patterns,
            default_timeout: 60, // 60 seconds default - enough for npm/build commands
        }
    }

    /// Check if a command is blocked
    fn is_blocked(&self, command: &str) -> bool {
        let lower = command.to_lowercase();
        self.blocked_patterns.iter().any(|p| lower.contains(p))
            || Self::is_dangerous_rm_root(&lower)
    }

    fn is_dangerous_rm_root(command: &str) -> bool {
        let tokens: Vec<&str> = command.split_whitespace().collect();
        let mut i = 0;

        while i < tokens.len() {
            let current = tokens[i];
            if current != "rm" {
                i += 1;
                continue;
            }

            let mut recursive = false;
            let mut root_target = false;
            let mut j = i + 1;

            while j < tokens.len() {
                let token = tokens[j].trim_matches(|ch: char| {
                    ch == '"' || ch == '\'' || ch == '`' || ch == ';' || ch == '|' || ch == '&'
                });

                if token.is_empty() {
                    j += 1;
                    continue;
                }

                if let Some(stripped) = token.strip_prefix('-') {
                    let is_short_option = !token.starts_with("--");
                    if token == "--recursive"
                        || token == "-r"
                        || token == "-R"
                        || (is_short_option && stripped.contains('r'))
                    {
                        recursive = true;
                    }
                    j += 1;
                    continue;
                }

                if token == "/" || token == "/*" {
                    root_target = true;
                }

                j += 1;
            }

            if recursive && root_target {
                return true;
            }

            i = j;
        }

        false
    }

    /// Check if a command likely mutates files or system state.
    fn is_mutating_command(command: &str) -> bool {
        let lower = command.to_lowercase();
        if lower.contains("find ") && lower.contains("-delete") {
            return true;
        }
        if lower.contains("find ") && lower.contains("-exec") && lower.contains("rm") {
            return true;
        }
        if lower.contains("xargs") && lower.contains("rm ") {
            return true;
        }

        let mutating_indicators = [
            "rm ",
            "mv ",
            "cp ",
            "mkdir ",
            "rmdir ",
            "touch ",
            "truncate ",
            "chmod ",
            "chown ",
            "dd ",
            "install ",
            "git clean",
            "git reset --hard",
            "sed -i",
            "perl -i",
            "tee ",
            ">>",
            " >",
            "1>",
            "2>",
        ];

        mutating_indicators
            .iter()
            .any(|indicator| lower.contains(indicator))
    }

    /// Expand a shell path token to an absolute path when possible.
    fn expand_path_token(token: &str) -> Option<PathBuf> {
        if token == "~" || token.starts_with("~/") {
            let home = std::env::var_os("HOME")?;
            let mut path = PathBuf::from(home);
            if token.len() > 2 {
                path.push(token.trim_start_matches("~/"));
            }
            return Some(path);
        }

        if token.starts_with('/') {
            return Some(PathBuf::from(token));
        }

        None
    }

    fn is_allowed_external_path(path: &Path) -> bool {
        path == Path::new("/dev/null") || path.starts_with("/tmp") || path.starts_with("/var/tmp")
    }

    fn has_relative_parent_escape(command: &str) -> bool {
        command
            .split(|ch: char| ch.is_whitespace() || [';', '|', '&', '(', ')'].contains(&ch))
            .map(|token| token.trim_matches('"').trim_matches('\'').trim_matches('`'))
            .any(|token| {
                token == ".."
                    || token.starts_with("../")
                    || token.contains("/../")
                    || token.ends_with("/..")
            })
    }

    /// Check whether the command attempts to mutate paths outside the project root.
    fn violates_workspace_boundary(&self, command: &str, context: &ToolContext) -> bool {
        if context.trust_mode || !Self::is_mutating_command(command) {
            return false;
        }

        if Self::has_relative_parent_escape(command) {
            return true;
        }

        let allowed_root = context
            .project_root
            .as_ref()
            .unwrap_or(&context.working_directory);
        let token_re =
            Regex::new(r#"(?P<path>(?:~|/)[^\s'"`;|&()]*)"#).expect("path regex must be valid");

        for capture in token_re.captures_iter(command) {
            let token = &capture["path"];
            let Some(path) = Self::expand_path_token(token) else {
                continue;
            };

            if path.starts_with(allowed_root) || Self::is_allowed_external_path(&path) {
                continue;
            }

            return true;
        }

        // Also catch patterns like: cd /some/path && rm file
        let cd_re =
            Regex::new(r#"(?:(?:^|&&|\|\||;)\s*)cd\s+([^\s;|&]+)"#).expect("cd regex must work");
        for capture in cd_re.captures_iter(command) {
            let Some(target) = capture.get(1) else {
                continue;
            };
            let token = target.as_str().trim_matches('"').trim_matches('\'');
            let Some(path) = Self::expand_path_token(token) else {
                continue;
            };

            if path.starts_with(allowed_root) || Self::is_allowed_external_path(&path) {
                continue;
            }

            return true;
        }

        false
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
        // Flexible parameter name lookup - support common alternatives models might use
        let command = input["command"]
            .as_str()
            .or_else(|| input["cmd"].as_str())
            .or_else(|| input["run"].as_str())
            .or_else(|| input["exec"].as_str())
            .ok_or_else(|| {
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

        if self.violates_workspace_boundary(command, context) {
            let workspace = context
                .project_root
                .as_ref()
                .unwrap_or(&context.working_directory)
                .display()
                .to_string();
            return Ok(ToolResult::error(
                tool_use_id,
                format!(
                    "Refusing to run a mutating shell command outside workspace '{}'. Use trust mode if you need system-wide changes.",
                    workspace
                ),
            ));
        }

        // Note: We previously blocked echo commands that looked like user communication,
        // but this caused more issues than it solved. The model sometimes uses echo
        // for status messages, and blocking them confused the model's flow.
        // The system prompt already instructs the model not to use echo for communication.

        // Spawn the command with piped stdout/stderr
        // Use stdin(null) to prevent commands from waiting for input
        let mut child = match Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&context.working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!("Failed to spawn command: {}", e),
                ));
            }
        };

        // Get stdout and stderr handles
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Collect output while streaming
        let mut stdout_output = String::new();
        let mut stderr_output = String::new();

        // Create tasks to read stdout and stderr concurrently
        // Use raw byte reading instead of lines() to handle npm-style progress (uses \r)

        let stdout_task = async {
            if let Some(mut stdout) = stdout {
                let mut output = String::new();
                let mut buf = [0u8; 1024];
                loop {
                    match stdout.read(&mut buf).await {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            if let Ok(text) = std::str::from_utf8(&buf[..n]) {
                                // Emit streaming output
                                context.emit_shell_output("stdout", text.to_string(), false, None);
                                output.push_str(text);
                            }
                        }
                        Err(_) => break,
                    }
                }
                output
            } else {
                String::new()
            }
        };

        let stderr_task = async {
            if let Some(mut stderr) = stderr {
                let mut output = String::new();
                let mut buf = [0u8; 1024];
                loop {
                    match stderr.read(&mut buf).await {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            if let Ok(text) = std::str::from_utf8(&buf[..n]) {
                                // Emit streaming output
                                context.emit_shell_output("stderr", text.to_string(), false, None);
                                output.push_str(text);
                            }
                        }
                        Err(_) => break,
                    }
                }
                output
            } else {
                String::new()
            }
        };

        // Run with timeout
        let result = timeout(Duration::from_secs(timeout_secs), async {
            // Read stdout and stderr concurrently
            let (stdout_result, stderr_result) = tokio::join!(stdout_task, stderr_task);
            stdout_output = stdout_result;
            stderr_output = stderr_result;

            // Wait for the process to exit
            child.wait().await
        })
        .await;

        match result {
            Ok(Ok(status)) => {
                let exit_code = status.code().unwrap_or(-1);

                // Emit completion event
                context.emit_shell_output("stdout", String::new(), true, Some(exit_code));

                let mut result_text = String::new();

                // Add exit code
                result_text.push_str(&format!("Exit code: {}\n", exit_code));

                // Add stdout if present
                if !stdout_output.is_empty() {
                    result_text.push_str("\n--- stdout ---\n");
                    // Truncate if too long
                    if stdout_output.len() > 30000 {
                        result_text.push_str(&stdout_output[..30000]);
                        result_text.push_str("\n... (output truncated)");
                    } else {
                        result_text.push_str(&stdout_output);
                    }
                }

                // Add stderr if present
                if !stderr_output.is_empty() {
                    result_text.push_str("\n--- stderr ---\n");
                    if stderr_output.len() > 10000 {
                        result_text.push_str(&stderr_output[..10000]);
                        result_text.push_str("\n... (output truncated)");
                    } else {
                        result_text.push_str(&stderr_output);
                    }
                }

                // Extract file paths from output and emit recall events
                // This captures files mentioned by commands like ls, find, cat, etc.
                let project_root = context.project_root.as_deref();
                let mut found_paths: Vec<PathBuf> = Vec::new();

                // Extract from stdout
                found_paths.extend(extract_paths_from_text(&stdout_output, project_root));
                // Extract from stderr (errors often mention file paths)
                found_paths.extend(extract_paths_from_text(&stderr_output, project_root));

                // Deduplicate
                let unique_paths: Vec<PathBuf> = found_paths
                    .into_iter()
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();

                if !unique_paths.is_empty() {
                    context.emit_search_match(unique_paths);
                }

                Ok(ToolResult::success(tool_use_id, result_text))
            }
            Ok(Err(e)) => {
                context.emit_shell_output("stderr", format!("Error: {}\n", e), true, Some(-1));
                Ok(ToolResult::error(
                    tool_use_id,
                    format!("Failed to execute command: {}", e),
                ))
            }
            Err(_) => {
                // Kill the process on timeout
                let _ = child.kill().await;
                context.emit_shell_output(
                    "stderr",
                    format!("Command timed out after {} seconds\n", timeout_secs),
                    true,
                    Some(-1),
                );
                Ok(ToolResult::error(
                    tool_use_id,
                    format!("Command timed out after {} seconds", timeout_secs),
                ))
            }
        }
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let command = input["command"].as_str().unwrap_or("unknown");

        // Determine if this is a destructive command
        let is_destructive = Self::is_mutating_command(command)
            || command.contains("rm ")
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

    fn create_untrusted_context(temp_dir: &TempDir) -> ToolContext {
        ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            Uuid::new_v4(),
            false,
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
        assert!(tool.is_blocked("rm -r -f /"));
        assert!(tool.is_blocked("rm --recursive /"));
        assert!(tool.is_blocked("rm -rf --no-preserve-root /"));
        assert!(tool.is_blocked("mkfs.ext4 /dev/sda"));
        assert!(tool.is_blocked(":(){:|:&};:"));
        assert!(tool.is_blocked("sudo apt install ripgrep"));
        assert!(!tool.is_blocked("rm -rf tmp/ted-test"));
        assert!(!tool.is_blocked("ls -la"));
        assert!(!tool.is_blocked("echo hello"));
    }

    #[test]
    fn test_workspace_boundary_detects_mutating_absolute_path() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_untrusted_context(&temp_dir);
        assert!(tool.violates_workspace_boundary("rm -rf /opt", &context));
    }

    #[test]
    fn test_workspace_boundary_allows_non_mutating_absolute_path() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_untrusted_context(&temp_dir);
        assert!(!tool.violates_workspace_boundary("cat /etc/hosts", &context));
    }

    #[test]
    fn test_workspace_boundary_allows_workspace_paths() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_untrusted_context(&temp_dir);
        let command = format!("touch {}/file.txt", temp_dir.path().display());
        assert!(!tool.violates_workspace_boundary(&command, &context));
    }

    #[test]
    fn test_is_mutating_command_find_delete() {
        assert!(ShellTool::is_mutating_command(
            "find . -name '*.tmp' -delete"
        ));
        assert!(!ShellTool::is_mutating_command("find . -name '*.rs'"));
    }

    #[test]
    fn test_is_mutating_command_find_exec_rm_and_xargs_rm() {
        assert!(ShellTool::is_mutating_command(
            "find . -name '*.tmp' -exec rm {} \\;"
        ));
        assert!(ShellTool::is_mutating_command(
            "find . -name '*.tmp' | xargs rm -f"
        ));
    }

    #[test]
    fn test_workspace_boundary_detects_find_delete_root() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_untrusted_context(&temp_dir);
        assert!(tool.violates_workspace_boundary("find / -name '*.tmp' -delete", &context));
    }

    #[test]
    fn test_workspace_boundary_detects_relative_parent_escape() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_untrusted_context(&temp_dir);
        assert!(tool.violates_workspace_boundary("rm -rf ../outside", &context));
        assert!(tool.violates_workspace_boundary("mv ./file ../target", &context));
    }

    #[test]
    fn test_workspace_boundary_allows_relative_parent_for_non_mutating_command() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_untrusted_context(&temp_dir);
        assert!(!tool.violates_workspace_boundary("cat ../README.md", &context));
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

        let input = serde_json::json!({"command": "find . -name '*.tmp' -exec rm {} \\;"});
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
    async fn test_execute_blocks_mutating_outside_workspace_when_untrusted() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_untrusted_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"command": "touch /opt/ted-should-not-exist.txt"}),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("outside workspace"));
    }

    // Note: test_blocks_echo_for_communication was removed because the blocking behavior
    // was intentionally removed (see comment in execute() around line 103).
    // Echo blocking caused more issues than it solved.

    #[tokio::test]
    async fn test_allows_legitimate_echo() {
        let temp_dir = TempDir::new().unwrap();
        let tool = ShellTool::new();
        let context = create_test_context(&temp_dir);

        // Should allow simple echo commands
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"command": "echo hello"}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());

        // Should allow echo with redirection
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"command": "echo 'content' > test.txt"}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());

        // Should allow echo with variables
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"command": "echo $HOME"}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
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
