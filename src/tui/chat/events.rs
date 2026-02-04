// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Event system for the chat TUI
//!
//! Events allow async operations (LLM responses, agent updates) to communicate
//! with the UI without blocking. Uses tokio mpsc channels for thread-safe messaging.

use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::tools::ToolResult;

/// Events for async communication between LLM/agent threads and UI
#[derive(Debug, Clone)]
pub enum ChatEvent {
    // === User input ===
    /// User submitted a message
    UserMessage(String),

    // === LLM responses ===
    /// LLM started generating a response
    StreamStart,
    /// Received a chunk of streaming text
    StreamDelta(String),
    /// LLM finished generating response
    StreamEnd,

    // === Tool calls ===
    /// Tool execution started
    ToolCallStart {
        id: String,
        name: String,
        input: Value,
    },
    /// Tool execution completed
    ToolCallEnd {
        id: String,
        name: String,
        result: ToolResult,
    },

    // === Agent lifecycle ===
    /// An agent was spawned
    AgentSpawned {
        id: Uuid,
        name: String,
        agent_type: String,
        task: String,
    },
    /// Agent made progress (completed an iteration)
    AgentProgress {
        id: Uuid,
        iteration: u32,
        max_iterations: u32,
        action: String,
    },
    /// Agent is waiting for rate limit budget
    AgentRateLimited {
        id: Uuid,
        wait_secs: f64,
        tokens_needed: u64,
    },
    /// Agent started a tool call
    AgentToolStart { id: Uuid, tool_name: String },
    /// Agent completed a tool call
    AgentToolEnd {
        id: Uuid,
        tool_name: String,
        success: bool,
    },
    /// Agent completed successfully
    AgentCompleted {
        id: Uuid,
        files_changed: Vec<String>,
        summary: Option<String>,
    },
    /// Agent failed with an error
    AgentFailed { id: Uuid, error: String },
    /// Agent was cancelled by user
    AgentCancelled { id: Uuid },

    // === System events ===
    /// An error occurred
    Error(String),
    /// Status message to display
    Status(String),
    /// Session ended (user typed exit)
    SessionEnded,
    /// Request to refresh the UI
    Refresh,
}

/// Type alias for the event sender
pub type EventSender = mpsc::UnboundedSender<ChatEvent>;

/// Type alias for the event receiver
pub type EventReceiver = mpsc::UnboundedReceiver<ChatEvent>;

/// Create a new event channel
pub fn create_event_channel() -> (EventSender, EventReceiver) {
    mpsc::unbounded_channel()
}

/// Helper for sending events, ignoring errors if receiver is dropped
pub fn send_event(tx: &EventSender, event: ChatEvent) {
    let _ = tx.send(event);
}

/// Wrapper that can be cloned and passed to async tasks
#[derive(Clone)]
pub struct EventEmitter {
    tx: EventSender,
}

impl EventEmitter {
    pub fn new(tx: EventSender) -> Self {
        Self { tx }
    }

    pub fn emit(&self, event: ChatEvent) {
        send_event(&self.tx, event);
    }

    pub fn stream_start(&self) {
        self.emit(ChatEvent::StreamStart);
    }

    pub fn stream_delta(&self, text: &str) {
        self.emit(ChatEvent::StreamDelta(text.to_string()));
    }

    pub fn stream_end(&self) {
        self.emit(ChatEvent::StreamEnd);
    }

    pub fn tool_start(&self, id: &str, name: &str, input: Value) {
        self.emit(ChatEvent::ToolCallStart {
            id: id.to_string(),
            name: name.to_string(),
            input,
        });
    }

    pub fn tool_end(&self, id: &str, name: &str, result: ToolResult) {
        self.emit(ChatEvent::ToolCallEnd {
            id: id.to_string(),
            name: name.to_string(),
            result,
        });
    }

    pub fn agent_spawned(&self, id: Uuid, name: &str, agent_type: &str, task: &str) {
        self.emit(ChatEvent::AgentSpawned {
            id,
            name: name.to_string(),
            agent_type: agent_type.to_string(),
            task: task.to_string(),
        });
    }

    pub fn agent_progress(&self, id: Uuid, iteration: u32, max_iterations: u32, action: &str) {
        self.emit(ChatEvent::AgentProgress {
            id,
            iteration,
            max_iterations,
            action: action.to_string(),
        });
    }

