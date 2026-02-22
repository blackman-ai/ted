// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Embedded mode chat runner
//!
//! Runs Ted in embedded mode, outputting JSONL events instead of interactive TUI.

use regex::Regex;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;

mod history;
mod tooling;

use self::history::{extract_history_messages, HistoryMessage};
use self::tooling::{is_file_mod_tool, parse_tool_from_json, EmbeddedToolExecutionStrategy};
#[cfg(test)]
use self::tooling::{
    map_file_edit_arguments, map_file_read_arguments, map_file_write_arguments,
    map_shell_arguments, tool_call_key,
};

use crate::caps::{CapLoader, CapResolver};
use crate::chat;
use crate::cli::ChatArgs;
use crate::config::Settings;
use crate::context::memory::MemoryStore;
use crate::context::{recall, summarizer};
use crate::embedded::{JsonLEmitter, PlanStep};
use crate::embeddings::EmbeddingGenerator;
use crate::error::{ApiError, Result, TedError};
use crate::llm::message::{ContentBlock, Conversation, Message, MessageContent};
use crate::llm::provider::{ContentBlockResponse, LlmProvider};
use crate::llm::providers::{
    AnthropicProvider, BlackmanProvider, LocalProvider, OpenRouterProvider,
};
use crate::models::download::BinaryDownloader;
use crate::tools::{ShellOutputEvent, ToolContext, ToolExecutor};

/// Embedded-mode observer for shared chat engine streaming callbacks.
struct EmbeddedStreamObserver {
    emitter: Arc<JsonLEmitter>,
}

impl chat::AgentLoopObserver for EmbeddedStreamObserver {
    fn on_text_delta(&mut self, text: &str) -> Result<()> {
        self.emitter
            .emit_message("assistant", text.to_string(), Some(true))?;
        Ok(())
    }

    fn on_rate_limited(&mut self, delay_secs: u64, attempt: u32, max_retries: u32) -> Result<()> {
        self.emitter.emit_status(
            "thinking",
            format!(
                "Rate limited. Retrying in {}s ({}/{})...",
                delay_secs, attempt, max_retries
            ),
            None,
        )?;
        Ok(())
    }

    fn on_context_too_long(&mut self, current: u32, limit: u32) -> Result<()> {
        eprintln!(
            "[CONTEXT] Context too long ({} tokens > {} limit). Auto-trimming...",
            current, limit
        );
        Ok(())
    }

    fn on_context_trimmed(&mut self, removed: usize) -> Result<()> {
        if removed > 0 {
            eprintln!("[CONTEXT] Removed {} older messages. Retrying...", removed);
            self.emitter.emit_status(
                "thinking",
                format!(
                    "Context trimmed ({} messages removed). Retrying...",
                    removed
                ),
                None,
            )?;
        }
        Ok(())
    }
}

pub async fn run_embedded_chat(args: ChatArgs, settings: Settings) -> Result<()> {
    let session_id = uuid::Uuid::new_v4();
    let emitter = Arc::new(JsonLEmitter::new(session_id.to_string()));
    run_embedded_chat_with_emitter(args, settings, session_id, emitter).await
}

