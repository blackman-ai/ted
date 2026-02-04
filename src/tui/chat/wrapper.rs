// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Event wrapper for non-invasive agent event emission
//!
//! This module provides wrappers that emit events to the chat TUI
//! without modifying the core agent runner code.

use futures::StreamExt;
use uuid::Uuid;

use crate::llm::provider::{ContentBlockDelta, ContentBlockResponse, StreamEvent};
use crate::tools::{ToolExecutor, ToolResult};

use super::events::{ChatEvent, EventEmitter, EventSender};

/// Wraps tool execution to emit events to the chat TUI
pub struct ToolExecutorWrapper {
    executor: ToolExecutor,
    emitter: EventEmitter,
}

impl ToolExecutorWrapper {
    pub fn new(executor: ToolExecutor, event_tx: EventSender) -> Self {
        Self {
            executor,
            emitter: EventEmitter::new(event_tx),
        }
    }

    /// Execute a tool and emit start/end events
    pub async fn execute_tool_use(
        &mut self,
        tool_use_id: &str,
        name: &str,
        input: serde_json::Value,
    ) -> crate::error::Result<ToolResult> {
        // Emit start event
        self.emitter.tool_start(tool_use_id, name, input.clone());

        // Execute the tool
        let result = self
            .executor
            .execute_tool_use(tool_use_id, name, input)
            .await?;

        // Emit end event
        self.emitter.tool_end(tool_use_id, name, result.clone());

        Ok(result)
    }

    /// Get mutable reference to the inner executor
    pub fn inner_mut(&mut self) -> &mut ToolExecutor {
        &mut self.executor
    }

    /// Get reference to the inner executor
    pub fn inner(&self) -> &ToolExecutor {
        &self.executor
    }
}

/// Wraps LLM streaming to emit events to the chat TUI
pub struct StreamingWrapper {
    emitter: EventEmitter,
}

impl StreamingWrapper {
    pub fn new(event_tx: EventSender) -> Self {
        Self {
            emitter: EventEmitter::new(event_tx),
        }
    }

    /// Process a stream and emit events
    pub async fn process_stream<S>(&self, mut stream: S) -> crate::error::Result<String>
    where
        S: futures::Stream<Item = Result<StreamEvent, crate::error::TedError>> + Unpin,
    {
        let mut full_text = String::new();
        let mut current_tool_id: Option<String> = None;

        self.emitter.stream_start();

        while let Some(event) = stream.next().await {
            match event {
                Ok(StreamEvent::ContentBlockStart { content_block, .. }) => {
                    if let ContentBlockResponse::ToolUse { id, name, .. } = content_block {
                        current_tool_id = Some(id.clone());
                        self.emitter.tool_start(&id, &name, serde_json::Value::Null);
                    }
                }
                Ok(StreamEvent::ContentBlockDelta { delta, .. }) => {
                    if let ContentBlockDelta::TextDelta { text } = delta {
                        full_text.push_str(&text);
                        self.emitter.stream_delta(&text);
                    }
                }
                Ok(StreamEvent::ContentBlockStop { .. }) => {
                    if let Some(_id) = current_tool_id.take() {
                        // Tool block completed - actual result comes from tool execution
                    }
                }
                Ok(StreamEvent::MessageStop) => {
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    self.emitter.error(&e.to_string());
                    return Err(e);
                }
            }
        }

        self.emitter.stream_end();
        Ok(full_text)
    }
}

/// Observer for agent lifecycle events
///
/// This can be passed into the agent runner to receive updates
/// without modifying the core agent logic.
#[derive(Clone)]
pub struct AgentObserver {
    emitter: EventEmitter,
    agent_id: Uuid,
}

impl AgentObserver {
    pub fn new(agent_id: Uuid, event_tx: EventSender) -> Self {
        Self {
            emitter: EventEmitter::new(event_tx),
            agent_id,
        }
    }

    /// Called when agent is spawned
    pub fn on_spawn(&self, name: &str, agent_type: &str, task: &str) {
        self.emitter
            .agent_spawned(self.agent_id, name, agent_type, task);
    }

    /// Called on each iteration
    pub fn on_iteration(&self, iteration: u32, max_iterations: u32, action: &str) {
        self.emitter
            .agent_progress(self.agent_id, iteration, max_iterations, action);
    }

