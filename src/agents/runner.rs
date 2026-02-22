// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Agent execution runner
//!
//! This module implements the execution loop for subagents, handling:
//! - LLM API calls
//! - Tool execution with permission filtering
//! - Memory strategy application
//! - Result generation

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use crate::error::{ApiError, Result, TedError};
use crate::llm::message::{ContentBlock, Message, MessageContent};
use crate::llm::provider::{
    CompletionRequest, ContentBlockResponse, LlmProvider, StopReason, ToolChoice, ToolDefinition,
};
use crate::tools::{ToolContext, ToolOutput, ToolRegistry, ToolResult};

use super::context::AgentContext;
use super::memory::{apply_memory_strategy, compact_to_budget, MemoryAction};
use super::types::AgentResult;

/// Progress event emitted by an agent during execution
#[derive(Debug, Clone)]
pub enum AgentProgressEvent {
    /// Agent started execution
    Started {
        agent_name: String,
        agent_type: String,
        max_iterations: u32,
    },
    /// Starting a new iteration
    IterationStart { iteration: u32, max_iterations: u32 },
    /// About to call a tool
    ToolStart {
        tool_name: String,
        input_summary: String,
    },
    /// Tool completed
    ToolComplete { tool_name: String, success: bool },
    /// Waiting due to rate limit
    RateLimited { wait_secs: f64 },
    /// Agent finished (successfully or not)
    Completed {
        success: bool,
        iterations: u32,
        summary: String,
    },
    /// Agent's LLM response text (for conversation mirroring in TUI)
    AssistantMessage { text: String },
    /// Agent started a tool call with full details (for conversation mirroring)
    ToolCallStarted {
        tool_id: String,
        tool_name: String,
        input: serde_json::Value,
    },
    /// Agent completed a tool call with full results (for conversation mirroring)
    ToolCallCompleted {
        tool_id: String,
        tool_name: String,
        success: bool,
        output_preview: Option<String>,
        output_full: Option<String>,
    },
}

impl AgentProgressEvent {
    /// Get a short status string for display
    pub fn status_text(&self) -> String {
        match self {
            AgentProgressEvent::Started { agent_type, .. } => {
                format!("Starting {} agent...", agent_type)
            }
            AgentProgressEvent::IterationStart {
                iteration,
                max_iterations,
            } => {
                format!("Iteration {}/{}", iteration, max_iterations)
            }
            AgentProgressEvent::ToolStart {
                tool_name,
                input_summary,
            } => {
                let summary = if input_summary.len() > 40 {
                    format!("{}...", &input_summary[..40])
                } else {
                    input_summary.clone()
                };
                format!("→ {} {}", tool_name, summary)
            }
            AgentProgressEvent::ToolComplete { tool_name, success } => {
                let status = if *success { "✓" } else { "✗" };
                format!("{} {}", status, tool_name)
            }
            AgentProgressEvent::RateLimited { wait_secs } => {
                format!("Rate limited ({:.1}s)", wait_secs)
            }
            AgentProgressEvent::Completed {
                success,
                iterations,
                summary,
            } => {
                if *success {
                    format!("Done ({} iters): {}", iterations, summary)
                } else {
                    format!("Failed after {} iters", iterations)
                }
            }
            AgentProgressEvent::AssistantMessage { text } => {
                let preview: String = text.chars().take(60).collect();
                if text.len() > 60 {
                    format!("Responded: {}...", preview)
                } else {
                    format!("Responded: {}", preview)
                }
            }
            AgentProgressEvent::ToolCallStarted { tool_name, .. } => {
                format!("Calling {}...", tool_name)
            }
            AgentProgressEvent::ToolCallCompleted {
                tool_name, success, ..
            } => {
                let status = if *success { "done" } else { "failed" };
                format!("{} {}", tool_name, status)
            }
        }
    }
}

/// Type alias for progress sender
pub type ProgressSender = tokio::sync::mpsc::UnboundedSender<AgentProgressEvent>;

/// Configuration for the agent runner
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// Maximum tokens for LLM response
    pub max_response_tokens: u32,
    /// Temperature for LLM sampling
    pub temperature: f32,
    /// Whether to print progress
    pub verbose: bool,
    /// Suppress ALL output (for TUI mode where prints break the display)
    pub quiet: bool,
    /// Maximum retries for rate-limited requests
    pub max_rate_limit_retries: u32,
    /// Base delay for exponential backoff (when no Retry-After header)
    pub base_retry_delay_secs: u64,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            max_response_tokens: 4096,
            temperature: 0.7,
            verbose: false,
            quiet: false,
            max_rate_limit_retries: 3,
            base_retry_delay_secs: 2,
        }
    }
}

