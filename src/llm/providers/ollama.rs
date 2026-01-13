// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Ollama local model provider implementation
//!
//! Implements the LlmProvider trait for Ollama local models.
//! Supports streaming responses and tool calling via Ollama's /api/chat endpoint.

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

use crate::error::{ApiError, Result, TedError};
use crate::llm::message::{ContentBlock, Message, MessageContent, Role, ToolResultContent};
use crate::llm::provider::{
    CompletionRequest, CompletionResponse, ContentBlockDelta, ContentBlockResponse, LlmProvider,
    ModelInfo, StopReason, StreamEvent, ToolDefinition, Usage,
};

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Regex pattern to match JSON tool calls in text content
/// Matches patterns like: {"name": "tool_name", "arguments": {...}}
use regex::Regex;
use std::sync::LazyLock;

static JSON_TOOL_CALL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\{\s*"name"\s*:\s*"([^"]+)"\s*,\s*"arguments"\s*:\s*(\{[^}]*\})\s*\}"#).unwrap()
});

/// Try to parse a JSON tool call from text content
/// Returns (tool_name, arguments) if found, None otherwise
fn try_parse_json_tool_call(text: &str) -> Option<(String, serde_json::Value)> {
    // First try the simple regex pattern
    if let Some(caps) = JSON_TOOL_CALL_PATTERN.captures(text) {
        let name = caps.get(1)?.as_str().to_string();
        let args_str = caps.get(2)?.as_str();
        if let Ok(args) = serde_json::from_str(args_str) {
            return Some((name, args));
        }
    }

    // Try to parse as a more complex JSON structure
    // Some models wrap the tool call in other text
    let trimmed = text.trim();

    // Find the start of a JSON object that looks like a tool call
    if let Some(start) = trimmed.find('{') {
        // Try to find matching closing brace
        let json_part = &trimmed[start..];
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_part) {
            if let Some(name) = parsed.get("name").and_then(|n| n.as_str()) {
                if let Some(args) = parsed.get("arguments") {
                    return Some((name.to_string(), args.clone()));
                }
            }
        }

        // Try parsing with more lenient approach (find closing brace)
        let mut depth = 0;
        let mut end_idx = 0;
        for (i, c) in json_part.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end_idx = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }

        if end_idx > 0 {
            let json_str = &json_part[..end_idx];
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(name) = parsed.get("name").and_then(|n| n.as_str()) {
                    if let Some(args) = parsed.get("arguments") {
                        return Some((name.to_string(), args.clone()));
                    }
                }
            }
        }
    }

    None
}

/// Ollama local model provider
pub struct OllamaProvider {
    client: Client,
    base_url: String,
}

