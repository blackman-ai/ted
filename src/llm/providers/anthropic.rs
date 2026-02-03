// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Anthropic Claude API provider implementation
//!
//! Implements the LlmProvider trait for Claude models.

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

use crate::error::{ApiError, Result, TedError};
use crate::llm::message::{ContentBlock, Message, MessageContent, Role, ToolResultContent};
use crate::llm::provider::{
    CompletionRequest, CompletionResponse, ContentBlockDelta, ContentBlockResponse, LlmProvider,
    ModelInfo, StopReason, StreamEvent, ToolChoice, ToolDefinition, Usage,
};

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic Claude provider
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: ANTHROPIC_API_URL.to_string(),
        }
    }

    /// Create with a custom base URL
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: base_url.into(),
        }
    }

    /// Convert internal messages to Anthropic format
    fn convert_messages(&self, messages: &[Message]) -> Vec<AnthropicMessage> {
        messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| {
                let role = match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::System => "user", // Should be filtered out
                };

                let content = match &m.content {
                    MessageContent::Text(text) => AnthropicContent::Text(text.clone()),
                    MessageContent::Blocks(blocks) => {
                        let converted: Vec<AnthropicContentBlock> = blocks
                            .iter()
                            .map(|b| match b {
                                ContentBlock::Text { text } => {
                                    AnthropicContentBlock::Text { text: text.clone() }
                                }
                                ContentBlock::ToolUse { id, name, input } => {
                                    AnthropicContentBlock::ToolUse {
                                        id: id.clone(),
                                        name: name.clone(),
                                        input: input.clone(),
                                    }
                                }
                                ContentBlock::ToolResult {
                                    tool_use_id,
                                    content,
                                    is_error,
                                } => {
                                    let content_str = match content {
                                        ToolResultContent::Text(t) => t.clone(),
                                        ToolResultContent::Blocks(blocks) => blocks
                                            .iter()
                                            .filter_map(|b| {
                                                if let crate::llm::message::ToolResultBlock::Text {
                                                    text,
                                                } = b
                                                {
                                                    Some(text.clone())
                                                } else {
                                                    None
                                                }
                                            })
                                            .collect::<Vec<_>>()
                                            .join("\n"),
                                    };
                                    AnthropicContentBlock::ToolResult {
                                        tool_use_id: tool_use_id.clone(),
                                        content: content_str,
                                        is_error: *is_error,
                                    }
                                }
                            })
                            .collect();
                        AnthropicContent::Blocks(converted)
                    }
                };

                AnthropicMessage {
                    role: role.to_string(),
                    content,
                }
            })
            .collect()
    }

    /// Convert tools to Anthropic format
    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<AnthropicTool> {
        tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: serde_json::json!({
                    "type": t.input_schema.schema_type,
                    "properties": t.input_schema.properties,
                    "required": t.input_schema.required,
                }),
            })
            .collect()
    }

    /// Build the request body
    fn build_request(&self, request: &CompletionRequest) -> AnthropicRequest {
        let tool_choice = match &request.tool_choice {
            ToolChoice::Auto => Some(AnthropicToolChoice::Auto),
            ToolChoice::None => None,
            ToolChoice::Required => Some(AnthropicToolChoice::Any),
            ToolChoice::Specific(name) => Some(AnthropicToolChoice::Tool { name: name.clone() }),
        };

        AnthropicRequest {
            model: request.model.clone(),
            messages: self.convert_messages(&request.messages),
            system: request.system.clone(),
            max_tokens: request.max_tokens,
            temperature: Some(request.temperature),
            tools: if request.tools.is_empty() {
                None
            } else {
                Some(self.convert_tools(&request.tools))
            },
            tool_choice,
            stream: Some(false),
        }
    }

    /// Extract Retry-After header value from HTTP response headers
    ///
    /// The Retry-After header can be either:
    /// - A number of seconds (e.g., "30")
    /// - An HTTP date (e.g., "Wed, 21 Oct 2015 07:28:00 GMT")
    ///
    /// We only parse the numeric form for simplicity.
    fn extract_retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
        headers
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
    }

    /// Parse token counts from an error message like "prompt is too long: 215300 tokens > 200000 maximum"
    fn parse_token_counts(message: &str) -> (u32, u32) {
        let numbers: Vec<u32> = message
            .split(|c: char| !c.is_ascii_digit())
            .filter_map(|s| s.parse().ok())
            .collect();

        match numbers.as_slice() {
            [current, limit, ..] => (*current, *limit),
            [single] => (*single, 0),
            _ => (0, 0),
        }
    }

    /// Parse an error response
    ///
    /// # Arguments
    /// * `status` - HTTP status code
    /// * `body` - Response body
    /// * `retry_after` - Optional Retry-After header value in seconds
    fn parse_error(&self, status: u16, body: &str, retry_after: Option<u64>) -> TedError {
        if let Ok(error_response) = serde_json::from_str::<AnthropicError>(body) {
            match error_response.error.error_type.as_str() {
                "authentication_error" => TedError::Api(ApiError::AuthenticationFailed),
                "rate_limit_error" => {
                    // Use Retry-After header if available, otherwise default to 10 seconds
                    let retry_secs = retry_after.unwrap_or(10) as u32;
                    TedError::Api(ApiError::RateLimited(retry_secs))
                }
                "invalid_request_error" => {
                    // Check for token limit errors (various phrasings)
                    let msg = &error_response.error.message;
                    if msg.contains("context")
                        || msg.contains("too long")
                        || msg.contains("tokens") && msg.contains("maximum")
                    {
                        // Try to parse numbers from the message
                        // Format: "prompt is too long: 215300 tokens > 200000 maximum"
                        let (current, limit) = Self::parse_token_counts(msg);
                        TedError::Api(ApiError::ContextTooLong { current, limit })
                    } else {
                        TedError::Api(ApiError::InvalidResponse(error_response.error.message))
                    }
                }
                _ => TedError::Api(ApiError::ServerError {
                    status,
                    message: error_response.error.message,
                }),
            }
        } else {
            TedError::Api(ApiError::ServerError {
                status,
                message: body.to_string(),
            })
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "claude-sonnet-4-20250514".to_string(),
                display_name: "Claude Sonnet 4".to_string(),
                context_window: 200_000,
                max_output_tokens: 64_000,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
            },
            ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                display_name: "Claude 3.5 Sonnet".to_string(),
                context_window: 200_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
            },
            ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                display_name: "Claude 3.5 Haiku".to_string(),
                context_window: 200_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.001,
                output_cost_per_1k: 0.005,
            },
        ]
    }

    fn supports_model(&self, model: &str) -> bool {
        self.available_models().iter().any(|m| m.id == model)
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let body = self.build_request(&request);

        let response = self
            .client
            .post(&self.base_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status().as_u16();

        if !response.status().is_success() {
            // Extract Retry-After header before consuming response body
            let retry_after = Self::extract_retry_after(response.headers());
            let body = response.text().await.unwrap_or_default();
            return Err(self.parse_error(status, &body, retry_after));
        }

        let api_response: AnthropicResponse = response.json().await?;

        let content = api_response
            .content
            .into_iter()
            .map(|block| match block {
                AnthropicContentBlock::Text { text } => ContentBlockResponse::Text { text },
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    ContentBlockResponse::ToolUse { id, name, input }
                }
                AnthropicContentBlock::ToolResult { .. } => {
                    // This shouldn't appear in a response
                    ContentBlockResponse::Text {
                        text: "[tool result]".to_string(),
                    }
                }
            })
            .collect();

        let stop_reason = api_response.stop_reason.as_deref().map(|r| match r {
            "end_turn" => StopReason::EndTurn,
            "max_tokens" => StopReason::MaxTokens,
            "tool_use" => StopReason::ToolUse,
            "stop_sequence" => StopReason::StopSequence,
            _ => StopReason::EndTurn,
        });

        Ok(CompletionResponse {
            id: api_response.id,
            model: api_response.model,
            content,
            stop_reason,
            usage: Usage {
                input_tokens: api_response.usage.input_tokens,
                output_tokens: api_response.usage.output_tokens,
                cache_creation_input_tokens: api_response
                    .usage
                    .cache_creation_input_tokens
                    .unwrap_or(0),
                cache_read_input_tokens: api_response.usage.cache_read_input_tokens.unwrap_or(0),
            },
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        let mut body = self.build_request(&request);
        body.stream = Some(true);

        let response = self
            .client
            .post(&self.base_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status().as_u16();

        if !response.status().is_success() {
            // Extract Retry-After header before consuming response body
            let retry_after = Self::extract_retry_after(response.headers());
            let body = response.text().await.unwrap_or_default();
            return Err(self.parse_error(status, &body, retry_after));
        }

        let byte_stream = response.bytes_stream();

        let event_stream = byte_stream
            .map(|result| result.map_err(|e| TedError::Api(ApiError::StreamError(e.to_string()))))
            .scan(String::new(), |buffer, result| {
                let chunk = match result {
                    Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                    Err(e) => return futures::future::ready(Some(vec![Err(e)])),
                };

                buffer.push_str(&chunk);

                let mut events = Vec::new();

                // Parse SSE events from buffer
                while let Some(pos) = buffer.find("\n\n") {
                    let event_str = buffer[..pos].to_string();
                    *buffer = buffer[pos + 2..].to_string();

                    if let Some(event) = parse_sse_event(&event_str) {
                        events.push(Ok(event));
                    }
                }

                futures::future::ready(Some(events))
            })
            .flat_map(futures::stream::iter);

        Ok(Box::pin(event_stream))
    }

    fn count_tokens(&self, text: &str, _model: &str) -> Result<u32> {
        // Simple approximation: ~4 characters per token for English
        // For accurate counts, we'd need tiktoken-rs
        Ok((text.len() as f64 / 4.0).ceil() as u32)
    }
}

/// Parse a Server-Sent Event
fn parse_sse_event(event_str: &str) -> Option<StreamEvent> {
    let mut event_type = None;
    let mut data = None;

    for line in event_str.lines() {
        if let Some(rest) = line.strip_prefix("event: ") {
            event_type = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("data: ") {
            data = Some(rest.to_string());
        }
    }

    let event_type = event_type?;
    let data = data?;

    match event_type.as_str() {
        "message_start" => {
            let parsed: serde_json::Value = serde_json::from_str(&data).ok()?;
            Some(StreamEvent::MessageStart {
                id: parsed["message"]["id"].as_str()?.to_string(),
                model: parsed["message"]["model"].as_str()?.to_string(),
            })
        }
        "content_block_start" => {
            let parsed: serde_json::Value = serde_json::from_str(&data).ok()?;
            let index = parsed["index"].as_u64()? as usize;
            let block = &parsed["content_block"];

            let content_block = match block["type"].as_str()? {
                "text" => ContentBlockResponse::Text {
                    text: block["text"].as_str().unwrap_or("").to_string(),
                },
                "tool_use" => ContentBlockResponse::ToolUse {
                    id: block["id"].as_str()?.to_string(),
                    name: block["name"].as_str()?.to_string(),
                    input: serde_json::Value::Object(serde_json::Map::new()),
                },
                _ => return None,
            };

            Some(StreamEvent::ContentBlockStart {
                index,
                content_block,
            })
        }
        "content_block_delta" => {
            let parsed: serde_json::Value = serde_json::from_str(&data).ok()?;
            let index = parsed["index"].as_u64()? as usize;
            let delta = &parsed["delta"];

            let delta = match delta["type"].as_str()? {
                "text_delta" => ContentBlockDelta::TextDelta {
                    text: delta["text"].as_str()?.to_string(),
                },
                "input_json_delta" => ContentBlockDelta::InputJsonDelta {
                    partial_json: delta["partial_json"].as_str()?.to_string(),
                },
                _ => return None,
            };

            Some(StreamEvent::ContentBlockDelta { index, delta })
        }
        "content_block_stop" => {
            let parsed: serde_json::Value = serde_json::from_str(&data).ok()?;
            let index = parsed["index"].as_u64()? as usize;
            Some(StreamEvent::ContentBlockStop { index })
        }
        "message_delta" => {
            let parsed: serde_json::Value = serde_json::from_str(&data).ok()?;
            let delta = &parsed["delta"];

            let stop_reason = delta["stop_reason"].as_str().map(|r| match r {
                "end_turn" => StopReason::EndTurn,
                "max_tokens" => StopReason::MaxTokens,
                "tool_use" => StopReason::ToolUse,
                "stop_sequence" => StopReason::StopSequence,
                _ => StopReason::EndTurn,
            });

            let usage = parsed.get("usage").map(|u| Usage {
                input_tokens: u["input_tokens"].as_u64().unwrap_or(0) as u32,
                output_tokens: u["output_tokens"].as_u64().unwrap_or(0) as u32,
                ..Default::default()
            });

            Some(StreamEvent::MessageDelta { stop_reason, usage })
        }
        "message_stop" => Some(StreamEvent::MessageStop),
        "ping" => Some(StreamEvent::Ping),
        "error" => {
            let parsed: serde_json::Value = serde_json::from_str(&data).ok()?;
            Some(StreamEvent::Error {
                error_type: parsed["error"]["type"].as_str()?.to_string(),
                message: parsed["error"]["message"].as_str()?.to_string(),
            })
        }
        _ => None,
    }
}

// Anthropic API types

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<AnthropicToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicToolChoice {
    Auto,
    Any,
    Tool { name: String },
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    model: String,
    content: Vec<AnthropicContentBlock>,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
    cache_creation_input_tokens: Option<u32>,
    cache_read_input_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    error: AnthropicErrorDetail,
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorDetail {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::message::Message;
    use crate::llm::provider::ToolInputSchema;

    #[test]
    fn test_provider_new() {
        let provider = AnthropicProvider::new("test-key");
        assert_eq!(provider.api_key, "test-key");
        assert_eq!(provider.base_url, ANTHROPIC_API_URL);
    }

    #[test]
    fn test_provider_with_base_url() {
        let provider = AnthropicProvider::with_base_url("test-key", "https://custom.api.com");
        assert_eq!(provider.api_key, "test-key");
        assert_eq!(provider.base_url, "https://custom.api.com");
    }

    #[test]
    fn test_provider_name() {
        let provider = AnthropicProvider::new("test-key");
        assert_eq!(provider.name(), "anthropic");
    }

    #[test]
    fn test_available_models() {
        let provider = AnthropicProvider::new("test-key");
        let models = provider.available_models();

        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "claude-sonnet-4-20250514"));
        assert!(models.iter().any(|m| m.id == "claude-3-5-sonnet-20241022"));
        assert!(models.iter().any(|m| m.id == "claude-3-5-haiku-20241022"));

        // Check model properties
        let sonnet = models
            .iter()
            .find(|m| m.id == "claude-sonnet-4-20250514")
            .unwrap();
        assert_eq!(sonnet.context_window, 200_000);
        assert!(sonnet.supports_tools);
        assert!(sonnet.supports_vision);
    }

    #[test]
    fn test_supports_model() {
        let provider = AnthropicProvider::new("test-key");

        assert!(provider.supports_model("claude-sonnet-4-20250514"));
        assert!(provider.supports_model("claude-3-5-sonnet-20241022"));
        assert!(provider.supports_model("claude-3-5-haiku-20241022"));
        assert!(!provider.supports_model("gpt-4"));
        assert!(!provider.supports_model("unknown-model"));
    }

    #[test]
    fn test_count_tokens() {
        let provider = AnthropicProvider::new("test-key");

        let count = provider
            .count_tokens("Hello, world!", "claude-3-5-sonnet-20241022")
            .unwrap();
        assert!(count > 0);

        // Longer text should have more tokens
        let long_text = "Hello ".repeat(100);
        let long_count = provider
            .count_tokens(&long_text, "claude-3-5-sonnet-20241022")
            .unwrap();
        assert!(long_count > count);
    }

    #[test]
    fn test_convert_simple_messages() {
        let provider = AnthropicProvider::new("test-key");
        let messages = vec![Message::user("Hello"), Message::assistant("Hi there!")];

        let converted = provider.convert_messages(&messages);

        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "assistant");
    }

    #[test]
    fn test_convert_messages_filters_system() {
        let provider = AnthropicProvider::new("test-key");
        let messages = vec![Message::system("System prompt"), Message::user("Hello")];

        let converted = provider.convert_messages(&messages);

        // System message should be filtered out
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
    }

    #[test]
    fn test_convert_messages_with_blocks() {
        let provider = AnthropicProvider::new("test-key");
        let messages = vec![Message::assistant_blocks(vec![
            ContentBlock::Text {
                text: "Let me help".to_string(),
            },
            ContentBlock::ToolUse {
                id: "tool-1".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({"path": "/test"}),
            },
        ])];

        let converted = provider.convert_messages(&messages);

        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "assistant");

        if let AnthropicContent::Blocks(blocks) = &converted[0].content {
            assert_eq!(blocks.len(), 2);
        } else {
            panic!("Expected blocks content");
        }
    }

    #[test]
    fn test_convert_tools() {
        let provider = AnthropicProvider::new("test-key");
        let tools = vec![ToolDefinition {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({"path": {"type": "string"}}),
                required: vec!["path".to_string()],
            },
        }];

        let converted = provider.convert_tools(&tools);

        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].name, "test_tool");
        assert_eq!(converted[0].description, "A test tool");
    }

    #[test]
    fn test_build_request_basic() {
        let provider = AnthropicProvider::new("test-key");
        let request =
            CompletionRequest::new("claude-3-5-sonnet-20241022", vec![Message::user("Hello")]);

        let built = provider.build_request(&request);

        assert_eq!(built.model, "claude-3-5-sonnet-20241022");
        assert!(!built.messages.is_empty());
        assert!(built.tools.is_none());
        assert_eq!(built.stream, Some(false));
    }

    #[test]
    fn test_build_request_with_tools() {
        let provider = AnthropicProvider::new("test-key");
        let tools = vec![ToolDefinition {
            name: "test".to_string(),
            description: "Test".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: vec![],
            },
        }];

        let request =
            CompletionRequest::new("claude-3-5-sonnet-20241022", vec![Message::user("Hello")])
                .with_tools(tools);

        let built = provider.build_request(&request);

        assert!(built.tools.is_some());
        assert_eq!(built.tools.unwrap().len(), 1);
    }

    #[test]
    fn test_build_request_with_system() {
        let provider = AnthropicProvider::new("test-key");
        let request =
            CompletionRequest::new("claude-3-5-sonnet-20241022", vec![Message::user("Hello")])
                .with_system("You are helpful");

        let built = provider.build_request(&request);

        assert_eq!(built.system, Some("You are helpful".to_string()));
    }

    #[test]
    fn test_parse_error_authentication() {
        let provider = AnthropicProvider::new("test-key");
        let body = r#"{"error": {"type": "authentication_error", "message": "Invalid API key"}}"#;

        let error = provider.parse_error(401, body, None);

        match error {
            TedError::Api(ApiError::AuthenticationFailed) => {}
            _ => panic!("Expected AuthenticationFailed error"),
        }
    }

    #[test]
    fn test_parse_error_rate_limit() {
        let provider = AnthropicProvider::new("test-key");
        let body = r#"{"error": {"type": "rate_limit_error", "message": "Too many requests"}}"#;

        // Test with no Retry-After header (uses default of 10 seconds)
        let error = provider.parse_error(429, body, None);
        match error {
            TedError::Api(ApiError::RateLimited(secs)) => {
                assert_eq!(secs, 10); // Default when no header
            }
            _ => panic!("Expected RateLimited error"),
        }

        // Test with Retry-After header
        let error = provider.parse_error(429, body, Some(30));
        match error {
            TedError::Api(ApiError::RateLimited(secs)) => {
                assert_eq!(secs, 30); // From header
            }
            _ => panic!("Expected RateLimited error"),
        }
    }

    #[test]
    fn test_parse_error_context_too_long() {
        let provider = AnthropicProvider::new("test-key");
        let body =
            r#"{"error": {"type": "invalid_request_error", "message": "context length exceeded"}}"#;

        let error = provider.parse_error(400, body, None);

        match error {
            TedError::Api(ApiError::ContextTooLong { .. }) => {}
            _ => panic!("Expected ContextTooLong error"),
        }
    }

    #[test]
    fn test_parse_error_invalid_request() {
        let provider = AnthropicProvider::new("test-key");
        let body = r#"{"error": {"type": "invalid_request_error", "message": "Invalid model"}}"#;

        let error = provider.parse_error(400, body, None);

        match error {
            TedError::Api(ApiError::InvalidResponse(_)) => {}
            _ => panic!("Expected InvalidResponse error"),
        }
    }

    #[test]
    fn test_parse_error_server_error() {
        let provider = AnthropicProvider::new("test-key");
        let body = r#"{"error": {"type": "server_error", "message": "Internal error"}}"#;

        let error = provider.parse_error(500, body, None);

        match error {
            TedError::Api(ApiError::ServerError { status, .. }) => {
                assert_eq!(status, 500);
            }
            _ => panic!("Expected ServerError"),
        }
    }

    #[test]
    fn test_parse_error_invalid_json() {
        let provider = AnthropicProvider::new("test-key");
        let body = "not json";

        let error = provider.parse_error(500, body, None);

        match error {
            TedError::Api(ApiError::ServerError { message, .. }) => {
                assert_eq!(message, "not json");
            }
            _ => panic!("Expected ServerError with body as message"),
        }
    }

    #[test]
    fn test_parse_sse_message_start() {
        let event = "event: message_start\ndata: {\"message\": {\"id\": \"msg_123\", \"model\": \"claude-3-5-sonnet-20241022\"}}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::MessageStart { id, model } => {
                assert_eq!(id, "msg_123");
                assert_eq!(model, "claude-3-5-sonnet-20241022");
            }
            _ => panic!("Expected MessageStart"),
        }
    }

    #[test]
    fn test_parse_sse_content_block_start_text() {
        let event = "event: content_block_start\ndata: {\"index\": 0, \"content_block\": {\"type\": \"text\", \"text\": \"\"}}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                assert_eq!(index, 0);
                match content_block {
                    ContentBlockResponse::Text { text } => assert_eq!(text, ""),
                    _ => panic!("Expected Text block"),
                }
            }
            _ => panic!("Expected ContentBlockStart"),
        }
    }

    #[test]
    fn test_parse_sse_content_block_start_tool_use() {
        let event = "event: content_block_start\ndata: {\"index\": 0, \"content_block\": {\"type\": \"tool_use\", \"id\": \"tool_1\", \"name\": \"file_read\"}}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                assert_eq!(index, 0);
                match content_block {
                    ContentBlockResponse::ToolUse { id, name, .. } => {
                        assert_eq!(id, "tool_1");
                        assert_eq!(name, "file_read");
                    }
                    _ => panic!("Expected ToolUse block"),
                }
            }
            _ => panic!("Expected ContentBlockStart"),
        }
    }

    #[test]
    fn test_parse_sse_content_block_delta_text() {
        let event = "event: content_block_delta\ndata: {\"index\": 0, \"delta\": {\"type\": \"text_delta\", \"text\": \"Hello\"}}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::ContentBlockDelta { index, delta } => {
                assert_eq!(index, 0);
                match delta {
                    ContentBlockDelta::TextDelta { text } => assert_eq!(text, "Hello"),
                    _ => panic!("Expected TextDelta"),
                }
            }
            _ => panic!("Expected ContentBlockDelta"),
        }
    }

    #[test]
    fn test_parse_sse_content_block_delta_json() {
        let event = "event: content_block_delta\ndata: {\"index\": 0, \"delta\": {\"type\": \"input_json_delta\", \"partial_json\": \"{\\\"path\\\":\"}}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::ContentBlockDelta { index, delta } => {
                assert_eq!(index, 0);
                match delta {
                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                        assert_eq!(partial_json, "{\"path\":");
                    }
                    _ => panic!("Expected InputJsonDelta"),
                }
            }
            _ => panic!("Expected ContentBlockDelta"),
        }
    }

    #[test]
    fn test_parse_sse_content_block_stop() {
        let event = "event: content_block_stop\ndata: {\"index\": 0}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::ContentBlockStop { index } => assert_eq!(index, 0),
            _ => panic!("Expected ContentBlockStop"),
        }
    }

    #[test]
    fn test_parse_sse_message_delta() {
        let event = "event: message_delta\ndata: {\"delta\": {\"stop_reason\": \"end_turn\"}, \"usage\": {\"input_tokens\": 10, \"output_tokens\": 20}}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::MessageDelta { stop_reason, usage } => {
                assert_eq!(stop_reason, Some(StopReason::EndTurn));
                assert!(usage.is_some());
            }
            _ => panic!("Expected MessageDelta"),
        }
    }

    #[test]
    fn test_parse_sse_message_delta_tool_use() {
        let event = "event: message_delta\ndata: {\"delta\": {\"stop_reason\": \"tool_use\"}}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::MessageDelta { stop_reason, .. } => {
                assert_eq!(stop_reason, Some(StopReason::ToolUse));
            }
            _ => panic!("Expected MessageDelta"),
        }
    }

    #[test]
    fn test_parse_sse_message_stop() {
        let event = "event: message_stop\ndata: {}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::MessageStop => {}
            _ => panic!("Expected MessageStop"),
        }
    }

    #[test]
    fn test_parse_sse_ping() {
        let event = "event: ping\ndata: {}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::Ping => {}
            _ => panic!("Expected Ping"),
        }
    }

    #[test]
    fn test_parse_sse_error() {
        let event = "event: error\ndata: {\"error\": {\"type\": \"api_error\", \"message\": \"Something went wrong\"}}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::Error {
                error_type,
                message,
            } => {
                assert_eq!(error_type, "api_error");
                assert_eq!(message, "Something went wrong");
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_parse_sse_unknown_event() {
        let event = "event: unknown_event\ndata: {}";

        let parsed = parse_sse_event(event);

        assert!(parsed.is_none());
    }

    #[test]
    fn test_parse_sse_missing_data() {
        let event = "event: message_start";

        let parsed = parse_sse_event(event);

        assert!(parsed.is_none());
    }

    #[test]
    fn test_parse_sse_missing_event() {
        let event = "data: {}";

        let parsed = parse_sse_event(event);

        assert!(parsed.is_none());
    }

    #[test]
    fn test_tool_choice_conversion() {
        let provider = AnthropicProvider::new("test-key");

        // Test Auto
        let request =
            CompletionRequest::new("claude-3-5-sonnet-20241022", vec![Message::user("Hi")])
                .with_tool_choice(ToolChoice::Auto);
        let built = provider.build_request(&request);
        assert!(matches!(built.tool_choice, Some(AnthropicToolChoice::Auto)));

        // Test None
        let request =
            CompletionRequest::new("claude-3-5-sonnet-20241022", vec![Message::user("Hi")])
                .with_tool_choice(ToolChoice::None);
        let built = provider.build_request(&request);
        assert!(built.tool_choice.is_none());

        // Test Required
        let request =
            CompletionRequest::new("claude-3-5-sonnet-20241022", vec![Message::user("Hi")])
                .with_tool_choice(ToolChoice::Required);
        let built = provider.build_request(&request);
        assert!(matches!(built.tool_choice, Some(AnthropicToolChoice::Any)));

        // Test Specific
        let request =
            CompletionRequest::new("claude-3-5-sonnet-20241022", vec![Message::user("Hi")])
                .with_tool_choice(ToolChoice::Specific("file_read".to_string()));
        let built = provider.build_request(&request);
        match built.tool_choice {
            Some(AnthropicToolChoice::Tool { name }) => assert_eq!(name, "file_read"),
            _ => panic!("Expected Tool choice"),
        }
    }

    #[test]
    fn test_tool_result_conversion() {
        let provider = AnthropicProvider::new("test-key");
        let messages = vec![Message::assistant_blocks(vec![ContentBlock::ToolResult {
            tool_use_id: "tool_1".to_string(),
            content: ToolResultContent::Text("file contents".to_string()),
            is_error: Some(false),
        }])];

        let converted = provider.convert_messages(&messages);

        assert_eq!(converted.len(), 1);
        if let AnthropicContent::Blocks(blocks) = &converted[0].content {
            match &blocks[0] {
                AnthropicContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    assert_eq!(tool_use_id, "tool_1");
                    assert_eq!(content, "file contents");
                    assert_eq!(*is_error, Some(false));
                }
                _ => panic!("Expected ToolResult"),
            }
        }
    }

    // ===== Additional Tests for Coverage =====

    #[test]
    fn test_parse_error_not_found() {
        let provider = AnthropicProvider::new("test-key");
        let body =
            r#"{"error": {"type": "not_found_error", "message": "Model claude-999 not found"}}"#;

        let error = provider.parse_error(404, body, None);

        // not_found_error falls through to ServerError since it's not explicitly handled
        match error {
            TedError::Api(ApiError::ServerError { status, message }) => {
                assert_eq!(status, 404);
                assert!(message.contains("not found"));
            }
            _ => panic!("Expected ServerError for 404"),
        }
    }

    #[test]
    fn test_parse_error_overloaded() {
        let provider = AnthropicProvider::new("test-key");
        let body = r#"{"error": {"type": "overloaded_error", "message": "API is overloaded"}}"#;

        let error = provider.parse_error(503, body, None);

        match error {
            TedError::Api(ApiError::ServerError { status, message }) => {
                assert_eq!(status, 503);
                assert!(message.contains("overloaded"));
            }
            _ => panic!("Expected ServerError for overloaded"),
        }
    }

    #[test]
    fn test_count_tokens_empty() {
        let provider = AnthropicProvider::new("test-key");
        let count = provider
            .count_tokens("", "claude-3-5-sonnet-20241022")
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_tokens_special_characters() {
        let provider = AnthropicProvider::new("test-key");
        let count = provider
            .count_tokens("你好世界 🌍 !@#$%", "claude-3-5-sonnet-20241022")
            .unwrap();
        assert!(count > 0);
    }

    #[test]
    fn test_build_request_with_max_tokens() {
        let provider = AnthropicProvider::new("test-key");
        let request =
            CompletionRequest::new("claude-3-5-sonnet-20241022", vec![Message::user("Hello")])
                .with_max_tokens(1000);

        let built = provider.build_request(&request);
        assert_eq!(built.max_tokens, 1000);
    }

    #[test]
    fn test_build_request_with_temperature() {
        let provider = AnthropicProvider::new("test-key");
        let request =
            CompletionRequest::new("claude-3-5-sonnet-20241022", vec![Message::user("Hello")])
                .with_temperature(0.7);

        let built = provider.build_request(&request);
        assert_eq!(built.temperature, Some(0.7));
    }

    #[test]
    fn test_parse_sse_message_delta_max_tokens() {
        let event = "event: message_delta\ndata: {\"delta\": {\"stop_reason\": \"max_tokens\"}}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::MessageDelta { stop_reason, .. } => {
                assert_eq!(stop_reason, Some(StopReason::MaxTokens));
            }
            _ => panic!("Expected MessageDelta"),
        }
    }

    #[test]
    fn test_parse_sse_message_delta_stop_sequence() {
        let event = "event: message_delta\ndata: {\"delta\": {\"stop_reason\": \"stop_sequence\"}}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::MessageDelta { stop_reason, .. } => {
                assert_eq!(stop_reason, Some(StopReason::StopSequence));
            }
            _ => panic!("Expected MessageDelta"),
        }
    }

    #[test]
    fn test_parse_sse_message_delta_unknown_stop_reason() {
        let event =
            "event: message_delta\ndata: {\"delta\": {\"stop_reason\": \"unknown_reason\"}}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::MessageDelta { stop_reason, .. } => {
                // Unknown reasons default to EndTurn
                assert_eq!(stop_reason, Some(StopReason::EndTurn));
            }
            _ => panic!("Expected MessageDelta"),
        }
    }

    #[test]
    fn test_parse_sse_content_block_start_unknown_type() {
        let event = "event: content_block_start\ndata: {\"index\": 0, \"content_block\": {\"type\": \"unknown_type\"}}";

        let parsed = parse_sse_event(event);
        assert!(parsed.is_none());
    }

    #[test]
    fn test_parse_sse_content_block_delta_unknown_type() {
        let event = "event: content_block_delta\ndata: {\"index\": 0, \"delta\": {\"type\": \"unknown_delta\"}}";

        let parsed = parse_sse_event(event);
        assert!(parsed.is_none());
    }

    #[test]
    fn test_convert_messages_empty() {
        let provider = AnthropicProvider::new("test-key");
        let messages: Vec<Message> = vec![];

        let converted = provider.convert_messages(&messages);
        assert!(converted.is_empty());
    }

    #[test]
    fn test_convert_messages_only_system() {
        let provider = AnthropicProvider::new("test-key");
        let messages = vec![Message::system("You are helpful")];

        let converted = provider.convert_messages(&messages);
        // System messages are filtered out
        assert!(converted.is_empty());
    }

    #[test]
    fn test_convert_tools_empty() {
        let provider = AnthropicProvider::new("test-key");
        let tools: Vec<ToolDefinition> = vec![];

        let converted = provider.convert_tools(&tools);
        assert!(converted.is_empty());
    }

    #[test]
    fn test_convert_tools_multiple() {
        let provider = AnthropicProvider::new("test-key");
        let tools = vec![
            ToolDefinition {
                name: "tool1".to_string(),
                description: "First tool".to_string(),
                input_schema: ToolInputSchema {
                    schema_type: "object".to_string(),
                    properties: serde_json::json!({}),
                    required: vec![],
                },
            },
            ToolDefinition {
                name: "tool2".to_string(),
                description: "Second tool".to_string(),
                input_schema: ToolInputSchema {
                    schema_type: "object".to_string(),
                    properties: serde_json::json!({}),
                    required: vec![],
                },
            },
        ];

        let converted = provider.convert_tools(&tools);
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].name, "tool1");
        assert_eq!(converted[1].name, "tool2");
    }

    #[test]
    fn test_api_constants() {
        // Verify API URL is valid
        assert!(ANTHROPIC_API_URL.starts_with("https://"));
        assert!(ANTHROPIC_API_URL.contains("anthropic"));
        // Verify version is set (format: YYYY-MM-DD)
        assert!(ANTHROPIC_VERSION.contains('-'));
    }

    #[test]
    fn test_model_info_properties() {
        let provider = AnthropicProvider::new("test-key");
        let models = provider.available_models();

        for model in &models {
            // All models should have basic properties set
            assert!(!model.id.is_empty());
            assert!(!model.display_name.is_empty());
            assert!(model.context_window > 0);
            assert!(model.max_output_tokens > 0);

            // All Anthropic models support tools and vision
            assert!(model.supports_tools);
            assert!(model.supports_vision);
        }
    }

    #[test]
    fn test_parse_sse_invalid_json() {
        let event = "event: message_start\ndata: {invalid json}";

        let parsed = parse_sse_event(event);
        assert!(parsed.is_none());
    }

    #[test]
    fn test_parse_sse_empty_string() {
        let parsed = parse_sse_event("");
        assert!(parsed.is_none());
    }

    #[test]
    fn test_parse_sse_whitespace_only() {
        let parsed = parse_sse_event("   \n\n   ");
        assert!(parsed.is_none());
    }

    #[test]
    fn test_parse_sse_partial_event() {
        let event = "event: content_block_delta\ndata: {\"index\": 0, \"delta\": {}}";
        let parsed = parse_sse_event(event);
        // Missing "type" in delta - should return None
        assert!(parsed.is_none());
    }

    #[test]
    fn test_parse_error_permission_denied() {
        let provider = AnthropicProvider::new("test-key");
        let body = r#"{"error": {"type": "permission_denied", "message": "Access denied"}}"#;

        let error = provider.parse_error(403, body, None);

        // permission_denied falls through to ServerError since it's not explicitly handled
        match error {
            TedError::Api(ApiError::ServerError { status, message }) => {
                assert_eq!(status, 403);
                assert!(message.contains("denied"));
            }
            _ => panic!("Expected ServerError for permission_denied"),
        }
    }

    #[test]
    fn test_extract_retry_after() {
        use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

        // Test with numeric retry-after
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("30"));
        assert_eq!(AnthropicProvider::extract_retry_after(&headers), Some(30));

        // Test with no retry-after header
        let empty_headers = HeaderMap::new();
        assert_eq!(AnthropicProvider::extract_retry_after(&empty_headers), None);

        // Test with non-numeric retry-after (HTTP date format - not supported)
        let mut date_headers = HeaderMap::new();
        date_headers.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"),
        );
        assert_eq!(AnthropicProvider::extract_retry_after(&date_headers), None);

        // Test with invalid header value
        let mut invalid_headers = HeaderMap::new();
        invalid_headers.insert(RETRY_AFTER, HeaderValue::from_static("not-a-number"));
        assert_eq!(
            AnthropicProvider::extract_retry_after(&invalid_headers),
            None
        );
    }

    #[test]
    fn test_build_request_extracts_system_message() {
        let provider = AnthropicProvider::new("test-key");
        let messages = vec![Message::system("Be helpful"), Message::user("Hello")];
        let request = CompletionRequest::new("claude-3-5-sonnet-20241022", messages)
            .with_system("Override system");

        let built = provider.build_request(&request);

        // with_system should override the system message in messages
        assert_eq!(built.system, Some("Override system".to_string()));
    }

    #[test]
    fn test_anthropic_request_serialization() {
        let request = AnthropicRequest {
            model: "claude-3-5-sonnet-20241022".to_string(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: AnthropicContent::Text("Hello".to_string()),
            }],
            system: None,
            max_tokens: 4096,
            temperature: None,
            tools: None,
            tool_choice: None,
            stream: Some(false),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("claude-3-5-sonnet-20241022"));
        assert!(json.contains("Hello"));
        assert!(json.contains("\"max_tokens\":4096"));
    }

    #[test]
    fn test_anthropic_content_text_serialization() {
        let content = AnthropicContent::Text("Hello, world!".to_string());
        let json = serde_json::to_string(&content).unwrap();
        assert_eq!(json, "\"Hello, world!\"");
    }

    #[test]
    fn test_anthropic_content_blocks_serialization() {
        let content = AnthropicContent::Blocks(vec![AnthropicContentBlock::Text {
            text: "Hello".to_string(),
        }]);
        let json = serde_json::to_string(&content).unwrap();
        assert!(json.contains("text"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_parse_sse_with_consecutive_lines() {
        // The parser iterates over all lines, so event and data don't need to be adjacent
        let event = "event: ping\n\ndata: {}";
        let parsed = parse_sse_event(event);
        // Since parse iterates over all lines, this should actually parse successfully
        match parsed {
            Some(StreamEvent::Ping) => {}
            other => panic!("Expected Ping, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_sse_message_delta_no_stop_reason() {
        let event = "event: message_delta\ndata: {\"delta\": {}, \"usage\": {\"input_tokens\": 10, \"output_tokens\": 20}}";

        let parsed = parse_sse_event(event).unwrap();

        match parsed {
            StreamEvent::MessageDelta { stop_reason, usage } => {
                assert!(stop_reason.is_none());
                assert!(usage.is_some());
                let usage = usage.unwrap();
                assert_eq!(usage.input_tokens, 10);
                assert_eq!(usage.output_tokens, 20);
            }
            _ => panic!("Expected MessageDelta"),
        }
    }

    #[test]
    fn test_tool_result_with_error() {
        let provider = AnthropicProvider::new("test-key");
        let messages = vec![Message::assistant_blocks(vec![ContentBlock::ToolResult {
            tool_use_id: "tool_err".to_string(),
            content: ToolResultContent::Text("Error: file not found".to_string()),
            is_error: Some(true),
        }])];

        let converted = provider.convert_messages(&messages);

        if let AnthropicContent::Blocks(blocks) = &converted[0].content {
            match &blocks[0] {
                AnthropicContentBlock::ToolResult { is_error, .. } => {
                    assert_eq!(*is_error, Some(true));
                }
                _ => panic!("Expected ToolResult"),
            }
        }
    }

    #[test]
    fn test_available_models_includes_all_expected() {
        let provider = AnthropicProvider::new("test-key");
        let models = provider.available_models();
        let model_ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();

        // Check for expected models
        assert!(model_ids.contains(&"claude-sonnet-4-20250514"));
        assert!(model_ids.contains(&"claude-3-5-sonnet-20241022"));
        assert!(model_ids.contains(&"claude-3-5-haiku-20241022"));
    }

    #[test]
    fn test_common_models_are_supported() {
        let provider = AnthropicProvider::new("test-key");
        assert!(provider.supports_model("claude-sonnet-4-20250514"));
        assert!(provider.supports_model("claude-3-5-sonnet-20241022"));
        assert!(provider.supports_model("claude-3-5-haiku-20241022"));
    }

    #[test]
    fn test_convert_message_user_role() {
        let provider = AnthropicProvider::new("test-key");
        let messages = vec![Message::user("Test user message")];

        let converted = provider.convert_messages(&messages);

        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
    }

    #[test]
    fn test_convert_message_with_mixed_content() {
        let provider = AnthropicProvider::new("test-key");
        let messages = vec![
            Message::user("Question"),
            Message::assistant("Answer"),
            Message::user("Follow-up"),
        ];

        let converted = provider.convert_messages(&messages);

        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "assistant");
        assert_eq!(converted[2].role, "user");
    }
}