    /// Called when waiting for rate limit
    pub fn on_rate_limited(&self, wait_secs: f64, tokens_needed: u64) {
        self.emitter.emit(ChatEvent::AgentRateLimited {
            id: self.agent_id,
            wait_secs,
            tokens_needed,
        });
    }

    /// Called when agent starts a tool
    pub fn on_tool_start(&self, tool_name: &str) {
        self.emitter.emit(ChatEvent::AgentToolStart {
            id: self.agent_id,
            tool_name: tool_name.to_string(),
        });
    }

    /// Called when agent finishes a tool
    pub fn on_tool_end(&self, tool_name: &str, success: bool) {
        self.emitter.emit(ChatEvent::AgentToolEnd {
            id: self.agent_id,
            tool_name: tool_name.to_string(),
            success,
        });
    }

    /// Called when agent completes successfully
    pub fn on_complete(&self, files_changed: Vec<String>, summary: Option<String>) {
        self.emitter
            .agent_completed(self.agent_id, files_changed, summary);
    }

    /// Called when agent fails
    pub fn on_error(&self, error: &str) {
        self.emitter.agent_failed(self.agent_id, error);
    }

    /// Called when agent is cancelled
    pub fn on_cancelled(&self) {
        self.emitter
            .emit(ChatEvent::AgentCancelled { id: self.agent_id });
    }
}

/// Factory for creating agent observers
pub struct AgentObserverFactory {
    event_tx: EventSender,
}

impl AgentObserverFactory {
    pub fn new(event_tx: EventSender) -> Self {
        Self { event_tx }
    }

    /// Create a new observer for an agent
    pub fn create(&self, agent_id: Uuid) -> AgentObserver {
        AgentObserver::new(agent_id, self.event_tx.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn test_agent_observer() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent_id = Uuid::new_v4();
        let observer = AgentObserver::new(agent_id, tx);

        observer.on_spawn("test-agent", "explore", "Test task");

        // Check event was sent
        let event = rx.try_recv().unwrap();
        assert!(matches!(event, ChatEvent::AgentSpawned { .. }));
    }

    #[test]
    fn test_agent_observer_factory() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let factory = AgentObserverFactory::new(tx);

        let observer1 = factory.create(Uuid::new_v4());
        let observer2 = factory.create(Uuid::new_v4());

        // Both should work independently
        assert_ne!(observer1.agent_id, observer2.agent_id);
    }

    #[test]
    fn test_agent_observer_on_iteration() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent_id = Uuid::new_v4();
        let observer = AgentObserver::new(agent_id, tx);