    pub fn agent_completed(&self, id: Uuid, files_changed: Vec<String>, summary: Option<String>) {
        self.emit(ChatEvent::AgentCompleted {
            id,
            files_changed,
            summary,
        });
    }

    pub fn agent_failed(&self, id: Uuid, error: &str) {
        self.emit(ChatEvent::AgentFailed {
            id,
            error: error.to_string(),
        });
    }

    pub fn error(&self, msg: &str) {
        self.emit(ChatEvent::Error(msg.to_string()));
    }

    pub fn status(&self, msg: &str) {
        self.emit(ChatEvent::Status(msg.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_event_channel() {
        let (tx, _rx) = create_event_channel();
        assert!(tx.send(ChatEvent::Refresh).is_ok());
    }

    #[test]
    fn test_event_emitter() {
        let (tx, mut rx) = create_event_channel();
        let emitter = EventEmitter::new(tx);

        emitter.stream_start();
        emitter.stream_delta("Hello");
        emitter.stream_end();

        // Verify events were sent
        assert!(matches!(rx.try_recv(), Ok(ChatEvent::StreamStart)));
        assert!(matches!(rx.try_recv(), Ok(ChatEvent::StreamDelta(_))));
        assert!(matches!(rx.try_recv(), Ok(ChatEvent::StreamEnd)));
    }

    #[test]
    fn test_send_event_ignores_closed_receiver() {
        let (tx, rx) = create_event_channel();
        drop(rx); // Close receiver

        // Should not panic
        send_event(&tx, ChatEvent::Refresh);
    }

    #[test]
    fn test_event_clone() {
        let event = ChatEvent::AgentProgress {
            id: Uuid::new_v4(),
            iteration: 5,
            max_iterations: 30,
            action: "Reading file".to_string(),
        };

        let cloned = event.clone();
        assert!(matches!(
            cloned,
            ChatEvent::AgentProgress { iteration: 5, .. }
        ));
    }

    #[test]
    fn test_event_emitter_tool_start() {
        let (tx, mut rx) = create_event_channel();
        let emitter = EventEmitter::new(tx);

        emitter.tool_start("tc1", "file_read", serde_json::json!({"path": "/test"}));

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::ToolCallStart { id, name, input } => {
                assert_eq!(id, "tc1");
                assert_eq!(name, "file_read");
                assert_eq!(input["path"], "/test");
            }
            _ => panic!("Expected ToolCallStart event"),
        }
    }

    #[test]
    fn test_event_emitter_tool_end() {
        let (tx, mut rx) = create_event_channel();
        let emitter = EventEmitter::new(tx);

        let result = ToolResult::success("tc1", "output");
        emitter.tool_end("tc1", "file_read", result);

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::ToolCallEnd { id, name, result } => {
                assert_eq!(id, "tc1");
                assert_eq!(name, "file_read");
                assert!(!result.is_error());
            }
            _ => panic!("Expected ToolCallEnd event"),
        }
    }

    #[test]
    fn test_event_emitter_agent_spawned() {
        let (tx, mut rx) = create_event_channel();
        let emitter = EventEmitter::new(tx);

        let id = Uuid::new_v4();
        emitter.agent_spawned(id, "research-agent", "explore", "Find API endpoints");

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::AgentSpawned {
                id: event_id,
                name,
                agent_type,
                task,
            } => {
                assert_eq!(event_id, id);
                assert_eq!(name, "research-agent");
                assert_eq!(agent_type, "explore");
                assert_eq!(task, "Find API endpoints");
            }
            _ => panic!("Expected AgentSpawned event"),
        }
    }

