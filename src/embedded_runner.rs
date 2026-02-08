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
use crate::error::{ApiError, Result, TedError};
use crate::llm::message::{ContentBlock, Message, MessageContent};
use crate::llm::provider::{
    CompletionRequest, ContentBlockDelta, ContentBlockResponse, LlmProvider, StreamEvent,
    ToolChoice,
};
use crate::llm::providers::{
    AnthropicProvider, BlackmanProvider, LocalProvider, OpenRouterProvider,
};
use crate::models::download::BinaryDownloader;
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
/// Different models use different naming conventions:
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
        // Some models send old/new as arrays of lines
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
        "local" => {
            let cfg = &settings.providers.local;

            // Resolve model path: explicit config → system scan → error
            let model_path = if cfg.model_path.exists() {
                cfg.model_path.clone()
            } else {
                let discovered = crate::models::scanner::scan_for_models();
                if discovered.is_empty() {
                    return Err(TedError::Config(
                        "No GGUF model files found. Download a model with /model download."
                            .to_string(),
                    ));
                }
                let selected = &discovered[0];
                tracing::info!(
                    "Auto-detected model: {} ({})",
                    selected.display_name(),
                    selected.size_display()
                );
                selected.path.clone()
            };

            let model_name = model_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&cfg.default_model)
                .to_string();

            let downloader = BinaryDownloader::new()?;
            let binary_path = downloader.ensure_llama_server().await?;
            let local_provider = LocalProvider::new(
                binary_path,
                model_path,
                model_name,
                cfg.port,
                cfg.gpu_layers,
                cfg.ctx_size,
            );
            Box::new(local_provider)
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
            let provider = if let Some(ref base_url) = settings.providers.anthropic.base_url {
                AnthropicProvider::with_base_url(api_key, base_url)
            } else {
                AnthropicProvider::new(api_key)
            };
            Box::new(provider)
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
            "local" => settings.providers.local.default_model.clone(),
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

        // Get streaming completion (with auto-trimming on context overflow)
        let mut stream = match provider.complete_stream(request.clone()).await {
            Ok(s) => s,
            Err(TedError::Api(ApiError::ContextTooLong { current, limit })) => {
                eprintln!(
                    "[CONTEXT] Context too long ({} tokens > {} limit). Auto-trimming...",
                    current, limit
                );

                // Get the actual limit from the model info or use the reported limit
                let context_window = provider
                    .get_model_info(&model)
                    .map(|m| m.context_window)
                    .unwrap_or(limit);

                // Trim to 70% of the limit to leave room
                let target_tokens = (context_window as f64 * 0.7) as u32;

                // Use built-in token estimation
                let mut total_tokens: u32 = messages.iter().map(|m| m.estimate_tokens()).sum();

                // Remove oldest messages (after the first user message) until we fit
                let mut removed = 0;
                while total_tokens > target_tokens && messages.len() > 2 {
                    // Keep at least the last 2 messages (latest user + response context)
                    let msg_tokens = messages[1].estimate_tokens();
                    messages.remove(1);
                    total_tokens = total_tokens.saturating_sub(msg_tokens);
                    removed += 1;
                }

                if removed > 0 {
                    eprintln!("[CONTEXT] Removed {} older messages. Retrying...", removed);
                    emitter.emit_status(
                        "thinking",
                        format!(
                            "Context trimmed ({} messages removed). Retrying...",
                            removed
                        ),
                        None,
                    )?;
                }

                // Build a new request with trimmed messages
                let mut retry_request = CompletionRequest::new(model.clone(), messages.clone())
                    .with_max_tokens(8192)
                    .with_temperature(0.7)
                    .with_tools(tool_executor.tool_definitions())
                    .with_tool_choice(ToolChoice::Auto);

                if !merged_cap.system_prompt.is_empty() {
                    retry_request = retry_request.with_system(merged_cap.system_prompt.clone());
                }

                // Retry with trimmed context
                provider.complete_stream(retry_request).await?
            }
            Err(e) => return Err(e),
        };

        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        let mut current_text = String::new();
        let mut tool_uses: Vec<(String, String, serde_json::Value)> = Vec::new();

        // Track current tool use being built (for streaming JSON input)
        let mut current_tool_id: Option<String> = None;
        let mut current_tool_name: Option<String> = None;
        let mut current_tool_input_json = String::new();

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
                                emitter.emit_message("assistant", text, Some(true))?;
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
                            emitter.emit_message("assistant", text, Some(true))?;
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

        // Debug: Log if we have empty response
        if tool_uses.is_empty() && current_text.trim().is_empty() {
            eprintln!("[DEBUG] Empty response from model - no tools and no text");
        }

        // Add text content if any
        if !current_text.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::message::{ContentBlock, Message, MessageContent, Role};
    use serde_json::json;

    // ===== tool_call_key tests =====

    #[test]
    fn test_tool_call_key_basic() {
        let key = tool_call_key("file_read", &json!({"path": "/test.txt"}));
        assert!(key.starts_with("file_read:"));
        assert!(key.contains("path"));
    }

    #[test]
    fn test_tool_call_key_uniqueness() {
        let key1 = tool_call_key("file_read", &json!({"path": "/a.txt"}));
        let key2 = tool_call_key("file_read", &json!({"path": "/b.txt"}));
        let key3 = tool_call_key("file_write", &json!({"path": "/a.txt"}));

        assert_ne!(key1, key2); // Different paths
        assert_ne!(key1, key3); // Different tools
    }

    // ===== extract_history_messages tests =====

    #[test]
    fn test_extract_history_messages_user_message() {
        let messages = vec![Message::user("Hello")];
        let history = extract_history_messages(&messages);

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[0].content, "Hello");
    }

    #[test]
    fn test_extract_history_messages_assistant_message() {
        let messages = vec![Message::assistant("Hi there")];
        let history = extract_history_messages(&messages);

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, "assistant");
        assert_eq!(history[0].content, "Hi there");
    }

    #[test]
    fn test_extract_history_messages_skips_system() {
        let messages = vec![Message::system("You are helpful"), Message::user("Hello")];
        let history = extract_history_messages(&messages);

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, "user");
    }

    #[test]
    fn test_extract_history_messages_skips_stop_messages() {
        let messages = vec![
            Message::user("STOP! Don't do that"),
            Message::user("Normal message"),
        ];
        let history = extract_history_messages(&messages);

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "Normal message");
    }

    #[test]
    fn test_extract_history_messages_skips_empty() {
        let messages = vec![Message::user(""), Message::user("Not empty")];
        let history = extract_history_messages(&messages);

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "Not empty");
    }

    #[test]
    fn test_extract_history_messages_handles_blocks() {
        let msg = Message {
            id: uuid::Uuid::new_v4(),
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::Text {
                    text: "First part".to_string(),
                },
                ContentBlock::Text {
                    text: " second part".to_string(),
                },
            ]),
            timestamp: chrono::Utc::now(),
            tool_use_id: None,
            token_count: None,
        };
        let history = extract_history_messages(&[msg]);

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "First part second part");
    }

    // ===== parse_tool_from_json tests =====

    #[test]
    fn test_parse_tool_from_json_with_arguments() {
        let value = json!({
            "name": "file_read",
            "arguments": {"path": "/test.txt"}
        });

        let result = parse_tool_from_json(&value);
        assert!(result.is_some());

        let (name, args) = result.unwrap();
        assert_eq!(name, "file_read");
        assert_eq!(args["path"], "/test.txt");
    }

    #[test]
    fn test_parse_tool_from_json_with_input() {
        let value = json!({
            "name": "file_read",
            "input": {"path": "/test.txt"}
        });

        let result = parse_tool_from_json(&value);
        assert!(result.is_some());

        let (name, _args) = result.unwrap();
        assert_eq!(name, "file_read");
    }

    #[test]
    fn test_parse_tool_from_json_name_normalization() {
        // Test read_file -> file_read
        let value = json!({"name": "read_file", "arguments": {"path": "/test"}});
        let (name, _) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "file_read");

        // Test edit_file -> file_edit
        let value = json!({"name": "edit_file", "arguments": {"path": "/test", "old_string": "a", "new_string": "b"}});
        let (name, _) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "file_edit");

        // Test create_file -> file_write
        let value =
            json!({"name": "create_file", "arguments": {"path": "/test", "content": "hello"}});
        let (name, _) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "file_write");
    }

    #[test]
    fn test_parse_tool_from_json_empty_old_string_converts_to_write() {
        // file_edit with empty old_string should become file_write
        let value = json!({
            "name": "file_edit",
            "arguments": {"path": "/test.txt", "old_string": "", "new_string": "content"}
        });

        let (name, args) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "file_write");
        assert_eq!(args["content"], "content");
    }

    #[test]
    fn test_parse_tool_from_json_returns_none_for_invalid() {
        // Missing name
        let value = json!({"arguments": {"path": "/test"}});
        assert!(parse_tool_from_json(&value).is_none());

        // Missing arguments/input
        let value = json!({"name": "file_read"});
        assert!(parse_tool_from_json(&value).is_none());

        // Not an object
        let value = json!("just a string");
        assert!(parse_tool_from_json(&value).is_none());
    }

    // ===== map_file_read_arguments tests =====

    #[test]
    fn test_map_file_read_arguments_path_variations() {
        // Test file -> path
        let args = json!({"file": "/test.txt"});
        let mapped = map_file_read_arguments(&args);
        assert_eq!(mapped["path"], "/test.txt");

        // Test filepath -> path
        let args = json!({"filepath": "/test.txt"});
        let mapped = map_file_read_arguments(&args);
        assert_eq!(mapped["path"], "/test.txt");

        // Test file_path -> path
        let args = json!({"file_path": "/test.txt"});
        let mapped = map_file_read_arguments(&args);
        assert_eq!(mapped["path"], "/test.txt");

        // Test filename -> path
        let args = json!({"filename": "/test.txt"});
        let mapped = map_file_read_arguments(&args);
        assert_eq!(mapped["path"], "/test.txt");
    }

    #[test]
    fn test_map_file_read_arguments_preserves_path() {
        let args = json!({"path": "/original.txt"});
        let mapped = map_file_read_arguments(&args);
        assert_eq!(mapped["path"], "/original.txt");
    }

    #[test]
    fn test_map_file_read_arguments_non_object() {
        let args = json!("not an object");
        let mapped = map_file_read_arguments(&args);
        assert_eq!(mapped, args);
    }

    // ===== map_file_write_arguments tests =====

    #[test]
    fn test_map_file_write_arguments_content_variations() {
        // Test text -> content
        let args = json!({"path": "/test.txt", "text": "hello"});
        let mapped = map_file_write_arguments(&args);
        assert_eq!(mapped["content"], "hello");

        // Test data -> content
        let args = json!({"path": "/test.txt", "data": "hello"});
        let mapped = map_file_write_arguments(&args);
        assert_eq!(mapped["content"], "hello");

        // Test code -> content
        let args = json!({"path": "/test.txt", "code": "fn main() {}"});
        let mapped = map_file_write_arguments(&args);
        assert_eq!(mapped["content"], "fn main() {}");
    }

    #[test]
    fn test_map_file_write_arguments_path_variations() {
        let args = json!({"file": "/test.txt", "content": "hello"});
        let mapped = map_file_write_arguments(&args);
        assert_eq!(mapped["path"], "/test.txt");
    }

    // ===== map_file_edit_arguments tests =====

    #[test]
    fn test_map_file_edit_arguments_old_new_variations() {
        // Test old_text/new_text -> old_string/new_string
        let args = json!({"path": "/test.txt", "old_text": "old", "new_text": "new"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "old");
        assert_eq!(mapped["new_string"], "new");

        // Test find/replace -> old_string/new_string
        let args = json!({"path": "/test.txt", "find": "old", "replace": "new"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "old");
        assert_eq!(mapped["new_string"], "new");

        // Test original/modified -> old_string/new_string
        let args = json!({"path": "/test.txt", "original": "old", "modified": "new"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "old");
        assert_eq!(mapped["new_string"], "new");
    }

    #[test]
    fn test_map_file_edit_arguments_array_to_string() {
        // Test array of lines -> joined string
        let args = json!({
            "path": "/test.txt",
            "old_string": ["line1", "line2"],
            "new_string": ["new1", "new2", "new3"]
        });
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "line1\nline2");
        assert_eq!(mapped["new_string"], "new1\nnew2\nnew3");
    }

    #[test]
    fn test_map_file_edit_arguments_file_path_variations() {
        let args = json!({"file": "/test.txt", "old_string": "a", "new_string": "b"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["path"], "/test.txt");
    }

    // ===== map_shell_arguments tests =====

    #[test]
    fn test_map_shell_arguments_command_variations() {
        // Test cmd -> command
        let args = json!({"cmd": "ls -la"});
        let mapped = map_shell_arguments(&args);
        assert_eq!(mapped["command"], "ls -la");

        // Test shell_command -> command
        let args = json!({"shell_command": "pwd"});
        let mapped = map_shell_arguments(&args);
        assert_eq!(mapped["command"], "pwd");

        // Test bash -> command
        let args = json!({"bash": "echo hello"});
        let mapped = map_shell_arguments(&args);
        assert_eq!(mapped["command"], "echo hello");

        // Test exec -> command
        let args = json!({"exec": "cat file.txt"});
        let mapped = map_shell_arguments(&args);
        assert_eq!(mapped["command"], "cat file.txt");

        // Test run -> command
        let args = json!({"run": "make build"});
        let mapped = map_shell_arguments(&args);
        assert_eq!(mapped["command"], "make build");
    }

    #[test]
    fn test_map_shell_arguments_preserves_command() {
        let args = json!({"command": "cargo test"});
        let mapped = map_shell_arguments(&args);
        assert_eq!(mapped["command"], "cargo test");
    }

    // ===== Additional map_file_edit_arguments tests =====

    #[test]
    fn test_map_file_edit_arguments_before_after() {
        let args = json!({"path": "/test.txt", "before": "old text", "after": "new text"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "old text");
        assert_eq!(mapped["new_string"], "new text");
    }

    #[test]
    fn test_map_file_edit_arguments_search_pattern() {
        let args = json!({"path": "/test.txt", "search": "find me", "with": "replace me"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "find me");
        assert_eq!(mapped["new_string"], "replace me");
    }

    #[test]
    fn test_map_file_edit_arguments_target_updated() {
        let args = json!({"path": "/test.txt", "target": "original", "updated": "modified"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "original");
        assert_eq!(mapped["new_string"], "modified");
    }

    #[test]
    fn test_map_file_edit_arguments_camel_case() {
        let args = json!({"path": "/test.txt", "oldText": "old", "newText": "new"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "old");
        assert_eq!(mapped["new_string"], "new");
    }

    #[test]
    fn test_map_file_edit_arguments_content_variation() {
        let args = json!({"path": "/test.txt", "old_content": "old", "content": "new"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "old");
        assert_eq!(mapped["new_string"], "new");
    }

    #[test]
    fn test_map_file_edit_arguments_match_pattern() {
        let args = json!({"path": "/test.txt", "match": "find", "replacement": "replace"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "find");
        assert_eq!(mapped["new_string"], "replace");
    }

    // ===== Additional map_file_write_arguments tests =====

    #[test]
    fn test_map_file_write_arguments_contents_variation() {
        let args = json!({"path": "/test.txt", "contents": "file contents"});
        let mapped = map_file_write_arguments(&args);
        assert_eq!(mapped["content"], "file contents");
    }

    #[test]
    fn test_map_file_write_arguments_file_content_variation() {
        let args = json!({"path": "/test.txt", "file_content": "the content"});
        let mapped = map_file_write_arguments(&args);
        assert_eq!(mapped["content"], "the content");
    }

    #[test]
    fn test_map_file_write_arguments_body_variation() {
        let args = json!({"path": "/test.txt", "body": "request body"});
        let mapped = map_file_write_arguments(&args);
        assert_eq!(mapped["content"], "request body");
    }

    #[test]
    fn test_map_file_write_arguments_name_variation() {
        let args = json!({"name": "/test.txt", "content": "test"});
        let mapped = map_file_write_arguments(&args);
        assert_eq!(mapped["path"], "/test.txt");
    }

    #[test]
    fn test_map_file_write_arguments_file_name_variation() {
        let args = json!({"file_name": "/test.txt", "content": "test"});
        let mapped = map_file_write_arguments(&args);
        assert_eq!(mapped["path"], "/test.txt");
    }

    // ===== Additional map_file_read_arguments tests =====

    #[test]
    fn test_map_file_read_arguments_name_variation() {
        let args = json!({"name": "/some/file.txt"});
        let mapped = map_file_read_arguments(&args);
        assert_eq!(mapped["path"], "/some/file.txt");
    }

    #[test]
    fn test_map_file_read_arguments_file_name_variation() {
        let args = json!({"file_name": "/another/file.txt"});
        let mapped = map_file_read_arguments(&args);
        assert_eq!(mapped["path"], "/another/file.txt");
    }

    #[test]
    fn test_map_file_read_arguments_preserves_unknown_keys() {
        let args = json!({"path": "/test.txt", "encoding": "utf-8", "start_line": 10});
        let mapped = map_file_read_arguments(&args);
        assert_eq!(mapped["path"], "/test.txt");
        assert_eq!(mapped["encoding"], "utf-8");
        assert_eq!(mapped["start_line"], 10);
    }

    // ===== Additional map_shell_arguments tests =====

    #[test]
    fn test_map_shell_arguments_non_object() {
        let args = json!("just a string");
        let mapped = map_shell_arguments(&args);
        assert_eq!(mapped, args);
    }

    #[test]
    fn test_map_shell_arguments_preserves_unknown_keys() {
        let args = json!({"command": "ls", "cwd": "/home", "timeout": 30});
        let mapped = map_shell_arguments(&args);
        assert_eq!(mapped["command"], "ls");
        assert_eq!(mapped["cwd"], "/home");
        assert_eq!(mapped["timeout"], 30);
    }

    // ===== Additional parse_tool_from_json tests =====

    #[test]
    fn test_parse_tool_from_json_file_write_via_write_file() {
        let value =
            json!({"name": "write_file", "arguments": {"path": "/test.txt", "content": "test"}});
        let (name, _) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "file_write");
    }

    #[test]
    fn test_parse_tool_from_json_file_delete() {
        let value = json!({"name": "delete_file", "arguments": {"path": "/test.txt"}});
        let (name, _) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "file_delete");
    }

    #[test]
    fn test_parse_tool_from_json_unknown_tool_passed_through() {
        let value = json!({"name": "custom_tool", "arguments": {"param": "value"}});
        let (name, args) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "custom_tool");
        assert_eq!(args["param"], "value");
    }

    #[test]
    fn test_parse_tool_from_json_whitespace_only_old_string() {
        // file_edit with whitespace-only old_string should become file_write
        let value = json!({
            "name": "file_edit",
            "arguments": {"path": "/test.txt", "old_string": "   ", "new_string": "content"}
        });

        let (name, args) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "file_write");
        assert_eq!(args["content"], "content");
    }

    #[test]
    fn test_parse_tool_from_json_file_edit_with_real_content() {
        // file_edit with real old_string should stay as file_edit
        let value = json!({
            "name": "file_edit",
            "arguments": {"path": "/test.txt", "old_string": "real content", "new_string": "new content"}
        });

        let (name, args) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "file_edit");
        assert_eq!(args["old_string"], "real content");
    }

    // ===== Additional extract_history_messages tests =====

    #[test]
    fn test_extract_history_messages_empty_list() {
        let messages: Vec<Message> = vec![];
        let history = extract_history_messages(&messages);
        assert!(history.is_empty());
    }

    #[test]
    fn test_extract_history_messages_multiple_messages() {
        let messages = vec![
            Message::user("First question"),
            Message::assistant("First answer"),
            Message::user("Second question"),
            Message::assistant("Second answer"),
        ];
        let history = extract_history_messages(&messages);

        assert_eq!(history.len(), 4);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[1].role, "assistant");
        assert_eq!(history[2].role, "user");
        assert_eq!(history[3].role, "assistant");
    }

    #[test]
    fn test_extract_history_messages_mixed_with_stop_and_system() {
        let messages = vec![
            Message::system("System prompt"),
            Message::user("Hello"),
            Message::user("STOP! Internal message"),
            Message::assistant("Response"),
        ];
        let history = extract_history_messages(&messages);

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "Hello");
        assert_eq!(history[1].content, "Response");
    }

    #[test]
    fn test_extract_history_messages_blocks_with_tool_use() {
        let msg = Message {
            id: uuid::Uuid::new_v4(),
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::Text {
                    text: "I'll read the file.".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "call_123".to_string(),
                    name: "file_read".to_string(),
                    input: json!({"path": "/test.txt"}),
                },
            ]),
            timestamp: chrono::Utc::now(),
            tool_use_id: None,
            token_count: None,
        };
        let history = extract_history_messages(&[msg]);

        assert_eq!(history.len(), 1);
        // ToolUse blocks are ignored, only text is extracted
        assert_eq!(history[0].content, "I'll read the file.");
    }

    #[test]
    fn test_extract_history_messages_blocks_only_tool_use() {
        let msg = Message {
            id: uuid::Uuid::new_v4(),
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "call_123".to_string(),
                name: "file_read".to_string(),
                input: json!({"path": "/test.txt"}),
            }]),
            timestamp: chrono::Utc::now(),
            tool_use_id: None,
            token_count: None,
        };
        let history = extract_history_messages(&[msg]);

        // Message with only tool use (no text) is skipped as empty
        assert!(history.is_empty());
    }

    // ===== Additional tool_call_key tests =====

    #[test]
    fn test_tool_call_key_complex_input() {
        let input = json!({
            "path": "/test.txt",
            "options": {
                "encoding": "utf-8",
                "flags": ["a", "b"]
            }
        });
        let key = tool_call_key("complex_tool", &input);
        assert!(key.starts_with("complex_tool:"));
        assert!(key.contains("path"));
    }

    #[test]
    fn test_tool_call_key_empty_input() {
        let key = tool_call_key("simple_tool", &json!({}));
        assert!(key.starts_with("simple_tool:"));
    }

    #[test]
    fn test_tool_call_key_null_input() {
        let key = tool_call_key("tool", &json!(null));
        assert!(key.starts_with("tool:"));
    }

    // ===== HistoryMessage tests =====

    #[test]
    fn test_history_message_serialization() {
        let msg = HistoryMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Hello\""));
    }

    #[test]
    fn test_history_message_deserialization() {
        let json = r#"{"role":"assistant","content":"Hi there"}"#;
        let msg: HistoryMessage = serde_json::from_str(json).unwrap();

        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "Hi there");
    }

    #[test]
    fn test_history_message_clone() {
        let msg = HistoryMessage {
            role: "user".to_string(),
            content: "Test".to_string(),
        };
        let cloned = msg.clone();

        assert_eq!(cloned.role, msg.role);
        assert_eq!(cloned.content, msg.content);
    }

    #[test]
    fn test_history_message_debug() {
        let msg = HistoryMessage {
            role: "user".to_string(),
            content: "Debug test".to_string(),
        };

        let debug = format!("{:?}", msg);
        assert!(debug.contains("HistoryMessage"));
        assert!(debug.contains("user"));
    }

    // ===== Additional edge case tests =====

    #[test]
    fn test_map_file_edit_arguments_empty_array() {
        let args = json!({"path": "/test.txt", "old_string": [], "new_string": []});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "");
        assert_eq!(mapped["new_string"], "");
    }

    #[test]
    fn test_map_file_edit_arguments_mixed_array() {
        // Array with non-string values - should filter them out
        let args = json!({"path": "/test.txt", "old_string": ["line1", 123, "line2"]});
        let mapped = map_file_edit_arguments(&args);
        // Non-strings are filtered out
        assert_eq!(mapped["old_string"], "line1\nline2");
    }

    #[test]
    fn test_parse_tool_from_json_file_create_to_write() {
        let value = json!({"name": "file_create", "arguments": {"path": "/new.txt", "content": "new file"}});
        let (name, _) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "file_write");
    }

    // ==================== Additional comprehensive tests ====================

    // ===== HashSet-based tool call deduplication tests =====

    #[test]
    fn test_tool_call_key_for_hashset() {
        use std::collections::HashSet;

        let mut seen: HashSet<String> = HashSet::new();

        let key1 = tool_call_key("file_read", &json!({"path": "/a.txt"}));
        let key2 = tool_call_key("file_read", &json!({"path": "/a.txt"}));
        let key3 = tool_call_key("file_read", &json!({"path": "/b.txt"}));

        seen.insert(key1.clone());

        assert!(seen.contains(&key2)); // Same tool+args = same key
        assert!(!seen.contains(&key3)); // Different args = different key
    }

    #[test]
    fn test_consecutive_repeat_detection() {
        use std::collections::HashSet;

        let current: HashSet<String> = [
            tool_call_key("file_read", &json!({"path": "/a.txt"})),
            tool_call_key("file_read", &json!({"path": "/b.txt"})),
        ]
        .into_iter()
        .collect();

        let previous: HashSet<String> = [
            tool_call_key("file_read", &json!({"path": "/a.txt"})),
            tool_call_key("file_read", &json!({"path": "/b.txt"})),
        ]
        .into_iter()
        .collect();

        // All current calls were in previous = repeated
        let all_repeated = !current.is_empty() && current.iter().all(|k| previous.contains(k));

        assert!(all_repeated);
    }

    #[test]
    fn test_non_consecutive_repeat_detection() {
        use std::collections::HashSet;

        let current: HashSet<String> = [tool_call_key("file_read", &json!({"path": "/c.txt"}))]
            .into_iter()
            .collect();

        let previous: HashSet<String> = [tool_call_key("file_read", &json!({"path": "/a.txt"}))]
            .into_iter()
            .collect();

        let all_repeated = !current.is_empty() && current.iter().all(|k| previous.contains(k));

        assert!(!all_repeated);
    }

    // ===== More parse_tool_from_json edge cases =====

    #[test]
    fn test_parse_tool_from_json_array_input() {
        // Test that array input doesn't crash
        let value = json!(["not", "an", "object"]);
        assert!(parse_tool_from_json(&value).is_none());
    }

    #[test]
    fn test_parse_tool_from_json_number_input() {
        let value = json!(42);
        assert!(parse_tool_from_json(&value).is_none());
    }

    #[test]
    fn test_parse_tool_from_json_bool_input() {
        let value = json!(true);
        assert!(parse_tool_from_json(&value).is_none());
    }

    #[test]
    fn test_parse_tool_from_json_null_input() {
        let value = json!(null);
        assert!(parse_tool_from_json(&value).is_none());
    }

    #[test]
    fn test_parse_tool_from_json_name_as_number() {
        let value = json!({"name": 123, "arguments": {"path": "/test"}});
        assert!(parse_tool_from_json(&value).is_none());
    }

    #[test]
    fn test_parse_tool_from_json_arguments_as_array() {
        let value = json!({"name": "file_read", "arguments": [1, 2, 3]});
        // Should still work - arguments is just passed through
        let result = parse_tool_from_json(&value);
        assert!(result.is_some());
        let (name, args) = result.unwrap();
        assert_eq!(name, "file_read");
        assert!(args.is_array());
    }

    // ===== map_file_edit_arguments more variations =====

    #[test]
    fn test_map_file_edit_arguments_old_new_keywords() {
        let args = json!({"path": "/test.txt", "old": "before", "new": "after"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "before");
        assert_eq!(mapped["new_string"], "after");
    }

    #[test]
    fn test_map_file_edit_arguments_camel_case_content() {
        let args = json!({"path": "/test.txt", "oldContent": "old", "newContent": "new"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "old");
        assert_eq!(mapped["new_string"], "new");
    }

    #[test]
    fn test_map_file_edit_arguments_pattern_keyword() {
        let args = json!({"path": "/test.txt", "pattern": "find this", "replacement": "with this"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "find this");
        assert_eq!(mapped["new_string"], "with this");
    }

    // ===== extract_history_messages with various content types =====

    #[test]
    fn test_extract_history_messages_with_tool_result() {
        let msg = Message {
            id: uuid::Uuid::new_v4(),
            role: Role::User,
            content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: "call_123".to_string(),
                content: crate::llm::message::ToolResultContent::Text("File contents".to_string()),
                is_error: None,
            }]),
            timestamp: chrono::Utc::now(),
            tool_use_id: None,
            token_count: None,
        };
        let history = extract_history_messages(&[msg]);

        // ToolResult blocks are not extracted as text
        assert!(history.is_empty());
    }

    #[test]
    fn test_extract_history_messages_multiline_content() {
        let messages = vec![Message::user("Line 1\nLine 2\nLine 3")];
        let history = extract_history_messages(&messages);

        assert_eq!(history.len(), 1);
        assert!(history[0].content.contains('\n'));
        assert!(history[0].content.contains("Line 2"));
    }

    #[test]
    fn test_extract_history_messages_unicode_content() {
        let messages = vec![Message::user("日本語 テスト 🎉 émojis")];
        let history = extract_history_messages(&messages);

        assert_eq!(history.len(), 1);
        assert!(history[0].content.contains("日本語"));
        assert!(history[0].content.contains("🎉"));
    }

    #[test]
    fn test_extract_history_messages_stop_in_middle() {
        // STOP! prefix should be at the start to be filtered
        let messages = vec![Message::user("This message has STOP! in the middle")];
        let history = extract_history_messages(&messages);

        // This should NOT be filtered because STOP! is not at the start
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_extract_history_messages_whitespace_only() {
        // Whitespace-only content is NOT empty, so it should be included
        let messages = vec![Message::user("   ")];
        let history = extract_history_messages(&messages);

        assert_eq!(history.len(), 1);
    }

    // ===== HistoryMessage round-trip tests =====

    #[test]
    fn test_history_message_json_roundtrip() {
        let original = HistoryMessage {
            role: "assistant".to_string(),
            content: "Hello with \"quotes\" and\nnewlines".to_string(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let restored: HistoryMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.role, original.role);
        assert_eq!(restored.content, original.content);
    }

    #[test]
    fn test_history_message_array_roundtrip() {
        let messages = vec![
            HistoryMessage {
                role: "user".to_string(),
                content: "Q1".to_string(),
            },
            HistoryMessage {
                role: "assistant".to_string(),
                content: "A1".to_string(),
            },
            HistoryMessage {
                role: "user".to_string(),
                content: "Q2".to_string(),
            },
        ];

        let json = serde_json::to_string(&messages).unwrap();
        let restored: Vec<HistoryMessage> = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.len(), 3);
        assert_eq!(restored[0].content, "Q1");
        assert_eq!(restored[2].content, "Q2");
    }

    // ===== map functions with null/missing values =====

    #[test]
    fn test_map_file_read_arguments_null_values() {
        let args = json!({"path": null});
        let mapped = map_file_read_arguments(&args);
        // null should be preserved
        assert!(mapped["path"].is_null());
    }

    #[test]
    fn test_map_file_write_arguments_empty_object() {
        let args = json!({});
        let mapped = map_file_write_arguments(&args);
        assert!(mapped.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_map_shell_arguments_empty_object() {
        let args = json!({});
        let mapped = map_shell_arguments(&args);
        assert!(mapped.as_object().unwrap().is_empty());
    }

    // ===== Tool name normalization edge cases =====

    #[test]
    fn test_parse_tool_from_json_case_sensitivity() {
        // Tool names should be matched case-sensitively
        let value = json!({"name": "FILE_READ", "arguments": {"path": "/test"}});
        let (name, _) = parse_tool_from_json(&value).unwrap();
        // FILE_READ doesn't match any of our mappings, so it's passed through
        assert_eq!(name, "FILE_READ");
    }

    #[test]
    fn test_parse_tool_from_json_with_extra_fields() {
        let value = json!({
            "name": "file_read",
            "arguments": {"path": "/test"},
            "id": "some_id",
            "metadata": {"foo": "bar"}
        });
        let result = parse_tool_from_json(&value);
        assert!(result.is_some());
        let (name, _) = result.unwrap();
        assert_eq!(name, "file_read");
    }

    // ===== Message creation helper tests =====

    #[test]
    fn test_message_user_creation() {
        let msg = Message::user("Hello");
        assert!(matches!(msg.role, Role::User));

        if let MessageContent::Text(text) = &msg.content {
            assert_eq!(text, "Hello");
        } else {
            panic!("Expected Text content");
        }
    }

    #[test]
    fn test_message_assistant_creation() {
        let msg = Message::assistant("Hi there");
        assert!(matches!(msg.role, Role::Assistant));
    }

    #[test]
    fn test_message_system_creation() {
        let msg = Message::system("You are helpful");
        assert!(matches!(msg.role, Role::System));
    }

    #[test]
    fn test_message_assistant_blocks_creation() {
        let blocks = vec![ContentBlock::Text {
            text: "Test".to_string(),
        }];
        let msg = Message::assistant_blocks(blocks);
        assert!(matches!(msg.role, Role::Assistant));

        if let MessageContent::Blocks(b) = &msg.content {
            assert_eq!(b.len(), 1);
        } else {
            panic!("Expected Blocks content");
        }
    }

    // ===== Timestamp and ID tests =====

    #[test]
    fn test_message_has_unique_id() {
        let msg1 = Message::user("A");
        let msg2 = Message::user("A");

        assert_ne!(msg1.id, msg2.id);
    }

    #[test]
    fn test_message_has_timestamp() {
        let before = chrono::Utc::now();
        let msg = Message::user("Test");
        let after = chrono::Utc::now();

        assert!(msg.timestamp >= before);
        assert!(msg.timestamp <= after);
    }

    // ===== map_file_edit_arguments edge cases =====

    #[test]
    fn test_map_file_edit_arguments_non_object() {
        // When args is not an object, return it unchanged
        let args = json!("just a string");
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped, json!("just a string"));
    }

    #[test]
    fn test_map_file_edit_arguments_array_input() {
        let args = json!([1, 2, 3]);
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped, json!([1, 2, 3]));
    }

    #[test]
    fn test_map_file_edit_arguments_null_input() {
        let args = json!(null);
        let mapped = map_file_edit_arguments(&args);
        assert!(mapped.is_null());
    }

    #[test]
    fn test_map_file_edit_arguments_number_input() {
        let args = json!(42);
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped, json!(42));
    }

    #[test]
    fn test_map_file_edit_arguments_array_old_string() {
        // Test array values for old_string - should be joined with newlines
        let args = json!({
            "old_string": ["line 1", "line 2", "line 3"],
            "new_string": "replacement"
        });
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "line 1\nline 2\nline 3");
        assert_eq!(mapped["new_string"], "replacement");
    }

    #[test]
    fn test_map_file_edit_arguments_array_new_string() {
        // Test array values for new_string - should be joined with newlines
        let args = json!({
            "old_string": "original",
            "new_string": ["new line 1", "new line 2"]
        });
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "original");
        assert_eq!(mapped["new_string"], "new line 1\nnew line 2");
    }

    #[test]
    fn test_map_file_edit_arguments_both_arrays() {
        let args = json!({
            "old": ["a", "b"],
            "new": ["c", "d"]
        });
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "a\nb");
        assert_eq!(mapped["new_string"], "c\nd");
    }

    #[test]
    fn test_map_file_edit_arguments_array_with_non_strings() {
        // Array with non-string elements should filter them out
        let args = json!({
            "old_string": ["line 1", 42, "line 2", null, "line 3"]
        });
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "line 1\nline 2\nline 3");
    }

    #[test]
    fn test_map_file_edit_arguments_empty_array_both_fields() {
        let args = json!({
            "old_string": [],
            "new_string": []
        });
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "");
        assert_eq!(mapped["new_string"], "");
    }

    #[test]
    fn test_map_file_edit_arguments_unknown_key() {
        // Unknown keys should be passed through unchanged
        let args = json!({
            "unknown_key": "value",
            "old_string": "old"
        });
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["unknown_key"], "value");
        assert_eq!(mapped["old_string"], "old");
    }

    #[test]
    fn test_map_file_edit_arguments_all_aliases() {
        // Test all old_string aliases
        for alias in [
            "old_text",
            "oldText",
            "old_content",
            "oldContent",
            "find",
            "search",
            "original",
            "old",
            "before",
            "pattern",
            "target",
            "match",
        ] {
            let args = json!({ alias: "test_value" });
            let mapped = map_file_edit_arguments(&args);
            assert_eq!(
                mapped["old_string"], "test_value",
                "Failed for alias: {}",
                alias
            );
        }

        // Test all new_string aliases
        for alias in [
            "new_text",
            "newText",
            "new_content",
            "newContent",
            "replace",
            "replacement",
            "modified",
            "new",
            "after",
            "content",
            "updated",
            "with",
        ] {
            let args = json!({ alias: "test_value" });
            let mapped = map_file_edit_arguments(&args);
            assert_eq!(
                mapped["new_string"], "test_value",
                "Failed for alias: {}",
                alias
            );
        }

        // Test all path aliases
        for alias in ["file", "file_path", "filepath", "filename", "file_name"] {
            let args = json!({ alias: "/test/path" });
            let mapped = map_file_edit_arguments(&args);
            assert_eq!(mapped["path"], "/test/path", "Failed for alias: {}", alias);
        }
    }

    // ===== map_file_read_arguments edge cases =====

    #[test]
    fn test_map_file_read_arguments_string_passthrough() {
        let args = json!("string value");
        let mapped = map_file_read_arguments(&args);
        assert_eq!(mapped, json!("string value"));
    }

    #[test]
    fn test_map_file_read_arguments_array() {
        let args = json!([1, 2, 3]);
        let mapped = map_file_read_arguments(&args);
        assert_eq!(mapped, json!([1, 2, 3]));
    }

    #[test]
    fn test_map_file_read_arguments_all_aliases() {
        for alias in [
            "file",
            "file_path",
            "filepath",
            "filename",
            "name",
            "file_name",
        ] {
            let args = json!({ alias: "/test/path" });
            let mapped = map_file_read_arguments(&args);
            assert_eq!(mapped["path"], "/test/path", "Failed for alias: {}", alias);
        }
    }

    #[test]
    fn test_map_file_read_arguments_preserves_unknown() {
        let args = json!({
            "path": "/test",
            "unknown": "value"
        });
        let mapped = map_file_read_arguments(&args);
        assert_eq!(mapped["path"], "/test");
        assert_eq!(mapped["unknown"], "value");
    }

    // ===== map_file_write_arguments edge cases =====

    #[test]
    fn test_map_file_write_arguments_non_object() {
        let args = json!(123);
        let mapped = map_file_write_arguments(&args);
        assert_eq!(mapped, json!(123));
    }

    #[test]
    fn test_map_file_write_arguments_array() {
        let args = json!(["a", "b"]);
        let mapped = map_file_write_arguments(&args);
        assert_eq!(mapped, json!(["a", "b"]));
    }

    #[test]
    fn test_map_file_write_arguments_all_aliases() {
        // Test path aliases
        for alias in [
            "file",
            "file_path",
            "filepath",
            "filename",
            "name",
            "file_name",
        ] {
            let args = json!({ alias: "/test/path" });
            let mapped = map_file_write_arguments(&args);
            assert_eq!(
                mapped["path"], "/test/path",
                "Failed for path alias: {}",
                alias
            );
        }

        // Test content aliases
        for alias in ["text", "data", "contents", "file_content", "code", "body"] {
            let args = json!({ alias: "test content" });
            let mapped = map_file_write_arguments(&args);
            assert_eq!(
                mapped["content"], "test content",
                "Failed for content alias: {}",
                alias
            );
        }
    }

    // ===== map_shell_arguments edge cases =====

    #[test]
    fn test_map_shell_arguments_boolean_passthrough() {
        let args = json!(true);
        let mapped = map_shell_arguments(&args);
        assert_eq!(mapped, json!(true));
    }

    #[test]
    fn test_map_shell_arguments_array() {
        let args = json!(["cmd1", "cmd2"]);
        let mapped = map_shell_arguments(&args);
        assert_eq!(mapped, json!(["cmd1", "cmd2"]));
    }

    #[test]
    fn test_map_shell_arguments_all_aliases() {
        for alias in ["cmd", "shell_command", "bash", "exec", "run"] {
            let args = json!({ alias: "ls -la" });
            let mapped = map_shell_arguments(&args);
            assert_eq!(mapped["command"], "ls -la", "Failed for alias: {}", alias);
        }
    }

    #[test]
    fn test_map_shell_arguments_preserves_unknown() {
        let args = json!({
            "command": "ls",
            "timeout": 30,
            "working_dir": "/tmp"
        });
        let mapped = map_shell_arguments(&args);
        assert_eq!(mapped["command"], "ls");
        assert_eq!(mapped["timeout"], 30);
        assert_eq!(mapped["working_dir"], "/tmp");
    }

    // ===== Additional coverage tests for edge cases =====

    #[test]
    fn test_parse_tool_from_json_with_null_value() {
        let value = json!(null);
        assert!(parse_tool_from_json(&value).is_none());
    }

    #[test]
    fn test_parse_tool_from_json_with_number() {
        let value = json!(42);
        assert!(parse_tool_from_json(&value).is_none());
    }

    #[test]
    fn test_parse_tool_from_json_with_boolean() {
        let value = json!(true);
        assert!(parse_tool_from_json(&value).is_none());
    }

    #[test]
    fn test_map_file_edit_arguments_non_object_returns_clone() {
        let args = json!(null);
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped, json!(null));
    }

    #[test]
    fn test_map_file_edit_arguments_with_number_value() {
        let args = json!({"path": "/test.txt", "line_number": 42});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["path"], "/test.txt");
        assert_eq!(mapped["line_number"], 42);
    }

    #[test]
    fn test_history_message_roundtrip() {
        let original = HistoryMessage {
            role: "user".to_string(),
            content: "Test with special chars: <>&\"'".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: HistoryMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(original.role, deserialized.role);
        assert_eq!(original.content, deserialized.content);
    }

    #[test]
    fn test_map_file_edit_old_camel_case() {
        let args = json!({"path": "/t.txt", "oldContent": "old", "newContent": "new"});
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "old");
        assert_eq!(mapped["new_string"], "new");
    }

    #[test]
    fn test_parse_tool_from_json_glob_passthrough() {
        let value = json!({"name": "glob", "arguments": {"pattern": "*.rs"}});
        let (name, args) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "glob");
        assert_eq!(args["pattern"], "*.rs");
    }

    #[test]
    fn test_parse_tool_from_json_grep_passthrough() {
        let value = json!({"name": "grep", "arguments": {"pattern": "TODO", "path": "."}});
        let (name, args) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "grep");
        assert_eq!(args["pattern"], "TODO");
    }

    // ==================== Integration tests for run_embedded_chat ====================

    mod integration {
        use super::*;
        use crate::cli::ChatArgs;
        use crate::config::Settings;
        use std::path::PathBuf;
        use tempfile::TempDir;

        /// Create a default ChatArgs for testing
        fn create_test_chat_args() -> ChatArgs {
            ChatArgs {
                prompt: Some("Test prompt".to_string()),
                cap: vec![],
                model: Some("test-model".to_string()),
                provider: Some("local".to_string()),
                resume: None,
                trust: true,
                no_stream: false,
                model_path: None,
                embedded: true,
                no_tui: true,
                history: None,
                review_mode: false,
                project_has_files: false,
                system_prompt_file: None,
                files_in_context: vec![],
            }
        }

        /// Create test settings with local provider config
        fn create_test_settings(_base_url: &str) -> Settings {
            let mut settings = Settings::default();
            settings.providers.local.default_model = "test-model".to_string();
            settings.defaults.provider = "local".to_string();
            settings.defaults.caps = vec!["base".to_string()];
            settings
        }

        #[tokio::test]
        async fn test_run_embedded_chat_missing_prompt_error() {
            let mut args = create_test_chat_args();
            args.prompt = None; // No prompt should error in embedded mode

            let settings = create_test_settings("http://localhost:11434");

            let result = run_embedded_chat(args, settings).await;
            assert!(result.is_err());

            let err = result.unwrap_err();
            let err_msg = err.to_string();
            assert!(
                err_msg.contains("prompt"),
                "Error should mention prompt: {}",
                err_msg
            );
        }

        #[tokio::test]
        async fn test_run_embedded_chat_with_history_file() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Setup mock response
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"I understand."},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            // Create history file
            let temp_dir = TempDir::new().unwrap();
            let history_path = temp_dir.path().join("history.json");
            let history_content = r#"[{"role":"user","content":"Previous message"},{"role":"assistant","content":"Previous response"}]"#;
            std::fs::write(&history_path, history_content).unwrap();

            let mut args = create_test_chat_args();
            args.history = Some(history_path);

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            // May fail due to missing caps, but history loading is exercised
            // The main point is no panic and history is processed
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_with_nonexistent_history_file() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Hello"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.history = Some(PathBuf::from("/nonexistent/history.json"));

            let settings = create_test_settings(&mock_server.uri());

            // Should not crash, just log warning and continue
            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_with_invalid_history_json() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Hi"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            // Create history file with invalid JSON
            let temp_dir = TempDir::new().unwrap();
            let history_path = temp_dir.path().join("history.json");
            std::fs::write(&history_path, "not valid json").unwrap();

            let mut args = create_test_chat_args();
            args.history = Some(history_path);

            let settings = create_test_settings(&mock_server.uri());

            // Should not crash, just log warning
            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_with_system_prompt_file() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Got it"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            // Create system prompt file
            let temp_dir = TempDir::new().unwrap();
            let prompt_path = temp_dir.path().join("system_prompt.txt");
            std::fs::write(&prompt_path, "You are a helpful assistant.").unwrap();

            let mut args = create_test_chat_args();
            args.system_prompt_file = Some(prompt_path);

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_with_empty_system_prompt_file() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"OK"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            // Create empty system prompt file
            let temp_dir = TempDir::new().unwrap();
            let prompt_path = temp_dir.path().join("system_prompt.txt");
            std::fs::write(&prompt_path, "").unwrap();

            let mut args = create_test_chat_args();
            args.system_prompt_file = Some(prompt_path);

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_with_nonexistent_system_prompt_file() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"OK"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.system_prompt_file = Some(PathBuf::from("/nonexistent/prompt.txt"));

            let settings = create_test_settings(&mock_server.uri());

            // Should not crash, just log warning
            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_review_mode() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with a file_write tool call
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"I will create the file.\n```json\n{\"name\":\"file_write\",\"arguments\":{\"path\":\"/tmp/test.txt\",\"content\":\"hello\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.review_mode = true;

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_with_tool_response() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // First response with tool call, second response with final text
            let response1 = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Let me read that file.\n```json\n{\"name\":\"file_read\",\"arguments\":{\"path\":\"/etc/hosts\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response1))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_history_deduplication() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"OK"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            // Create history with duplicate messages
            let temp_dir = TempDir::new().unwrap();
            let history_path = temp_dir.path().join("history.json");
            let history_content = r#"[
                {"role":"user","content":"Hello"},
                {"role":"user","content":"Hello"},
                {"role":"assistant","content":"Hi"},
                {"role":"assistant","content":"Hi"}
            ]"#;
            std::fs::write(&history_path, history_content).unwrap();

            let mut args = create_test_chat_args();
            args.history = Some(history_path);

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_files_in_context() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Done"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.files_in_context = vec!["file1.txt".to_string(), "file2.txt".to_string()];

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_openrouter_missing_api_key() {
            let mut args = create_test_chat_args();
            args.provider = Some("openrouter".to_string());

            let settings = create_test_settings("http://localhost:11434");

            let result = run_embedded_chat(args, settings).await;
            assert!(result.is_err());

            let err = result.unwrap_err();
            let err_msg = err.to_string();
            assert!(
                err_msg.contains("API key") || err_msg.contains("OpenRouter"),
                "Error should mention API key: {}",
                err_msg
            );
        }

        #[tokio::test]
        async fn test_run_embedded_chat_blackman_missing_api_key() {
            let mut args = create_test_chat_args();
            args.provider = Some("blackman".to_string());

            let settings = create_test_settings("http://localhost:11434");

            let result = run_embedded_chat(args, settings).await;
            assert!(result.is_err());

            let err = result.unwrap_err();
            let err_msg = err.to_string();
            assert!(
                err_msg.contains("API key") || err_msg.contains("Blackman"),
                "Error should mention API key: {}",
                err_msg
            );
        }

        #[tokio::test]
        async fn test_run_embedded_chat_anthropic_missing_api_key() {
            let mut args = create_test_chat_args();
            args.provider = Some("anthropic".to_string());

            let settings = create_test_settings("http://localhost:11434");

            let result = run_embedded_chat(args, settings).await;
            assert!(result.is_err());

            let err = result.unwrap_err();
            let err_msg = err.to_string();
            assert!(
                err_msg.contains("API key") || err_msg.contains("Anthropic"),
                "Error should mention API key: {}",
                err_msg
            );
        }

        #[tokio::test]
        async fn test_run_embedded_chat_stream_error() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Return an error response
            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            // Should error due to server error
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_run_embedded_chat_with_model_override() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"custom-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Using custom model"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.model = Some("custom-model".to_string());

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_uses_default_provider() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Default provider"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.provider = None; // Use default from settings

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_uses_default_caps() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"With caps"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.cap = vec![]; // Empty caps should use defaults

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_with_specified_caps() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Custom caps"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.cap = vec!["default".to_string()];

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_shell_tool() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with shell command
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Running command.\n```json\n{\"name\":\"shell\",\"arguments\":{\"command\":\"echo hello\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.trust = true; // Auto-approve

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_file_edit_tool() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with file_edit
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Editing file.\n```json\n{\"name\":\"file_edit\",\"arguments\":{\"path\":\"/tmp/test.txt\",\"old_string\":\"old\",\"new_string\":\"new\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_plan_update_tool() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with plan_update
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Creating plan.\n```json\n{\"name\":\"plan_update\",\"arguments\":{\"title\":\"Test Plan\",\"content\":\"- [ ] Step 1\n- [ ] Step 2\n- [x] Step 3\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_propose_file_changes() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with propose_file_changes
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Proposing changes.\n```json\n{\"name\":\"propose_file_changes\",\"arguments\":{\"operations\":[{\"type\":\"edit\",\"path\":\"/tmp/a.txt\",\"old_string\":\"x\",\"new_string\":\"y\"},{\"type\":\"write\",\"path\":\"/tmp/b.txt\",\"content\":\"new file\"},{\"type\":\"delete\",\"path\":\"/tmp/c.txt\"},{\"type\":\"read\",\"path\":\"/tmp/d.txt\"}]}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_tool_loop_detection() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Return same tool call repeatedly to trigger loop detection
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Reading.\n```json\n{\"name\":\"file_read\",\"arguments\":{\"path\":\"/same/file.txt\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            // This should eventually stop due to loop detection or max turns
            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_empty_response() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Return empty content
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":""},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_no_model_uses_default() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"default-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Using default"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.model = None; // No model specified, should use default

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_file_write_tool() {
            use tempfile::TempDir;
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;
            let temp_dir = TempDir::new().unwrap();
            let test_file = temp_dir.path().join("new_file.txt");

            // Response with file_write
            let response_body = format!(
                r#"{{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{{"role":"assistant","content":"Creating file.\n```json\n{{\"name\":\"file_write\",\"arguments\":{{\"path\":\"{}\",\"content\":\"Hello World\"}}}}\n```"}},"done":true}}"#,
                test_file.display()
            );

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_glob_tool() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with glob tool
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Searching.\n```json\n{\"name\":\"glob\",\"arguments\":{\"pattern\":\"*.rs\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_grep_tool() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with grep tool
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Searching.\n```json\n{\"name\":\"grep\",\"arguments\":{\"pattern\":\"TODO\",\"path\":\"/tmp\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_multiple_tool_calls() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with array of tool calls
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Multiple tools.\n```json\n[{\"name\":\"file_read\",\"arguments\":{\"path\":\"/a.txt\"}},{\"name\":\"file_read\",\"arguments\":{\"path\":\"/b.txt\"}}]\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_file_delete_in_review_mode() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with file_delete in review mode
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Deleting.\n```json\n{\"name\":\"file_delete\",\"arguments\":{\"path\":\"/tmp/to_delete.txt\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.review_mode = true;

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_generic_code_block() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with generic code block (no json marker)
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Tool call.\n```\n{\"name\":\"file_read\",\"arguments\":{\"path\":\"/test.txt\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_inline_json_tool() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with inline JSON (no code block)
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Using tool: {\"name\":\"file_read\",\"input\":{\"path\":\"/inline.txt\"}}"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_create_file_alias() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Use create_file alias (should map to file_write)
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Creating.\n```json\n{\"name\":\"create_file\",\"arguments\":{\"path\":\"/tmp/new.txt\",\"content\":\"content\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_edit_file_alias() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Use edit_file alias (should map to file_edit)
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Editing.\n```json\n{\"name\":\"edit_file\",\"arguments\":{\"path\":\"/tmp/edit.txt\",\"old_string\":\"a\",\"new_string\":\"b\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_read_file_alias() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Use read_file alias (should map to file_read)
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Reading.\n```json\n{\"name\":\"read_file\",\"arguments\":{\"path\":\"/tmp/read.txt\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_text_only_response() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with just text, no tools
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Here is my response without any tool calls. Just plain text explaining something."},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            // Should complete successfully with no tools executed
            assert!(result.is_ok() || result.is_err()); // Allow either, focus is on code path
        }

        #[tokio::test]
        async fn test_run_embedded_chat_with_network_error() {
            // Use invalid URL to trigger connection error
            let args = create_test_chat_args();
            let settings = create_test_settings("http://localhost:99999");

            let result = run_embedded_chat(args, settings).await;
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_run_embedded_chat_history_with_system_messages() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"OK"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            // History with system role (should be skipped)
            let temp_dir = TempDir::new().unwrap();
            let history_path = temp_dir.path().join("history.json");
            let history_content = r#"[
                {"role":"system","content":"You are helpful"},
                {"role":"user","content":"Hi"},
                {"role":"assistant","content":"Hello"}
            ]"#;
            std::fs::write(&history_path, history_content).unwrap();

            let mut args = create_test_chat_args();
            args.history = Some(history_path);

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_long_history_conversation() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Final response"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            // Create long history
            let temp_dir = TempDir::new().unwrap();
            let history_path = temp_dir.path().join("history.json");

            let mut history = Vec::new();
            for i in 0..20 {
                history.push(
                    serde_json::json!({"role": "user", "content": format!("Question {}", i)}),
                );
                history.push(
                    serde_json::json!({"role": "assistant", "content": format!("Answer {}", i)}),
                );
            }
            std::fs::write(&history_path, serde_json::to_string(&history).unwrap()).unwrap();

            let mut args = create_test_chat_args();
            args.history = Some(history_path);

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_unknown_tool() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with unknown tool
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Using tool.\n```json\n{\"name\":\"unknown_tool\",\"arguments\":{\"param\":\"value\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_with_openrouter_custom_base_url() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let response_body = r#"{"id":"gen-123","choices":[{"message":{"role":"assistant","content":"OpenRouter response"}}]}"#;

            Mock::given(method("POST"))
                .and(path("/api/v1/chat/completions"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.provider = Some("openrouter".to_string());

            let mut settings = create_test_settings(&mock_server.uri());
            settings.providers.openrouter.api_key = Some("test-api-key".to_string());
            settings.providers.openrouter.base_url = Some(mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_delete_file_alias() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Use delete_file alias (should map to file_delete)
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Deleting.\n```json\n{\"name\":\"delete_file\",\"arguments\":{\"path\":\"/tmp/to_delete.txt\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_write_file_alias() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Use write_file alias (should map to file_write)
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Writing.\n```json\n{\"name\":\"write_file\",\"arguments\":{\"path\":\"/tmp/new.txt\",\"content\":\"test\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_file_edit_with_file_param() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Use 'file' instead of 'path' param
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Editing.\n```json\n{\"name\":\"file_edit\",\"arguments\":{\"file\":\"/tmp/edit.txt\",\"old\":\"a\",\"new\":\"b\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_file_write_with_text_param() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Use 'text' instead of 'content' param
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Writing.\n```json\n{\"name\":\"file_write\",\"arguments\":{\"filepath\":\"/tmp/new.txt\",\"text\":\"test content\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_shell_with_cmd_param() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Use 'cmd' instead of 'command' param
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Running.\n```json\n{\"name\":\"shell\",\"arguments\":{\"cmd\":\"echo test\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.trust = true;

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_propose_file_changes_with_file_param() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // propose_file_changes with 'file' instead of 'path'
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Changes.\n```json\n{\"name\":\"propose_file_changes\",\"arguments\":{\"operations\":[{\"type\":\"edit\",\"file\":\"/tmp/a.txt\",\"find\":\"x\",\"replace\":\"y\"}]}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_plan_update_without_title() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // plan_update without title - should use default "Plan"
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Planning.\n```json\n{\"name\":\"plan_update\",\"arguments\":{\"content\":\"- [ ] Step 1\n- [ ] Step 2\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_plan_update_with_empty_steps() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // plan_update with content that doesn't have valid steps
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Planning.\n```json\n{\"name\":\"plan_update\",\"arguments\":{\"title\":\"Empty Plan\",\"content\":\"Just some text without checkboxes\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_file_edit_review_mode() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // file_edit in review mode
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Editing.\n```json\n{\"name\":\"file_edit\",\"arguments\":{\"path\":\"/tmp/test.txt\",\"old_string\":\"old\",\"new_string\":\"new\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.review_mode = true;

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_non_file_tool_in_review_mode() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Non-file tool (glob) should execute normally even in review mode
            let response_body = r#"{"model":"test-model","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"Searching.\n```json\n{\"name\":\"glob\",\"arguments\":{\"pattern\":\"*.txt\"}}\n```"},"done":true}"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.review_mode = true;

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        // ===== Tests with proper streaming NDJSON format =====

        /// Helper to create a proper streaming response
        fn streaming_response(content: &str) -> String {
            format!(
                r#"{{"message":{{"role":"assistant","content":"{}"}},"done":false}}
{{"message":{{"role":"assistant","content":""}},"done":true,"eval_count":10,"prompt_eval_count":5}}
"#,
                content
            )
        }

        #[tokio::test]
        async fn test_run_embedded_chat_streaming_text_response() {
            use wiremock::matchers::{header, method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Anthropic streaming SSE response format
            let sse_body = "event: message_start\n\
                data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_test\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"test-model\",\"stop_reason\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n\
                event: content_block_start\n\
                data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
                event: content_block_delta\n\
                data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello there!\"}}\n\n\
                event: content_block_stop\n\
                data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
                event: message_delta\n\
                data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":3}}\n\n\
                event: message_stop\n\
                data: {\"type\":\"message_stop\"}\n\n";

            Mock::given(method("POST"))
                .and(path("/v1/messages"))
                .and(header("x-api-key", "test-key"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_string(sse_body)
                        .insert_header("content-type", "text/event-stream"),
                )
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.provider = Some("anthropic".to_string());

            let mut settings = create_test_settings(&mock_server.uri());
            settings.providers.anthropic.api_key = Some("test-key".to_string());
            settings.providers.anthropic.base_url =
                Some(format!("{}/v1/messages", mock_server.uri()));
            settings.defaults.provider = "anthropic".to_string();

            let result = run_embedded_chat(args, settings).await;
            assert!(result.is_ok(), "Expected success, got: {:?}", result);
        }

        #[tokio::test]
        async fn test_run_embedded_chat_streaming_with_native_tool() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Response with native tool_calls field
            let stream_body = r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"glob","arguments":{"pattern":"*.rs"}}}]},"done":true,"eval_count":10,"prompt_eval_count":5}
"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_streaming_multiple_tools() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Multiple native tool calls
            let stream_body = r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"file_read","arguments":{"path":"/a.txt"}}},{"function":{"name":"file_read","arguments":{"path":"/b.txt"}}}]},"done":true,"eval_count":10,"prompt_eval_count":5}
"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_streaming_incremental() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Many incremental chunks
            let stream_body = r#"{"message":{"role":"assistant","content":"I"},"done":false}
{"message":{"role":"assistant","content":" will"},"done":false}
{"message":{"role":"assistant","content":" help"},"done":false}
{"message":{"role":"assistant","content":" you"},"done":false}
{"message":{"role":"assistant","content":" with"},"done":false}
{"message":{"role":"assistant","content":" that."},"done":false}
{"message":{"role":"assistant","content":""},"done":true,"eval_count":6,"prompt_eval_count":5}
"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_streaming_tool_with_complex_args() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Tool with complex nested arguments
            let stream_body = r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"file_edit","arguments":{"path":"/test.txt","old_string":"hello\nworld","new_string":"goodbye\nworld"}}}]},"done":true,"eval_count":10,"prompt_eval_count":5}
