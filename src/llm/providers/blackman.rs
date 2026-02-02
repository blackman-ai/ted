// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Blackman AI provider implementation
//!
//! Implements the LlmProvider trait for Blackman AI, which provides optimized
//! cloud routing with 15-30% cost savings through prompt optimization and
//! semantic caching. OpenAI-compatible API.
//!
//! ## Action Routing
//!
//! This provider supports intelligent action routing via the `X-Blackman-Action` header.
//! When tools are included in a request, the provider automatically detects the action
//! type and sends it to Blackman for optimal model routing:
//!
//! - `agent_file_search` - glob/grep operations
//! - `agent_file_read` - reading file contents
//! - `agent_code_edit` - writing/editing code files
//! - `agent_bash_command` - shell command execution
//! - `agent_planning` - architecture/planning tasks
//! - `chat` - simple conversation without tools

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

/// Default Blackman API base URL (without endpoint path)
const BLACKMAN_API_BASE_URL: &str = "https://app.useblackman.ai";
/// The completions endpoint path
const COMPLETIONS_ENDPOINT: &str = "/v1/completions";

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
            base_url: BLACKMAN_API_BASE_URL.to_string(),
        }
    }

    /// Create with a custom base URL
    ///
    /// The base_url can be either:
    /// - Just the domain (e.g., "https://app.useblackman.ai") - /v1/completions will be appended
    /// - Full URL with path (e.g., "https://app.useblackman.ai/v1/completions") - used as-is
    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        let base = base_url.into();
        // Normalize: remove trailing slash
        let base = base.trim_end_matches('/').to_string();
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: base,
        }
    }

    /// Get the full API endpoint URL
    fn api_url(&self) -> String {
        // If base_url already contains /v1/, use it as-is
        if self.base_url.contains("/v1/") {
            self.base_url.clone()
        } else {
            format!("{}{}", self.base_url, COMPLETIONS_ENDPOINT)
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
                                        blocks
                                            .iter()
                                            .filter_map(|b| match b {
                                                crate::llm::message::ToolResultBlock::Text {
                                                    text,
                                                } => Some(text.clone()),
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

    /// Detect action type from tools for Blackman routing optimization.
    ///
    /// This maps Ted's tools to Blackman's action types for intelligent routing:
    /// - File search (glob, grep) → agent_file_search (mini tier)
    /// - File read → agent_file_read (mini tier)
    /// - File write/edit → agent_code_edit (core tier)
    /// - Shell → agent_bash_command (core tier)
    /// - No tools → chat (mini tier)
    ///
    /// The action type helps Blackman route requests to the most cost-effective
    /// model tier while maintaining quality for each task type.
    fn detect_action_type(&self, tools: &[ToolDefinition], system: Option<&str>) -> &'static str {
        // Check for planning keywords in system prompt (highest tier)
        if let Some(sys) = system {
            let sys_lower = sys.to_lowercase();
            if sys_lower.contains("plan")
                || sys_lower.contains("architect")
                || sys_lower.contains("design")
                || sys_lower.contains("strategy")
            {
                return "agent_planning";
            }
        }

        // No tools = simple chat
        if tools.is_empty() {
            return "chat";
        }

        // Analyze tools to determine the primary action
        // Priority: code_edit > bash > file_search > file_read > tool_use
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();

        // Code modification tools get highest priority (core tier)
        if tool_names
            .iter()
            .any(|n| *n == "file_write" || *n == "file_edit")
        {
            return "agent_code_edit";
        }

        // Shell commands (core tier)
        if tool_names.contains(&"shell") {
            return "agent_bash_command";
        }

        // Search operations (mini tier)
        if tool_names.iter().any(|n| *n == "glob" || *n == "grep") {
            return "agent_file_search";
        }

        // File reading (mini tier)
        if tool_names.contains(&"file_read") {
            return "agent_file_read";
        }

        // Generic tool use (core tier)
        "agent_tool_use"
    }

    /// Detect the upstream provider from the model name.
    /// Blackman API requires a provider field to route to the correct backend.
    fn detect_provider(&self, model: &str) -> &'static str {
        let model_lower = model.to_lowercase();

        if model_lower.starts_with("gpt-")
            || model_lower.starts_with("o1")
            || model_lower.starts_with("o3")
        {
            "OpenAI"
        } else if model_lower.starts_with("claude") {
            "Anthropic"
        } else if model_lower.starts_with("gemini") {
            "Gemini"
        } else if model_lower.starts_with("mistral") || model_lower.starts_with("mixtral") {
            "Mistral"
        } else if model_lower.starts_with("llama") || model_lower.contains("groq") {
            "Groq"
        } else if model_lower.starts_with("deepseek") {
            // DeepSeek is typically accessed via OpenAI-compatible API
            "OpenAI"
        } else {
            // Default to Anthropic for unknown models
            "Anthropic"
        }
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

        // Detect action type for Blackman routing optimization
        let action_type = self.detect_action_type(&request.tools, request.system.as_deref());

        // Detect the upstream provider for routing
        let provider = self.detect_provider(&request.model);

        let mut body = serde_json::json!({
            "provider": provider,
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
            .post(self.api_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("X-Blackman-Action", action_type)
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
        let choice = blackman_response.choices.first().ok_or_else(|| {
            TedError::Api(ApiError::InvalidResponse(
                "No choices in response".to_string(),
            ))
        })?;

        let mut content_blocks = Vec::new();

        if let Some(text) = &choice.message.content {
            if !text.is_empty() {
                content_blocks.push(ContentBlockResponse::Text { text: text.clone() });
            }
        }

        if let Some(tool_calls) = &choice.message.tool_calls {
            for tc in tool_calls {
                let input: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));

                content_blocks.push(ContentBlockResponse::ToolUse {
                    id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    input,
                });
            }
        }

        Ok(CompletionResponse {
            id: blackman_response
                .id
                .unwrap_or_else(|| format!("blackman-{}", uuid::Uuid::new_v4())),
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

        // Detect action type for Blackman routing optimization
        let action_type = self.detect_action_type(&request.tools, request.system.as_deref());

        // Detect the upstream provider for routing
        let provider = self.detect_provider(&request.model);

        let mut body = serde_json::json!({
            "provider": provider,
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
            .post(self.api_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("X-Blackman-Action", action_type)
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

                    if let Some(data) = line.strip_prefix("data: ") {
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
    #[allow(dead_code)]
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
    use crate::llm::provider::ToolInputSchema;

    fn make_tool(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: format!("Test tool: {}", name),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: vec![],
            },
        }
    }

    #[test]
    fn test_provider_creation() {
        let provider = BlackmanProvider::new("test-key");
        assert_eq!(provider.name(), "blackman");
        assert_eq!(
            provider.api_url(),
            "https://app.useblackman.ai/v1/completions"
        );
    }

    #[test]
    fn test_custom_base_url() {
        // Test with full URL (including path)
        let provider =
            BlackmanProvider::with_base_url("test-key", "https://custom.api.com/v1/completions");
        assert_eq!(provider.base_url, "https://custom.api.com/v1/completions");
        assert_eq!(provider.api_url(), "https://custom.api.com/v1/completions");

        // Test with just domain - should append /v1/completions
        let provider = BlackmanProvider::with_base_url("test-key", "https://custom.api.com");
        assert_eq!(provider.base_url, "https://custom.api.com");
        assert_eq!(provider.api_url(), "https://custom.api.com/v1/completions");

        // Test with trailing slash - should normalize
        let provider = BlackmanProvider::with_base_url("test-key", "https://custom.api.com/");
        assert_eq!(provider.base_url, "https://custom.api.com");
        assert_eq!(provider.api_url(), "https://custom.api.com/v1/completions");
    }

    #[test]
    fn test_action_type_chat() {
        let provider = BlackmanProvider::new("test-key");
        let action = provider.detect_action_type(&[], None);
        assert_eq!(action, "chat");
    }

    #[test]
    fn test_action_type_file_search() {
        let provider = BlackmanProvider::new("test-key");

        // glob tool
        let tools = vec![make_tool("glob")];
        assert_eq!(
            provider.detect_action_type(&tools, None),
            "agent_file_search"
        );

        // grep tool
        let tools = vec![make_tool("grep")];
        assert_eq!(
            provider.detect_action_type(&tools, None),
            "agent_file_search"
        );
    }

    #[test]
    fn test_action_type_file_read() {
        let provider = BlackmanProvider::new("test-key");
        let tools = vec![make_tool("file_read")];
        assert_eq!(provider.detect_action_type(&tools, None), "agent_file_read");
    }

    #[test]
    fn test_action_type_code_edit() {
        let provider = BlackmanProvider::new("test-key");

        // file_write tool
        let tools = vec![make_tool("file_write")];
        assert_eq!(provider.detect_action_type(&tools, None), "agent_code_edit");

        // file_edit tool
        let tools = vec![make_tool("file_edit")];
        assert_eq!(provider.detect_action_type(&tools, None), "agent_code_edit");
    }

    #[test]
    fn test_action_type_bash() {
        let provider = BlackmanProvider::new("test-key");
        let tools = vec![make_tool("shell")];
        assert_eq!(
            provider.detect_action_type(&tools, None),
            "agent_bash_command"
        );
    }

    #[test]
    fn test_action_type_planning() {
        let provider = BlackmanProvider::new("test-key");
        let tools = vec![make_tool("file_read")];

        // Planning keywords in system prompt
        assert_eq!(
            provider.detect_action_type(
                &tools,
                Some("You are a software architect. Plan the implementation.")
            ),
            "agent_planning"
        );
        assert_eq!(
            provider.detect_action_type(&tools, Some("Design the system architecture.")),
            "agent_planning"
        );
        assert_eq!(
            provider.detect_action_type(&tools, Some("Create a strategy for the migration.")),
            "agent_planning"
        );
    }

    #[test]
    fn test_action_type_priority() {
        let provider = BlackmanProvider::new("test-key");

        // When multiple tools present, code_edit takes priority
        let tools = vec![
            make_tool("file_read"),
            make_tool("glob"),
            make_tool("file_write"),
        ];
        assert_eq!(provider.detect_action_type(&tools, None), "agent_code_edit");

        // shell has higher priority than file_search
        let tools = vec![make_tool("glob"), make_tool("shell")];
        assert_eq!(
            provider.detect_action_type(&tools, None),
            "agent_bash_command"
        );

        // But planning in system prompt overrides tool-based detection
        let tools = vec![make_tool("file_write")];
        assert_eq!(
            provider.detect_action_type(&tools, Some("Plan the architecture")),
            "agent_planning"
        );
    }

    #[test]
    fn test_action_type_tool_use_fallback() {
        let provider = BlackmanProvider::new("test-key");
        // Unknown tool falls back to tool_use
        let tools = vec![make_tool("some_custom_tool")];
        assert_eq!(provider.detect_action_type(&tools, None), "agent_tool_use");
    }

    #[test]
    fn test_detect_provider() {
        let provider = BlackmanProvider::new("test-key");

        // OpenAI models
        assert_eq!(provider.detect_provider("gpt-4o"), "OpenAI");
        assert_eq!(provider.detect_provider("gpt-4o-mini"), "OpenAI");
        assert_eq!(provider.detect_provider("o1-preview"), "OpenAI");

        // Anthropic models
        assert_eq!(
            provider.detect_provider("claude-sonnet-4-20250514"),
            "Anthropic"
        );
        assert_eq!(provider.detect_provider("claude-3-7-sonnet"), "Anthropic");

        // Gemini models
        assert_eq!(provider.detect_provider("gemini-pro"), "Gemini");

        // Mistral models
        assert_eq!(provider.detect_provider("mistral-large"), "Mistral");
        assert_eq!(provider.detect_provider("mixtral-8x7b"), "Mistral");

        // Groq models
        assert_eq!(provider.detect_provider("llama-3-70b"), "Groq");

        // DeepSeek (via OpenAI-compatible API)
        assert_eq!(provider.detect_provider("deepseek-chat"), "OpenAI");

        // Unknown defaults to Anthropic
        assert_eq!(provider.detect_provider("unknown-model"), "Anthropic");
    }

    // ===== Additional tests for convert_messages =====

    #[test]
    fn test_convert_messages_user_text() {
        let provider = BlackmanProvider::new("test-key");
        let messages = vec![Message::user("Hello")];
        let converted = provider.convert_messages(&messages, None);

        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
        match &converted[0].content {
            BlackmanContent::Text(text) => assert_eq!(text, "Hello"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_convert_messages_with_system() {
        let provider = BlackmanProvider::new("test-key");
        let messages = vec![Message::user("Hello")];
        let converted = provider.convert_messages(&messages, Some("You are helpful"));

        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "system");
        match &converted[0].content {
            BlackmanContent::Text(text) => assert_eq!(text, "You are helpful"),
            _ => panic!("Expected text content"),
        }
    }

    #[test]
    fn test_convert_messages_assistant() {
        let provider = BlackmanProvider::new("test-key");
        let messages = vec![Message::assistant("I can help")];
        let converted = provider.convert_messages(&messages, None);

        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "assistant");
    }

    #[test]
    fn test_convert_messages_skips_system_role() {
        let provider = BlackmanProvider::new("test-key");
        let messages = vec![Message::system("System message"), Message::user("Hello")];
        let converted = provider.convert_messages(&messages, None);

        // System messages in the message list are skipped (use system parameter instead)
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
    }

    // ===== Additional tests for convert_tools =====

    #[test]
    fn test_convert_tools_empty() {
        let provider = BlackmanProvider::new("test-key");
        let tools: Vec<ToolDefinition> = vec![];
        let converted = provider.convert_tools(&tools);

        assert!(converted.is_empty());
    }

    #[test]
    fn test_convert_tools_single() {
        let provider = BlackmanProvider::new("test-key");
        let tools = vec![make_tool("test_tool")];
        let converted = provider.convert_tools(&tools);

        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].tool_type, "function");
        assert_eq!(converted[0].function.name, "test_tool");
    }

    #[test]
    fn test_convert_tools_multiple() {
        let provider = BlackmanProvider::new("test-key");
        let tools = vec![make_tool("tool1"), make_tool("tool2"), make_tool("tool3")];
        let converted = provider.convert_tools(&tools);

        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0].function.name, "tool1");
        assert_eq!(converted[1].function.name, "tool2");
        assert_eq!(converted[2].function.name, "tool3");
    }

    #[test]
    fn test_convert_tools_preserves_description() {
        let provider = BlackmanProvider::new("test-key");
        let tools = vec![make_tool("file_read")];
        let converted = provider.convert_tools(&tools);

        assert_eq!(converted[0].function.description, "Test tool: file_read");
    }

    // ===== Tests for available_models =====

    #[test]
    fn test_available_models_not_empty() {
        let provider = BlackmanProvider::new("test-key");
        let models = provider.available_models();
        assert!(!models.is_empty());
    }

    #[test]
    fn test_available_models_contains_gpt4o() {
        let provider = BlackmanProvider::new("test-key");
        let models = provider.available_models();
        assert!(models.iter().any(|m| m.id == "gpt-4o"));
    }

    #[test]
    fn test_available_models_contains_claude() {
        let provider = BlackmanProvider::new("test-key");
        let models = provider.available_models();
        assert!(models.iter().any(|m| m.id.contains("claude")));
    }

    #[test]
    fn test_available_models_have_tool_support() {
        let provider = BlackmanProvider::new("test-key");
        let models = provider.available_models();
        // All listed models should support tools
        for model in &models {
            assert!(
                model.supports_tools,
                "Model {} should support tools",
                model.id
            );
        }
    }

    // ===== Tests for supports_model =====

    #[test]
    fn test_supports_model_true() {
        let provider = BlackmanProvider::new("test-key");
        assert!(provider.supports_model("gpt-4o"));
        assert!(provider.supports_model("gpt-4o-mini"));
    }

    #[test]
    fn test_supports_model_false() {
        let provider = BlackmanProvider::new("test-key");
        assert!(!provider.supports_model("nonexistent-model"));
        assert!(!provider.supports_model("gpt-5-super")); // Hypothetical
    }

    // ===== Tests for count_tokens =====

    #[test]
    fn test_count_tokens_empty() {
        let provider = BlackmanProvider::new("test-key");
        let count = provider.count_tokens("", "gpt-4o").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_count_tokens_short() {
        let provider = BlackmanProvider::new("test-key");
        // "hello" is 5 chars, 5/4 = 1 token
        let count = provider.count_tokens("hello", "gpt-4o").unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_count_tokens_longer() {
        let provider = BlackmanProvider::new("test-key");
        // 100 chars / 4 = 25 tokens
        let text = "x".repeat(100);
        let count = provider.count_tokens(&text, "gpt-4o").unwrap();
        assert_eq!(count, 25);
    }

    // ===== Tests for api_url edge cases =====

    #[test]
    fn test_api_url_with_v1_in_middle() {
        let provider = BlackmanProvider::with_base_url("key", "https://api.com/v1/custom");
        assert_eq!(provider.api_url(), "https://api.com/v1/custom");
    }

    #[test]
    fn test_api_url_without_v1() {
        let provider = BlackmanProvider::with_base_url("key", "https://api.com/custom");
        assert_eq!(provider.api_url(), "https://api.com/custom/v1/completions");
    }

    // ===== Tests for detect_provider edge cases =====

    #[test]
    fn test_detect_provider_o3_model() {
        let provider = BlackmanProvider::new("test-key");
        assert_eq!(provider.detect_provider("o3-preview"), "OpenAI");
        assert_eq!(provider.detect_provider("o3-mini"), "OpenAI");
    }

    #[test]
    fn test_detect_provider_groq_in_name() {
        let provider = BlackmanProvider::new("test-key");
        assert_eq!(provider.detect_provider("groq-hosted-llama"), "Groq");
    }

    #[test]
    fn test_detect_provider_case_insensitive() {
        let provider = BlackmanProvider::new("test-key");
        assert_eq!(provider.detect_provider("GPT-4O"), "OpenAI");
        assert_eq!(provider.detect_provider("CLAUDE-3-OPUS"), "Anthropic");
        assert_eq!(provider.detect_provider("Gemini-Pro"), "Gemini");
    }

    // ===== Tests for name() =====

    #[test]
    fn test_name() {
        let provider = BlackmanProvider::new("test-key");
        assert_eq!(provider.name(), "blackman");
    }
}
