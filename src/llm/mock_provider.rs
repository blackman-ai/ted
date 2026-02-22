// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Mock LLM provider for testing
//!
//! Provides a configurable mock implementation of the LlmProvider trait
//! that can be used in unit tests without making real API calls.

use async_trait::async_trait;
use futures::stream::{self, Stream};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::Result;
use crate::llm::provider::{
    CompletionRequest, CompletionResponse, ContentBlockResponse, LlmProvider, ModelInfo,
    StopReason, StreamEvent, Usage,
};

/// A mock LLM provider for testing
#[derive(Clone)]
pub struct MockProvider {
    /// Provider name
    name: String,
    /// Configured responses
    responses: Arc<Mutex<Vec<MockResponse>>>,
    /// Call counter
    call_count: Arc<AtomicUsize>,
    /// Recorded requests
    recorded_requests: Arc<Mutex<Vec<CompletionRequest>>>,
    /// Available models
    models: Vec<ModelInfo>,
}

/// A pre-configured response for the mock provider
#[derive(Clone, Debug)]
pub struct MockResponse {
    /// Text content to return
    pub text: String,
    /// Tool calls to return (optional)
    pub tool_calls: Vec<MockToolCall>,
    /// Stop reason
    pub stop_reason: StopReason,
    /// Token usage
    pub usage: Usage,
}

/// A mock tool call
#[derive(Clone, Debug)]
pub struct MockToolCall {
    /// Tool call ID
    pub id: String,
    /// Tool name
    pub name: String,
    /// Tool input (JSON)
    pub input: serde_json::Value,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProvider {
    /// Create a new mock provider
    pub fn new() -> Self {
        Self {
            name: "mock".to_string(),
            responses: Arc::new(Mutex::new(vec![MockResponse::default()])),
            call_count: Arc::new(AtomicUsize::new(0)),
            recorded_requests: Arc::new(Mutex::new(vec![])),
            models: vec![Self::default_model()],
        }
    }

    /// Create a mock provider with a custom name
    pub fn with_name(name: impl Into<String>) -> Self {
        let mut provider = Self::new();
        provider.name = name.into();
        provider
    }

    /// Create a default model info
    fn default_model() -> ModelInfo {
        ModelInfo {
            id: "mock-model".to_string(),
            display_name: "Mock Model".to_string(),
            context_window: 128000,
            max_output_tokens: 8192,
            supports_tools: true,
            supports_vision: false,
            input_cost_per_1k: 0.0,
            output_cost_per_1k: 0.0,
        }
    }

    /// Set the text response
    pub fn with_response(self, text: impl Into<String>) -> Self {
        let mut responses = match self.responses.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!("Mock provider responses lock was poisoned, recovering");
                poisoned.into_inner()
            }
        };
        responses.clear();
        responses.push(MockResponse {
            text: text.into(),
            ..Default::default()
        });
        drop(responses);
        self
    }

    /// Queue multiple responses (returned in order)
    pub fn with_responses(self, texts: Vec<String>) -> Self {
        let mut responses = match self.responses.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!("Mock provider responses lock was poisoned, recovering");
                poisoned.into_inner()
            }
        };
        responses.clear();
        for text in texts {
            responses.push(MockResponse {
                text,
                ..Default::default()
            });
        }
        drop(responses);
        self
    }

    /// Set a tool call response
    pub fn with_tool_call(self, name: impl Into<String>, input: serde_json::Value) -> Self {
        let mut responses = match self.responses.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!("Mock provider responses lock was poisoned, recovering");
                poisoned.into_inner()
            }
        };
        responses.clear();
        responses.push(MockResponse {
            text: String::new(),
            tool_calls: vec![MockToolCall {
                id: format!("toolu_{}", uuid::Uuid::new_v4().simple()),
                name: name.into(),
                input,
            }],
            stop_reason: StopReason::ToolUse,
            usage: Usage::default(),
        });
        drop(responses);
        self
    }

    /// Add custom models
    pub fn with_models(mut self, models: Vec<ModelInfo>) -> Self {
        self.models = models;
        self
    }

    /// Get the number of times complete() was called
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    /// Get all recorded requests
    pub fn recorded_requests(&self) -> Vec<CompletionRequest> {
        self.recorded_requests.lock().unwrap().clone()
    }

    /// Get the last request made
    pub fn last_request(&self) -> Option<CompletionRequest> {
        self.recorded_requests.lock().unwrap().last().cloned()
    }

    /// Reset call count and recorded requests
    pub fn reset(&self) {
        self.call_count.store(0, Ordering::SeqCst);
        self.recorded_requests.lock().unwrap().clear();
    }

    /// Get the next response
    fn next_response(&self) -> MockResponse {
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);
        let responses = self.responses.lock().unwrap();
        // Cycle through responses or return the last one
        if responses.is_empty() {
            MockResponse::default()
        } else {
            responses[count.min(responses.len() - 1)].clone()
        }
    }
}

