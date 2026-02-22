// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use uuid::Uuid;

use crate::error::Result;

use super::super::events::ChatEvent;
use super::{ChatApp, ChatMode};

impl ChatApp {
    /// Handle a chat event.
    pub(super) async fn handle_event(&mut self, event: ChatEvent) -> Result<()> {
        match event {
            ChatEvent::UserMessage(text) => {
                self.add_user_message(text);
            }

            ChatEvent::StreamStart => {
                self.start_assistant_message();
            }

            ChatEvent::StreamDelta(text) => {
                self.append_to_current_message(&text);
            }

            ChatEvent::StreamEnd => {
                self.finish_current_message();
            }

            ChatEvent::ToolCallStart { id, name, input } => {
                self.add_tool_call(&id, &name, input);
            }

            ChatEvent::ToolCallEnd {
                id,
                name: _,
                result,
            } => {
                self.complete_tool_call(&id, result);
            }

            ChatEvent::AgentSpawned {
                id,
                name,
                agent_type,
                task,
            } => {
                self.agents
                    .track(id, id.to_string(), name, agent_type, task);
                // Auto-expand agent pane when agents are spawned.
                if !self.agent_pane_expanded && self.agents.total_count() > 0 {
                    self.agent_pane_expanded = true;
                    self.agent_pane_height = (self.agents.total_count() as u16 + 2).min(10);
                }
            }

            ChatEvent::AgentProgress {
                id,
                iteration,
                max_iterations,
                action,
            } => {
                self.agents
                    .update_progress(&id, iteration, max_iterations, &action);
            }

            ChatEvent::AgentRateLimited { id, wait_secs, .. } => {
                self.agents.set_rate_limited(&id, wait_secs);
            }

            ChatEvent::AgentToolStart { id, tool_name } => {
                self.agents.set_current_tool(&id, Some(&tool_name));
            }

            ChatEvent::AgentToolEnd { id, .. } => {
                self.agents.set_current_tool(&id, None);
            }

            ChatEvent::AgentCompleted {
                id,
                files_changed,
                summary,
            } => {
                self.agents.set_completed(&id, files_changed, summary);
            }

            ChatEvent::AgentFailed { id, error } => {
                self.agents.set_failed(&id, &error);
            }

            ChatEvent::AgentCancelled { id } => {
                self.agents.set_cancelled(&id);
            }

            ChatEvent::Error(msg) => {
                self.set_error(&msg);
            }

            ChatEvent::Status(msg) => {
                self.set_status(&msg);
            }

            ChatEvent::SessionEnded => {
                self.should_quit = true;
            }

            ChatEvent::Refresh => {
                // Just triggers a redraw.
            }
        }

        Ok(())
    }

