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
use crate::embedded::{HistoryMessageData, JsonLEmitter, PlanStep};
use crate::error::{Result, TedError};
use crate::llm::message::{ContentBlock, Message, MessageContent};
use crate::llm::provider::{
    CompletionRequest, ContentBlockDelta, ContentBlockResponse, LlmProvider, StreamEvent,
    ToolChoice,
};
use crate::llm::providers::{AnthropicProvider, OllamaProvider, OpenRouterProvider};
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
                MessageContent::Blocks(blocks) => {
                    blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("")
                }
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
            eprintln!("[TOOL PARSE] Found JSON block ({} chars): {}", json_text.len(), &json_text[..json_text.len().min(200)]);

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_text)
            {
                eprintln!("[TOOL PARSE] Successfully parsed JSON");
                if let Some(tool) = parse_tool_from_json(&parsed) {
                    eprintln!("[TOOL PARSE] Extracted tool: {}", tool.0);
                    tools.push(tool);
                }
            } else {
                eprintln!("[TOOL PARSE] Failed to parse JSON block");
            }
        }
    }

    // Pattern 2: Try to find JSON objects by scanning for balanced braces
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
                if json_str.contains("\"name\"") && (json_str.contains("\"arguments\"") || json_str.contains("\"input\"")) {
                    eprintln!("[TOOL PARSE] Found potential tool JSON ({} chars)", json_str.len());

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
    let arguments = obj.get("arguments")
        .or_else(|| obj.get("input"))
        .cloned()?;

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

    // Map argument names for file_edit: some models use old_text/new_text instead of old_string/new_string
    let mapped_arguments = if mapped_name == "file_edit" {
        map_file_edit_arguments(&arguments)
    } else if mapped_name == "file_write" {
        map_file_write_arguments(&arguments)
    } else {
        arguments
    };

    eprintln!("[TOOL PARSE] Parsed tool: {} -> {}", name, mapped_name);
    Some((mapped_name.to_string(), mapped_arguments))
}

