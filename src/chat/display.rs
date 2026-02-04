// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Display formatting for chat interface
//!
//! This module provides testable formatting functions for displaying
//! chat interface elements. Functions return formatted strings rather
//! than writing directly to stdout, making them easy to test.

use crate::tools::ToolResult;

/// Format tool invocation display
pub struct ToolInvocationDisplay {
    pub tool_name: String,
    pub summary: String,
}

/// Format a tool invocation for display
pub fn format_tool_invocation(tool_name: &str, input: &serde_json::Value) -> ToolInvocationDisplay {
    let summary = format_tool_input_summary(tool_name, input);
    ToolInvocationDisplay {
        tool_name: tool_name.to_string(),
        summary,
    }
}

/// Format tool input into a summary string based on tool type
pub fn format_tool_input_summary(tool_name: &str, input: &serde_json::Value) -> String {
    match tool_name {
        "file_read" | "read_file" => {
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                format!("Reading {}", truncate_path(path, 50))
            } else {
                "Reading file".to_string()
            }
        }
        "file_write" | "write_file" => {
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                format!("Writing {}", truncate_path(path, 50))
            } else {
                "Writing file".to_string()
            }
        }
        "file_edit" | "edit_file" => {
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                format!("Editing {}", truncate_path(path, 50))
            } else {
                "Editing file".to_string()
            }
        }
        "shell" | "bash" | "execute_command" => {
            if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                format!("$ {}", truncate_string(cmd, 50))
            } else {
                "Executing command".to_string()
            }
        }
        "glob" | "find_files" => {
            if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                format!("Finding {}", pattern)
            } else {
                "Finding files".to_string()
            }
        }
        "grep" | "search" => {
            if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                format!("Searching for '{}'", truncate_string(pattern, 30))
            } else {
                "Searching".to_string()
            }
        }
        "spawn_agent" => {
            if let Some(task) = input.get("task").and_then(|v| v.as_str()) {
                format!("Agent: {}", truncate_string(task, 40))
            } else {
                "Spawning agent".to_string()
            }
        }
        "plan_create" | "create_plan" => {
            if let Some(title) = input.get("title").and_then(|v| v.as_str()) {
                format!("Creating plan: {}", truncate_string(title, 40))
            } else {
                "Creating plan".to_string()
            }
        }
        "plan_update" | "update_plan" => {
            if let Some(title) = input.get("title").and_then(|v| v.as_str()) {
                format!("Updating plan: {}", truncate_string(title, 40))
            } else {
                "Updating plan".to_string()
            }
        }
        _ => {
            // Generic formatting for unknown tools
            format!("Running {}", tool_name)
        }
    }
}

/// Truncate a string for display with ellipsis
pub fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Truncate a path for display, keeping the filename visible
pub fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }

    // Try to keep the filename
    if let Some(idx) = path.rfind('/') {
        let filename = &path[idx + 1..];
        if filename.len() < max_len - 4 {
            // ".../" + filename
            return format!("...{}", &path[idx..]);
        }
    }

    // Fall back to simple truncation
    format!("{}...", &path[..max_len.saturating_sub(3)])
}

/// Format tool result for display
pub struct ToolResultDisplay {
    pub is_error: bool,
    pub summary: String,
    pub output_preview: Option<String>,
}

/// Format a tool result for display
pub fn format_tool_result(tool_name: &str, result: &ToolResult) -> ToolResultDisplay {
    let is_error = result.is_error();
    let output = result.output_text();

    let (summary, output_preview) = if is_error {
        let error_preview = extract_error_preview(output, 100);
        (format!("Error: {}", error_preview), None)
    } else {
        match tool_name {
            "file_read" | "read_file" => {
                let line_count = output.lines().count();
                (
                    format!("Read {} lines", line_count),
                    Some(preview_content(output, 3)),
                )
            }
            "file_write" | "write_file" => ("File written successfully".to_string(), None),
            "file_edit" | "edit_file" => ("File edited successfully".to_string(), None),
            "shell" | "bash" | "execute_command" => {
                let line_count = output.lines().count();
                if line_count == 0 || output.trim().is_empty() {
                    ("Command completed (no output)".to_string(), None)
                } else {
                    (
                        format!("Output ({} lines)", line_count),
                        Some(preview_content(output, 5)),
                    )
                }
            }
            "glob" | "find_files" => {
                let file_count = output.lines().count();
                (
                    format!("Found {} files", file_count),
                    Some(preview_content(output, 3)),
                )
            }
            "grep" | "search" => {
                let match_count = output.lines().count();
                (
                    format!("Found {} matches", match_count),
                    Some(preview_content(output, 3)),
                )
            }
            "spawn_agent" => {
                if output.trim().is_empty() {
                    ("Agent completed".to_string(), None)
                } else {
                    (
                        "Agent completed".to_string(),
                        Some(preview_content(output, 5)),
                    )
                }
            }
            _ => {
                if output.trim().is_empty() {
                    ("Completed".to_string(), None)
                } else {
                    let line_count = output.lines().count();
                    (
                        format!("Completed ({} lines)", line_count),
                        Some(preview_content(output, 3)),
                    )
                }
            }
        }
    };

    ToolResultDisplay {
        is_error,
        summary,
        output_preview,
    }
}

