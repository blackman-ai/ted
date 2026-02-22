// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Embedded mode for GUI integration
//!
//! When Ted runs with --embedded flag, it outputs JSONL events to stdout
//! instead of running the interactive TUI. This allows desktop apps and
//! IDE extensions to spawn Ted as a subprocess and receive structured events.

use serde::{Deserialize, Serialize};
use serde_json;
use std::io::{self, Write};

/// Base event structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseEvent<T> {
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: i64,
    pub session_id: String,
    pub data: T,
}

/// Plan event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanData {
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_files: Option<Vec<String>>,
}

/// File create event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCreateData {
    pub path: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
}

/// File edit event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEditData {
    pub path: String,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// File delete event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDeleteData {
    pub path: String,
}

/// Command event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandData {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::HashMap<String, String>>,
}

/// Command output event data (streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOutputData {
    pub stream: String, // "stdout" or "stderr"
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

/// Status event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusData {
    pub state: String, // thinking, reading, writing, running
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
}

/// Error event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorData {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// Completion event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionData {
    pub success: bool,
    pub summary: String,
    pub files_changed: Vec<String>,
}

/// Message event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageData {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<bool>,
}

/// Conversation history event data (for multi-turn persistence)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationHistoryData {
    pub messages: Vec<HistoryMessageData>,
}

/// Individual message in history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryMessageData {
    pub role: String,
    pub content: String,
}

/// JSONL event emitter
pub struct JsonLEmitter {
    session_id: String,
    target: EmitTarget,
}

enum EmitTarget {
    Stdout,
    #[cfg(test)]
    Buffer(std::sync::Arc<std::sync::Mutex<Vec<String>>>),
}