/// Agent runner that executes subagents
pub struct AgentRunner {
    /// LLM provider for making API calls
    provider: Arc<dyn LlmProvider>,
    /// Tool registry with available tools
    tool_registry: ToolRegistry,
    /// Runner configuration
    config: RunnerConfig,
}

impl AgentRunner {
    /// Create a new agent runner
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            provider,
            tool_registry: ToolRegistry::with_builtins(),
            config: RunnerConfig::default(),
        }
    }

    /// Create a runner with custom configuration
    pub fn with_config(provider: Arc<dyn LlmProvider>, config: RunnerConfig) -> Self {
        Self {
            provider,
            tool_registry: ToolRegistry::with_builtins(),
            config,
        }
    }

    /// Run a subagent to completion
    pub async fn run(&self, context: AgentContext) -> Result<AgentResult> {
        self.run_with_progress(context, None).await
    }

    /// Run a subagent to completion with optional progress reporting
    pub async fn run_with_progress(
        &self,
        mut context: AgentContext,
        progress: Option<ProgressSender>,
    ) -> Result<AgentResult> {
        let started_at = Utc::now();
        let agent_id = context.config.id;
        let agent_name = context.config.name.clone();
        let agent_type = context.config.agent_type.clone();
        let model = context
            .config
            .model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());

        // Helper to send progress events
        let send_progress = |event: AgentProgressEvent| {
            if let Some(ref tx) = progress {
                let _ = tx.send(event);
            }
        };

        // Emit started event
        send_progress(AgentProgressEvent::Started {
            agent_name: agent_name.clone(),
            agent_type: agent_type.clone(),
            max_iterations: context.config.max_iterations,
        });

        // Log agent start for visibility (unless in quiet mode for TUI)
        if !self.config.quiet {
            eprintln!(
                "  [{}] Starting ({}, max {} iters)",
                agent_name, context.config.agent_type, context.config.max_iterations
            );

            if self.config.verbose {
                eprintln!("  [{}] Task: {}", agent_name, context.config.task);
                eprintln!("  [{}] Model: {}", agent_name, model);
            }
        }

        // Get filtered tool definitions based on agent permissions
        let tool_definitions = self.get_filtered_tools(&context);

        // Create tool context for execution
        let tool_context = ToolContext::new(
            context.config.working_dir.clone(),
            Some(context.config.working_dir.clone()),
            agent_id,
            true, // Subagents run in trust mode within their permission scope
        );

        let mut errors: Vec<String> = Vec::new();
        let mut last_output = String::new();

        // Main agent loop
        loop {
            context.increment_iteration();

            // Emit iteration start event
            send_progress(AgentProgressEvent::IterationStart {
                iteration: context.iterations(),
                max_iterations: context.config.max_iterations,
            });

            if self.config.verbose && !self.config.quiet {
                eprintln!(
                    "  [{}] Iteration {}/{}",
                    agent_name,
                    context.iterations(),
                    context.config.max_iterations
                );
            }

            // Check limits
            if context.exceeded_iterations() {
                if !self.config.quiet {
                    eprintln!(
                        "  [{}] Exceeded max iterations ({})",
                        agent_name, context.config.max_iterations
                    );
                }
                errors.push(format!(
                    "Exceeded maximum iterations ({})",
                    context.config.max_iterations
                ));
                break;
            }

            if context.exceeded_token_budget() {
                if !self.config.quiet {
                    eprintln!(
                        "  [{}] Exceeded token budget ({} tokens)",
                        agent_name, context.config.token_budget
                    );
                }
                errors.push(format!(
                    "Exceeded token budget ({} tokens)",
                    context.config.token_budget
                ));
                break;
            }

            // Apply memory strategy
            let memory_strategy = context.config.memory_strategy.clone();
            match apply_memory_strategy(context.conversation_mut(), &memory_strategy)? {
                MemoryAction::Trimmed { count } => {
                    if self.config.verbose && !self.config.quiet {
                        eprintln!("  [{}] Memory: Trimmed {} old messages", agent_name, count);
                    }
                }
                MemoryAction::NeedsSummarization { messages } => {
                    // For now, we'll just note this - full summarization would
                    // require another LLM call which we might want to add later
                    if self.config.verbose && !self.config.quiet {
                        eprintln!(
                            "  [{}] Memory: {} messages need summarization (skipping)",
                            agent_name,
                            messages.len()
                        );
                    }
                }
                MemoryAction::None => {}
            }

            // Compact if still over budget
            let current_tokens = context.conversation().estimate_tokens();
            let token_budget = context.config.token_budget;
            if current_tokens > token_budget {
                let removed = compact_to_budget(
                    context.conversation_mut(),
                    token_budget * 80 / 100, // Target 80% of budget
                );
                if self.config.verbose && !self.config.quiet && removed > 0 {
                    eprintln!(
                        "  [{}] Memory: Compacted {} messages to fit budget",
                        agent_name, removed
                    );
                }
            }

            // Build completion request
            let request = CompletionRequest {
                model: model.clone(),
                messages: context.conversation().messages.clone(),
                system: context.conversation().system_prompt.clone(),
                max_tokens: self.config.max_response_tokens,
                temperature: self.config.temperature,
                tools: tool_definitions.clone(),
                tool_choice: ToolChoice::Auto,
            };

            // Check rate budget before making request (proactive rate limiting)
            if let Some(allocation) = context.rate_allocation() {
                // Estimate tokens for this request (rough estimate based on conversation size)
                let estimated_tokens = self.estimate_request_tokens(&request);
                let wait_time = allocation.wait_for_budget(estimated_tokens).await;
                if wait_time > Duration::from_millis(100) && !self.config.quiet {
                    eprintln!(
                        "  [{}] Rate budget: waited {:.1}s for {} tokens",
                        agent_name,
                        wait_time.as_secs_f64(),
                        estimated_tokens
                    );
                }
            }

            // Make LLM call with rate limit retry logic
            let current_iter = context.iterations();
            let response = match self
                .complete_with_retry(
                    &request,
                    &agent_name,
                    current_iter,
                    self.config.verbose && !self.config.quiet,
                )
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    if !self.config.quiet {
                        eprintln!(
                            "  [{}] Failed at iteration {}: {}",
                            agent_name, current_iter, e
                        );
                    }
                    errors.push(format!("LLM API error: {}", e));
                    break;
                }
            };

            // Track token usage and record in rate budget allocation
            let tokens_this_turn = response.usage.input_tokens + response.usage.output_tokens;
            if let Some(allocation) = context.rate_allocation() {
                allocation.record_usage(tokens_this_turn as u64);
            }

            // Process response content
            let mut has_tool_use = false;
            let mut text_response = String::new();

            for block in &response.content {
                match block {
                    ContentBlockResponse::Text { text } => {
                        text_response.push_str(text);
                    }
                    ContentBlockResponse::ToolUse { .. } => {
                        has_tool_use = true;
                    }
                }
            }

            last_output = text_response.clone();

            // Emit assistant message for conversation mirroring
            if !text_response.is_empty() {
                send_progress(AgentProgressEvent::AssistantMessage {
                    text: text_response.clone(),
                });
            }

            // Create assistant message from response
            let assistant_msg = self.response_to_message(&response.content);
            context.add_message(assistant_msg).await?;

            // If there are tool uses, execute them
            if has_tool_use {
                let tool_results = self
                    .execute_tools(&response.content, &context, &tool_context, &progress)
                    .await?;

                // Track file access
                for result in &tool_results {
                    self.track_file_access(result, &mut context);
                }

                // Add tool results to conversation
                let tool_result_msg = self.tool_results_to_message(&tool_results);
                context.add_message(tool_result_msg).await?;
            }

            // Check stop reason
            match response.stop_reason {
                Some(StopReason::EndTurn) if !has_tool_use => {
                    // Agent is done
                    if self.config.verbose && !self.config.quiet {
                        eprintln!("  [{}] End turn (completing)", agent_name);
                    }
                    break;
                }
                Some(StopReason::MaxTokens) => {
                    // Continue, the agent might have more to say
                    if self.config.verbose && !self.config.quiet {
                        eprintln!("  [{}] Hit response token limit, continuing...", agent_name);
                    }
                }
                Some(StopReason::ToolUse) => {
                    // Continue after tool execution
                    if self.config.verbose && !self.config.quiet {
                        eprintln!("  [{}] Tool use completed, continuing...", agent_name);
                    }
                }
                _ => {
                    // Continue
                }
            }
        }

        // Build result
        let success = errors.is_empty();
        let summary = if success {
            self.generate_summary(&last_output)
        } else {
            format!("Agent failed: {}", errors.join("; "))
        };

        // Emit completed event
        send_progress(AgentProgressEvent::Completed {
            success,
            iterations: context.iterations(),
            summary: summary.clone(),
        });

        // Log completion status (unless in quiet mode)
        let final_iter = context.iterations();
        if !self.config.quiet {
            if success {
                eprintln!(
                    "  [{}] Completed successfully ({} iters, {} tokens)",
                    agent_name,
                    final_iter,
                    context.tokens_used()
                );
            } else {
                eprintln!(
                    "  [{}] Failed after {} iters: {}",
                    agent_name,
                    final_iter,
                    errors.join("; ")
                );
            }
        }

        // Finalize context (store completion marker)
        context.finalize(success, &summary).await?;

        let result = if success {
            AgentResult::success(agent_id, agent_name, last_output, summary, started_at)
                .with_files_changed(context.files_changed().to_vec())
                .with_files_read(context.files_read().to_vec())
                .with_iterations(context.iterations())
                .with_tokens_used(context.tokens_used())
        } else {
            AgentResult::failure(agent_id, agent_name, errors, started_at)
                .with_files_read(context.files_read().to_vec())
                .with_iterations(context.iterations())
                .with_tokens_used(context.tokens_used())
        };

        // Add bead ID if tracking
        let result = if let Some(bead_id) = context.config.bead_id.clone() {
            result.with_bead_id(bead_id)
        } else {
            result
        };

        Ok(result)
    }

    /// Get tool definitions filtered by agent permissions
    fn get_filtered_tools(&self, context: &AgentContext) -> Vec<ToolDefinition> {
        self.tool_registry
            .definitions()
            .into_iter()
            .filter(|def| context.is_tool_allowed(&def.name))
            .collect()
    }

    /// Make an LLM completion request with retry logic for rate limits
    ///
    /// This method handles rate limit errors by waiting and retrying, using either
    /// the Retry-After value from the API or exponential backoff.
    ///
    /// # Arguments
    /// * `request` - The completion request to send
    /// * `agent_name` - Name of the agent (for logging)
    /// * `iteration` - Current iteration number (for logging context)
    /// * `verbose` - Whether to print verbose output
    async fn complete_with_retry(
        &self,
        request: &CompletionRequest,
        agent_name: &str,
        iteration: u32,
        verbose: bool,
    ) -> Result<crate::llm::provider::CompletionResponse> {
        let mut attempt = 0;

        loop {
            attempt += 1;

            if verbose && attempt == 1 {
                eprintln!("  [{}] Making LLM request (iter {})", agent_name, iteration);
            }

            match self.provider.complete(request.clone()).await {
                Ok(response) => {
                    if attempt > 1 && !self.config.quiet {
                        eprintln!(
                            "  [{}] Rate limit resolved after {} retries (iter {})",
                            agent_name,
                            attempt - 1,
                            iteration
                        );
                    }
                    return Ok(response);
                }
                Err(TedError::Api(ApiError::RateLimited(retry_after))) => {
                    if attempt > self.config.max_rate_limit_retries {
                        if !self.config.quiet {
                            eprintln!(
                                "  [{}] Rate limit: exhausted all {} retries (iter {})",
                                agent_name, self.config.max_rate_limit_retries, iteration
                            );
                        }
                        return Err(TedError::Api(ApiError::RateLimited(retry_after)));
                    }

                    // Use retry_after from API if available, otherwise use exponential backoff
                    let delay_secs = if retry_after > 0 {
                        retry_after as u64
                    } else {
                        self.config.base_retry_delay_secs.pow(attempt)
                    };

                    // Provide more context about the rate limit
                    if !self.config.quiet {
                        let source_hint = if retry_after > 0 {
                            format!("API requested {}s wait", retry_after)
                        } else {
                            "using backoff".to_string()
                        };

                        eprintln!(
                            "  [{}] Rate limited (iter {}, retry {}/{}) - {} - waiting {}s",
                            agent_name,
                            iteration,
                            attempt,
                            self.config.max_rate_limit_retries,
                            source_hint,
                            delay_secs
                        );
                    }

                    tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                }
                Err(e) => {
                    // Non-rate-limit errors are not retried
                    if verbose {
                        eprintln!("  [{}] LLM error (iter {}): {}", agent_name, iteration, e);
                    }
                    return Err(e);
                }
            }
        }
    }

    /// Convert response content blocks to a Message
    fn response_to_message(&self, content: &[ContentBlockResponse]) -> Message {
        let blocks: Vec<ContentBlock> = content
            .iter()
            .map(|block| match block {
                ContentBlockResponse::Text { text } => ContentBlock::Text { text: text.clone() },
                ContentBlockResponse::ToolUse { id, name, input } => ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                },
            })
            .collect();

        Message::assistant_blocks(blocks)
    }

    /// Execute tool uses and return results
    async fn execute_tools(
        &self,
        content: &[ContentBlockResponse],
        agent_context: &AgentContext,
        tool_context: &ToolContext,
        progress: &Option<ProgressSender>,
    ) -> Result<Vec<ToolResult>> {
        let mut results = Vec::new();

        // Helper to send progress events
        let send_progress = |event: AgentProgressEvent| {
            if let Some(ref tx) = progress {
                let _ = tx.send(event);
            }
        };

        for block in content {
            if let ContentBlockResponse::ToolUse { id, name, input } = block {
                // Check if tool is allowed
                if !agent_context.is_tool_allowed(name) {
                    send_progress(AgentProgressEvent::ToolComplete {
                        tool_name: name.clone(),
                        success: false,
                    });
                    results.push(ToolResult {
                        tool_use_id: id.clone(),
                        output: ToolOutput::Error(format!(
                            "Tool '{}' is not allowed for this agent type",
                            name
                        )),
                    });
                    continue;
                }

                // Get the tool
                let tool = match self.tool_registry.get(name) {
                    Some(t) => t.clone(),
                    None => {
                        send_progress(AgentProgressEvent::ToolComplete {
                            tool_name: name.clone(),
                            success: false,
                        });
                        results.push(ToolResult {
                            tool_use_id: id.clone(),
                            output: ToolOutput::Error(format!("Unknown tool: {}", name)),
                        });
                        continue;
                    }
                };

                // Create input summary for progress reporting
                let input_summary = summarize_tool_input(name, input);

                // Emit tool start events
                send_progress(AgentProgressEvent::ToolStart {
                    tool_name: name.clone(),
                    input_summary,
                });
                send_progress(AgentProgressEvent::ToolCallStarted {
                    tool_id: id.clone(),
                    tool_name: name.clone(),
                    input: input.clone(),
                });

                // Execute the tool
                if self.config.verbose && !self.config.quiet {
                    eprintln!("  [{}] → Tool: {}", agent_context.config.name, name);
                }

                match tool.execute(id.clone(), input.clone(), tool_context).await {
                    Ok(result) => {
                        let success = !result.is_error();
                        send_progress(AgentProgressEvent::ToolComplete {
                            tool_name: name.clone(),
                            success,
                        });
                        // Emit rich tool completion for conversation mirroring
                        let output_text = result.output_text().to_string();
                        let output_preview = if output_text.chars().count() > 100 {
                            Some(output_text.chars().take(97).collect::<String>() + "...")
                        } else {
                            Some(output_text.clone())
                        };
                        send_progress(AgentProgressEvent::ToolCallCompleted {
                            tool_id: id.clone(),
                            tool_name: name.clone(),
                            success,
                            output_preview,
                            output_full: Some(output_text),
                        });
                        if self.config.verbose && !self.config.quiet {
                            if result.is_error() {
                                eprintln!(
                                    "  [{}]   ✗ Error: {}",
                                    agent_context.config.name,
                                    truncate_str(result.output_text(), 100)
                                );
                            } else {
                                eprintln!("  [{}]   ✓ Success", agent_context.config.name);
                            }
                        }
                        results.push(result);
                    }
                    Err(e) => {
                        send_progress(AgentProgressEvent::ToolComplete {
                            tool_name: name.clone(),
                            success: false,
                        });
                        send_progress(AgentProgressEvent::ToolCallCompleted {
                            tool_id: id.clone(),
                            tool_name: name.clone(),
                            success: false,
                            output_preview: Some(e.to_string()),
                            output_full: Some(e.to_string()),
                        });
                        results.push(ToolResult {
                            tool_use_id: id.clone(),
                            output: ToolOutput::Error(e.to_string()),
                        });
                    }
                }
            }
        }

        Ok(results)
    }

    /// Convert tool results to a user message for the conversation
    fn tool_results_to_message(&self, results: &[ToolResult]) -> Message {
        let blocks: Vec<ContentBlock> = results
            .iter()
            .map(|r| ContentBlock::ToolResult {
                tool_use_id: r.tool_use_id.clone(),
                content: crate::llm::message::ToolResultContent::Text(r.output_text().to_string()),
                is_error: if r.is_error() { Some(true) } else { None },
            })
            .collect();

        // Tool results go in a user message
        Message {
            id: Uuid::new_v4(),
            role: crate::llm::message::Role::User,
            content: MessageContent::Blocks(blocks),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        }
    }

    /// Track file access from tool results
    fn track_file_access(&self, _result: &ToolResult, _context: &mut AgentContext) {
        // This is a simplified version - in practice we'd inspect the tool inputs
        // to determine which files were accessed. For now, we rely on the tool
        // implementations to track this.

        // The tool result might contain file path information that we could parse
        // but this would require tool-specific logic.
    }

    /// Generate a summary from the agent's final output
    fn generate_summary(&self, output: &str) -> String {
        // Simple summary: take first ~200 chars or first paragraph
        let summary = output
            .split("\n\n")
            .next()
            .unwrap_or(output)
            .chars()
            .take(200)
            .collect::<String>();

        if summary.len() < output.len() {
            format!("{}...", summary.trim())
        } else {
            summary.trim().to_string()
        }
    }

    /// Estimate the number of tokens for a request
    ///
    /// This is a rough estimate used for proactive rate limiting.
    /// It counts characters and applies a multiplier, plus adds expected output tokens.
    fn estimate_request_tokens(&self, request: &CompletionRequest) -> u64 {
        // Estimate input tokens (roughly 4 chars per token for English text)
        let mut char_count: usize = 0;

        // System prompt
        if let Some(system) = &request.system {
            char_count += system.len();
        }

        // Messages
        for msg in &request.messages {
            char_count += msg.estimate_chars();
        }

        // Convert chars to tokens (approximately 4 chars per token)
        let input_tokens = (char_count / 4) as u64;

        // Add expected output tokens (use max_tokens as upper bound, but estimate ~50%)
        let expected_output = (request.max_tokens as u64) / 2;

        input_tokens + expected_output
    }
}