/// Extract a preview of error message
fn extract_error_preview(error: &str, max_len: usize) -> String {
    let first_line = error.lines().next().unwrap_or("");
    truncate_string(first_line, max_len)
}

/// Preview content with limited lines
fn preview_content(content: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = content.lines().take(max_lines).collect();
    let preview = lines
        .iter()
        .map(|line| truncate_string(line, 80))
        .collect::<Vec<_>>()
        .join("\n");

    let total_lines = content.lines().count();
    if total_lines > max_lines {
        format!("{}\n... ({} more lines)", preview, total_lines - max_lines)
    } else {
        preview
    }
}

/// Format shell output for display
pub struct ShellOutputDisplay {
    pub status: ShellStatus,
    pub output_lines: Vec<String>,
    pub total_lines: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellStatus {
    Success,
    Error(i32),
}

/// Format shell command output for display
pub fn format_shell_output(
    stdout: &str,
    stderr: &str,
    exit_code: i32,
    max_lines: usize,
) -> ShellOutputDisplay {
    let status = if exit_code == 0 {
        ShellStatus::Success
    } else {
        ShellStatus::Error(exit_code)
    };

    // Combine stdout and stderr
    let all_lines: Vec<String> = stdout
        .lines()
        .chain(stderr.lines())
        .map(|s| truncate_string(s, 120))
        .collect();

    let total_lines = all_lines.len();
    let truncated = total_lines > max_lines;

    let output_lines = if truncated {
        // Show first half and last half
        let half = max_lines / 2;
        let mut lines: Vec<String> = all_lines.iter().take(half).cloned().collect();
        lines.push(format!("... ({} more lines) ...", total_lines - max_lines));
        lines.extend(all_lines.iter().skip(total_lines - half).cloned());
        lines
    } else {
        all_lines
    };

    ShellOutputDisplay {
        status,
        output_lines,
        total_lines,
        truncated,
    }
}

/// Format welcome message
pub fn format_welcome(
    provider_name: &str,
    model: &str,
    trust_mode: bool,
    session_id: &str,
    caps: &[String],
) -> String {
    let mut output = String::new();
    output.push_str("Ted - AI coding assistant\n");
    output.push_str(&format!("Provider: {} | Model: {}\n", provider_name, model));
    output.push_str(&format!(
        "Session: {}\n",
        &session_id[..8.min(session_id.len())]
    ));

    if !caps.is_empty() {
        output.push_str(&format!("Caps: {}\n", caps.join(", ")));
    }

    if trust_mode {
        output.push_str("Trust mode: enabled (tools run without confirmation)\n");
    }

    output.push_str("\nType /help for commands, or start chatting.\n");
    output
}

/// Format cap badge for display
pub fn format_cap_badge(cap_name: &str) -> String {
    format!("[{}]", cap_name)
}

/// Format multiple cap badges
pub fn format_cap_badges(caps: &[String]) -> String {
    caps.iter()
        .map(|c| format_cap_badge(c))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Format session info for display
pub struct SessionDisplay {
    pub id_short: String,
    pub date: String,
    pub message_count: usize,
    pub summary: String,
    pub is_current: bool,
}

/// Format a session for list display
pub fn format_session_item(
    session_id: &str,
    date: &str,
    message_count: usize,
    summary: Option<&str>,
    is_current: bool,
) -> SessionDisplay {
    let summary_text = summary.unwrap_or("(no summary)");
    let truncated_summary = truncate_string(summary_text, 40);

    SessionDisplay {
        id_short: session_id[..8.min(session_id.len())].to_string(),
        date: date.to_string(),
        message_count,
        summary: truncated_summary,
        is_current,
    }
}

/// Format response prefix with caps
pub fn format_response_prefix(caps: &[String]) -> String {
    if caps.is_empty() {
        "Ted: ".to_string()
    } else {
        format!("Ted {}: ", format_cap_badges(caps))
    }
}

/// Format rate limit warning
pub fn format_rate_limit_warning(delay_secs: u64, attempt: u32, max_retries: u32) -> String {
    format!(
        "Rate limited. Retrying in {} seconds... (attempt {}/{})",
        delay_secs, attempt, max_retries
    )
}

/// Format context overflow warning
pub fn format_context_overflow_warning(current: u32, limit: u32) -> String {
    format!(
        "Context too long ({} tokens > {} limit). Auto-trimming older messages...",
        current, limit
    )
}

/// Format interrupt message
pub fn format_interrupt_message() -> String {
    "Interrupted\nType your next message or use /help for commands.".to_string()
}

/// Format model switch message
pub fn format_model_switch(model: &str) -> String {
    let mut msg = format!("Switched to model: {}", model);
    if model.contains("haiku") {
        msg.push_str("\nTip: Haiku has higher rate limits and is faster.");
    }
    msg
}

/// Format new session message
pub fn format_new_session(session_id: &str) -> String {
    format!(
        "Started new session: {}\nContext cleared. Ready for a fresh conversation.",
        &session_id[..8.min(session_id.len())]
    )
}

/// Format session switch message
pub fn format_session_switch(
    session_id: &str,
    summary: Option<&str>,
    message_count: usize,
    date: &str,
) -> String {
    let mut msg = format!(
        "Switching to session: {}",
        &session_id[..8.min(session_id.len())]
    );
    if let Some(s) = summary {
        msg.push_str(&format!("\n  {}", s));
    }
    msg.push_str(&format!("\n  {} messages from {}", message_count, date));
    msg.push_str("\n\nSession switched. Note: Previous messages are not loaded into context.");
    msg.push_str("\nUse /stats to see session info.");
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== truncate_string tests ====================

    #[test]
    fn test_truncate_string_short() {
        assert_eq!(truncate_string("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_string_exact() {
        assert_eq!(truncate_string("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_string_long() {
        assert_eq!(truncate_string("hello world", 8), "hello...");
    }

    #[test]
    fn test_truncate_string_empty() {
        assert_eq!(truncate_string("", 10), "");
    }

    #[test]
    fn test_truncate_string_very_short_max() {
        assert_eq!(truncate_string("hello", 3), "...");
    }

    // ==================== truncate_path tests ====================

    #[test]
    fn test_truncate_path_short() {
        assert_eq!(
            truncate_path("/home/user/file.txt", 50),
            "/home/user/file.txt"
        );
    }

    #[test]
    fn test_truncate_path_long_keeps_filename() {
        let path = "/very/long/path/to/some/directory/file.txt";
        let truncated = truncate_path(path, 20);
        assert!(truncated.contains("file.txt"));
    }

    #[test]
    fn test_truncate_path_no_slash() {
        // When max_len is 10, we get 7 chars + "..." = 10 chars
        assert_eq!(truncate_path("verylongfilenamehere.txt", 10), "verylon...");
    }

    // ==================== format_tool_input_summary tests ====================

    #[test]
    fn test_format_tool_input_summary_file_read() {
        let input = serde_json::json!({"path": "/home/user/test.txt"});
        let summary = format_tool_input_summary("file_read", &input);
        assert!(summary.contains("Reading"));
        assert!(summary.contains("test.txt"));
    }

    #[test]
    fn test_format_tool_input_summary_file_write() {
        let input = serde_json::json!({"path": "/home/user/output.txt"});
        let summary = format_tool_input_summary("file_write", &input);
        assert!(summary.contains("Writing"));
    }

    #[test]
    fn test_format_tool_input_summary_shell() {
        let input = serde_json::json!({"command": "ls -la"});
        let summary = format_tool_input_summary("shell", &input);
        assert!(summary.contains("$ ls -la"));
    }

    #[test]
    fn test_format_tool_input_summary_glob() {
        let input = serde_json::json!({"pattern": "*.rs"});
        let summary = format_tool_input_summary("glob", &input);
        assert!(summary.contains("Finding"));
        assert!(summary.contains("*.rs"));
    }

    #[test]
    fn test_format_tool_input_summary_grep() {
        let input = serde_json::json!({"pattern": "TODO"});
        let summary = format_tool_input_summary("grep", &input);
        assert!(summary.contains("Searching"));
        assert!(summary.contains("TODO"));
    }

    #[test]
    fn test_format_tool_input_summary_spawn_agent() {
        let input = serde_json::json!({"task": "Review the code"});
        let summary = format_tool_input_summary("spawn_agent", &input);
        assert!(summary.contains("Agent"));
        assert!(summary.contains("Review"));
    }

    #[test]
    fn test_format_tool_input_summary_unknown_tool() {
        let input = serde_json::json!({});
        let summary = format_tool_input_summary("unknown_tool", &input);
        assert!(summary.contains("Running unknown_tool"));
    }

    #[test]
    fn test_format_tool_input_summary_missing_path() {
        let input = serde_json::json!({});
        let summary = format_tool_input_summary("file_read", &input);
        assert_eq!(summary, "Reading file");
    }

    // ==================== format_tool_invocation tests ====================

    #[test]
    fn test_format_tool_invocation() {
        let input = serde_json::json!({"path": "/test/file.txt"});
        let display = format_tool_invocation("file_read", &input);
        assert_eq!(display.tool_name, "file_read");
        assert!(display.summary.contains("Reading"));
    }

    // ==================== format_shell_output tests ====================

    #[test]
    fn test_format_shell_output_success() {
        let display = format_shell_output("line1\nline2", "", 0, 10);
        assert_eq!(display.status, ShellStatus::Success);
        assert_eq!(display.total_lines, 2);
        assert!(!display.truncated);
    }

    #[test]
    fn test_format_shell_output_error() {
        let display = format_shell_output("", "error message", 1, 10);
        assert_eq!(display.status, ShellStatus::Error(1));
    }

    #[test]
    fn test_format_shell_output_truncated() {
        let stdout = (0..20)
            .map(|i| format!("line{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let display = format_shell_output(&stdout, "", 0, 10);
        assert!(display.truncated);
        assert_eq!(display.total_lines, 20);
    }

    #[test]
    fn test_format_shell_output_combined() {
        let display = format_shell_output("stdout", "stderr", 0, 10);
        assert_eq!(display.total_lines, 2);
    }

    // ==================== format_welcome tests ====================

    #[test]
    fn test_format_welcome_basic() {
        let welcome = format_welcome("anthropic", "claude-3", false, "abc12345", &[]);
        assert!(welcome.contains("Ted"));
        assert!(welcome.contains("anthropic"));
        assert!(welcome.contains("claude-3"));
        assert!(welcome.contains("abc12345"));
    }

    #[test]
    fn test_format_welcome_with_caps() {
        let caps = vec!["base".to_string(), "code".to_string()];
        let welcome = format_welcome("anthropic", "claude-3", false, "abc12345", &caps);
        assert!(welcome.contains("base"));
        assert!(welcome.contains("code"));
    }

    #[test]
    fn test_format_welcome_trust_mode() {
        let welcome = format_welcome("anthropic", "claude-3", true, "abc12345", &[]);
        assert!(welcome.contains("Trust mode"));
    }

    // ==================== format_cap_badge tests ====================

    #[test]
    fn test_format_cap_badge() {
        assert_eq!(format_cap_badge("base"), "[base]");
        assert_eq!(format_cap_badge("code"), "[code]");
    }

    #[test]
    fn test_format_cap_badges() {
        let caps = vec!["base".to_string(), "code".to_string()];
        assert_eq!(format_cap_badges(&caps), "[base] [code]");
    }

    #[test]
    fn test_format_cap_badges_empty() {
        let caps: Vec<String> = vec![];
        assert_eq!(format_cap_badges(&caps), "");
    }

    // ==================== format_session_item tests ====================

    #[test]
    fn test_format_session_item() {
        let display = format_session_item(
            "abc12345-6789",
            "2025-01-01 12:00",
            10,
            Some("Test session"),
            false,
        );
        assert_eq!(display.id_short, "abc12345");
        assert_eq!(display.date, "2025-01-01 12:00");
        assert_eq!(display.message_count, 10);
        assert_eq!(display.summary, "Test session");
        assert!(!display.is_current);
    }

    #[test]
    fn test_format_session_item_no_summary() {
        let display = format_session_item("abc12345", "2025-01-01", 5, None, true);
        assert_eq!(display.summary, "(no summary)");
        assert!(display.is_current);
    }

    #[test]
    fn test_format_session_item_long_summary() {
        let long_summary = "a".repeat(100);
        let display = format_session_item("abc12345", "2025-01-01", 5, Some(&long_summary), false);
        assert!(display.summary.len() <= 43); // 40 + "..."
    }

    // ==================== format_response_prefix tests ====================

    #[test]
    fn test_format_response_prefix_no_caps() {
        assert_eq!(format_response_prefix(&[]), "Ted: ");
    }

    #[test]
    fn test_format_response_prefix_with_caps() {
        let caps = vec!["base".to_string()];
        assert_eq!(format_response_prefix(&caps), "Ted [base]: ");
    }

    #[test]
    fn test_format_response_prefix_multiple_caps() {
        let caps = vec!["base".to_string(), "code".to_string()];
        assert_eq!(format_response_prefix(&caps), "Ted [base] [code]: ");
    }

    // ==================== format_rate_limit_warning tests ====================

    #[test]
    fn test_format_rate_limit_warning() {
        let warning = format_rate_limit_warning(5, 1, 3);
        assert!(warning.contains("5 seconds"));
        assert!(warning.contains("attempt 1/3"));
    }

    // ==================== format_context_overflow_warning tests ====================

    #[test]
    fn test_format_context_overflow_warning() {
        let warning = format_context_overflow_warning(250000, 200000);
        assert!(warning.contains("250000"));
        assert!(warning.contains("200000"));
    }

    // ==================== format_interrupt_message tests ====================

    #[test]
    fn test_format_interrupt_message() {
        let msg = format_interrupt_message();
        assert!(msg.contains("Interrupted"));
        assert!(msg.contains("/help"));
    }

    // ==================== format_model_switch tests ====================

    #[test]
    fn test_format_model_switch_regular() {
        let msg = format_model_switch("claude-3-5-sonnet");
        assert!(msg.contains("claude-3-5-sonnet"));
        assert!(!msg.contains("Tip"));
    }

    #[test]
    fn test_format_model_switch_haiku() {
        let msg = format_model_switch("claude-3-5-haiku-20241022");
        assert!(msg.contains("haiku"));
        assert!(msg.contains("Tip"));
        assert!(msg.contains("rate limits"));
    }

    // ==================== format_new_session tests ====================

    #[test]
    fn test_format_new_session() {
        let msg = format_new_session("abc12345-6789-0123");
        assert!(msg.contains("abc12345"));
        assert!(msg.contains("Context cleared"));
    }

    // ==================== format_session_switch tests ====================

    #[test]
    fn test_format_session_switch() {
        let msg = format_session_switch(
            "abc12345-6789",
            Some("Working on feature X"),
            15,
            "2025-01-01 12:00",
        );
        assert!(msg.contains("abc12345"));
        assert!(msg.contains("Working on feature X"));
        assert!(msg.contains("15 messages"));
        assert!(msg.contains("2025-01-01 12:00"));
    }

    #[test]
    fn test_format_session_switch_no_summary() {
        let msg = format_session_switch("abc12345", None, 5, "2025-01-01");
        assert!(msg.contains("abc12345"));
        assert!(!msg.contains("None"));
    }

    // ==================== ShellStatus tests ====================

    #[test]
    fn test_shell_status_debug() {
        let status = ShellStatus::Success;
        let debug = format!("{:?}", status);
        assert!(debug.contains("Success"));
    }

    #[test]
    fn test_shell_status_error_code() {
        let status = ShellStatus::Error(127);
        if let ShellStatus::Error(code) = status {
            assert_eq!(code, 127);
        } else {
            panic!("Expected Error variant");
        }
    }

    #[test]
    fn test_shell_status_eq() {
        assert_eq!(ShellStatus::Success, ShellStatus::Success);
        assert_eq!(ShellStatus::Error(1), ShellStatus::Error(1));
        assert_ne!(ShellStatus::Success, ShellStatus::Error(0));
        assert_ne!(ShellStatus::Error(1), ShellStatus::Error(2));
    }

    // ==================== preview_content tests (via format_tool_result) ====================

    // Note: preview_content is private, but we can test it through format_tool_result

    // ==================== Edge cases ====================

    #[test]
    fn test_empty_inputs() {
        assert_eq!(truncate_string("", 10), "");
        assert_eq!(truncate_path("", 10), "");
        let display = format_shell_output("", "", 0, 10);
        assert_eq!(display.total_lines, 0);
    }

    #[test]
    fn test_unicode_handling() {
        assert_eq!(truncate_string("hello 世界", 20), "hello 世界");
        // Note: byte-based truncation may cut unicode characters
    }

    #[test]
    fn test_special_characters() {
        let input = serde_json::json!({"command": "echo 'test' | grep -v ''"});
        let summary = format_tool_input_summary("shell", &input);
        assert!(summary.contains("echo"));
    }

    #[test]
    fn test_long_path_truncation() {
        let path = "/very/very/very/long/path/to/some/deeply/nested/directory/structure/file.txt";
        let truncated = truncate_path(path, 30);
        assert!(truncated.len() <= 33); // 30 + "..."
    }
}