/// Map file_edit argument names from various LLM output formats to our expected format
fn map_file_edit_arguments(args: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = args.as_object() else {
        return args.clone();
    };

    let mut mapped = serde_json::Map::new();

    for (key, value) in obj {
        let mapped_key = match key.as_str() {
            // Map various names to our expected format
            "old_text" | "oldText" | "find" | "search" | "original" => "old_string",
            "new_text" | "newText" | "replace" | "replacement" | "modified" => "new_string",
            "file" | "file_path" | "filepath" | "filename" => "path",
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
            "file" | "file_path" | "filepath" | "filename" => "path",
            "text" | "data" | "contents" => "content",
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
    let merged_cap = resolver.resolve_and_merge(&cap_names)?;

    // Determine model
    let model = args
        .model
        .clone()
        .or_else(|| merged_cap.preferred_model().map(|s| s.to_string()))
        .unwrap_or_else(|| match provider_name.as_str() {
            "ollama" => settings.providers.ollama.default_model.clone(),
            "openrouter" => settings.providers.openrouter.default_model.clone(),
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
    .with_shell_output_sender(shell_tx);
    let mut tool_executor = ToolExecutor::new(tool_context, args.trust);

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
                            for h in history {
                                let msg = match h.role.as_str() {
                                    "user" => Message::user(h.content),
                                    "assistant" => Message::assistant(h.content),
                                    _ => continue,
                                };
                                messages.push(msg);
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

    // Track if we loaded history (this affects enforcement behavior)
    let has_history = !messages.is_empty();

    // Add user message
    messages.push(Message::user(prompt.clone()));

    // Emit initial status
    emitter.emit_status("thinking", "Processing your request...".to_string(), None)?;

    // Track files changed
    let mut files_changed: Vec<String> = Vec::new();

    // Track tool calls across turns to detect loops
    let mut previous_tool_calls: HashSet<String> = HashSet::new();
    let mut consecutive_repeats = 0;

    // Track if this is the first turn (model hasn't responded yet in this conversation)
    // If we loaded history, this is NOT the first turn - user already had a conversation
    let mut is_first_turn = !has_history;

    // Track if any tools were actually executed (for completion message)
    let mut tools_executed = 0;

    // Track if the model has explored the codebase (used read-only tools like glob/read_file)
    // If exploration happened, we expect the model to make edits if the user request implies existing content
    let mut has_explored = false;

    // Track if the model has made any edits (used write tools like file_edit/file_write)
    // Once edits are made, we should allow the model to respond with a summary without forcing more edits
    let mut has_made_edits = false;

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

                        tool_uses.push((id, name, input));
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
            eprintln!("[TOOL PARSE] Attempting to extract tools from buffered text ({} chars)", buffered_text.len());
            eprintln!("[TOOL PARSE] Buffered text preview: {}", &buffered_text[..buffered_text.len().min(500)]);

            // Try to extract JSON tool calls from markdown code blocks
            let extracted_tools = extract_json_tool_calls(&buffered_text);
            if !extracted_tools.is_empty() {
                eprintln!("[TOOL PARSE] Extracted {} tool calls from text", extracted_tools.len());
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
            eprintln!("[DEBUG] might_be_tool_call={}, buffered_text.len()={}", might_be_tool_call, buffered_text.len());
        }

        // Add text content if any
        // For Ollama: if we detected tool uses and buffered text (meaning the text was JSON tool calls),
        // don't include the text in the message - it was just the JSON representation
        let should_include_text = if is_ollama && might_be_tool_call && !tool_uses.is_empty() {
            false // Text was JSON tool call output, don't include it
        } else {
            !current_text.is_empty()
        };

        // ENFORCEMENT: On first turn with Ollama, enforce appropriate behavior based on tools used
        // Read-only tools (glob, grep, read_file, list_directory) are allowed for exploration
        // Write tools (file_create, file_edit, file_delete, shell) require asking questions first
        // EXCEPTION: If user request implies EXISTING content (modify, adjust, change, fix, update, add to)
        //            then model should explore first, not ask questions
        let user_text_lower = prompt.to_lowercase();
        let implies_existing = user_text_lower.contains("adjust")
            || user_text_lower.contains("modify")
            || user_text_lower.contains("change")
            || user_text_lower.contains("fix")
            || user_text_lower.contains("update")
            || user_text_lower.contains("add")
            || user_text_lower.contains("remove")
            || user_text_lower.contains("edit")
            || user_text_lower.contains("tweak")
            || user_text_lower.contains("move")
            || user_text_lower.contains("the header")
            || user_text_lower.contains("the button")
            || user_text_lower.contains("the game")
            || user_text_lower.contains("the app")
            || user_text_lower.contains("the page")
            || user_text_lower.contains("the site")
            || user_text_lower.contains("the website")
            || user_text_lower.contains("the style")
            || user_text_lower.contains("the css")
            || user_text_lower.contains("the color")
            || user_text_lower.contains("my app")
            || user_text_lower.contains("my game")
            || user_text_lower.contains("my page")
            || user_text_lower.contains("my site")
            || user_text_lower.contains("my project")
            || user_text_lower.contains("isnt updating")
            || user_text_lower.contains("isn't updating")
            || user_text_lower.contains("not updating")
            || user_text_lower.contains("this file")
            || user_text_lower.contains("this page")
            || user_text_lower.contains("colorful")
            || user_text_lower.contains("more color")
            || user_text_lower.contains("why is")  // debugging questions imply existing content
            || user_text_lower.contains("why does")
            || user_text_lower.contains("why doesn't")
            || user_text_lower.contains("why isn't")
            || user_text_lower.contains("not working")
            || user_text_lower.contains("not showing")
            || user_text_lower.contains("congrats") // "make it say congrats when..."
            || user_text_lower.contains("when someone wins")
            || user_text_lower.contains("stylish")  // "make it more stylish"
            || user_text_lower.contains("modern")   // "make it more modern"
            || user_text_lower.contains("look better")
            || user_text_lower.contains("look nicer")
            || user_text_lower.contains("more attractive");

        eprintln!("[ENFORCEMENT DEBUG] is_first_turn={}, is_ollama={}, tool_uses.len()={}, has_history={}, implies_existing={}, has_explored={}, has_made_edits={}",
            is_first_turn, is_ollama, tool_uses.len(), has_history, implies_existing, has_explored, has_made_edits);

        // First turn enforcement for Ollama models
        if is_first_turn && is_ollama {
            if !tool_uses.is_empty() {
                // Model used tools - check if they're appropriate
                // Categorize tools: read-only exploration vs write operations
                // tool_uses is Vec<(id, name, input)>
                let read_only_tools = ["glob", "grep", "read_file", "list_directory", "search"];
                let has_write_tools = tool_uses.iter().any(|(_, name, _)| {
                    let name_lower = name.to_lowercase();
                    !read_only_tools.iter().any(|ro| name_lower.contains(ro))
                });
                let only_read_tools = !has_write_tools;

                // If only using read-only tools, that's good! Model is exploring the codebase.
                if only_read_tools {
                    eprintln!("[ENFORCEMENT] First turn: Model is exploring codebase with read-only tools. Allowing.");
                    has_explored = true;
                } else if implies_existing {
                    // User request implies existing content, but model tried to write without exploring first!
                    // Model MUST explore first to understand the existing codebase before making changes.
                    eprintln!("[ENFORCEMENT] First turn: User wants to modify existing content, but model skipped exploration. Rejecting.");

                    // Tell the model to explore first
                    let clarification_message = "STOP! You tried to modify files without first understanding the existing codebase.\n\n\
                        The user wants to modify EXISTING content. Before making changes, you MUST:\n\
                        1. Use glob(\"*\") to see what files exist in the current directory\n\
                        2. Use read_file to examine the relevant files (look for index.html, main.py, etc.)\n\
                        3. THEN make targeted changes that fit the existing codebase\n\n\
                        Do NOT guess file paths or content. Start by exploring with glob(\"*\").";

                    messages.push(Message::user(clarification_message.to_string()));

                    is_first_turn = false;
                    continue; // Skip - get model's next response
                } else {
                    // Model tried to write without exploring or asking - check if it asked questions
                    let text_before_tools = current_text.trim();
                    let asked_questions = text_before_tools.contains('?') && text_before_tools.len() > 20;

                    if !asked_questions {
                        // Model jumped straight to write tools without asking - reject
                        eprintln!("[ENFORCEMENT] First turn: Model used write tools without asking questions. Rejecting.");

                        // Tell the model to explore first
                        let clarification_message = "STOP! You tried to modify files without first understanding the existing codebase.\n\n\
                            Before making changes, you MUST:\n\
                            1. Use glob to see what files exist\n\
                            2. Use read_file to examine the relevant files\n\
                            3. Then make targeted changes that fit the existing codebase\n\n\
                            Start by exploring the current directory with glob.";

                        messages.push(Message::user(clarification_message.to_string()));

                        is_first_turn = false;
                        continue; // Skip - get model's next response
                    }
                }
            } else if implies_existing {
                // Model used NO tools but the request implies existing content
                // The model should be exploring, not giving generic advice!
                eprintln!("[ENFORCEMENT] First turn: User request implies existing content but model gave no tools. Forcing exploration.");

                let clarification_message = "STOP! The user is asking about EXISTING content in this project, but you didn't explore the codebase.\n\n\
                    You MUST use tools to understand what exists before responding:\n\
                    1. Use glob(\"*\") to see what files exist in the current directory\n\
                    2. Use read_file to examine the relevant files\n\
                    3. THEN provide a response based on the ACTUAL content\n\n\
                    Do NOT give generic advice. Start by exploring with glob(\"*\").";

                messages.push(Message::user(clarification_message.to_string()));

                is_first_turn = false;
                continue; // Skip - get model's next response
            }
        }

        // ENFORCEMENT: After first turn, if user confirms they want something built,
        // model MUST use tools - not just give instructions
        if !is_first_turn && is_ollama && tool_uses.is_empty() {
            let user_text = prompt.to_lowercase();
            let assistant_text = current_text.to_lowercase();

            // Check if user is confirming they want something built
            // This includes:
            // 1. Explicit confirmation keywords
            // 2. Answering clarifying questions (implies "proceed with my preferences")
            // 3. Requests that imply modifying existing content
            let user_wants_build = user_text.contains("build")
                || user_text.contains("create")
                || user_text.contains("make")
                || user_text.contains("yes")
                || user_text.contains("go ahead")
                || user_text.contains("let's do")
                || user_text.contains("start")
                || user_text.contains("please")
                || user_text.contains("do it")
                || user_text.contains("proceed")
                || user_text.contains("sounds good")
                || user_text.contains("that works")
                || user_text.contains("perfect")
                || user_text.contains("great")
                || user_text.contains("ok")
                || user_text.contains("sure")
                // User is answering questions about preferences (implies they want to proceed)
                || user_text.contains("minimal")
                || user_text.contains("simple")
                || user_text.contains("modern")
                || user_text.contains("clean")
                || user_text.contains("no registration")
                || user_text.contains("no preference")
                || user_text.contains("whatever")
                || user_text.contains("anything")
                || user_text.contains("up to you")
                || user_text.contains("you decide")
                || user_text.contains("your choice")
                // User request implies modifying existing content
                || implies_existing;

            // Check if model is giving instructions instead of building
            // Model should be using tools, not explaining steps or just outputting plans
            let giving_instructions = assistant_text.contains("you need to")
                || assistant_text.contains("you can")
                || assistant_text.contains("you'll need")
                || assistant_text.contains("first, install")
                || assistant_text.contains("install ruby")
                || assistant_text.contains("install jekyll")
                || assistant_text.contains("install hugo")
                || assistant_text.contains("download")
                || assistant_text.contains("follow the")
                || assistant_text.contains("step 1")
                || assistant_text.contains("### step")
                || assistant_text.contains("here's how")
                || assistant_text.contains("here are the steps")
                || assistant_text.contains("let's proceed")
                || assistant_text.contains("how we can proceed")
                // Catch when model outputs a plan but doesn't actually build
                || assistant_text.contains("here's a plan")
                || assistant_text.contains("#### tasks")
                || assistant_text.contains("### plan")
                || assistant_text.contains("- [ ]") // Task list without execution
                // Catch when model asks for confirmation instead of just building
                || assistant_text.contains("should i create")
                || assistant_text.contains("should i start")
                || assistant_text.contains("would you like me to")
                || assistant_text.contains("do you want me to")
                || assistant_text.contains("shall i")
                || assistant_text.contains("ready to start")
                || assistant_text.contains("let me know when")
                // Catch when model asks clarifying questions instead of working on existing files
                || (implies_existing && assistant_text.contains("clarifying questions"))
                || (implies_existing && assistant_text.contains("what style"))
                || (implies_existing && assistant_text.contains("what features"))
                || (implies_existing && assistant_text.contains("who is the intended"));

            eprintln!("[ENFORCEMENT DEBUG] Build enforcement: user_wants_build={}, giving_instructions={}",
                user_wants_build, giving_instructions);
            eprintln!("[ENFORCEMENT DEBUG] User text: {}", user_text);
            eprintln!(
                "[ENFORCEMENT DEBUG] Assistant text (first 200): {}",
                &assistant_text[..assistant_text.len().min(200)]
            );

            // Only enforce if model hasn't already made edits - allow summary responses after edits
            if user_wants_build && giving_instructions && !has_made_edits {
                eprintln!("[ENFORCEMENT] User wants to build but model is giving instructions. Forcing tool use.");

                let build_message = if implies_existing {
                    "STOP! The user wants to MODIFY EXISTING CODE. Do NOT ask clarifying questions.\n\n\
                    You MUST:\n\
                    1. Use glob to see what files exist in the project\n\
                    2. Use read_file to examine the relevant files (look for HTML, JS, CSS files)\n\
                    3. Use file_edit to make the specific changes the user requested\n\n\
                    The project already exists - explore it and make the changes NOW."
                } else {
                    "STOP! The user has already given you the information you need.\n\n\
                    Do NOT ask for more confirmation. Do NOT ask 'should I create...?' or 'would you like...?'\n\n\
                    You MUST start building NOW using your tools:\n\
                    1. Use file_write to create the project files\n\
                    2. Create index.html, styles.css, and any needed JS files\n\
                    3. Actually BUILD it - the user wants results, not questions\n\n\
                    START CREATING FILES NOW with file_write."
                };

                messages.push(Message::user(build_message.to_string()));
                continue; // Force model to try again with tools
            }

            // ENFORCEMENT: Model explored (used read-only tools) but now responds with no tools
            // If the user request implies existing content and model hasn't made edits yet, force edits
            // BUT if model has already made edits, allow it to respond with a summary
            eprintln!("[ENFORCEMENT DEBUG] Post-exploration check: has_explored={}, implies_existing={}, has_made_edits={}",
                has_explored, implies_existing, has_made_edits);
            if has_explored && implies_existing && !has_made_edits {
                eprintln!("[ENFORCEMENT] Model explored but now responds with no edits. Forcing file_edit.");

                let edit_message = "STOP! You already explored the codebase and found the files. Now you MUST make the changes.\n\n\
                    Do NOT explain what you found. Do NOT describe what you would do.\n\
                    Use file_edit NOW to make the specific changes the user requested.\n\n\
                    The user wants you to MODIFY the existing files, not describe them.";

                messages.push(Message::user(edit_message.to_string()));
                continue; // Force model to try again with tools
            }
        }

        // ENFORCEMENT: Model is using tools but ONLY read-only tools even after exploring
        // If the request implies existing content and model keeps exploring without editing, force edits
        if !is_first_turn && is_ollama && has_explored && implies_existing && !has_made_edits && !tool_uses.is_empty() {
            // Check if current turn uses ONLY read-only tools (no write tools)
            let read_only_tools = ["glob", "grep", "read_file", "list_directory", "search"];
            let write_tools = ["file_write", "file_edit", "file_delete", "create_file", "edit_file", "delete_file", "shell"];
            let has_write_tool = tool_uses.iter().any(|(_, name, _)| {
                let name_lower = name.to_lowercase();
                write_tools.iter().any(|wt| name_lower.contains(wt))
            });
            let only_read_tools = tool_uses.iter().all(|(_, name, _)| {
                let name_lower = name.to_lowercase();
                read_only_tools.iter().any(|ro| name_lower.contains(ro))
            });

            // If model is giving instructions while only using read tools (not making changes)
            let is_giving_instructions = current_text.contains("you might consider")
                || current_text.contains("here's how")
                || current_text.contains("you can approach")
                || current_text.contains("you could")
                || current_text.contains("I recommend")
                || current_text.contains("I suggest")
                || current_text.contains("steps to")
                || current_text.contains("1. Open")
                || current_text.contains("1. Replace");

            if only_read_tools && !has_write_tool && is_giving_instructions {
                eprintln!("[ENFORCEMENT] Model is giving instructions while still exploring. Forcing immediate edits.");

                let edit_message = "STOP! Do NOT give instructions. The user wants you to make changes, not describe how they could do it themselves.\n\n\
                    You've already read the files. Now use file_edit to make the changes yourself.\n\n\
                    Do it NOW - call file_edit with the specific changes.";

                messages.push(Message::user(edit_message.to_string()));
                continue;
            }
        }

        is_first_turn = false;

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
                if let Err(e) = emitter.emit_conversation_history(extract_history_messages(&messages)) {
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

        // Track if this turn includes exploration (read-only tools)
        let read_only_tools = ["glob", "grep", "read_file", "list_directory", "search"];
        let turn_has_exploration = tool_uses.iter().any(|(_, name, _)| {
            let name_lower = name.to_lowercase();
            read_only_tools.iter().any(|ro| name_lower.contains(ro))
        });

        // Log which tools are being executed
        for (_, name, _) in &tool_uses {
            eprintln!("[TOOL EXEC] Executing tool: {}", name);
        }

        if turn_has_exploration {
            eprintln!("[TOOL EXEC] This turn includes exploration tools, setting has_explored=true");
            has_explored = true;
        }

        // Track if this turn includes write operations
        let write_tools = ["file_write", "file_edit", "file_delete", "create_file", "edit_file", "delete_file"];
        let turn_has_writes = tool_uses.iter().any(|(_, name, _)| {
            let name_lower = name.to_lowercase();
            write_tools.iter().any(|wt| name_lower.contains(wt))
        });
        if turn_has_writes {
            has_made_edits = true;
        }

        // Emit tool preview events AFTER enforcement check passed
        // (We deferred emission during streaming to avoid showing rejected tools)
        for (_id, name, input) in &tool_uses {
            match name.as_str() {
                "file_write" => {
                    if let (Some(path), Some(content)) = (
                        input.get("path").and_then(|v| v.as_str()),
                        input.get("content").and_then(|v| v.as_str()),
                    ) {
                        emitter.emit_file_create(path.to_string(), content.to_string(), None)?;
                        if !files_changed.contains(&path.to_string()) {
                            files_changed.push(path.to_string());
                        }
                    }
                }
                "file_edit" => {
                    if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                        let old_text = input.get("old_string").and_then(|v| v.as_str());
                        let new_text = input.get("new_string").and_then(|v| v.as_str());
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
                }
                "shell" => {
                    if let Some(command) = input.get("command").and_then(|v| v.as_str()) {
                        emitter.emit_command(command.to_string(), None, None)?;
                    }
                }
                "plan_update" => {
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
                _ => {}
            }
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
        let file_mod_tools = ["file_write", "file_edit", "file_delete", "create_file", "edit_file", "delete_file"];

        for (id, name, input) in tool_uses {
            let name_lower = name.to_lowercase();
            let is_file_mod = file_mod_tools.iter().any(|t| name_lower.contains(t));

            // In review mode, skip file modifications but return success
            // The events have already been emitted, so Teddy can show them for review
            if review_mode && is_file_mod {
                eprintln!("[REVIEW MODE] Skipping execution of file tool: {} (events already emitted)", name);
                tools_executed += 1;

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

            let result = tool_executor.execute_tool_use(&id, &name, input).await?;

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
        // This ensures that even if Ted is killed mid-conversation, Teddy has the latest history
        if let Err(e) = emitter.emit_conversation_history(extract_history_messages(&messages)) {
            eprintln!("[HISTORY] Failed to emit turn history: {}", e);
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

    emitter.emit_completion(is_task_complete, completion_message, files_changed)?;

    Ok(())
}
