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

use regex::Regex;
use std::sync::LazyLock;

/// Regex to extract JSON from markdown code blocks
static MARKDOWN_JSON_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"```(?:json)?\s*\n?\s*(\{[\s\S]*?\})\s*\n?```").unwrap());

/// ChatML special tokens that models like Qwen output - need to filter these
static CHATML_TOKENS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<\|im_(?:start|end)\|>(?:\w+)?").unwrap());

/// Regex to parse Qwen-style XML tool calls: <function=name><parameter=key>value</parameter></function>
/// Also handles the variant: <tool_call><function=name>...</function></tool_call>
static QWEN_TOOL_CALL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:<tool_call>\s*)?<function=(\w+)>([\s\S]*?)</function>(?:\s*</tool_call>)?")
        .unwrap()
});

/// Regex to parse parameters within a Qwen-style tool call
static QWEN_PARAMETER_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<parameter=(\w+)>\s*([\s\S]*?)\s*</parameter>").unwrap());

/// Strip ChatML special tokens from text (like <|im_start|>, <|im_end|>assistant, etc.)
fn strip_chatml_tokens(text: &str) -> String {
    CHATML_TOKENS.replace_all(text, "").to_string()
}

/// Try to parse Qwen-style XML tool calls from text
/// Format: <function=name><parameter=key>value</parameter>...</function>
/// Returns vector of (tool_name, arguments as JSON)
fn try_parse_qwen_tool_calls(text: &str) -> Vec<(String, serde_json::Value)> {
    let mut results = Vec::new();
    let mut seen_keys = std::collections::HashSet::new();

    for caps in QWEN_TOOL_CALL_PATTERN.captures_iter(text) {
        if let (Some(name_match), Some(params_match)) = (caps.get(1), caps.get(2)) {
            let name = name_match.as_str().to_string();
            let params_text = params_match.as_str();

            // Parse all parameters into a JSON object
            let mut args = serde_json::Map::new();
            for param_caps in QWEN_PARAMETER_PATTERN.captures_iter(params_text) {
                if let (Some(key_match), Some(value_match)) = (param_caps.get(1), param_caps.get(2))
                {
                    let key = key_match.as_str().to_string();
                    let value = value_match.as_str().trim().to_string();

                    // Try to parse as JSON first, fall back to string
                    let json_value =
                        serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value));
                    args.insert(key, json_value);
                }
            }

            let args_value = serde_json::Value::Object(args);
            let key = format!("{}:{}", name, args_value);
            if seen_keys.insert(key) {
                results.push((name, args_value));
            }
        }
    }

    results
}

/// Try to parse ALL JSON tool calls from text content
/// Returns vector of (tool_name, arguments) for all found tool calls
/// DEDUPLICATES identical tool calls - if the model outputs the same call 3 times, we only return it once
/// Supports multiple formats:
/// 1. JSON in markdown code blocks: ```json {"name": "tool", "arguments": {...}} ```
/// 2. Raw JSON: {"name": "tool", "arguments": {...}}
/// 3. Qwen XML format: <function=tool><parameter=key>value</parameter></function>
fn try_parse_all_json_tool_calls(text: &str) -> Vec<(String, serde_json::Value)> {
    // First, strip any ChatML special tokens that models like Qwen output
    let cleaned_text = strip_chatml_tokens(text);
    let text = cleaned_text.as_str();

    let mut results = Vec::new();
    let mut seen_keys = std::collections::HashSet::new();

    // First, check for Qwen-style XML tool calls (highest priority for Qwen models)
    let qwen_results = try_parse_qwen_tool_calls(text);
    if !qwen_results.is_empty() {
        return qwen_results;
    }

    // Next, check for JSON inside markdown code blocks (common pattern from LLMs)
    for caps in MARKDOWN_JSON_PATTERN.captures_iter(text) {
        if let Some(json_match) = caps.get(1) {
            let json_str = json_match.as_str();
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(name) = parsed.get("name").and_then(|n| n.as_str()) {
                    if let Some(args) = parsed.get("arguments") {
                        let key = format!("{}:{}", name, args);
                        if seen_keys.insert(key) {
                            results.push((name.to_string(), args.clone()));
                        }
                    }
                }
            }
        }
    }

    // If we found tool calls in code blocks, return them (already deduplicated)
    if !results.is_empty() {
        return results;
    }

    // Otherwise, try to find raw JSON tool calls
    let trimmed = text.trim();
    let mut search_start = 0;

    while let Some(start) = trimmed[search_start..].find('{') {
        let abs_start = search_start + start;
        let json_part = &trimmed[abs_start..];

        // Try parsing with brace matching
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
                        let key = format!("{}:{}", name, args);
                        if seen_keys.insert(key) {
                            results.push((name.to_string(), args.clone()));
                        }
                    }
                }
            }
            search_start = abs_start + end_idx;
        } else {
            search_start = abs_start + 1;
        }
    }

    results
}

/// Try to parse a single JSON tool call from text content (for backwards compatibility)
/// Returns (tool_name, arguments) if found, None otherwise
#[cfg(test)]
fn try_parse_json_tool_call(text: &str) -> Option<(String, serde_json::Value)> {
    try_parse_all_json_tool_calls(text).into_iter().next()
}

/// Ollama local model provider
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    /// Use OpenAI-compatible API (/v1/chat/completions) instead of native Ollama API
    /// This can improve tool calling reliability for some models
    use_openai_api: bool,
}

