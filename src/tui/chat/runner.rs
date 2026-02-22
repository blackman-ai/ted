// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Chat TUI runner
//!
//! Integrates the chat TUI with the LLM provider and tool execution.
//! This module handles the actual chat loop within the TUI context.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event::{Event as TermEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use crate::caps::available_caps;
use crate::config::Settings;
use crate::context::ContextManager;
use crate::error::{Result, TedError};
use crate::history::{HistoryStore, SessionInfo};
use crate::llm::message::{Conversation, Message};
use crate::llm::provider::LlmProvider;
use crate::tools::builtin::ProgressTracker;
use crate::tools::ToolExecutor;

use super::app::ChatMode;
use super::state::{AgentTracker, DisplayMessage, InputState};
use super::ChatTuiConfig;

mod commands;
mod execution;
mod keymap;
mod render;
mod settings;
mod turn;

use commands::handle_command;
use keymap::handle_key;
#[cfg(test)]
use keymap::{handle_help_key, handle_input_key, handle_normal_key, handle_settings_key};
use render::*;
use settings::{SettingsField, SettingsSection, SettingsState};
use turn::process_llm_response;

/// Simplified TUI state for the runner
pub struct TuiState {
    pub config: ChatTuiConfig,
    pub mode: ChatMode,
    pub messages: Vec<DisplayMessage>,
    pub agents: AgentTracker,
    pub input: InputState,
    pub status_message: Option<String>,
    pub status_is_error: bool,
    pub is_processing: bool,
    pub should_quit: bool,
    pub agent_pane_visible: bool,
    pub agent_pane_expanded: bool,
    pub agent_pane_height: u16,
    pub scroll_offset: usize,
    pub selected_agent_index: usize,
    /// Track chat area height for auto-scroll calculations
    pub chat_area_height: u16,
    /// Current model (can be changed with /model or settings)
    pub current_model: String,
    /// Settings editor state
    pub settings_state: Option<SettingsState>,
    /// Need to restart after settings change (provider changed)
    pub needs_restart: bool,
    /// Animation frame counter for thinking indicator
    pub animation_frame: u8,
    /// Available caps (name, is_builtin)
    pub available_caps: Vec<(String, bool)>,
    /// Currently enabled caps
    pub enabled_caps: Vec<String>,
    /// Agent progress tracker for spawn_agent visibility
    pub agent_progress_tracker: Option<ProgressTracker>,
    /// Flag indicating caps changed and system prompt needs regeneration
    pub caps_changed: bool,
    /// Queued messages to send after current processing completes
    pub pending_messages: Vec<String>,
    /// Tool call ID of the agent whose conversation is shown in the split pane
    pub focused_agent_tool_id: Option<String>,
}

impl TuiState {
    pub fn new(config: ChatTuiConfig, settings: &Settings) -> Self {
        let current_model = config.model.clone();
        let enabled_caps = config.caps.clone();
        let all_caps = available_caps().unwrap_or_default();
        let settings_state = Some(SettingsState::new(
            settings,
            &config,
            &enabled_caps,
            &all_caps,
        ));
        Self {
            config,
            mode: ChatMode::Input,
            messages: Vec::new(),
            agents: AgentTracker::new(),
            input: InputState::new(),
            status_message: None,
            status_is_error: false,
            is_processing: false,
            should_quit: false,
            agent_pane_visible: true,
            agent_pane_expanded: false,
            agent_pane_height: 4,
            scroll_offset: 0,
            selected_agent_index: 0,
            chat_area_height: 20, // Will be updated on first render
            current_model,
            settings_state,
            needs_restart: false,
            animation_frame: 0,
            available_caps: all_caps,
            enabled_caps,
            agent_progress_tracker: None,
            caps_changed: false,
            pending_messages: Vec::new(),
            focused_agent_tool_id: None,
        }
    }

    /// Set the agent progress tracker for spawn_agent visibility
    pub fn with_progress_tracker(mut self, tracker: ProgressTracker) -> Self {
        self.agent_progress_tracker = Some(tracker);
        self
    }