impl Default for MockResponse {
    fn default() -> Self {
        Self {
            text: "Mock response".to_string(),
            tool_calls: vec![],
            stop_reason: StopReason::EndTurn,
            usage: Usage {
                input_tokens: 10,
                output_tokens: 20,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
        }
    }
}

#[async_trait]
impl LlmProvider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        self.models.clone()
    }

    fn supports_model(&self, model: &str) -> bool {
        self.models.iter().any(|m| m.id == model)
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        // Record the request
        self.recorded_requests.lock().unwrap().push(request.clone());

        let response = self.next_response();

        let mut content = vec![];

        // Add text content if present
        if !response.text.is_empty() {
            content.push(ContentBlockResponse::Text {
                text: response.text,
            });
        }

        // Add tool calls
        for tool_call in response.tool_calls {
            content.push(ContentBlockResponse::ToolUse {
                id: tool_call.id,
                name: tool_call.name,
                input: tool_call.input,
            });
        }

        Ok(CompletionResponse {
            id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            model: request.model,
            content,
            stop_reason: Some(response.stop_reason),
            usage: response.usage,
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        // Record the request
        self.recorded_requests.lock().unwrap().push(request.clone());

        let response = self.next_response();
        let model = request.model.clone();
        let msg_id = format!("msg_{}", uuid::Uuid::new_v4().simple());

        let mut events = vec![Ok(StreamEvent::MessageStart {
            id: msg_id.clone(),
            model: model.clone(),
        })];

        // Add text content
        if !response.text.is_empty() {
            events.push(Ok(StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlockResponse::Text {
                    text: String::new(),
                },
            }));

            // Stream the text in chunks
            for chunk in response.text.chars().collect::<Vec<_>>().chunks(10) {
                let text: String = chunk.iter().collect();
                events.push(Ok(StreamEvent::ContentBlockDelta {
                    index: 0,
                    delta: crate::llm::provider::ContentBlockDelta::TextDelta { text },
                }));
            }

            events.push(Ok(StreamEvent::ContentBlockStop { index: 0 }));
        }

        // Add tool calls
        for (i, tool_call) in response.tool_calls.into_iter().enumerate() {
            let index = if response.text.is_empty() { i } else { i + 1 };
            events.push(Ok(StreamEvent::ContentBlockStart {
                index,
                content_block: ContentBlockResponse::ToolUse {
                    id: tool_call.id,
                    name: tool_call.name,
                    input: tool_call.input,
                },
            }));
            events.push(Ok(StreamEvent::ContentBlockStop { index }));
        }

        events.push(Ok(StreamEvent::MessageDelta {
            stop_reason: Some(response.stop_reason),
            usage: Some(response.usage),
        }));
        events.push(Ok(StreamEvent::MessageStop));

        Ok(Box::pin(stream::iter(events)))
    }

    fn count_tokens(&self, text: &str, _model: &str) -> Result<u32> {
        // Simple approximation: ~4 characters per token
        Ok((text.len() / 4).max(1) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::message::Message;

    #[test]
    fn test_mock_provider_creation() {
        let provider = MockProvider::new();
        assert_eq!(provider.name(), "mock");
        assert_eq!(provider.call_count(), 0);
    }

    #[test]
    fn test_mock_provider_with_name() {
        let provider = MockProvider::with_name("test-provider");
        assert_eq!(provider.name(), "test-provider");
    }

    #[test]
    fn test_mock_provider_available_models() {
        let provider = MockProvider::new();
        let models = provider.available_models();
        assert!(!models.is_empty());
        assert_eq!(models[0].id, "mock-model");
    }

    #[test]
    fn test_mock_provider_supports_model() {
        let provider = MockProvider::new();
        assert!(provider.supports_model("mock-model"));
        assert!(!provider.supports_model("unknown-model"));
    }

    #[test]
    fn test_mock_provider_with_custom_models() {
        let custom_model = ModelInfo {
            id: "custom-model".to_string(),
            display_name: "Custom Model".to_string(),
            context_window: 4096,
            max_output_tokens: 1024,
            supports_tools: false,
            supports_vision: true,
            input_cost_per_1k: 0.001,
            output_cost_per_1k: 0.002,
        };
        let provider = MockProvider::new().with_models(vec![custom_model]);
        assert!(provider.supports_model("custom-model"));
        assert!(!provider.supports_model("mock-model"));
    }

    #[test]
    fn test_mock_provider_with_response() {
        let provider = MockProvider::new().with_response("Hello, world!");
        let responses = provider.responses.lock().unwrap();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].text, "Hello, world!");
    }

    #[test]
    fn test_mock_provider_with_responses() {
        let provider = MockProvider::new().with_responses(vec![
            "First response".to_string(),
            "Second response".to_string(),
        ]);
        let responses = provider.responses.lock().unwrap();
        assert_eq!(responses.len(), 2);
    }

    #[tokio::test]
    async fn test_mock_provider_complete() {
        let provider = MockProvider::new().with_response("Test response");

        let request = CompletionRequest::new("mock-model", vec![Message::user("Hello")]);

        let response = provider.complete(request).await.unwrap();
        assert_eq!(response.model, "mock-model");
        assert!(!response.content.is_empty());

        if let ContentBlockResponse::Text { text } = &response.content[0] {
            assert_eq!(text, "Test response");
        } else {
            panic!("Expected text content");
        }

        assert_eq!(provider.call_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_provider_records_requests() {
        let provider = MockProvider::new();

        let request = CompletionRequest::new("mock-model", vec![Message::user("Test message")]);

        provider.complete(request.clone()).await.unwrap();

        let recorded = provider.recorded_requests();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].model, "mock-model");
    }

    #[tokio::test]
    async fn test_mock_provider_last_request() {
        let provider = MockProvider::new();

        let request1 = CompletionRequest::new("model-1", vec![Message::user("First")]);
        let request2 = CompletionRequest::new("model-2", vec![Message::user("Second")]);

        provider.complete(request1).await.unwrap();
        provider.complete(request2).await.unwrap();

        let last = provider.last_request().unwrap();
        assert_eq!(last.model, "model-2");
    }

    #[tokio::test]
    async fn test_mock_provider_reset() {
        let provider = MockProvider::new();

        let request = CompletionRequest::new("mock-model", vec![Message::user("Test")]);

        provider.complete(request).await.unwrap();
        assert_eq!(provider.call_count(), 1);
        assert!(!provider.recorded_requests().is_empty());

        provider.reset();
        assert_eq!(provider.call_count(), 0);
        assert!(provider.recorded_requests().is_empty());
    }

    #[tokio::test]
    async fn test_mock_provider_multiple_responses() {
        let provider = MockProvider::new().with_responses(vec![
            "First".to_string(),
            "Second".to_string(),
            "Third".to_string(),
        ]);

        let make_request = || CompletionRequest::new("mock-model", vec![Message::user("Test")]);

        let r1 = provider.complete(make_request()).await.unwrap();
        let r2 = provider.complete(make_request()).await.unwrap();
        let r3 = provider.complete(make_request()).await.unwrap();
        let r4 = provider.complete(make_request()).await.unwrap(); // Should repeat last

        let get_text = |r: CompletionResponse| -> String {
            if let ContentBlockResponse::Text { text } = &r.content[0] {
                text.clone()
            } else {
                panic!("Expected text")
            }
        };

        assert_eq!(get_text(r1), "First");
        assert_eq!(get_text(r2), "Second");
        assert_eq!(get_text(r3), "Third");
        assert_eq!(get_text(r4), "Third"); // Repeats last
    }

    #[tokio::test]
    async fn test_mock_provider_with_tool_call() {
        let provider = MockProvider::new()
            .with_tool_call("file_read", serde_json::json!({ "path": "/test/file.txt" }));

        let request = CompletionRequest::new("mock-model", vec![Message::user("Read a file")]);

        let response = provider.complete(request).await.unwrap();
        assert_eq!(response.stop_reason, Some(StopReason::ToolUse));

        let tool_use = response
            .content
            .iter()
            .find(|c| matches!(c, ContentBlockResponse::ToolUse { .. }));
        assert!(tool_use.is_some());

        if let ContentBlockResponse::ToolUse { name, input, .. } = tool_use.unwrap() {
            assert_eq!(name, "file_read");
            assert_eq!(input["path"], "/test/file.txt");
        }
    }

    #[test]
    fn test_mock_provider_count_tokens() {
        let provider = MockProvider::new();

        // ~4 chars per token
        assert_eq!(provider.count_tokens("test", "mock-model").unwrap(), 1);
        assert_eq!(
            provider.count_tokens("hello world", "mock-model").unwrap(),
            2
        );
        assert_eq!(
            provider
                .count_tokens("a".repeat(100).as_str(), "mock-model")
                .unwrap(),
            25
        );
    }

    #[tokio::test]
    async fn test_mock_provider_complete_stream() {
        use futures::StreamExt;

        let provider = MockProvider::new().with_response("Streaming test");

        let request = CompletionRequest::new("mock-model", vec![Message::user("Test")]);

        let mut stream = provider.complete_stream(request).await.unwrap();
        let mut events = vec![];

        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }

        // Should have: MessageStart, ContentBlockStart, ContentBlockDelta(s), ContentBlockStop, MessageDelta, MessageStop
        assert!(events.len() >= 4);
        assert!(matches!(events[0], StreamEvent::MessageStart { .. }));
        assert!(matches!(events.last().unwrap(), StreamEvent::MessageStop));
    }

    #[test]
    fn test_mock_response_default() {
        let response = MockResponse::default();
        assert_eq!(response.text, "Mock response");
        assert!(response.tool_calls.is_empty());
        assert_eq!(response.stop_reason, StopReason::EndTurn);
    }

    #[test]
    fn test_mock_provider_clone() {
        let provider = MockProvider::new().with_response("Cloneable");
        let cloned = provider.clone();

        assert_eq!(cloned.name(), provider.name());
        // They share the same Arc'd state - acquire lock once to avoid deadlock
        let text = provider.responses.lock().unwrap()[0].text.clone();
        assert_eq!(text, "Cloneable");
        // Verify the cloned provider points to the same Arc
        assert!(std::sync::Arc::ptr_eq(
            &provider.responses,
            &cloned.responses
        ));
    }

    #[test]
    fn test_mock_provider_default() {
        let provider = MockProvider::default();
        assert_eq!(provider.name(), "mock");
    }
}
