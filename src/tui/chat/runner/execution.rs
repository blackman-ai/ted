// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event as TermEvent, KeyCode, KeyModifiers};
use ratatui::prelude::*;
use uuid::Uuid;

use crate::error::{Result, TedError};
use crate::tools::builtin::{AgentConversationEntry, ToolCallEntryStatus};
use crate::tools::{ToolExecutor, ToolResult};
use crate::tui::chat::app::ChatMode;
use crate::tui::chat::state::agents::AgentStatus;
use crate::tui::chat::state::{AgentTracker, DisplayMessage, DisplayToolCall};

use super::render::draw_tui;
use super::{handle_command, handle_key, TuiState};

fn sync_all_agents_from_tracker(
    state: &mut TuiState,
    agent_tools: &[(String, String, serde_json::Value)],
) {
    if let Some(ref tracker) = state.agent_progress_tracker {
        for (id, _, _) in agent_tools {
            let progress_data = if let Ok(guard) = tracker.try_lock() {
                guard.get(id.as_str()).map(|p| {
                    (
                        p.display_status(),
                        p.agent_type.clone(),
                        p.task.clone(),
                        p.conversation.clone(),
                        p.rate_limited,
                        p.rate_limit_wait_secs,
                        p.completed,
                    )
                })
            } else {
                None
            };

            if let Some((
                status_text,
                agent_type,
                task_str,
                conversation,
                rate_limited,
                rate_limit_wait,
                completed,
            )) = progress_data
            {
                // Update the DisplayToolCall's progress display
                if let Some(msg) = state.messages.last_mut() {
                    if let Some(tc) = msg.find_tool_call_mut(id) {
                        tc.set_progress_text(&status_text);
                    }
                }

                // Sync conversation for split-pane display
                sync_agent_conversation(
                    &mut state.agents,
                    id,
                    &conversation,
                    &mut state.focused_agent_tool_id,
                    &agent_type,
                    &task_str,
                );

                // Sync rate limit and completion status
                if let Some(agent) = state.agents.get_mut_by_tool_call_id(id) {
                    if rate_limited {
                        agent.status = AgentStatus::RateLimited {
                            wait_secs: rate_limit_wait,
                        };
                    } else if completed {
                        // Will be set properly when tool result arrives
                    } else if !matches!(agent.status, AgentStatus::Running) {
                        agent.status = AgentStatus::Running;
                    }
                }
            }
        }
    }
}

/// Handle input events during tool execution (scrolling, agent focus, typing).
fn handle_tool_execution_input(state: &mut TuiState, interrupted: &Arc<AtomicBool>) {
    while let Ok(true) = crossterm::event::poll(Duration::from_millis(0)) {
        if let Ok(TermEvent::Key(key)) = crossterm::event::read() {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                interrupted.store(true, Ordering::SeqCst);
                return;
            }
            // Handle Enter to submit/queue messages during tool execution
            if key.modifiers == KeyModifiers::NONE
                && key.code == KeyCode::Enter
                && !state.input.is_empty()
                && state.mode == ChatMode::Input
            {
                let input_text = state.input.submit();
                if input_text.trim().starts_with('/') {
                    let _ = handle_command(&input_text, state, None);
                } else {
                    state.pending_messages.push(input_text);
                    state.set_status(&format!(
                        "Message queued ({} pending)",
                        state.pending_messages.len()
                    ));
                }
            } else {
                // Delegate all other input to the standard mode handlers
                let _ = handle_key(state, key);
            }
        } else {
            break;
        }
    }
}

pub(super) const TUI_STREAM_INTERRUPTED: &str = "__tui_stream_interrupted__";

pub(super) struct TuiStreamObserver<'a, B: Backend> {
    pub(super) state: &'a mut TuiState,
    pub(super) terminal: &'a mut Terminal<B>,
    pub(super) interrupted: &'a Arc<AtomicBool>,
}