async fn run_embedded_chat_with_emitter(
    args: ChatArgs,
    settings: Settings,
    session_id: uuid::Uuid,
    emitter: Arc<JsonLEmitter>,
) -> Result<()> {
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

            if let Some(base_url) = cfg
                .base_url
                .as_ref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
            {
                let local_provider = LocalProvider::with_external_server(
                    base_url.trim_end_matches('/').to_string(),
                    cfg.default_model.clone(),
                    cfg.ctx_size,
                );
                Box::new(local_provider)
            } else {
                // Resolve model path: explicit config → system scan → error
                let model_path = if cfg.model_path.exists() {
                    cfg.model_path.clone()
                } else {
                    let discovered = crate::models::scanner::scan_for_models();
                    if discovered.is_empty() {
                        return Err(TedError::Config(
                            "No GGUF model files found. Place one at ~/.ted/models/local/model.gguf or set settings.providers.local.model_path.".to_string(),
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

    tracing::info!(
        target: "ted.embedded",
        session_id = %session_id,
        provider = %provider_name,
        model = %model,
        review_mode,
        trust_mode = args.trust,
        caps = cap_names.len(),
        "starting embedded chat run"
    );

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
    let mut tool_executor = if args.no_tools {
        eprintln!("[TOOLS] Disabled for this turn (--no-tools)");
        ToolExecutor::new_without_tools(tool_context, args.trust)
    } else {
        ToolExecutor::new(tool_context, args.trust)
    };
    let tool_definitions = if args.no_tools {
        Vec::new()
    } else {
        tool_executor.tool_definitions()
    };

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
    let emitter_clone = Arc::clone(&emitter);
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

    // Track if any tools were actually executed (for completion message)
    let mut tools_executed = 0;
    let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut tool_call_tracker = chat::ToolCallTracker::new(chat::engine::MAX_RECENT_TOOL_CALLS);

    // Main agent loop
    let max_turns = 25;
    for turn_num in 0..max_turns {
        tracing::debug!(
            target: "ted.embedded",
            turn = turn_num + 1,
            max_turns,
            message_count = messages.len(),
            "embedded turn start"
        );

        let mut conversation = Conversation::new();
        conversation.messages = messages.clone();
        if !merged_cap.system_prompt.is_empty() {
            conversation.set_system(&merged_cap.system_prompt);
        }

        let mut observer = EmbeddedStreamObserver {
            emitter: Arc::clone(&emitter),
        };

        let response = chat::engine::get_response_with_context_retry(
            provider.as_ref(),
            &model,
            &mut conversation,
            8192,
            0.7,
            tool_definitions.clone(),
            true,
            &[],
            &mut observer,
        )
        .await;

        let (response_content, _stop_reason) = match response {
            Ok(result) => result,
            Err(TedError::Api(ApiError::ServerError { message, .. })) => {
                emitter.emit_error("llm_error".to_string(), message.clone(), None, None)?;
                return Err(TedError::Config(format!("LLM error - {}", message)));
            }
            Err(e) => return Err(e),
        };

        // Keep trimmed state produced by shared context-retry logic.
        messages = conversation.messages;

        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        let mut current_text = String::new();
        let mut tool_uses: Vec<(String, String, serde_json::Value)> = Vec::new();
        let mut suppressed_native_tool_uses = false;

        for block in &response_content {
            match block {
                ContentBlockResponse::Text { text } => {
                    if !text.is_empty() {
                        current_text.push_str(text);
                    }
                }
                ContentBlockResponse::ToolUse { id, name, input } => {
                    if args.no_tools {
                        suppressed_native_tool_uses = true;
                        tracing::debug!(
                            target: "ted.embedded",
                            turn = turn_num + 1,
                            tool_name = %name,
                            "ignored native tool call because --no-tools is enabled"
                        );
                        continue;
                    }
                    let normalized_input = crate::chat::agent::normalize_tool_use_input(input);
                    // Normalize tool names/arguments across model-specific variants.
                    if let Some((mapped_name, mapped_input)) =
                        parse_tool_from_json(&serde_json::json!({
                            "name": name,
                            "arguments": normalized_input
                        }))
                    {
                        tool_uses.push((id.clone(), mapped_name, mapped_input));
                    } else {
                        tool_uses.push((id.clone(), name.clone(), normalized_input));
                    }
                }
            };
        }

        // Local/smaller models may emit JSON tool calls inside text instead of native tool blocks.
        // Parse those so we can still execute actions instead of stopping at prose.
        if !args.no_tools && tool_uses.is_empty() && !current_text.trim().is_empty() {
            let inferred_tool_uses = extract_tool_uses_from_text(&current_text);
            if !inferred_tool_uses.is_empty() {
                tracing::info!(
                    target: "ted.embedded",
                    turn = turn_num + 1,
                    inferred_tool_calls = inferred_tool_uses.len(),
                    "inferred tool calls from assistant text fallback"
                );
                for (idx, (name, input)) in inferred_tool_uses.into_iter().enumerate() {
                    tool_uses.push((
                        format!("text_tool_{}_{}", turn_num + 1, idx + 1),
                        name,
                        input,
                    ));
                }
            }
        }

        if args.no_tools && suppressed_native_tool_uses && current_text.trim().is_empty() {
            current_text =
                "I can chat and help plan. Ask me to build when you're ready.".to_string();
        }

        // Debug: Log if we have empty response
        if tool_uses.is_empty() && current_text.trim().is_empty() {
            tracing::warn!(
                target: "ted.embedded",
                turn = turn_num + 1,
                "empty model response: no text and no tool calls"
            );
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
                tracing::warn!(
                    target: "ted.embedded",
                    error = %e,
                    "failed to emit final conversation history"
                );
            }
            break;
        }

        // Execute tools and collect results
        emitter.emit_status(
            "running",
            format!("Executing {} tool(s)...", tool_uses.len()),
            None,
        )?;
        tracing::info!(
            target: "ted.embedded",
            turn = turn_num + 1,
            tool_calls = tool_uses.len(),
            "executing tool batch"
        );

        // Emit tool preview events for file operations
        for (id, name, input) in &tool_uses {
            tracing::debug!(
                target: "ted.embedded",
                turn = turn_num + 1,
                tool_use_id = %id,
                tool_name = %name,
                "tool call requested"
            );
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

        let mut strategy = EmbeddedToolExecutionStrategy { review_mode };
        let mut observer = chat::NoopAgentLoopObserver;
        let outcome = chat::engine::execute_tool_uses_with_strategy(
            &tool_uses,
            &mut tool_executor,
            &interrupted,
            &mut tool_call_tracker,
            &mut observer,
            &mut strategy,
        )
        .await?;

        tools_executed += outcome.executed_calls.len();
        let has_review_file_mods = review_mode
            && outcome
                .executed_calls
                .iter()
                .any(|(_, name, _)| is_file_mod_tool(name));

        if outcome.loop_detected {
            emitter.emit_status(
                "thinking",
                "Detected repeated tool call loop; asking model to try another path...".to_string(),
                None,
            )?;
        }

        #[cfg(debug_assertions)]
        for result in &outcome.results {
            eprintln!(
                "[DEBUG] Tool result - is_error: {}, output: {}",
                result.is_error(),
                result.output_text()
            );
        }

        let tool_result_blocks: Vec<ContentBlock> = outcome
            .results
            .into_iter()
            .map(|result| {
                let output = result.output_text().to_string();
                let is_error = result.is_error();
                crate::chat::agent::tool_result_block(result.tool_use_id, output, is_error)
            })
            .collect();

        // Add tool results as user message
        messages.push(Message::user_blocks(tool_result_blocks));

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

fn extract_tool_uses_from_text(text: &str) -> Vec<(String, serde_json::Value)> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    let mut seen_candidates = HashSet::new();

    let mut add_candidate = |candidate: &str| {
        let c = candidate.trim();
        if c.is_empty() || seen_candidates.contains(c) {
            return;
        }
        seen_candidates.insert(c.to_string());
        candidates.push(c.to_string());
    };

    add_candidate(trimmed);

    let code_fence_re =
        Regex::new(r"(?s)```(?:json)?\s*(.*?)\s*```").expect("code fence regex should be valid");
    for capture in code_fence_re.captures_iter(trimmed) {
        if let Some(body) = capture.get(1) {
            add_candidate(body.as_str());
        }
    }

    let mut tool_uses: Vec<(String, serde_json::Value)> = Vec::new();
    let mut seen_tool_calls = HashSet::new();

    for candidate in candidates {
        if let Some(value) = parse_json_candidate(&candidate) {
            collect_tool_uses_from_value(&value, &mut tool_uses, &mut seen_tool_calls);
        }
    }

    tool_uses
}

fn parse_json_candidate(candidate: &str) -> Option<serde_json::Value> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(candidate) {
        return Some(value);
    }

    let first_brace = candidate.find('{')?;
    let last_brace = candidate.rfind('}')?;
    if last_brace <= first_brace {
        return None;
    }

    serde_json::from_str::<serde_json::Value>(&candidate[first_brace..=last_brace]).ok()
}

fn collect_tool_uses_from_value(
    value: &serde_json::Value,
    tool_uses: &mut Vec<(String, serde_json::Value)>,
    seen_tool_calls: &mut HashSet<String>,
) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                collect_tool_uses_from_value(item, tool_uses, seen_tool_calls);
            }
        }
        serde_json::Value::Object(obj) => {
            if let Some((name, input)) = parse_tool_from_json(value) {
                let key = format!("{}:{}", name, input);
                if seen_tool_calls.insert(key) {
                    tool_uses.push((name, input));
                }
            }

            // OpenAI-compatible tool call chunk: {"function":{"name":"...","arguments":...}}
            if let Some(function) = obj.get("function").and_then(|v| v.as_object()) {
                if let Some(name) = function.get("name").and_then(|v| v.as_str()) {
                    let synthetic = serde_json::json!({
                        "name": name,
                        "arguments": function.get("arguments").cloned().unwrap_or_else(|| serde_json::json!({}))
                    });
                    if let Some((mapped_name, mapped_input)) = parse_tool_from_json(&synthetic) {
                        let key = format!("{}:{}", mapped_name, mapped_input);
                        if seen_tool_calls.insert(key) {
                            tool_uses.push((mapped_name, mapped_input));
                        }
                    }
                }
            }

            for key in ["tool_calls", "calls", "actions", "content"] {
                if let Some(nested) = obj.get(key) {
                    collect_tool_uses_from_value(nested, tool_uses, seen_tool_calls);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