impl OllamaProvider {
    /// Create a new Ollama provider with default base URL (http://localhost:11434)
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: DEFAULT_OLLAMA_URL.to_string(),
            use_openai_api: false,
        }
    }

    /// Create with a custom base URL
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            use_openai_api: false,
        }
    }

    /// Create with OpenAI-compatible API mode
    /// When enabled, uses /v1/chat/completions endpoint with OpenAI-style tool calling
    pub fn with_openai_api(base_url: impl Into<String>, use_openai_api: bool) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            use_openai_api,
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
    fn convert_messages(&self, messages: &[Message], system: Option<&str>) -> Vec<OllamaMessage> {
        let mut result = Vec::new();

        // Add system message first if provided (many models don't read the system field)
        if let Some(sys) = system {
            result.push(OllamaMessage {
                role: "system".to_string(),
                content: sys.to_string(),
                tool_calls: None,
            });
        }

        result.extend(messages.iter().filter(|m| m.role != Role::System).map(|m| {
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
                                is_error,
                            } => {
                                // For tool results, format clearly so the model understands
                                // this is the result of its tool call
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
                                // Format tool result clearly for Ollama models
                                // Keep it simple - verbose reminders can interfere with tool calling
                                let is_err = is_error.unwrap_or(false);
                                if is_err {
                                    text_parts.push(format!("[TOOL ERROR]\n{}", content_str));
                                } else {
                                    text_parts.push(format!("[TOOL RESULT]\n{}", content_str));
                                }
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
        }));

        result
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

        // Direct guidance for Ollama models - critical rules first
        let system_content = if !request.tools.is_empty() {
            // Build tool list dynamically from actual available tools
            let tool_names: Vec<&str> = request.tools.iter().map(|t| t.name.as_str()).collect();
            let tool_list = tool_names.join(", ");

            let tool_hint = format!(
                r#"
TOOLS: {}

CRITICAL RULES:

1. NEVER use shell with "find" or "ls" to list files. Use glob instead.
   - BAD: shell "find . -name *.rs"
   - GOOD: glob "src/**/*.rs"
   glob respects .gitignore and won't return build artifacts.

2. For complex tasks, use spawn_agent IMMEDIATELY:
   - "look at the project" → spawn_agent(agent_type="explore", task="...")
   - "analyze the code" → spawn_agent(agent_type="explore", task="...")
   - "what needs improvement" → spawn_agent(agent_type="explore", task="...")
   - "review the code" → spawn_agent(agent_type="explore", task="...")

3. For simple tasks (editing one file, quick question):
   - Use glob to find files
   - Use file_read to read them
   - Make your changes

DO NOT ask the user what they want to explore. Just use spawn_agent."#,
                tool_list
            );

            // Add model-specific guidance for models that need extra help
            let model_guidance = self.get_model_specific_guidance(&request.model);

            match &request.system {
                Some(sys) => Some(format!("{}\n\n{}{}", sys, tool_hint, model_guidance)),
                None => Some(format!("{}{}", tool_hint, model_guidance)),
            }
        } else {
            request.system.clone()
        };

        // Pass system as first message (many models don't read the separate system field)
        OllamaRequest {
            model: request.model.clone(),
            messages: self.convert_messages(&request.messages, system_content.as_deref()),
            system: None, // System is now in messages array for better model compatibility
            stream,
            options: Some(OllamaOptions {
                temperature: Some(request.temperature),
                num_predict: Some(request.max_tokens as i64),
            }),
            tools,
        }
    }

    /// Get model-specific guidance for models that need extra help with tool usage
    /// Smaller models and certain model families benefit from more explicit instructions
    fn get_model_specific_guidance(&self, model: &str) -> String {
        let model_lower = model.to_lowercase();

        // Qwen models have known issues with tool calling format (see QwenLM/Qwen3-Coder#475)
        // They frequently omit <tool_call> tags and need explicit format reminders
        if model_lower.contains("qwen") {
            return r#"

<IMPORTANT>
When calling tools, you MUST use this EXACT format:
<tool_call>
<function=TOOL_NAME><parameter=PARAM_NAME>value</parameter></function>
</tool_call>

CRITICAL RULES:
- Do NOT omit the <tool_call> tags
- Do NOT mix text and tool calls in the same response
- If calling a tool, ONLY output the tool call with NO other text
- Answer the user's ACTUAL question, not topics from examples
- Only report results you actually received from tools
</IMPORTANT>"#
                .to_string();
        }

        // Smaller models (7B and under) need more hand-holding
        if model_lower.contains(":7b")
            || model_lower.contains(":3b")
            || model_lower.contains(":1b")
            || model_lower.contains("-7b")
            || model_lower.contains("-3b")
        {
            return r#"

## Important for Smaller Models:
- Always use tools to explore - don't guess about the codebase
- One tool call at a time, wait for results before proceeding
- If a search finds nothing, try different search terms
- Stay focused on the user's specific request"#
                .to_string();
        }

        // CodeLlama and similar may need explicit coding focus
        if model_lower.contains("codellama") || model_lower.contains("code-llama") {
            return r#"

## Code Assistant Reminders:
- Search the codebase before making assumptions
- Read existing files to understand patterns before suggesting changes
- Use grep to find code patterns and implementations"#
                .to_string();
        }

        // DeepSeek models
        if model_lower.contains("deepseek") {
            return r#"

## DeepSeek Reminders:
- Use grep for content search, glob for file name patterns
- Explore first, then respond with findings
- Keep responses focused and actionable"#
                .to_string();
        }

        // Llama 3.x models - generally capable but benefit from structure
        if model_lower.contains("llama3")
            || model_lower.contains("llama-3")
            || model_lower.contains("llama:3")
        {
            return r#"

## Llama 3 Reminders:
- Use grep("topic") to find files about the topic the user asked about
- Read the files you find before answering
- Base your answer on actual code from this project
- Stay focused on answering the user's specific question"#
                .to_string();
        }

        // Mistral/Mixtral models
        if model_lower.contains("mistral") || model_lower.contains("mixtral") {
            return r#"

## Mistral Reminders:
- Search first with grep, then read files
- Answer based on code you actually read
- Stay on topic - don't drift to unrelated searches"#
                .to_string();
        }

        // Phi models (Microsoft)
        if model_lower.contains("phi") {
            return r#"

## Phi Model Reminders:
- Use grep to search file contents for the topic
- Read relevant files before responding
- Keep answers grounded in actual code from this project"#
                .to_string();
        }

        // Gemma models (Google)
        if model_lower.contains("gemma") {
            return r#"

## Gemma Reminders:
- Search with grep first to find relevant files
- Read those files to understand the code
- Answer based on what you actually found"#
                .to_string();
        }

        // Default for any other local model - provide basic guidance
        // since most local models benefit from explicit instructions
        r#"

## Local Model Guidance:
- Use grep("topic") to find files related to what the user asked
- Read the files you find with file_read
- Answer based on actual code you read from this project
- Stay focused on the user's question - don't go off-topic
- Only report what you actually found - never invent results"#
            .to_string()
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

    /// Pull a model from Ollama registry
    /// This is called automatically when a model is not found
    pub async fn pull_model(&self, model: &str) -> Result<()> {
        let url = format!("{}/api/pull", self.base_url);

        #[derive(Serialize)]
        struct PullRequest {
            name: String,
            stream: bool,
        }

        eprintln!(
            "[OLLAMA] Pulling model '{}'... (this may take a while)",
            model
        );

        let request = PullRequest {
            name: model.to_string(),
            stream: true, // Stream so we can show progress
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| TedError::Api(ApiError::Network(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(TedError::Api(ApiError::ServerError {
                status,
                message: format!("Failed to pull model '{}': {}", model, body),
            }));
        }

        // Stream the pull progress
        let mut stream = response.bytes_stream();
        use futures::StreamExt;

        while let Some(chunk) = stream.next().await {
            if let Ok(bytes) = chunk {
                // Parse progress updates from the stream
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    for line in text.lines() {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                            if let Some(status) = json.get("status").and_then(|s| s.as_str()) {
                                // Show download progress
                                if let Some(completed) =
                                    json.get("completed").and_then(|c| c.as_u64())
                                {
                                    if let Some(total) = json.get("total").and_then(|t| t.as_u64())
                                    {
                                        let pct = (completed as f64 / total as f64 * 100.0) as u32;
                                        eprint!("\r[OLLAMA] {}: {}%", status, pct);
                                    }
                                } else {
                                    eprintln!("[OLLAMA] {}", status);
                                }
                            }
                        }
                    }
                }
            }
        }
        eprintln!(); // New line after progress

        eprintln!("[OLLAMA] Successfully pulled '{}'", model);
        Ok(())
    }

    /// Parse a streaming chunk from Ollama's NDJSON response
    fn parse_stream_chunk(line: &str) -> Option<OllamaStreamResponse> {
        if line.trim().is_empty() {
            return None;
        }
        serde_json::from_str(line).ok()
    }

    // ==================== OpenAI-Compatible Mode ====================

    /// Build an OpenAI-compatible request
    fn build_openai_request(&self, request: &CompletionRequest, stream: bool) -> OpenAIRequest {
        let tools = if request.tools.is_empty() {
            None
        } else {
            Some(self.convert_tools_openai(&request.tools))
        };

        // Build tool hint dynamically from actual available tools
        let system_content = if !request.tools.is_empty() {
            let tool_names: Vec<&str> = request.tools.iter().map(|t| t.name.as_str()).collect();
            let tool_list = tool_names.join(", ");

            let tool_hint = format!(
                r#"
TOOLS: {}

CRITICAL RULES:

1. NEVER use shell with "find" or "ls" to list files. Use glob instead.
   - BAD: shell "find . -name *.rs"
   - GOOD: glob "src/**/*.rs"
   glob respects .gitignore and won't return build artifacts.

2. For complex tasks, use spawn_agent IMMEDIATELY:
   - "look at the project" → spawn_agent(agent_type="explore", task="...")
   - "analyze the code" → spawn_agent(agent_type="explore", task="...")
   - "what needs improvement" → spawn_agent(agent_type="explore", task="...")
   - "review the code" → spawn_agent(agent_type="explore", task="...")

3. For simple tasks (editing one file, quick question):
   - Use glob to find files
   - Use file_read to read them
   - Make your changes

DO NOT ask the user what they want to explore. Just use spawn_agent."#,
                tool_list
            );

            match &request.system {
                Some(sys) => Some(format!("{}\n\n{}", sys, tool_hint)),
                None => Some(tool_hint),
            }
        } else {
            request.system.clone()
        };

        OpenAIRequest {
            model: request.model.clone(),
            messages: self.convert_messages_openai(&request.messages, system_content.as_deref()),
            max_tokens: Some(request.max_tokens),
            temperature: Some(request.temperature),
            tools,
            stream: Some(stream),
        }
    }

    /// Convert messages to OpenAI format
    fn convert_messages_openai(
        &self,
        messages: &[Message],
        system: Option<&str>,
    ) -> Vec<OpenAIMessage> {
        let mut result = Vec::new();

        // Add system message first if provided
        if let Some(sys) = system {
            result.push(OpenAIMessage {
                role: "system".to_string(),
                content: Some(sys.to_string()),
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
                    result.push(OpenAIMessage {
                        role: role.to_string(),
                        content: Some(text.clone()),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
                MessageContent::Blocks(blocks) => {
                    let mut text_parts = Vec::new();
                    let mut tool_calls = Vec::new();
                    let mut tool_results: Vec<(String, String)> = Vec::new();

                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => {
                                text_parts.push(text.clone());
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls.push(OpenAIToolCall {
                                    id: id.clone(),
                                    r#type: "function".to_string(),
                                    function: OpenAIFunctionCall {
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
                                    format!("[TOOL ERROR]\n{}", content_str)
                                } else {
                                    format!("[TOOL RESULT]\n{}", content_str)
                                };
                                tool_results.push((tool_use_id.clone(), result_content));
                            }
                        }
                    }

                    // Add assistant message with text and/or tool calls
                    if role == "assistant" && (!text_parts.is_empty() || !tool_calls.is_empty()) {
                        result.push(OpenAIMessage {
                            role: "assistant".to_string(),
                            content: if text_parts.is_empty() {
                                None
                            } else {
                                Some(text_parts.join("\n"))
                            },
                            tool_calls: if tool_calls.is_empty() {
                                None
                            } else {
                                Some(tool_calls)
                            },
                            tool_call_id: None,
                        });
                    }

                    // Add tool results as separate messages
                    for (tool_id, result_content) in tool_results {
                        result.push(OpenAIMessage {
                            role: "tool".to_string(),
                            content: Some(result_content),
                            tool_calls: None,
                            tool_call_id: Some(tool_id),
                        });
                    }

                    // Add user message with text
                    if role == "user" && !text_parts.is_empty() {
                        result.push(OpenAIMessage {
                            role: "user".to_string(),
                            content: Some(text_parts.join("\n")),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                }
            }
        }

        result
    }

    /// Convert tools to OpenAI format
    fn convert_tools_openai(&self, tools: &[ToolDefinition]) -> Vec<OpenAITool> {
        tools
            .iter()
            .map(|t| OpenAITool {
                r#type: "function".to_string(),
                function: OpenAIFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: serde_json::to_value(&t.input_schema).unwrap_or_default(),
                },
            })
            .collect()
    }

    /// Complete using OpenAI-compatible endpoint
    async fn complete_openai(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let body = self.build_openai_request(&request, false);

        #[cfg(debug_assertions)]
        eprintln!("[OLLAMA OPENAI] Using OpenAI-compatible endpoint: {}", url);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
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
            let body_text = response.text().await.unwrap_or_default();
            return Err(self.parse_error(status, &body_text));
        }

        let response_text = response.text().await?;

        #[cfg(debug_assertions)]
        eprintln!(
            "[OLLAMA OPENAI] Response: {}",
            &response_text[..response_text.len().min(500)]
        );

        let api_response: OpenAIResponse = serde_json::from_str(&response_text).map_err(|e| {
            TedError::Api(ApiError::ServerError {
                status: 200,
                message: format!("Failed to parse OpenAI response: {}", e),
            })
        })?;

        // Convert response to our format
        let choice = api_response.choices.into_iter().next().ok_or_else(|| {
            TedError::Api(ApiError::InvalidResponse(
                "No choices in OpenAI response".to_string(),
            ))
        })?;

        let mut content: Vec<ContentBlockResponse> = Vec::new();

        // Add text content
        if let Some(text) = choice.message.content {
            if !text.is_empty() {
                content.push(ContentBlockResponse::Text { text });
            }
        }

        // Add tool calls if present
        if let Some(tool_calls) = choice.message.tool_calls {
            for tc in tool_calls {
                let input: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                content.push(ContentBlockResponse::ToolUse {
                    id: tc.id,
                    name: tc.function.name,
                    input,
                });
            }
        }

        // Determine stop reason
        let stop_reason = match choice.finish_reason.as_deref() {
            Some("tool_calls") => Some(StopReason::ToolUse),
            Some("stop") => Some(StopReason::EndTurn),
            Some("length") => Some(StopReason::MaxTokens),
            _ => {
                if content
                    .iter()
                    .any(|c| matches!(c, ContentBlockResponse::ToolUse { .. }))
                {
                    Some(StopReason::ToolUse)
                } else {
                    Some(StopReason::EndTurn)
                }
            }
        };

        Ok(CompletionResponse {
            id: format!("ollama-openai-{}", uuid::Uuid::new_v4()),
            model: request.model,
            content,
            stop_reason,
            usage: api_response.usage.map_or(
                Usage {
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                |u| Usage {
                    input_tokens: u.prompt_tokens,
                    output_tokens: u.completion_tokens,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
            ),
        })
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
        // Use OpenAI-compatible endpoint if enabled
        if self.use_openai_api {
            return self.complete_openai(request).await;
        }

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
            let body_text = response.text().await.unwrap_or_default();
            let err = self.parse_error(status, &body_text);

            // Auto-pull model if not found
            if let TedError::Api(ApiError::ModelNotFound(_)) = &err {
                eprintln!(
                    "[OLLAMA] Model '{}' not found, attempting to pull...",
                    request.model
                );
                if self.pull_model(&request.model).await.is_ok() {
                    // Retry the request after pulling
                    return Box::pin(self.complete(request)).await;
                }
            }
            return Err(err);
        }

        let response_text = response.text().await?;

        #[cfg(debug_assertions)]
        eprintln!("[OLLAMA DEBUG] Raw response: {} chars", response_text.len());
        #[cfg(debug_assertions)]
        if response_text.len() < 500 {
            eprintln!("[OLLAMA DEBUG] Response body: {}", response_text);
        } else {
            eprintln!(
                "[OLLAMA DEBUG] Response preview: {}...",
                &response_text[..500]
            );
        }

        let api_response: OllamaResponse = serde_json::from_str(&response_text).map_err(|e| {
            eprintln!("[OLLAMA ERROR] Failed to parse response: {}", e);
            eprintln!(
                "[OLLAMA ERROR] Response was: {}",
                &response_text[..response_text.len().min(1000)]
            );
            TedError::Api(ApiError::ServerError {
                status: 200,
                message: format!("Failed to parse Ollama response: {}", e),
            })
        })?;

        // Convert response to our format
        let mut content: Vec<ContentBlockResponse> = Vec::new();

        #[cfg(debug_assertions)]
        let msg_content_len = api_response.message.content.len();

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

        #[cfg(debug_assertions)]
        eprintln!(
            "[OLLAMA DEBUG] Parsed response - content blocks: {}, message content len: {}",
            content.len(),
            msg_content_len
        );

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

        #[cfg(debug_assertions)]
        eprintln!("[OLLAMA DEBUG] Stop reason: {:?}", stop_reason);

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
        // For OpenAI-compatible mode, convert to non-streaming call and emit events
        // This ensures tool calling works reliably even though we lose true streaming
        if self.use_openai_api {
            let response = self.complete_openai(request).await?;

            // Convert the response to a stream of events
            let mut events: Vec<Result<StreamEvent>> = Vec::new();

            events.push(Ok(StreamEvent::MessageStart {
                id: response.id.clone(),
                model: response.model.clone(),
            }));

            for (idx, block) in response.content.iter().enumerate() {
                match block {
                    ContentBlockResponse::Text { text } => {
                        events.push(Ok(StreamEvent::ContentBlockStart {
                            index: idx,
                            content_block: ContentBlockResponse::Text {
                                text: String::new(),
                            },
                        }));
                        events.push(Ok(StreamEvent::ContentBlockDelta {
                            index: idx,
                            delta: ContentBlockDelta::TextDelta { text: text.clone() },
                        }));
                        events.push(Ok(StreamEvent::ContentBlockStop { index: idx }));
                    }
                    ContentBlockResponse::ToolUse { id, name, input } => {
                        events.push(Ok(StreamEvent::ContentBlockStart {
                            index: idx,
                            content_block: ContentBlockResponse::ToolUse {
                                id: id.clone(),
                                name: name.clone(),
                                input: serde_json::Value::Object(serde_json::Map::new()),
                            },
                        }));
                        events.push(Ok(StreamEvent::ContentBlockDelta {
                            index: idx,
                            delta: ContentBlockDelta::InputJsonDelta {
                                partial_json: serde_json::to_string(input).unwrap_or_default(),
                            },
                        }));
                        events.push(Ok(StreamEvent::ContentBlockStop { index: idx }));
                    }
                }
            }

            events.push(Ok(StreamEvent::MessageDelta {
                stop_reason: response.stop_reason,
                usage: Some(response.usage),
            }));

            return Ok(Box::pin(futures::stream::iter(events)));
        }

        let url = format!("{}/api/chat", self.base_url);
        let body = self.build_request(&request, true);
        let model = request.model.clone();

        // Debug: Log what we're sending to Ollama
        #[cfg(debug_assertions)]
        {
            eprintln!(
                "[OLLAMA DEBUG] Sending {} messages to Ollama",
                body.messages.len()
            );
            for (i, msg) in body.messages.iter().enumerate() {
                // Use char_indices to safely truncate at character boundary
                let content_preview = if msg.content.chars().count() > 200 {
                    let end_idx = msg
                        .content
                        .char_indices()
                        .nth(200)
                        .map(|(i, _)| i)
                        .unwrap_or(msg.content.len());
                    format!(
                        "{}... ({} chars total)",
                        &msg.content[..end_idx],
                        msg.content.len()
                    )
                } else {
                    msg.content.clone()
                };
                eprintln!(
                    "[OLLAMA DEBUG] Message {}: role={}, content={}",
                    i, msg.role, content_preview
                );
                if let Some(ref tools) = msg.tool_calls {
                    eprintln!("[OLLAMA DEBUG]   tool_calls: {:?}", tools.len());
                }
            }
            if let Some(ref system) = body.system {
                eprintln!("[OLLAMA DEBUG] System prompt: {} chars", system.len());
            }
        }

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
            let body_text = response.text().await.unwrap_or_default();
            let err = self.parse_error(status, &body_text);

            // Auto-pull model if not found
            if let TedError::Api(ApiError::ModelNotFound(_)) = &err {
                eprintln!(
                    "[OLLAMA] Model '{}' not found, attempting to pull...",
                    request.model
                );
                if self.pull_model(&request.model).await.is_ok() {
                    // Retry the request after pulling
                    return Box::pin(self.complete_stream(request)).await;
                }
            }
            return Err(err);
        }

        let byte_stream = response.bytes_stream();

        // Generate a message ID for this stream
        let message_id = format!("ollama-{}", uuid::Uuid::new_v4());

        // Track state across the stream
        // State: (buffer, message_started, content_block_idx, msg_id, model_name, accumulated_text, has_native_tool_calls, text_block_started, might_be_tool_call)
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
                    false, // might_be_tool_call - if true, buffer text instead of streaming
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
                        might_be_tool_call,
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

                                // Check if this looks like it might be a JSON tool call
                                // If so, buffer it instead of streaming to avoid showing raw JSON to user
                                if !*might_be_tool_call {
                                    let trimmed = accumulated_text.trim_start();
                                    // If it starts with {, ```, or <|im_ it might be a tool call - buffer it
                                    // <|im_start|> and <|im_end|> are ChatML tokens from Qwen models
                                    if trimmed.starts_with('{')
                                        || trimmed.starts_with("```")
                                        || trimmed.starts_with("<|im_")
                                    {
                                        *might_be_tool_call = true;
                                    }
                                }

                                // Only stream text if it doesn't look like a tool call
                                if !*might_be_tool_call {
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

                                    // Strip ChatML tokens before emitting text
                                    let cleaned_text =
                                        strip_chatml_tokens(&chunk_response.message.content);
                                    if !cleaned_text.is_empty() {
                                        events.push(Ok(StreamEvent::ContentBlockDelta {
                                            index: *content_block_idx,
                                            delta: ContentBlockDelta::TextDelta {
                                                text: cleaned_text,
                                            },
                                        }));
                                    }
                                }
                            }

                            // Handle done
                            if chunk_response.done {
                                // Check if the text contains JSON tool calls that weren't handled natively
                                // This handles models that output tool calls as JSON text
                                let mut detected_tool_calls = false;

                                if !*has_native_tool_calls && !accumulated_text.is_empty() {
                                    let tool_calls =
                                        try_parse_all_json_tool_calls(accumulated_text);

                                    if !tool_calls.is_empty() {
                                        detected_tool_calls = true;

                                        // Close the text block if one was started
                                        if *text_block_started {
                                            events.push(Ok(StreamEvent::ContentBlockStop {
                                                index: *content_block_idx,
                                            }));
                                            *content_block_idx += 1;
                                        }

                                        // Emit tool use events for ALL detected tool calls
                                        for (tool_name, tool_args) in tool_calls {
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

                                            let args_str = serde_json::to_string(&tool_args)
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
                                    } else if *might_be_tool_call && !accumulated_text.is_empty() {
                                        // We buffered text thinking it was a tool call, but it wasn't
                                        // Now emit it as regular text (after cleaning ChatML tokens)
                                        let cleaned = strip_chatml_tokens(accumulated_text);
                                        if !cleaned.trim().is_empty() {
                                            events.push(Ok(StreamEvent::ContentBlockStart {
                                                index: *content_block_idx,
                                                content_block: ContentBlockResponse::Text {
                                                    text: String::new(),
                                                },
                                            }));
                                            events.push(Ok(StreamEvent::ContentBlockDelta {
                                                index: *content_block_idx,
                                                delta: ContentBlockDelta::TextDelta {
                                                    text: cleaned,
                                                },
                                            }));
                                            *text_block_started = true;
                                        }
                                    }
                                }

                                if !detected_tool_calls {
                                    // Close any open content block
                                    if *text_block_started {
                                        events.push(Ok(StreamEvent::ContentBlockStop {
                                            index: *content_block_idx,
                                        }));
                                    }
                                }

                                // Determine stop reason
                                let has_tools = detected_tool_calls
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

// OpenAI-compatible API types (for /v1/chat/completions endpoint)
// These provide better tool calling reliability for some models

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OpenAIToolCall {
    id: String,
    #[serde(rename = "type")]
    r#type: String,
    function: OpenAIFunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    r#type: String,
    function: OpenAIFunction,
}

#[derive(Debug, Serialize)]
struct OpenAIFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::message::Message;
    use crate::llm::provider::ToolInputSchema;

    // ===== ChatML Token Stripping Tests =====

    #[test]
    fn test_strip_chatml_tokens_basic() {
        let text = "<|im_start|>assistant\nHello world";
        let result = strip_chatml_tokens(text);
        assert_eq!(result, "\nHello world");
    }

    #[test]
    fn test_strip_chatml_tokens_multiple() {
        let text = "<|im_start|>assistant\n{\"name\": \"test\"}<|im_end|>";
        let result = strip_chatml_tokens(text);
        assert_eq!(result, "\n{\"name\": \"test\"}");
    }

    #[test]
    fn test_strip_chatml_tokens_in_json() {
        let text =
            r#"<|im_start|>{"name": "plan_update", "arguments": {"action": "create"}}<|im_end|>"#;
        let result = strip_chatml_tokens(text);
        assert_eq!(
            result,
            r#"{"name": "plan_update", "arguments": {"action": "create"}}"#
        );
    }

    #[test]
    fn test_strip_chatml_tokens_empty_text() {
        let text = "";
        let result = strip_chatml_tokens(text);
        assert_eq!(result, "");
    }

    #[test]
    fn test_strip_chatml_tokens_no_tokens() {
        let text = "This is regular text without any special tokens.";
        let result = strip_chatml_tokens(text);
        assert_eq!(result, text);
    }

    #[test]
    fn test_parse_tool_call_with_chatml_tokens() {
        // This is what Qwen actually outputs - should still parse the tool call
        let text = r#"<|im_start|>{"name": "plan_update", "arguments": {"action": "create", "title": "Test", "content": "- [ ] Task 1"}}"#;
        let result = try_parse_json_tool_call(text);
        assert!(result.is_some());
        let (name, args) = result.unwrap();
        assert_eq!(name, "plan_update");
        assert_eq!(args["action"], "create");
    }

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

    // ===== Qwen XML Format Tool Call Parsing Tests =====

    #[test]
    fn test_try_parse_qwen_tool_calls_simple() {
        let text = r#"<function=glob><parameter=pattern>*</parameter></function>"#;
        let results = try_parse_qwen_tool_calls(text);
        assert_eq!(results.len(), 1);
        let (name, args) = &results[0];
        assert_eq!(name, "glob");
        assert_eq!(args["pattern"], "*");
    }

    #[test]
    fn test_try_parse_qwen_tool_calls_with_tool_call_wrapper() {
        let text = r#"<tool_call><function=file_read><parameter=path>src/main.rs</parameter></function></tool_call>"#;
        let results = try_parse_qwen_tool_calls(text);
        assert_eq!(results.len(), 1);
        let (name, args) = &results[0];
        assert_eq!(name, "file_read");
        assert_eq!(args["path"], "src/main.rs");
    }

    #[test]
    fn test_try_parse_qwen_tool_calls_multiple_parameters() {
        let text = r#"<function=file_write><parameter=path>test.txt</parameter><parameter=content>Hello World</parameter></function>"#;
        let results = try_parse_qwen_tool_calls(text);
        assert_eq!(results.len(), 1);
        let (name, args) = &results[0];
        assert_eq!(name, "file_write");
        assert_eq!(args["path"], "test.txt");
        assert_eq!(args["content"], "Hello World");
    }

    #[test]
    fn test_try_parse_qwen_tool_calls_with_newlines() {
        let text = r#"<function=glob>
<parameter=pattern>
**/*.rs
</parameter>
</function>"#;
        let results = try_parse_qwen_tool_calls(text);
        assert_eq!(results.len(), 1);
        let (name, args) = &results[0];
        assert_eq!(name, "glob");
        assert_eq!(args["pattern"], "**/*.rs");
    }

    #[test]
    fn test_try_parse_qwen_tool_calls_with_surrounding_text() {
        let text = r#"I'll help you explore the project.

<function=glob><parameter=pattern>*</parameter></function>

Let me see what files exist."#;
        let results = try_parse_qwen_tool_calls(text);
        assert_eq!(results.len(), 1);
        let (name, args) = &results[0];
        assert_eq!(name, "glob");
        assert_eq!(args["pattern"], "*");
    }

    #[test]
    fn test_try_parse_qwen_tool_calls_no_match() {
        let text = "This is just regular text without any tool calls.";
        let results = try_parse_qwen_tool_calls(text);
        assert!(results.is_empty());
    }

    #[test]
    fn test_try_parse_all_prefers_qwen_format() {
        // When Qwen format is present, it should be parsed even if there's also JSON
        let text = r#"<function=glob><parameter=pattern>*.rs</parameter></function>"#;
        let results = try_parse_all_json_tool_calls(text);
        assert_eq!(results.len(), 1);
        let (name, args) = &results[0];
        assert_eq!(name, "glob");
        assert_eq!(args["pattern"], "*.rs");
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

        let converted = provider.convert_messages(&messages, None);

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

        let converted = provider.convert_messages(&messages, None);

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

        // System is now in messages array (first message) for better model compatibility
        assert!(built.system.is_none());
        assert!(!built.messages.is_empty());
        assert_eq!(built.messages[0].role, "system");
        assert!(built.messages[0].content.contains("You are helpful"));
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

        let converted = provider.convert_messages(&messages, None);
        assert!(converted.is_empty());
    }

    #[test]
    fn test_convert_messages_only_system() {
        let provider = OllamaProvider::new();
        let messages = vec![Message::system("You are helpful")];

        let converted = provider.convert_messages(&messages, None);
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

    // ===== Wiremock Integration Tests =====

    #[tokio::test]
    async fn test_health_check_success() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": []
            })))
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let result = provider.health_check().await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_health_check_server_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let result = provider.health_check().await;

        assert!(result.is_ok());
        assert!(!result.unwrap()); // 500 means not healthy
    }

    #[tokio::test]
    async fn test_list_local_models_success() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": [
                    {"name": "qwen2.5-coder:14b"},
                    {"name": "llama3.2:latest"},
                    {"name": "codellama:7b"}
                ]
            })))
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let result = provider.list_local_models().await;

        assert!(result.is_ok());
        let models = result.unwrap();
        assert_eq!(models.len(), 3);
        assert!(models.contains(&"qwen2.5-coder:14b".to_string()));
        assert!(models.contains(&"llama3.2:latest".to_string()));
        assert!(models.contains(&"codellama:7b".to_string()));
    }

    #[tokio::test]
    async fn test_list_local_models_empty() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "models": []
            })))
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let result = provider.list_local_models().await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_local_models_server_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal error"))
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let result = provider.list_local_models().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_complete_success() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help you?"
                },
                "done": true,
                "prompt_eval_count": 10,
                "eval_count": 8
            })))
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let request = CompletionRequest::new("qwen2.5-coder:14b", vec![Message::user("Hi")]);

        let result = provider.complete(request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(!response.content.is_empty());
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 8);
    }

    #[tokio::test]
    async fn test_complete_with_tool_calls() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "function": {
                            "name": "file_read",
                            "arguments": {"path": "/src/main.rs"}
                        }
                    }]
                },
                "done": true,
                "prompt_eval_count": 15,
                "eval_count": 20
            })))
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let request =
            CompletionRequest::new("qwen2.5-coder:14b", vec![Message::user("Read main.rs")]);

        let result = provider.complete(request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.stop_reason, Some(StopReason::ToolUse));

        // Check that tool use is in the content
        let has_tool_use = response
            .content
            .iter()
            .any(|c| matches!(c, ContentBlockResponse::ToolUse { .. }));
        assert!(has_tool_use);
    }

    #[tokio::test]
    async fn test_complete_server_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(
                ResponseTemplate::new(500)
                    .set_body_json(serde_json::json!({"error": "Internal server error"})),
            )
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let request = CompletionRequest::new("qwen2.5-coder:14b", vec![Message::user("Hi")]);

        let result = provider.complete(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_complete_stream_success() {
        use futures::StreamExt;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Simulate streaming NDJSON response
        let stream_body = r#"{"message":{"role":"assistant","content":"Hello"},"done":false}
{"message":{"role":"assistant","content":" world"},"done":false}
{"message":{"role":"assistant","content":"!"},"done":true,"eval_count":3,"prompt_eval_count":5}
"#;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let request = CompletionRequest::new("qwen2.5-coder:14b", vec![Message::user("Hi")]);

        let result = provider.complete_stream(request).await;
        assert!(result.is_ok());

        let mut stream = result.unwrap();
        let mut events = Vec::new();

        while let Some(event) = stream.next().await {
            events.push(event);
        }

        // Should have received some events
        assert!(!events.is_empty());

        // Should have a MessageStart
        let has_start = events
            .iter()
            .any(|e| matches!(e, Ok(StreamEvent::MessageStart { .. })));
        assert!(has_start);

        // Should have a MessageStop
        let has_stop = events
            .iter()
            .any(|e| matches!(e, Ok(StreamEvent::MessageStop)));
        assert!(has_stop);
    }

    #[tokio::test]
    async fn test_complete_stream_with_tool_call() {
        use futures::StreamExt;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Simulate streaming response with native tool call
        let stream_body = r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"glob","arguments":{"pattern":"*.rs"}}}]},"done":true,"eval_count":10,"prompt_eval_count":5}
