// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! LLM Provider trait and related types
//!
//! Defines the abstraction layer for different LLM backends.

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

use crate::error::Result;
use crate::llm::message::Message;

/// Main trait for LLM providers
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Get the provider name (e.g., "anthropic", "openai")
    fn name(&self) -> &str;

    /// List available models
    fn available_models(&self) -> Vec<ModelInfo>;

    /// Check if a specific model is supported
    fn supports_model(&self, model: &str) -> bool;

    /// Get model info by ID
    fn get_model_info(&self, model: &str) -> Option<ModelInfo> {
        self.available_models().into_iter().find(|m| m.id == model)
    }

    /// Non-streaming completion
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;

    /// Streaming completion
    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>>;

    /// Count tokens for a text (provider-specific tokenization)
    fn count_tokens(&self, text: &str, model: &str) -> Result<u32>;
}

/// Request for completion
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// Model to use
    pub model: String,

    /// Messages in the conversation
    pub messages: Vec<Message>,

    /// System prompt
    pub system: Option<String>,

    /// Maximum tokens in response
    pub max_tokens: u32,

    /// Sampling temperature
    pub temperature: f32,

    /// Tools available for the model to use
    pub tools: Vec<ToolDefinition>,

    /// How to handle tool choice
    pub tool_choice: ToolChoice,
}

/// Response from a completion request
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    /// Response ID
    pub id: String,

    /// Model used
    pub model: String,

    /// Response content
    pub content: Vec<ContentBlockResponse>,

    /// Stop reason
    pub stop_reason: Option<StopReason>,

    /// Token usage
    pub usage: Usage,
}

/// A content block in the response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlockResponse {
    /// Text content
    Text { text: String },

    /// Tool use request
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

/// Why the model stopped generating
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Natural end of message
    EndTurn,
    /// Hit max tokens
    MaxTokens,
    /// Wants to use a tool
    ToolUse,
    /// Stop sequence hit
    StopSequence,
}

/// Token usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Input tokens
    pub input_tokens: u32,
    /// Output tokens
    pub output_tokens: u32,
    /// Cache creation tokens (if caching enabled)
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    /// Cache read tokens (if caching enabled)
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

/// Events from a streaming response
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Start of message
    MessageStart { id: String, model: String },

    /// Start of a content block
    ContentBlockStart {
        index: usize,
        content_block: ContentBlockResponse,
    },

    /// Delta to a content block
    ContentBlockDelta {
        index: usize,
        delta: ContentBlockDelta,
    },

    /// End of a content block
    ContentBlockStop { index: usize },

    /// Message delta (stop reason, usage)
    MessageDelta {
        stop_reason: Option<StopReason>,
        usage: Option<Usage>,
    },

    /// End of message
    MessageStop,

    /// Ping (keep-alive)
    Ping,

    /// Error
    Error { error_type: String, message: String },
}

/// Delta update to a content block
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlockDelta {
    /// Text delta
    TextDelta { text: String },

    /// Partial JSON for tool input
    InputJsonDelta { partial_json: String },
}

/// Tool definition for the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,

    /// Tool description
    pub description: String,

    /// Input schema (JSON Schema)
    pub input_schema: ToolInputSchema,
}

/// Input schema for a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInputSchema {
    /// Schema type (always "object")
    #[serde(rename = "type")]
    pub schema_type: String,

    /// Property definitions
    pub properties: serde_json::Value,

    /// Required properties
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
}

/// How the model should choose to use tools
#[derive(Debug, Clone, Default)]
pub enum ToolChoice {
    /// Let the model decide
    #[default]
    Auto,
    /// Don't use any tools
    None,
    /// Must use a tool
    Required,
    /// Use a specific tool
    Specific(String),
}

/// Information about a model
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Model identifier
    pub id: String,

    /// Human-readable name
    pub display_name: String,

    /// Maximum context window in tokens
    pub context_window: u32,

    /// Maximum output tokens
    pub max_output_tokens: u32,

    /// Whether the model supports tool use
    pub supports_tools: bool,

    /// Whether the model supports vision
    pub supports_vision: bool,

    /// Input cost per 1K tokens (USD)
    pub input_cost_per_1k: f64,

    /// Output cost per 1K tokens (USD)
    pub output_cost_per_1k: f64,
}