    /// Handle a keyboard event.
    pub(super) fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Result<()> {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Global keys that work in any mode.
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                if self.is_processing {
                    self.set_error(
                        "Cancellation of active requests is not wired yet. Wait for completion or quit.",
                    );
                } else if self.agents.has_active() {
                    // Show confirmation to cancel agents.
                    self.show_confirm("Cancel all running agents?", |app| {
                        let active_ids: Vec<Uuid> = app
                            .agents
                            .active()
                            .into_iter()
                            .map(|agent| agent.id)
                            .collect();

                        for id in &active_ids {
                            app.agents.set_cancelled(id);
                        }

                        app.set_status(&format!(
                            "Marked {} running agent(s) as cancelled",
                            active_ids.len()
                        ));
                    });
                } else {
                    self.should_quit = true;
                }
                return Ok(());
            }
            (KeyModifiers::CONTROL, KeyCode::Char('l')) => {
                // Clear and redraw.
                return Ok(());
            }
            _ => {}
        }

        // Mode-specific handling.
        match self.mode {
            ChatMode::Input => self.handle_input_key(key)?,
            ChatMode::Normal => self.handle_normal_key(key)?,
            ChatMode::AgentFocus => self.handle_agent_focus_key(key)?,
            ChatMode::Help => self.handle_help_key(key)?,
            ChatMode::CommandPalette => self.handle_command_palette_key(key)?,
            ChatMode::Confirm => self.handle_confirm_key(key)?,
            ChatMode::Settings => {} // Handled by runner.rs.
        }

        Ok(())
    }

    /// Handle keys in input mode.
    pub(super) fn handle_input_key(&mut self, key: crossterm::event::KeyEvent) -> Result<()> {
        use crossterm::event::{KeyCode, KeyModifiers};

        match (key.modifiers, key.code) {
            // Submit
            (KeyModifiers::NONE, KeyCode::Enter) => {
                if !self.input.is_empty() {
                    let text = self.input.submit();
                    self.submit_message(text);
                }
            }
            // Escape to normal mode
            (KeyModifiers::NONE, KeyCode::Esc) => {
                self.mode = ChatMode::Normal;
            }
            // History navigation
            (KeyModifiers::NONE, KeyCode::Up) => {
                self.input.history_prev();
            }
            (KeyModifiers::NONE, KeyCode::Down) => {
                self.input.history_next();
            }
            // Cursor movement
            (KeyModifiers::NONE, KeyCode::Left) => {
                self.input.move_left();
            }
            (KeyModifiers::NONE, KeyCode::Right) => {
                self.input.move_right();
            }
            (KeyModifiers::NONE, KeyCode::Home) | (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
                self.input.move_home();
            }
            (KeyModifiers::NONE, KeyCode::End) | (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
                self.input.move_end();
            }
            // Deletion
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                self.input.backspace();
            }
            (KeyModifiers::NONE, KeyCode::Delete) => {
                self.input.delete();
            }
            (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
                self.input.delete_word();
            }
            (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                self.input.clear();
            }
            // Tab for agent pane toggle
            (KeyModifiers::NONE, KeyCode::Tab) => {
                self.toggle_agent_pane();
            }
            // Character input
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                self.input.insert_char(c);
            }
            _ => {}
        }

        Ok(())
    }

    /// Handle keys in normal mode (scrolling).
    pub(super) fn handle_normal_key(&mut self, key: crossterm::event::KeyEvent) -> Result<()> {
        use crossterm::event::{KeyCode, KeyModifiers};

        match (key.modifiers, key.code) {
            // Enter input mode
            (KeyModifiers::NONE, KeyCode::Enter) | (KeyModifiers::NONE, KeyCode::Char('i')) => {
                self.mode = ChatMode::Input;
            }
            // Scrolling
            (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
                self.scroll_state.scroll_up(1);
            }
            (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
                let total_height = self
                    .scroll_state
                    .calculate_total_height(&self.messages, self.message_area_width);
                self.scroll_state.scroll_down(1, total_height);
            }
            (KeyModifiers::NONE, KeyCode::PageUp) => {
                self.scroll_state.page_up();
            }
            (KeyModifiers::NONE, KeyCode::PageDown) => {
                let total_height = self
                    .scroll_state
                    .calculate_total_height(&self.messages, self.message_area_width);
                self.scroll_state.page_down(total_height);
            }
            (KeyModifiers::NONE, KeyCode::Char('g')) => {
                self.scroll_state.scroll_to_top();
            }
            (KeyModifiers::SHIFT, KeyCode::Char('G')) => {
                let total_height = self
                    .scroll_state
                    .calculate_total_height(&self.messages, self.message_area_width);
                self.scroll_state.scroll_to_bottom(total_height);
            }
            // Toggle agent pane
            (KeyModifiers::NONE, KeyCode::Tab) => {
                self.toggle_agent_pane();
            }
            // Focus agent pane
            (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
                if self.agents.total_count() > 0 {
                    self.mode = ChatMode::AgentFocus;
                    self.agent_pane_visible = true;
                }
            }
            // Help
            (KeyModifiers::NONE, KeyCode::Char('?')) => {
                self.mode = ChatMode::Help;
            }
            // Quit
            (KeyModifiers::NONE, KeyCode::Char('q')) => {
                self.should_quit = true;
            }
            _ => {}
        }

        Ok(())
    }

    /// Handle keys in agent focus mode.
    pub(super) fn handle_agent_focus_key(&mut self, key: crossterm::event::KeyEvent) -> Result<()> {
        use crossterm::event::{KeyCode, KeyModifiers};

        match (key.modifiers, key.code) {
            // Navigation
            (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
                if self.selected_agent_index > 0 {
                    self.selected_agent_index -= 1;
                }
            }
            (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
                if self.selected_agent_index < self.agents.total_count().saturating_sub(1) {
                    self.selected_agent_index += 1;
                }
            }
            // Expand/collapse
            (KeyModifiers::NONE, KeyCode::Enter) => {
                self.agent_pane_expanded = !self.agent_pane_expanded;
            }
            // Cancel selected agent
            (KeyModifiers::NONE, KeyCode::Char('c')) => {
                let selected_id = self
                    .agents
                    .all()
                    .get(self.selected_agent_index)
                    .map(|agent| agent.id);

                if let Some(id) = selected_id {
                    self.agents.set_cancelled(&id);
                    self.set_status("Marked selected agent as cancelled");
                }
            }
            // Exit agent focus
            (KeyModifiers::NONE, KeyCode::Esc) | (KeyModifiers::NONE, KeyCode::Tab) => {
                self.mode = ChatMode::Input;
            }
            _ => {}
        }

        Ok(())
    }

    /// Handle keys in help mode.
    pub(super) fn handle_help_key(&mut self, key: crossterm::event::KeyEvent) -> Result<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                self.mode = ChatMode::Input;
            }
            _ => {}
        }

        Ok(())
    }

    /// Handle keys in command palette mode.
    pub(super) fn handle_command_palette_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Result<()> {
        use crossterm::event::KeyCode;

        if key.code == KeyCode::Esc {
            self.mode = ChatMode::Input;
        }

        Ok(())
    }

    /// Handle keys in confirm mode.
    pub(super) fn handle_confirm_key(&mut self, key: crossterm::event::KeyEvent) -> Result<()> {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                if let Some(callback) = self.confirm_callback.take() {
                    callback(self);
                }
                self.confirm_message = None;
                self.mode = ChatMode::Input;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.confirm_message = None;
                self.confirm_callback = None;
                self.mode = ChatMode::Input;
            }
            _ => {}
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::context::ContextManager;
    use crate::llm::mock_provider::MockProvider;
    use crate::llm::provider::LlmProvider;
    use crate::tools::{ToolContext, ToolExecutor, ToolResult};
    use crate::tui::chat::{AgentStatus, ChatTuiConfig};
    use std::sync::Arc;
    use uuid::Uuid;

    async fn make_app() -> ChatApp {
        let storage_path =
            std::env::temp_dir().join(format!("ted-controller-tests-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&storage_path).expect("temp storage dir should be creatable");

        let config = ChatTuiConfig {
            session_id: Uuid::new_v4(),
            provider_name: "mock".to_string(),
            model: "test-model".to_string(),
            caps: vec!["base".to_string()],
            trust_mode: false,
            stream_enabled: true,
        };

        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::new());
        let tool_context = ToolContext::new(
            storage_path.clone(),
            Some(storage_path.clone()),
            config.session_id,
            false,
        );
        let tool_executor = ToolExecutor::new(tool_context, false);
        let context_manager = ContextManager::new_session(storage_path).await.unwrap();

        ChatApp::new(
            config,
            event_tx,
            event_rx,
            provider,
            tool_executor,
            context_manager,
            Settings::default(),
        )
    }

    #[tokio::test]
    async fn test_handle_event_core_variants() {
        let mut app = make_app().await;
        let agent_id = Uuid::new_v4();

        app.handle_event(ChatEvent::UserMessage("hello".to_string()))
            .await
            .unwrap();
        assert_eq!(app.messages.len(), 1);

        app.handle_event(ChatEvent::StreamStart).await.unwrap();
        app.handle_event(ChatEvent::StreamDelta("world".to_string()))
            .await
            .unwrap();
        app.handle_event(ChatEvent::ToolCallStart {
            id: "tool-1".to_string(),
            name: "shell".to_string(),
            input: serde_json::json!({"command":"echo hi"}),
        })
        .await
        .unwrap();
        app.handle_event(ChatEvent::ToolCallEnd {
            id: "tool-1".to_string(),
            name: "shell".to_string(),
            result: ToolResult::success("tool-1", "ok"),
        })
        .await
        .unwrap();
        app.handle_event(ChatEvent::StreamEnd).await.unwrap();
        assert!(!app.is_processing);

        app.handle_event(ChatEvent::AgentSpawned {
            id: agent_id,
            name: "reviewer".to_string(),
            agent_type: "review".to_string(),
            task: "review code".to_string(),
        })
        .await
        .unwrap();
        assert!(app.agent_pane_expanded);
        assert!(app.agents.total_count() > 0);

        app.handle_event(ChatEvent::AgentProgress {
            id: agent_id,
            iteration: 2,
            max_iterations: 5,
            action: "checking".to_string(),
        })
        .await
        .unwrap();
        app.handle_event(ChatEvent::AgentRateLimited {
            id: agent_id,
            wait_secs: 1.5,
            tokens_needed: 1000,
        })
        .await
        .unwrap();
        app.handle_event(ChatEvent::AgentToolStart {
            id: agent_id,
            tool_name: "grep".to_string(),
        })
        .await
        .unwrap();
        app.handle_event(ChatEvent::AgentToolEnd {
            id: agent_id,
            tool_name: "grep".to_string(),
            success: true,
        })
        .await
        .unwrap();
        app.handle_event(ChatEvent::AgentCompleted {
            id: agent_id,
            files_changed: vec!["src/main.rs".to_string()],
            summary: Some("done".to_string()),
        })
        .await
        .unwrap();
        app.handle_event(ChatEvent::AgentFailed {
            id: agent_id,
            error: "failed".to_string(),
        })
        .await
        .unwrap();
        app.handle_event(ChatEvent::AgentCancelled { id: agent_id })
            .await
            .unwrap();

        app.handle_event(ChatEvent::Error("boom".to_string()))
            .await
            .unwrap();
        assert!(app.status_is_error);
        app.handle_event(ChatEvent::Status("ok".to_string()))
            .await
            .unwrap();
        assert!(!app.status_is_error);

        app.handle_event(ChatEvent::Refresh).await.unwrap();
        app.handle_event(ChatEvent::SessionEnded).await.unwrap();
        assert!(app.should_quit);
    }

    #[tokio::test]
    async fn test_handle_key_ctrl_c_paths() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.is_processing = true;
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
            .unwrap();
        assert!(app.status_is_error);
        assert!(app
            .status_message
            .as_deref()
            .unwrap_or_default()
            .contains("Cancellation"));

        app.is_processing = false;
        let id = Uuid::new_v4();
        app.agents.track(
            id,
            "tool-cancel".to_string(),
            "agent".to_string(),
            "review".to_string(),
            "task".to_string(),
        );
        app.agents.set_running(&id);
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Confirm);
        assert!(app.confirm_message.is_some());

        app.handle_confirm_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Input);
        assert!(app.confirm_message.is_none());

        let mut app = make_app().await;
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
            .unwrap();
        assert!(app.should_quit);
    }

    #[tokio::test]
    async fn test_handle_input_key_and_mode_transitions() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.mode = ChatMode::Input;
        app.input.insert_char('/');
        app.input.insert_char('h');
        app.input.insert_char('e');
        app.input.insert_char('l');
        app.input.insert_char('p');

        app.handle_input_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Help);

        app.mode = ChatMode::Input;
        app.handle_input_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Normal);

        app.mode = ChatMode::Input;
        app.input.insert_char('x');
        app.handle_input_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
            .unwrap();
        app.handle_input_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE))
            .unwrap();
        app.handle_input_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL))
            .unwrap();
        assert!(app.input.is_empty());
    }

    #[tokio::test]
    async fn test_handle_normal_and_agent_focus_keys() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.mode = ChatMode::Normal;
        app.handle_normal_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Help);

        app.mode = ChatMode::Normal;
        let id = Uuid::new_v4();
        app.agents.track(
            id,
            "tool-focus".to_string(),
            "agent".to_string(),
            "explore".to_string(),
            "task".to_string(),
        );
        app.handle_normal_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(app.mode, ChatMode::AgentFocus);

        app.handle_agent_focus_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
            .unwrap();
        app.handle_agent_focus_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert!(app.agent_pane_expanded);

        app.selected_agent_index = 0;
        app.handle_agent_focus_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE))
            .unwrap();
        let tracked = app.agents.get(&id).unwrap();
        assert!(matches!(tracked.status, AgentStatus::Cancelled));

        app.handle_agent_focus_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Input);
    }

    #[tokio::test]
    async fn test_help_palette_and_confirm_negative_paths() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.mode = ChatMode::Help;
        app.handle_help_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Input);

        app.mode = ChatMode::CommandPalette;
        app.handle_command_palette_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Input);

        app.show_confirm("Are you sure?", |_| {});
        app.handle_confirm_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Input);
        assert!(app.confirm_message.is_none());
    }

    #[tokio::test]
    async fn test_handle_key_dispatches_all_modes_and_ctrl_l() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;

        // Global Ctrl+L should always return Ok and keep app responsive.
        app.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL))
            .unwrap();

        // Input mode dispatch.
        app.mode = ChatMode::Input;
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Normal);

        // Normal mode dispatch.
        app.mode = ChatMode::Normal;
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Help);

        // Help mode dispatch (non-exit key should stay in Help).
        app.mode = ChatMode::Help;
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Help);

        // Command palette dispatch.
        app.mode = ChatMode::CommandPalette;
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Input);

        // Confirm mode dispatch.
        app.show_confirm("confirm?", |_| {});
        app.mode = ChatMode::Confirm;
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Confirm);
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Input);

        // Agent focus mode dispatch.
        let id = Uuid::new_v4();
        app.agents.track(
            id,
            "tool-focus-dispatch".to_string(),
            "agent".to_string(),
            "explore".to_string(),
            "task".to_string(),
        );
        app.mode = ChatMode::AgentFocus;
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Input);

        // Settings mode dispatch should be a no-op in controller.
        app.mode = ChatMode::Settings;
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Settings);
    }

    #[tokio::test]
    async fn test_handle_input_key_navigation_and_editing_paths() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.mode = ChatMode::Input;
        app.input.history = vec!["one".to_string(), "two".to_string()];
        app.input.set_buffer("abc def".to_string());
        app.input.cursor = app.input.text().chars().count();

        app.handle_input_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.input.text(), "two");
        app.handle_input_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
            .unwrap();

        app.input.set_buffer("abc def".to_string());
        app.input.cursor = app.input.text().chars().count();

        app.handle_input_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
            .unwrap();
        app.handle_input_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
            .unwrap();
        app.handle_input_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.input.cursor, 0);
        app.handle_input_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.input.cursor, app.input.text().chars().count());
        app.handle_input_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(app.input.cursor, 0);
        app.handle_input_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(app.input.cursor, app.input.text().chars().count());

        app.handle_input_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
            .unwrap();
        app.input.cursor = 0;
        app.handle_input_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE))
            .unwrap();
        app.handle_input_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL))
            .unwrap();
        app.handle_input_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL))
            .unwrap();
        assert!(app.input.is_empty());

        let pane_before = app.agent_pane_visible;
        app.handle_input_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_ne!(app.agent_pane_visible, pane_before);

        // Default branch (no-op).
        app.handle_input_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE))
            .unwrap();
    }

    #[tokio::test]
    async fn test_handle_normal_key_scroll_quit_and_default_paths() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        app.mode = ChatMode::Normal;
        app.add_user_message("line one".to_string());
        app.add_user_message("line two".to_string());

        app.handle_normal_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
            .unwrap();
        app.handle_normal_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE))
            .unwrap();
        app.handle_normal_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
            .unwrap();
        app.handle_normal_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE))
            .unwrap();
        app.handle_normal_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE))
            .unwrap();
        app.handle_normal_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
            .unwrap();
        app.handle_normal_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE))
            .unwrap();
        app.handle_normal_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT))
            .unwrap();

        let pane_before = app.agent_pane_visible;
        app.handle_normal_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_ne!(app.agent_pane_visible, pane_before);

        app.should_quit = false;
        app.handle_normal_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
            .unwrap();
        assert!(app.should_quit);

        // Enter/i both return to input mode.
        app.mode = ChatMode::Normal;
        app.handle_normal_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Input);
        app.mode = ChatMode::Normal;
        app.handle_normal_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Input);

        // Default branch (no-op).
        app.mode = ChatMode::Normal;
        app.handle_normal_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Normal);
    }

    #[tokio::test]
    async fn test_handle_agent_focus_and_misc_default_paths() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut app = make_app().await;
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        app.agents.track(
            first,
            "tool-first".to_string(),
            "agent-a".to_string(),
            "explore".to_string(),
            "task-a".to_string(),
        );
        app.agents.track(
            second,
            "tool-second".to_string(),
            "agent-b".to_string(),
            "review".to_string(),
            "task-b".to_string(),
        );

        app.mode = ChatMode::AgentFocus;
        app.selected_agent_index = 1;
        app.handle_agent_focus_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.selected_agent_index, 0);
        app.handle_agent_focus_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.selected_agent_index, 1);

        // Default branch for agent focus.
        app.handle_agent_focus_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::AgentFocus);

        // Help mode default branch.
        app.mode = ChatMode::Help;
        app.handle_help_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Help);

        // Confirm mode default branch.
        app.show_confirm("confirm?", |_| {});
        app.mode = ChatMode::Confirm;
        app.handle_confirm_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(app.mode, ChatMode::Confirm);
    }
}