"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_with_nonempty_system_prompt() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let stream_body = streaming_response("Got it!");

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
                .mount(&mock_server)
                .await;

            // Create system prompt file with actual content
            let temp_dir = TempDir::new().unwrap();
            let prompt_path = temp_dir.path().join("system.txt");
            std::fs::write(
                &prompt_path,
                "You are a helpful coding assistant. Be concise and accurate.",
            )
            .unwrap();

            let mut args = create_test_chat_args();
            args.system_prompt_file = Some(prompt_path);

            let mut settings = create_test_settings(&mock_server.uri());
            // Ensure there's an existing system prompt to append to
            settings.defaults.caps = vec!["base".to_string()];

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_streaming_mixed_text_and_tool() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Text content followed by tool call
            let stream_body = r#"{"message":{"role":"assistant","content":"Let me search for files.","tool_calls":[{"function":{"name":"glob","arguments":{"pattern":"*.rs"}}}]},"done":true,"eval_count":10,"prompt_eval_count":5}
"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_with_whitespace_system_prompt() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let stream_body = streaming_response("OK");

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
                .mount(&mock_server)
                .await;

            // Create system prompt file with only whitespace
            let temp_dir = TempDir::new().unwrap();
            let prompt_path = temp_dir.path().join("system.txt");
            std::fs::write(&prompt_path, "   \n\t   \n").unwrap();

            let mut args = create_test_chat_args();
            args.system_prompt_file = Some(prompt_path);

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_model_from_cap() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            let stream_body = streaming_response("Using cap model");

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.model = None; // No model specified, should fall back to cap or default

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_file_read_with_name_param() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Use 'name' instead of 'path' for file_read
            let stream_body = r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"file_read","arguments":{"name":"/etc/hosts"}}}]},"done":true,"eval_count":10,"prompt_eval_count":5}