impl OllamaProvider {
    /// Create a new Ollama provider with default base URL (http://localhost:11434)
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: DEFAULT_OLLAMA_URL.to_string(),
        }
    }

    /// Create with a custom base URL
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
        }
    }

    /// Check if Ollama is running and reachable
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/api/tags", self.base_url);
        match self.client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(e) => {
                if e.is_connect() {
                    Err(TedError::Api(ApiError::Network(
                        "Ollama is not running. Start the Ollama app or run 'ollama serve'"
                            .to_string(),
                    )))
                } else {
                    Err(TedError::Http(e))
                }
            }
        }
    }

    /// List available models from Ollama
    pub async fn list_local_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self.client.get(&url).send().await.map_err(|e| {
            if e.is_connect() {
                TedError::Api(ApiError::Network(
                    "Ollama is not running. Start the Ollama app or run 'ollama serve'".to_string(),
                ))
            } else {
                TedError::Http(e)
            }
        })?;

        if !response.status().is_success() {
            return Err(TedError::Api(ApiError::ServerError {
                status: response.status().as_u16(),
                message: "Failed to list models".to_string(),
            }));
        }

        let body: OllamaTagsResponse = response.json().await?;
        Ok(body.models.into_iter().map(|m| m.name).collect())
    }

    /// Convert internal messages to Ollama format
    fn convert_messages(&self, messages: &[Message]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| {
                let role = match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::System => "system", // Should be filtered, but handle anyway
                };

                match &m.content {
                    MessageContent::Text(text) => OllamaMessage {
                        role: role.to_string(),
                        content: text.clone(),
                        tool_calls: None,
                    },
                    MessageContent::Blocks(blocks) => {
                        // Collect text content
                        let mut text_parts: Vec<String> = Vec::new();
                        let mut tool_calls: Vec<OllamaToolCall> = Vec::new();

                        for block in blocks {
                            match block {
                                ContentBlock::Text { text } => {
                                    text_parts.push(text.clone());
                                }
                                ContentBlock::ToolUse { id, name, input } => {
                                    tool_calls.push(OllamaToolCall {
                                        function: OllamaFunctionCall {
                                            name: name.clone(),
                                            arguments: input.clone(),
                                        },
                                    });
                                    // Store the ID in the text for reference (Ollama doesn't have IDs)
                                    let _ = id; // Ollama doesn't use tool call IDs like Anthropic
                                }
                                ContentBlock::ToolResult {
                                    tool_use_id: _,
                                    content,
                                    is_error: _,
                                } => {
                                    // For tool results, format as text content
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
                                    text_parts.push(content_str);
                                }
                            }
                        }

                        OllamaMessage {
                            role: role.to_string(),
                            content: text_parts.join("\n"),
                            tool_calls: if tool_calls.is_empty() {
                                None
                            } else {
                                Some(tool_calls)
                            },
                        }
                    }
                }
            })
            .collect()
    }

    /// Convert tools to Ollama format
    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<OllamaTool> {
        tools
            .iter()
            .map(|t| OllamaTool {
                tool_type: "function".to_string(),
                function: OllamaFunction {
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
    fn build_request(&self, request: &CompletionRequest, stream: bool) -> OllamaRequest {
        let tools = if request.tools.is_empty() {
            None
        } else {
            Some(self.convert_tools(&request.tools))
        };

        // Enhance system prompt with tool usage guidance for Ollama models
        // Structure: PERSONA first (most important), then tools, then persona reminder
        let enhanced_system = if !request.tools.is_empty() {
            let tool_guidance = r#"
TOOLS: You have access to: glob (find files), file_read (read files), grep (search), shell (run commands), file_write (create/overwrite files), file_edit (modify existing files).
To use a tool, output ONLY: {"name": "tool_name", "arguments": {...}}

CRITICAL RULES:
- BEFORE answering questions about code, USE glob and file_read to explore first. Never give generic advice - base answers on actual code you've read.
- When asked to make code changes, you MUST use file_write or file_edit tools - do NOT just output code snippets or say you cannot modify files. You CAN and MUST modify files directly.
- To create a new file: {"name": "file_write", "arguments": {"path": "/full/path/to/file.rs", "content": "file content here"}}
- To edit an existing file: {"name": "file_edit", "arguments": {"path": "/full/path/to/file.rs", "old_text": "text to replace", "new_text": "replacement text"}}"#;

            match &request.system {
                Some(sys) => {
                    // Extract persona/personality from the cap prompt
                    // Put persona FIRST (critical for smaller models), tools in middle, persona reminder at end
                    #[cfg(debug_assertions)]
                    eprintln!("[DEBUG] Cap system prompt length: {} chars", sys.len());

                    // Format: Full persona -> Brief tools -> Persona reminder
                    Some(format!(
                        "IMPORTANT - YOUR PERSONALITY AND ROLE:\n{}\n\n{}\n\nREMEMBER: Stay in character as described above. Your personality should come through in ALL your responses.",
                        sys,
                        tool_guidance
                    ))
                }
                None => Some(tool_guidance.trim_start().to_string()),
            }
        } else {
            // No tools - just use the system prompt with persona emphasis
            match &request.system {
                Some(sys) => Some(format!(
                    "IMPORTANT - YOUR PERSONALITY AND ROLE:\n{}\n\nREMEMBER: Stay in character as described above. Your personality should come through in ALL your responses.",
                    sys
                )),
                None => None,
            }
        };

        OllamaRequest {
            model: request.model.clone(),
            messages: self.convert_messages(&request.messages),
            system: enhanced_system,
            stream,
            options: Some(OllamaOptions {
                temperature: Some(request.temperature),
                num_predict: Some(request.max_tokens as i64),
            }),
            tools,
        }
    }

    /// Parse an error response
    fn parse_error(&self, status: u16, body: &str) -> TedError {
        if let Ok(error_response) = serde_json::from_str::<OllamaError>(body) {
            let message = error_response.error;
            if message.contains("model") && message.contains("not found") {
                TedError::Api(ApiError::ModelNotFound(message))
            } else {
                TedError::Api(ApiError::ServerError { status, message })
            }
        } else {
            TedError::Api(ApiError::ServerError {
                status,
                message: body.to_string(),
            })
        }
    }

    /// Parse a streaming chunk from Ollama's NDJSON response
    fn parse_stream_chunk(line: &str) -> Option<OllamaStreamResponse> {
        if line.trim().is_empty() {
            return None;
        }
        serde_json::from_str(line).ok()
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        // Return common Ollama models - actual availability depends on what's pulled
        vec![
            ModelInfo {
                id: "qwen2.5-coder:14b".to_string(),
                display_name: "Qwen 2.5 Coder 14B".to_string(),
                context_window: 32_768,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.0, // Local = free
                output_cost_per_1k: 0.0,
            },
            ModelInfo {
                id: "qwen2.5-coder:7b".to_string(),
                display_name: "Qwen 2.5 Coder 7B".to_string(),
                context_window: 32_768,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
            },
            ModelInfo {
                id: "llama3.2:latest".to_string(),
                display_name: "Llama 3.2".to_string(),
                context_window: 128_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
            },
            ModelInfo {
                id: "codellama:latest".to_string(),
                display_name: "Code Llama".to_string(),
                context_window: 16_384,
                max_output_tokens: 4_096,
                supports_tools: false,
                supports_vision: false,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
            },
            ModelInfo {
                id: "deepseek-coder-v2:latest".to_string(),
                display_name: "DeepSeek Coder V2".to_string(),
                context_window: 128_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
            },
            ModelInfo {
                id: "mistral:latest".to_string(),
                display_name: "Mistral".to_string(),
                context_window: 32_768,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
            },
        ]
    }

    fn supports_model(&self, model: &str) -> bool {
        // Ollama supports any model that's been pulled
        // We'll be permissive here - actual availability is checked at runtime
        !model.is_empty()
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let url = format!("{}/api/chat", self.base_url);
        let body = self.build_request(&request, false);

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    TedError::Api(ApiError::Network(
                        "Ollama is not running. Start the Ollama app or run 'ollama serve'"
                            .to_string(),
                    ))
                } else {
                    TedError::Http(e)
                }
            })?;

        let status = response.status().as_u16();

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(self.parse_error(status, &body));
        }

        let api_response: OllamaResponse = response.json().await?;

        // Convert response to our format
        let mut content: Vec<ContentBlockResponse> = Vec::new();

        // Add text content
        if !api_response.message.content.is_empty() {
            content.push(ContentBlockResponse::Text {
                text: api_response.message.content,
            });
        }

        // Add tool calls if present
        if let Some(tool_calls) = api_response.message.tool_calls {
            for (idx, tc) in tool_calls.into_iter().enumerate() {
                content.push(ContentBlockResponse::ToolUse {
                    id: format!("tool_{}", idx),
                    name: tc.function.name,
                    input: tc.function.arguments,
                });
            }
        }

        // Determine stop reason
        let stop_reason = if content
            .iter()
            .any(|c| matches!(c, ContentBlockResponse::ToolUse { .. }))
        {
            Some(StopReason::ToolUse)
        } else if api_response.done {
            Some(StopReason::EndTurn)
        } else {
            None
        };

        Ok(CompletionResponse {
            id: format!("ollama-{}", uuid::Uuid::new_v4()),
            model: request.model,
            content,
            stop_reason,
            usage: Usage {
                input_tokens: api_response.prompt_eval_count.unwrap_or(0) as u32,
                output_tokens: api_response.eval_count.unwrap_or(0) as u32,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        let url = format!("{}/api/chat", self.base_url);
        let body = self.build_request(&request, true);
        let model = request.model.clone();

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    TedError::Api(ApiError::Network(
                        "Ollama is not running. Start the Ollama app or run 'ollama serve'"
                            .to_string(),
                    ))
                } else {
                    TedError::Http(e)
                }
            })?;

        let status = response.status().as_u16();

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(self.parse_error(status, &body));
        }

        let byte_stream = response.bytes_stream();

        // Generate a message ID for this stream
        let message_id = format!("ollama-{}", uuid::Uuid::new_v4());

        // Track state across the stream
        // State: (buffer, message_started, content_block_idx, msg_id, model_name, accumulated_text, has_native_tool_calls, text_block_started)
        let event_stream = byte_stream
            .map(|result| result.map_err(|e| TedError::Api(ApiError::StreamError(e.to_string()))))
            .scan(
                (
                    String::new(),
                    false,
                    0usize,
                    message_id,
                    model,
                    String::new(),
                    false,
                    false, // text_block_started
                ),
                |state, result| {
                    let (
                        buffer,
                        message_started,
                        content_block_idx,
                        msg_id,
                        model_name,
                        accumulated_text,
                        has_native_tool_calls,
                        text_block_started,
                    ) = state;

                    let chunk = match result {
                        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                        Err(e) => return futures::future::ready(Some(vec![Err(e)])),
                    };

                    buffer.push_str(&chunk);

                    let mut events = Vec::new();

                    // Parse NDJSON - each line is a complete JSON object
                    while let Some(pos) = buffer.find('\n') {
                        let line = buffer[..pos].to_string();
                        *buffer = buffer[pos + 1..].to_string();

                        if let Some(chunk_response) = Self::parse_stream_chunk(&line) {
                            // Emit MessageStart if this is the first chunk
                            if !*message_started {
                                *message_started = true;
                                events.push(Ok(StreamEvent::MessageStart {
                                    id: msg_id.clone(),
                                    model: model_name.clone(),
                                }));
                            }

                            // Handle native tool calls from API
                            if let Some(tool_calls) = chunk_response.message.tool_calls {
                                *has_native_tool_calls = true;
                                for tc in tool_calls {
                                    // Start a new tool use block
                                    events.push(Ok(StreamEvent::ContentBlockStart {
                                        index: *content_block_idx,
                                        content_block: ContentBlockResponse::ToolUse {
                                            id: format!("tool_{}", *content_block_idx),
                                            name: tc.function.name.clone(),
                                            input: serde_json::Value::Object(
                                                serde_json::Map::new(),
                                            ),
                                        },
                                    }));

                                    // Send the arguments as a delta
                                    let args_str = serde_json::to_string(&tc.function.arguments)
                                        .unwrap_or_default();
                                    events.push(Ok(StreamEvent::ContentBlockDelta {
                                        index: *content_block_idx,
                                        delta: ContentBlockDelta::InputJsonDelta {
                                            partial_json: args_str,
                                        },
                                    }));

                                    events.push(Ok(StreamEvent::ContentBlockStop {
                                        index: *content_block_idx,
                                    }));

                                    *content_block_idx += 1;
                                }
                            }

                            // Handle text content - accumulate for potential JSON tool call parsing
                            if !chunk_response.message.content.is_empty() {
                                accumulated_text.push_str(&chunk_response.message.content);

                                // Start text block only once (not for every chunk!)
                                if !*text_block_started {
                                    *text_block_started = true;
                                    events.push(Ok(StreamEvent::ContentBlockStart {
                                        index: *content_block_idx,
                                        content_block: ContentBlockResponse::Text {
                                            text: String::new(),
                                        },
                                    }));
                                }

                                events.push(Ok(StreamEvent::ContentBlockDelta {
                                    index: *content_block_idx,
                                    delta: ContentBlockDelta::TextDelta {
                                        text: chunk_response.message.content,
                                    },
                                }));
                            }

                            // Handle done
                            if chunk_response.done {
                                // Check if the text contains a JSON tool call that wasn't handled natively
                                // This handles models that output tool calls as JSON text
                                let mut detected_tool_call = false;

                                if !*has_native_tool_calls && !accumulated_text.is_empty() {
                                    if let Some((tool_name, tool_args)) =
                                        try_parse_json_tool_call(accumulated_text)
                                    {
                                        detected_tool_call = true;

                                        // Close the text block first
                                        events.push(Ok(StreamEvent::ContentBlockStop {
                                            index: *content_block_idx,
                                        }));
                                        *content_block_idx += 1;

                                        // Emit tool use events
                                        events.push(Ok(StreamEvent::ContentBlockStart {
                                            index: *content_block_idx,
                                            content_block: ContentBlockResponse::ToolUse {
                                                id: format!("tool_{}", *content_block_idx),
                                                name: tool_name,
                                                input: serde_json::Value::Object(
                                                    serde_json::Map::new(),
                                                ),
                                            },
                                        }));

                                        let args_str =
                                            serde_json::to_string(&tool_args).unwrap_or_default();
                                        events.push(Ok(StreamEvent::ContentBlockDelta {
                                            index: *content_block_idx,
                                            delta: ContentBlockDelta::InputJsonDelta {
                                                partial_json: args_str,
                                            },
                                        }));

                                        events.push(Ok(StreamEvent::ContentBlockStop {
                                            index: *content_block_idx,
                                        }));
                                    }
                                }

                                if !detected_tool_call {
                                    // Close any open content block
                                    events.push(Ok(StreamEvent::ContentBlockStop {
                                        index: *content_block_idx,
                                    }));
                                }

                                // Determine stop reason
                                let has_tools = detected_tool_call
                                    || *has_native_tool_calls
                                    || events.iter().any(|e| {
                                        matches!(
                                            e,
                                            Ok(StreamEvent::ContentBlockStart {
                                                content_block: ContentBlockResponse::ToolUse { .. },
                                                ..
                                            })
                                        )
                                    });

                                let stop_reason = if has_tools {
                                    Some(StopReason::ToolUse)
                                } else {
                                    Some(StopReason::EndTurn)
                                };

                                events.push(Ok(StreamEvent::MessageDelta {
                                    stop_reason,
                                    usage: Some(Usage {
                                        input_tokens: chunk_response.prompt_eval_count.unwrap_or(0)
                                            as u32,
                                        output_tokens: chunk_response.eval_count.unwrap_or(0)
                                            as u32,
                                        cache_creation_input_tokens: 0,
                                        cache_read_input_tokens: 0,
                                    }),
                                }));

                                events.push(Ok(StreamEvent::MessageStop));
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
        // Simple approximation: ~4 characters per token
        // Ollama doesn't provide a tokenization API
        Ok((text.len() as f64 / 4.0).ceil() as u32)
    }
}

// Ollama API types

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaTool>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<i64>,
}

#[derive(Debug, Serialize)]
struct OllamaTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OllamaFunction,
}

#[derive(Debug, Serialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaToolCall {
    function: OllamaFunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaFunctionCall {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
    done: bool,
    #[serde(default)]
    prompt_eval_count: Option<i64>,
    #[serde(default)]
    eval_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamResponse {
    message: OllamaMessage,
    done: bool,
    #[serde(default)]
    prompt_eval_count: Option<i64>,
    #[serde(default)]
    eval_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct OllamaError {
    error: String,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::message::Message;
    use crate::llm::provider::ToolInputSchema;

    // ===== JSON Tool Call Parsing Tests =====

    #[test]
    fn test_try_parse_json_tool_call_simple() {
        let text = r#"{"name": "glob", "arguments": {"pattern": "**/*"}}"#;
        let result = try_parse_json_tool_call(text);
        assert!(result.is_some());
        let (name, args) = result.unwrap();
        assert_eq!(name, "glob");
        assert_eq!(args["pattern"], "**/*");
    }

    #[test]
    fn test_try_parse_json_tool_call_with_whitespace() {
        let text = r#"
        {
            "name": "file_read",
            "arguments": {
                "path": "/src/main.rs"
            }
        }
        "#;
        let result = try_parse_json_tool_call(text);
        assert!(result.is_some());
        let (name, args) = result.unwrap();
        assert_eq!(name, "file_read");
        assert_eq!(args["path"], "/src/main.rs");
    }

    #[test]
    fn test_try_parse_json_tool_call_with_surrounding_text() {
        let text = r#"I'll use the glob tool to find files:
{"name": "glob", "arguments": {"pattern": "*.rs"}}
"#;
        let result = try_parse_json_tool_call(text);
        assert!(result.is_some());
        let (name, args) = result.unwrap();
        assert_eq!(name, "glob");
        assert_eq!(args["pattern"], "*.rs");
    }

    #[test]
    fn test_try_parse_json_tool_call_no_match() {
        let text = "This is just regular text without a tool call.";
        let result = try_parse_json_tool_call(text);
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_json_tool_call_invalid_json() {
        let text = r#"{"name": "glob", "arguments": {not valid json}"#;
        let result = try_parse_json_tool_call(text);
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_json_tool_call_missing_name() {
        let text = r#"{"arguments": {"pattern": "*.rs"}}"#;
        let result = try_parse_json_tool_call(text);
        assert!(result.is_none());
    }

    #[test]
    fn test_try_parse_json_tool_call_missing_arguments() {
        let text = r#"{"name": "glob"}"#;
        let result = try_parse_json_tool_call(text);
        assert!(result.is_none());
    }

    // ===== Provider Tests =====

    #[test]
    fn test_provider_new() {
        let provider = OllamaProvider::new();
        assert_eq!(provider.base_url, DEFAULT_OLLAMA_URL);
    }

    #[test]
    fn test_provider_with_base_url() {
        let provider = OllamaProvider::with_base_url("http://custom:8080");
        assert_eq!(provider.base_url, "http://custom:8080");
    }

    #[test]
    fn test_provider_name() {
        let provider = OllamaProvider::new();
        assert_eq!(provider.name(), "ollama");
    }

    #[test]
    fn test_available_models() {
        let provider = OllamaProvider::new();
        let models = provider.available_models();

        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id.contains("qwen")));
        assert!(models.iter().any(|m| m.id.contains("llama")));

        // All models should be free (local)
        for model in &models {
            assert_eq!(model.input_cost_per_1k, 0.0);
            assert_eq!(model.output_cost_per_1k, 0.0);
        }
    }

    #[test]
    fn test_supports_model() {
        let provider = OllamaProvider::new();

        // Should support any non-empty model name
        assert!(provider.supports_model("qwen2.5-coder:14b"));
        assert!(provider.supports_model("llama3.2:latest"));
        assert!(provider.supports_model("custom-model:tag"));
        assert!(!provider.supports_model(""));
    }

    #[test]
    fn test_count_tokens() {
        let provider = OllamaProvider::new();

        let count = provider.count_tokens("Hello, world!", "any-model").unwrap();
        assert!(count > 0);

        let long_text = "Hello ".repeat(100);
        let long_count = provider.count_tokens(&long_text, "any-model").unwrap();
        assert!(long_count > count);
    }

    #[test]
    fn test_convert_simple_messages() {
        let provider = OllamaProvider::new();
        let messages = vec![Message::user("Hello"), Message::assistant("Hi there!")];

        let converted = provider.convert_messages(&messages);

        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "user");
        assert_eq!(converted[0].content, "Hello");
        assert_eq!(converted[1].role, "assistant");
        assert_eq!(converted[1].content, "Hi there!");
    }

    #[test]
    fn test_convert_messages_filters_system() {
        let provider = OllamaProvider::new();
        let messages = vec![Message::system("System prompt"), Message::user("Hello")];

        let converted = provider.convert_messages(&messages);

        // System message should be filtered out
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
    }

    #[test]
    fn test_convert_tools() {
        let provider = OllamaProvider::new();
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
        assert_eq!(converted[0].tool_type, "function");
        assert_eq!(converted[0].function.name, "test_tool");
        assert_eq!(converted[0].function.description, "A test tool");
    }

    #[test]
    fn test_build_request_basic() {
        let provider = OllamaProvider::new();
        let request = CompletionRequest::new("qwen2.5-coder:14b", vec![Message::user("Hello")]);

        let built = provider.build_request(&request, false);

        assert_eq!(built.model, "qwen2.5-coder:14b");
        assert!(!built.messages.is_empty());
        assert!(!built.stream);
        assert!(built.tools.is_none());
    }

    #[test]
    fn test_build_request_with_stream() {
        let provider = OllamaProvider::new();
        let request = CompletionRequest::new("qwen2.5-coder:14b", vec![Message::user("Hello")]);

        let built = provider.build_request(&request, true);

        assert!(built.stream);
    }

    #[test]
    fn test_build_request_with_tools() {
        let provider = OllamaProvider::new();
        let tools = vec![ToolDefinition {
            name: "test".to_string(),
            description: "Test".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: vec![],
            },
        }];

        let request = CompletionRequest::new("qwen2.5-coder:14b", vec![Message::user("Hello")])
            .with_tools(tools);

        let built = provider.build_request(&request, false);

        assert!(built.tools.is_some());
        assert_eq!(built.tools.unwrap().len(), 1);
    }

    #[test]
    fn test_build_request_with_system() {
        let provider = OllamaProvider::new();
        let request = CompletionRequest::new("qwen2.5-coder:14b", vec![Message::user("Hello")])
            .with_system("You are helpful");

        let built = provider.build_request(&request, false);

        // System prompt is wrapped with persona emphasis (no tools = simpler format)
        assert!(built.system.is_some());
        let system = built.system.unwrap();
        assert!(system.contains("You are helpful"));
        assert!(system.contains("IMPORTANT"));
        assert!(system.contains("Stay in character"));
    }

    #[test]
    fn test_parse_stream_chunk_valid() {
        let json = r#"{"message":{"role":"assistant","content":"Hello"},"done":false}"#;
        let chunk = OllamaProvider::parse_stream_chunk(json);

        assert!(chunk.is_some());
        let chunk = chunk.unwrap();
        assert_eq!(chunk.message.content, "Hello");
        assert!(!chunk.done);
    }

    #[test]
    fn test_parse_stream_chunk_done() {
        let json = r#"{"message":{"role":"assistant","content":""},"done":true,"eval_count":42}"#;
        let chunk = OllamaProvider::parse_stream_chunk(json);

        assert!(chunk.is_some());
        let chunk = chunk.unwrap();
        assert!(chunk.done);
        assert_eq!(chunk.eval_count, Some(42));
    }

    #[test]
    fn test_parse_stream_chunk_empty() {
        let chunk = OllamaProvider::parse_stream_chunk("");
        assert!(chunk.is_none());

        let chunk = OllamaProvider::parse_stream_chunk("   ");
        assert!(chunk.is_none());
    }

    #[test]
    fn test_parse_stream_chunk_invalid_json() {
        let chunk = OllamaProvider::parse_stream_chunk("{invalid}");
        assert!(chunk.is_none());
    }

    #[test]
    fn test_parse_error_model_not_found() {
        let provider = OllamaProvider::new();
        let body = r#"{"error": "model 'nonexistent' not found"}"#;

        let error = provider.parse_error(404, body);

        match error {
            TedError::Api(ApiError::ModelNotFound(msg)) => {
                assert!(msg.contains("not found"));
            }
            _ => panic!("Expected ModelNotFound error"),
        }
    }

    #[test]
    fn test_parse_error_generic() {
        let provider = OllamaProvider::new();
        let body = r#"{"error": "something went wrong"}"#;

        let error = provider.parse_error(500, body);

        match error {
            TedError::Api(ApiError::ServerError { status, message }) => {
                assert_eq!(status, 500);
                assert!(message.contains("something went wrong"));
            }
            _ => panic!("Expected ServerError"),
        }
    }

    #[test]
    fn test_parse_error_invalid_json() {
        let provider = OllamaProvider::new();
        let body = "not json";

        let error = provider.parse_error(500, body);

        match error {
            TedError::Api(ApiError::ServerError { message, .. }) => {
                assert_eq!(message, "not json");
            }
            _ => panic!("Expected ServerError with body as message"),
        }
    }

    #[test]
    fn test_ollama_request_serialization() {
        let request = OllamaRequest {
            model: "qwen2.5-coder:14b".to_string(),
            messages: vec![OllamaMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
                tool_calls: None,
            }],
            system: None,
            stream: false,
            options: Some(OllamaOptions {
                temperature: Some(0.7),
                num_predict: Some(1000),
            }),
            tools: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("qwen2.5-coder:14b"));
        assert!(json.contains("Hello"));
        assert!(json.contains("0.7"));
    }

    #[test]
    fn test_convert_messages_empty() {
        let provider = OllamaProvider::new();
        let messages: Vec<Message> = vec![];

        let converted = provider.convert_messages(&messages);
        assert!(converted.is_empty());
    }

    #[test]
    fn test_convert_messages_only_system() {
        let provider = OllamaProvider::new();
        let messages = vec![Message::system("You are helpful")];

        let converted = provider.convert_messages(&messages);
        // System messages are filtered out
        assert!(converted.is_empty());
    }

    #[test]
    fn test_convert_tools_empty() {
        let provider = OllamaProvider::new();
        let tools: Vec<ToolDefinition> = vec![];

        let converted = provider.convert_tools(&tools);
        assert!(converted.is_empty());
    }

    #[test]
    fn test_convert_tools_multiple() {
        let provider = OllamaProvider::new();
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
        assert_eq!(converted[0].function.name, "tool1");
        assert_eq!(converted[1].function.name, "tool2");
    }

    #[test]
    fn test_default_trait() {
        let provider = OllamaProvider::default();
        assert_eq!(provider.base_url, DEFAULT_OLLAMA_URL);
    }

    #[test]
    fn test_model_info_properties() {
        let provider = OllamaProvider::new();
        let models = provider.available_models();

        for model in &models {
            assert!(!model.id.is_empty());
            assert!(!model.display_name.is_empty());
            assert!(model.context_window > 0);
            assert!(model.max_output_tokens > 0);
        }
    }
}
