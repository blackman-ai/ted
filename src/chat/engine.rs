// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Reusable non-TUI chat engine loop.
//!
//! This module centralizes the CLI-style agent loop, retry behavior, streaming
//! accumulation, and tool execution flow so multiple frontends can share it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;

use crate::chat::agent::{
    calculate_trim_target, extract_text_content, extract_tool_uses, format_loop_error,
    response_to_message_blocks, tool_result_block, ToolCallTracker,
};
use crate::chat::streaming::{StreamAccumulator, StreamEventResult};
use crate::config::Settings;
use crate::context::ContextManager;
use crate::error::{ApiError, Result, TedError};
use crate::llm::message::{ContentBlock, Conversation, Message, MessageContent, Role};
use crate::llm::provider::{
    CompletionRequest, ContentBlockResponse, LlmProvider, StopReason, ToolChoice, ToolDefinition,
};
use crate::tools::{ToolExecutor, ToolResult};

const MAX_RETRIES: u32 = 3;
const BASE_RETRY_DELAY: u64 = 2;
pub const MAX_CONSECUTIVE_IDENTICAL_CALLS: usize = 2;
pub const MAX_RECENT_TOOL_CALLS: usize = 10;
const TOOL_EXECUTION_DELAY_MS: u64 = 100;
const POST_TOOL_LOOP_DELAY_MS: u64 = 500;
const LOCAL_BUILDER_FALLBACK_TOOLS: &[&str] = &[
    "file_write",
    "file_edit",
    "file_read",
    "glob",
    "grep",
    "shell",
];
const LOCAL_BUILDER_REQUIRED_HINT: &str = "You are in builder execution mode. \
Respond using tool calls only. Do not output prose, markdown, or code blocks.";
const LOCAL_BUILDER_STRICT_HINT: &str = "MANDATORY: emit at least one valid tool call now \
(typically file_write first in an empty project). Return only tool calls.";
const BUILDER_ACTION_KEYWORDS: &[&str] = &[
    "build",
    "create",
    "make",
    "scaffold",
    "generate",
    "implement",
];
const BUILDER_ARTIFACT_KEYWORDS: &[&str] = &[
    "app",
    "application",
    "project",
    "site",
    "website",
    "blog",
    "dashboard",
    "api",
    "tool",
];

/// Tool use tuple `(id, name, input)`.
pub type ToolUse = (String, String, serde_json::Value);

fn latest_user_text(conversation: &Conversation) -> Option<&str> {
    conversation
        .messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .and_then(Message::text)
}

fn is_builder_intent(conversation: &Conversation) -> bool {
    let Some(text) = latest_user_text(conversation) else {
        return false;
    };
    let lower = text.to_lowercase();

    if lower.contains("[new project - empty directory]") {
        return true;
    }

    if lower.contains("file_write")
        || lower.contains("create files")
        || lower.contains("start creating files")
    {
        return true;
    }

    let has_action = BUILDER_ACTION_KEYWORDS
        .iter()
        .any(|keyword| lower.contains(keyword));
    let has_artifact = BUILDER_ARTIFACT_KEYWORDS
        .iter()
        .any(|keyword| lower.contains(keyword));

    has_action && has_artifact
}

fn filter_local_builder_fallback_tools(tools: &[ToolDefinition]) -> Vec<ToolDefinition> {
    tools
        .iter()
        .filter(|tool| LOCAL_BUILDER_FALLBACK_TOOLS.contains(&tool.name.as_str()))
        .cloned()
        .collect()
}

fn should_retry_with_required_builder_tools(
    provider_name: &str,
    conversation: &Conversation,
    tools: &[ToolDefinition],
    response_content: &[ContentBlockResponse],
    stop_reason: Option<StopReason>,
) -> bool {
    if provider_name != "local" || tools.is_empty() {
        return false;
    }

    if stop_reason == Some(StopReason::ToolUse) {
        return false;
    }

    if !extract_tool_uses(response_content).is_empty() {
        return false;
    }

    is_builder_intent(conversation)
}

/// Batch output from a tool execution strategy.
#[derive(Debug, Default)]
pub struct ToolExecutionBatch {
    pub results: Vec<ToolResult>,
    pub cancelled_tool_use_ids: Vec<String>,
}

/// Outcome of shared tool-use execution.
#[derive(Debug, Default)]
pub struct ToolExecutionOutcome {
    pub results: Vec<ToolResult>,
    pub executed_calls: Vec<ToolUse>,
    pub cancelled_tool_use_ids: Vec<String>,
    pub loop_detected: bool,
}

/// Output hooks for the reusable agent loop.
///
/// Frontends can implement this trait to render text, status, and tool output.
pub trait AgentLoopObserver {
    fn on_response_prefix(&mut self, _active_caps: &[String]) -> Result<()> {
        Ok(())
    }

    fn on_text_delta(&mut self, _text: &str) -> Result<()> {
        Ok(())
    }

    /// Called for each incoming stream event before processing.
    fn on_stream_event_tick(&mut self) -> Result<()> {
        Ok(())
    }

    fn on_rate_limited(
        &mut self,
        _delay_secs: u64,
        _attempt: u32,
        _max_retries: u32,
    ) -> Result<()> {
        Ok(())
    }

    fn on_context_too_long(&mut self, _current: u32, _limit: u32) -> Result<()> {
        Ok(())
    }

    fn on_context_trimmed(&mut self, _removed: usize) -> Result<()> {
        Ok(())
    }

    fn on_tool_phase_start(&mut self) -> Result<()> {
        Ok(())
    }

    fn on_tool_invocation(&mut self, _tool_name: &str, _input: &serde_json::Value) -> Result<()> {
        Ok(())
    }

    fn on_tool_result(&mut self, _tool_name: &str, _result: &ToolResult) -> Result<()> {
        Ok(())
    }

    fn on_loop_detected(&mut self, _tool_name: &str, _count: usize) -> Result<()> {
        Ok(())
    }

    fn on_loop_recovery(&mut self) -> Result<()> {
        Ok(())
    }

    fn on_agent_complete(&mut self) -> Result<()> {
        Ok(())
    }
}

/// No-op observer for callers that don't need output hooks.
#[derive(Debug, Default)]
pub struct NoopAgentLoopObserver;

impl AgentLoopObserver for NoopAgentLoopObserver {}

/// Strategy for executing a set of tool calls.
#[async_trait(?Send)]
pub trait ToolExecutionStrategy {
    async fn execute_tool_calls(
        &mut self,
        tool_executor: &mut ToolExecutor,
        calls: &[ToolUse],
        interrupted: &Arc<AtomicBool>,
    ) -> Result<ToolExecutionBatch>;
}

/// Default sequential tool execution strategy.
#[derive(Debug, Default)]
pub struct SequentialToolExecutionStrategy;

#[async_trait(?Send)]
impl ToolExecutionStrategy for SequentialToolExecutionStrategy {
    async fn execute_tool_calls(
        &mut self,
        tool_executor: &mut ToolExecutor,
        calls: &[ToolUse],
        _interrupted: &Arc<AtomicBool>,
    ) -> Result<ToolExecutionBatch> {
        let mut results = Vec::with_capacity(calls.len());
        for (id, name, input) in calls {
            results.push(
                tool_executor
                    .execute_tool_use(id, name, input.clone())
                    .await?,
            );
        }
        Ok(ToolExecutionBatch {
            results,
            cancelled_tool_use_ids: Vec::new(),
        })
    }
}

