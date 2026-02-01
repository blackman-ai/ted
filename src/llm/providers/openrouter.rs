// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! OpenRouter API provider implementation
//!
//! Implements the LlmProvider trait for OpenRouter, which provides access
//! to 100+ models through a single OpenAI-compatible API.

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

const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// OpenRouter provider - access to 100+ models via single API
pub struct OpenRouterProvider {
    client: Client,
    api_key: String,
    base_url: String,
    site_url: Option<String>,
    site_name: Option<String>,
}

impl OpenRouterProvider {
    /// Create a new OpenRouter provider
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: OPENROUTER_API_URL.to_string(),
            site_url: None,
            site_name: Some("Ted AI Agent".to_string()),
        }
    }

    /// Create with a custom base URL
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: base_url.into(),
            site_url: None,
            site_name: Some("Ted AI Agent".to_string()),
        }
    }

    /// Set the site URL for OpenRouter rankings
    pub fn with_site_url(mut self, url: impl Into<String>) -> Self {
        self.site_url = Some(url.into());
        self
    }

    /// Set the site name for OpenRouter rankings
    pub fn with_site_name(mut self, name: impl Into<String>) -> Self {
        self.site_name = Some(name.into());
        self
    }

    /// Convert internal messages to OpenRouter/OpenAI format
    fn convert_messages(
        &self,
        messages: &[Message],
        system: Option<&str>,
    ) -> Vec<OpenRouterMessage> {
        let mut result = Vec::new();

        // Add system message first if provided
        if let Some(sys) = system {
            result.push(OpenRouterMessage {
                role: "system".to_string(),
                content: OpenRouterContent::Text(sys.to_string()),
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
                    result.push(OpenRouterMessage {
                        role: role.to_string(),
                        content: OpenRouterContent::Text(text.clone()),
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
                                tool_calls.push(OpenRouterToolCall {
                                    id: id.clone(),
                                    r#type: "function".to_string(),
                                    function: OpenRouterFunctionCall {
                                        name: name.clone(),
                                        arguments: serde_json::to_string(input).unwrap_or_default(),
                                    },
                                });
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
                                let result_content = if is_error.unwrap_or(false) {
                                    format!("Error: {}", content_str)
                                } else {
                                    content_str
                                };
                                tool_results.push((tool_use_id.clone(), result_content));
                            }
                        }
                    }

                    // Add assistant message with tool calls if present
                    if !tool_calls.is_empty() || !text_parts.is_empty() {
                        let content = if text_parts.is_empty() {
                            OpenRouterContent::Text(String::new())
                        } else {
                            OpenRouterContent::Text(text_parts.join("\n"))
                        };

                        result.push(OpenRouterMessage {
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

                    // Add tool results as separate messages
                    for (tool_use_id, content) in tool_results {
                        result.push(OpenRouterMessage {
                            role: "tool".to_string(),
                            content: OpenRouterContent::Text(content),
                            tool_calls: None,
                            tool_call_id: Some(tool_use_id),
                        });
                    }
                }
            }
        }

        result
    }

    /// Convert tools to OpenRouter/OpenAI format
    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<OpenRouterTool> {
        tools
            .iter()
            .map(|t| OpenRouterTool {
                r#type: "function".to_string(),
                function: OpenRouterFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: serde_json::json!({
                        "type": t.input_schema.schema_type,
                        "properties": t.input_schema.properties,
                        "required": t.input_schema.required,
                    }),
                },
            })
            .collect()
    }

    /// Build the request body
    fn build_request(&self, request: &CompletionRequest, stream: bool) -> OpenRouterRequest {
        let tool_choice = match &request.tool_choice {
            ToolChoice::Auto => Some(OpenRouterToolChoice::Auto),
            ToolChoice::None => Some(OpenRouterToolChoice::None),
            ToolChoice::Required => Some(OpenRouterToolChoice::Required),
            ToolChoice::Specific(name) => Some(OpenRouterToolChoice::Function {
                r#type: "function".to_string(),
                function: OpenRouterFunctionName { name: name.clone() },
            }),
        };

        OpenRouterRequest {
            model: request.model.clone(),
            messages: self.convert_messages(&request.messages, request.system.as_deref()),
            max_tokens: Some(request.max_tokens),
            temperature: Some(request.temperature),
            tools: if request.tools.is_empty() {
                None
            } else {
                Some(self.convert_tools(&request.tools))
            },
            tool_choice: if request.tools.is_empty() {
                None
            } else {
                tool_choice
            },
            stream: Some(stream),
        }
    }

    /// Parse an error response
    fn parse_error(&self, status: u16, body: &str) -> TedError {
        if let Ok(error_response) = serde_json::from_str::<OpenRouterError>(body) {
            let message = error_response.error.message;
            let code = error_response.error.code.as_deref().unwrap_or("");

            match code {
                "invalid_api_key" | "authentication_error" => {
                    TedError::Api(ApiError::AuthenticationFailed)
                }
                "rate_limit_exceeded" => TedError::Api(ApiError::RateLimited(60)),
                "context_length_exceeded" => {
                    // Try to parse token counts from message
                    let (current, limit) = Self::parse_token_counts(&message);
                    TedError::Api(ApiError::ContextTooLong { current, limit })
                }
                "model_not_found" => TedError::Api(ApiError::ModelNotFound(message)),
                _ => {
                    if message.contains("context")
                        || message.contains("token") && message.contains("limit")
                    {
                        let (current, limit) = Self::parse_token_counts(&message);
                        TedError::Api(ApiError::ContextTooLong { current, limit })
                    } else {
                        TedError::Api(ApiError::ServerError { status, message })
                    }
                }
            }
        } else {
            TedError::Api(ApiError::ServerError {
                status,
                message: body.to_string(),
            })
        }
    }

    /// Parse token counts from an error message
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
}

