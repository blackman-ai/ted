// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::io::{self, Write};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crossterm::{
    style::{Color, ResetColor, SetForegroundColor},
    ExecutableCommand,
};

use ted::chat;
use ted::config::Settings;
use ted::context::ContextManager;
use ted::error::Result;
use ted::llm::message::Conversation;
use ted::llm::provider::LlmProvider;
use ted::tools::{ToolExecutor, ToolResult};

#[cfg(test)]
use ted::llm::provider::{CompletionRequest, ContentBlockResponse, StopReason};

use super::{print_response_prefix, print_tool_invocation, print_tool_result};

#[derive(Default)]
struct CliAgentObserver;

impl chat::AgentLoopObserver for CliAgentObserver {
    fn on_response_prefix(&mut self, active_caps: &[String]) -> Result<()> {
        print_response_prefix(active_caps)
    }

    fn on_text_delta(&mut self, text: &str) -> Result<()> {
        let mut stdout = io::stdout();
        print!("{}", text);
        stdout.flush()?;
        Ok(())
    }

    fn on_rate_limited(&mut self, delay_secs: u64, attempt: u32, max_retries: u32) -> Result<()> {
        let mut stdout = io::stdout();
        stdout.execute(SetForegroundColor(Color::Yellow))?;
        println!(
            "\n⏳ Rate limited. Retrying in {} seconds... (attempt {}/{})",
            delay_secs, attempt, max_retries
        );
        stdout.execute(ResetColor)?;
        Ok(())
    }

    fn on_context_too_long(&mut self, current: u32, limit: u32) -> Result<()> {
        let mut stdout = io::stdout();
        stdout.execute(SetForegroundColor(Color::Yellow))?;
        println!(
            "\n⚠ Context too long ({} tokens > {} limit). Auto-trimming older messages...",
            current, limit
        );
        stdout.execute(ResetColor)?;
        Ok(())
    }

    fn on_context_trimmed(&mut self, removed: usize) -> Result<()> {
        if removed > 0 {
            println!("  Removed {} older messages. Retrying...\n", removed);
        }
        Ok(())
    }

    fn on_tool_phase_start(&mut self) -> Result<()> {
        println!();
        Ok(())
    }

    fn on_tool_invocation(&mut self, tool_name: &str, input: &serde_json::Value) -> Result<()> {
        print_tool_invocation(tool_name, input)
    }

    fn on_tool_result(&mut self, tool_name: &str, result: &ToolResult) -> Result<()> {
        print_tool_result(tool_name, result)
    }

    fn on_loop_detected(&mut self, tool_name: &str, count: usize) -> Result<()> {
        println!(
            "  ⚠️  Loop detected: '{}' called {} times with same arguments. Breaking loop.",
            tool_name, count
        );
        Ok(())
    }

    fn on_loop_recovery(&mut self) -> Result<()> {
        println!("\n  Giving model a chance to try a different approach...\n");
        Ok(())
    }

    fn on_agent_complete(&mut self) -> Result<()> {
        println!();
        Ok(())
    }
}

/// Run the agent loop - handles streaming, tool use, and multi-turn interactions.
/// Returns Ok(true) if completed normally, Ok(false) if interrupted by Ctrl+C.
/// On error or interruption, automatically restores conversation to its initial state.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_agent_loop(
    provider: &dyn LlmProvider,
    model: &str,
    conversation: &mut Conversation,
    tool_executor: &mut ToolExecutor,
    settings: &Settings,
    context_manager: &ContextManager,
    stream: bool,
    active_caps: &[String],
    interrupted: Arc<AtomicBool>,
) -> Result<bool> {
    let mut observer = CliAgentObserver;
    chat::engine::run_agent_loop(
        provider,
        model,
        conversation,
        tool_executor,
        settings,
        context_manager,
        stream,
        active_caps,
        interrupted,
        &mut observer,
    )
    .await
}

/// Inner implementation of the agent loop.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_agent_loop_inner(
    provider: &dyn LlmProvider,
    model: &str,
    conversation: &mut Conversation,
    tool_executor: &mut ToolExecutor,
    settings: &Settings,
    context_manager: &ContextManager,
    stream: bool,
    active_caps: &[String],
    interrupted: Arc<AtomicBool>,
) -> Result<bool> {
    let mut observer = CliAgentObserver;
    chat::engine::run_agent_loop_inner(
        provider,
        model,
        conversation,
        tool_executor,
        settings,
        context_manager,
        stream,
        active_caps,
        interrupted,
        &mut observer,
    )
    .await
}

/// Get response from LLM with retry logic for rate limits.
#[cfg(test)]
pub(super) async fn get_response_with_retry(
    provider: &dyn LlmProvider,
    request: CompletionRequest,
    stream: bool,
    active_caps: &[String],
) -> Result<(Vec<ContentBlockResponse>, Option<StopReason>)> {
    let mut observer = CliAgentObserver;
    chat::engine::get_response_with_retry(provider, request, stream, active_caps, &mut observer)
        .await
}

/// Stream the response from the LLM and return content blocks.
#[cfg(test)]
pub(super) async fn stream_response(
    provider: &dyn LlmProvider,
    request: CompletionRequest,
    active_caps: &[String],
) -> Result<(Vec<ContentBlockResponse>, Option<StopReason>)> {
    let mut observer = CliAgentObserver;
    chat::engine::stream_response(provider, request, active_caps, &mut observer).await
}