    /// Advance the animation frame (called on each render tick)
    pub fn tick_animation(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);
    }

    /// Get the current thinking indicator text
    pub fn thinking_indicator(&self) -> &'static str {
        // Cycle through different dot patterns every ~200ms (4 frames at 50ms poll)
        match (self.animation_frame / 4) % 4 {
            0 => "●○○",
            1 => "○●○",
            2 => "○○●",
            _ => "○●○",
        }
    }

    pub fn set_status(&mut self, msg: &str) {
        self.status_message = Some(msg.to_string());
        self.status_is_error = false;
    }

    pub fn set_error(&mut self, msg: &str) {
        self.status_message = Some(msg.to_string());
        self.status_is_error = true;
    }

    /// Calculate total height of all messages in lines
    fn total_messages_height(&self) -> usize {
        self.messages
            .iter()
            .map(|m| {
                let content_lines = m.content.lines().count().max(1);
                let tool_call_lines: usize = m
                    .tool_calls
                    .iter()
                    .map(|tc| if tc.expanded { 5 } else { 2 })
                    .sum();
                // Header (1) + content + tool calls + spacing (1)
                1 + content_lines + tool_call_lines + 1
            })
            .sum()
    }

    /// Auto-scroll to show the latest content
    pub fn scroll_to_bottom(&mut self, visible_height: u16) {
        let total_height = self.total_messages_height();
        if total_height > visible_height as usize {
            self.scroll_offset = total_height - visible_height as usize;
        } else {
            self.scroll_offset = 0;
        }
    }

    /// Update chat area height based on terminal size
    pub fn update_chat_height(&mut self, terminal_height: u16) {
        let title_height: u16 = 1;
        let input_height: u16 = 3;
        let agent_height = if self.agent_pane_visible && self.agents.total_count() > 0 {
            if self.agent_pane_expanded {
                self.agent_pane_height.min(terminal_height / 3)
            } else {
                3
            }
        } else {
            0
        };

        self.chat_area_height = terminal_height
            .saturating_sub(title_height)
            .saturating_sub(input_height)
            .saturating_sub(agent_height);
    }

    /// Auto-scroll using current chat height
    pub fn auto_scroll(&mut self) {
        self.scroll_to_bottom(self.chat_area_height);
    }

    /// Scroll up by a number of lines
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Scroll down by a number of lines
    pub fn scroll_down(&mut self, lines: usize) {
        let total_height = self.total_messages_height();
        let max_offset = total_height.saturating_sub(self.chat_area_height as usize);
        self.scroll_offset = (self.scroll_offset + lines).min(max_offset);
    }
}