#[async_trait]
impl LlmProvider for OpenRouterProvider {
    fn name(&self) -> &str {
        "openrouter"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        // Popular models available on OpenRouter
        // Note: OpenRouter has 100+ models, we list the most useful for coding
        vec![
            // Anthropic Claude models
            ModelInfo {
                id: "anthropic/claude-sonnet-4-20250514".to_string(),
                display_name: "Claude Sonnet 4 (via OpenRouter)".to_string(),
                context_window: 200_000,
                max_output_tokens: 64_000,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
            },
            ModelInfo {
                id: "anthropic/claude-3.5-sonnet".to_string(),
                display_name: "Claude 3.5 Sonnet (via OpenRouter)".to_string(),
                context_window: 200_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
            },
            ModelInfo {
                id: "anthropic/claude-3.5-haiku".to_string(),
                display_name: "Claude 3.5 Haiku (via OpenRouter)".to_string(),
                context_window: 200_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.001,
                output_cost_per_1k: 0.005,
            },
            // OpenAI models
            ModelInfo {
                id: "openai/gpt-4o".to_string(),
                display_name: "GPT-4o (via OpenRouter)".to_string(),
                context_window: 128_000,
                max_output_tokens: 16_384,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.005,
                output_cost_per_1k: 0.015,
            },
            ModelInfo {
                id: "openai/gpt-4o-mini".to_string(),
                display_name: "GPT-4o Mini (via OpenRouter)".to_string(),
                context_window: 128_000,
                max_output_tokens: 16_384,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.00015,
                output_cost_per_1k: 0.0006,
            },
            ModelInfo {
                id: "openai/o1".to_string(),
                display_name: "OpenAI o1 (via OpenRouter)".to_string(),
                context_window: 200_000,
                max_output_tokens: 100_000,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.015,
                output_cost_per_1k: 0.060,
            },
            ModelInfo {
                id: "openai/o1-mini".to_string(),
                display_name: "OpenAI o1-mini (via OpenRouter)".to_string(),
                context_window: 128_000,
                max_output_tokens: 65_536,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.012,
            },
            // Google models
            ModelInfo {
                id: "google/gemini-2.0-flash-exp:free".to_string(),
                display_name: "Gemini 2.0 Flash (Free)".to_string(),
                context_window: 1_000_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
            },
            ModelInfo {
                id: "google/gemini-pro-1.5".to_string(),
                display_name: "Gemini Pro 1.5 (via OpenRouter)".to_string(),
                context_window: 2_000_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: true,
                input_cost_per_1k: 0.00125,
                output_cost_per_1k: 0.005,
            },
            // DeepSeek models (great for coding)
            ModelInfo {
                id: "deepseek/deepseek-chat".to_string(),
                display_name: "DeepSeek Chat (via OpenRouter)".to_string(),
                context_window: 64_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.00014,
                output_cost_per_1k: 0.00028,
            },
            ModelInfo {
                id: "deepseek/deepseek-r1".to_string(),
                display_name: "DeepSeek R1 (via OpenRouter)".to_string(),
                context_window: 64_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.00055,
                output_cost_per_1k: 0.00219,
            },
            // Meta Llama models
            ModelInfo {
                id: "meta-llama/llama-3.3-70b-instruct".to_string(),
                display_name: "Llama 3.3 70B (via OpenRouter)".to_string(),
                context_window: 128_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.0004,
                output_cost_per_1k: 0.0004,
            },
            // Mistral models
            ModelInfo {
                id: "mistralai/mistral-large-2411".to_string(),
                display_name: "Mistral Large (via OpenRouter)".to_string(),
                context_window: 128_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.002,
                output_cost_per_1k: 0.006,
            },
            ModelInfo {
                id: "mistralai/codestral-2501".to_string(),
                display_name: "Codestral (via OpenRouter)".to_string(),
                context_window: 256_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.0003,
                output_cost_per_1k: 0.0009,
            },
            // Qwen models (great for coding)
            ModelInfo {
                id: "qwen/qwen-2.5-coder-32b-instruct".to_string(),
                display_name: "Qwen 2.5 Coder 32B (via OpenRouter)".to_string(),
                context_window: 32_768,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.00018,
                output_cost_per_1k: 0.00018,
            },
        ]
    }