/// Execute tool uses with shared loop detection and pluggable execution strategy.
#[allow(clippy::too_many_arguments)]
pub async fn execute_tool_uses_with_strategy(
    tool_uses: &[ToolUse],
    tool_executor: &mut ToolExecutor,
    interrupted: &Arc<AtomicBool>,
    tool_call_tracker: &mut ToolCallTracker,
    observer: &mut dyn AgentLoopObserver,
    strategy: &mut dyn ToolExecutionStrategy,
) -> Result<ToolExecutionOutcome> {
    let mut loop_detected = false;
    let mut executable_calls: Vec<ToolUse> = Vec::new();
    let mut precomputed_results: std::collections::HashMap<String, ToolResult> =
        std::collections::HashMap::new();

    tracing::debug!(
        target: "ted.chat.engine",
        requested_tool_calls = tool_uses.len(),
        "starting tool execution batch"
    );

    for (id, name, input) in tool_uses {
        if let Some(loop_info) =
            tool_call_tracker.check_loop(name, input, MAX_CONSECUTIVE_IDENTICAL_CALLS)
        {
            loop_detected = true;
            tracing::warn!(
                target: "ted.chat.engine",
                tool_name = %name,
                consecutive_count = loop_info.consecutive_count,
                "detected repeated tool loop; injecting recovery error result"
            );
            observer.on_loop_detected(name, loop_info.consecutive_count)?;
            precomputed_results.insert(
                id.clone(),
                ToolResult::error(
                    id.clone(),
                    format_loop_error(name, loop_info.consecutive_count),
                ),
            );
            tool_call_tracker.clear();
            continue;
        }

        tool_call_tracker.track(name, input);
        observer.on_tool_invocation(name, input)?;
        executable_calls.push((id.clone(), name.clone(), input.clone()));
    }

    let batch = strategy
        .execute_tool_calls(tool_executor, &executable_calls, interrupted)
        .await?;

    tracing::debug!(
        target: "ted.chat.engine",
        executable_tool_calls = executable_calls.len(),
        strategy_results = batch.results.len(),
        cancelled_tool_calls = batch.cancelled_tool_use_ids.len(),
        "tool execution strategy completed"
    );

    let mut results_by_id: std::collections::HashMap<String, ToolResult> = batch
        .results
        .into_iter()
        .map(|r| (r.tool_use_id.clone(), r))
        .collect();
    let cancelled_ids: std::collections::HashSet<String> =
        batch.cancelled_tool_use_ids.iter().cloned().collect();

    for (id, name, _) in &executable_calls {
        if let Some(result) = results_by_id.get(id) {
            observer.on_tool_result(name, result)?;
        }
    }

    let mut ordered_results: Vec<ToolResult> = Vec::with_capacity(tool_uses.len());
    for (id, _, _) in tool_uses {
        if let Some(result) = precomputed_results.remove(id) {
            ordered_results.push(result);
            continue;
        }
        if let Some(result) = results_by_id.remove(id) {
            ordered_results.push(result);
            continue;
        }
        if cancelled_ids.contains(id) {
            ordered_results.push(ToolResult::error(id.clone(), "Cancelled by user"));
        }
    }

    // Preserve any strategy results that did not map to a known request id.
    ordered_results.extend(results_by_id.into_values());

    if loop_detected {
        observer.on_loop_recovery()?;
    }

    tracing::debug!(
        target: "ted.chat.engine",
        total_results = ordered_results.len(),
        loop_detected,
        "tool batch normalized"
    );

    Ok(ToolExecutionOutcome {
        results: ordered_results,
        executed_calls: executable_calls,
        cancelled_tool_use_ids: batch.cancelled_tool_use_ids,
        loop_detected,
    })
}

/// Run the agent loop and restore conversation state on error/interruption.
#[allow(clippy::too_many_arguments)]
pub async fn run_agent_loop(
    provider: &dyn LlmProvider,
    model: &str,
    conversation: &mut Conversation,
    tool_executor: &mut ToolExecutor,
    settings: &Settings,
    context_manager: &ContextManager,
    stream: bool,
    active_caps: &[String],
    interrupted: Arc<AtomicBool>,
    observer: &mut dyn AgentLoopObserver,
) -> Result<bool> {
    let initial_message_count = conversation.messages.len();

    tracing::info!(
        target: "ted.chat.engine",
        model = %model,
        stream,
        starting_messages = initial_message_count,
        active_caps = active_caps.len(),
        "agent loop start"
    );

    let result = run_agent_loop_inner(
        provider,
        model,
        conversation,
        tool_executor,
        settings,
        context_manager,
        stream,
        active_caps,
        interrupted,
        observer,
    )
    .await;

    match &result {
        Err(_) | Ok(false) => {
            conversation.messages.truncate(initial_message_count);
            tracing::info!(
                target: "ted.chat.engine",
                final_messages = conversation.messages.len(),
                "agent loop rolled back conversation state"
            );
        }
        Ok(true) => {}
    }

    match &result {
        Ok(true) => tracing::info!(
            target: "ted.chat.engine",
            final_messages = conversation.messages.len(),
            "agent loop complete"
        ),
        Ok(false) => tracing::info!(target: "ted.chat.engine", "agent loop interrupted"),
        Err(error) => tracing::warn!(
            target: "ted.chat.engine",
            error = %error,
            "agent loop failed"
        ),
    }

    result
}

