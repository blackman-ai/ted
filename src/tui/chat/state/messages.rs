// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Message state for the chat TUI
//!
//! Represents messages for display in the chat interface.

use std::time::{Instant, SystemTime};

use serde_json::Value;
use uuid::Uuid;

/// Safely truncate a string at a character boundary, appending "..." if truncated.
/// This avoids panics when slicing multi-byte UTF-8 characters.
pub fn truncate_string(s: &str, max_chars: usize) -> String {
    if s.chars().count() > max_chars {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}

/// Role of a message participant
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl MessageRole {
    pub fn label(&self) -> &'static str {
        match self {
            MessageRole::User => "you",
            MessageRole::Assistant => "ted",
            MessageRole::System => "system",
        }
    }
}

/// Status of a tool call
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallStatus {
    /// Tool is currently executing
    Running,
    /// Tool completed successfully
    Success,
    /// Tool failed
    Failed,
    /// Tool was cancelled
    Cancelled,
}

impl ToolCallStatus {
    pub fn indicator(&self) -> char {
        match self {
            ToolCallStatus::Running => '⏳',
            ToolCallStatus::Success => '✓',
            ToolCallStatus::Failed => '✗',
            ToolCallStatus::Cancelled => '⊘',
        }
    }
}

/// A tool call for display
#[derive(Debug, Clone)]
pub struct DisplayToolCall {
    /// Unique ID of the tool call
    pub id: String,
    /// Name of the tool
    pub name: String,
    /// Input parameters (as JSON)
    pub input: Value,
    /// Summary of the input for display
    pub input_summary: String,
    /// Status of the tool call
    pub status: ToolCallStatus,
    /// Result preview (if completed)
    pub result_preview: Option<String>,
    /// Full result (for expanded view)
    pub result_full: Option<String>,
    /// Whether the tool call is expanded in the UI
    pub expanded: bool,
    /// Start time for duration tracking
    pub started_at: Instant,
    /// Completion time
    pub completed_at: Option<Instant>,
}

impl DisplayToolCall {
    /// Create a new tool call in running state
    pub fn new(id: String, name: String, input: Value) -> Self {
        let input_summary = Self::summarize_input(&name, &input);
        Self {
            id,
            name,
            input,
            input_summary,
            status: ToolCallStatus::Running,
            result_preview: None,
            result_full: None,
            expanded: false,
            started_at: Instant::now(),
            completed_at: None,
        }
    }

    /// Generate a summary of tool input for display
    fn summarize_input(name: &str, input: &Value) -> String {
        match name {
            "file_read" | "glob" | "grep" => {
                if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                    path.to_string()
                } else if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                    pattern.to_string()
                } else {
                    "(no path)".to_string()
                }
            }
            "file_write" | "file_edit" => {
                if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                    path.to_string()
                } else {
                    "(no path)".to_string()
                }
            }
            "shell" => {
                if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                    truncate_string(cmd, 40)
                } else {
                    "(no command)".to_string()
                }
            }
            "spawn_agent" => {
                let agent_type = input
                    .get("agent_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let task = input
                    .get("task")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no task)");
                let task_short = truncate_string(task, 30);
                format!("{}: {}", agent_type, task_short)
            }
            _ => {
                // Generic: show first string value
                if let Some(obj) = input.as_object() {
                    for (_, v) in obj {
                        if let Some(s) = v.as_str() {
                            return truncate_string(s, 40);
                        }
                    }
                }
                "".to_string()
            }
        }
    }

    /// Mark the tool call as completed successfully
    pub fn complete_success(
        &mut self,
        result_preview: Option<String>,
        result_full: Option<String>,
    ) {
        self.status = ToolCallStatus::Success;
        self.result_preview = result_preview;
        self.result_full = result_full;
        self.completed_at = Some(Instant::now());
    }

    /// Mark the tool call as failed
    pub fn complete_failed(&mut self, error: String) {
        self.status = ToolCallStatus::Failed;
        self.result_preview = Some(error.clone());
        self.result_full = Some(error);
        self.completed_at = Some(Instant::now());
    }

    /// Set progress text directly (for spawn_agent with pre-formatted status)
    pub fn set_progress_text(&mut self, text: &str) {
        // Only update if still running
        if self.status == ToolCallStatus::Running {
            self.result_preview = Some(text.to_string());
        }
    }

    /// Get elapsed time
    pub fn elapsed_secs(&self) -> f64 {
        if let Some(completed) = self.completed_at {
            completed.duration_since(self.started_at).as_secs_f64()
        } else {
            self.started_at.elapsed().as_secs_f64()
        }
    }
}

