// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Embedded mode chat runner
//!
//! Runs Ted in embedded mode, outputting JSONL events instead of interactive TUI.

use futures::StreamExt;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::caps::{CapLoader, CapResolver};
use crate::cli::ChatArgs;
use crate::config::Settings;
use crate::context::memory::MemoryStore;
use crate::context::{recall, summarizer};
use crate::embedded::{HistoryMessageData, JsonLEmitter, PlanStep};
use crate::embeddings::EmbeddingGenerator;
use crate::error::{Result, TedError};
use crate::llm::message::{ContentBlock, Message, MessageContent};
use crate::llm::provider::{
    CompletionRequest, ContentBlockDelta, ContentBlockResponse, LlmProvider, StreamEvent,
    ToolChoice,
};
use crate::llm::providers::{
    AnthropicProvider, BlackmanProvider, OllamaProvider, OpenRouterProvider,
};
use crate::tools::{ShellOutputEvent, ToolContext, ToolExecutor};

/// Simple message struct for history serialization
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct HistoryMessage {
    role: String,
    content: String,
}

/// Extract history messages from a list of Messages for persistence
/// Filters out internal enforcement messages (those starting with "STOP!")
fn extract_history_messages(messages: &[Message]) -> Vec<HistoryMessageData> {
    messages
        .iter()
        .filter_map(|msg| {
            let role = match msg.role {
                crate::llm::message::Role::User => "user",
                crate::llm::message::Role::Assistant => "assistant",
                crate::llm::message::Role::System => return None,
            };

            let text = match &msg.content {
                MessageContent::Text(text) => text.clone(),
                MessageContent::Blocks(blocks) => blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
            };

            // Skip empty messages
            if text.is_empty() {
                return None;
            }

            // Skip internal enforcement messages (they start with "STOP!")
            // These are injected to guide the model but shouldn't be saved to history
            if text.starts_with("STOP!") {
                return None;
            }

            Some(HistoryMessageData {
                role: role.to_string(),
                content: text,
            })
        })
        .collect()
}

/// Create a hash key for deduplicating tool calls
fn tool_call_key(name: &str, input: &serde_json::Value) -> String {
    format!("{}:{}", name, input)
}

/// Extract JSON tool calls from text that may contain markdown code blocks
/// Ollama models often output tool calls as ```json ... ``` blocks
fn extract_json_tool_calls(text: &str) -> Vec<(String, serde_json::Value)> {
    let mut tools = Vec::new();

    // Pattern 1: Look for ```json ... ``` blocks containing tool calls
    let json_block_re = regex::Regex::new(r"```json\s*([\s\S]*?)```").unwrap();
    for cap in json_block_re.captures_iter(text) {
        if let Some(json_str) = cap.get(1) {
            let json_text = json_str.as_str().trim();
            eprintln!(
                "[TOOL PARSE] Found JSON block ({} chars): {}",
                json_text.len(),
                &json_text[..json_text.len().min(200)]
            );

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_text) {
                eprintln!("[TOOL PARSE] Successfully parsed JSON");

                // Check if it's an array of tool calls
                if let Some(arr) = parsed.as_array() {
                    for item in arr {
                        if let Some(tool) = parse_tool_from_json(item) {
                            eprintln!("[TOOL PARSE] Extracted tool from array: {}", tool.0);
                            tools.push(tool);
                        }
                    }
                } else if let Some(tool) = parse_tool_from_json(&parsed) {
                    eprintln!("[TOOL PARSE] Extracted tool: {}", tool.0);
                    tools.push(tool);
                }
            } else {
                eprintln!("[TOOL PARSE] Failed to parse JSON block");
            }
        }
    }

    // Pattern 2: Look for ``` ... ``` blocks without json marker (some models do this)
    if tools.is_empty() {
        let generic_block_re = regex::Regex::new(r"```\s*([\s\S]*?)```").unwrap();
        for cap in generic_block_re.captures_iter(text) {
            if let Some(block_str) = cap.get(1) {
                let block_text = block_str.as_str().trim();
                // Only try to parse if it looks like JSON
                if block_text.starts_with('{') || block_text.starts_with('[') {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(block_text) {
                        if let Some(arr) = parsed.as_array() {
                            for item in arr {
                                if let Some(tool) = parse_tool_from_json(item) {
                                    tools.push(tool);
                                }
                            }
                        } else if let Some(tool) = parse_tool_from_json(&parsed) {
                            tools.push(tool);
                        }
                    }
                }
            }
        }
    }

    // Pattern 3: Try to find JSON objects by scanning for balanced braces
    // This is more robust than a regex for complex nested JSON
    if tools.is_empty() {
        eprintln!("[TOOL PARSE] No tools from markdown blocks, trying brace scanning");
        for tool in extract_json_objects_by_braces(text) {
            tools.push(tool);
        }
    }

    tools
}

