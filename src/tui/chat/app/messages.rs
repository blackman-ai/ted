// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use super::super::events::ChatEvent;
use super::super::state::{DisplayMessage, DisplayToolCall};
use super::{ChatApp, ChatMode};

impl ChatApp {
    pub(super) fn add_user_message(&mut self, text: String) {
        self.messages.push(DisplayMessage::user(text));
        self.scroll_state.invalidate_cache();
        let total_height = self
            .scroll_state
            .calculate_total_height(&self.messages, self.message_area_width);
        self.scroll_state.maybe_auto_scroll(total_height);
    }

    pub(super) fn start_assistant_message(&mut self) {
        self.messages
            .push(DisplayMessage::assistant_streaming(self.caps.clone()));
        self.is_processing = true;
        self.scroll_state.invalidate_cache();
        let total_height = self
            .scroll_state
            .calculate_total_height(&self.messages, self.message_area_width);
        self.scroll_state.maybe_auto_scroll(total_height);
    }

    pub(super) fn append_to_current_message(&mut self, text: &str) {
        if let Some(msg) = self.messages.last_mut() {
            if msg.is_streaming {
                msg.append_content(text);
                // Invalidate cache since message content changed.
                self.scroll_state.invalidate_cache();
                // Auto-scroll if enabled to follow the streaming content.
                if self.scroll_state.auto_scroll_enabled {
                    let total_height = self
                        .scroll_state
                        .calculate_total_height(&self.messages, self.message_area_width);
                    self.scroll_state.maybe_auto_scroll(total_height);
                }
            }
        }
    }

    pub(super) fn finish_current_message(&mut self) {
        if let Some(msg) = self.messages.last_mut() {
            msg.finish_streaming();
        }
        self.is_processing = false;
    }

    pub(super) fn add_tool_call(&mut self, id: &str, name: &str, input: serde_json::Value) {
        if let Some(msg) = self.messages.last_mut() {
            msg.add_tool_call(DisplayToolCall::new(
                id.to_string(),
                name.to_string(),
                input,
            ));
        }
    }

    pub(super) fn complete_tool_call(&mut self, id: &str, result: crate::tools::ToolResult) {
        if let Some(msg) = self.messages.last_mut() {
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
    }

    pub(super) fn submit_message(&mut self, text: String) {
        // Check for commands.
        let trimmed = text.trim();
        if trimmed.starts_with('/') {
            self.handle_command(trimmed);
            return;
        }

        // Regular message.
        let _ = self.event_tx.send(ChatEvent::UserMessage(text));
    }

    pub(super) fn handle_command(&mut self, command: &str) {
        match command {
            "/help" => {
                self.mode = ChatMode::Help;
            }
            "/quit" | "/exit" => {
                self.should_quit = true;
            }
            "/clear" => {
                self.messages.clear();
                self.set_status("Chat cleared");
            }
            "/agents" => {
                self.toggle_agent_pane();
            }
            _ => {
                self.set_error(&format!("Unknown command: {}", command));
            }
        }
    }

    pub(super) fn toggle_agent_pane(&mut self) {
        self.agent_pane_visible = !self.agent_pane_visible;
        // Scroll viewport is recalculated during render.
    }

    pub(super) fn set_status(&mut self, msg: &str) {
        self.status_message = Some(msg.to_string());
        self.status_is_error = false;
    }

    pub(super) fn set_error(&mut self, msg: &str) {
        self.status_message = Some(msg.to_string());
        self.status_is_error = true;
    }

    pub(super) fn show_confirm<F>(&mut self, message: &str, callback: F)
    where
        F: FnOnce(&mut ChatApp) + Send + 'static,
    {
        self.confirm_message = Some(message.to_string());
        self.confirm_callback = Some(Box::new(callback));
        self.mode = ChatMode::Confirm;
    }
}