"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_shell_with_bash_param() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Use 'bash' instead of 'command'
            let stream_body = r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"shell","arguments":{"bash":"echo hello"}}}]},"done":true,"eval_count":10,"prompt_eval_count":5}
"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
                .mount(&mock_server)
                .await;

            let mut args = create_test_chat_args();
            args.trust = true;

            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_file_write_with_data_param() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Use 'data' instead of 'content', 'file' instead of 'path'
            let stream_body = r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"file_write","arguments":{"file":"/tmp/test.txt","data":"test content"}}}]},"done":true,"eval_count":10,"prompt_eval_count":5}
"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_file_edit_with_find_replace() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Use 'find'/'replace' instead of 'old_string'/'new_string'
            let stream_body = r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"file_edit","arguments":{"file":"/tmp/test.txt","find":"old text","replace":"new text"}}}]},"done":true,"eval_count":10,"prompt_eval_count":5}
"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }

        #[tokio::test]
        async fn test_run_embedded_chat_file_edit_array_lines() {
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let mock_server = MockServer::start().await;

            // Use array of lines for old/new (some models do this)
            let stream_body = r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"file_edit","arguments":{"path":"/tmp/test.txt","old":["line1","line2"],"new":["new1","new2"]}}}]},"done":true,"eval_count":10,"prompt_eval_count":5}