/// Inner implementation of the loop without conversation rollback.
#[allow(clippy::too_many_arguments)]
pub async fn run_agent_loop_inner(
    provider: &dyn LlmProvider,
    model: &str,
    conversation: &mut Conversation,
    tool_executor: &mut ToolExecutor,
    settings: &Settings,
    context_manager: &ContextManager,
    stream: bool,
    active_caps: &[String],
    interrupted: Arc<AtomicBool>,
    observer: &mut dyn AgentLoopObserver,
) -> Result<bool> {
    let mut tool_call_tracker = ToolCallTracker::new(MAX_RECENT_TOOL_CALLS);
    let mut turn_index: usize = 0;

    loop {
        turn_index += 1;

        if interrupted.load(Ordering::SeqCst) {
            tracing::debug!(
                target: "ted.chat.engine",
                turn = turn_index,
                "interruption requested before turn execution"
            );
            return Ok(false);
        }

        tracing::debug!(
            target: "ted.chat.engine",
            turn = turn_index,
            conversation_messages = conversation.messages.len(),
            "starting model turn"
        );

        let (response_content, stop_reason) = get_response_with_context_retry(
            provider,
            model,
            conversation,
            settings.defaults.max_tokens,
            settings.defaults.temperature,
            tool_executor.tool_definitions(),
            stream,
            active_caps,
            observer,
        )
        .await?;

        tracing::debug!(
            target: "ted.chat.engine",
            turn = turn_index,
            response_blocks = response_content.len(),
            ?stop_reason,
            "received model response"
        );

        let text_content = extract_text_content(&response_content);
        if !text_content.is_empty() {
            context_manager
                .store_message("assistant", &text_content, None)
                .await?;
        }

        let tool_uses = extract_tool_uses(&response_content);
        let assistant_blocks = response_to_message_blocks(&response_content);

        conversation.push(Message {
            id: uuid::Uuid::new_v4(),
            role: crate::llm::message::Role::Assistant,
            content: MessageContent::Blocks(assistant_blocks),
            timestamp: chrono::Utc::now(),
            tool_use_id: None,
            token_count: None,
        });

        if !tool_uses.is_empty() {
            tracing::info!(
                target: "ted.chat.engine",
                turn = turn_index,
                tool_calls = tool_uses.len(),
                "entering tool execution phase"
            );
            observer.on_tool_phase_start()?;

            let mut strategy = SequentialToolExecutionStrategy;
            let outcome = execute_tool_uses_with_strategy(
                &tool_uses,
                tool_executor,
                &interrupted,
                &mut tool_call_tracker,
                observer,
                &mut strategy,
            )
            .await?;

            tracing::info!(
                target: "ted.chat.engine",
                turn = turn_index,
                tool_results = outcome.results.len(),
                cancelled_tool_calls = outcome.cancelled_tool_use_ids.len(),
                loop_detected = outcome.loop_detected,
                "tool execution phase complete"
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
                        .store_tool_call(name, input, result.output_text(), result.is_error(), None)
                        .await?;
                    tokio::time::sleep(Duration::from_millis(TOOL_EXECUTION_DELAY_MS)).await;
                }
            }

            let result_blocks: Vec<ContentBlock> = outcome
                .results
                .into_iter()
                .map(|r| {
                    let output = r.output_text().to_string();
                    let is_error = r.is_error();
                    tool_result_block(r.tool_use_id, output, is_error)
                })
                .collect();

            conversation.push(Message::user_blocks(result_blocks));

            tokio::time::sleep(Duration::from_millis(POST_TOOL_LOOP_DELAY_MS)).await;
            continue;
        }

        if stop_reason != Some(StopReason::ToolUse) {
            observer.on_agent_complete()?;
            tracing::debug!(
                target: "ted.chat.engine",
                turn = turn_index,
                ?stop_reason,
                "turn completed without further tool calls"
            );
            break;
        }
    }

    Ok(true)
}

/// Request a completion and handle one automatic context-trim retry.
#[allow(clippy::too_many_arguments)]
pub async fn get_response_with_context_retry(
    provider: &dyn LlmProvider,
    model: &str,
    conversation: &mut Conversation,
    max_tokens: u32,
    temperature: f32,
    tools: Vec<ToolDefinition>,
    stream: bool,
    active_caps: &[String],
    observer: &mut dyn AgentLoopObserver,
) -> Result<(Vec<ContentBlockResponse>, Option<StopReason>)> {
    let build_request = |conversation: &Conversation,
                         request_tools: Vec<ToolDefinition>,
                         tool_choice: ToolChoice,
                         extra_system_hint: Option<&str>| {
        let mut request = CompletionRequest::new(model, conversation.messages.clone())
            .with_max_tokens(max_tokens)
            .with_temperature(temperature)
            .with_tools(request_tools)
            .with_tool_choice(tool_choice);

        let mut effective_system = conversation.system_prompt.clone().unwrap_or_default();
        if let Some(hint) = extra_system_hint {
            if !effective_system.is_empty() {
                effective_system.push_str("\n\n");
            }
            effective_system.push_str(hint);
        }

        if !effective_system.is_empty() {
            request = request.with_system(&effective_system);
        }

        request
    };

    let local_builder_mode =
        provider.name() == "local" && !tools.is_empty() && is_builder_intent(conversation);

    let initial_tools = if local_builder_mode {
        let filtered = filter_local_builder_fallback_tools(&tools);
        if filtered.is_empty() {
            tools.clone()
        } else {
            filtered
        }
    } else {
        tools.clone()
    };

    // Local builder flows should avoid streaming speculative prose responses from
    // earlier fallback attempts; request a full response and then decide on retries.
    let effective_stream = if local_builder_mode { false } else { stream };

    let request = if local_builder_mode {
        build_request(
            conversation,
            initial_tools.clone(),
            ToolChoice::Required,
            Some(LOCAL_BUILDER_REQUIRED_HINT),
        )
    } else {
        build_request(conversation, tools.clone(), ToolChoice::Auto, None)
    };

    match get_response_with_retry(provider, request, effective_stream, active_caps, observer).await
    {
        Ok((response_content, stop_reason)) => {
            if should_retry_with_required_builder_tools(
                provider.name(),
                conversation,
                &initial_tools,
                &response_content,
                stop_reason,
            ) {
                let fallback_tools = filter_local_builder_fallback_tools(&tools);
                if !fallback_tools.is_empty() {
                    tracing::info!(
                        target: "ted.chat.engine",
                        tool_count = fallback_tools.len(),
                        "local builder response had no tool calls; retrying once with required tool choice"
                    );
                    let retry_request = build_request(
                        conversation,
                        fallback_tools.clone(),
                        ToolChoice::Required,
                        Some(LOCAL_BUILDER_REQUIRED_HINT),
                    );
                    let retry_result = get_response_with_retry(
                        provider,
                        retry_request,
                        effective_stream,
                        active_caps,
                        observer,
                    )
                    .await?;

                    if should_retry_with_required_builder_tools(
                        provider.name(),
                        conversation,
                        &fallback_tools,
                        &retry_result.0,
                        retry_result.1,
                    ) {
                        tracing::info!(
                            target: "ted.chat.engine",
                            tool_count = fallback_tools.len(),
                            "local builder required retry still had no tool calls; applying strict tool-only retry"
                        );
                        let strict_request = build_request(
                            conversation,
                            fallback_tools,
                            ToolChoice::Required,
                            Some(LOCAL_BUILDER_STRICT_HINT),
                        );
                        return get_response_with_retry(
                            provider,
                            strict_request,
                            effective_stream,
                            active_caps,
                            observer,
                        )
                        .await;
                    }

                    return Ok(retry_result);
                }
            }

            Ok((response_content, stop_reason))
        }
        Err(TedError::Api(ApiError::ContextTooLong { current, limit })) => {
            tracing::warn!(
                target: "ted.chat.engine",
                current_tokens = current,
                context_limit = limit,
                "context limit exceeded; attempting trim-and-retry"
            );
            observer.on_context_too_long(current, limit)?;

            let context_window = provider
                .get_model_info(model)
                .map(|m| m.context_window)
                .unwrap_or(limit);
            let target_tokens = calculate_trim_target(context_window);
            let removed = conversation.trim_to_fit(target_tokens);
            observer.on_context_trimmed(removed)?;
            tracing::info!(
                target: "ted.chat.engine",
                removed_messages = removed,
                target_tokens,
                "applied context trim"
            );

            if removed == 0 {
                return Err(TedError::Api(ApiError::ContextTooLong { current, limit }));
            }

            let retry_request = if local_builder_mode {
                build_request(
                    conversation,
                    initial_tools.clone(),
                    ToolChoice::Required,
                    Some(LOCAL_BUILDER_REQUIRED_HINT),
                )
            } else {
                build_request(conversation, tools.clone(), ToolChoice::Auto, None)
            };
            get_response_with_retry(
                provider,
                retry_request,
                effective_stream,
                active_caps,
                observer,
            )
            .await
        }
        Err(e) => Err(e),
    }
}

