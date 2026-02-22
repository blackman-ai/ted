// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Chat application state machine
//!
//! The main state container for the chat TUI, handling mode transitions,
//! events, and coordination between components.

use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::config::Settings;
use crate::context::ContextManager;
use crate::error::Result;
use crate::llm::provider::LlmProvider;
use crate::tools::ToolExecutor;

use super::events::{EventReceiver, EventSender};
use super::state::{AgentTracker, DisplayMessage, InputState, ScrollState};
use super::ChatTuiConfig;

mod controller;
mod messages;

/// Type alias for confirmation dialog callback
pub type ConfirmCallback = Box<dyn FnOnce(&mut ChatApp) + Send>;

/// Current mode of the chat UI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMode {
    /// Viewing chat, can scroll
    Normal,
    /// Typing in input area
    Input,
    /// Navigating agent list
    AgentFocus,
    /// Showing help overlay
    Help,
    /// Showing command palette
    CommandPalette,
    /// Confirmation dialog
    Confirm,
    /// Editing settings
    Settings,
}

/// Result of a tick (event loop iteration)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickResult {
    /// Continue running
    Continue,
    /// User wants to quit
    Quit,
    /// Restart needed (e.g., settings changed)
    Restart,
}

/// Main application state for the chat TUI
pub struct ChatApp {
    // === Configuration ===
    pub session_id: Uuid,
    pub provider_name: String,
    pub model: String,
    pub caps: Vec<String>,
    pub trust_mode: bool,
    pub stream_enabled: bool,

    // === UI State ===
    pub mode: ChatMode,
    pub scroll_state: ScrollState,
    pub agent_pane_visible: bool,
    pub agent_pane_height: u16,
    pub agent_pane_expanded: bool,
    pub selected_agent_index: usize,
    pub message_area_width: u16,

    // === Content ===
    pub messages: Vec<DisplayMessage>,
    pub agents: AgentTracker,
    pub input: InputState,

    // === Status ===
    pub status_message: Option<String>,
    pub status_is_error: bool,
    pub is_processing: bool,
    pub should_quit: bool,
    pub should_restart: bool,

    // === Confirmation dialog ===
    pub confirm_message: Option<String>,
    pub confirm_callback: Option<ConfirmCallback>,

    // === Resources ===
    pub settings: Settings,
    event_tx: EventSender,
    event_rx: EventReceiver,
    provider: Arc<dyn LlmProvider>,
    tool_executor: ToolExecutor,
    context_manager: ContextManager,
}

impl ChatApp {
    /// Create a new chat application
    pub fn new(
        config: ChatTuiConfig,
        event_tx: EventSender,
        event_rx: EventReceiver,
        provider: Arc<dyn LlmProvider>,
        tool_executor: ToolExecutor,
        context_manager: ContextManager,
        settings: Settings,
    ) -> Self {
        Self {
            session_id: config.session_id,
            provider_name: config.provider_name,
            model: config.model,
            // Filter out "base" from display - it's always applied silently
            caps: config.caps.into_iter().filter(|c| c != "base").collect(),
            trust_mode: config.trust_mode,
            stream_enabled: config.stream_enabled,

            mode: ChatMode::Input,
            scroll_state: ScrollState::new(),
            agent_pane_visible: true,
            agent_pane_height: 4,
            agent_pane_expanded: false,
            selected_agent_index: 0,
            message_area_width: 80,

            messages: Vec::new(),
            agents: AgentTracker::new(),
            input: InputState::new(),

            status_message: None,
            status_is_error: false,
            is_processing: false,
            should_quit: false,
            should_restart: false,

            confirm_message: None,
            confirm_callback: None,

            settings,
            event_tx,
            event_rx,
            provider,
            tool_executor,
            context_manager,
        }
    }

    /// Get the event sender for passing to async tasks
    pub fn event_sender(&self) -> EventSender {
        self.event_tx.clone()
    }

    /// Process one tick of the event loop
    pub async fn tick(&mut self) -> Result<TickResult> {
        // Check for quit/restart
        if self.should_quit {
            return Ok(TickResult::Quit);
        }
        if self.should_restart {
            return Ok(TickResult::Restart);
        }

        // Handle events with timeout for smooth UI updates
        tokio::select! {
            Some(event) = self.event_rx.recv() => {
                self.handle_event(event).await?;
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                // Tick for animations/updates
            }
        }

        // Check keyboard input (non-blocking)
        if crossterm::event::poll(Duration::from_millis(0))? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                self.handle_key(key)?;
            }
        }

        Ok(TickResult::Continue)
    }

    /// Update message area width for accurate wrapped-height calculations.
    pub fn update_message_area_width(&mut self, width: u16) {
        self.message_area_width = width.max(1);
    }

    // === Accessors for UI rendering ===

    pub fn provider(&self) -> &dyn LlmProvider {
        self.provider.as_ref()
    }

    pub fn tool_executor(&self) -> &ToolExecutor {
        &self.tool_executor
    }

    pub fn context_manager(&self) -> &ContextManager {
        &self.context_manager
    }
}

#[cfg(test)]
mod tests;
