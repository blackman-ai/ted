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
use crate::llm::providers::{AnthropicProvider, OllamaProvider};
use crate::tools::{ShellOutputEvent, ToolContext, ToolExecutor};

/// Simple message struct for history serialization
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct HistoryMessage {
    role: String,
    content: String,
}

/// Create a hash key for deduplicating tool calls
fn tool_call_key(name: &str, input: &serde_json::Value) -> String {
    format!("{}:{}", name, input)
}

pub async fn run_embedded_chat(args: ChatArgs, settings: Settings) -> Result<()> {
    // Get prompt (required in embedded mode)
    let prompt = args
        .prompt
        .clone()
        .ok_or_else(|| TedError::Config("Embedded mode requires a prompt argument".to_string()))?;

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

    // Main agent loop
    let max_turns = 25;
    for _turn in 0..max_turns {
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
        // emit the buffered text now
        if is_ollama && might_be_tool_call && tool_uses.is_empty() && !buffered_text.is_empty() {
            emitter.emit_message("assistant", buffered_text.clone(), Some(false))?;
        }

        // Add text content if any
        // For Ollama: if we detected tool uses and buffered text (meaning the text was JSON tool calls),
        // don't include the text in the message - it was just the JSON representation
        let should_include_text = if is_ollama && might_be_tool_call && !tool_uses.is_empty() {
            false // Text was JSON tool call output, don't include it
        } else {
            !current_text.is_empty()
        };

        // ENFORCEMENT: On first turn, model MUST ask questions before using tools
        // This is a hard enforcement for Ollama models that ignore system prompts
        // We check BEFORE adding anything to message history so we can reject the turn
        eprintln!("[ENFORCEMENT DEBUG] is_first_turn={}, is_ollama={}, tool_uses.len()={}, has_history={}",
            is_first_turn, is_ollama, tool_uses.len(), has_history);
        if is_first_turn && is_ollama && !tool_uses.is_empty() {
            // Check if the model asked questions (text contains '?' before tools)
            let text_before_tools = current_text.trim();
            let asked_questions = text_before_tools.contains('?') && text_before_tools.len() > 20;

            if !asked_questions {
                // Model jumped straight to tools without asking - reject the tool calls
                eprintln!("[ENFORCEMENT] First turn: Model used tools without asking questions. Rejecting.");

                // Tell the model to ask questions instead - don't emit anything to UI,
                // let the model generate its own questions
                let clarification_message = "STOP! You jumped straight to using tools without asking the user any questions.\n\n\
                    The user's request needs clarification first. Ask them questions like:\n\
                    - What style/design do they prefer?\n\
                    - What features are most important?\n\
                    - Do they have technical preferences?\n\
                    - What's the intended audience?\n\n\
                    Write your questions as plain text. Do NOT use any tools until the user answers.";

                messages.push(Message::user(clarification_message.to_string()));

                is_first_turn = false;
                continue; // Skip adding assistant message and tool execution - get model's next response
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
                || user_text.contains("your choice");

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
                || assistant_text.contains("let me know when");

            eprintln!("[ENFORCEMENT DEBUG] Build enforcement: user_wants_build={}, giving_instructions={}",
                user_wants_build, giving_instructions);
            eprintln!("[ENFORCEMENT DEBUG] User text: {}", user_text);
            eprintln!(
                "[ENFORCEMENT DEBUG] Assistant text (first 200): {}",
                &assistant_text[..assistant_text.len().min(200)]
            );

            if user_wants_build && giving_instructions {
                eprintln!("[ENFORCEMENT] User wants to build but model is giving instructions. Forcing tool use.");

                let build_message = "STOP! The user has already given you the information you need.\n\n\
                    Do NOT ask for more confirmation. Do NOT ask 'should I create...?' or 'would you like...?'\n\n\
                    You MUST start building NOW using your tools:\n\
                    1. Use file_write to create the project files\n\
                    2. Create index.html, styles.css, and any needed JS files\n\
                    3. Actually BUILD it - the user wants results, not questions\n\n\
                    START CREATING FILES NOW with file_write.";

                messages.push(Message::user(build_message.to_string()));
                continue; // Force model to try again with tools
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

        // If no tool uses, we're done
        if tool_uses.is_empty() {
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
                // Model is stuck in a loop after 3 consecutive repeats - break out
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
        for (id, name, input) in tool_uses {
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

    // Emit conversation history for multi-turn persistence
    // This allows Teddy to save the history and pass it back on the next turn
    let history_messages: Vec<HistoryMessageData> = messages
        .iter()
        .filter_map(|msg| {
            // Convert messages to simple role/content pairs
            // Skip tool use/result messages - we only need user/assistant text
            let role = match msg.role {
                crate::llm::message::Role::User => "user",
                crate::llm::message::Role::Assistant => "assistant",
                crate::llm::message::Role::System => return None, // Skip system messages
            };

            match &msg.content {
                MessageContent::Text(text) => Some(HistoryMessageData {
                    role: role.to_string(),
                    content: text.clone(),
                }),
                MessageContent::Blocks(blocks) => {
                    // Extract text content from blocks
                    let text: String = blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");

                    if !text.is_empty() {
                        Some(HistoryMessageData {
                            role: role.to_string(),
                            content: text,
                        })
                    } else {
                        None
                    }
                }
            }
        })
        .collect();

    emitter.emit_conversation_history(history_messages)?;

    emitter.emit_completion(is_task_complete, completion_message, files_changed)?;

    Ok(())
}