        observer.on_iteration(5, 30, "Reading files");

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::AgentProgress {
                id,
                iteration,
                max_iterations,
                action,
            } => {
                assert_eq!(id, agent_id);
                assert_eq!(iteration, 5);
                assert_eq!(max_iterations, 30);
                assert_eq!(action, "Reading files");
            }
            _ => panic!("Expected AgentProgress event"),
        }
    }

    #[test]
    fn test_agent_observer_on_rate_limited() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent_id = Uuid::new_v4();
        let observer = AgentObserver::new(agent_id, tx);

        observer.on_rate_limited(15.5, 10000);

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::AgentRateLimited {
                id,
                wait_secs,
                tokens_needed,
            } => {
                assert_eq!(id, agent_id);
                assert!((wait_secs - 15.5).abs() < 0.01);
                assert_eq!(tokens_needed, 10000);
            }
            _ => panic!("Expected AgentRateLimited event"),
        }
    }

    #[test]
    fn test_agent_observer_on_tool_start() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent_id = Uuid::new_v4();
        let observer = AgentObserver::new(agent_id, tx);

        observer.on_tool_start("file_read");

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::AgentToolStart { id, tool_name } => {
                assert_eq!(id, agent_id);
                assert_eq!(tool_name, "file_read");
            }
            _ => panic!("Expected AgentToolStart event"),
        }
    }

    #[test]
    fn test_agent_observer_on_tool_end() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent_id = Uuid::new_v4();
        let observer = AgentObserver::new(agent_id, tx);

        observer.on_tool_end("file_read", true);

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::AgentToolEnd {
                id,
                tool_name,
                success,
            } => {
                assert_eq!(id, agent_id);
                assert_eq!(tool_name, "file_read");
                assert!(success);
            }
            _ => panic!("Expected AgentToolEnd event"),
        }
    }

    #[test]
    fn test_agent_observer_on_tool_end_failure() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent_id = Uuid::new_v4();
        let observer = AgentObserver::new(agent_id, tx);

        observer.on_tool_end("shell", false);

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::AgentToolEnd {
                id,
                tool_name,
                success,
            } => {
                assert_eq!(id, agent_id);
                assert_eq!(tool_name, "shell");
                assert!(!success);
            }
            _ => panic!("Expected AgentToolEnd event"),
        }
    }

    #[test]
    fn test_agent_observer_on_complete() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent_id = Uuid::new_v4();
        let observer = AgentObserver::new(agent_id, tx);

        let files = vec!["file1.rs".to_string(), "file2.rs".to_string()];
        observer.on_complete(files.clone(), Some("Task completed".to_string()));

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::AgentCompleted {
                id,
                files_changed,
                summary,
            } => {
                assert_eq!(id, agent_id);
                assert_eq!(files_changed, files);
                assert_eq!(summary, Some("Task completed".to_string()));
            }
            _ => panic!("Expected AgentCompleted event"),
        }
    }

    #[test]
    fn test_agent_observer_on_complete_no_summary() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent_id = Uuid::new_v4();
        let observer = AgentObserver::new(agent_id, tx);

        observer.on_complete(vec![], None);

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::AgentCompleted {
                id,
                files_changed,
                summary,
            } => {
                assert_eq!(id, agent_id);
                assert!(files_changed.is_empty());
                assert!(summary.is_none());
            }
            _ => panic!("Expected AgentCompleted event"),
        }
    }

    #[test]
    fn test_agent_observer_on_error() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent_id = Uuid::new_v4();
        let observer = AgentObserver::new(agent_id, tx);

        observer.on_error("Something went wrong");

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::AgentFailed { id, error } => {
                assert_eq!(id, agent_id);
                assert_eq!(error, "Something went wrong");
            }
            _ => panic!("Expected AgentFailed event"),
        }
    }

    #[test]
    fn test_agent_observer_on_cancelled() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent_id = Uuid::new_v4();
        let observer = AgentObserver::new(agent_id, tx);

        observer.on_cancelled();

        let event = rx.try_recv().unwrap();
        match event {
            ChatEvent::AgentCancelled { id } => {
                assert_eq!(id, agent_id);
            }
            _ => panic!("Expected AgentCancelled event"),
        }
    }

    #[test]
    fn test_agent_observer_clone() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent_id = Uuid::new_v4();
        let observer = AgentObserver::new(agent_id, tx);
        let observer2 = observer.clone();

        observer.on_spawn("agent1", "explore", "task1");
        observer2.on_spawn("agent2", "implement", "task2");

        // Both events should be received
        let event1 = rx.try_recv().unwrap();
        let event2 = rx.try_recv().unwrap();
        assert!(matches!(event1, ChatEvent::AgentSpawned { .. }));
        assert!(matches!(event2, ChatEvent::AgentSpawned { .. }));
    }

    #[test]
    fn test_streaming_wrapper_new() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let _wrapper = StreamingWrapper::new(tx);
        // Just verify it can be created
    }

    #[test]
    fn test_agent_observer_full_lifecycle() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let agent_id = Uuid::new_v4();
        let observer = AgentObserver::new(agent_id, tx);

        // Simulate full agent lifecycle
        observer.on_spawn("test-agent", "explore", "Find files");
        observer.on_iteration(1, 10, "Starting");
        observer.on_tool_start("glob");
        observer.on_tool_end("glob", true);
        observer.on_iteration(2, 10, "Processing results");
        observer.on_complete(
            vec!["file.rs".to_string()],
            Some("Found 1 file".to_string()),
        );

        // Verify all events in order
        assert!(matches!(
            rx.try_recv().unwrap(),
            ChatEvent::AgentSpawned { .. }
        ));
        assert!(matches!(
            rx.try_recv().unwrap(),
            ChatEvent::AgentProgress { .. }
        ));
        assert!(matches!(
            rx.try_recv().unwrap(),
            ChatEvent::AgentToolStart { .. }
        ));
        assert!(matches!(
            rx.try_recv().unwrap(),
            ChatEvent::AgentToolEnd { .. }
        ));
        assert!(matches!(
            rx.try_recv().unwrap(),
            ChatEvent::AgentProgress { .. }
        ));
        assert!(matches!(
            rx.try_recv().unwrap(),
            ChatEvent::AgentCompleted { .. }
        ));
    }

    // ===== ToolExecutorWrapper Tests =====

    #[tokio::test]
    async fn test_tool_executor_wrapper_new() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let tool_context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            Uuid::new_v4(),
            false,
        );
        let executor = crate::tools::ToolExecutor::new(tool_context, false);
        let (tx, _rx) = mpsc::unbounded_channel();

        let wrapper = ToolExecutorWrapper::new(executor, tx);

        // Should be able to access inner
        let _inner = wrapper.inner();
    }

    #[tokio::test]
    async fn test_tool_executor_wrapper_inner_mut() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let tool_context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            Uuid::new_v4(),
            false,
        );
        let executor = crate::tools::ToolExecutor::new(tool_context, false);
        let (tx, _rx) = mpsc::unbounded_channel();

        let mut wrapper = ToolExecutorWrapper::new(executor, tx);

        // Should be able to get mutable inner
        let _inner = wrapper.inner_mut();
    }

    #[tokio::test]
    async fn test_tool_executor_wrapper_execute_tool_use() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let tool_context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            Uuid::new_v4(),
            false,
        );
        let executor = crate::tools::ToolExecutor::new(tool_context, false);
        let (tx, mut rx) = mpsc::unbounded_channel();

        let mut wrapper = ToolExecutorWrapper::new(executor, tx);

        // Execute a glob tool (should work without special setup)
        let result = wrapper
            .execute_tool_use(
                "test-id",
                "glob",
                serde_json::json!({
                    "pattern": "*.txt",
                    "path": temp_dir.path().to_str().unwrap()
                }),
            )
            .await;

        // Should complete without error (even if no files match)
        assert!(result.is_ok());

        // Should have emitted start and end events
        let event1 = rx.try_recv().unwrap();
        assert!(matches!(event1, ChatEvent::ToolCallStart { .. }));

        let event2 = rx.try_recv().unwrap();
        assert!(matches!(event2, ChatEvent::ToolCallEnd { .. }));
    }

    #[tokio::test]
    async fn test_tool_executor_wrapper_execute_unknown_tool() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let tool_context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            Uuid::new_v4(),
            false,
        );
        let executor = crate::tools::ToolExecutor::new(tool_context, false);
        let (tx, mut rx) = mpsc::unbounded_channel();

        let mut wrapper = ToolExecutorWrapper::new(executor, tx);

        // Execute an unknown tool
        let result = wrapper
            .execute_tool_use("test-id", "unknown_tool", serde_json::json!({}))
            .await;

        // Should return an error result
        assert!(result.is_err());

        // Should still have emitted start event
        let event1 = rx.try_recv().unwrap();
        assert!(matches!(event1, ChatEvent::ToolCallStart { .. }));
    }

    // ===== StreamingWrapper Tests =====

    #[tokio::test]
    async fn test_streaming_wrapper_process_stream_empty() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let wrapper = StreamingWrapper::new(tx);

        // Create an empty stream that just sends MessageStop
        let events: Vec<Result<StreamEvent, crate::error::TedError>> =
            vec![Ok(StreamEvent::MessageStop)];
        let stream = futures::stream::iter(events);

        let result = wrapper.process_stream(stream).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");

        // Should have emitted stream start and end
        let event1 = rx.try_recv().unwrap();
        assert!(matches!(event1, ChatEvent::StreamStart));

        let event2 = rx.try_recv().unwrap();
        assert!(matches!(event2, ChatEvent::StreamEnd));
    }

    #[tokio::test]
    async fn test_streaming_wrapper_process_stream_with_text() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let wrapper = StreamingWrapper::new(tx);

        // Create a stream with text deltas
        let events: Vec<Result<StreamEvent, crate::error::TedError>> = vec![
            Ok(StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::TextDelta {
                    text: "Hello ".to_string(),
                },
            }),
            Ok(StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::TextDelta {
                    text: "World!".to_string(),
                },
            }),
            Ok(StreamEvent::MessageStop),
        ];
        let stream = futures::stream::iter(events);

        let result = wrapper.process_stream(stream).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello World!");

        // Should have emitted stream start, deltas, and end
        let event1 = rx.try_recv().unwrap();
        assert!(matches!(event1, ChatEvent::StreamStart));

        // Two delta events
        let event2 = rx.try_recv().unwrap();
        assert!(matches!(event2, ChatEvent::StreamDelta(_)));

        let event3 = rx.try_recv().unwrap();
        assert!(matches!(event3, ChatEvent::StreamDelta(_)));

        let event4 = rx.try_recv().unwrap();
        assert!(matches!(event4, ChatEvent::StreamEnd));
    }

    #[tokio::test]
    async fn test_streaming_wrapper_process_stream_with_tool() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let wrapper = StreamingWrapper::new(tx);

        // Create a stream with tool use
        let events: Vec<Result<StreamEvent, crate::error::TedError>> = vec![
            Ok(StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlockResponse::ToolUse {
                    id: "tool-1".to_string(),
                    name: "file_read".to_string(),
                    input: serde_json::json!({"path": "/test"}),
                },
            }),
            Ok(StreamEvent::ContentBlockStop { index: 0 }),
            Ok(StreamEvent::MessageStop),
        ];
        let stream = futures::stream::iter(events);

        let result = wrapper.process_stream(stream).await;
        assert!(result.is_ok());

        // Should have emitted stream start, tool start, and stream end
        let event1 = rx.try_recv().unwrap();
        assert!(matches!(event1, ChatEvent::StreamStart));

        let event2 = rx.try_recv().unwrap();
        assert!(matches!(event2, ChatEvent::ToolCallStart { .. }));

        let event3 = rx.try_recv().unwrap();
        assert!(matches!(event3, ChatEvent::StreamEnd));
    }

    #[tokio::test]
    async fn test_streaming_wrapper_process_stream_with_error() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let wrapper = StreamingWrapper::new(tx);

        // Create a stream that errors
        let events: Vec<Result<StreamEvent, crate::error::TedError>> = vec![
            Ok(StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::TextDelta {
                    text: "Start".to_string(),
                },
            }),
            Err(crate::error::TedError::Api(
                crate::error::ApiError::StreamError("Test error".to_string()),
            )),
        ];
        let stream = futures::stream::iter(events);

        let result = wrapper.process_stream(stream).await;
        assert!(result.is_err());

        // Should have emitted stream start, delta, and error
        let event1 = rx.try_recv().unwrap();
        assert!(matches!(event1, ChatEvent::StreamStart));

        let event2 = rx.try_recv().unwrap();
        assert!(matches!(event2, ChatEvent::StreamDelta(_)));

        let event3 = rx.try_recv().unwrap();
        assert!(matches!(event3, ChatEvent::Error(_)));
    }

    #[tokio::test]
    async fn test_streaming_wrapper_process_stream_message_start() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let wrapper = StreamingWrapper::new(tx);

        // Create a stream with MessageStart (should be handled)
        let events: Vec<Result<StreamEvent, crate::error::TedError>> = vec![
            Ok(StreamEvent::MessageStart {
                id: "msg-1".to_string(),
                model: "test-model".to_string(),
            }),
            Ok(StreamEvent::MessageStop),
        ];
        let stream = futures::stream::iter(events);

        let result = wrapper.process_stream(stream).await;
        assert!(result.is_ok());

        // Should have stream start and end
        let event1 = rx.try_recv().unwrap();
        assert!(matches!(event1, ChatEvent::StreamStart));

        let event2 = rx.try_recv().unwrap();
        assert!(matches!(event2, ChatEvent::StreamEnd));
    }

    #[tokio::test]
    async fn test_streaming_wrapper_process_stream_message_delta() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let wrapper = StreamingWrapper::new(tx);

        use crate::llm::provider::{StopReason, Usage};

        // Create a stream with MessageDelta (should be handled)
        let events: Vec<Result<StreamEvent, crate::error::TedError>> = vec![
            Ok(StreamEvent::MessageDelta {
                stop_reason: Some(StopReason::EndTurn),
                usage: Some(Usage {
                    input_tokens: 10,
                    output_tokens: 20,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                }),
            }),
            Ok(StreamEvent::MessageStop),
        ];
        let stream = futures::stream::iter(events);

        let result = wrapper.process_stream(stream).await;
        assert!(result.is_ok());

        // Should have stream start and end
        let event1 = rx.try_recv().unwrap();
        assert!(matches!(event1, ChatEvent::StreamStart));

        let event2 = rx.try_recv().unwrap();
        assert!(matches!(event2, ChatEvent::StreamEnd));
    }
}