/// A message for display in the chat
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    /// Unique ID
    pub id: Uuid,
    /// Role (user, assistant, system)
    pub role: MessageRole,
    /// Text content
    pub content: String,
    /// Timestamp
    pub timestamp: SystemTime,
    /// Tool calls associated with this message
    pub tool_calls: Vec<DisplayToolCall>,
    /// Whether the message is currently streaming
    pub is_streaming: bool,
    /// Caps active when this message was sent (for assistant messages)
    pub active_caps: Vec<String>,
}

impl DisplayMessage {
    /// Create a new user message
    pub fn user(content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::User,
            content,
            timestamp: SystemTime::now(),
            tool_calls: Vec::new(),
            is_streaming: false,
            active_caps: Vec::new(),
        }
    }

    /// Create a new assistant message
    pub fn assistant(content: String, caps: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content,
            timestamp: SystemTime::now(),
            tool_calls: Vec::new(),
            is_streaming: false,
            active_caps: caps,
        }
    }

    /// Create a new streaming assistant message
    pub fn assistant_streaming(caps: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: String::new(),
            timestamp: SystemTime::now(),
            tool_calls: Vec::new(),
            is_streaming: true,
            active_caps: caps,
        }
    }

    /// Create a system message
    pub fn system(content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::System,
            content,
            timestamp: SystemTime::now(),
            tool_calls: Vec::new(),
            is_streaming: false,
            active_caps: Vec::new(),
        }
    }

    /// Append content to a streaming message
    pub fn append_content(&mut self, text: &str) {
        self.content.push_str(text);
    }

    /// Mark streaming as complete
    pub fn finish_streaming(&mut self) {
        self.is_streaming = false;
    }

    /// Add a tool call to this message
    pub fn add_tool_call(&mut self, tool_call: DisplayToolCall) {
        self.tool_calls.push(tool_call);
    }

    /// Find a tool call by ID
    pub fn find_tool_call(&self, id: &str) -> Option<&DisplayToolCall> {
        self.tool_calls.iter().find(|tc| tc.id == id)
    }

    /// Find a tool call by ID (mutable)
    pub fn find_tool_call_mut(&mut self, id: &str) -> Option<&mut DisplayToolCall> {
        self.tool_calls.iter_mut().find(|tc| tc.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== truncate_string Tests =====

    #[test]
    fn test_truncate_string_short() {
        let result = truncate_string("hello", 10);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_truncate_string_exact() {
        let result = truncate_string("hello", 5);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_truncate_string_long() {
        let result = truncate_string("hello world this is a long string", 10);
        assert_eq!(result, "hello w...");
    }

    #[test]
    fn test_truncate_string_unicode() {
        let result = truncate_string("你好世界", 3);
        // Should truncate at character boundary
        assert!(result.ends_with("..."));
    }

    // ===== MessageRole Tests =====

    #[test]
    fn test_message_role_labels() {
        assert_eq!(MessageRole::User.label(), "you");
        assert_eq!(MessageRole::Assistant.label(), "ted");
        assert_eq!(MessageRole::System.label(), "system");
    }

    #[test]
    fn test_message_role_equality() {
        assert_eq!(MessageRole::User, MessageRole::User);
        assert_ne!(MessageRole::User, MessageRole::Assistant);
    }

    // ===== ToolCallStatus Tests =====

    #[test]
    fn test_tool_call_status_indicators() {
        assert_eq!(ToolCallStatus::Running.indicator(), '⏳');
        assert_eq!(ToolCallStatus::Success.indicator(), '✓');
        assert_eq!(ToolCallStatus::Failed.indicator(), '✗');
        assert_eq!(ToolCallStatus::Cancelled.indicator(), '⊘');
    }

    #[test]
    fn test_tool_call_status_equality() {
        assert_eq!(ToolCallStatus::Running, ToolCallStatus::Running);
        assert_ne!(ToolCallStatus::Running, ToolCallStatus::Success);
    }

    // ===== DisplayToolCall Tests =====

    #[test]
    fn test_display_tool_call_new() {
        let input = serde_json::json!({"path": "/src/main.rs"});
        let tc = DisplayToolCall::new("tc1".to_string(), "file_read".to_string(), input.clone());

        assert_eq!(tc.id, "tc1");
        assert_eq!(tc.name, "file_read");
        assert_eq!(tc.input, input);
        assert_eq!(tc.input_summary, "/src/main.rs");
        assert_eq!(tc.status, ToolCallStatus::Running);
        assert!(tc.result_preview.is_none());
        assert!(tc.result_full.is_none());
        assert!(!tc.expanded);
        assert!(tc.completed_at.is_none());
    }

    #[test]
    fn test_tool_call_summarize_glob() {
        let input = serde_json::json!({"pattern": "**/*.rs"});
        let tc = DisplayToolCall::new("1".to_string(), "glob".to_string(), input);
        assert_eq!(tc.input_summary, "**/*.rs");
    }

    #[test]
    fn test_tool_call_summarize_grep() {
        let input = serde_json::json!({"pattern": "TODO"});
        let tc = DisplayToolCall::new("1".to_string(), "grep".to_string(), input);
        assert_eq!(tc.input_summary, "TODO");
    }

    #[test]
    fn test_tool_call_summarize_file_write() {
        let input = serde_json::json!({"path": "/tmp/test.txt", "content": "hello"});
        let tc = DisplayToolCall::new("1".to_string(), "file_write".to_string(), input);
        assert_eq!(tc.input_summary, "/tmp/test.txt");
    }

    #[test]
    fn test_tool_call_summarize_file_edit() {
        let input = serde_json::json!({"path": "/src/lib.rs"});
        let tc = DisplayToolCall::new("1".to_string(), "file_edit".to_string(), input);
        assert_eq!(tc.input_summary, "/src/lib.rs");
    }

    #[test]
    fn test_tool_call_summarize_shell() {
        let input = serde_json::json!({"command": "cargo test"});
        let tc = DisplayToolCall::new("1".to_string(), "shell".to_string(), input);
        assert_eq!(tc.input_summary, "cargo test");
    }

    #[test]
    fn test_tool_call_summarize_shell_long() {
        let input =
            serde_json::json!({"command": "this is a very long command that should be truncated"});
        let tc = DisplayToolCall::new("1".to_string(), "shell".to_string(), input);
        assert!(tc.input_summary.ends_with("..."));
        assert!(tc.input_summary.len() <= 43); // 40 + "..."
    }

    #[test]
    fn test_tool_call_summarize_shell_no_command() {
        let input = serde_json::json!({});
        let tc = DisplayToolCall::new("1".to_string(), "shell".to_string(), input);
        assert_eq!(tc.input_summary, "(no command)");
    }

    #[test]
    fn test_tool_call_summarize_spawn_agent() {
        let input = serde_json::json!({
            "agent_type": "research",
            "task": "Find all usages of the API"
        });
        let tc = DisplayToolCall::new("1".to_string(), "spawn_agent".to_string(), input);
        assert!(tc.input_summary.starts_with("research:"));
        assert!(tc.input_summary.contains("Find all"));
    }

    #[test]
    fn test_tool_call_summarize_spawn_agent_missing_fields() {
        let input = serde_json::json!({});
        let tc = DisplayToolCall::new("1".to_string(), "spawn_agent".to_string(), input);
        assert_eq!(tc.input_summary, "unknown: (no task)");
    }

    #[test]
    fn test_tool_call_summarize_unknown_tool() {
        let input = serde_json::json!({"some_field": "some_value"});
        let tc = DisplayToolCall::new("1".to_string(), "unknown_tool".to_string(), input);
        assert_eq!(tc.input_summary, "some_value");
    }

    #[test]
    fn test_tool_call_summarize_unknown_tool_no_string() {
        let input = serde_json::json!({"number": 42});
        let tc = DisplayToolCall::new("1".to_string(), "unknown_tool".to_string(), input);
        assert_eq!(tc.input_summary, "");
    }

    #[test]
    fn test_tool_call_summarize_file_read_no_path() {
        let input = serde_json::json!({});
        let tc = DisplayToolCall::new("1".to_string(), "file_read".to_string(), input);
        assert_eq!(tc.input_summary, "(no path)");
    }

    #[test]
    fn test_tool_call_summarize_file_write_no_path() {
        let input = serde_json::json!({"content": "hello"});
        let tc = DisplayToolCall::new("1".to_string(), "file_write".to_string(), input);
        assert_eq!(tc.input_summary, "(no path)");
    }

    #[test]
    fn test_tool_call_complete_success() {
        let input = serde_json::json!({"command": "ls"});
        let mut tc = DisplayToolCall::new("1".to_string(), "shell".to_string(), input);

        tc.complete_success(
            Some("file1.txt\nfile2.txt".to_string()),
            Some("Full output here".to_string()),
        );

        assert_eq!(tc.status, ToolCallStatus::Success);
        assert_eq!(tc.result_preview, Some("file1.txt\nfile2.txt".to_string()));
        assert_eq!(tc.result_full, Some("Full output here".to_string()));
        assert!(tc.completed_at.is_some());
    }

    #[test]
    fn test_tool_call_complete_failed() {
        let input = serde_json::json!({"command": "invalid_command"});
        let mut tc = DisplayToolCall::new("1".to_string(), "shell".to_string(), input);

        tc.complete_failed("Command not found".to_string());

        assert_eq!(tc.status, ToolCallStatus::Failed);
        assert_eq!(tc.result_preview, Some("Command not found".to_string()));
        assert_eq!(tc.result_full, Some("Command not found".to_string()));
        assert!(tc.completed_at.is_some());
    }

    #[test]
    fn test_tool_call_set_progress_text() {
        let input = serde_json::json!({"task": "research"});
        let mut tc = DisplayToolCall::new("1".to_string(), "spawn_agent".to_string(), input);

        tc.set_progress_text("[5/30] → file_read");
        assert_eq!(tc.result_preview, Some("[5/30] → file_read".to_string()));

        // Complete the call
        tc.complete_success(None, None);
        let old_preview = tc.result_preview.clone();

        // set_progress_text should not update after completion
        tc.set_progress_text("[6/30] → grep");
        assert_eq!(tc.result_preview, old_preview);
    }

    #[test]
    fn test_tool_call_elapsed_secs() {
        let input = serde_json::json!({"command": "sleep 1"});
        let tc = DisplayToolCall::new("1".to_string(), "shell".to_string(), input);

        // Elapsed should be small but non-negative
        let elapsed = tc.elapsed_secs();
        assert!(elapsed >= 0.0);
        assert!(elapsed < 1.0);
    }

    #[test]
    fn test_tool_call_elapsed_secs_completed() {
        let input = serde_json::json!({"command": "ls"});
        let mut tc = DisplayToolCall::new("1".to_string(), "shell".to_string(), input);

        // Complete immediately
        tc.complete_success(None, None);

        // Elapsed should be very small
        let elapsed = tc.elapsed_secs();
        assert!(elapsed >= 0.0);
        assert!(elapsed < 0.1);
    }

    // ===== DisplayMessage Tests =====

    #[test]
    fn test_display_message_user() {
        let msg = DisplayMessage::user("Hello".to_string());
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        assert!(!msg.is_streaming);
        assert!(msg.tool_calls.is_empty());
        assert!(msg.active_caps.is_empty());
    }

    #[test]
    fn test_display_message_assistant() {
        let caps = vec!["rust-expert".to_string(), "code-review".to_string()];
        let msg = DisplayMessage::assistant("Response text".to_string(), caps.clone());

        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.content, "Response text");
        assert!(!msg.is_streaming);
        assert_eq!(msg.active_caps, caps);
    }

    #[test]
    fn test_display_message_streaming() {
        let mut msg = DisplayMessage::assistant_streaming(vec!["rust-expert".to_string()]);
        assert!(msg.is_streaming);
        assert!(msg.content.is_empty());

        msg.append_content("Hello ");
        msg.append_content("world!");
        assert_eq!(msg.content, "Hello world!");

        msg.finish_streaming();
        assert!(!msg.is_streaming);
    }

    #[test]
    fn test_display_message_system() {
        let msg = DisplayMessage::system("System notification".to_string());
        assert_eq!(msg.role, MessageRole::System);
        assert_eq!(msg.content, "System notification");
        assert!(!msg.is_streaming);
        assert!(msg.active_caps.is_empty());
    }

    #[test]
    fn test_display_message_add_tool_call() {
        let mut msg = DisplayMessage::assistant_streaming(vec![]);
        assert!(msg.tool_calls.is_empty());

        let tc = DisplayToolCall::new(
            "tc1".to_string(),
            "file_read".to_string(),
            serde_json::json!({"path": "/test"}),
        );
        msg.add_tool_call(tc);

        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].id, "tc1");
    }

    #[test]
    fn test_display_message_find_tool_call() {
        let mut msg = DisplayMessage::assistant_streaming(vec![]);

        let tc1 = DisplayToolCall::new(
            "tc1".to_string(),
            "file_read".to_string(),
            serde_json::json!({}),
        );
        let tc2 = DisplayToolCall::new(
            "tc2".to_string(),
            "shell".to_string(),
            serde_json::json!({}),
        );
        msg.add_tool_call(tc1);
        msg.add_tool_call(tc2);

        let found = msg.find_tool_call("tc2");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "shell");

        let not_found = msg.find_tool_call("tc3");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_display_message_find_tool_call_mut() {
        let mut msg = DisplayMessage::assistant_streaming(vec![]);

        let tc = DisplayToolCall::new(
            "tc1".to_string(),
            "shell".to_string(),
            serde_json::json!({"command": "ls"}),
        );
        msg.add_tool_call(tc);

        // Find and modify
        if let Some(tc) = msg.find_tool_call_mut("tc1") {
            tc.complete_success(Some("Output".to_string()), None);
        }

        // Verify modification
        let tc = msg.find_tool_call("tc1").unwrap();
        assert_eq!(tc.status, ToolCallStatus::Success);
    }

    #[test]
    fn test_tool_call_summary() {
        let input = serde_json::json!({"path": "/src/main.rs"});
        let tc = DisplayToolCall::new("1".to_string(), "file_read".to_string(), input);
        assert_eq!(tc.input_summary, "/src/main.rs");
    }

    #[test]
    fn test_tool_call_lifecycle() {
        let input = serde_json::json!({"command": "cargo test"});
        let mut tc = DisplayToolCall::new("1".to_string(), "shell".to_string(), input);

        assert!(matches!(tc.status, ToolCallStatus::Running));

        tc.complete_success(Some("All tests passed".to_string()), None);
        assert!(matches!(tc.status, ToolCallStatus::Success));
        assert!(tc.completed_at.is_some());
    }
}