    fn supports_model(&self, model: &str) -> bool {
        // OpenRouter supports many models - check our curated list
        // or allow any model string (OpenRouter will validate)
        self.available_models().iter().any(|m| m.id == model) || model.contains("/")
        // OpenRouter uses provider/model format
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let body = self.build_request(&request, false);

        let mut req = self
            .client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", &self.api_key))
            .header("Content-Type", "application/json");

        // Add optional headers for OpenRouter rankings
        if let Some(ref site_url) = self.site_url {
            req = req.header("HTTP-Referer", site_url);
        }
        if let Some(ref site_name) = self.site_name {
            req = req.header("X-Title", site_name);
        }

        let response = req.json(&body).send().await?;

        let status = response.status().as_u16();

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(self.parse_error(status, &body));
        }

        let api_response: OpenRouterResponse = response.json().await?;

        // Convert response to our format
        let choice = api_response.choices.into_iter().next().ok_or_else(|| {
            TedError::Api(ApiError::InvalidResponse(
                "No choices in response".to_string(),
            ))
        })?;

        let mut content = Vec::new();

        // Add text content if present
        if let Some(text) = choice.message.content {
            if !text.is_empty() {
                content.push(ContentBlockResponse::Text { text });
            }
        }

        // Add tool calls if present
        if let Some(tool_calls) = choice.message.tool_calls {
            for tc in tool_calls {
                let input: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));
                content.push(ContentBlockResponse::ToolUse {
                    id: tc.id,
                    name: tc.function.name,
                    input,
                });
            }
        }

        let stop_reason = choice.finish_reason.as_deref().map(|r| match r {
            "stop" => StopReason::EndTurn,
            "length" => StopReason::MaxTokens,
            "tool_calls" | "function_call" => StopReason::ToolUse,
            _ => StopReason::EndTurn,
        });

        Ok(CompletionResponse {
            id: api_response.id,
            model: api_response.model,
            content,
            stop_reason,
            usage: Usage {
                input_tokens: api_response.usage.prompt_tokens,
                output_tokens: api_response.usage.completion_tokens,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        let body = self.build_request(&request, true);

        let mut req = self
            .client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", &self.api_key))
            .header("Content-Type", "application/json");

        if let Some(ref site_url) = self.site_url {
            req = req.header("HTTP-Referer", site_url);
        }
        if let Some(ref site_name) = self.site_name {
            req = req.header("X-Title", site_name);
        }

        let response = req.json(&body).send().await?;

        let status = response.status().as_u16();

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(self.parse_error(status, &body));
        }

        let byte_stream = response.bytes_stream();

        // State for stream processing
        // (buffer, message_started, current_content_index, current_tool_index, accumulated_tool_args)
        type StreamState = (
            String,
            bool,
            usize,
            Option<usize>,
            std::collections::HashMap<String, String>,
        );

        let event_stream = byte_stream
            .map(|result| result.map_err(|e| TedError::Api(ApiError::StreamError(e.to_string()))))
            .scan(
                (
                    String::new(),
                    false,
                    0usize,
                    None::<usize>,
                    std::collections::HashMap::new(),
                ),
                |state: &mut StreamState, result| {
                    let (buffer, message_started, content_idx, tool_idx, tool_args) = state;

                    let chunk = match result {
                        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                        Err(e) => return futures::future::ready(Some(vec![Err(e)])),
                    };

                    buffer.push_str(&chunk);

                    let mut events = Vec::new();

                    // Parse SSE events (data: ... lines)
                    while let Some(line_end) = buffer.find('\n') {
                        let line = buffer[..line_end].trim().to_string();
                        *buffer = buffer[line_end + 1..].to_string();

                        if line.is_empty() || line.starts_with(':') {
                            continue;
                        }

                        if let Some(data) = line.strip_prefix("data: ") {
                            if data == "[DONE]" {
                                events.push(Ok(StreamEvent::MessageStop));
                                continue;
                            }

                            if let Ok(chunk) = serde_json::from_str::<OpenRouterStreamChunk>(data) {
                                // Emit MessageStart on first chunk
                                if !*message_started {
                                    *message_started = true;
                                    events.push(Ok(StreamEvent::MessageStart {
                                        id: chunk.id.clone(),
                                        model: chunk.model.clone().unwrap_or_default(),
                                    }));
                                }

                                if let Some(choice) = chunk.choices.into_iter().next() {
                                    let delta = choice.delta;

                                    // Handle text content
                                    if let Some(text) = delta.content {
                                        if !text.is_empty() {
                                            // Start content block if needed
                                            if tool_idx.is_none() && *content_idx == 0 {
                                                events.push(Ok(StreamEvent::ContentBlockStart {
                                                    index: *content_idx,
                                                    content_block: ContentBlockResponse::Text {
                                                        text: String::new(),
                                                    },
                                                }));
                                            }

                                            events.push(Ok(StreamEvent::ContentBlockDelta {
                                                index: *content_idx,
                                                delta: ContentBlockDelta::TextDelta { text },
                                            }));
                                        }
                                    }

                                    // Handle tool calls
                                    if let Some(tool_calls) = delta.tool_calls {
                                        for tc in tool_calls {
                                            let tc_index = tc.index.unwrap_or(0);
                                            let tool_id = tc
                                                .id
                                                .clone()
                                                .unwrap_or_else(|| format!("tool_{}", tc_index));

                                            // Start new tool use block
                                            if tool_idx.is_none() || *tool_idx != Some(tc_index) {
                                                // Close previous content block if any
                                                if *content_idx > 0 || tool_idx.is_some() {
                                                    let prev_idx = tool_idx.unwrap_or(*content_idx);
                                                    events.push(Ok(
                                                        StreamEvent::ContentBlockStop {
                                                            index: prev_idx,
                                                        },
                                                    ));
                                                }

                                                *tool_idx = Some(tc_index);
                                                let block_index = *content_idx + tc_index + 1;

                                                if let Some(ref func) = tc.function {
                                                    events.push(Ok(
                                                        StreamEvent::ContentBlockStart {
                                                            index: block_index,
                                                            content_block:
                                                                ContentBlockResponse::ToolUse {
                                                                    id: tool_id.clone(),
                                                                    name: func
                                                                        .name
                                                                        .clone()
                                                                        .unwrap_or_default(),
                                                                    input:
                                                                        serde_json::Value::Object(
                                                                            serde_json::Map::new(),
                                                                        ),
                                                                },
                                                        },
                                                    ));
                                                }
                                            }

                                            // Accumulate arguments
                                            if let Some(ref func) = tc.function {
                                                if let Some(ref args) = func.arguments {
                                                    tool_args
                                                        .entry(tool_id.clone())
                                                        .or_default()
                                                        .push_str(args);

                                                    let block_index = *content_idx + tc_index + 1;
                                                    events.push(Ok(
                                                        StreamEvent::ContentBlockDelta {
                                                            index: block_index,
                                                            delta:
                                                                ContentBlockDelta::InputJsonDelta {
                                                                    partial_json: args.clone(),
                                                                },
                                                        },
                                                    ));
                                                }
                                            }
                                        }
                                    }

                                    // Handle finish reason
                                    if let Some(finish_reason) = choice.finish_reason {
                                        // Close any open blocks
                                        if let Some(ti) = *tool_idx {
                                            events.push(Ok(StreamEvent::ContentBlockStop {
                                                index: *content_idx + ti + 1,
                                            }));
                                        } else if *content_idx == 0 && *message_started {
                                            events.push(Ok(StreamEvent::ContentBlockStop {
                                                index: 0,
                                            }));
                                        }

                                        let stop_reason = match finish_reason.as_str() {
                                            "stop" => Some(StopReason::EndTurn),
                                            "length" => Some(StopReason::MaxTokens),
                                            "tool_calls" | "function_call" => {
                                                Some(StopReason::ToolUse)
                                            }
                                            _ => Some(StopReason::EndTurn),
                                        };

                                        events.push(Ok(StreamEvent::MessageDelta {
                                            stop_reason,
                                            usage: chunk.usage.map(|u| Usage {
                                                input_tokens: u.prompt_tokens,
                                                output_tokens: u.completion_tokens,
                                                cache_creation_input_tokens: 0,
                                                cache_read_input_tokens: 0,
                                            }),
                                        }));
                                    }
                                }
                            }
                        }
                    }

                    futures::future::ready(Some(events))
                },
            )
            .flat_map(futures::stream::iter);

        Ok(Box::pin(event_stream))
    }

    fn count_tokens(&self, text: &str, _model: &str) -> Result<u32> {
        // Simple approximation: ~4 characters per token for English
        Ok((text.len() as f64 / 4.0).ceil() as u32)
    }
}