/// Request a completion with retry behavior for rate limits.
pub async fn get_response_with_retry(
    provider: &dyn LlmProvider,
    request: CompletionRequest,
    stream: bool,
    active_caps: &[String],
    observer: &mut dyn AgentLoopObserver,
) -> Result<(Vec<ContentBlockResponse>, Option<StopReason>)> {
    let mut attempt = 0;

    loop {
        attempt += 1;
        tracing::debug!(
            target: "ted.chat.engine",
            model = %request.model,
            attempt,
            stream,
            message_count = request.messages.len(),
            tool_count = request.tools.len(),
            "requesting model completion"
        );

        let result = if stream {
            stream_response(provider, request.clone(), active_caps, observer).await
        } else {
            observer.on_response_prefix(active_caps)?;
            provider
                .complete(request.clone())
                .await
                .map(|r| (r.content, r.stop_reason))
        };

        match result {
            Ok(response) => return Ok(response),
            Err(TedError::Api(ApiError::RateLimited(retry_after))) => {
                if attempt > MAX_RETRIES {
                    return Err(TedError::Api(ApiError::RateLimited(retry_after)));
                }

                let delay_secs = if retry_after > 0 {
                    retry_after as u64
                } else {
                    BASE_RETRY_DELAY.pow(attempt)
                };

                tracing::warn!(
                    target: "ted.chat.engine",
                    model = %request.model,
                    attempt,
                    max_retries = MAX_RETRIES,
                    retry_after_secs = delay_secs,
                    "rate limited; retrying request"
                );

                observer.on_rate_limited(delay_secs, attempt, MAX_RETRIES)?;
                tokio::time::sleep(Duration::from_secs(delay_secs)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Stream a completion while emitting observer callbacks for displayed text.
pub async fn stream_response(
    provider: &dyn LlmProvider,
    request: CompletionRequest,
    active_caps: &[String],
    observer: &mut dyn AgentLoopObserver,
) -> Result<(Vec<ContentBlockResponse>, Option<StopReason>)> {
    let mut stream = provider.complete_stream(request).await?;
    let mut accumulator = StreamAccumulator::new();
    let mut prefix_printed = false;

    while let Some(event) = stream.next().await {
        observer.on_stream_event_tick()?;
        let event = event?;
        let processed = accumulator.process_event(event);

        match processed {
            StreamEventResult::TextDelta(text) => {
                if !prefix_printed {
                    observer.on_response_prefix(active_caps)?;
                    prefix_printed = true;
                }
                observer.on_text_delta(&text)?;
            }
            StreamEventResult::Error {
                error_type,
                message,
            } => {
                return Err(TedError::Api(ApiError::ServerError {
                    status: 0,
                    message: format!("{}: {}", error_type, message),
                }));
            }
            StreamEventResult::MessageStop => break,
            _ => {}
        }
    }

    Ok(accumulator.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::context::{ContextManager, SessionId};
    use crate::llm::message::{MessageContent, Role};
    use crate::llm::provider::{
        CompletionResponse, ContentBlockDelta, ModelInfo, StreamEvent, Usage,
    };
    use futures::stream;
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex;
    use tempfile::TempDir;

    type StreamResultQueue = Arc<Mutex<VecDeque<Result<Vec<Result<StreamEvent>>>>>>;

    struct TestObserver {
        response_prefix_count: usize,
        text_deltas: String,
        stream_tick_count: usize,
        rate_limit_events: Vec<(u64, u32, u32)>,
        context_too_long_events: Vec<(u32, u32)>,
        context_trimmed_events: Vec<usize>,
        loop_detected_count: usize,
        loop_recovery_count: usize,
        tool_invocation_count: usize,
        tool_result_count: usize,
        agent_complete_count: usize,
    }

    impl TestObserver {
        fn new() -> Self {
            Self {
                response_prefix_count: 0,
                text_deltas: String::new(),
                stream_tick_count: 0,
                rate_limit_events: Vec::new(),
                context_too_long_events: Vec::new(),
                context_trimmed_events: Vec::new(),
                loop_detected_count: 0,
                loop_recovery_count: 0,
                tool_invocation_count: 0,
                tool_result_count: 0,
                agent_complete_count: 0,
            }
        }
    }

    impl AgentLoopObserver for TestObserver {
        fn on_response_prefix(&mut self, _active_caps: &[String]) -> Result<()> {
            self.response_prefix_count += 1;
            Ok(())
        }

        fn on_text_delta(&mut self, text: &str) -> Result<()> {
            self.text_deltas.push_str(text);
            Ok(())
        }

        fn on_stream_event_tick(&mut self) -> Result<()> {
            self.stream_tick_count += 1;
            Ok(())
        }

        fn on_rate_limited(
            &mut self,
            delay_secs: u64,
            attempt: u32,
            max_retries: u32,
        ) -> Result<()> {
            self.rate_limit_events
                .push((delay_secs, attempt, max_retries));
            Ok(())
        }

        fn on_context_too_long(&mut self, current: u32, limit: u32) -> Result<()> {
            self.context_too_long_events.push((current, limit));
            Ok(())
        }

        fn on_context_trimmed(&mut self, removed: usize) -> Result<()> {
            self.context_trimmed_events.push(removed);
            Ok(())
        }

        fn on_tool_invocation(
            &mut self,
            _tool_name: &str,
            _input: &serde_json::Value,
        ) -> Result<()> {
            self.tool_invocation_count += 1;
            Ok(())
        }

        fn on_tool_result(&mut self, _tool_name: &str, _result: &ToolResult) -> Result<()> {
            self.tool_result_count += 1;
            Ok(())
        }

        fn on_loop_detected(&mut self, _tool_name: &str, _count: usize) -> Result<()> {
            self.loop_detected_count += 1;
            Ok(())
        }

        fn on_loop_recovery(&mut self) -> Result<()> {
            self.loop_recovery_count += 1;
            Ok(())
        }

        fn on_agent_complete(&mut self) -> Result<()> {
            self.agent_complete_count += 1;
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockStrategy {
        batch: ToolExecutionBatch,
    }

    #[async_trait(?Send)]
    impl ToolExecutionStrategy for MockStrategy {
        async fn execute_tool_calls(
            &mut self,
            _tool_executor: &mut ToolExecutor,
            _calls: &[ToolUse],
            _interrupted: &Arc<AtomicBool>,
        ) -> Result<ToolExecutionBatch> {
            Ok(std::mem::take(&mut self.batch))
        }
    }

    fn make_tool_executor() -> ToolExecutor {
        let working_dir = std::env::current_dir().unwrap();
        let ctx = crate::tools::ToolContext::new(working_dir, None, uuid::Uuid::new_v4(), true);
        ToolExecutor::new(ctx, true)
    }

    fn make_tool_executor_at(workdir: &std::path::Path) -> ToolExecutor {
        let ctx = crate::tools::ToolContext::new(
            workdir.to_path_buf(),
            Some(workdir.to_path_buf()),
            uuid::Uuid::new_v4(),
            true,
        );
        ToolExecutor::new(ctx, true)
    }

    async fn make_context_manager() -> (TempDir, ContextManager) {
        let temp = TempDir::new().unwrap();
        let manager = ContextManager::new(temp.path().to_path_buf(), SessionId::new())
            .await
            .unwrap();
        (temp, manager)
    }

    #[derive(Clone)]
    struct SequenceProvider {
        provider_name: String,
        models: Vec<ModelInfo>,
        complete_results: Arc<Mutex<VecDeque<Result<CompletionResponse>>>>,
        stream_results: StreamResultQueue,
        complete_calls: Arc<AtomicUsize>,
        stream_calls: Arc<AtomicUsize>,
        complete_requests: Arc<Mutex<Vec<CompletionRequest>>>,
    }

    impl SequenceProvider {
        fn new(models: Vec<ModelInfo>) -> Self {
            Self {
                provider_name: "sequence-provider".to_string(),
                models,
                complete_results: Arc::new(Mutex::new(VecDeque::new())),
                stream_results: Arc::new(Mutex::new(VecDeque::new())),
                complete_calls: Arc::new(AtomicUsize::new(0)),
                stream_calls: Arc::new(AtomicUsize::new(0)),
                complete_requests: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn named(name: &str, models: Vec<ModelInfo>) -> Self {
            let mut provider = Self::new(models);
            provider.provider_name = name.to_string();
            provider
        }

        fn push_complete_result(&self, result: Result<CompletionResponse>) {
            self.complete_results.lock().unwrap().push_back(result);
        }

        fn push_stream_result(&self, result: Result<Vec<Result<StreamEvent>>>) {
            self.stream_results.lock().unwrap().push_back(result);
        }

        fn complete_call_count(&self) -> usize {
            self.complete_calls.load(Ordering::SeqCst)
        }

        fn stream_call_count(&self) -> usize {
            self.stream_calls.load(Ordering::SeqCst)
        }

        fn complete_requests(&self) -> Vec<CompletionRequest> {
            self.complete_requests.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl LlmProvider for SequenceProvider {
        fn name(&self) -> &str {
            &self.provider_name
        }

        fn available_models(&self) -> Vec<ModelInfo> {
            self.models.clone()
        }

        fn supports_model(&self, model: &str) -> bool {
            self.models.iter().any(|m| m.id == model)
        }

        async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
            self.complete_calls.fetch_add(1, Ordering::SeqCst);
            self.complete_requests.lock().unwrap().push(request);
            self.complete_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err(TedError::Internal("missing complete result".to_string())))
        }

        async fn complete_stream(
            &self,
            _request: CompletionRequest,
        ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamEvent>> + Send>>> {
            self.stream_calls.fetch_add(1, Ordering::SeqCst);
            let stream_result = self
                .stream_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err(TedError::Internal("missing stream result".to_string())));

            match stream_result {
                Ok(events) => Ok(Box::pin(stream::iter(events))),
                Err(e) => Err(e),
            }
        }

        fn count_tokens(&self, text: &str, _model: &str) -> Result<u32> {
            Ok((text.len() / 4).max(1) as u32)
        }
    }

    fn test_model_info(id: &str, context_window: u32) -> ModelInfo {
        ModelInfo {
            id: id.to_string(),
            display_name: id.to_string(),
            context_window,
            max_output_tokens: 8_192,
            supports_tools: true,
            supports_vision: false,
            input_cost_per_1k: 0.0,
            output_cost_per_1k: 0.0,
        }
    }

    fn completion_response(
        model: &str,
        content: Vec<ContentBlockResponse>,
        stop_reason: StopReason,
    ) -> CompletionResponse {
        CompletionResponse {
            id: "resp_1".to_string(),
            model: model.to_string(),
            content,
            stop_reason: Some(stop_reason),
            usage: Usage::default(),
        }
    }

    #[tokio::test]
    async fn test_execute_tool_uses_with_strategy_orders_cancelled_results() {
        let mut tool_executor = make_tool_executor();
        let interrupted = Arc::new(AtomicBool::new(false));
        let mut tracker = ToolCallTracker::new(MAX_RECENT_TOOL_CALLS);
        let mut observer = TestObserver::new();
        let tool_uses = vec![
            (
                "tool_a".to_string(),
                "shell".to_string(),
                serde_json::json!({"command":"echo a"}),
            ),
            (
                "tool_b".to_string(),
                "shell".to_string(),
                serde_json::json!({"command":"echo b"}),
            ),
        ];

        let mut strategy = MockStrategy {
            batch: ToolExecutionBatch {
                results: vec![ToolResult::success("tool_b", "ok")],
                cancelled_tool_use_ids: vec!["tool_a".to_string()],
            },
        };

        let outcome = execute_tool_uses_with_strategy(
            &tool_uses,
            &mut tool_executor,
            &interrupted,
            &mut tracker,
            &mut observer,
            &mut strategy,
        )
        .await
        .unwrap();

        assert_eq!(outcome.results.len(), 2);
        assert_eq!(outcome.results[0].tool_use_id, "tool_a");
        assert!(outcome.results[0].is_error());
        assert_eq!(outcome.results[1].tool_use_id, "tool_b");
        assert!(!outcome.loop_detected);
        assert_eq!(observer.tool_invocation_count, 2);
        assert_eq!(observer.tool_result_count, 1);
    }

    #[tokio::test]
    async fn test_execute_tool_uses_with_strategy_detects_loops() {
        let mut tool_executor = make_tool_executor();
        let interrupted = Arc::new(AtomicBool::new(false));
        let mut tracker = ToolCallTracker::new(MAX_RECENT_TOOL_CALLS);
        let mut observer = TestObserver::new();
        let tool_uses = vec![
            (
                "tool_1".to_string(),
                "shell".to_string(),
                serde_json::json!({"command":"echo x"}),
            ),
            (
                "tool_2".to_string(),
                "shell".to_string(),
                serde_json::json!({"command":"echo x"}),
            ),
            (
                "tool_3".to_string(),
                "shell".to_string(),
                serde_json::json!({"command":"echo x"}),
            ),
        ];

        let mut strategy = MockStrategy {
            batch: ToolExecutionBatch {
                results: vec![
                    ToolResult::success("tool_1", "ok"),
                    ToolResult::success("tool_2", "ok"),
                ],
                cancelled_tool_use_ids: vec![],
            },
        };

        let outcome = execute_tool_uses_with_strategy(
            &tool_uses,
            &mut tool_executor,
            &interrupted,
            &mut tracker,
            &mut observer,
            &mut strategy,
        )
        .await
        .unwrap();

        assert_eq!(outcome.results.len(), 3);
        assert_eq!(outcome.results[2].tool_use_id, "tool_3");
        assert!(outcome.results[2].is_error());
        assert!(outcome.loop_detected);
        assert_eq!(observer.loop_detected_count, 1);
        assert_eq!(observer.loop_recovery_count, 1);
    }

    #[tokio::test]
    async fn test_get_response_with_retry_non_stream_success() {
        let provider = SequenceProvider::new(vec![test_model_info("test-model", 8_000)]);
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::Text {
                text: "Hello from model".to_string(),
            }],
            StopReason::EndTurn,
        )));

        let request = CompletionRequest::new("test-model", vec![Message::user("Hi")]);
        let mut observer = TestObserver::new();
        let active_caps = vec!["base".to_string()];
        let (content, stop_reason) =
            get_response_with_retry(&provider, request, false, &active_caps, &mut observer)
                .await
                .unwrap();

        assert_eq!(provider.complete_call_count(), 1);
        assert_eq!(observer.response_prefix_count, 1);
        assert_eq!(stop_reason, Some(StopReason::EndTurn));
        assert!(matches!(
            &content[0],
            ContentBlockResponse::Text { text } if text == "Hello from model"
        ));
    }

    #[tokio::test]
    async fn test_get_response_with_retry_retries_rate_limit_then_succeeds() {
        let provider = SequenceProvider::new(vec![test_model_info("test-model", 8_000)]);
        provider.push_complete_result(Err(TedError::Api(ApiError::RateLimited(1))));
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::Text {
                text: "Recovered".to_string(),
            }],
            StopReason::EndTurn,
        )));

        let request = CompletionRequest::new("test-model", vec![Message::user("Hi")]);
        let mut observer = TestObserver::new();
        let active_caps = vec!["base".to_string()];
        let (content, stop_reason) =
            get_response_with_retry(&provider, request, false, &active_caps, &mut observer)
                .await
                .unwrap();

        assert_eq!(provider.complete_call_count(), 2);
        assert_eq!(observer.rate_limit_events, vec![(1, 1, MAX_RETRIES)]);
        assert_eq!(stop_reason, Some(StopReason::EndTurn));
        assert!(matches!(
            &content[0],
            ContentBlockResponse::Text { text } if text == "Recovered"
        ));
    }

    #[tokio::test]
    async fn test_get_response_with_context_retry_trims_and_retries() {
        let provider = SequenceProvider::new(vec![test_model_info("test-model", 2_000)]);
        provider.push_complete_result(Err(TedError::Api(ApiError::ContextTooLong {
            current: 9_000,
            limit: 2_000,
        })));
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::Text {
                text: "After trim".to_string(),
            }],
            StopReason::EndTurn,
        )));

        let mut conversation = Conversation::new();
        for i in 0..10 {
            let text = format!("message-{} {}", i, "x".repeat(700));
            let role = if i % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            };
            conversation.push(Message {
                id: uuid::Uuid::new_v4(),
                role,
                content: MessageContent::Text(text),
                timestamp: chrono::Utc::now(),
                tool_use_id: None,
                token_count: None,
            });
        }
        let original_len = conversation.messages.len();

        let mut observer = TestObserver::new();
        let active_caps = vec!["base".to_string()];
        let (content, stop_reason) = get_response_with_context_retry(
            &provider,
            "test-model",
            &mut conversation,
            4_096,
            0.7,
            Vec::new(),
            false,
            &active_caps,
            &mut observer,
        )
        .await
        .unwrap();

        assert_eq!(provider.complete_call_count(), 2);
        assert_eq!(observer.context_too_long_events, vec![(9_000, 2_000)]);
        assert_eq!(observer.context_trimmed_events.len(), 1);
        assert!(observer.context_trimmed_events[0] > 0);
        assert!(conversation.messages.len() < original_len);
        assert_eq!(stop_reason, Some(StopReason::EndTurn));
        assert!(matches!(
            &content[0],
            ContentBlockResponse::Text { text } if text == "After trim"
        ));
    }

    fn test_tool_definition(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: format!("{} description", name),
            input_schema: crate::llm::provider::ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: vec![],
            },
        }
    }

    #[tokio::test]
    async fn test_local_builder_retries_with_required_tool_choice() {
        let provider = SequenceProvider::named("local", vec![test_model_info("test-model", 8_000)]);
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::Text {
                text: "I'll explain how to do it".to_string(),
            }],
            StopReason::EndTurn,
        )));
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::ToolUse {
                id: "tool_1".to_string(),
                name: "file_write".to_string(),
                input: serde_json::json!({
                    "path": "index.html",
                    "content": "<!doctype html><html></html>"
                }),
            }],
            StopReason::ToolUse,
        )));

        let mut conversation = Conversation::new();
        conversation.push(Message::user("Please build a simple website"));
        let tools = vec![test_tool_definition("file_write")];

        let mut observer = TestObserver::new();
        let active_caps = vec!["base".to_string()];
        let (content, stop_reason) = get_response_with_context_retry(
            &provider,
            "test-model",
            &mut conversation,
            4_096,
            0.7,
            tools,
            false,
            &active_caps,
            &mut observer,
        )
        .await
        .unwrap();

        assert_eq!(provider.complete_call_count(), 2);
        assert_eq!(stop_reason, Some(StopReason::ToolUse));
        assert!(matches!(
            &content[0],
            ContentBlockResponse::ToolUse { name, .. } if name == "file_write"
        ));

        let requests = provider.complete_requests();
        assert_eq!(requests.len(), 2);
        assert!(matches!(requests[0].tool_choice, ToolChoice::Required));
        assert!(matches!(requests[1].tool_choice, ToolChoice::Required));
        assert!(requests[0]
            .system
            .as_deref()
            .unwrap_or_default()
            .contains("tool calls only"));
        assert!(requests[1]
            .system
            .as_deref()
            .unwrap_or_default()
            .contains("tool calls only"));
    }

    #[tokio::test]
    async fn test_local_builder_applies_strict_retry_after_required_retry() {
        let provider = SequenceProvider::named("local", vec![test_model_info("test-model", 8_000)]);
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::Text {
                text: "I'll explain how to do it".to_string(),
            }],
            StopReason::EndTurn,
        )));
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::Text {
                text: "Still explaining".to_string(),
            }],
            StopReason::EndTurn,
        )));
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::ToolUse {
                id: "tool_1".to_string(),
                name: "file_write".to_string(),
                input: serde_json::json!({
                    "path": "index.html",
                    "content": "<!doctype html><html></html>"
                }),
            }],
            StopReason::ToolUse,
        )));

        let mut conversation = Conversation::new();
        conversation.push(Message::user("Create a new app website"));
        let tools = vec![test_tool_definition("file_write")];

        let mut observer = TestObserver::new();
        let active_caps = vec!["base".to_string()];
        let (content, stop_reason) = get_response_with_context_retry(
            &provider,
            "test-model",
            &mut conversation,
            4_096,
            0.7,
            tools,
            false,
            &active_caps,
            &mut observer,
        )
        .await
        .unwrap();

        assert_eq!(provider.complete_call_count(), 3);
        assert_eq!(stop_reason, Some(StopReason::ToolUse));
        assert!(matches!(
            &content[0],
            ContentBlockResponse::ToolUse { name, .. } if name == "file_write"
        ));

        let requests = provider.complete_requests();
        assert_eq!(requests.len(), 3);
        assert!(matches!(requests[0].tool_choice, ToolChoice::Required));
        assert!(matches!(requests[1].tool_choice, ToolChoice::Required));
        assert!(matches!(requests[2].tool_choice, ToolChoice::Required));
        assert!(requests[0]
            .system
            .as_deref()
            .unwrap_or_default()
            .contains("tool calls only"));
        assert!(requests[2]
            .system
            .as_deref()
            .unwrap_or_default()
            .contains("MANDATORY"));
    }

    #[tokio::test]
    async fn test_local_builder_mode_disables_streaming() {
        let provider = SequenceProvider::named("local", vec![test_model_info("test-model", 8_000)]);
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::ToolUse {
                id: "tool_1".to_string(),
                name: "file_write".to_string(),
                input: serde_json::json!({
                    "path": "index.html",
                    "content": "<!doctype html><html></html>"
                }),
            }],
            StopReason::ToolUse,
        )));

        let mut conversation = Conversation::new();
        conversation.push(Message::user("Build a simple website"));
        let tools = vec![test_tool_definition("file_write")];

        let mut observer = TestObserver::new();
        let active_caps = vec!["base".to_string()];
        let (_content, stop_reason) = get_response_with_context_retry(
            &provider,
            "test-model",
            &mut conversation,
            4_096,
            0.7,
            tools,
            true,
            &active_caps,
            &mut observer,
        )
        .await
        .unwrap();

        assert_eq!(stop_reason, Some(StopReason::ToolUse));
        assert_eq!(provider.complete_call_count(), 1);
        assert_eq!(provider.stream_call_count(), 0);
    }

    #[tokio::test]
    async fn test_get_response_with_context_retry_returns_error_when_nothing_trimmed() {
        let provider = SequenceProvider::new(vec![test_model_info("test-model", 2_000)]);
        provider.push_complete_result(Err(TedError::Api(ApiError::ContextTooLong {
            current: 5_000,
            limit: 2_000,
        })));

        let mut conversation = Conversation::new();
        let mut observer = TestObserver::new();
        let active_caps = vec!["base".to_string()];
        let error = get_response_with_context_retry(
            &provider,
            "test-model",
            &mut conversation,
            4_096,
            0.7,
            Vec::new(),
            false,
            &active_caps,
            &mut observer,
        )
        .await
        .unwrap_err();

        match error {
            TedError::Api(ApiError::ContextTooLong { current, limit }) => {
                assert_eq!((current, limit), (5_000, 2_000));
            }
            other => panic!("Expected ContextTooLong, got {other:?}"),
        }
        assert_eq!(provider.complete_call_count(), 1);
        assert_eq!(observer.context_too_long_events, vec![(5_000, 2_000)]);
        assert_eq!(observer.context_trimmed_events, vec![0]);
    }

    #[tokio::test]
    async fn test_stream_response_accumulates_text_and_prefix_once() {
        let provider = SequenceProvider::new(vec![test_model_info("test-model", 8_000)]);
        provider.push_stream_result(Ok(vec![
            Ok(StreamEvent::MessageStart {
                id: "m1".to_string(),
                model: "test-model".to_string(),
            }),
            Ok(StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlockResponse::Text {
                    text: String::new(),
                },
            }),
            Ok(StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::TextDelta {
                    text: "Hello ".to_string(),
                },
            }),
            Ok(StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::TextDelta {
                    text: "world".to_string(),
                },
            }),
            Ok(StreamEvent::ContentBlockStop { index: 0 }),
            Ok(StreamEvent::MessageDelta {
                stop_reason: Some(StopReason::EndTurn),
                usage: None,
            }),
            Ok(StreamEvent::MessageStop),
        ]));

        let request = CompletionRequest::new("test-model", vec![Message::user("Hi")]);
        let mut observer = TestObserver::new();
        let active_caps = vec!["base".to_string()];
        let (content, stop_reason) =
            stream_response(&provider, request, &active_caps, &mut observer)
                .await
                .unwrap();

        assert_eq!(provider.stream_call_count(), 1);
        assert_eq!(observer.response_prefix_count, 1);
        assert_eq!(observer.text_deltas, "Hello world");
        assert!(observer.stream_tick_count >= 1);
        assert_eq!(stop_reason, Some(StopReason::EndTurn));
        assert!(matches!(
            &content[0],
            ContentBlockResponse::Text { text } if text == "Hello world"
        ));
    }

    #[tokio::test]
    async fn test_stream_response_converts_stream_error_event() {
        let provider = SequenceProvider::new(vec![test_model_info("test-model", 8_000)]);
        provider.push_stream_result(Ok(vec![
            Ok(StreamEvent::MessageStart {
                id: "m1".to_string(),
                model: "test-model".to_string(),
            }),
            Ok(StreamEvent::Error {
                error_type: "overloaded_error".to_string(),
                message: "try again later".to_string(),
            }),
        ]));

        let request = CompletionRequest::new("test-model", vec![Message::user("Hi")]);
        let mut observer = TestObserver::new();
        let active_caps = vec!["base".to_string()];
        let error = stream_response(&provider, request, &active_caps, &mut observer)
            .await
            .unwrap_err();

        match error {
            TedError::Api(ApiError::ServerError { status, message }) => {
                assert_eq!(status, 0);
                assert!(message.contains("overloaded_error"));
                assert!(message.contains("try again later"));
            }
            other => panic!("Expected ServerError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_sequential_tool_execution_strategy_executes_calls() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("note.txt");
        std::fs::write(&file_path, "hello strategy").unwrap();

        let mut tool_executor = make_tool_executor_at(temp.path());
        let interrupted = Arc::new(AtomicBool::new(false));
        let calls = vec![(
            "tool_1".to_string(),
            "file_read".to_string(),
            serde_json::json!({"path": file_path.to_string_lossy().to_string()}),
        )];
        let mut strategy = SequentialToolExecutionStrategy;

        let batch = strategy
            .execute_tool_calls(&mut tool_executor, &calls, &interrupted)
            .await
            .unwrap();

        assert_eq!(batch.results.len(), 1);
        assert!(!batch.results[0].is_error());
        assert!(batch.cancelled_tool_use_ids.is_empty());
    }

    #[tokio::test]
    async fn test_execute_tool_uses_with_strategy_preserves_unmapped_results() {
        let mut tool_executor = make_tool_executor();
        let interrupted = Arc::new(AtomicBool::new(false));
        let mut tracker = ToolCallTracker::new(MAX_RECENT_TOOL_CALLS);
        let mut observer = TestObserver::new();
        let tool_uses = vec![(
            "known_id".to_string(),
            "shell".to_string(),
            serde_json::json!({"command":"echo ok"}),
        )];

        let mut strategy = MockStrategy {
            batch: ToolExecutionBatch {
                results: vec![ToolResult::success("extra_id", "extra result")],
                cancelled_tool_use_ids: vec![],
            },
        };

        let outcome = execute_tool_uses_with_strategy(
            &tool_uses,
            &mut tool_executor,
            &interrupted,
            &mut tracker,
            &mut observer,
            &mut strategy,
        )
        .await
        .unwrap();

        assert_eq!(outcome.results.len(), 1);
        assert_eq!(outcome.results[0].tool_use_id, "extra_id");
    }

    #[tokio::test]
    async fn test_run_agent_loop_non_tool_completion() {
        let provider = SequenceProvider::new(vec![test_model_info("test-model", 8_000)]);
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::Text {
                text: "Assistant completed".to_string(),
            }],
            StopReason::EndTurn,
        )));

        let (_temp, context_manager) = make_context_manager().await;
        let settings = Settings::default();
        let mut tool_executor = make_tool_executor();
        let mut conversation = Conversation::new();
        conversation.push(Message::user("hi"));
        let mut observer = TestObserver::new();
        let interrupted = Arc::new(AtomicBool::new(false));

        let completed = run_agent_loop(
            &provider,
            "test-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            false,
            &["base".to_string()],
            interrupted,
            &mut observer,
        )
        .await
        .unwrap();

        assert!(completed);
        assert_eq!(observer.agent_complete_count, 1);
        assert_eq!(conversation.messages.len(), 2);
        let chunks = context_manager.get_all_chunks().await.unwrap();
        assert!(!chunks.is_empty());
    }

    #[tokio::test]
    async fn test_run_agent_loop_with_tool_use_then_text_completion() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("context.txt");
        std::fs::write(&file_path, "tool branch").unwrap();

        let provider = SequenceProvider::new(vec![test_model_info("test-model", 8_000)]);
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::ToolUse {
                id: "tool_1".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({"path": file_path.to_string_lossy().to_string()}),
            }],
            StopReason::ToolUse,
        )));
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::Text {
                text: "All done".to_string(),
            }],
            StopReason::EndTurn,
        )));

        let (_ctx_temp, context_manager) = make_context_manager().await;
        let settings = Settings::default();
        let mut tool_executor = make_tool_executor_at(temp.path());
        let mut conversation = Conversation::new();
        conversation.push(Message::user("read file"));
        let mut observer = TestObserver::new();
        let interrupted = Arc::new(AtomicBool::new(false));

        let completed = run_agent_loop(
            &provider,
            "test-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            false,
            &[],
            interrupted,
            &mut observer,
        )
        .await
        .unwrap();

        assert!(completed);
        assert!(observer.tool_invocation_count >= 1);
        assert!(observer.tool_result_count >= 1);
        let has_tool_result = conversation.messages.iter().any(|msg| {
            matches!(
                &msg.content,
                MessageContent::Blocks(blocks)
                    if blocks.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }))
            )
        });
        assert!(has_tool_result);
    }

    #[tokio::test]
    async fn test_run_agent_loop_interrupted_rolls_back_conversation() {
        let provider = SequenceProvider::new(vec![test_model_info("test-model", 8_000)]);
        let (_temp, context_manager) = make_context_manager().await;
        let settings = Settings::default();
        let mut tool_executor = make_tool_executor();
        let mut conversation = Conversation::new();
        conversation.push(Message::user("hi"));
        let initial_len = conversation.messages.len();
        let mut observer = TestObserver::new();
        let interrupted = Arc::new(AtomicBool::new(true));

        let completed = run_agent_loop(
            &provider,
            "test-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            false,
            &[],
            interrupted,
            &mut observer,
        )
        .await
        .unwrap();

        assert!(!completed);
        assert_eq!(conversation.messages.len(), initial_len);
    }

    #[tokio::test]
    async fn test_run_agent_loop_error_rolls_back_conversation() {
        let provider = SequenceProvider::new(vec![test_model_info("test-model", 8_000)]);
        provider.push_complete_result(Err(TedError::Api(ApiError::InvalidResponse(
            "bad payload".to_string(),
        ))));

        let (_temp, context_manager) = make_context_manager().await;
        let settings = Settings::default();
        let mut tool_executor = make_tool_executor();
        let mut conversation = Conversation::new();
        conversation.push(Message::user("hi"));
        let initial_len = conversation.messages.len();
        let mut observer = TestObserver::new();
        let interrupted = Arc::new(AtomicBool::new(false));

        let result = run_agent_loop(
            &provider,
            "test-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            false,
            &[],
            interrupted,
            &mut observer,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(conversation.messages.len(), initial_len);
    }

    #[tokio::test]
    async fn test_get_response_with_retry_gives_up_after_max_retries() {
        let provider = SequenceProvider::new(vec![test_model_info("test-model", 8_000)]);
        provider.push_complete_result(Err(TedError::Api(ApiError::RateLimited(1))));
        provider.push_complete_result(Err(TedError::Api(ApiError::RateLimited(1))));
        provider.push_complete_result(Err(TedError::Api(ApiError::RateLimited(1))));
        provider.push_complete_result(Err(TedError::Api(ApiError::RateLimited(1))));

        let request = CompletionRequest::new("test-model", vec![Message::user("Hi")]);
        let mut observer = TestObserver::new();
        let error = get_response_with_retry(&provider, request, false, &[], &mut observer)
            .await
            .unwrap_err();

        match error {
            TedError::Api(ApiError::RateLimited(retry_after)) => {
                assert_eq!(retry_after, 1);
            }
            other => panic!("Expected RateLimited error, got {other:?}"),
        }
        assert_eq!(provider.complete_call_count(), 4);
        assert_eq!(observer.rate_limit_events.len(), 3);
    }

    #[tokio::test]
    async fn test_get_response_with_context_retry_passthrough_non_context_error() {
        let provider = SequenceProvider::new(vec![test_model_info("test-model", 8_000)]);
        provider.push_complete_result(Err(TedError::Api(ApiError::InvalidResponse(
            "bad response".to_string(),
        ))));

        let mut conversation = Conversation::new();
        conversation.push(Message::user("hello"));
        let mut observer = TestObserver::new();
        let error = get_response_with_context_retry(
            &provider,
            "test-model",
            &mut conversation,
            4096,
            0.7,
            Vec::new(),
            false,
            &[],
            &mut observer,
        )
        .await
        .unwrap_err();

        match error {
            TedError::Api(ApiError::InvalidResponse(msg)) => {
                assert!(msg.contains("bad response"));
            }
            other => panic!("Expected InvalidResponse error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_run_agent_loop_streaming_completion_path() {
        let provider = SequenceProvider::new(vec![test_model_info("test-model", 8_000)]);
        provider.push_stream_result(Ok(vec![
            Ok(StreamEvent::MessageStart {
                id: "m1".to_string(),
                model: "test-model".to_string(),
            }),
            Ok(StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlockResponse::Text {
                    text: String::new(),
                },
            }),
            Ok(StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::TextDelta {
                    text: "streamed output".to_string(),
                },
            }),
            Ok(StreamEvent::ContentBlockStop { index: 0 }),
            Ok(StreamEvent::MessageDelta {
                stop_reason: Some(StopReason::EndTurn),
                usage: None,
            }),
            Ok(StreamEvent::MessageStop),
        ]));

        let (_temp, context_manager) = make_context_manager().await;
        let settings = Settings::default();
        let mut tool_executor = make_tool_executor();
        let mut conversation = Conversation::new();
        conversation.push(Message::user("stream this"));
        let mut observer = TestObserver::new();
        let interrupted = Arc::new(AtomicBool::new(false));

        let completed = run_agent_loop(
            &provider,
            "test-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            true,
            &["base".to_string()],
            interrupted,
            &mut observer,
        )
        .await
        .unwrap();

        assert!(completed);
        assert_eq!(provider.stream_call_count(), 1);
        assert!(observer.text_deltas.contains("streamed output"));
        assert_eq!(observer.agent_complete_count, 1);
    }

    #[tokio::test]
    async fn test_run_agent_loop_retries_when_stop_reason_tool_use_has_no_tools() {
        let provider = SequenceProvider::new(vec![test_model_info("test-model", 8_000)]);
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::Text {
                text: "thinking".to_string(),
            }],
            StopReason::ToolUse,
        )));
        provider.push_complete_result(Ok(completion_response(
            "test-model",
            vec![ContentBlockResponse::Text {
                text: "final".to_string(),
            }],
            StopReason::EndTurn,
        )));

        let (_temp, context_manager) = make_context_manager().await;
        let settings = Settings::default();
        let mut tool_executor = make_tool_executor();
        let mut conversation = Conversation::new();
        conversation.push(Message::user("continue"));
        let mut observer = TestObserver::new();
        let interrupted = Arc::new(AtomicBool::new(false));

        let completed = run_agent_loop(
            &provider,
            "test-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            false,
            &[],
            interrupted,
            &mut observer,
        )
        .await
        .unwrap();

        assert!(completed);
        assert_eq!(provider.complete_call_count(), 2);
        assert_eq!(observer.agent_complete_count, 1);
    }
}
