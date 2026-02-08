// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Local LLM provider using llama-server subprocess
//!
//! Downloads and manages a llama-server binary, starts it as a subprocess,
//! and communicates via the OpenAI-compatible HTTP API. This makes local
//! model inference opaque to the user.

pub mod server;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::{ApiError, Result, TedError};
use crate::llm::message::{ContentBlock, Message, MessageContent, Role, ToolResultContent};
use crate::llm::provider::{
    CompletionRequest, CompletionResponse, ContentBlockDelta, ContentBlockResponse, LlmProvider,
    ModelInfo, StopReason, StreamEvent, ToolChoice, ToolDefinition, Usage,
};

use server::LlamaServer;

/// Local LLM provider using llama-server subprocess
pub struct LocalProvider {
    server: Arc<Mutex<Option<LlamaServer>>>,
    client: Client,
    binary_path: PathBuf,
    model_path: PathBuf,
    model_name: String,
    port: u16,
    gpu_layers: Option<i32>,
    ctx_size: Option<u32>,
}

impl LocalProvider {
    pub fn new(
        binary_path: PathBuf,
        model_path: PathBuf,
        model_name: String,
        port: u16,
        gpu_layers: Option<i32>,
        ctx_size: Option<u32>,
    ) -> Self {
        Self {
            server: Arc::new(Mutex::new(None)),
            client: Client::new(),
            binary_path,
            model_path,
            model_name,
            port,
            gpu_layers,
            ctx_size,
        }
    }

    /// Ensure the llama-server subprocess is running
    async fn ensure_server(&self) -> Result<String> {
        let mut guard = self.server.lock().await;

        // Check if server is already running
        if let Some(ref server) = *guard {
            if server.is_running() {
                return Ok(server.base_url());
            }
        }

        // Start a new server
        let server = LlamaServer::new(
            self.binary_path.clone(),
            self.model_path.clone(),
            Some(self.port),
            self.gpu_layers,
            self.ctx_size,
        );

        server.start().await?;
        let url = server.base_url();
        *guard = Some(server);

        Ok(url)
    }