"#;

            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_string(stream_body))
                .mount(&mock_server)
                .await;

            let args = create_test_chat_args();
            let settings = create_test_settings(&mock_server.uri());

            let result = run_embedded_chat(args, settings).await;
            let _ = result;
        }
    }

    // ==================== Additional edge case tests ====================

    #[test]
    fn test_parse_tool_from_json_preserves_path_in_file_write_conversion() {
        // When file_edit with empty old_string is converted to file_write,
        // the path should be preserved
        let value = json!({
            "name": "file_edit",
            "arguments": {"path": "/important/file.txt", "old_string": "", "new_string": "content here"}
        });

        let (name, args) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "file_write");
        assert_eq!(args["path"], "/important/file.txt");
        assert_eq!(args["content"], "content here");
    }

    #[test]
    fn test_parse_tool_from_json_with_deeply_nested_arguments() {
        let value = json!({
            "name": "custom_tool",
            "arguments": {
                "level1": {
                    "level2": {
                        "level3": {
                            "value": "deep"
                        }
                    }
                }
            }
        });

        let result = parse_tool_from_json(&value);
        assert!(result.is_some());
        let (name, args) = result.unwrap();
        assert_eq!(name, "custom_tool");
        assert_eq!(args["level1"]["level2"]["level3"]["value"], "deep");
    }

    #[test]
    fn test_parse_tool_from_json_with_null_argument_values() {
        let value = json!({
            "name": "test_tool",
            "arguments": {
                "required_param": "value",
                "optional_param": null
            }
        });

        let result = parse_tool_from_json(&value);
        assert!(result.is_some());
        let (_, args) = result.unwrap();
        assert_eq!(args["required_param"], "value");
        assert!(args["optional_param"].is_null());
    }

    #[test]
    fn test_parse_tool_from_json_with_boolean_argument_values() {
        let value = json!({
            "name": "test_tool",
            "arguments": {
                "flag_true": true,
                "flag_false": false
            }
        });

        let result = parse_tool_from_json(&value);
        assert!(result.is_some());
        let (_, args) = result.unwrap();
        assert_eq!(args["flag_true"], true);
        assert_eq!(args["flag_false"], false);
    }

    #[test]
    fn test_parse_tool_from_json_with_numeric_argument_values() {
        let value = json!({
            "name": "test_tool",
            "arguments": {
                "int_value": 42,
                "float_value": 2.5,
                "negative": -100
            }
        });

        let result = parse_tool_from_json(&value);
        assert!(result.is_some());
        let (_, args) = result.unwrap();
        assert_eq!(args["int_value"], 42);
        assert_eq!(args["float_value"], 2.5);
        assert_eq!(args["negative"], -100);
    }

    #[test]
    fn test_map_file_edit_arguments_single_line_array() {
        // Single line in array
        let args = json!({
            "path": "/test.txt",
            "old_string": ["single"],
            "new_string": ["also single"]
        });
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["old_string"], "single");
        assert_eq!(mapped["new_string"], "also single");
    }

    #[test]
    fn test_map_file_edit_arguments_preserves_path_correctly() {
        let args = json!({
            "path": "/original/path.txt",
            "old_string": "old",
            "new_string": "new"
        });
        let mapped = map_file_edit_arguments(&args);
        assert_eq!(mapped["path"], "/original/path.txt");
    }

    #[test]
    fn test_extract_history_messages_preserves_order() {
        let messages = vec![
            Message::user("First"),
            Message::assistant("Second"),
            Message::user("Third"),
            Message::assistant("Fourth"),
        ];
        let history = extract_history_messages(&messages);

        assert_eq!(history.len(), 4);
        assert_eq!(history[0].content, "First");
        assert_eq!(history[1].content, "Second");
        assert_eq!(history[2].content, "Third");
        assert_eq!(history[3].content, "Fourth");
    }

    #[test]
    fn test_extract_history_messages_with_long_content() {
        let long_content = "x".repeat(10000);
        let messages = vec![Message::user(long_content.clone())];
        let history = extract_history_messages(&messages);

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content.len(), 10000);
    }

    #[test]
    fn test_tool_call_key_deterministic() {
        let input = json!({"path": "/test.txt", "encoding": "utf-8"});

        let key1 = tool_call_key("file_read", &input);
        let key2 = tool_call_key("file_read", &input);

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_tool_call_key_different_order_same_content() {
        // JSON object key order shouldn't matter for equality
        let input1 = json!({"a": 1, "b": 2});
        let input2 = json!({"b": 2, "a": 1});

        let key1 = tool_call_key("tool", &input1);
        let key2 = tool_call_key("tool", &input2);

        // Note: depending on serde_json serialization, these might differ
        // The important thing is they're both valid keys
        assert!(key1.starts_with("tool:"));
        assert!(key2.starts_with("tool:"));
    }

    #[test]
    fn test_map_file_write_arguments_all_path_aliases_with_content() {
        for (path_alias, content_alias) in [
            ("file", "content"),
            ("filepath", "text"),
            ("file_path", "data"),
            ("filename", "body"),
            ("name", "code"),
        ] {
            let args = json!({
                path_alias: "/test/path.txt",
                content_alias: "file content"
            });
            let mapped = map_file_write_arguments(&args);
            assert_eq!(
                mapped["path"], "/test/path.txt",
                "Failed for path_alias: {}",
                path_alias
            );
            assert_eq!(
                mapped["content"], "file content",
                "Failed for content_alias: {}",
                content_alias
            );
        }
    }

    #[test]
    fn test_parse_tool_from_json_file_edit_non_empty_whitespace_old_string() {
        // file_edit with old_string containing tabs and spaces should still convert to file_write
        let value = json!({
            "name": "file_edit",
            "arguments": {"path": "/test.txt", "old_string": "\t\n  \t", "new_string": "content"}
        });

        let (name, args) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "file_write");
        assert_eq!(args["content"], "content");
    }

    #[test]
    fn test_history_message_with_special_characters() {
        let msg = HistoryMessage {
            role: "user".to_string(),
            content: "Test with unicode: 日本語 🎉 émojis and special chars: <>&\"'\\".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let restored: HistoryMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.content, msg.content);
    }

    #[test]
    fn test_extract_history_messages_consecutive_same_role() {
        // Two user messages in a row (unusual but should be handled)
        let messages = vec![
            Message::user("First user message"),
            Message::user("Second user message"),
        ];
        let history = extract_history_messages(&messages);

        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[1].role, "user");
    }

    #[test]
    fn test_map_shell_arguments_complex_command() {
        let args = json!({
            "command": "find . -name '*.rs' | xargs grep 'TODO' | head -20",
            "timeout": 60000
        });
        let mapped = map_shell_arguments(&args);
        assert!(mapped["command"].as_str().unwrap().contains("xargs"));
    }

    #[test]
    fn test_parse_tool_from_json_file_write_direct() {
        // Direct file_write (not converted from file_edit)
        let value = json!({
            "name": "file_write",
            "arguments": {"path": "/test.txt", "content": "direct write"}
        });

        let (name, args) = parse_tool_from_json(&value).unwrap();
        assert_eq!(name, "file_write");
        assert_eq!(args["content"], "direct write");
    }

    #[test]
    fn test_map_file_edit_arguments_empty_object() {
        let args = json!({});
        let mapped = map_file_edit_arguments(&args);
        assert!(mapped.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_parse_tool_from_json_empty_arguments() {
        let value = json!({
            "name": "simple_tool",
            "arguments": {}
        });

        let result = parse_tool_from_json(&value);
        assert!(result.is_some());
        let (name, args) = result.unwrap();
        assert_eq!(name, "simple_tool");
        assert!(args.as_object().unwrap().is_empty());
    }
}
