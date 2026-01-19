// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Blackman AI provider implementation
//!
//! Implements the LlmProvider trait for Blackman AI, which provides optimized
//! cloud routing with 15-30% cost savings through prompt optimization and
//! semantic caching. OpenAI-compatible API.

use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

use crate::error::{ApiError, Result, TedError};
use crate::llm::message::{ContentBlock, Message, MessageContent, Role, ToolResultContent};
use crate::llm::provider::{
    CompletionRequest, CompletionResponse, ContentBlockDelta, ContentBlockResponse, LlmProvider,
    ModelInfo, StopReason, StreamEvent, ToolChoice, ToolDefinition, Usage,
};

const BLACKMAN_API_URL: &str = "https://app.useblackman.ai/v1/chat/completions";

/// Blackman AI provider - optimized cloud routing with cost savings
pub struct BlackmanProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl BlackmanProvider {
    /// Create a new Blackman AI provider
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: BLACKMAN_API_URL.to_string(),
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

    /// Convert internal messages to OpenAI format (Blackman AI is OpenAI-compatible)
    fn convert_messages(&self, messages: &[Message], system: Option<&str>) -> Vec<BlackmanMessage> {
        let mut result = Vec::new();

        // Add system message first if provided
        if let Some(sys) = system {
            result.push(BlackmanMessage {
                role: "system".to_string(),
                content: BlackmanContent::Text(sys.to_string()),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        for m in messages.iter().filter(|m| m.role != Role::System) {
            let role = match m.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => continue, // Already handled above
            };

            match &m.content {
                MessageContent::Text(text) => {
                    result.push(BlackmanMessage {
                        role: role.to_string(),
                        content: BlackmanContent::Text(text.clone()),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
                MessageContent::Blocks(blocks) => {
                    // Separate tool calls from text and tool results
                    let mut text_parts = Vec::new();
                    let mut tool_calls = Vec::new();
                    let mut tool_results = Vec::new();

                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => {
                                text_parts.push(text.clone());
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls.push(BlackmanToolCall {
                                    id: id.clone(),
                                    tool_type: "function".to_string(),
                                    function: BlackmanFunction {
                                        name: name.clone(),
                                        arguments: serde_json::to_string(input).unwrap_or_default(),
                                    },
                                });
                            }
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content: tool_content,
                                ..
                            } => {
                                let content_str = match tool_content {
                                    ToolResultContent::Text(t) => t.clone(),
                                    ToolResultContent::Blocks(blocks) => {
                                        // Concatenate block contents
                                        blocks.iter()
                                            .filter_map(|b| match b {
                                                crate::llm::message::ToolResultBlock::Text { text } => Some(text.clone()),
                                                _ => None,
                                            })
                                            .collect::<Vec<_>>()
                                            .join("\n")
                                    }
                                };
                                tool_results.push((tool_use_id.clone(), content_str));
                            }
                        }
                    }

                    // Add assistant message with text and tool calls
                    if !text_parts.is_empty() || !tool_calls.is_empty() {
                        let content = if text_parts.is_empty() {
                            BlackmanContent::Null
                        } else {
                            BlackmanContent::Text(text_parts.join("\n"))
                        };

                        result.push(BlackmanMessage {
                            role: role.to_string(),
                            content,
                            tool_calls: if tool_calls.is_empty() {
                                None
                            } else {
                                Some(tool_calls)
                            },
                            tool_call_id: None,
                        });
                    }

                    // Add tool result messages
                    for (tool_id, content) in tool_results {
                        result.push(BlackmanMessage {
                            role: "tool".to_string(),
                            content: BlackmanContent::Text(content),
                            tool_calls: None,
                            tool_call_id: Some(tool_id),
                        });
                    }
                }
            }
        }

        result
    }

    /// Convert tools to OpenAI function format
    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<BlackmanTool> {
        tools
            .iter()
            .map(|t| {
                // Convert ToolInputSchema to JSON Value
                let parameters = serde_json::json!({
                    "type": t.input_schema.schema_type,
                    "properties": t.input_schema.properties,
                    "required": t.input_schema.required,
                });
                BlackmanTool {
                    tool_type: "function".to_string(),
                    function: BlackmanFunctionDef {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters,
                    },
                }
            })
            .collect()
    }
}

#[async_trait]
impl LlmProvider for BlackmanProvider {
    fn name(&self) -> &str {
        "blackman"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        // Blackman AI supports all major models via routing
        vec![
            ModelInfo {
                id: "gpt-4o".to_string(),
                display_name: "GPT-4o".to_string(),
                context_window: 128000,
                max_output_tokens: 16384,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.005,
                output_cost_per_1k: 0.015,
            },
            ModelInfo {
                id: "gpt-4o-mini".to_string(),
                display_name: "GPT-4o Mini".to_string(),
                context_window: 128000,
                max_output_tokens: 16384,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.00015,
                output_cost_per_1k: 0.0006,
            },
            ModelInfo {
                id: "claude-3-7-sonnet".to_string(),
                display_name: "Claude 3.7 Sonnet".to_string(),
                context_window: 200000,
                max_output_tokens: 8192,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
            },
            ModelInfo {
                id: "claude-sonnet-4".to_string(),
                display_name: "Claude Sonnet 4".to_string(),
                context_window: 200000,
                max_output_tokens: 8192,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
            },
            ModelInfo {
                id: "deepseek-chat".to_string(),
                display_name: "DeepSeek Chat".to_string(),
                context_window: 128000,
                max_output_tokens: 8192,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.00014,
                output_cost_per_1k: 0.00028,
            },
        ]
    }

    fn supports_model(&self, model: &str) -> bool {
        self.available_models().iter().any(|m| m.id == model)
    }

    fn count_tokens(&self, text: &str, _model: &str) -> Result<u32> {
        // Rough approximation: ~4 characters per token for English text
        Ok((text.len() / 4) as u32)
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let blackman_messages = self.convert_messages(&request.messages, request.system.as_deref());

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": blackman_messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "stream": false,
        });

        if !request.tools.is_empty() {
            body["tools"] = serde_json::to_value(self.convert_tools(&request.tools))?;

            match &request.tool_choice {
                ToolChoice::Auto => {
                    body["tool_choice"] = serde_json::json!("auto");
                }
                ToolChoice::Required => {
                    body["tool_choice"] = serde_json::json!("required");
                }
                ToolChoice::None => {
                    body["tool_choice"] = serde_json::json!("none");
                }
                ToolChoice::Specific(tool_name) => {
                    body["tool_choice"] = serde_json::json!({
                        "type": "function",
                        "function": {"name": tool_name}
                    });
                }
            }
        }

        let response = self
            .client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| TedError::Api(ApiError::Network(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(TedError::Api(ApiError::ServerError {
                status,
                message: format!("Blackman AI API error: {}", body),
            }));
        }

        let blackman_response: BlackmanResponse = response
            .json()
            .await
            .map_err(|e| TedError::Api(ApiError::InvalidResponse(e.to_string())))?;

        // Convert to our format
        let choice = blackman_response
            .choices
            .first()
            .ok_or_else(|| TedError::Api(ApiError::InvalidResponse("No choices in response".to_string())))?;

        let mut content_blocks = Vec::new();

        if let Some(text) = &choice.message.content {
            if !text.is_empty() {
                content_blocks.push(ContentBlockResponse::Text {
                    text: text.clone(),
                });
            }
        }

        if let Some(tool_calls) = &choice.message.tool_calls {
            for tc in tool_calls {
                let input: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(serde_json::json!({}));

                content_blocks.push(ContentBlockResponse::ToolUse {
                    id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    input,
                });
            }
        }

        Ok(CompletionResponse {
            id: blackman_response.id.unwrap_or_else(|| format!("blackman-{}", uuid::Uuid::new_v4())),
            model: request.model.clone(),
            content: content_blocks,
            stop_reason: Some(StopReason::EndTurn),
            usage: Usage {
                input_tokens: blackman_response.usage.prompt_tokens as u32,
                output_tokens: blackman_response.usage.completion_tokens as u32,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        let blackman_messages = self.convert_messages(&request.messages, request.system.as_deref());

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": blackman_messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "stream": true,
        });

        if !request.tools.is_empty() {
            body["tools"] = serde_json::to_value(self.convert_tools(&request.tools))?;

            match &request.tool_choice {
                ToolChoice::Auto => {
                    body["tool_choice"] = serde_json::json!("auto");
                }
                ToolChoice::Required => {
                    body["tool_choice"] = serde_json::json!("required");
                }
                ToolChoice::None => {
                    body["tool_choice"] = serde_json::json!("none");
                }
                ToolChoice::Specific(tool_name) => {
                    body["tool_choice"] = serde_json::json!({
                        "type": "function",
                        "function": {"name": tool_name}
                    });
                }
            }
        }

        let response = self
            .client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| TedError::Api(ApiError::Network(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(TedError::Api(ApiError::ServerError {
                status,
                message: format!("Blackman AI API error: {}", body),
            }));
        }

        let model = request.model.clone();
        let stream = response.bytes_stream();

        // Use async_stream to properly handle state across chunks
        let event_stream = async_stream::try_stream! {
            let mut buffer = String::new();
            let mut content_block_index: usize = 0;
            let mut started = false;

            for await chunk_result in stream {
                let chunk = chunk_result.map_err(|e| TedError::Api(ApiError::Network(e.to_string())))?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if data == "[DONE]" {
                            yield StreamEvent::MessageDelta {
                                stop_reason: Some(StopReason::EndTurn),
                                usage: None,
                            };
                            yield StreamEvent::MessageStop;
                            continue;
                        }

                        if let Ok(chunk) = serde_json::from_str::<BlackmanStreamChunk>(data) {
                            // Emit MessageStart on first chunk
                            if !started {
                                started = true;
                                yield StreamEvent::MessageStart {
                                    id: chunk.id.clone().unwrap_or_else(|| format!("blackman-{}", uuid::Uuid::new_v4())),
                                    model: model.clone(),
                                };
                            }

                            if let Some(choice) = chunk.choices.first() {
                                if let Some(content) = &choice.delta.content {
                                    yield StreamEvent::ContentBlockDelta {
                                        index: content_block_index,
                                        delta: ContentBlockDelta::TextDelta {
                                            text: content.clone(),
                                        },
                                    };
                                }

                                if let Some(tool_calls) = &choice.delta.tool_calls {
                                    for tc in tool_calls {
                                        if let Some(func) = &tc.function {
                                            if let Some(name) = &func.name {
                                                content_block_index += 1;
                                                yield StreamEvent::ContentBlockStart {
                                                    index: content_block_index,
                                                    content_block: ContentBlockResponse::ToolUse {
                                                        id: tc.id.as_ref().unwrap_or(&String::new()).clone(),
                                                        name: name.clone(),
                                                        input: serde_json::json!({}),
                                                    },
                                                };
                                            }
                                            if let Some(args) = &func.arguments {
                                                yield StreamEvent::ContentBlockDelta {
                                                    index: content_block_index,
                                                    delta: ContentBlockDelta::InputJsonDelta {
                                                        partial_json: args.clone(),
                                                    },
                                                };
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };

        Ok(Box::pin(event_stream))
    }
}

// Blackman AI API types (OpenAI-compatible)

#[derive(Debug, Serialize)]
struct BlackmanMessage {
    role: String,
    #[serde(skip_serializing_if = "is_null_content")]
    content: BlackmanContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<BlackmanToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum BlackmanContent {
    Text(String),
    Null,
}

fn is_null_content(content: &BlackmanContent) -> bool {
    matches!(content, BlackmanContent::Null)
}

#[derive(Debug, Serialize)]
struct BlackmanToolCall {
    id: String,
    #[serde(rename = "type")]
    tool_type: String,
    function: BlackmanFunction,
}

#[derive(Debug, Serialize)]
struct BlackmanFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct BlackmanTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: BlackmanFunctionDef,
}

#[derive(Debug, Serialize)]
struct BlackmanFunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct BlackmanResponse {
    id: Option<String>,
    choices: Vec<BlackmanChoice>,
    usage: BlackmanUsage,
}

#[derive(Debug, Deserialize)]
struct BlackmanChoice {
    message: BlackmanResponseMessage,
}

#[derive(Debug, Deserialize)]
struct BlackmanResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<BlackmanResponseToolCall>>,
}

#[derive(Debug, Deserialize)]
struct BlackmanResponseToolCall {
    id: String,
    function: BlackmanResponseFunction,
}

#[derive(Debug, Deserialize)]
struct BlackmanResponseFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct BlackmanUsage {
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
}

#[derive(Debug, Deserialize)]
struct BlackmanStreamChunk {
    id: Option<String>,
    choices: Vec<BlackmanStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct BlackmanStreamChoice {
    delta: BlackmanDelta,
}

#[derive(Debug, Deserialize)]
struct BlackmanDelta {
    content: Option<String>,
    tool_calls: Option<Vec<BlackmanStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct BlackmanStreamToolCall {
    id: Option<String>,
    function: Option<BlackmanStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct BlackmanStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = BlackmanProvider::new("test-key");
        assert_eq!(provider.name(), "blackman");
        assert!(provider.supports_streaming());
        assert!(provider.supports_tools());
    }

    #[test]
    fn test_custom_base_url() {
        let provider = BlackmanProvider::with_base_url("test-key", "https://custom.api.com/v1/chat/completions");
        assert_eq!(provider.base_url, "https://custom.api.com/v1/chat/completions");
    }
}