impl<B: Backend> crate::chat::engine::AgentLoopObserver for TuiStreamObserver<'_, B> {
    fn on_rate_limited(&mut self, delay_secs: u64, attempt: u32, max_retries: u32) -> Result<()> {
        self.state.set_status(&format!(
            "Rate limited. Retrying in {}s ({}/{})...",
            delay_secs, attempt, max_retries
        ));
        Ok(())
    }

    fn on_context_too_long(&mut self, current: u32, limit: u32) -> Result<()> {
        self.state.set_status(&format!(
            "Context too long ({} > {}). Auto-trimming...",
            current, limit
        ));
        Ok(())
    }

    fn on_context_trimmed(&mut self, removed: usize) -> Result<()> {
        if removed > 0 {
            self.state.set_status(&format!(
                "Context trimmed ({} messages removed). Retrying...",
                removed
            ));
        }
        Ok(())
    }

    fn on_stream_event_tick(&mut self) -> Result<()> {
        if self.interrupted.load(Ordering::SeqCst) {
            return Err(TedError::Agent(TUI_STREAM_INTERRUPTED.to_string()));
        }

        // Non-blocking key handling while streaming so the UI remains interactive.
        if let Ok(true) = crossterm::event::poll(Duration::from_millis(0)) {
            if let Ok(TermEvent::Key(key)) = crossterm::event::read() {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    self.interrupted.store(true, Ordering::SeqCst);
                    return Err(TedError::Agent(TUI_STREAM_INTERRUPTED.to_string()));
                }

                if key.modifiers == KeyModifiers::NONE
                    && key.code == KeyCode::Enter
                    && !self.state.input.is_empty()
                    && self.state.mode == ChatMode::Input
                {
                    let input_text = self.state.input.submit();
                    if input_text.trim().starts_with('/') {
                        let _ = handle_command(&input_text, self.state, None);
                    } else {
                        self.state.pending_messages.push(input_text);
                        self.state.set_status(&format!(
                            "Message queued ({} pending)",
                            self.state.pending_messages.len()
                        ));
                    }
                } else {
                    let _ = handle_key(self.state, key);
                }
            }
        }

        Ok(())
    }

    fn on_text_delta(&mut self, text: &str) -> Result<()> {
        if let Some(msg) = self.state.messages.last_mut() {
            msg.append_content(text);
        }
        self.state.tick_animation();
        self.state.auto_scroll();
        let _ = self.terminal.draw(|f| draw_tui(f, self.state));
        Ok(())
    }
}

pub(super) struct TuiNonStreamObserver<'a> {
    pub(super) state: &'a mut TuiState,
}

impl crate::chat::engine::AgentLoopObserver for TuiNonStreamObserver<'_> {
    fn on_rate_limited(&mut self, delay_secs: u64, attempt: u32, max_retries: u32) -> Result<()> {
        self.state.set_status(&format!(
            "Rate limited. Retrying in {}s ({}/{})...",
            delay_secs, attempt, max_retries
        ));
        Ok(())
    }

    fn on_context_too_long(&mut self, current: u32, limit: u32) -> Result<()> {
        self.state.set_status(&format!(
            "Context too long ({} > {}). Auto-trimming...",
            current, limit
        ));
        Ok(())
    }

    fn on_context_trimmed(&mut self, removed: usize) -> Result<()> {
        if removed > 0 {
            self.state.set_status(&format!(
                "Context trimmed ({} messages removed). Retrying...",
                removed
            ));
        }
        Ok(())
    }
}

pub(super) struct TuiToolExecutionStrategy<'a, B: Backend> {
    pub(super) state: &'a mut TuiState,
    pub(super) terminal: &'a mut Terminal<B>,
}