"#;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let request =
            CompletionRequest::new("qwen2.5-coder:14b", vec![Message::user("List files")]);

        let result = provider.complete_stream(request).await;
        assert!(result.is_ok());

        let mut stream = result.unwrap();
        let mut events = Vec::new();

        while let Some(event) = stream.next().await {
            events.push(event);
        }

        // Should have tool use events
        let has_tool_use = events.iter().any(|e| {
            matches!(
                e,
                Ok(StreamEvent::ContentBlockStart {
                    content_block: ContentBlockResponse::ToolUse { .. },
                    ..
                })
            )
        });
        assert!(has_tool_use);
    }

    #[tokio::test]
    async fn test_complete_stream_json_tool_call_in_text() {
        use futures::StreamExt;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Model outputs tool call as JSON text (common for smaller models)
        let stream_body = r#"{"message":{"role":"assistant","content":"{\"name\": \"glob\", \"arguments\": {\"pattern\": \"*.rs\"}}"},"done":true,"eval_count":10,"prompt_eval_count":5}
"#;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let request = CompletionRequest::new("qwen2.5-coder:7b", vec![Message::user("List files")]);

        let result = provider.complete_stream(request).await;
        assert!(result.is_ok());

        let mut stream = result.unwrap();
        let mut events = Vec::new();

        while let Some(event) = stream.next().await {
            events.push(event);
        }

        // Should detect and parse the JSON tool call
        let has_tool_use = events.iter().any(|e| {
            matches!(
                e,
                Ok(StreamEvent::ContentBlockStart {
                    content_block: ContentBlockResponse::ToolUse { .. },
                    ..
                })
            )
        });
        assert!(has_tool_use);
    }

    #[tokio::test]
    async fn test_complete_stream_server_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(
                ResponseTemplate::new(500)
                    .set_body_json(serde_json::json!({"error": "Server error"})),
            )
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let request = CompletionRequest::new("qwen2.5-coder:14b", vec![Message::user("Hi")]);

        let result = provider.complete_stream(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_pull_model_success() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Simulate streaming pull progress
        let pull_body = r#"{"status":"pulling manifest"}
{"status":"downloading","completed":1000,"total":10000}
{"status":"downloading","completed":10000,"total":10000}
{"status":"success"}
"#;

        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_string(pull_body))
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let result = provider.pull_model("qwen2.5-coder:14b").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pull_model_failure() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Model not found"))
            .mount(&mock_server)
            .await;

        let provider = OllamaProvider::with_base_url(mock_server.uri());
        let result = provider.pull_model("nonexistent-model").await;

        assert!(result.is_err());
    }

    // ===== Additional Tool Call Deduplication Tests =====

    #[test]
    fn test_try_parse_all_deduplicates() {
        // Same tool call repeated multiple times
        let text = r#"{"name": "glob", "arguments": {"pattern": "*.rs"}}
{"name": "glob", "arguments": {"pattern": "*.rs"}}
{"name": "glob", "arguments": {"pattern": "*.rs"}}"#;

        let results = try_parse_all_json_tool_calls(text);
        // Should deduplicate to 1
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_try_parse_all_multiple_different() {
        let text = r#"{"name": "glob", "arguments": {"pattern": "*.rs"}}
{"name": "file_read", "arguments": {"path": "/src/main.rs"}}"#;

        let results = try_parse_all_json_tool_calls(text);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_try_parse_markdown_code_block() {
        let text = r#"I'll read the file:

```json
{"name": "file_read", "arguments": {"path": "/src/lib.rs"}}
```

Let me check it."#;

        let results = try_parse_all_json_tool_calls(text);
        assert_eq!(results.len(), 1);
        let (name, args) = &results[0];
        assert_eq!(name, "file_read");
        assert_eq!(args["path"], "/src/lib.rs");
    }

    #[test]
    fn test_try_parse_qwen_deduplicates() {
        let text = r#"<function=glob><parameter=pattern>*.rs</parameter></function>
<function=glob><parameter=pattern>*.rs</parameter></function>"#;

        let results = try_parse_qwen_tool_calls(text);
        assert_eq!(results.len(), 1);
    }
}