/// Truncate a string for display
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.replace('\n', " ")
    } else {
        format!("{}...", s[..max_len].replace('\n', " "))
    }
}

/// Summarize tool input for progress display
fn summarize_tool_input(tool_name: &str, input: &serde_json::Value) -> String {
    match tool_name {
        "file_read" | "glob" => {
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                return path.to_string();
            }
            if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                return pattern.to_string();
            }
        }
        "grep" => {
            if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                return format!("/{}/", pattern);
            }
        }
        "file_write" | "file_edit" => {
            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                return path.to_string();
            }
        }
        "shell" => {
            if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                let short = if cmd.len() > 50 {
                    format!("{}...", &cmd[..47])
                } else {
                    cmd.to_string()
                };
                return short;
            }
        }
        "spawn_agent" => {
            if let Some(agent_type) = input.get("agent_type").and_then(|v| v.as_str()) {
                if let Some(task) = input.get("task").and_then(|v| v.as_str()) {
                    let short_task = if task.len() > 40 {
                        format!("{}...", &task[..37])
                    } else {
                        task.to_string()
                    };
                    return format!("{}: {}", agent_type, short_task);
                }
                return agent_type.to_string();
            }
        }
        _ => {}
    }

    // Fallback: show first key-value pair
    if let Some(obj) = input.as_object() {
        if let Some((key, val)) = obj.iter().next() {
            let val_str = match val {
                serde_json::Value::String(s) => {
                    if s.len() > 40 {
                        format!("{}...", &s[..37])
                    } else {
                        s.clone()
                    }
                }
                _ => val.to_string(),
            };
            return format!("{}: {}", key, val_str);
        }
    }

    String::new()
}

/// Handle for a background agent
pub struct BackgroundAgentHandle {
    /// Agent ID
    pub id: Uuid,
    /// Agent name
    pub name: String,
    /// Task handle for the async execution
    handle: tokio::task::JoinHandle<Result<AgentResult>>,
}

impl BackgroundAgentHandle {
    /// Check if the agent is still running
    pub fn is_running(&self) -> bool {
        !self.handle.is_finished()
    }

    /// Wait for the agent to complete and get the result
    pub async fn wait(self) -> Result<AgentResult> {
        self.handle
            .await
            .map_err(|e| TedError::ToolExecution(format!("Agent task panicked: {}", e)))?
    }
}

/// Spawn an agent to run in the background
pub fn spawn_background_agent(
    runner: Arc<AgentRunner>,
    context: AgentContext,
) -> BackgroundAgentHandle {
    let id = context.config.id;
    let name = context.config.name.clone();

    let handle = tokio::spawn(async move { runner.run(context).await });

    BackgroundAgentHandle { id, name, handle }
}

#[cfg(test)]
mod tests;