#[async_trait::async_trait(?Send)]
impl<B: Backend> crate::chat::engine::ToolExecutionStrategy for TuiToolExecutionStrategy<'_, B> {
    async fn execute_tool_calls(
        &mut self,
        tool_executor: &mut ToolExecutor,
        calls: &[crate::chat::engine::ToolUse],
        interrupted: &Arc<AtomicBool>,
    ) -> Result<crate::chat::engine::ToolExecutionBatch> {
        let mut tool_results: Vec<ToolResult> = Vec::new();
        let mut cancelled_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Separate spawn_agent calls from regular tools for parallel execution.
        let mut regular_tools: Vec<(String, String, serde_json::Value)> = Vec::new();
        let mut agent_tools: Vec<(String, String, serde_json::Value)> = Vec::new();

        for (id, name, input) in calls {
            let parsed_input = input.clone();

            // Add tool call to UI.
            if let Some(msg) = self.state.messages.last_mut() {
                msg.add_tool_call(DisplayToolCall::new(
                    id.clone(),
                    name.clone(),
                    parsed_input.clone(),
                ));
            }

            if name == "spawn_agent" {
                agent_tools.push((id.clone(), name.clone(), parsed_input));
            } else {
                regular_tools.push((id.clone(), name.clone(), parsed_input));
            }
        }

        self.state.tick_animation();
        self.state.auto_scroll();
        let _ = self.terminal.draw(|f| draw_tui(f, self.state));

        // Phase 1: launch all spawn_agent calls concurrently.
        let mut agent_handles: Vec<(String, tokio::task::JoinHandle<ToolResult>)> = Vec::new();
        for (id, _name, parsed_input) in &agent_tools {
            if interrupted.load(Ordering::SeqCst) {
                break;
            }

            match tool_executor.approve_and_get_tool("spawn_agent", parsed_input) {
                Ok(Some((tool, ctx))) => {
                    let tool_id = id.clone();
                    let input = parsed_input.clone();
                    let handle = tokio::spawn(async move {
                        match tool.execute(tool_id.clone(), input, &ctx).await {
                            Ok(result) => result,
                            Err(e) => ToolResult::error(&tool_id, e.to_string()),
                        }
                    });
                    agent_handles.push((id.clone(), handle));
                }
                Ok(None) => {
                    tool_results.push(ToolResult::error(id, "Permission denied by user"));
                }
                Err(e) => {
                    tool_results.push(ToolResult::error(id, e.to_string()));
                }
            }
        }

        // Phase 2: execute regular tools while agents run in background.
        let mut cancelled_mid_execution = false;
        for (id, name, parsed_input) in &regular_tools {
            if interrupted.load(Ordering::SeqCst) {
                break;
            }

            let id_clone = id.clone();
            let name_clone = name.clone();
            let tool_future =
                tool_executor.execute_tool_use(&id_clone, &name_clone, parsed_input.clone());
            tokio::pin!(tool_future);

            let result = loop {
                if interrupted.load(Ordering::SeqCst) {
                    if let Some(msg) = self.state.messages.last_mut() {
                        if let Some(tc) = msg.find_tool_call_mut(id) {
                            tc.complete_failed("Cancelled by user".to_string());
                        }
                    }
                    cancelled_ids.insert(id.clone());
                    cancelled_mid_execution = true;
                    break None;
                }

                tokio::select! {
                    result = &mut tool_future => {
                        break Some(result?);
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        self.state.tick_animation();
                        sync_all_agents_from_tracker(self.state, &agent_tools);
                        handle_tool_execution_input(self.state, interrupted);
                        self.state.auto_scroll();
                        let _ = self.terminal.draw(|f| draw_tui(f, self.state));
                    }
                }
            };

            if cancelled_mid_execution {
                break;
            }

            if let Some(result) = result {
                update_tool_call_ui(self.state, id, &result);
                let _ = self.terminal.draw(|f| draw_tui(f, self.state));
                tool_results.push(result);
            }
        }

        // Phase 3: wait for spawned agent tasks to complete.
        if !agent_handles.is_empty() && !interrupted.load(Ordering::SeqCst) {
            loop {
                if interrupted.load(Ordering::SeqCst) {
                    break;
                }

                let all_done = agent_handles.iter().all(|(_, h)| h.is_finished());
                sync_all_agents_from_tracker(self.state, &agent_tools);
                handle_tool_execution_input(self.state, interrupted);

                self.state.tick_animation();
                self.state.auto_scroll();
                let _ = self.terminal.draw(|f| draw_tui(f, self.state));

                if all_done {
                    break;
                }

                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            for (id, handle) in agent_handles {
                let result = match handle.await {
                    Ok(result) => result,
                    Err(e) => ToolResult::error(&id, format!("Agent task panicked: {}", e)),
                };
                update_tool_call_ui(self.state, &id, &result);
                tool_results.push(result);
            }

            let _ = self.terminal.draw(|f| draw_tui(f, self.state));
        }

        // Ensure any unprocessed tool calls still get a cancelled result id on interrupt.
        if interrupted.load(Ordering::SeqCst) {
            let completed_ids: std::collections::HashSet<String> =
                tool_results.iter().map(|r| r.tool_use_id.clone()).collect();
            for (id, _, _) in calls {
                if !completed_ids.contains(id) {
                    cancelled_ids.insert(id.clone());
                    if let Some(msg) = self.state.messages.last_mut() {
                        if let Some(tc) = msg.find_tool_call_mut(id) {
                            tc.complete_failed("Cancelled by user".to_string());
                        }
                    }
                }
            }
        }

        Ok(crate::chat::engine::ToolExecutionBatch {
            results: tool_results,
            cancelled_tool_use_ids: cancelled_ids.into_iter().collect(),
        })
    }
}

