// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Embedded mode for GUI integration
//!
//! When Ted runs with --embedded flag, it outputs JSONL events to stdout
//! instead of running the interactive TUI. This allows desktop apps like
//! Teddy to spawn Ted as a subprocess and receive structured events.

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
}

impl JsonLEmitter {
    pub fn new(session_id: String) -> Self {
        Self { session_id }
    }

    fn emit<T: Serialize>(&self, event_type: &str, data: T) -> io::Result<()> {
        let event = BaseEvent {
            event_type: event_type.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            session_id: self.session_id.clone(),
            data,
        };

        let json = serde_json::to_string(&event)?;
        println!("{}", json);
        io::stdout().flush()?;
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
