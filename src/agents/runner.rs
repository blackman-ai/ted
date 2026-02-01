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

use chrono::Utc;
use uuid::Uuid;

use crate::error::{Result, TedError};
use crate::llm::message::{ContentBlock, Message, MessageContent};
use crate::llm::provider::{
    CompletionRequest, ContentBlockResponse, LlmProvider, StopReason, ToolChoice, ToolDefinition,
};
use crate::tools::{ToolContext, ToolOutput, ToolRegistry, ToolResult};

use super::context::AgentContext;
use super::memory::{apply_memory_strategy, compact_to_budget, MemoryAction};
use super::types::AgentResult;

/// Configuration for the agent runner
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// Maximum tokens for LLM response
    pub max_response_tokens: u32,
    /// Temperature for LLM sampling
    pub temperature: f32,
    /// Whether to print progress
    pub verbose: bool,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            max_response_tokens: 4096,
            temperature: 0.7,
            verbose: false,
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
    pub async fn run(&self, mut context: AgentContext) -> Result<AgentResult> {
        let started_at = Utc::now();
        let agent_id = context.config.id;
        let agent_name = context.config.name.clone();
        let model = context
            .config
            .model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());

        if self.config.verbose {
            println!("=== Starting agent '{}' ===", agent_name);
            println!("  Type: {}", context.config.agent_type);
            println!("  Task: {}", context.config.task);
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

            if self.config.verbose {
                println!(
                    "\n--- Iteration {} / {} ---",
                    context.iterations(),
                    context.config.max_iterations
                );
            }

            // Check limits
            if context.exceeded_iterations() {
                errors.push(format!(
                    "Exceeded maximum iterations ({})",
                    context.config.max_iterations
                ));
                break;
            }

            if context.exceeded_token_budget() {
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
                    if self.config.verbose {
                        println!("  Memory: Trimmed {} old messages", count);
                    }
                }
                MemoryAction::NeedsSummarization { messages } => {
                    // For now, we'll just note this - full summarization would
                    // require another LLM call which we might want to add later
                    if self.config.verbose {
                        println!(
                            "  Memory: {} messages need summarization (skipping)",
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
                if self.config.verbose && removed > 0 {
                    println!("  Memory: Compacted {} messages to fit budget", removed);
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

            // Make LLM call
            let response = match self.provider.complete(request).await {
                Ok(resp) => resp,
                Err(e) => {
                    errors.push(format!("LLM API error: {}", e));
                    break;
                }
            };

            // Track token usage
            let _tokens_this_turn = response.usage.input_tokens + response.usage.output_tokens;

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

            // Create assistant message from response
            let assistant_msg = self.response_to_message(&response.content);
            context.add_message(assistant_msg).await?;

            // If there are tool uses, execute them
            if has_tool_use {
                let tool_results = self
                    .execute_tools(&response.content, &context, &tool_context)
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
                    if self.config.verbose {
                        println!("  Agent completed (end_turn)");
                    }
                    break;
                }
                Some(StopReason::MaxTokens) => {
                    // Continue, the agent might have more to say
                    if self.config.verbose {
                        println!("  Hit max tokens, continuing...");
                    }
                }
                Some(StopReason::ToolUse) => {
                    // Continue after tool execution
                    if self.config.verbose {
                        println!("  Tool use completed, continuing...");
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

        if self.config.verbose {
            println!("\n{}", result.format_for_parent());
        }

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
    ) -> Result<Vec<ToolResult>> {
        let mut results = Vec::new();

        for block in content {
            if let ContentBlockResponse::ToolUse { id, name, input } = block {
                // Check if tool is allowed
                if !agent_context.is_tool_allowed(name) {
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
                        results.push(ToolResult {
                            tool_use_id: id.clone(),
                            output: ToolOutput::Error(format!("Unknown tool: {}", name)),
                        });
                        continue;
                    }
                };

                // Execute the tool
                if self.config.verbose {
                    println!("  → Using tool: {}", name);
                }

                match tool.execute(id.clone(), input.clone(), tool_context).await {
                    Ok(result) => {
                        if self.config.verbose {
                            if result.is_error() {
                                println!(
                                    "    ✗ Error: {}",
                                    truncate_str(result.output_text(), 100)
                                );
                            } else {
                                println!("    ✓ Success");
                            }
                        }
                        results.push(result);
                    }
                    Err(e) => {
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
}

/// Truncate a string for display
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.replace('\n', " ")
    } else {
        format!("{}...", s[..max_len].replace('\n', " "))
    }
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
mod tests {
    use super::*;

    // Note: Full integration tests would require mocking the LLM provider
    // These are unit tests for the helper functions

    #[test]
    fn test_truncate_str_short() {
        let short = "Hello";
        assert_eq!(truncate_str(short, 100), "Hello");
    }

    #[test]
    fn test_truncate_str_long() {
        let long = "A".repeat(150);
        let result = truncate_str(&long, 100);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 104);
    }

    #[test]
    fn test_truncate_str_newlines() {
        let with_newlines = "Line 1\nLine 2\nLine 3";
        let result = truncate_str(with_newlines, 100);
        assert!(!result.contains('\n'));
    }

    #[test]
    fn test_runner_config_default() {
        let config = RunnerConfig::default();
        assert_eq!(config.max_response_tokens, 4096);
        assert_eq!(config.temperature, 0.7);
        assert!(!config.verbose);
    }
}