/// Extract JSON objects from text by scanning for balanced braces
/// More robust than regex for nested JSON structures
fn extract_json_objects_by_braces(text: &str) -> Vec<(String, serde_json::Value)> {
    let mut tools = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Look for start of JSON object
        if chars[i] == '{' {
            // Try to find balanced closing brace
            let mut brace_count = 1;
            let start = i;
            i += 1;

            while i < chars.len() && brace_count > 0 {
                match chars[i] {
                    '{' => brace_count += 1,
                    '}' => brace_count -= 1,
                    '"' => {
                        // Skip string content (handle escaped quotes)
                        i += 1;
                        while i < chars.len() {
                            if chars[i] == '\\' && i + 1 < chars.len() {
                                i += 2; // Skip escaped char
                                continue;
                            }
                            if chars[i] == '"' {
                                break;
                            }
                            i += 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }

            if brace_count == 0 {
                // Found balanced braces, try to parse
                let json_str: String = chars[start..i].iter().collect();

                // Check if it looks like a tool call before parsing (performance optimization)
                if json_str.contains("\"name\"")
                    && (json_str.contains("\"arguments\"") || json_str.contains("\"input\""))
                {
                    eprintln!(
                        "[TOOL PARSE] Found potential tool JSON ({} chars)",
                        json_str.len()
                    );

                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) {
                        if let Some(tool) = parse_tool_from_json(&parsed) {
                            eprintln!("[TOOL PARSE] Extracted tool via brace scan: {}", tool.0);
                            tools.push(tool);
                        }
                    }
                }
            }
        } else {
            i += 1;
        }
    }

    tools
}

/// Parse a tool call from a JSON value
fn parse_tool_from_json(value: &serde_json::Value) -> Option<(String, serde_json::Value)> {
    let obj = value.as_object()?;

    // Look for {"name": "...", "arguments": {...}} or {"name": "...", "input": {...}} format
    let name = obj.get("name")?.as_str()?;

    // Try "arguments" first, then "input" as fallback (different LLM output formats)
    let arguments = obj.get("arguments").or_else(|| obj.get("input")).cloned()?;

    // Map tool names: normalize various formats to our internal tool names
    // Our tools are: file_read, file_edit, file_write, file_delete, glob, grep, shell
    // The tool registry also has aliases (read_file -> file_read, edit_file -> file_edit, etc.)
    // but we normalize here for consistency in event emission
    let mapped_name = match name {
        "file_read" | "read_file" => "file_read",
        "file_edit" | "edit_file" => "file_edit",
        "file_create" | "create_file" | "file_write" | "write_file" => "file_write",
        "file_delete" | "delete_file" => "file_delete",
        _ => name,
    };

    // Map argument names for various tools: different models use different names
    let mapped_arguments = match mapped_name {
        "file_read" => map_file_read_arguments(&arguments),
        "file_edit" => map_file_edit_arguments(&arguments),
        "file_write" => map_file_write_arguments(&arguments),
        "shell" => map_shell_arguments(&arguments),
        _ => arguments,
    };

    // Special case: file_edit with empty old_string should become file_write
    // Models sometimes use edit with empty old to mean "write/create file"
    let (final_name, final_args) = if mapped_name == "file_edit" {
        let old_string = mapped_arguments
            .get("old_string")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if old_string.is_empty() || old_string.trim().is_empty() {
            // Convert to file_write: rename new_string to content
            let mut write_args = serde_json::Map::new();
            if let Some(path) = mapped_arguments.get("path") {
                write_args.insert("path".to_string(), path.clone());
            }
            if let Some(new_content) = mapped_arguments.get("new_string") {
                write_args.insert("content".to_string(), new_content.clone());
            }
            (
                "file_write".to_string(),
                serde_json::Value::Object(write_args),
            )
        } else {
            (mapped_name.to_string(), mapped_arguments)
        }
    } else {
        (mapped_name.to_string(), mapped_arguments)
    };

    Some((final_name, final_args))
}

/// Map file_edit argument names from various LLM output formats to our expected format
/// Different models (Ollama, etc.) use different naming conventions:
/// - old/new, old_text/new_text, old_string/new_string, find/replace, etc.
/// - Some send arrays of lines instead of strings
fn map_file_edit_arguments(args: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = args.as_object() else {
        return args.clone();
    };

    let mut mapped = serde_json::Map::new();

    for (key, value) in obj {
        let mapped_key = match key.as_str() {
            // Map various names for "what to find/replace"
            "old_text" | "oldText" | "old_content" | "oldContent" | "find" | "search"
            | "original" | "old" | "before" | "pattern" | "target" | "match" => "old_string",

            // Map various names for "what to replace with"
            "new_text" | "newText" | "new_content" | "newContent" | "replace" | "replacement"
            | "modified" | "new" | "after" | "content" | "updated" | "with" => "new_string",

            // Map path variations
            "file" | "file_path" | "filepath" | "filename" | "file_name" => "path",

            // Already correct names - still pass through for array conversion
            "old_string" => "old_string",
            "new_string" => "new_string",
            "path" => "path",

            _ => key.as_str(),
        };

        // Handle array values - join them into a single string with newlines
        // Models like Ollama often send old/new as arrays of lines
        let mapped_value =
            if (mapped_key == "old_string" || mapped_key == "new_string") && value.is_array() {
                if let Some(arr) = value.as_array() {
                    let joined = arr
                        .iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    serde_json::Value::String(joined)
                } else {
                    value.clone()
                }
            } else {
                value.clone()
            };

        mapped.insert(mapped_key.to_string(), mapped_value);
    }

    serde_json::Value::Object(mapped)
}

/// Map file_read/read_file argument names from various LLM output formats to our expected format
fn map_file_read_arguments(args: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = args.as_object() else {
        return args.clone();
    };

    let mut mapped = serde_json::Map::new();

    for (key, value) in obj {
        let mapped_key = match key.as_str() {
            "file" | "file_path" | "filepath" | "filename" | "name" | "file_name" => "path",
            _ => key.as_str(),
        };
        mapped.insert(mapped_key.to_string(), value.clone());
    }

    serde_json::Value::Object(mapped)
}

/// Map file_write argument names from various LLM output formats to our expected format
fn map_file_write_arguments(args: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = args.as_object() else {
        return args.clone();
    };

    let mut mapped = serde_json::Map::new();

    for (key, value) in obj {
        let mapped_key = match key.as_str() {
            "file" | "file_path" | "filepath" | "filename" | "name" | "file_name" => "path",
            "text" | "data" | "contents" | "file_content" | "code" | "body" => "content",
            _ => key.as_str(),
        };
        mapped.insert(mapped_key.to_string(), value.clone());
    }

    serde_json::Value::Object(mapped)
}

/// Map shell/command argument names from various LLM output formats
fn map_shell_arguments(args: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = args.as_object() else {
        return args.clone();
    };

    let mut mapped = serde_json::Map::new();

    for (key, value) in obj {
        let mapped_key = match key.as_str() {
            "cmd" | "shell_command" | "bash" | "exec" | "run" => "command",
            _ => key.as_str(),
        };
        mapped.insert(mapped_key.to_string(), value.clone());
    }

    serde_json::Value::Object(mapped)
}

pub async fn run_embedded_chat(args: ChatArgs, settings: Settings) -> Result<()> {
    // Get prompt (required in embedded mode)
    let prompt = args
        .prompt
        .clone()
        .ok_or_else(|| TedError::Config("Embedded mode requires a prompt argument".to_string()))?;

    // Review mode: emit file events but don't execute file modifications
    let review_mode = args.review_mode;
    if review_mode {
        eprintln!("[REVIEW MODE] Enabled - file modifications will be emitted but not executed");
    }

    // Determine provider
    let provider_name = args
        .provider
        .clone()
        .unwrap_or_else(|| settings.defaults.provider.clone());

    // Create provider
    let provider: Box<dyn LlmProvider> = match provider_name.as_str() {
        "ollama" => {
            let ollama_provider =
                OllamaProvider::with_base_url(&settings.providers.ollama.base_url);
            Box::new(ollama_provider)
        }
        "openrouter" => {
            let api_key = settings
                .get_openrouter_api_key()
                .ok_or_else(|| TedError::Config("No OpenRouter API key found".to_string()))?;
            let provider = if let Some(ref base_url) = settings.providers.openrouter.base_url {
                OpenRouterProvider::with_base_url(api_key, base_url)
            } else {
                OpenRouterProvider::new(api_key)
            };
            Box::new(provider)
        }
        "blackman" => {
            let api_key = settings
                .get_blackman_api_key()
                .ok_or_else(|| TedError::Config("No Blackman AI API key found. Set BLACKMAN_API_KEY environment variable or configure in settings.".to_string()))?;
            let base_url = settings.get_blackman_base_url();
            Box::new(BlackmanProvider::with_base_url(api_key, base_url))
        }
        _ => {
            let api_key = settings
                .get_anthropic_api_key()
                .ok_or_else(|| TedError::Config("No Anthropic API key found".to_string()))?;
            Box::new(AnthropicProvider::new(api_key))
        }
    };

    // Load caps
    let cap_names: Vec<String> = if args.cap.is_empty() {
        settings.defaults.caps.clone()
    } else {
        args.cap.clone()
    };

    let loader = CapLoader::new();
    let resolver = CapResolver::new(loader.clone());
    let mut merged_cap = resolver.resolve_and_merge(&cap_names)?;

    // If a system prompt file was provided (by frontend like Teddy), append its content
    // This allows frontends to inject opinionated defaults without modifying Ted's core
    if let Some(ref prompt_file) = args.system_prompt_file {
        match std::fs::read_to_string(prompt_file) {
            Ok(extra_prompt) => {
                if !extra_prompt.trim().is_empty() {
                    eprintln!(
                        "[PROMPT] Appending custom system prompt from {:?}",
                        prompt_file
                    );
                    if !merged_cap.system_prompt.is_empty() {
                        merged_cap.system_prompt.push_str("\n\n");
                    }
                    merged_cap.system_prompt.push_str(&extra_prompt);
                }
            }
            Err(e) => {
                eprintln!(
                    "[PROMPT] Warning: Could not read system prompt file {:?}: {}",
                    prompt_file, e
                );
            }
        }
    }

    // Determine model
    let model = args
        .model
        .clone()
        .or_else(|| merged_cap.preferred_model().map(|s| s.to_string()))
        .unwrap_or_else(|| match provider_name.as_str() {
            "ollama" => settings.providers.ollama.default_model.clone(),
            "openrouter" => settings.providers.openrouter.default_model.clone(),
            "blackman" => "gpt-4o-mini".to_string(), // Default Blackman model
            _ => settings.providers.anthropic.default_model.clone(),
        });

    // Create session
    let session_id = uuid::Uuid::new_v4();
    let emitter = JsonLEmitter::new(session_id.to_string());

    // Setup working directory
    let working_directory = std::env::current_dir()?;
    let project_root = crate::utils::find_project_root();

    // Create channel for shell output streaming
    let (shell_tx, mut shell_rx) = mpsc::unbounded_channel::<ShellOutputEvent>();

    // Create tool context and executor with shell output sender
    let tool_context = ToolContext::new(
        working_directory.clone(),
        project_root.clone(),
        session_id,
        args.trust,
    )
    .with_shell_output_sender(shell_tx)
    .with_files_in_context(args.files_in_context.clone());
    let mut tool_executor = ToolExecutor::new(tool_context, args.trust);

    // Initialize conversation memory (only if explicitly enabled)
    // Memory is disabled by default because the summarizer can produce garbage summaries
    // that pollute future conversations. Set TED_ENABLE_MEMORY=1 to enable.
    let memory_store = if std::env::var("TED_ENABLE_MEMORY").is_ok() {
        let memory_path = dirs::home_dir()
            .ok_or_else(|| TedError::Config("Could not determine home directory".to_string()))?
            .join(".ted")
            .join("memory.db");

        // Ensure directory exists
        if let Some(parent) = memory_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let embedding_generator = EmbeddingGenerator::new();
        match MemoryStore::open(&memory_path, embedding_generator.clone()) {
            Ok(store) => Some((store, embedding_generator)),
            Err(e) => {
                eprintln!(
                    "[MEMORY] Failed to open memory store: {}. Memory disabled.",
                    e
                );
                None
            }
        }
    } else {
        None
    };

    // Spawn a task to forward shell output events to JSONL emitter
    let emitter_for_shell = Arc::new(emitter);
    let emitter_clone = Arc::clone(&emitter_for_shell);
    tokio::spawn(async move {
        eprintln!("[RECV DEBUG] Shell output receiver task started");
        while let Some(event) = shell_rx.recv().await {
            eprintln!(
                "[RECV DEBUG] Received shell output event: stream={}, len={}, done={}",
                event.stream,
                event.text.len(),
                event.done
            );
            let _ = emitter_clone.emit_command_output(
                &event.stream,
                event.text,
                if event.done { Some(true) } else { None },
                event.exit_code,
            );
        }
        eprintln!("[RECV DEBUG] Shell output receiver task ended");
    });
    let emitter = emitter_for_shell;

    // Build messages - load history if provided
    let mut messages: Vec<Message> = Vec::new();

    // Load conversation history if a history file was provided
    if let Some(ref history_path) = args.history {
        if history_path.exists() {
            match std::fs::read_to_string(history_path) {
                Ok(history_json) => {
                    match serde_json::from_str::<Vec<HistoryMessage>>(&history_json) {
                        Ok(history) => {
                            eprintln!(
                                "[HISTORY DEBUG] Loaded {} messages from history",
                                history.len()
                            );
                            // Deduplicate consecutive messages with same role and content
                            let mut last_role = String::new();
                            let mut last_content = String::new();
                            let mut deduped_count = 0;
                            for h in history {
                                // Skip if same role and content as previous (duplicate)
                                if h.role == last_role && h.content == last_content {
                                    deduped_count += 1;
                                    continue;
                                }
                                last_role = h.role.clone();
                                last_content = h.content.clone();

                                let msg = match h.role.as_str() {
                                    "user" => Message::user(h.content),
                                    "assistant" => Message::assistant(h.content),
                                    _ => continue,
                                };
                                messages.push(msg);
                            }
                            if deduped_count > 0 {
                                eprintln!(
                                    "[HISTORY DEBUG] Removed {} duplicate messages",
                                    deduped_count
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("[HISTORY DEBUG] Failed to parse history JSON: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[HISTORY DEBUG] Failed to read history file: {}", e);
                }
            }
        } else {
            eprintln!(
                "[HISTORY DEBUG] History file does not exist: {:?}",
                history_path
            );
        }
    }

    // Add user message
    messages.push(Message::user(prompt.clone()));

    // Memory recall: Search for relevant past conversations and inject into system prompt
    if let Some((ref memory_store, _)) = memory_store {
        match recall::recall_relevant_context(&prompt, memory_store, 3).await {
            Ok(Some(context)) => {
                eprintln!("[MEMORY] Recalled relevant context from past conversations");
                merged_cap.system_prompt.push_str(&context);
            }
            Ok(None) => {
                eprintln!("[MEMORY] No relevant past conversations found");
            }
            Err(e) => {
                eprintln!("[MEMORY] Error recalling context: {}", e);
            }
        }
    }

    // Emit initial status
    emitter.emit_status("thinking", "Processing your request...".to_string(), None)?;

    // Track files changed
    let mut files_changed: Vec<String> = Vec::new();

    // Track tool calls across turns to detect loops
    let mut previous_tool_calls: HashSet<String> = HashSet::new();
    let mut consecutive_repeats = 0;

    // Track if any tools were actually executed (for completion message)
    let mut tools_executed = 0;

    // Main agent loop
    let max_turns = 25;
    for turn_num in 0..max_turns {
        eprintln!("[LOOP] Starting turn {}/{}", turn_num + 1, max_turns);

        // Create completion request using the builder pattern
        let mut request = CompletionRequest::new(model.clone(), messages.clone())
            .with_max_tokens(8192)
            .with_temperature(0.7)
            .with_tools(tool_executor.tool_definitions())
            .with_tool_choice(ToolChoice::Auto);

        if !merged_cap.system_prompt.is_empty() {
            request = request.with_system(merged_cap.system_prompt.clone());
        }

        // Get streaming completion
        let mut stream = provider.complete_stream(request).await?;

        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        let mut current_text = String::new();
        let mut buffered_text = String::new(); // Buffer text that might be JSON tool calls
        let mut might_be_tool_call = false; // Track if we're buffering potential tool call JSON
        let mut tool_uses: Vec<(String, String, serde_json::Value)> = Vec::new();

        // Track current tool use being built (for streaming JSON input)
        let mut current_tool_id: Option<String> = None;
        let mut current_tool_name: Option<String> = None;
        let mut current_tool_input_json = String::new();

        // For Ollama, we need to buffer text because it outputs JSON tool calls as text
        let is_ollama = provider_name == "ollama";

        // Process stream
        while let Some(event_result) = stream.next().await {
            match event_result? {
                StreamEvent::MessageStart { .. } => {
                    // Message started
                }
                StreamEvent::ContentBlockStart { content_block, .. } => {
                    match content_block {
                        ContentBlockResponse::Text { text } => {
                            if !text.is_empty() {
                                current_text.push_str(&text);

                                // For Ollama, buffer text that might be JSON
                                if is_ollama {
                                    buffered_text.push_str(&text);
                                    let trimmed = buffered_text.trim_start();
                                    if trimmed.starts_with('{') || trimmed.starts_with("```") {
                                        might_be_tool_call = true;
                                    }
                                    // Only stream if it doesn't look like a tool call
                                    if !might_be_tool_call {
                                        emitter.emit_message("assistant", text, Some(true))?;
                                    }
                                } else {
                                    emitter.emit_message("assistant", text, Some(true))?;
                                }
                            }
                        }
                        ContentBlockResponse::ToolUse { id, name, input } => {
                            // Tool use started - may have empty input initially (for streaming)
                            current_tool_id = Some(id.clone());
                            current_tool_name = Some(name.clone());
                            // If input is not empty/null, use it; otherwise start empty for streaming
                            let input_str = input.to_string();
                            if input_str == "{}" || input_str == "null" {
                                current_tool_input_json = String::new();
                            } else {
                                current_tool_input_json = input_str;
                            }
                        }
                    }
                }
                StreamEvent::ContentBlockDelta { delta, .. } => {
                    match delta {
                        ContentBlockDelta::TextDelta { text } => {
                            current_text.push_str(&text);

                            // For Ollama, buffer text that might be JSON
                            if is_ollama {
                                buffered_text.push_str(&text);
                                if !might_be_tool_call {
                                    let trimmed = buffered_text.trim_start();
                                    if trimmed.starts_with('{') || trimmed.starts_with("```") {
                                        might_be_tool_call = true;
                                    }
                                }
                                // Only stream if it doesn't look like a tool call
                                if !might_be_tool_call {
                                    emitter.emit_message("assistant", text, Some(true))?;
                                }
                            } else {
                                emitter.emit_message("assistant", text, Some(true))?;
                            }
                        }
                        ContentBlockDelta::InputJsonDelta { partial_json } => {
                            // Accumulate partial JSON for tool input
                            current_tool_input_json.push_str(&partial_json);
                        }
                    }
                }
                StreamEvent::ContentBlockStop { .. } => {
                    // Finalize tool use if we were building one
                    // NOTE: We collect tool uses here but DON'T emit events yet
                    // Events are emitted after enforcement check to avoid showing
                    // tools that will be rejected on first turn
                    if let (Some(id), Some(name)) =
                        (current_tool_id.take(), current_tool_name.take())
                    {
                        // Parse the accumulated JSON
                        let input: serde_json::Value = if current_tool_input_json.is_empty() {
                            serde_json::json!({})
                        } else {
                            serde_json::from_str(&current_tool_input_json)
                                .unwrap_or(serde_json::json!({}))
                        };

                        // Apply the same mapping as for text-extracted tools
                        // This handles different parameter names from different models
                        if let Some((mapped_name, mapped_input)) =
                            parse_tool_from_json(&serde_json::json!({
                                "name": name,
                                "arguments": input
                            }))
                        {
                            tool_uses.push((id, mapped_name, mapped_input));
                        } else {
                            // Fallback if parsing fails
                            tool_uses.push((id, name, input));
                        }
                        current_tool_input_json.clear();
                    }
                }
                StreamEvent::MessageDelta { .. } => {
                    // Usage/stop reason update
                }
                StreamEvent::MessageStop => {
                    break;
                }
                StreamEvent::Ping => {
                    // Keep-alive
                }
                StreamEvent::Error {
                    error_type,
                    message,
                } => {
                    emitter.emit_error(error_type.clone(), message.clone(), None, None)?;
                    return Err(TedError::Config(format!(
                        "LLM error - {}: {}",
                        error_type, message
                    )));
                }
            }
        }

        // If we buffered text thinking it was a tool call but got no tool uses,
        // try to parse JSON tool calls from the text (Ollama often outputs them as markdown)
        if is_ollama && might_be_tool_call && tool_uses.is_empty() && !buffered_text.is_empty() {
            eprintln!(
                "[TOOL PARSE] Attempting to extract tools from buffered text ({} chars)",
                buffered_text.len()
            );
            eprintln!(
                "[TOOL PARSE] Buffered text preview: {}",
                &buffered_text[..buffered_text.len().min(500)]
            );

            // Try to extract JSON tool calls from markdown code blocks
            let extracted_tools = extract_json_tool_calls(&buffered_text);
            if !extracted_tools.is_empty() {
                eprintln!(
                    "[TOOL PARSE] Extracted {} tool calls from text",
                    extracted_tools.len()
                );
                for (name, input) in extracted_tools {
                    let id = uuid::Uuid::new_v4().to_string();
                    tool_uses.push((id, name, input));
                }
            } else {
                eprintln!("[TOOL PARSE] No tools extracted, emitting as message");
                // No tool calls found, emit the text as a message
                emitter.emit_message("assistant", buffered_text.clone(), Some(false))?;
            }
        }

        // Debug: Log if we have empty response
        if tool_uses.is_empty() && current_text.trim().is_empty() {
            eprintln!("[DEBUG] Empty response from model - no tools and no text");
            eprintln!(
                "[DEBUG] might_be_tool_call={}, buffered_text.len()={}",
                might_be_tool_call,
                buffered_text.len()
            );
        }

        // Add text content if any
        // For Ollama: if we detected tool uses and buffered text (meaning the text was JSON tool calls),
        // don't include the text in the message - it was just the JSON representation
        let should_include_text = if is_ollama && might_be_tool_call && !tool_uses.is_empty() {
            false // Text was JSON tool call output, don't include it
        } else {
            !current_text.is_empty()
        };

        if should_include_text {
            assistant_blocks.push(ContentBlock::Text {
                text: current_text.clone(),
            });
        }

        // Add tool uses to content
        for (id, name, input) in &tool_uses {
            assistant_blocks.push(ContentBlock::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            });
        }

        // Add assistant message
        if !assistant_blocks.is_empty() {
            messages.push(Message::assistant_blocks(assistant_blocks));
        }

        // If no tool uses, we're done - but emit history first
        if tool_uses.is_empty() {
            if let Err(e) = emitter.emit_conversation_history(extract_history_messages(&messages)) {
                eprintln!("[HISTORY] Failed to emit final history: {}", e);
            }
            break;
        }

        // Check for tool call loops - if the model is repeating the exact same calls
        let current_tool_keys: HashSet<String> = tool_uses
            .iter()
            .map(|(_, name, input)| tool_call_key(name, input))
            .collect();

        // Check if ALL current tool calls were seen in the previous turn
        let all_repeated = !current_tool_keys.is_empty()
            && current_tool_keys
                .iter()
                .all(|k| previous_tool_calls.contains(k));

        if all_repeated {
            consecutive_repeats += 1;
            if consecutive_repeats >= 3 {
                // Model is stuck in a loop - emit history before breaking
                if let Err(e) =
                    emitter.emit_conversation_history(extract_history_messages(&messages))
                {
                    eprintln!("[HISTORY] Failed to emit loop history: {}", e);
                }

                emitter.emit_error(
                    "loop_detected".to_string(),
                    "Model is repeating the same tool calls. Breaking loop.".to_string(),
                    Some(
                        "The model may need clearer instructions or a different approach."
                            .to_string(),
                    ),
                    None,
                )?;
                break;
            }
        } else {
            consecutive_repeats = 0;
        }

        // Update previous tool calls for next iteration
        previous_tool_calls = current_tool_keys;

        // Execute tools and collect results
        emitter.emit_status(
            "running",
            format!("Executing {} tool(s)...", tool_uses.len()),
            None,
        )?;

        // Log which tools are being executed
        for (_, name, _) in &tool_uses {
            eprintln!("[TOOL EXEC] Executing tool: {}", name);
        }

        // Emit tool preview events for file operations
        for (_id, name, input) in &tool_uses {
            let name_lower = name.to_lowercase();
            // Handle file write tools (including aliases: write, write_file, file_write)
            let is_file_write = name_lower == "file_write"
                || name_lower == "write"
                || name_lower == "write_file"
                || name_lower == "create_file";
            let is_file_edit =
                name_lower == "file_edit" || name_lower == "edit" || name_lower == "edit_file";

            // Helper to get parameter with fallback names
            fn get_param<'a>(input: &'a serde_json::Value, names: &[&str]) -> Option<&'a str> {
                for name in names {
                    if let Some(val) = input.get(*name).and_then(|v| v.as_str()) {
                        return Some(val);
                    }
                }
                None
            }

            if is_file_write {
                let path = get_param(input, &["path", "file", "file_path", "filepath"]);
                let content = get_param(input, &["content", "text", "body", "data"]);
                if let (Some(path), Some(content)) = (path, content) {
                    emitter.emit_file_create(path.to_string(), content.to_string(), None)?;
                    if !files_changed.contains(&path.to_string()) {
                        files_changed.push(path.to_string());
                    }
                }
            } else if is_file_edit {
                let path = get_param(input, &["path", "file", "file_path", "filepath"]);
                if let Some(path) = path {
                    let old_text = get_param(
                        input,
                        &[
                            "old_string",
                            "old",
                            "old_text",
                            "search",
                            "find",
                            "original",
                            "from",
                        ],
                    );
                    let new_text = get_param(
                        input,
                        &[
                            "new_string",
                            "new",
                            "new_text",
                            "replace",
                            "replacement",
                            "to",
                        ],
                    );
                    emitter.emit_file_edit(
                        path.to_string(),
                        "replace".to_string(),
                        old_text.map(|s| s.to_string()),
                        new_text.map(|s| s.to_string()),
                        None,
                        None,
                    )?;
                    if !files_changed.contains(&path.to_string()) {
                        files_changed.push(path.to_string());
                    }
                }
            } else if name_lower == "shell" {
                if let Some(command) = input.get("command").and_then(|v| v.as_str()) {
                    emitter.emit_command(command.to_string(), None, None)?;
                }
            } else if name_lower == "propose_file_changes" {
                // Extract operations from the changeset
                if let Some(operations) = input.get("operations").and_then(|v| v.as_array()) {
                    for op in operations {
                        if let Some(op_type) = op.get("type").and_then(|v| v.as_str()) {
                            // Flexible path lookup for changeset operations
                            let path = get_param(op, &["path", "file", "file_path", "filepath"]);
                            if let Some(path) = path {
                                match op_type {
                                    "edit" => {
                                        let old_text = get_param(
                                            op,
                                            &[
                                                "old_string",
                                                "old",
                                                "old_text",
                                                "search",
                                                "find",
                                                "original",
                                                "from",
                                            ],
                                        );
                                        let new_text = get_param(
                                            op,
                                            &[
                                                "new_string",
                                                "new",
                                                "new_text",
                                                "replace",
                                                "replacement",
                                                "to",
                                            ],
                                        );
                                        emitter.emit_file_edit(
                                            path.to_string(),
                                            "replace".to_string(),
                                            old_text.map(|s| s.to_string()),
                                            new_text.map(|s| s.to_string()),
                                            None,
                                            None,
                                        )?;
                                        if !files_changed.contains(&path.to_string()) {
                                            files_changed.push(path.to_string());
                                        }
                                    }
                                    "write" => {
                                        let content =
                                            get_param(op, &["content", "text", "body", "data"]);
                                        if let Some(content) = content {
                                            emitter.emit_file_create(
                                                path.to_string(),
                                                content.to_string(),
                                                None,
                                            )?;
                                            if !files_changed.contains(&path.to_string()) {
                                                files_changed.push(path.to_string());
                                            }
                                        }
                                    }
                                    "delete" => {
                                        // Emit file deletion event (we'll need to add this to emitter if not exists)
                                        if !files_changed.contains(&path.to_string()) {
                                            files_changed.push(path.to_string());
                                        }
                                    }
                                    "read" => {
                                        // Read operations don't change files
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            } else if name_lower == "plan_update" {
                if let Some(content) = input.get("content").and_then(|v| v.as_str()) {
                    let plan_steps: Vec<PlanStep> = content
                        .lines()
                        .enumerate()
                        .filter_map(|(i, line)| {
                            let trimmed = line.trim();
                            if trimmed.starts_with("- [ ]") || trimmed.starts_with("- [x]") {
                                let desc = trimmed
                                    .trim_start_matches("- [ ]")
                                    .trim_start_matches("- [x]")
                                    .trim();
                                if !desc.is_empty() {
                                    Some(PlanStep {
                                        id: (i + 1).to_string(),
                                        description: desc.to_string(),
                                        estimated_files: None,
                                    })
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .collect();

                    if !plan_steps.is_empty() {
                        let title = input
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Plan");
                        emitter.emit_status(
                            "planning",
                            format!("Created plan: {}", title),
                            None,
                        )?;
                        emitter.emit_plan(plan_steps)?;
                    }
                }
            }
            // Other tool types don't need special event emission
        }

        // Debug: Log tool calls being executed
        #[cfg(debug_assertions)]
        for (id, name, input) in &tool_uses {
            eprintln!(
                "[DEBUG] Executing tool: {} (id: {}) with input: {}",
                name, id, input
            );
        }

        let mut tool_result_blocks: Vec<ContentBlock> = Vec::new();

        // File modification tools that should be skipped in review mode
        let file_mod_tools = [
            "file_write",
            "file_edit",
            "file_delete",
            "create_file",
            "edit_file",
            "delete_file",
        ];

        // Track if we made file mods in review mode - we'll exit after this turn
        let mut has_review_file_mods = false;

        for (id, name, input) in tool_uses {
            let name_lower = name.to_lowercase();
            let is_file_mod = file_mod_tools.iter().any(|t| name_lower.contains(t));

            // In review mode, skip file modifications but return success
            // The events have already been emitted, so the frontend can show them for review
            if review_mode && is_file_mod {
                eprintln!(
                    "[REVIEW MODE] Skipping execution of file tool: {} (events already emitted)",
                    name
                );
                tools_executed += 1;
                has_review_file_mods = true;

                // Return a mock success result so the model knows the "change was applied"
                let mock_result = match name.as_str() {
                    "file_write" | "create_file" => {
                        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("file");
                        format!("Successfully created {} (pending review)", path)
                    }
                    "file_edit" | "edit_file" => {
                        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("file");
                        format!("Successfully edited {} (pending review)", path)
                    }
                    "file_delete" | "delete_file" => {
                        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("file");
                        format!("Successfully deleted {} (pending review)", path)
                    }
                    _ => "Operation completed (pending review)".to_string(),
                };

                tool_result_blocks.push(ContentBlock::ToolResult {
                    tool_use_id: id,
                    content: crate::llm::message::ToolResultContent::Text(mock_result),
                    is_error: None,
                });
                continue;
            }

            let result = tool_executor
                .execute_tool_use(&id, &name, input.clone())
                .await?;

            tools_executed += 1;

            #[cfg(debug_assertions)]
            eprintln!(
                "[DEBUG] Tool result - is_error: {}, output: {}",
                result.is_error(),
                result.output_text()
            );

            tool_result_blocks.push(ContentBlock::ToolResult {
                tool_use_id: id,
                content: crate::llm::message::ToolResultContent::Text(
                    result.output_text().to_string(),
                ),
                is_error: if result.is_error() { Some(true) } else { None },
            });
        }

        // Add tool results as user message
        messages.push(Message {
            id: uuid::Uuid::new_v4(),
            role: crate::llm::message::Role::User,
            content: MessageContent::Blocks(tool_result_blocks),
            timestamp: chrono::Utc::now(),
            tool_use_id: None,
            token_count: None,
        });

        // Emit conversation history after each turn for multi-turn persistence
        // This ensures that even if Ted is killed mid-conversation, the frontend has the latest history
        if let Err(e) = emitter.emit_conversation_history(extract_history_messages(&messages)) {
            eprintln!("[HISTORY] Failed to emit turn history: {}", e);
        }

        // In review mode, if we made file modifications, EXIT the loop
        // The user needs to review and accept/reject before we continue
        if has_review_file_mods {
            eprintln!(
                "[REVIEW MODE] File modifications pending review. Exiting loop to wait for user."
            );
            break;
        }

        eprintln!("[LOOP] End of turn. tools_executed={}, files_changed={:?}. Continuing to next LLM call...", tools_executed, files_changed);
    }

    // Emit completion - only if we actually did something meaningful
    let completion_message = if !files_changed.is_empty() {
        format!("Modified {} file(s)", files_changed.len())
    } else if tools_executed > 0 {
        format!("Executed {} tool(s)", tools_executed)
    } else {
        // No tools executed - model is likely asking questions or waiting for input
        "Waiting for your response".to_string()
    };

    // Only mark as "successful completion" if actual work was done
    // "Waiting for response" is not a failure, but not a task completion either
    let is_task_complete = tools_executed > 0 || !files_changed.is_empty();

    // Emit final conversation history for multi-turn persistence
    emitter.emit_conversation_history(extract_history_messages(&messages))?;

    emitter.emit_completion(
        is_task_complete,
        completion_message.clone(),
        files_changed.clone(),
    )?;

    // Store conversation in memory (if enabled)
    if let Some((ref memory_store, ref embedding_generator)) = memory_store {
        eprintln!("[MEMORY] Storing conversation in memory...");

        // Generate summary
        let summary = match summarizer::summarize_conversation(&messages, provider.as_ref()).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[MEMORY] Failed to generate summary: {}", e);
                format!(
                    "{} - {}",
                    prompt.chars().take(100).collect::<String>(),
                    completion_message
                )
            }
        };

        // Extract metadata
        let files = summarizer::extract_files_changed(&messages);
        let tags = summarizer::extract_tags(&messages);

        // Format full content
        let content = messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    crate::llm::message::Role::User => "User",
                    crate::llm::message::Role::Assistant => "Assistant",
                    crate::llm::message::Role::System => "System",
                };
                match &m.content {
                    MessageContent::Text(t) => format!("{}: {}", role, t),
                    MessageContent::Blocks(blocks) => {
                        let text_parts: Vec<String> = blocks
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::Text { text } => Some(text.clone()),
                                ContentBlock::ToolUse { name, .. } => {
                                    Some(format!("[tool: {}]", name))
                                }
                                _ => None,
                            })
                            .collect();
                        format!("{}: {}", role, text_parts.join(" "))
                    }
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Store in memory
        if let Err(e) = recall::store_conversation(
            session_id,
            summary.clone(),
            files,
            tags,
            content,
            embedding_generator,
            memory_store,
        )
        .await
        {
            eprintln!("[MEMORY] Failed to store conversation: {}", e);
        } else {
            eprintln!(
                "[MEMORY] Conversation stored successfully. Summary: {}",
                summary
            );
        }
    }

    Ok(())
}