    /// Convert internal messages to OpenAI format
    fn convert_messages(&self, messages: &[Message], system: Option<&str>) -> Vec<OaiMessage> {
        let mut result = Vec::new();

        if let Some(sys) = system {
            result.push(OaiMessage {
                role: "system".to_string(),
                content: OaiContent::Text(sys.to_string()),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        for m in messages.iter().filter(|m| m.role != Role::System) {
            let role = match m.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => continue,
            };

            match &m.content {
                MessageContent::Text(text) => {
                    result.push(OaiMessage {
                        role: role.to_string(),
                        content: OaiContent::Text(text.clone()),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
                MessageContent::Blocks(blocks) => {
                    let mut text_parts = Vec::new();
                    let mut tool_calls = Vec::new();
                    let mut tool_results = Vec::new();

                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => {
                                text_parts.push(text.clone());
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls.push(OaiToolCall {
                                    id: id.clone(),
                                    r#type: "function".to_string(),
                                    function: OaiFunctionCall {
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

                    if !tool_calls.is_empty() || !text_parts.is_empty() {
                        let content = if text_parts.is_empty() {
                            OaiContent::Text(String::new())
                        } else {
                            OaiContent::Text(text_parts.join("\n"))
                        };

                        result.push(OaiMessage {
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

                    for (tool_use_id, content) in tool_results {
                        result.push(OaiMessage {
                            role: "tool".to_string(),
                            content: OaiContent::Text(content),
                            tool_calls: None,
                            tool_call_id: Some(tool_use_id),
                        });
                    }
                }
            }
        }

        result
    }

    /// Convert tools to OpenAI format
    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<OaiTool> {
        tools
            .iter()
            .map(|t| OaiTool {
                r#type: "function".to_string(),
                function: OaiFunction {
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
    fn build_request(&self, request: &CompletionRequest, stream: bool) -> OaiRequest {
        let tool_choice = match &request.tool_choice {
            ToolChoice::Auto => Some(OaiToolChoice::Auto),
            ToolChoice::None => Some(OaiToolChoice::None),
            ToolChoice::Required => Some(OaiToolChoice::Required),
            ToolChoice::Specific(name) => Some(OaiToolChoice::Function {
                r#type: "function".to_string(),
                function: OaiFunctionName { name: name.clone() },
            }),
        };

        OaiRequest {
            model: self.model_name.clone(),
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
}

#[async_trait]
impl LlmProvider for LocalProvider {
    fn name(&self) -> &str {
        "local"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![ModelInfo {
            id: self.model_name.clone(),
            display_name: format!("Local: {}", self.model_name),
            context_window: self.ctx_size.unwrap_or(4096),
            max_output_tokens: self.ctx_size.unwrap_or(4096),
            supports_tools: true,
            supports_vision: false,
            input_cost_per_1k: 0.0,
            output_cost_per_1k: 0.0,
        }]
    }

    fn supports_model(&self, _model: &str) -> bool {
        true // Local server handles whatever model is loaded
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let base_url = self.ensure_server().await?;
        let url = format!("{}/v1/chat/completions", base_url);
        let body = self.build_request(&request, false);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                TedError::Api(ApiError::Network(format!(
                    "Failed to connect to local llama-server: {}",
                    e
                )))
            })?;

        let status = response.status().as_u16();

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(TedError::Api(ApiError::ServerError {
                status,
                message: format!("Local llama-server error: {}", body),
            }));
        }

        let api_response: OaiResponse = response.json().await.map_err(|e| {
            TedError::Api(ApiError::InvalidResponse(format!(
                "Failed to parse local server response: {}",
                e
            )))
        })?;

        let choice = api_response.choices.into_iter().next().ok_or_else(|| {
            TedError::Api(ApiError::InvalidResponse(
                "No choices in response".to_string(),
            ))
        })?;

        let mut content = Vec::new();

        if let Some(text) = choice.message.content {
            if !text.is_empty() {
                content.push(ContentBlockResponse::Text { text });
            }
        }

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
        let base_url = self.ensure_server().await?;
        let url = format!("{}/v1/chat/completions", base_url);
        let body = self.build_request(&request, true);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                TedError::Api(ApiError::Network(format!(
                    "Failed to connect to local llama-server: {}",
                    e
                )))
            })?;

        let status = response.status().as_u16();

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(TedError::Api(ApiError::ServerError {
                status,
                message: format!("Local llama-server error: {}", body),
            }));
        }

        let byte_stream = response.bytes_stream();

        // Stream processing state
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

                            if let Ok(chunk) = serde_json::from_str::<OaiStreamChunk>(data) {
                                if !*message_started {
                                    *message_started = true;
                                    events.push(Ok(StreamEvent::MessageStart {
                                        id: chunk.id.clone(),
                                        model: chunk.model.clone().unwrap_or_default(),
                                    }));
                                }

                                if let Some(choice) = chunk.choices.into_iter().next() {
                                    let delta = choice.delta;

                                    if let Some(text) = delta.content {
                                        if !text.is_empty() {
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

                                    if let Some(tool_calls) = delta.tool_calls {
                                        for tc in tool_calls {
                                            let tc_index = tc.index.unwrap_or(0);
                                            let tool_id = tc
                                                .id
                                                .clone()
                                                .unwrap_or_else(|| format!("tool_{}", tc_index));

                                            if tool_idx.is_none() || *tool_idx != Some(tc_index) {
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

                                    if let Some(finish_reason) = choice.finish_reason {
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
        // Approximate: ~4 characters per token
        Ok((text.len() as f64 / 4.0).ceil() as u32)
    }
}

// OpenAI-compatible API types (used by llama-server)

#[derive(Debug, Serialize)]
struct OaiRequest {
    model: String,
    messages: Vec<OaiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OaiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<OaiToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
struct OaiMessage {
    role: String,
    content: OaiContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OaiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OaiContent {
    Text(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct OaiToolCall {
    id: String,
    #[serde(rename = "type")]
    r#type: String,
    function: OaiFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct OaiFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OaiTool {
    #[serde(rename = "type")]
    r#type: String,
    function: OaiFunction,
}

#[derive(Debug, Serialize)]
struct OaiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug)]
enum OaiToolChoice {
    Auto,
    None,
    Required,
    Function {
        r#type: String,
        function: OaiFunctionName,
    },
}

impl Serialize for OaiToolChoice {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            OaiToolChoice::Auto => serializer.serialize_str("auto"),
            OaiToolChoice::None => serializer.serialize_str("none"),
            OaiToolChoice::Required => serializer.serialize_str("required"),
            OaiToolChoice::Function { r#type, function } => {
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
struct OaiFunctionName {
    name: String,
}

#[derive(Debug, Deserialize)]
struct OaiResponse {
    id: String,
    model: String,
    choices: Vec<OaiChoice>,
    usage: OaiUsage,
}

#[derive(Debug, Deserialize)]
struct OaiChoice {
    message: OaiResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OaiResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OaiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OaiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

// Streaming types

#[derive(Debug, Deserialize)]
struct OaiStreamChunk {
    id: String,
    model: Option<String>,
    choices: Vec<OaiStreamChoice>,
    usage: Option<OaiUsage>,
}

#[derive(Debug, Deserialize)]
struct OaiStreamChoice {
    delta: OaiStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OaiStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OaiStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OaiStreamToolCall {
    index: Option<usize>,
    id: Option<String>,
    function: Option<OaiStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct OaiStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::message::Message;

    #[test]
    fn test_local_provider_name() {
        let provider = LocalProvider::new(
            PathBuf::from("/usr/bin/llama-server"),
            PathBuf::from("/models/test.gguf"),
            "test-model".to_string(),
            8847,
            None,
            None,
        );
        assert_eq!(provider.name(), "local");
    }

    #[test]
    fn test_local_provider_available_models() {
        let provider = LocalProvider::new(
            PathBuf::from("/usr/bin/llama-server"),
            PathBuf::from("/models/test.gguf"),
            "qwen2.5-coder".to_string(),
            8847,
            None,
            Some(8192),
        );
        let models = provider.available_models();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "qwen2.5-coder");
        assert_eq!(models[0].input_cost_per_1k, 0.0);
    }

    #[test]
    fn test_local_provider_supports_any_model() {
        let provider = LocalProvider::new(
            PathBuf::from("/usr/bin/llama-server"),
            PathBuf::from("/models/test.gguf"),
            "test".to_string(),
            8847,
            None,
            None,
        );
        assert!(provider.supports_model("anything"));
    }

    #[test]
    fn test_convert_messages_simple() {
        let provider = LocalProvider::new(
            PathBuf::from("/usr/bin/llama-server"),
            PathBuf::from("/models/test.gguf"),
            "test".to_string(),
            8847,
            None,
            None,
        );

        let messages = vec![Message::user("Hello")];
        let converted = provider.convert_messages(&messages, Some("You are helpful"));

        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].role, "system");
        assert_eq!(converted[1].role, "user");
    }

    #[test]
    fn test_build_request_no_tools() {
        let provider = LocalProvider::new(
            PathBuf::from("/usr/bin/llama-server"),
            PathBuf::from("/models/test.gguf"),
            "test-model".to_string(),
            8847,
            None,
            None,
        );

        let request = CompletionRequest::new("test-model", vec![Message::user("Hello")]);
        let oai_request = provider.build_request(&request, false);

        assert_eq!(oai_request.model, "test-model");
        assert!(oai_request.tools.is_none());
        assert_eq!(oai_request.stream, Some(false));
    }

    #[test]
    fn test_count_tokens() {
        let provider = LocalProvider::new(
            PathBuf::from("/usr/bin/llama-server"),
            PathBuf::from("/models/test.gguf"),
            "test".to_string(),
            8847,
            None,
            None,
        );

        let count = provider.count_tokens("Hello world!", "test").unwrap();
        assert_eq!(count, 3); // 12 chars / 4 = 3
    }
}