/// Update a tool call's display in the UI with its result.
fn update_tool_call_ui(state: &mut TuiState, id: &str, result: &ToolResult) {
    if let Some(msg) = state.messages.last_mut() {
        if let Some(tc) = msg.find_tool_call_mut(id) {
            let output = result.output_text();
            if result.is_error() {
                tc.complete_failed(output.to_string());
            } else {
                let preview = if output.chars().count() > 100 {
                    let truncated: String = output.chars().take(97).collect();
                    Some(format!("{}...", truncated))
                } else {
                    Some(output.to_string())
                };
                tc.complete_success(preview, Some(output.to_string()));
            }
        }
    }
    state.tick_animation();
}

/// Sync conversation data from ProgressTracker into TrackedAgent for split-pane display.
/// Called during the TUI poll loop when a spawn_agent tool is running.
fn sync_agent_conversation(
    agents: &mut AgentTracker,
    tool_call_id: &str,
    conversation: &[AgentConversationEntry],
    focused_agent: &mut Option<String>,
    agent_type: &str,
    task: &str,
) {
    // Ensure this agent is tracked
    if agents.get_by_tool_call_id(tool_call_id).is_none() {
        let uuid = Uuid::new_v4();
        agents.track(
            uuid,
            tool_call_id.to_string(),
            agent_type.to_string(),
            agent_type.to_string(),
            task.to_string(),
        );
        agents.set_running(&uuid);
        // Auto-focus the first agent
        if focused_agent.is_none() {
            *focused_agent = Some(tool_call_id.to_string());
        }
    }

    // Rebuild messages from conversation entries
    if let Some(agent) = agents.get_mut_by_tool_call_id(tool_call_id) {
        agent.messages.clear();
        let mut current_msg: Option<DisplayMessage> = None;

        for entry in conversation {
            match entry {
                AgentConversationEntry::AssistantMessage(text) => {
                    // Flush previous message
                    if let Some(msg) = current_msg.take() {
                        agent.messages.push(msg);
                    }
                    current_msg = Some(DisplayMessage::assistant(text.clone(), vec![]));
                }
                AgentConversationEntry::ToolCall {
                    id,
                    name,
                    input,
                    status,
                    output_full,
                    ..
                } => {
                    // Ensure we have a message to attach tool calls to
                    if current_msg.is_none() {
                        current_msg = Some(DisplayMessage::assistant(String::new(), vec![]));
                    }
                    let msg = current_msg.as_mut().unwrap();
                    let mut tc = DisplayToolCall::new(id.clone(), name.clone(), input.clone());
                    match status {
                        ToolCallEntryStatus::Success { preview } => {
                            tc.complete_success(preview.clone(), output_full.clone());
                        }
                        ToolCallEntryStatus::Failed { error } => {
                            tc.complete_failed(error.clone());
                        }
                        ToolCallEntryStatus::Running => {} // stays running
                    }
                    msg.add_tool_call(tc);
                }
            }
        }
        // Flush last message
        if let Some(msg) = current_msg {
            agent.messages.push(msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::engine::{AgentLoopObserver, ToolExecutionStrategy};
    use crate::config::Settings;
    use crate::llm::mock_provider::MockProvider;
    use crate::llm::provider::LlmProvider;
    use crate::skills::SkillRegistry;
    use crate::tools::builtin::{new_progress_tracker, AgentProgressState};
    use crate::tools::ToolContext;
    use crate::tui::chat::state::ToolCallStatus;
    use crate::tui::chat::ChatTuiConfig;
    use ratatui::backend::TestBackend;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn make_state() -> TuiState {
        let settings = Settings::default();
        let config = ChatTuiConfig {
            session_id: uuid::Uuid::new_v4(),
            provider_name: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            caps: vec!["base".to_string()],
            trust_mode: false,
            stream_enabled: true,
        };
        TuiState::new(config, &settings)
    }

    fn make_terminal() -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(120, 40)).unwrap()
    }

    fn make_tool_executor(workdir: &std::path::Path) -> ToolExecutor {
        let context = ToolContext::new(
            workdir.to_path_buf(),
            Some(workdir.to_path_buf()),
            uuid::Uuid::new_v4(),
            true,
        );
        ToolExecutor::new(context, true)
    }

    #[test]
    fn test_update_tool_call_ui_success_and_error() {
        let mut state = make_state();
        let mut message = DisplayMessage::assistant(String::new(), vec![]);
        message.add_tool_call(DisplayToolCall::new(
            "tc_success".to_string(),
            "shell".to_string(),
            serde_json::json!({"command": "echo ok"}),
        ));
        message.add_tool_call(DisplayToolCall::new(
            "tc_error".to_string(),
            "shell".to_string(),
            serde_json::json!({"command": "exit 1"}),
        ));
        state.messages.push(message);

        let long_output = "x".repeat(140);
        let success = ToolResult::success("tc_success", long_output.clone());
        update_tool_call_ui(&mut state, "tc_success", &success);

        let error = ToolResult::error("tc_error", "command failed");
        update_tool_call_ui(&mut state, "tc_error", &error);

        let msg = state.messages.last().unwrap();
        let success_tc = msg.find_tool_call("tc_success").unwrap();
        assert_eq!(success_tc.status, ToolCallStatus::Success);
        assert_eq!(
            success_tc.result_full.as_deref(),
            Some(long_output.as_str())
        );
        let preview = success_tc.result_preview.as_deref().unwrap();
        assert!(preview.len() <= 100);
        assert!(preview.ends_with("..."));

        let error_tc = msg.find_tool_call("tc_error").unwrap();
        assert_eq!(error_tc.status, ToolCallStatus::Failed);
        assert_eq!(error_tc.result_preview.as_deref(), Some("command failed"));
    }

    #[test]
    fn test_sync_agent_conversation_creates_agent_and_focus() {
        let mut agents = AgentTracker::new();
        let mut focused = None;
        let conversation = vec![
            AgentConversationEntry::AssistantMessage("Working".to_string()),
            AgentConversationEntry::ToolCall {
                id: "tool_1".to_string(),
                name: "shell".to_string(),
                input: serde_json::json!({"command":"echo hi"}),
                input_summary: "echo hi".to_string(),
                status: ToolCallEntryStatus::Running,
                output_full: None,
            },
            AgentConversationEntry::AssistantMessage("Done".to_string()),
            AgentConversationEntry::ToolCall {
                id: "tool_2".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({"path":"Cargo.toml"}),
                input_summary: "Cargo.toml".to_string(),
                status: ToolCallEntryStatus::Success {
                    preview: Some("ok".to_string()),
                },
                output_full: Some("full output".to_string()),
            },
        ];

        sync_agent_conversation(
            &mut agents,
            "spawn_1",
            &conversation,
            &mut focused,
            "implement",
            "Do a task",
        );

        assert_eq!(focused.as_deref(), Some("spawn_1"));
        let agent = agents.get_by_tool_call_id("spawn_1").unwrap();
        assert_eq!(agent.messages.len(), 2);
        assert_eq!(agent.messages[0].content, "Working");
        assert_eq!(agent.messages[0].tool_calls.len(), 1);
        assert_eq!(
            agent.messages[0].tool_calls[0].status,
            ToolCallStatus::Running
        );
        assert_eq!(agent.messages[1].content, "Done");
        assert_eq!(agent.messages[1].tool_calls.len(), 1);
        assert_eq!(
            agent.messages[1].tool_calls[0].status,
            ToolCallStatus::Success
        );
        assert_eq!(
            agent.messages[1].tool_calls[0].result_full.as_deref(),
            Some("full output")
        );
    }

    #[test]
    fn test_sync_agent_conversation_tool_call_without_assistant_message() {
        let mut agents = AgentTracker::new();
        let mut focused = None;
        let conversation = vec![AgentConversationEntry::ToolCall {
            id: "tool_failed".to_string(),
            name: "shell".to_string(),
            input: serde_json::json!({"command":"bad"}),
            input_summary: "bad".to_string(),
            status: ToolCallEntryStatus::Failed {
                error: "boom".to_string(),
            },
            output_full: Some("boom".to_string()),
        }];

        sync_agent_conversation(
            &mut agents,
            "spawn_2",
            &conversation,
            &mut focused,
            "review",
            "Review task",
        );

        let agent = agents.get_by_tool_call_id("spawn_2").unwrap();
        assert_eq!(agent.messages.len(), 1);
        assert_eq!(agent.messages[0].content, "");
        assert_eq!(agent.messages[0].tool_calls.len(), 1);
        assert_eq!(
            agent.messages[0].tool_calls[0].status,
            ToolCallStatus::Failed
        );
    }

    #[test]
    fn test_sync_all_agents_from_tracker_updates_tool_call_and_rate_limit() {
        let tracker = new_progress_tracker();
        {
            let mut guard = tracker.try_lock().unwrap();
            let mut progress = AgentProgressState {
                iteration: 1,
                max_iterations: 3,
                current_tool: Some("shell".to_string()),
                agent_type: "implement".to_string(),
                task: "Run shell".to_string(),
                rate_limited: true,
                rate_limit_wait_secs: 2.5,
                ..Default::default()
            };
            progress
                .conversation
                .push(AgentConversationEntry::AssistantMessage(
                    "Processing".to_string(),
                ));
            guard.insert("spawn_tool".to_string(), progress);
        }

        let mut state = make_state().with_progress_tracker(tracker);
        let mut message = DisplayMessage::assistant(String::new(), vec![]);
        message.add_tool_call(DisplayToolCall::new(
            "spawn_tool".to_string(),
            "spawn_agent".to_string(),
            serde_json::json!({"task": "Run shell"}),
        ));
        state.messages.push(message);

        sync_all_agents_from_tracker(
            &mut state,
            &[(
                "spawn_tool".to_string(),
                "spawn_agent".to_string(),
                serde_json::json!({"task":"Run shell"}),
            )],
        );

        let tool_call = state
            .messages
            .last()
            .and_then(|m| m.find_tool_call("spawn_tool"))
            .unwrap();
        assert!(tool_call
            .result_preview
            .as_deref()
            .unwrap_or_default()
            .contains("shell"));
        assert_eq!(state.focused_agent_tool_id.as_deref(), Some("spawn_tool"));
        let agent = state.agents.get_by_tool_call_id("spawn_tool").unwrap();
        assert!(matches!(
            agent.status,
            AgentStatus::RateLimited { wait_secs } if (wait_secs - 2.5).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn test_sync_all_agents_from_tracker_preserves_completed_status() {
        let tracker = new_progress_tracker();
        {
            let mut guard = tracker.try_lock().unwrap();
            let mut progress = AgentProgressState {
                iteration: 2,
                max_iterations: 2,
                completed: true,
                agent_type: "plan".to_string(),
                task: "Finish planning".to_string(),
                ..Default::default()
            };
            progress
                .conversation
                .push(AgentConversationEntry::AssistantMessage("Done".to_string()));
            guard.insert("spawn_done".to_string(), progress);
        }

        let mut state = make_state().with_progress_tracker(tracker);
        let mut message = DisplayMessage::assistant(String::new(), vec![]);
        message.add_tool_call(DisplayToolCall::new(
            "spawn_done".to_string(),
            "spawn_agent".to_string(),
            serde_json::json!({"task":"Finish planning"}),
        ));
        state.messages.push(message);

        let calls = vec![(
            "spawn_done".to_string(),
            "spawn_agent".to_string(),
            serde_json::json!({"task":"Finish planning"}),
        )];

        sync_all_agents_from_tracker(&mut state, &calls);
        if let Some(agent) = state.agents.get_mut_by_tool_call_id("spawn_done") {
            agent.status = AgentStatus::Completed;
        }
        sync_all_agents_from_tracker(&mut state, &calls);

        let agent = state.agents.get_by_tool_call_id("spawn_done").unwrap();
        assert_eq!(agent.status, AgentStatus::Completed);
    }

    #[test]
    fn test_non_stream_observer_status_updates() {
        let mut state = make_state();
        {
            let mut observer = TuiNonStreamObserver { state: &mut state };
            observer.on_rate_limited(3, 1, 3).unwrap();
            assert!(observer
                .state
                .status_message
                .as_deref()
                .unwrap_or_default()
                .contains("Retrying in 3s"));

            observer.on_context_too_long(9000, 8000).unwrap();
            assert!(observer
                .state
                .status_message
                .as_deref()
                .unwrap_or_default()
                .contains("Context too long"));

            observer.on_context_trimmed(2).unwrap();
            assert!(observer
                .state
                .status_message
                .as_deref()
                .unwrap_or_default()
                .contains("2 messages removed"));
        }
    }

    #[test]
    fn test_stream_observer_on_text_delta_appends_content() {
        let mut state = make_state();
        state.messages.push(DisplayMessage::assistant_streaming(
            vec!["base".to_string()],
        ));
        let interrupted = Arc::new(AtomicBool::new(false));
        let mut terminal = make_terminal();

        {
            let mut observer = TuiStreamObserver {
                state: &mut state,
                terminal: &mut terminal,
                interrupted: &interrupted,
            };
            observer.on_text_delta("hello ").unwrap();
            observer.on_text_delta("world").unwrap();
        }

        let msg = state.messages.last().unwrap();
        assert_eq!(msg.content, "hello world");
    }

    #[test]
    fn test_stream_observer_tick_interrupt_flag_errors() {
        let mut state = make_state();
        let interrupted = Arc::new(AtomicBool::new(true));
        let mut terminal = make_terminal();

        let mut observer = TuiStreamObserver {
            state: &mut state,
            terminal: &mut terminal,
            interrupted: &interrupted,
        };

        let error = observer.on_stream_event_tick().unwrap_err();
        match error {
            TedError::Agent(msg) => assert_eq!(msg, TUI_STREAM_INTERRUPTED),
            other => panic!("Expected interrupted error, got {other:?}"),
        }
    }

    #[test]
    fn test_stream_observer_context_trimmed_zero_keeps_status() {
        let mut state = make_state();
        state.set_status("existing status");
        let interrupted = Arc::new(AtomicBool::new(false));
        let mut terminal = make_terminal();

        {
            let mut observer = TuiStreamObserver {
                state: &mut state,
                terminal: &mut terminal,
                interrupted: &interrupted,
            };
            observer.on_context_trimmed(0).unwrap();
        }

        assert_eq!(state.status_message.as_deref(), Some("existing status"));
    }

    #[test]
    fn test_stream_observer_tick_no_interrupt_returns_ok() {
        let mut state = make_state();
        let interrupted = Arc::new(AtomicBool::new(false));
        let mut terminal = make_terminal();
        let mut observer = TuiStreamObserver {
            state: &mut state,
            terminal: &mut terminal,
            interrupted: &interrupted,
        };
        assert!(observer.on_stream_event_tick().is_ok());
    }

    #[tokio::test]
    async fn test_tool_execution_strategy_regular_tool_success() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("hello.txt");
        std::fs::write(&file_path, "hello from tool").unwrap();

        let mut state = make_state();
        state
            .messages
            .push(DisplayMessage::assistant(String::new(), vec![]));
        let mut terminal = make_terminal();
        let mut executor = make_tool_executor(temp.path());
        let interrupted = Arc::new(AtomicBool::new(false));
        let calls = vec![(
            "read_1".to_string(),
            "file_read".to_string(),
            serde_json::json!({"path": file_path.to_string_lossy().to_string()}),
        )];

        let mut strategy = TuiToolExecutionStrategy {
            state: &mut state,
            terminal: &mut terminal,
        };
        let batch = strategy
            .execute_tool_calls(&mut executor, &calls, &interrupted)
            .await
            .unwrap();

        assert_eq!(batch.results.len(), 1);
        assert!(batch.cancelled_tool_use_ids.is_empty());
        assert!(!batch.results[0].is_error());
        let tool_call = state
            .messages
            .last()
            .and_then(|m| m.find_tool_call("read_1"))
            .unwrap();
        assert_eq!(tool_call.status, ToolCallStatus::Success);
    }

    #[tokio::test]
    async fn test_tool_execution_strategy_interrupted_marks_cancelled_calls() {
        let temp = TempDir::new().unwrap();
        let mut state = make_state();
        state
            .messages
            .push(DisplayMessage::assistant(String::new(), vec![]));
        let mut terminal = make_terminal();
        let mut executor = make_tool_executor(temp.path());
        let interrupted = Arc::new(AtomicBool::new(true));
        let calls = vec![(
            "cancel_1".to_string(),
            "shell".to_string(),
            serde_json::json!({"command": "echo never-runs"}),
        )];

        let mut strategy = TuiToolExecutionStrategy {
            state: &mut state,
            terminal: &mut terminal,
        };
        let batch = strategy
            .execute_tool_calls(&mut executor, &calls, &interrupted)
            .await
            .unwrap();

        assert!(batch.results.is_empty());
        assert_eq!(batch.cancelled_tool_use_ids, vec!["cancel_1".to_string()]);
        let tool_call = state
            .messages
            .last()
            .and_then(|m| m.find_tool_call("cancel_1"))
            .unwrap();
        assert_eq!(tool_call.status, ToolCallStatus::Failed);
    }

    #[tokio::test]
    async fn test_tool_execution_strategy_spawn_agent_without_registration_returns_error() {
        let temp = TempDir::new().unwrap();
        let mut state = make_state();
        state
            .messages
            .push(DisplayMessage::assistant(String::new(), vec![]));
        let mut terminal = make_terminal();
        let mut executor = make_tool_executor(temp.path());
        let interrupted = Arc::new(AtomicBool::new(false));
        let calls = vec![(
            "spawn_1".to_string(),
            "spawn_agent".to_string(),
            serde_json::json!({"agent_type":"plan","task":"Draft a plan"}),
        )];

        let mut strategy = TuiToolExecutionStrategy {
            state: &mut state,
            terminal: &mut terminal,
        };
        let batch = strategy
            .execute_tool_calls(&mut executor, &calls, &interrupted)
            .await
            .unwrap();

        assert_eq!(batch.results.len(), 1);
        assert!(batch.results[0].is_error());
        assert!(batch.results[0]
            .output_text()
            .contains("Unknown tool: spawn_agent"));
    }

    #[tokio::test]
    async fn test_tool_execution_strategy_spawn_agent_registered_background() {
        let temp = TempDir::new().unwrap();
        let mut state = make_state();
        state
            .messages
            .push(DisplayMessage::assistant(String::new(), vec![]));
        let mut terminal = make_terminal();
        let mut executor = make_tool_executor(temp.path());

        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::new());
        let skill_registry = Arc::new(SkillRegistry::new());
        executor.registry_mut().register_spawn_agent(
            provider,
            skill_registry,
            "mock-model".to_string(),
        );

        let interrupted = Arc::new(AtomicBool::new(false));
        let calls = vec![(
            "spawn_bg_1".to_string(),
            "spawn_agent".to_string(),
            serde_json::json!({
                "agent_type": "plan",
                "task": "Outline architecture",
                "background": true
            }),
        )];

        let mut strategy = TuiToolExecutionStrategy {
            state: &mut state,
            terminal: &mut terminal,
        };
        let batch = strategy
            .execute_tool_calls(&mut executor, &calls, &interrupted)
            .await
            .unwrap();

        assert_eq!(batch.results.len(), 1);
        assert!(!batch.results[0].is_error());
        assert!(batch.results[0]
            .output_text()
            .contains("Spawned background agent"));
        let tool_call = state
            .messages
            .last()
            .and_then(|m| m.find_tool_call("spawn_bg_1"))
            .unwrap();
        assert_eq!(tool_call.status, ToolCallStatus::Success);
    }

    #[tokio::test]
    async fn test_tool_execution_strategy_cancels_mid_regular_tool_execution() {
        let temp = TempDir::new().unwrap();
        let mut state = make_state();
        state
            .messages
            .push(DisplayMessage::assistant(String::new(), vec![]));
        let mut terminal = make_terminal();
        let mut executor = make_tool_executor(temp.path());
        let interrupted = Arc::new(AtomicBool::new(false));
        let interrupted_setter = Arc::clone(&interrupted);

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            interrupted_setter.store(true, Ordering::SeqCst);
        });

        let calls = vec![(
            "cancel_mid_1".to_string(),
            "shell".to_string(),
            serde_json::json!({"command": "sleep 1"}),
        )];

        let mut strategy = TuiToolExecutionStrategy {
            state: &mut state,
            terminal: &mut terminal,
        };
        let batch = strategy
            .execute_tool_calls(&mut executor, &calls, &interrupted)
            .await
            .unwrap();

        assert!(batch.results.is_empty());
        assert!(batch
            .cancelled_tool_use_ids
            .contains(&"cancel_mid_1".to_string()));
        let tool_call = state
            .messages
            .last()
            .and_then(|m| m.find_tool_call("cancel_mid_1"))
            .unwrap();
        assert_eq!(tool_call.status, ToolCallStatus::Failed);
    }
}