    #[test]
    fn test_event_emitter_agent_progress() {
        let (tx, mut rx) = create_event_channel();
        let emitter = EventEmitter::new(tx);

        let id = Uuid::new_v4();
        emitter.agent_progress(id, 5, 30, "Reading files");

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::AgentProgress {
                id: event_id,
                iteration,
                max_iterations,
                action,
            } => {
                assert_eq!(event_id, id);
                assert_eq!(iteration, 5);
                assert_eq!(max_iterations, 30);
                assert_eq!(action, "Reading files");
            }
            _ => panic!("Expected AgentProgress event"),
        }
    }

    #[test]
    fn test_event_emitter_agent_completed() {
        let (tx, mut rx) = create_event_channel();
        let emitter = EventEmitter::new(tx);

        let id = Uuid::new_v4();
        let files = vec!["file1.rs".to_string(), "file2.rs".to_string()];
        emitter.agent_completed(id, files.clone(), Some("Task completed".to_string()));

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::AgentCompleted {
                id: event_id,
                files_changed,
                summary,
            } => {
                assert_eq!(event_id, id);
                assert_eq!(files_changed, files);
                assert_eq!(summary, Some("Task completed".to_string()));
            }
            _ => panic!("Expected AgentCompleted event"),
        }
    }

    #[test]
    fn test_event_emitter_agent_failed() {
        let (tx, mut rx) = create_event_channel();
        let emitter = EventEmitter::new(tx);

        let id = Uuid::new_v4();
        emitter.agent_failed(id, "Something went wrong");

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::AgentFailed {
                id: event_id,
                error,
            } => {
                assert_eq!(event_id, id);
                assert_eq!(error, "Something went wrong");
            }
            _ => panic!("Expected AgentFailed event"),
        }
    }

    #[test]
    fn test_event_emitter_error() {
        let (tx, mut rx) = create_event_channel();
        let emitter = EventEmitter::new(tx);

        emitter.error("An error occurred");

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::Error(msg) => {
                assert_eq!(msg, "An error occurred");
            }
            _ => panic!("Expected Error event"),
        }
    }

    #[test]
    fn test_event_emitter_status() {
        let (tx, mut rx) = create_event_channel();
        let emitter = EventEmitter::new(tx);

        emitter.status("Processing...");

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::Status(msg) => {
                assert_eq!(msg, "Processing...");
            }
            _ => panic!("Expected Status event"),
        }
    }

    #[test]
    fn test_event_emitter_emit() {
        let (tx, mut rx) = create_event_channel();
        let emitter = EventEmitter::new(tx);

        emitter.emit(ChatEvent::Refresh);

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, ChatEvent::Refresh));
    }

    #[test]
    fn test_event_emitter_clone() {
        let (tx, mut rx) = create_event_channel();
        let emitter = EventEmitter::new(tx);
        let emitter2 = emitter.clone();

        emitter.stream_start();
        emitter2.stream_end();

        assert!(matches!(rx.try_recv(), Ok(ChatEvent::StreamStart)));
        assert!(matches!(rx.try_recv(), Ok(ChatEvent::StreamEnd)));
    }

    #[test]
    fn test_chat_event_debug() {
        let event = ChatEvent::UserMessage("Hello".to_string());
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("UserMessage"));
        assert!(debug_str.contains("Hello"));
    }

    #[test]
    fn test_all_chat_event_variants_debug() {
        let events: Vec<ChatEvent> = vec![
            ChatEvent::UserMessage("test".to_string()),
            ChatEvent::StreamStart,
            ChatEvent::StreamDelta("chunk".to_string()),
            ChatEvent::StreamEnd,
            ChatEvent::ToolCallStart {
                id: "tc1".to_string(),
                name: "tool".to_string(),
                input: serde_json::json!({}),
            },
            ChatEvent::ToolCallEnd {
                id: "tc1".to_string(),
                name: "tool".to_string(),
                result: ToolResult::success("tc", "ok"),
            },
            ChatEvent::AgentSpawned {
                id: Uuid::new_v4(),
                name: "agent".to_string(),
                agent_type: "explore".to_string(),
                task: "task".to_string(),
            },
            ChatEvent::AgentProgress {
                id: Uuid::new_v4(),
                iteration: 1,
                max_iterations: 10,
                action: "action".to_string(),
            },
            ChatEvent::AgentRateLimited {
                id: Uuid::new_v4(),
                wait_secs: 5.0,
                tokens_needed: 1000,
            },
            ChatEvent::AgentToolStart {
                id: Uuid::new_v4(),
                tool_name: "tool".to_string(),
            },
            ChatEvent::AgentToolEnd {
                id: Uuid::new_v4(),
                tool_name: "tool".to_string(),
                success: true,
            },
            ChatEvent::AgentCompleted {
                id: Uuid::new_v4(),
                files_changed: vec![],
                summary: None,
            },
            ChatEvent::AgentFailed {
                id: Uuid::new_v4(),
                error: "error".to_string(),
            },
            ChatEvent::AgentCancelled { id: Uuid::new_v4() },
            ChatEvent::Error("error".to_string()),
            ChatEvent::Status("status".to_string()),
            ChatEvent::SessionEnded,
            ChatEvent::Refresh,
        ];

        for event in events {
            let debug_str = format!("{:?}", event);
            assert!(!debug_str.is_empty());
        }
    }
}