// OpenRouter API types (OpenAI-compatible format)

#[derive(Debug, Serialize)]
struct OpenRouterRequest {
    model: String,
    messages: Vec<OpenRouterMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenRouterTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<OpenRouterToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
struct OpenRouterMessage {
    role: String,
    content: OpenRouterContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenRouterToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenRouterContent {
    Text(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenRouterToolCall {
    id: String,
    #[serde(rename = "type")]
    r#type: String,
    function: OpenRouterFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenRouterFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenRouterTool {
    #[serde(rename = "type")]
    r#type: String,
    function: OpenRouterFunction,
}

#[derive(Debug, Serialize)]
struct OpenRouterFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug)]
enum OpenRouterToolChoice {
    Auto,
    None,
    Required,
    Function {
        r#type: String,
        function: OpenRouterFunctionName,
    },
}

impl Serialize for OpenRouterToolChoice {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            OpenRouterToolChoice::Auto => serializer.serialize_str("auto"),
            OpenRouterToolChoice::None => serializer.serialize_str("none"),
            OpenRouterToolChoice::Required => serializer.serialize_str("required"),
            OpenRouterToolChoice::Function { r#type, function } => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", r#type)?;
                map.serialize_entry("function", function)?;
                map.end()
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct OpenRouterFunctionName {
    name: String,
}

#[derive(Debug, Deserialize)]
struct OpenRouterResponse {
    id: String,
    model: String,
    choices: Vec<OpenRouterChoice>,
    usage: OpenRouterUsage,
}

#[derive(Debug, Deserialize)]
struct OpenRouterChoice {
    message: OpenRouterResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenRouterToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenRouterError {
    error: OpenRouterErrorDetail,
}

#[derive(Debug, Deserialize)]
struct OpenRouterErrorDetail {
    message: String,
    code: Option<String>,
}

// Streaming types
#[derive(Debug, Deserialize)]
struct OpenRouterStreamChunk {
    id: String,
    model: Option<String>,
    choices: Vec<OpenRouterStreamChoice>,
    usage: Option<OpenRouterUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamChoice {
    delta: OpenRouterStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenRouterStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamToolCall {
    index: Option<usize>,
    id: Option<String>,
    function: Option<OpenRouterStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::message::Message;
    use crate::llm::provider::ToolInputSchema;

    #[test]
    fn test_provider_new() {
        let provider = OpenRouterProvider::new("test-key");
        assert_eq!(provider.api_key, "test-key");
        assert_eq!(provider.base_url, OPENROUTER_API_URL);
    }

    #[test]
    fn test_provider_with_base_url() {
        let provider = OpenRouterProvider::with_base_url("test-key", "https://custom.api.com");
        assert_eq!(provider.api_key, "test-key");
        assert_eq!(provider.base_url, "https://custom.api.com");
    }

    #[test]
    fn test_provider_with_site_info() {
        let provider = OpenRouterProvider::new("test-key")
            .with_site_url("https://example.com")
            .with_site_name("My App");
        assert_eq!(provider.site_url, Some("https://example.com".to_string()));
        assert_eq!(provider.site_name, Some("My App".to_string()));
    }

    #[test]
    fn test_provider_name() {
        let provider = OpenRouterProvider::new("test-key");
        assert_eq!(provider.name(), "openrouter");
    }

    #[test]
    fn test_available_models() {
        let provider = OpenRouterProvider::new("test-key");
        let models = provider.available_models();

        assert!(!models.is_empty());
        // Check for some expected models
        assert!(models.iter().any(|m| m.id.contains("claude")));
        assert!(models.iter().any(|m| m.id.contains("gpt-4")));
        assert!(models.iter().any(|m| m.id.contains("gemini")));
    }

    #[test]
    fn test_supports_model() {
        let provider = OpenRouterProvider::new("test-key");

        // Known models
        assert!(provider.supports_model("anthropic/claude-3.5-sonnet"));
        assert!(provider.supports_model("openai/gpt-4o"));

        // Any provider/model format should work
        assert!(provider.supports_model("some-provider/some-model"));

        // Models without / might not be supported
        assert!(!provider.supports_model("unknown-model-no-slash"));
    }

    #[test]
    fn test_count_tokens() {
        let provider = OpenRouterProvider::new("test-key");

        let count = provider.count_tokens("Hello, world!", "gpt-4").unwrap();
        assert!(count > 0);

        let long_text = "Hello ".repeat(100);
        let long_count = provider.count_tokens(&long_text, "gpt-4").unwrap();
        assert!(long_count > count);
    }

    #[test]
    fn test_convert_simple_messages() {
        let provider = OpenRouterProvider::new("test-key");
        let messages = vec![Message::user("Hello"), Message::assistant("Hi there!")];

        let converted = provider.convert_messages(&messages, None);

        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[1].role, "assistant");
    }

    #[test]
    fn test_convert_messages_with_system() {
        let provider = OpenRouterProvider::new("test-key");
        let messages = vec![Message::user("Hello")];

        let converted = provider.convert_messages(&messages, Some("You are helpful"));

        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "system");
        assert_eq!(converted[1].role, "user");
    }

    #[test]
    fn test_convert_tools() {
        let provider = OpenRouterProvider::new("test-key");
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
        assert_eq!(converted[0].function.name, "test_tool");
        assert_eq!(converted[0].r#type, "function");
    }

    #[test]
    fn test_build_request_basic() {
        let provider = OpenRouterProvider::new("test-key");
        let request = CompletionRequest::new("openai/gpt-4o", vec![Message::user("Hello")]);

        let built = provider.build_request(&request, false);

        assert_eq!(built.model, "openai/gpt-4o");
        assert!(!built.messages.is_empty());
        assert!(built.tools.is_none());
        assert_eq!(built.stream, Some(false));
    }

    #[test]
    fn test_build_request_with_tools() {
        let provider = OpenRouterProvider::new("test-key");
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
            CompletionRequest::new("openai/gpt-4o", vec![Message::user("Hello")]).with_tools(tools);

        let built = provider.build_request(&request, false);

        assert!(built.tools.is_some());
        assert_eq!(built.tools.unwrap().len(), 1);
    }

    #[test]
    fn test_parse_error_authentication() {
        let provider = OpenRouterProvider::new("test-key");
        let body = r#"{"error": {"code": "invalid_api_key", "message": "Invalid API key"}}"#;

        let error = provider.parse_error(401, body);

        match error {
            TedError::Api(ApiError::AuthenticationFailed) => {}
            _ => panic!("Expected AuthenticationFailed error"),
        }
    }

    #[test]
    fn test_parse_error_rate_limit() {
        let provider = OpenRouterProvider::new("test-key");
        let body = r#"{"error": {"code": "rate_limit_exceeded", "message": "Too many requests"}}"#;

        let error = provider.parse_error(429, body);

        match error {
            TedError::Api(ApiError::RateLimited(_)) => {}
            _ => panic!("Expected RateLimited error"),
        }
    }

    #[test]
    fn test_parse_error_model_not_found() {
        let provider = OpenRouterProvider::new("test-key");
        let body = r#"{"error": {"code": "model_not_found", "message": "Model xyz not found"}}"#;

        let error = provider.parse_error(404, body);

        match error {
            TedError::Api(ApiError::ModelNotFound(_)) => {}
            _ => panic!("Expected ModelNotFound error"),
        }
    }

    #[test]
    fn test_parse_error_context_too_long() {
        let provider = OpenRouterProvider::new("test-key");
        let body = r#"{"error": {"code": "context_length_exceeded", "message": "context too long: 150000 tokens > 128000 limit"}}"#;

        let error = provider.parse_error(400, body);

        match error {
            TedError::Api(ApiError::ContextTooLong { current, limit }) => {
                assert_eq!(current, 150000);
                assert_eq!(limit, 128000);
            }
            _ => panic!("Expected ContextTooLong error"),
        }
    }

    #[test]
    fn test_tool_choice_serialization() {
        // Test Auto
        let auto = serde_json::to_string(&OpenRouterToolChoice::Auto).unwrap();
        assert_eq!(auto, "\"auto\"");

        // Test None
        let none = serde_json::to_string(&OpenRouterToolChoice::None).unwrap();
        assert_eq!(none, "\"none\"");

        // Test Required
        let required = serde_json::to_string(&OpenRouterToolChoice::Required).unwrap();
        assert_eq!(required, "\"required\"");
    }

    #[test]
    fn test_model_info_properties() {
        let provider = OpenRouterProvider::new("test-key");
        let models = provider.available_models();

        for model in &models {
            assert!(!model.id.is_empty());
            assert!(!model.display_name.is_empty());
            assert!(model.context_window > 0);
            assert!(model.max_output_tokens > 0);
        }
    }
}