impl CompletionRequest {
    /// Create a new completion request
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            system: None,
            max_tokens: 8192,
            temperature: 0.7,
            tools: vec![],
            tool_choice: ToolChoice::Auto,
        }
    }

    /// Set the system prompt
    pub fn with_system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Set tools
    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    /// Set tool choice
    pub fn with_tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = tool_choice;
        self
    }
}

impl Usage {
    /// Get total tokens used
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::message::Message;

    // ===== CompletionRequest Tests =====

    #[test]
    fn test_completion_request_new() {
        let messages = vec![Message::user("Hello")];
        let request = CompletionRequest::new("claude-3", messages);

        assert_eq!(request.model, "claude-3");
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.max_tokens, 8192);
        assert!((request.temperature - 0.7).abs() < 0.001);
        assert!(request.system.is_none());
        assert!(request.tools.is_empty());
    }

    #[test]
    fn test_completion_request_with_system() {
        let messages = vec![Message::user("Hello")];
        let request =
            CompletionRequest::new("claude-3", messages).with_system("You are a helpful assistant");

        assert_eq!(
            request.system,
            Some("You are a helpful assistant".to_string())
        );
    }

    #[test]
    fn test_completion_request_with_max_tokens() {
        let messages = vec![Message::user("Hello")];
        let request = CompletionRequest::new("claude-3", messages).with_max_tokens(4096);

        assert_eq!(request.max_tokens, 4096);
    }

    #[test]
    fn test_completion_request_with_temperature() {
        let messages = vec![Message::user("Hello")];
        let request = CompletionRequest::new("claude-3", messages).with_temperature(0.5);

        assert!((request.temperature - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_completion_request_with_tools() {
        let tools = vec![ToolDefinition {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: vec![],
            },
        }];
        let messages = vec![Message::user("Hello")];
        let request = CompletionRequest::new("claude-3", messages).with_tools(tools);

        assert_eq!(request.tools.len(), 1);
        assert_eq!(request.tools[0].name, "test_tool");
    }

    #[test]
    fn test_completion_request_with_tool_choice() {
        let messages = vec![Message::user("Hello")];
        let request =
            CompletionRequest::new("claude-3", messages).with_tool_choice(ToolChoice::Required);

        assert!(matches!(request.tool_choice, ToolChoice::Required));
    }

    #[test]
    fn test_completion_request_chained() {
        let messages = vec![Message::user("Hello")];
        let request = CompletionRequest::new("claude-3", messages)
            .with_system("System prompt")
            .with_max_tokens(2048)
            .with_temperature(0.9)
            .with_tool_choice(ToolChoice::None);

        assert_eq!(request.system, Some("System prompt".to_string()));
        assert_eq!(request.max_tokens, 2048);
        assert!((request.temperature - 0.9).abs() < 0.001);
        assert!(matches!(request.tool_choice, ToolChoice::None));
    }

    // ===== Usage Tests =====

    #[test]
    fn test_usage_total_tokens() {
        let usage = Usage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };

        assert_eq!(usage.total_tokens(), 150);
    }

    #[test]
    fn test_usage_default() {
        let usage = Usage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.cache_creation_input_tokens, 0);
        assert_eq!(usage.cache_read_input_tokens, 0);
        assert_eq!(usage.total_tokens(), 0);
    }

    #[test]
    fn test_usage_with_cache() {
        let usage = Usage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: 25,
            cache_read_input_tokens: 10,
        };

        // total_tokens only counts input + output
        assert_eq!(usage.total_tokens(), 150);
        // But cache tokens are stored
        assert_eq!(usage.cache_creation_input_tokens, 25);
        assert_eq!(usage.cache_read_input_tokens, 10);
    }

    // ===== StopReason Tests =====

    #[test]
    fn test_stop_reason_equality() {
        assert_eq!(StopReason::EndTurn, StopReason::EndTurn);
        assert_eq!(StopReason::MaxTokens, StopReason::MaxTokens);
        assert_eq!(StopReason::ToolUse, StopReason::ToolUse);
        assert_eq!(StopReason::StopSequence, StopReason::StopSequence);
        assert_ne!(StopReason::EndTurn, StopReason::ToolUse);
    }

    #[test]
    fn test_stop_reason_debug() {
        let reason = StopReason::EndTurn;
        let debug = format!("{:?}", reason);
        assert!(debug.contains("EndTurn"));
    }

    // ===== ToolChoice Tests =====

    #[test]
    fn test_tool_choice_default() {
        let choice = ToolChoice::default();
        assert!(matches!(choice, ToolChoice::Auto));
    }

    #[test]
    fn test_tool_choice_variants() {
        let auto = ToolChoice::Auto;
        let none = ToolChoice::None;
        let required = ToolChoice::Required;
        let specific = ToolChoice::Specific("test_tool".to_string());

        assert!(matches!(auto, ToolChoice::Auto));
        assert!(matches!(none, ToolChoice::None));
        assert!(matches!(required, ToolChoice::Required));

        if let ToolChoice::Specific(name) = specific {
            assert_eq!(name, "test_tool");
        } else {
            panic!("Expected Specific variant");
        }
    }

    // ===== ToolDefinition Tests =====

    #[test]
    fn test_tool_definition_creation() {
        let tool = ToolDefinition {
            name: "file_read".to_string(),
            description: "Read a file".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({
                    "path": {
                        "type": "string",
                        "description": "Path to the file"
                    }
                }),
                required: vec!["path".to_string()],
            },
        };

        assert_eq!(tool.name, "file_read");
        assert_eq!(tool.description, "Read a file");
        assert_eq!(tool.input_schema.schema_type, "object");
        assert_eq!(tool.input_schema.required.len(), 1);
    }

    #[test]
    fn test_tool_definition_clone() {
        let tool = ToolDefinition {
            name: "test".to_string(),
            description: "test desc".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: vec![],
            },
        };

        let cloned = tool.clone();
        assert_eq!(cloned.name, tool.name);
        assert_eq!(cloned.description, tool.description);
    }

    // ===== ToolInputSchema Tests =====

    #[test]
    fn test_tool_input_schema_empty() {
        let schema = ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({}),
            required: vec![],
        };

        assert_eq!(schema.schema_type, "object");
        assert!(schema.required.is_empty());
    }

    #[test]
    fn test_tool_input_schema_with_properties() {
        let schema = ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({
                "path": {"type": "string"},
                "content": {"type": "string"}
            }),
            required: vec!["path".to_string()],
        };

        assert_eq!(schema.required.len(), 1);
        assert_eq!(schema.required[0], "path");
    }

    // ===== ContentBlockResponse Tests =====

    #[test]
    fn test_content_block_response_text() {
        let block = ContentBlockResponse::Text {
            text: "Hello world".to_string(),
        };

        if let ContentBlockResponse::Text { text } = block {
            assert_eq!(text, "Hello world");
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn test_content_block_response_tool_use() {
        let block = ContentBlockResponse::ToolUse {
            id: "tool_123".to_string(),
            name: "file_read".to_string(),
            input: serde_json::json!({"path": "/test.txt"}),
        };

        if let ContentBlockResponse::ToolUse { id, name, input } = block {
            assert_eq!(id, "tool_123");
            assert_eq!(name, "file_read");
            assert!(input.get("path").is_some());
        } else {
            panic!("Expected ToolUse variant");
        }
    }

    // ===== ContentBlockDelta Tests =====

    #[test]
    fn test_content_block_delta_text() {
        let delta = ContentBlockDelta::TextDelta {
            text: "Hello".to_string(),
        };

        if let ContentBlockDelta::TextDelta { text } = delta {
            assert_eq!(text, "Hello");
        } else {
            panic!("Expected TextDelta variant");
        }
    }

    #[test]
    fn test_content_block_delta_json() {
        let delta = ContentBlockDelta::InputJsonDelta {
            partial_json: r#"{"path":"#.to_string(),
        };

        if let ContentBlockDelta::InputJsonDelta { partial_json } = delta {
            assert!(partial_json.contains("path"));
        } else {
            panic!("Expected InputJsonDelta variant");
        }
    }

    // ===== StreamEvent Tests =====

    #[test]
    fn test_stream_event_message_start() {
        let event = StreamEvent::MessageStart {
            id: "msg_123".to_string(),
            model: "claude-3".to_string(),
        };

        if let StreamEvent::MessageStart { id, model } = event {
            assert_eq!(id, "msg_123");
            assert_eq!(model, "claude-3");
        } else {
            panic!("Expected MessageStart variant");
        }
    }

    #[test]
    fn test_stream_event_content_block_start() {
        let event = StreamEvent::ContentBlockStart {
            index: 0,
            content_block: ContentBlockResponse::Text {
                text: String::new(),
            },
        };

        if let StreamEvent::ContentBlockStart { index, .. } = event {
            assert_eq!(index, 0);
        } else {
            panic!("Expected ContentBlockStart variant");
        }
    }

    #[test]
    fn test_stream_event_ping() {
        let event = StreamEvent::Ping;
        assert!(matches!(event, StreamEvent::Ping));
    }

    #[test]
    fn test_stream_event_error() {
        let event = StreamEvent::Error {
            error_type: "rate_limit".to_string(),
            message: "Too many requests".to_string(),
        };

        if let StreamEvent::Error {
            error_type,
            message,
        } = event
        {
            assert_eq!(error_type, "rate_limit");
            assert_eq!(message, "Too many requests");
        } else {
            panic!("Expected Error variant");
        }
    }

    // ===== ModelInfo Tests =====

    #[test]
    fn test_model_info_creation() {
        let info = ModelInfo {
            id: "claude-3".to_string(),
            display_name: "Claude 3".to_string(),
            context_window: 100000,
            max_output_tokens: 8192,
            supports_tools: true,
            supports_vision: true,
            input_cost_per_1k: 0.008,
            output_cost_per_1k: 0.024,
        };

        assert_eq!(info.id, "claude-3");
        assert_eq!(info.context_window, 100000);
        assert!(info.supports_tools);
        assert!(info.supports_vision);
    }

    #[test]
    fn test_model_info_clone() {
        let info = ModelInfo {
            id: "test".to_string(),
            display_name: "Test Model".to_string(),
            context_window: 1000,
            max_output_tokens: 100,
            supports_tools: false,
            supports_vision: false,
            input_cost_per_1k: 0.001,
            output_cost_per_1k: 0.002,
        };

        let cloned = info.clone();
        assert_eq!(cloned.id, info.id);
        assert_eq!(cloned.context_window, info.context_window);
    }

    // ===== CompletionResponse Tests =====

    #[test]
    fn test_completion_response_creation() {
        let response = CompletionResponse {
            id: "resp_123".to_string(),
            model: "claude-3".to_string(),
            content: vec![ContentBlockResponse::Text {
                text: "Hello".to_string(),
            }],
            stop_reason: Some(StopReason::EndTurn),
            usage: Usage::default(),
        };

        assert_eq!(response.id, "resp_123");
        assert_eq!(response.model, "claude-3");
        assert_eq!(response.content.len(), 1);
        assert_eq!(response.stop_reason, Some(StopReason::EndTurn));
    }

    #[test]
    fn test_completion_response_with_tool_use() {
        let response = CompletionResponse {
            id: "resp_456".to_string(),
            model: "claude-3".to_string(),
            content: vec![
                ContentBlockResponse::Text {
                    text: "Let me read that file".to_string(),
                },
                ContentBlockResponse::ToolUse {
                    id: "tool_789".to_string(),
                    name: "file_read".to_string(),
                    input: serde_json::json!({"path": "/test.txt"}),
                },
            ],
            stop_reason: Some(StopReason::ToolUse),
            usage: Usage {
                input_tokens: 50,
                output_tokens: 30,
                ..Default::default()
            },
        };

        assert_eq!(response.content.len(), 2);
        assert_eq!(response.stop_reason, Some(StopReason::ToolUse));
    }
}