impl JsonLEmitter {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            target: EmitTarget::Stdout,
        }
    }

    #[cfg(test)]
    pub fn with_buffer(
        session_id: String,
        buffer: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ) -> Self {
        Self {
            session_id,
            target: EmitTarget::Buffer(buffer),
        }
    }

    fn emit<T: Serialize>(&self, event_type: &str, data: T) -> io::Result<()> {
        let event = BaseEvent {
            event_type: event_type.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            session_id: self.session_id.clone(),
            data,
        };

        let json = serde_json::to_string(&event)?;
        match &self.target {
            EmitTarget::Stdout => {
                println!("{}", json);
                io::stdout().flush()?;
            }
            #[cfg(test)]
            EmitTarget::Buffer(buffer) => {
                let mut guard = buffer
                    .lock()
                    .map_err(|_| io::Error::other("event buffer lock poisoned"))?;
                guard.push(json);
            }
        }
        Ok(())
    }

    pub fn emit_plan(&self, steps: Vec<PlanStep>) -> io::Result<()> {
        self.emit("plan", PlanData { steps })
    }

    pub fn emit_file_create(
        &self,
        path: String,
        content: String,
        mode: Option<u32>,
    ) -> io::Result<()> {
        self.emit(
            "file_create",
            FileCreateData {
                path,
                content,
                mode,
            },
        )
    }

    pub fn emit_file_edit(
        &self,
        path: String,
        operation: String,
        old_text: Option<String>,
        new_text: Option<String>,
        line: Option<usize>,
        text: Option<String>,
    ) -> io::Result<()> {
        self.emit(
            "file_edit",
            FileEditData {
                path,
                operation,
                old_text,
                new_text,
                line,
                text,
            },
        )
    }

    pub fn emit_file_delete(&self, path: String) -> io::Result<()> {
        self.emit("file_delete", FileDeleteData { path })
    }

    pub fn emit_command(
        &self,
        command: String,
        cwd: Option<String>,
        env: Option<std::collections::HashMap<String, String>>,
    ) -> io::Result<()> {
        self.emit("command", CommandData { command, cwd, env })
    }

    pub fn emit_command_output(
        &self,
        stream: &str,
        text: String,
        done: Option<bool>,
        exit_code: Option<i32>,
    ) -> io::Result<()> {
        self.emit(
            "command_output",
            CommandOutputData {
                stream: stream.to_string(),
                text,
                done,
                exit_code,
            },
        )
    }

    pub fn emit_status(
        &self,
        state: &str,
        message: String,
        progress: Option<u8>,
    ) -> io::Result<()> {
        self.emit(
            "status",
            StatusData {
                state: state.to_string(),
                message,
                progress,
            },
        )
    }

    pub fn emit_error(
        &self,
        code: String,
        message: String,
        suggested_fix: Option<String>,
        context: Option<serde_json::Value>,
    ) -> io::Result<()> {
        self.emit(
            "error",
            ErrorData {
                code,
                message,
                suggested_fix,
                context,
            },
        )
    }

    pub fn emit_completion(
        &self,
        success: bool,
        summary: String,
        files_changed: Vec<String>,
    ) -> io::Result<()> {
        self.emit(
            "completion",
            CompletionData {
                success,
                summary,
                files_changed,
            },
        )
    }

    pub fn emit_message(&self, role: &str, content: String, delta: Option<bool>) -> io::Result<()> {
        self.emit(
            "message",
            MessageData {
                role: role.to_string(),
                content,
                delta,
            },
        )
    }

    pub fn emit_conversation_history(&self, messages: Vec<HistoryMessageData>) -> io::Result<()> {
        self.emit("conversation_history", ConversationHistoryData { messages })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== BaseEvent tests =====

    #[test]
    fn test_base_event_creation() {
        let event = BaseEvent {
            event_type: "test".to_string(),
            timestamp: 1234567890,
            session_id: "session-123".to_string(),
            data: "test data",
        };

        assert_eq!(event.event_type, "test");
        assert_eq!(event.timestamp, 1234567890);
        assert_eq!(event.session_id, "session-123");
    }

    #[test]
    fn test_base_event_serialization() {
        let event = BaseEvent {
            event_type: "test".to_string(),
            timestamp: 1234567890,
            session_id: "session-123".to_string(),
            data: serde_json::json!({"key": "value"}),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"test\""));
        assert!(json.contains("\"session_id\":\"session-123\""));
    }

    // ===== PlanStep tests =====

    #[test]
    fn test_plan_step_creation() {
        let step = PlanStep {
            id: "step-1".to_string(),
            description: "Do something".to_string(),
            estimated_files: Some(vec!["file1.rs".to_string()]),
        };

        assert_eq!(step.id, "step-1");
        assert_eq!(step.description, "Do something");
        assert!(step.estimated_files.is_some());
    }

    #[test]
    fn test_plan_step_serialization() {
        let step = PlanStep {
            id: "step-1".to_string(),
            description: "Do something".to_string(),
            estimated_files: None,
        };

        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("\"id\":\"step-1\""));
        // estimated_files should be skipped when None
        assert!(!json.contains("estimated_files"));
    }

    #[test]
    fn test_plan_data_creation() {
        let data = PlanData {
            steps: vec![
                PlanStep {
                    id: "1".to_string(),
                    description: "First".to_string(),
                    estimated_files: None,
                },
                PlanStep {
                    id: "2".to_string(),
                    description: "Second".to_string(),
                    estimated_files: None,
                },
            ],
        };

        assert_eq!(data.steps.len(), 2);
    }

    // ===== FileCreateData tests =====

    #[test]
    fn test_file_create_data() {
        let data = FileCreateData {
            path: "/test/file.rs".to_string(),
            content: "fn main() {}".to_string(),
            mode: Some(0o644),
        };

        assert_eq!(data.path, "/test/file.rs");
        assert_eq!(data.content, "fn main() {}");
        assert_eq!(data.mode, Some(0o644));
    }

    #[test]
    fn test_file_create_data_serialization() {
        let data = FileCreateData {
            path: "/test.txt".to_string(),
            content: "hello".to_string(),
            mode: None,
        };

        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"path\":\"/test.txt\""));
        // mode should be skipped when None
        assert!(!json.contains("mode"));
    }

    // ===== FileEditData tests =====

    #[test]
    fn test_file_edit_data() {
        let data = FileEditData {
            path: "/test.rs".to_string(),
            operation: "replace".to_string(),
            old_text: Some("old".to_string()),
            new_text: Some("new".to_string()),
            line: None,
            text: None,
        };

        assert_eq!(data.path, "/test.rs");
        assert_eq!(data.operation, "replace");
        assert_eq!(data.old_text, Some("old".to_string()));
    }

    // ===== FileDeleteData tests =====

    #[test]
    fn test_file_delete_data() {
        let data = FileDeleteData {
            path: "/to/delete.txt".to_string(),
        };

        assert_eq!(data.path, "/to/delete.txt");
    }

    // ===== CommandData tests =====

    #[test]
    fn test_command_data() {
        let mut env = std::collections::HashMap::new();
        env.insert("VAR".to_string(), "value".to_string());

        let data = CommandData {
            command: "cargo build".to_string(),
            cwd: Some("/project".to_string()),
            env: Some(env),
        };

        assert_eq!(data.command, "cargo build");
        assert_eq!(data.cwd, Some("/project".to_string()));
        assert!(data.env.is_some());
    }

    // ===== CommandOutputData tests =====

    #[test]
    fn test_command_output_data() {
        let data = CommandOutputData {
            stream: "stdout".to_string(),
            text: "Hello, world!".to_string(),
            done: Some(true),
            exit_code: Some(0),
        };

        assert_eq!(data.stream, "stdout");
        assert_eq!(data.text, "Hello, world!");
        assert_eq!(data.done, Some(true));
        assert_eq!(data.exit_code, Some(0));
    }

    // ===== StatusData tests =====

    #[test]
    fn test_status_data() {
        let data = StatusData {
            state: "thinking".to_string(),
            message: "Processing request".to_string(),
            progress: Some(50),
        };

        assert_eq!(data.state, "thinking");
        assert_eq!(data.message, "Processing request");
        assert_eq!(data.progress, Some(50));
    }

    // ===== ErrorData tests =====

    #[test]
    fn test_error_data() {
        let data = ErrorData {
            code: "FILE_NOT_FOUND".to_string(),
            message: "The file does not exist".to_string(),
            suggested_fix: Some("Create the file first".to_string()),
            context: Some(serde_json::json!({"path": "/missing.txt"})),
        };

        assert_eq!(data.code, "FILE_NOT_FOUND");
        assert_eq!(data.message, "The file does not exist");
        assert!(data.suggested_fix.is_some());
    }

    // ===== CompletionData tests =====

    #[test]
    fn test_completion_data() {
        let data = CompletionData {
            success: true,
            summary: "Task completed".to_string(),
            files_changed: vec!["file1.rs".to_string(), "file2.rs".to_string()],
        };

        assert!(data.success);
        assert_eq!(data.summary, "Task completed");
        assert_eq!(data.files_changed.len(), 2);
    }

    // ===== MessageData tests =====

    #[test]
    fn test_message_data() {
        let data = MessageData {
            role: "assistant".to_string(),
            content: "Hello!".to_string(),
            delta: Some(true),
        };

        assert_eq!(data.role, "assistant");
        assert_eq!(data.content, "Hello!");
        assert_eq!(data.delta, Some(true));
    }

    // ===== HistoryMessageData tests =====

    #[test]
    fn test_history_message_data() {
        let data = HistoryMessageData {
            role: "user".to_string(),
            content: "What is Rust?".to_string(),
        };

        assert_eq!(data.role, "user");
        assert_eq!(data.content, "What is Rust?");
    }

    // ===== ConversationHistoryData tests =====

    #[test]
    fn test_conversation_history_data() {
        let data = ConversationHistoryData {
            messages: vec![
                HistoryMessageData {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                },
                HistoryMessageData {
                    role: "assistant".to_string(),
                    content: "Hi there!".to_string(),
                },
            ],
        };

        assert_eq!(data.messages.len(), 2);
    }

    // ===== JsonLEmitter tests =====

    #[test]
    fn test_jsonl_emitter_creation() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        assert_eq!(emitter.session_id, "test-session");
    }

    // ===== Clone trait tests =====

    #[test]
    fn test_plan_step_clone() {
        let step = PlanStep {
            id: "step-1".to_string(),
            description: "Do something".to_string(),
            estimated_files: Some(vec!["file.rs".to_string()]),
        };
        let cloned = step.clone();

        assert_eq!(cloned.id, step.id);
        assert_eq!(cloned.description, step.description);
        assert_eq!(cloned.estimated_files, step.estimated_files);
    }

    #[test]
    fn test_file_create_data_clone() {
        let data = FileCreateData {
            path: "/test.rs".to_string(),
            content: "content".to_string(),
            mode: Some(0o755),
        };
        let cloned = data.clone();

        assert_eq!(cloned.path, data.path);
        assert_eq!(cloned.content, data.content);
        assert_eq!(cloned.mode, data.mode);
    }

    #[test]
    fn test_file_edit_data_clone() {
        let data = FileEditData {
            path: "/test.rs".to_string(),
            operation: "replace".to_string(),
            old_text: Some("old".to_string()),
            new_text: Some("new".to_string()),
            line: Some(10),
            text: Some("line text".to_string()),
        };
        let cloned = data.clone();

        assert_eq!(cloned.path, data.path);
        assert_eq!(cloned.operation, data.operation);
        assert_eq!(cloned.old_text, data.old_text);
        assert_eq!(cloned.new_text, data.new_text);
        assert_eq!(cloned.line, data.line);
        assert_eq!(cloned.text, data.text);
    }

    #[test]
    fn test_status_data_clone() {
        let data = StatusData {
            state: "thinking".to_string(),
            message: "Working...".to_string(),
            progress: Some(75),
        };
        let cloned = data.clone();

        assert_eq!(cloned.state, data.state);
        assert_eq!(cloned.message, data.message);
        assert_eq!(cloned.progress, data.progress);
    }

    #[test]
    fn test_error_data_clone() {
        let data = ErrorData {
            code: "ERR_001".to_string(),
            message: "Error occurred".to_string(),
            suggested_fix: Some("Fix it".to_string()),
            context: Some(serde_json::json!({"key": "value"})),
        };
        let cloned = data.clone();

        assert_eq!(cloned.code, data.code);
        assert_eq!(cloned.message, data.message);
        assert_eq!(cloned.suggested_fix, data.suggested_fix);
        assert_eq!(cloned.context, data.context);
    }

    #[test]
    fn test_message_data_clone() {
        let data = MessageData {
            role: "user".to_string(),
            content: "Hello".to_string(),
            delta: Some(false),
        };
        let cloned = data.clone();

        assert_eq!(cloned.role, data.role);
        assert_eq!(cloned.content, data.content);
        assert_eq!(cloned.delta, data.delta);
    }

    // ===== Debug trait tests =====

    #[test]
    fn test_plan_step_debug() {
        let step = PlanStep {
            id: "step-1".to_string(),
            description: "Test".to_string(),
            estimated_files: None,
        };
        let debug = format!("{:?}", step);
        assert!(debug.contains("PlanStep"));
        assert!(debug.contains("step-1"));
    }

    #[test]
    fn test_base_event_debug() {
        let event = BaseEvent {
            event_type: "test".to_string(),
            timestamp: 0,
            session_id: "session".to_string(),
            data: "data",
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("BaseEvent"));
    }

    #[test]
    fn test_file_delete_data_debug() {
        let data = FileDeleteData {
            path: "/delete/me.txt".to_string(),
        };
        let debug = format!("{:?}", data);
        assert!(debug.contains("FileDeleteData"));
        assert!(debug.contains("/delete/me.txt"));
    }

    #[test]
    fn test_command_data_debug() {
        let data = CommandData {
            command: "ls".to_string(),
            cwd: None,
            env: None,
        };
        let debug = format!("{:?}", data);
        assert!(debug.contains("CommandData"));
        assert!(debug.contains("ls"));
    }

    // ===== Deserialization tests =====

    #[test]
    fn test_plan_step_deserialize() {
        let json = r#"{"id":"step-1","description":"Do something"}"#;
        let step: PlanStep = serde_json::from_str(json).unwrap();
        assert_eq!(step.id, "step-1");
        assert_eq!(step.description, "Do something");
        assert!(step.estimated_files.is_none());
    }

    #[test]
    fn test_plan_step_deserialize_with_files() {
        let json =
            r#"{"id":"step-2","description":"With files","estimated_files":["a.rs","b.rs"]}"#;
        let step: PlanStep = serde_json::from_str(json).unwrap();
        assert_eq!(
            step.estimated_files,
            Some(vec!["a.rs".to_string(), "b.rs".to_string()])
        );
    }

    #[test]
    fn test_file_create_data_deserialize() {
        let json = r#"{"path":"/test.rs","content":"fn main() {}"}"#;
        let data: FileCreateData = serde_json::from_str(json).unwrap();
        assert_eq!(data.path, "/test.rs");
        assert_eq!(data.content, "fn main() {}");
        assert!(data.mode.is_none());
    }

    #[test]
    fn test_file_edit_data_deserialize() {
        let json = r#"{"path":"/edit.rs","operation":"replace","old_text":"old","new_text":"new"}"#;
        let data: FileEditData = serde_json::from_str(json).unwrap();
        assert_eq!(data.path, "/edit.rs");
        assert_eq!(data.operation, "replace");
        assert_eq!(data.old_text, Some("old".to_string()));
        assert_eq!(data.new_text, Some("new".to_string()));
    }

    #[test]
    fn test_command_output_data_deserialize() {
        let json = r#"{"stream":"stdout","text":"output","done":true,"exit_code":0}"#;
        let data: CommandOutputData = serde_json::from_str(json).unwrap();
        assert_eq!(data.stream, "stdout");
        assert_eq!(data.text, "output");
        assert_eq!(data.done, Some(true));
        assert_eq!(data.exit_code, Some(0));
    }

    #[test]
    fn test_status_data_deserialize() {
        let json = r#"{"state":"reading","message":"Reading files","progress":50}"#;
        let data: StatusData = serde_json::from_str(json).unwrap();
        assert_eq!(data.state, "reading");
        assert_eq!(data.message, "Reading files");
        assert_eq!(data.progress, Some(50));
    }

    #[test]
    fn test_error_data_deserialize() {
        let json = r#"{"code":"ERR","message":"Error message"}"#;
        let data: ErrorData = serde_json::from_str(json).unwrap();
        assert_eq!(data.code, "ERR");
        assert_eq!(data.message, "Error message");
        assert!(data.suggested_fix.is_none());
        assert!(data.context.is_none());
    }

    #[test]
    fn test_completion_data_deserialize() {
        let json = r#"{"success":true,"summary":"Done","files_changed":["a.rs"]}"#;
        let data: CompletionData = serde_json::from_str(json).unwrap();
        assert!(data.success);
        assert_eq!(data.summary, "Done");
        assert_eq!(data.files_changed, vec!["a.rs"]);
    }

    #[test]
    fn test_message_data_deserialize() {
        let json = r#"{"role":"assistant","content":"Hello"}"#;
        let data: MessageData = serde_json::from_str(json).unwrap();
        assert_eq!(data.role, "assistant");
        assert_eq!(data.content, "Hello");
        assert!(data.delta.is_none());
    }

    #[test]
    fn test_base_event_deserialize() {
        let json = r#"{"type":"test","timestamp":12345,"session_id":"sess-1","data":"payload"}"#;
        let event: BaseEvent<String> = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "test");
        assert_eq!(event.timestamp, 12345);
        assert_eq!(event.session_id, "sess-1");
        assert_eq!(event.data, "payload");
    }

    // ===== Roundtrip serialization tests =====

    #[test]
    fn test_plan_data_roundtrip() {
        let original = PlanData {
            steps: vec![PlanStep {
                id: "1".to_string(),
                description: "First".to_string(),
                estimated_files: Some(vec!["a.rs".to_string()]),
            }],
        };

        let json = serde_json::to_string(&original).unwrap();
        let restored: PlanData = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.steps.len(), original.steps.len());
        assert_eq!(restored.steps[0].id, original.steps[0].id);
    }

    #[test]
    fn test_conversation_history_roundtrip() {
        let original = ConversationHistoryData {
            messages: vec![
                HistoryMessageData {
                    role: "user".to_string(),
                    content: "Question".to_string(),
                },
                HistoryMessageData {
                    role: "assistant".to_string(),
                    content: "Answer".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&original).unwrap();
        let restored: ConversationHistoryData = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.messages.len(), 2);
        assert_eq!(restored.messages[0].role, "user");
        assert_eq!(restored.messages[1].role, "assistant");
    }

    // ===== Edge cases =====

    #[test]
    fn test_empty_strings() {
        let data = FileCreateData {
            path: "".to_string(),
            content: "".to_string(),
            mode: None,
        };

        let json = serde_json::to_string(&data).unwrap();
        let restored: FileCreateData = serde_json::from_str(&json).unwrap();

        assert!(restored.path.is_empty());
        assert!(restored.content.is_empty());
    }

    #[test]
    fn test_unicode_content() {
        let data = MessageData {
            role: "user".to_string(),
            content: "日本語 🚀 émojis".to_string(),
            delta: None,
        };

        let json = serde_json::to_string(&data).unwrap();
        let restored: MessageData = serde_json::from_str(&json).unwrap();

        assert!(restored.content.contains("日本語"));
        assert!(restored.content.contains("🚀"));
    }

    #[test]
    fn test_special_characters() {
        let data = CommandData {
            command: "echo 'hello \"world\"' | grep \"test\"".to_string(),
            cwd: Some("/path/with spaces/and'quotes".to_string()),
            env: None,
        };

        let json = serde_json::to_string(&data).unwrap();
        let restored: CommandData = serde_json::from_str(&json).unwrap();

        assert!(restored.command.contains("'hello"));
        assert!(restored.cwd.unwrap().contains("spaces"));
    }

    #[test]
    fn test_large_content() {
        let large_content = "x".repeat(100_000);
        let data = FileCreateData {
            path: "/large.txt".to_string(),
            content: large_content.clone(),
            mode: None,
        };

        let json = serde_json::to_string(&data).unwrap();
        let restored: FileCreateData = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.content.len(), 100_000);
    }

    #[test]
    fn test_completion_data_empty_files() {
        let data = CompletionData {
            success: false,
            summary: "Failed".to_string(),
            files_changed: vec![],
        };

        assert!(!data.success);
        assert!(data.files_changed.is_empty());
    }

    #[test]
    fn test_error_data_full() {
        let data = ErrorData {
            code: "NETWORK_ERROR".to_string(),
            message: "Connection failed".to_string(),
            suggested_fix: Some("Check your network".to_string()),
            context: Some(serde_json::json!({
                "url": "https://api.example.com",
                "status": 503,
                "retry_count": 3
            })),
        };

        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("NETWORK_ERROR"));
        assert!(json.contains("suggested_fix"));
        assert!(json.contains("context"));
    }

    // ===== JsonLEmitter method tests =====
    // These test the emit methods. They print to stdout, so we verify they don't panic
    // and return Ok results.

    #[test]
    fn test_jsonl_emitter_emit_plan() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let steps = vec![
            PlanStep {
                id: "step-1".to_string(),
                description: "First step".to_string(),
                estimated_files: Some(vec!["file.rs".to_string()]),
            },
            PlanStep {
                id: "step-2".to_string(),
                description: "Second step".to_string(),
                estimated_files: None,
            },
        ];

        let result = emitter.emit_plan(steps);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_plan_empty() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_plan(vec![]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_file_create() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_file_create(
            "/test/path.rs".to_string(),
            "fn main() {}".to_string(),
            Some(0o644),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_file_create_no_mode() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result =
            emitter.emit_file_create("/test/path.rs".to_string(), "content".to_string(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_file_edit() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_file_edit(
            "/test/path.rs".to_string(),
            "replace".to_string(),
            Some("old text".to_string()),
            Some("new text".to_string()),
            None,
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_file_edit_insert() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_file_edit(
            "/test/path.rs".to_string(),
            "insert".to_string(),
            None,
            None,
            Some(10),
            Some("inserted line".to_string()),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_file_delete() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_file_delete("/test/to-delete.rs".to_string());
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_command() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_command(
            "cargo build".to_string(),
            Some("/project".to_string()),
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_command_with_env() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let mut env = std::collections::HashMap::new();
        env.insert("RUST_BACKTRACE".to_string(), "1".to_string());
        env.insert("DEBUG".to_string(), "true".to_string());

        let result = emitter.emit_command(
            "cargo test".to_string(),
            Some("/project".to_string()),
            Some(env),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_command_minimal() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_command("ls".to_string(), None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_command_output_stdout() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result =
            emitter.emit_command_output("stdout", "Hello, world!\n".to_string(), None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_command_output_stderr() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_command_output(
            "stderr",
            "Warning: something happened\n".to_string(),
            None,
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_command_output_done() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_command_output("stdout", "".to_string(), Some(true), Some(0));
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_command_output_error_exit() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_command_output(
            "stderr",
            "Error: command failed".to_string(),
            Some(true),
            Some(1),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_status_thinking() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_status("thinking", "Processing request...".to_string(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_status_reading() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_status("reading", "Reading file.rs".to_string(), Some(25));
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_status_writing() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_status("writing", "Writing changes".to_string(), Some(50));
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_status_running() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_status("running", "Executing command".to_string(), Some(75));
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_status_complete() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_status("complete", "Done".to_string(), Some(100));
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_error_simple() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_error(
            "FILE_NOT_FOUND".to_string(),
            "The file does not exist".to_string(),
            None,
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_error_with_fix() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_error(
            "PERMISSION_DENIED".to_string(),
            "Cannot write to file".to_string(),
            Some("Check file permissions or run with elevated privileges".to_string()),
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_error_with_context() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let context = serde_json::json!({
            "path": "/protected/file.txt",
            "user": "test",
            "mode": 0o400
        });

        let result = emitter.emit_error(
            "PERMISSION_DENIED".to_string(),
            "Cannot write to file".to_string(),
            Some("Change file permissions".to_string()),
            Some(context),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_completion_success() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_completion(
            true,
            "Successfully completed the task".to_string(),
            vec!["file1.rs".to_string(), "file2.rs".to_string()],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_completion_failure() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_completion(
            false,
            "Task failed due to compilation errors".to_string(),
            vec![],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_completion_no_files() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result =
            emitter.emit_completion(true, "Completed without file changes".to_string(), vec![]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_message_user() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_message("user", "Hello, please help me".to_string(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_message_assistant() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result =
            emitter.emit_message("assistant", "I'll help you with that.".to_string(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_message_delta() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_message("assistant", "Streaming ".to_string(), Some(true));
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_message_delta_complete() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_message("assistant", "done.".to_string(), Some(false));
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_conversation_history() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let messages = vec![
            HistoryMessageData {
                role: "user".to_string(),
                content: "What is Rust?".to_string(),
            },
            HistoryMessageData {
                role: "assistant".to_string(),
                content: "Rust is a systems programming language.".to_string(),
            },
            HistoryMessageData {
                role: "user".to_string(),
                content: "Thanks!".to_string(),
            },
        ];

        let result = emitter.emit_conversation_history(messages);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_emit_conversation_history_empty() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_conversation_history(vec![]);
        assert!(result.is_ok());
    }

    // ===== Additional edge case tests =====

    #[test]
    fn test_jsonl_emitter_unicode_session_id() {
        let emitter = JsonLEmitter::new("セッション-日本語-🦀".to_string());
        let result = emitter.emit_status("thinking", "Processing".to_string(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_empty_session_id() {
        let emitter = JsonLEmitter::new("".to_string());
        let result = emitter.emit_status("thinking", "Processing".to_string(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_long_content() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let long_content = "x".repeat(100_000);
        let result = emitter.emit_message("assistant", long_content, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_special_chars_in_content() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let content = "Special chars: \n\t\r\"\\/'<>&";
        let result = emitter.emit_message("user", content.to_string(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_file_path_with_spaces() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_file_create(
            "/path/with spaces/and special (chars)/file.rs".to_string(),
            "content".to_string(),
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_many_files_changed() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let files: Vec<String> = (0..1000).map(|i| format!("file_{}.rs", i)).collect();
        let result = emitter.emit_completion(true, "Many files changed".to_string(), files);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_many_plan_steps() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let steps: Vec<PlanStep> = (0..100)
            .map(|i| PlanStep {
                id: format!("step-{}", i),
                description: format!("Step {} description", i),
                estimated_files: Some(vec![format!("file_{}.rs", i)]),
            })
            .collect();
        let result = emitter.emit_plan(steps);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jsonl_emitter_error_nested_context() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let context = serde_json::json!({
            "level1": {
                "level2": {
                    "level3": {
                        "deep_value": 42
                    }
                }
            },
            "array": [1, 2, 3, {"nested": true}]
        });

        let result = emitter.emit_error(
            "COMPLEX_ERROR".to_string(),
            "Complex context".to_string(),
            None,
            Some(context),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_file_edit_all_options() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_file_edit(
            "/test/path.rs".to_string(),
            "complex".to_string(),
            Some("old".to_string()),
            Some("new".to_string()),
            Some(42),
            Some("line text".to_string()),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_command_empty_env() {
        let emitter = JsonLEmitter::new("test-session".to_string());
        let result = emitter.emit_command(
            "echo hello".to_string(),
            None,
            Some(std::collections::HashMap::new()),
        );
        assert!(result.is_ok());
    }
}
