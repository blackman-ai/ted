// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ratatui::prelude::*;

use crate::config::Settings;
use crate::context::ContextManager;
use crate::error::{Result, TedError};
use crate::llm::message::{ContentBlock, Conversation, Message};
use crate::llm::provider::{ContentBlockResponse, LlmProvider, StopReason};
use crate::tools::ToolExecutor;
use crate::tui::chat::app::ChatMode;
use crate::tui::chat::state::DisplayMessage;

use super::execution::{
    TuiNonStreamObserver, TuiStreamObserver, TuiToolExecutionStrategy, TUI_STREAM_INTERRUPTED,
};
use super::render::draw_tui;
use super::TuiState;

/// Process LLM response and handle tool calls
#[allow(clippy::too_many_arguments)]
pub(super) async fn process_llm_response<B: Backend>(
    provider: &Arc<dyn LlmProvider>,
    model: &str,
    conversation: &mut Conversation,
    tool_executor: &mut ToolExecutor,
    settings: &Settings,
    context_manager: &ContextManager,
    state: &mut TuiState,
    stream_enabled: bool,
    interrupted: &Arc<AtomicBool>,
    terminal: &mut Terminal<B>,
) -> Result<bool> {
    let initial_message_count = conversation.messages.len();
    let mut tool_call_tracker =
        crate::chat::ToolCallTracker::new(crate::chat::engine::MAX_RECENT_TOOL_CALLS);
    let mut turn_index: usize = 0;

    let run_result: Result<bool> = async {
        loop {
            turn_index += 1;

            if interrupted.load(Ordering::SeqCst) {
                tracing::info!(
                    target: "ted.tui.runner",
                    turn = turn_index,
                    "turn processing interrupted before model request"
                );
                return Ok(false);
            }

            tracing::debug!(
                target: "ted.tui.runner",
                turn = turn_index,
                stream_enabled,
                pending_messages = state.pending_messages.len(),
                "starting TUI turn"
            );

            // Start assistant message in UI
            state.messages.push(DisplayMessage::assistant_streaming(
                state.enabled_caps.clone(),
            ));

            // Get response through shared chat engine request handling
            let fetch_response = if stream_enabled {
                let mut observer = TuiStreamObserver {
                    state,
                    terminal,
                    interrupted,
                };
                crate::chat::engine::get_response_with_context_retry(
                    provider.as_ref(),
                    model,
                    conversation,
                    settings.defaults.max_tokens,
                    settings.defaults.temperature,
                    tool_executor.tool_definitions(),
                    true,
                    &[],
                    &mut observer,
                )
                .await
            } else {
                let mut observer = TuiNonStreamObserver { state };
                crate::chat::engine::get_response_with_context_retry(
                    provider.as_ref(),
                    model,
                    conversation,
                    settings.defaults.max_tokens,
                    settings.defaults.temperature,
                    tool_executor.tool_definitions(),
                    false,
                    &[],
                    &mut observer,
                )
                .await
            };

            let (response_content, stop_reason) = match fetch_response {
                Ok((content, reason)) => (content, reason.unwrap_or(StopReason::EndTurn)),
                Err(TedError::Agent(msg)) if msg == TUI_STREAM_INTERRUPTED => {
                    if let Some(msg) = state.messages.last_mut() {
                        msg.finish_streaming();
                    }
                    return Ok(false);
                }
                Err(e) => return Err(e),
            };

            let mut response_text = String::new();
            let tool_uses = crate::chat::agent::extract_tool_uses_normalized(&response_content);
            tracing::debug!(
                target: "ted.tui.runner",
                turn = turn_index,
                response_blocks = response_content.len(),
                tool_uses = tool_uses.len(),
                ?stop_reason,
                "received model response in TUI"
            );
            for block in &response_content {
                if let ContentBlockResponse::Text { text } = block {
                    response_text.push_str(text);
                }
            }

            if !stream_enabled {
                if let Some(msg) = state.messages.last_mut() {
                    msg.content = response_text.clone();
                }

                // Refresh UI to show non-streaming response
                state.tick_animation();
                state.auto_scroll();
                let _ = terminal.draw(|f| draw_tui(f, state));
            }

            // Finish streaming message
            if let Some(msg) = state.messages.last_mut() {
                msg.finish_streaming();
            }

            if !response_text.is_empty() {
                context_manager
                    .store_message("assistant", &response_text, None)
                    .await?;
            }

            // Add assistant message to conversation with normalized tool inputs.
            let content_blocks: Vec<ContentBlock> = response_content
                .iter()
                .map(|block| match block {
                    ContentBlockResponse::Text { text } => {
                        ContentBlock::Text { text: text.clone() }
                    }
                    ContentBlockResponse::ToolUse { id, name, input } => ContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: crate::chat::agent::normalize_tool_use_input(input),
                    },
                })
                .collect();
            if !content_blocks.is_empty() {
                conversation.push(Message::assistant_blocks(content_blocks));
            }

            // Execute tool calls
            if !tool_uses.is_empty() && stop_reason == StopReason::ToolUse {
                tracing::info!(
                    target: "ted.tui.runner",
                    turn = turn_index,
                    tool_calls = tool_uses.len(),
                    "executing tool calls in TUI"
                );
                let mut observer = crate::chat::NoopAgentLoopObserver;
                let mut strategy = TuiToolExecutionStrategy { state, terminal };
                let outcome = crate::chat::engine::execute_tool_uses_with_strategy(
                    &tool_uses,
                    tool_executor,
                    interrupted,
                    &mut tool_call_tracker,
                    &mut observer,
                    &mut strategy,
                )
                .await?;

                if outcome.loop_detected {
                    state.set_status(
                        "Detected repeated tool call loop; asking model to try another path...",
                    );
                }

                tracing::info!(
                    target: "ted.tui.runner",
                    turn = turn_index,
                    tool_results = outcome.results.len(),
                    cancelled_tool_calls = outcome.cancelled_tool_use_ids.len(),
                    loop_detected = outcome.loop_detected,
                    "tool execution completed in TUI"
                );

                let cancelled_ids: std::collections::HashSet<String> =
                    outcome.cancelled_tool_use_ids.iter().cloned().collect();
                let executed_by_id: std::collections::HashMap<String, (String, serde_json::Value)> =
                    outcome
                        .executed_calls
                        .iter()
                        .map(|(id, name, input)| (id.clone(), (name.clone(), input.clone())))
                        .collect();
                for result in &outcome.results {
                    if cancelled_ids.contains(&result.tool_use_id) {
                        continue;
                    }
                    if let Some((name, input)) = executed_by_id.get(&result.tool_use_id) {
                        context_manager
                            .store_tool_call(
                                name,
                                input,
                                result.output_text(),
                                result.is_error(),
                                None,
                            )
                            .await?;
                    }
                }

                let tool_result_blocks: Vec<ContentBlock> = outcome
                    .results
                    .into_iter()
                    .map(|r| {
                        let output = r.output_text().to_string();
                        let is_error = r.is_error();
                        crate::chat::agent::tool_result_block(r.tool_use_id, output, is_error)
                    })
                    .collect();

                if !tool_result_blocks.is_empty() {
                    conversation.push(Message::user_blocks(tool_result_blocks));
                }

                // Continue loop to get next response
                continue;
            }

            // No more tool calls, we're done
            tracing::debug!(
                target: "ted.tui.runner",
                turn = turn_index,
                "completed TUI turn without additional tool loop"
            );
            return Ok(true);
        }
    }
    .await;

    // Keep TUI conversation state consistent with CLI behavior on interruption or error.
    if run_result.is_err() || matches!(run_result, Ok(false)) {
        conversation.messages.truncate(initial_message_count);
    }

    // Clear the agent split pane before returning from this user turn.
    state.focused_agent_tool_id = None;
    if state.mode == ChatMode::AgentFocus {
        state.mode = ChatMode::Input;
    }

    run_result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::context::SessionId;
    use crate::error::ApiError;
    use crate::llm::provider::{CompletionRequest, CompletionResponse, ModelInfo, Usage};
    use crate::tools::ToolContext;
    use crate::tui::chat::ChatTuiConfig;
    use async_trait::async_trait;
    use futures::stream::Stream;
    use ratatui::backend::TestBackend;
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::sync::Mutex;

    #[derive(Default)]
    struct SequenceProvider {
        completions: Mutex<VecDeque<Result<CompletionResponse>>>,
        stream_error: Mutex<Option<TedError>>,
    }

    impl SequenceProvider {
        fn with_completions(completions: Vec<CompletionResponse>) -> Self {
            let mut queue = VecDeque::new();
            for completion in completions {
                queue.push_back(Ok(completion));
            }
            Self {
                completions: Mutex::new(queue),
                stream_error: Mutex::new(None),
            }
        }

        fn with_stream_error(error: TedError) -> Self {
            Self {
                completions: Mutex::new(VecDeque::new()),
                stream_error: Mutex::new(Some(error)),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for SequenceProvider {
        fn name(&self) -> &str {
            "test-sequence"
        }

        fn available_models(&self) -> Vec<ModelInfo> {
            vec![ModelInfo {
                id: "test-model".to_string(),
                display_name: "Test Model".to_string(),
                context_window: 32_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
            }]
        }

        fn supports_model(&self, model: &str) -> bool {
            model == "test-model"
        }

        async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse> {
            self.completions
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| {
                    Err(TedError::Api(ApiError::InvalidResponse(
                        "no completion".to_string(),
                    )))
                })
        }

        async fn complete_stream(
            &self,
            _request: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<crate::llm::provider::StreamEvent>> + Send>>>
        {
            if let Some(error) = self.stream_error.lock().unwrap().take() {
                return Err(error);
            }
            Err(TedError::Api(ApiError::InvalidResponse(
                "stream not configured".to_string(),
            )))
        }

        fn count_tokens(&self, text: &str, _model: &str) -> Result<u32> {
            Ok((text.len() / 4).max(1) as u32)
        }
    }

    fn completion(
        content: Vec<ContentBlockResponse>,
        stop_reason: StopReason,
    ) -> CompletionResponse {
        CompletionResponse {
            id: "resp-test".to_string(),
            model: "test-model".to_string(),
            content,
            stop_reason: Some(stop_reason),
            usage: Usage::default(),
        }
    }

    fn make_state() -> TuiState {
        let settings = Settings::default();
        let config = ChatTuiConfig {
            session_id: uuid::Uuid::new_v4(),
            provider_name: "anthropic".to_string(),
            model: "test-model".to_string(),
            caps: vec!["base".to_string()],
            trust_mode: false,
            stream_enabled: true,
        };
        TuiState::new(config, &settings)
    }

    async fn make_context_manager() -> (tempfile::TempDir, ContextManager) {
        let temp = tempfile::TempDir::new().unwrap();
        let manager = ContextManager::new(temp.path().to_path_buf(), SessionId::new())
            .await
            .unwrap();
        (temp, manager)
    }

    fn make_terminal() -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(120, 40)).unwrap()
    }

    fn make_tool_executor(workdir: &std::path::Path) -> ToolExecutor {
        let ctx = ToolContext::new(
            workdir.to_path_buf(),
            Some(workdir.to_path_buf()),
            uuid::Uuid::new_v4(),
            true,
        );
        ToolExecutor::new(ctx, true)
    }

    #[tokio::test]
    async fn test_process_llm_response_interrupted_before_request() {
        let provider: Arc<dyn LlmProvider> = Arc::new(SequenceProvider::default());
        let settings = Settings::default();
        let (_temp, context_manager) = make_context_manager().await;
        let mut state = make_state();
        state.mode = ChatMode::AgentFocus;
        state.focused_agent_tool_id = Some("spawn_1".to_string());
        let mut conversation = Conversation::new();
        conversation.push(Message::user("hello"));
        let initial_len = conversation.messages.len();
        let mut tool_executor = make_tool_executor(&std::env::current_dir().unwrap());
        let interrupted = Arc::new(AtomicBool::new(true));
        let mut terminal = make_terminal();

        let result = process_llm_response(
            &provider,
            "test-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            &mut state,
            false,
            &interrupted,
            &mut terminal,
        )
        .await
        .unwrap();

        assert!(!result);
        assert_eq!(conversation.messages.len(), initial_len);
        assert_eq!(state.mode, ChatMode::Input);
        assert!(state.focused_agent_tool_id.is_none());
    }

    #[tokio::test]
    async fn test_process_llm_response_non_stream_text_completion() {
        let provider: Arc<dyn LlmProvider> =
            Arc::new(SequenceProvider::with_completions(vec![completion(
                vec![ContentBlockResponse::Text {
                    text: "assistant reply".to_string(),
                }],
                StopReason::EndTurn,
            )]));
        let settings = Settings::default();
        let (_temp, context_manager) = make_context_manager().await;
        let mut state = make_state();
        let mut conversation = Conversation::new();
        conversation.push(Message::user("hello"));
        let mut tool_executor = make_tool_executor(&std::env::current_dir().unwrap());
        let interrupted = Arc::new(AtomicBool::new(false));
        let mut terminal = make_terminal();

        let result = process_llm_response(
            &provider,
            "test-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            &mut state,
            false,
            &interrupted,
            &mut terminal,
        )
        .await
        .unwrap();

        assert!(result);
        assert!(state.messages.last().is_some());
        assert_eq!(state.messages.last().unwrap().content, "assistant reply");
        assert!(!state.messages.last().unwrap().is_streaming);
        assert!(matches!(
            conversation.messages.last().unwrap().content,
            crate::llm::message::MessageContent::Blocks(_)
        ));
    }

    #[tokio::test]
    async fn test_process_llm_response_handles_stream_interrupted_error() {
        let provider: Arc<dyn LlmProvider> = Arc::new(SequenceProvider::with_stream_error(
            TedError::Agent(TUI_STREAM_INTERRUPTED.to_string()),
        ));
        let settings = Settings::default();
        let (_temp, context_manager) = make_context_manager().await;
        let mut state = make_state();
        let mut conversation = Conversation::new();
        conversation.push(Message::user("hello"));
        let initial_len = conversation.messages.len();
        let mut tool_executor = make_tool_executor(&std::env::current_dir().unwrap());
        let interrupted = Arc::new(AtomicBool::new(false));
        let mut terminal = make_terminal();

        let result = process_llm_response(
            &provider,
            "test-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            &mut state,
            true,
            &interrupted,
            &mut terminal,
        )
        .await
        .unwrap();

        assert!(!result);
        assert_eq!(conversation.messages.len(), initial_len);
        assert!(!state.messages.last().unwrap().is_streaming);
    }

    #[tokio::test]
    async fn test_process_llm_response_executes_tool_use_then_finishes() {
        let temp = tempfile::TempDir::new().unwrap();
        let file_path = temp.path().join("sample.txt");
        std::fs::write(&file_path, "hello file").unwrap();

        let provider: Arc<dyn LlmProvider> = Arc::new(SequenceProvider::with_completions(vec![
            completion(
                vec![ContentBlockResponse::ToolUse {
                    id: "tool_1".to_string(),
                    name: "file_read".to_string(),
                    input: serde_json::json!({"path":"sample.txt"}),
                }],
                StopReason::ToolUse,
            ),
            completion(
                vec![ContentBlockResponse::Text {
                    text: "done".to_string(),
                }],
                StopReason::EndTurn,
            ),
        ]));
        let settings = Settings::default();
        let (_ctx_temp, context_manager) = make_context_manager().await;
        let mut state = make_state();
        let mut conversation = Conversation::new();
        conversation.push(Message::user("read the file"));
        let mut tool_executor = make_tool_executor(temp.path());
        let interrupted = Arc::new(AtomicBool::new(false));
        let mut terminal = make_terminal();

        let result = process_llm_response(
            &provider,
            "test-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            &mut state,
            false,
            &interrupted,
            &mut terminal,
        )
        .await
        .unwrap();

        assert!(result);
        assert!(conversation.messages.len() >= 4);
        let has_tool_result = conversation.messages.iter().any(|msg| {
            matches!(
                &msg.content,
                crate::llm::message::MessageContent::Blocks(blocks)
                    if blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }))
            )
        });
        assert!(has_tool_result);
        assert_eq!(state.messages.last().unwrap().content, "done");
    }
}