/// Run the chat TUI with the given configuration
#[allow(clippy::too_many_arguments)]
pub async fn run_chat_tui_loop(
    config: ChatTuiConfig,
    provider: Arc<dyn LlmProvider>,
    mut tool_executor: ToolExecutor,
    context_manager: ContextManager,
    settings: Settings,
    mut conversation: Conversation,
    mut history_store: HistoryStore,
    mut session_info: SessionInfo,
    agent_progress_tracker: ProgressTracker,
) -> Result<()> {
    // Setup terminal with panic hook to restore terminal on crash
    let original_panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Restore terminal before panicking
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_panic_hook(panic_info);
    }));

    enable_raw_mode().map_err(|e| TedError::Tui(e.to_string()))?;
    let mut stdout = io::stdout();
    // Don't enable mouse capture - we don't handle mouse events
    execute!(stdout, EnterAlternateScreen).map_err(|e| TedError::Tui(e.to_string()))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| TedError::Tui(e.to_string()))?;

    // Create TUI state with agent progress tracker
    let mut state =
        TuiState::new(config.clone(), &settings).with_progress_tracker(agent_progress_tracker);
    let mut message_count = session_info.message_count;
    let interrupted = Arc::new(AtomicBool::new(false));

    // Main loop - returns (result, needs_restart)
    let result: (Result<()>, bool) = loop {
        // Update chat height based on terminal size before render
        let terminal_size = terminal.size().map(|s| s.height).unwrap_or(24);
        state.update_chat_height(terminal_size);

        // Tick animation (for thinking indicator)
        state.tick_animation();

        // Render UI
        terminal
            .draw(|f| draw_tui(f, &state))
            .map_err(|e| TedError::Tui(e.to_string()))?;

        // Poll for events with timeout
        let has_event = crossterm::event::poll(Duration::from_millis(50))
            .map_err(|e| TedError::Tui(e.to_string()))?;

        if has_event {
            let event = crossterm::event::read().map_err(|e| TedError::Tui(e.to_string()))?;

            match event {
                TermEvent::Key(key) => {
                    // Handle Ctrl+C globally
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        if state.is_processing {
                            interrupted.store(true, Ordering::SeqCst);
                            state.is_processing = false;
                            state.set_status("Interrupted");
                        } else {
                            break (Ok(()), false);
                        }
                        continue;
                    }

                    // Handle Enter in input mode to submit message
                    if state.mode == ChatMode::Input
                        && key.code == KeyCode::Enter
                        && !state.input.is_empty()
                    {
                        let input_text = state.input.submit();

                        // Check for exit commands
                        let trimmed = input_text.trim().to_lowercase();
                        if trimmed == "exit"
                            || trimmed == "quit"
                            || trimmed == "/exit"
                            || trimmed == "/quit"
                        {
                            break (Ok(()), false);
                        }

                        // Check for commands (always process immediately)
                        if input_text.trim().starts_with('/') {
                            handle_command(&input_text, &mut state, Some(&mut conversation))?;

                            // Check if the command queued an LLM message (e.g., /explain, /commit)
                            // If so, process it immediately instead of waiting
                            if !state.pending_messages.is_empty() && !state.is_processing {
                                let llm_message = state.pending_messages.remove(0);

                                // Add user message to UI (show the command + its generated prompt)
                                state
                                    .messages
                                    .push(DisplayMessage::user(llm_message.clone()));
                                state.auto_scroll();

                                // Mark as processing and re-render
                                state.is_processing = true;
                                terminal
                                    .draw(|f| draw_tui(f, &state))
                                    .map_err(|e| TedError::Tui(e.to_string()))?;

                                // Store in context
                                context_manager
                                    .store_message("user", &llm_message, None)
                                    .await?;

                                // Add to conversation
                                conversation.push(Message::user(&llm_message));

                                // Update history
                                crate::chat::record_message_and_persist(
                                    &mut history_store,
                                    &mut session_info,
                                    &mut message_count,
                                    None,
                                )?;

                                // Process with LLM
                                interrupted.store(false, Ordering::SeqCst);

                                let result = process_llm_response(
                                    &provider,
                                    &state.current_model.clone(),
                                    &mut conversation,
                                    &mut tool_executor,
                                    &settings,
                                    &context_manager,
                                    &mut state,
                                    config.stream_enabled,
                                    &interrupted,
                                    &mut terminal,
                                )
                                .await;

                                state.is_processing = false;
                                state.auto_scroll();

                                match result {
                                    Ok(true) => {
                                        crate::chat::record_message_and_persist(
                                            &mut history_store,
                                            &mut session_info,
                                            &mut message_count,
                                            None,
                                        )?;
                                        crate::chat::trim_conversation_if_needed(
                                            provider.as_ref(),
                                            state.current_model.as_str(),
                                            &mut conversation,
                                        );
                                    }
                                    Ok(false) => state.set_status("Interrupted"),
                                    Err(e) => {
                                        state.set_error(&format!("Error: {}", e));
                                    }
                                }
                            }

                            continue;
                        }

                        // If currently processing, queue the message for later
                        if state.is_processing {
                            state.pending_messages.push(input_text);
                            state.set_status(&format!(
                                "Message queued ({} pending)",
                                state.pending_messages.len()
                            ));
                            continue;
                        }

                        // Add user message to UI
                        state
                            .messages
                            .push(DisplayMessage::user(input_text.clone()));
                        state.auto_scroll();

                        // Mark as processing and re-render immediately to show the sent message
                        state.is_processing = true;
                        state.set_status("Sending...");
                        terminal
                            .draw(|f| draw_tui(f, &state))
                            .map_err(|e| TedError::Tui(e.to_string()))?;

                        // Store in context
                        context_manager
                            .store_message("user", &input_text, None)
                            .await?;

                        // Add to conversation
                        conversation.push(Message::user(&input_text));

                        // Update history
                        crate::chat::record_message_and_persist(
                            &mut history_store,
                            &mut session_info,
                            &mut message_count,
                            Some(&input_text),
                        )?;

                        // Process with LLM
                        interrupted.store(false, Ordering::SeqCst);

                        // Process response (use current model from state, which can be changed via /model)
                        let result = process_llm_response(
                            &provider,
                            &state.current_model.clone(),
                            &mut conversation,
                            &mut tool_executor,
                            &settings,
                            &context_manager,
                            &mut state,
                            config.stream_enabled,
                            &interrupted,
                            &mut terminal,
                        )
                        .await;

                        state.is_processing = false;
                        state.auto_scroll(); // Scroll to bottom after processing

                        match result {
                            Ok(true) => {
                                crate::chat::record_message_and_persist(
                                    &mut history_store,
                                    &mut session_info,
                                    &mut message_count,
                                    None,
                                )?;
                                crate::chat::trim_conversation_if_needed(
                                    provider.as_ref(),
                                    state.current_model.as_str(),
                                    &mut conversation,
                                );
                            }
                            Ok(false) => state.set_status("Interrupted"),
                            Err(e) => {
                                state.set_error(&format!("Error: {}", e));
                            }
                        }

                        // Process any queued messages
                        while !state.pending_messages.is_empty()
                            && !interrupted.load(Ordering::SeqCst)
                        {
                            let queued_text = state.pending_messages.remove(0);

                            // Add user message to UI
                            state
                                .messages
                                .push(DisplayMessage::user(queued_text.clone()));
                            state.auto_scroll();

                            // Mark as processing
                            state.is_processing = true;
                            let pending_count = state.pending_messages.len();
                            if pending_count > 0 {
                                state.set_status(&format!(
                                    "Processing queued... ({} remaining)",
                                    pending_count
                                ));
                            } else {
                                state.set_status("Processing queued message...");
                            }
                            terminal
                                .draw(|f| draw_tui(f, &state))
                                .map_err(|e| TedError::Tui(e.to_string()))?;

                            // Store in context
                            context_manager
                                .store_message("user", &queued_text, None)
                                .await?;

                            // Add to conversation
                            conversation.push(Message::user(&queued_text));

                            // Update history
                            crate::chat::record_message_and_persist(
                                &mut history_store,
                                &mut session_info,
                                &mut message_count,
                                None,
                            )?;

                            // Process response
                            let result = process_llm_response(
                                &provider,
                                &state.current_model.clone(),
                                &mut conversation,
                                &mut tool_executor,
                                &settings,
                                &context_manager,
                                &mut state,
                                config.stream_enabled,
                                &interrupted,
                                &mut terminal,
                            )
                            .await;

                            state.is_processing = false;
                            state.auto_scroll();

                            match result {
                                Ok(true) => {
                                    crate::chat::record_message_and_persist(
                                        &mut history_store,
                                        &mut session_info,
                                        &mut message_count,
                                        None,
                                    )?;
                                    crate::chat::trim_conversation_if_needed(
                                        provider.as_ref(),
                                        state.current_model.as_str(),
                                        &mut conversation,
                                    );
                                }
                                Ok(false) => {
                                    state.set_status("Interrupted");
                                    break;
                                }
                                Err(e) => {
                                    state.set_error(&format!("Error: {}", e));
                                    break; // Stop processing queue on error
                                }
                            }
                        }

                        continue;
                    }

                    // Handle other keys based on mode
                    handle_key(&mut state, key)?;
                }
                TermEvent::Resize(_, _) => {
                    // Terminal resized, will re-render automatically
                }
                _ => {}
            }
        }

        // Regenerate system prompt if caps changed
        if state.caps_changed {
            state.caps_changed = false;
            let loader = crate::caps::CapLoader::new();
            let resolver = crate::caps::CapResolver::new(loader);
            if let Ok(merged) = resolver.resolve_and_merge(&state.enabled_caps) {
                conversation.set_system(&merged.system_prompt);
                state.set_status("Caps updated - system prompt regenerated");
            }
        }

        if state.should_quit {
            break (Ok(()), state.needs_restart);
        }
    };

    let (result, needs_restart) = result;

    // Restore terminal (and reset panic hook)
    let _ = std::panic::take_hook(); // Remove our custom panic hook

    disable_raw_mode().map_err(|e| TedError::Tui(e.to_string()))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|e| TedError::Tui(e.to_string()))?;
    terminal
        .show_cursor()
        .map_err(|e| TedError::Tui(e.to_string()))?;

    // If provider changed, restart ted
    if needs_restart {
        println!("\nRestarting with new provider...\n");

        // Get the current executable path
        let exe = std::env::current_exe().map_err(|e| TedError::Tui(e.to_string()))?;

        // Restart by exec'ing ourselves
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = std::process::Command::new(&exe).exec();
            return Err(TedError::Tui(format!("Failed to restart: {}", err)));
        }

        #[cfg(not(unix))]
        {
            // On Windows, spawn a new process and exit
            let _ = std::process::Command::new(&exe).spawn();
            std::process::exit(0);
        }
    }

    println!("\nGoodbye!");

    result
}

#[cfg(test)]
mod tests;
