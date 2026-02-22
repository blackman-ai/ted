// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::Settings;
use crate::error::Result;

use super::super::app::ChatMode;
use super::{SettingsField, SettingsSection, TuiState};

pub(super) fn handle_key(state: &mut TuiState, key: KeyEvent) -> Result<()> {
    match state.mode {
        ChatMode::Input => handle_input_key(state, key),
        ChatMode::Normal => handle_normal_key(state, key),
        ChatMode::Help => handle_help_key(state, key),
        ChatMode::Settings => handle_settings_key(state, key),
        ChatMode::AgentFocus => handle_agent_focus_key(state, key),
        _ => Ok(()),
    }
}

pub(super) fn handle_input_key(state: &mut TuiState, key: KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Esc) => {
            state.mode = ChatMode::Normal;
        }
        (KeyModifiers::NONE, KeyCode::Up) => {
            state.input.history_prev();
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            state.input.history_next();
        }
        (KeyModifiers::NONE, KeyCode::Left) => {
            state.input.move_left();
        }
        (KeyModifiers::NONE, KeyCode::Right) => {
            state.input.move_right();
        }
        (KeyModifiers::NONE, KeyCode::Home) | (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
            state.input.move_home();
        }
        (KeyModifiers::NONE, KeyCode::End) | (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
            state.input.move_end();
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            state.input.backspace();
        }
        (KeyModifiers::NONE, KeyCode::Delete) => {
            state.input.delete();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
            state.input.delete_word();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
            state.input.clear();
        }
        (KeyModifiers::NONE, KeyCode::Tab) => {
            state.agent_pane_visible = !state.agent_pane_visible;
        }
        // Page Up/Down for scrolling chat while in input mode
        (KeyModifiers::NONE, KeyCode::PageUp) => {
            let scroll_amount = (state.chat_area_height / 2).max(1) as usize;
            state.scroll_up(scroll_amount);
        }
        (KeyModifiers::NONE, KeyCode::PageDown) => {
            let scroll_amount = (state.chat_area_height / 2).max(1) as usize;
            state.scroll_down(scroll_amount);
        }
        // Ctrl+Up/Down for single-line scrolling
        (KeyModifiers::CONTROL, KeyCode::Up) => {
            state.scroll_up(1);
        }
        (KeyModifiers::CONTROL, KeyCode::Down) => {
            state.scroll_down(1);
        }
        // Ctrl+? or Ctrl+/ to show help from Input mode
        (KeyModifiers::CONTROL, KeyCode::Char('/')) => {
            state.mode = ChatMode::Help;
        }
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            state.input.insert_char(c);
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn handle_normal_key(state: &mut TuiState, key: KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        // Esc returns to Input mode (intuitive: Esc always goes back to typing)
        (KeyModifiers::NONE, KeyCode::Esc)
        | (KeyModifiers::NONE, KeyCode::Enter)
        | (KeyModifiers::NONE, KeyCode::Char('i')) => {
            state.mode = ChatMode::Input;
        }
        (KeyModifiers::NONE, KeyCode::Tab) => {
            state.agent_pane_visible = !state.agent_pane_visible;
        }
        (KeyModifiers::NONE, KeyCode::Char('?')) => {
            state.mode = ChatMode::Help;
        }
        (KeyModifiers::NONE, KeyCode::Char('q')) => {
            state.should_quit = true;
        }
        // Scrolling in normal mode - j/k or arrow keys
        (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
            state.scroll_up(1);
        }
        (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
            state.scroll_down(1);
        }
        (KeyModifiers::NONE, KeyCode::PageUp) => {
            let scroll_amount = (state.chat_area_height / 2).max(1) as usize;
            state.scroll_up(scroll_amount);
        }
        (KeyModifiers::NONE, KeyCode::PageDown) => {
            let scroll_amount = (state.chat_area_height / 2).max(1) as usize;
            state.scroll_down(scroll_amount);
        }
        // G to jump to bottom (end)
        (KeyModifiers::SHIFT, KeyCode::Char('G')) => {
            state.auto_scroll();
        }
        // gg to jump to top (we'll use g for simplicity)
        (KeyModifiers::NONE, KeyCode::Char('g')) => {
            state.scroll_offset = 0;
        }
        // 'a' to focus agent pane (if any agents exist)
        (KeyModifiers::NONE, KeyCode::Char('a')) => {
            if state.agents.total_count() > 0 && state.focused_agent_tool_id.is_some() {
                state.mode = ChatMode::AgentFocus;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn handle_agent_focus_key(state: &mut TuiState, key: KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        // Exit agent focus
        (KeyModifiers::NONE, KeyCode::Esc) | (KeyModifiers::NONE, KeyCode::Char('q')) => {
            state.mode = ChatMode::Input;
        }
        (KeyModifiers::NONE, KeyCode::Tab) => {
            state.mode = ChatMode::Input;
        }
        // Scroll agent conversation up
        (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
            if let Some(agent) = state
                .focused_agent_tool_id
                .as_ref()
                .and_then(|id| state.agents.get_mut_by_tool_call_id(id))
            {
                agent.conversation_scroll.scroll_offset =
                    agent.conversation_scroll.scroll_offset.saturating_add(1);
            }
        }
        // Scroll agent conversation down
        (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
            if let Some(agent) = state
                .focused_agent_tool_id
                .as_ref()
                .and_then(|id| state.agents.get_mut_by_tool_call_id(id))
            {
                agent.conversation_scroll.scroll_offset =
                    agent.conversation_scroll.scroll_offset.saturating_sub(1);
            }
        }
        // Page scroll
        (KeyModifiers::NONE, KeyCode::PageUp) => {
            if let Some(agent) = state
                .focused_agent_tool_id
                .as_ref()
                .and_then(|id| state.agents.get_mut_by_tool_call_id(id))
            {
                agent.conversation_scroll.scroll_offset =
                    agent.conversation_scroll.scroll_offset.saturating_add(10);
            }
        }
        (KeyModifiers::NONE, KeyCode::PageDown) => {
            if let Some(agent) = state
                .focused_agent_tool_id
                .as_ref()
                .and_then(|id| state.agents.get_mut_by_tool_call_id(id))
            {
                agent.conversation_scroll.scroll_offset =
                    agent.conversation_scroll.scroll_offset.saturating_sub(10);
            }
        }
        // Jump to top
        (KeyModifiers::NONE, KeyCode::Char('g')) => {
            if let Some(agent) = state
                .focused_agent_tool_id
                .as_ref()
                .and_then(|id| state.agents.get_mut_by_tool_call_id(id))
            {
                agent.conversation_scroll.scroll_offset = 0;
            }
        }
        // Jump to bottom
        (KeyModifiers::SHIFT, KeyCode::Char('G')) => {
            if let Some(agent) = state
                .focused_agent_tool_id
                .as_ref()
                .and_then(|id| state.agents.get_mut_by_tool_call_id(id))
            {
                // Set to a large value; render will clamp it
                agent.conversation_scroll.scroll_offset = usize::MAX;
            }
        }
        // Switch between agents: [ and ] or h and l
        (KeyModifiers::NONE, KeyCode::Char('[')) | (KeyModifiers::NONE, KeyCode::Char('h')) => {
            cycle_focused_agent(state, -1);
        }
        (KeyModifiers::NONE, KeyCode::Char(']')) | (KeyModifiers::NONE, KeyCode::Char('l')) => {
            cycle_focused_agent(state, 1);
        }
        _ => {}
    }
    Ok(())
}

/// Cycle the focused agent forward or backward
pub(super) fn cycle_focused_agent(state: &mut TuiState, direction: i32) {
    let agents = state.agents.all();
    if agents.is_empty() {
        return;
    }
    let current_idx = state
        .focused_agent_tool_id
        .as_ref()
        .and_then(|id| agents.iter().position(|a| a.tool_call_id == *id))
        .unwrap_or(0);

    let new_idx = if direction > 0 {
        (current_idx + 1) % agents.len()
    } else if current_idx == 0 {
        agents.len() - 1
    } else {
        current_idx - 1
    };

    state.focused_agent_tool_id = Some(agents[new_idx].tool_call_id.clone());
}

pub(super) fn handle_help_key(state: &mut TuiState, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
            state.mode = ChatMode::Input;
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn handle_settings_key(state: &mut TuiState, key: KeyEvent) -> Result<()> {
    let settings = match &mut state.settings_state {
        Some(s) => s,
        None => {
            state.mode = ChatMode::Input;
            return Ok(());
        }
    };

    // Handle editing mode (only in General section)
    if settings.is_editing {
        match key.code {
            KeyCode::Esc => {
                settings.cancel_editing();
            }
            KeyCode::Enter => {
                settings.confirm_editing();
                // Update current_model if model was changed
                state.current_model = settings.model.clone();
            }
            KeyCode::Backspace => {
                settings.backspace();
            }
            KeyCode::Char(c) => {
                settings.insert_char(c);
            }
            _ => {}
        }
        return Ok(());
    }

    // Navigation mode - common keys
    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) | (_, KeyCode::Char('q')) => {
            state.mode = ChatMode::Input;
            return Ok(());
        }
        (_, KeyCode::Tab) => {
            settings.next_section();
            return Ok(());
        }
        (KeyModifiers::SHIFT, KeyCode::BackTab) => {
            settings.prev_section();
            return Ok(());
        }
        (KeyModifiers::SHIFT, KeyCode::Char('S')) | (KeyModifiers::NONE, KeyCode::Char('s')) => {
            // Save settings
            if settings.has_changes {
                let new_model = settings.model.clone();
                let new_stream = settings.stream;
                let new_trust = settings.trust_mode;
                let new_provider = settings.provider.clone();
                let new_temp = settings.temperature;
                let new_max_tokens = settings.max_tokens;
                let new_caps = settings.caps_enabled.clone();
                // Save current API key to the map before saving
                settings
                    .api_keys_by_provider
                    .insert(settings.provider.clone(), settings.api_key.clone());
                let api_keys = settings.api_keys_by_provider.clone();
                let provider_changed = new_provider != state.config.provider_name;
                settings.has_changes = false;

                // Apply changes to state
                state.current_model = new_model.clone();
                state.config.stream_enabled = new_stream;
                state.config.trust_mode = new_trust;
                // Check if caps actually changed before marking
                if state.enabled_caps != new_caps {
                    state.caps_changed = true;
                }
                state.enabled_caps = new_caps.clone();
                state.config.caps = new_caps.clone();
                state.mode = ChatMode::Input;

                // Save to settings file
                if let Ok(mut file_settings) = Settings::load() {
                    file_settings.defaults.temperature = new_temp;
                    file_settings.defaults.max_tokens = new_max_tokens;
                    file_settings.defaults.stream = new_stream;
                    file_settings.defaults.caps = new_caps.clone();

                    // Update model and API keys based on provider
                    match new_provider.as_str() {
                        "anthropic" => {
                            file_settings.providers.anthropic.default_model = new_model.clone()
                        }
                        "openrouter" => {
                            file_settings.providers.openrouter.default_model = new_model.clone()
                        }
                        "blackman" => {
                            file_settings.providers.blackman.default_model = new_model.clone()
                        }
                        "local" | "llama-cpp" => {
                            // Update local settings with selected model
                            file_settings.providers.local.default_model = new_model.clone();
                            // Update model path to point to the selected model
                            if let Some(home) = dirs::home_dir() {
                                let model_path = home
                                    .join(".ted")
                                    .join("models")
                                    .join("local")
                                    .join(format!("{}.gguf", new_model));
                                if model_path.exists() {
                                    file_settings.providers.local.model_path = model_path;
                                }
                            }
                        }
                        _ => {}
                    }

                    // Save API keys for all providers (only update if non-empty to preserve existing keys)
                    if let Some(key) = api_keys.get("anthropic") {
                        if !key.is_empty() {
                            file_settings.providers.anthropic.api_key = Some(key.clone());
                        }
                        // If empty, don't touch existing key - user may not have edited it
                    }
                    if let Some(key) = api_keys.get("openrouter") {
                        if !key.is_empty() {
                            file_settings.providers.openrouter.api_key = Some(key.clone());
                        }
                    }
                    if let Some(key) = api_keys.get("blackman") {
                        if !key.is_empty() {
                            file_settings.providers.blackman.api_key = Some(key.clone());
                        }
                    }

                    if provider_changed {
                        file_settings.defaults.provider = new_provider;
                    }

                    if let Err(e) = file_settings.save() {
                        state.set_error(&format!("Failed to save: {}", e));
                        return Ok(());
                    }
                }

                // Check if provider changed (requires restart)
                if provider_changed {
                    state.needs_restart = true;
                    state.should_quit = true;
                    state.set_status("Provider changed. Restarting...");
                } else {
                    state.set_status("Settings saved to ~/.ted/settings.json");
                }
            }
            return Ok(());
        }
        _ => {}
    }

    // Section-specific key handling
    match settings.current_section {
        SettingsSection::General => match (key.modifiers, key.code) {
            (_, KeyCode::Up) | (_, KeyCode::Char('k')) => {
                settings.move_up();
            }
            (_, KeyCode::Down) | (_, KeyCode::Char('j')) => {
                settings.move_down();
            }
            (_, KeyCode::Enter) => match settings.selected_field() {
                SettingsField::Stream | SettingsField::TrustMode => {
                    settings.toggle_bool();
                }
                SettingsField::Provider => {
                    settings.cycle_provider(true);
                }
                SettingsField::Model => {
                    settings.cycle_model(true);
                }
                _ => {
                    settings.start_editing();
                }
            },
            (_, KeyCode::Left) | (_, KeyCode::Char('h')) => match settings.selected_field() {
                SettingsField::Provider => settings.cycle_provider(false),
                SettingsField::Model => settings.cycle_model(false),
                _ => {}
            },
            (_, KeyCode::Right) | (_, KeyCode::Char('l')) => match settings.selected_field() {
                SettingsField::Provider => settings.cycle_provider(true),
                SettingsField::Model => settings.cycle_model(true),
                _ => {}
            },
            (_, KeyCode::Char(' ')) => match settings.selected_field() {
                SettingsField::Stream | SettingsField::TrustMode => {
                    settings.toggle_bool();
                }
                SettingsField::Provider => {
                    settings.cycle_provider(true);
                }
                SettingsField::Model => {
                    settings.cycle_model(true);
                }
                _ => {}
            },
            _ => {}
        },
        SettingsSection::Capabilities => match (key.modifiers, key.code) {
            (_, KeyCode::Up) | (_, KeyCode::Char('k')) => {
                settings.caps_move_up();
            }
            (_, KeyCode::Down) | (_, KeyCode::Char('j')) => {
                settings.caps_move_down();
            }
            (_, KeyCode::Enter) | (_, KeyCode::Char(' ')) => {
                settings.toggle_cap();
            }
            _ => {}
        },
    }

    Ok(())
}
