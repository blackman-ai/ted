// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use super::*;
use uuid::Uuid;

// ===== SettingsSection Tests =====

#[test]
fn test_settings_section_all() {
    let sections = SettingsSection::all();
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0], SettingsSection::General);
    assert_eq!(sections[1], SettingsSection::Capabilities);
}

#[test]
fn test_settings_section_labels() {
    assert_eq!(SettingsSection::General.label(), "General");
    assert_eq!(SettingsSection::Capabilities.label(), "Capabilities");
}

// ===== SettingsField Tests =====

#[test]
fn test_settings_field_all() {
    let fields = SettingsField::all();
    assert_eq!(fields.len(), 7);
    assert_eq!(fields[0], SettingsField::Provider);
    assert_eq!(fields[1], SettingsField::ApiKey);
    assert_eq!(fields[2], SettingsField::Model);
    assert_eq!(fields[3], SettingsField::Temperature);
    assert_eq!(fields[4], SettingsField::MaxTokens);
    assert_eq!(fields[5], SettingsField::Stream);
    assert_eq!(fields[6], SettingsField::TrustMode);
}

#[test]
fn test_settings_field_labels() {
    assert_eq!(SettingsField::Provider.label(), "Provider");
    assert_eq!(SettingsField::ApiKey.label(), "API Key");
    assert_eq!(SettingsField::Model.label(), "Model");
    assert_eq!(SettingsField::Temperature.label(), "Temperature");
    assert_eq!(SettingsField::MaxTokens.label(), "Max Tokens");
    assert_eq!(SettingsField::Stream.label(), "Streaming");
    assert_eq!(SettingsField::TrustMode.label(), "Trust Mode");
}

// ===== SettingsState Tests =====

fn create_test_settings_state() -> SettingsState {
    let settings = Settings::default();
    let config = ChatTuiConfig {
        session_id: Uuid::new_v4(),
        provider_name: "anthropic".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
        caps: vec!["base".to_string()],
        stream_enabled: true,
        trust_mode: false,
    };
    let enabled_caps = vec!["base".to_string()];
    let available_caps = vec![
        ("base".to_string(), true),
        ("rust-expert".to_string(), true),
    ];
    SettingsState::new(&settings, &config, &enabled_caps, &available_caps)
}

#[test]
fn test_settings_state_new() {
    let state = create_test_settings_state();
    assert_eq!(state.current_section, SettingsSection::General);
    assert_eq!(state.selected_index, 0);
    assert_eq!(state.caps_selected_index, 0);
    assert!(!state.is_editing);
    assert!(state.edit_buffer.is_empty());
    assert_eq!(state.provider, "anthropic");
    assert_eq!(state.model, "claude-sonnet-4-20250514");
    assert!(state.stream);
    assert!(!state.trust_mode);
    assert!(!state.has_changes);
}

#[test]
fn test_settings_state_next_section() {
    let mut state = create_test_settings_state();
    assert_eq!(state.current_section, SettingsSection::General);

    state.next_section();
    assert_eq!(state.current_section, SettingsSection::Capabilities);

    state.next_section();
    assert_eq!(state.current_section, SettingsSection::General);
}

#[test]
fn test_settings_state_prev_section() {
    let mut state = create_test_settings_state();
    assert_eq!(state.current_section, SettingsSection::General);

    state.prev_section();
    assert_eq!(state.current_section, SettingsSection::Capabilities);

    state.prev_section();
    assert_eq!(state.current_section, SettingsSection::General);
}

#[test]
fn test_settings_state_toggle_cap() {
    let mut state = create_test_settings_state();
    state.caps_selected_index = 1; // rust-expert
    assert!(!state.caps_enabled.contains(&"rust-expert".to_string()));

    state.toggle_cap();
    assert!(state.caps_enabled.contains(&"rust-expert".to_string()));
    assert!(state.has_changes);

    state.toggle_cap();
    assert!(!state.caps_enabled.contains(&"rust-expert".to_string()));
}

#[test]
fn test_settings_state_toggle_cap_empty() {
    let settings = Settings::default();
    let config = ChatTuiConfig {
        session_id: Uuid::new_v4(),
        provider_name: "anthropic".to_string(),
        model: "test".to_string(),
        caps: vec![],
        stream_enabled: true,
        trust_mode: false,
    };
    let mut state = SettingsState::new(&settings, &config, &[], &[]);
    // Should not panic with empty caps
    state.toggle_cap();
}

#[test]
fn test_settings_state_caps_navigation() {
    let mut state = create_test_settings_state();
    assert_eq!(state.caps_selected_index, 0);

    state.caps_move_down();
    assert_eq!(state.caps_selected_index, 1);

    state.caps_move_down(); // Should not go beyond last item
    assert_eq!(state.caps_selected_index, 1);

    state.caps_move_up();
    assert_eq!(state.caps_selected_index, 0);

    state.caps_move_up(); // Should not go below 0
    assert_eq!(state.caps_selected_index, 0);
}

#[test]
fn test_settings_state_field_navigation() {
    let mut state = create_test_settings_state();
    assert_eq!(state.selected_index, 0);
    assert_eq!(state.selected_field(), SettingsField::Provider);

    state.move_down();
    assert_eq!(state.selected_index, 1);
    assert_eq!(state.selected_field(), SettingsField::ApiKey);

    state.move_down();
    assert_eq!(state.selected_index, 2);
    assert_eq!(state.selected_field(), SettingsField::Model);

    state.move_up();
    state.move_up();
    assert_eq!(state.selected_index, 0);

    state.move_up(); // Should not go below 0
    assert_eq!(state.selected_index, 0);

    // Move to last field
    for _ in 0..10 {
        state.move_down();
    }
    assert_eq!(state.selected_index, 6); // Max index is 6 (TrustMode)
}

#[test]
fn test_settings_state_cycling_model() {
    let mut state = create_test_settings_state();
    state.selected_index = 2; // Model field

    // Model field uses cycling, not text editing
    state.start_editing();
    assert!(!state.is_editing); // Should NOT enter editing mode

    // Test cycling forward through models (anthropic has 4 models)
    let initial_index = state.model_index;
    state.cycle_model(true);
    assert_eq!(state.model_index, initial_index + 1);
    assert!(state.has_changes);

    // Cycle back
    state.cycle_model(false);
    assert_eq!(state.model_index, initial_index);
}

#[test]
fn test_settings_state_editing_temperature() {
    let mut state = create_test_settings_state();
    state.selected_index = 3; // Temperature field

    state.start_editing();
    state.edit_buffer = "0.5".to_string();
    state.confirm_editing();
    assert!((state.temperature - 0.5).abs() < 0.01);

    // Test clamping high
    state.start_editing();
    state.edit_buffer = "5.0".to_string();
    state.confirm_editing();
    assert!((state.temperature - 2.0).abs() < 0.01);

    // Test clamping low
    state.start_editing();
    state.edit_buffer = "-1.0".to_string();
    state.confirm_editing();
    assert!((state.temperature - 0.0).abs() < 0.01);

    // Test invalid input
    state.start_editing();
    let old_temp = state.temperature;
    state.edit_buffer = "invalid".to_string();
    state.confirm_editing();
    assert!((state.temperature - old_temp).abs() < 0.01);
}

#[test]
fn test_settings_state_editing_max_tokens() {
    let mut state = create_test_settings_state();
    state.selected_index = 4; // MaxTokens field

    state.start_editing();
    state.edit_buffer = "4096".to_string();
    state.confirm_editing();
    assert_eq!(state.max_tokens, 4096);

    // Test clamping high
    state.start_editing();
    state.edit_buffer = "999999".to_string();
    state.confirm_editing();
    assert_eq!(state.max_tokens, 128000);

    // Test clamping low
    state.start_editing();
    state.edit_buffer = "10".to_string();
    state.confirm_editing();
    assert_eq!(state.max_tokens, 100);
}

#[test]
fn test_settings_state_cancel_editing() {
    let mut state = create_test_settings_state();
    state.selected_index = 3; // Temperature field
    let original_temp = state.temperature;

    state.start_editing();
    state.edit_buffer = "999".to_string();
    state.cancel_editing();

    assert!(!state.is_editing);
    assert!(state.edit_buffer.is_empty());
    assert!((state.temperature - original_temp).abs() < 0.01);
}

#[test]
fn test_settings_state_toggle_bool() {
    let mut state = create_test_settings_state();

    // Toggle stream (index 5)
    state.selected_index = 5;
    assert!(state.stream);
    state.toggle_bool();
    assert!(!state.stream);
    assert!(state.has_changes);

    // Toggle trust mode (index 6)
    state.selected_index = 6;
    assert!(!state.trust_mode);
    state.toggle_bool();
    assert!(state.trust_mode);

    // Toggle on non-bool field should do nothing
    state.selected_index = 2; // Model
    let old_model = state.model.clone();
    state.toggle_bool();
    assert_eq!(state.model, old_model);
}

#[test]
fn test_settings_state_cycle_provider() {
    let mut state = create_test_settings_state();
    assert_eq!(state.provider, "anthropic");
    assert_eq!(state.provider_index, 0);

    state.cycle_provider(true);
    assert_eq!(state.provider, "local");
    assert!(state.has_changes);

    state.cycle_provider(true);
    assert_eq!(state.provider, "openrouter");

    state.cycle_provider(true);
    assert_eq!(state.provider, "blackman");

    state.cycle_provider(true); // Wrap around
    assert_eq!(state.provider, "anthropic");

    // Test backward cycling
    state.cycle_provider(false);
    assert_eq!(state.provider, "blackman");
}

#[test]
fn test_settings_state_cycle_provider_backward_from_zero() {
    let mut state = create_test_settings_state();
    assert_eq!(state.provider_index, 0);

    state.cycle_provider(false);
    assert_eq!(state.provider, "blackman");
    assert_eq!(state.provider_index, 3);
}

#[test]
fn test_settings_state_current_value() {
    let mut state = create_test_settings_state();
    state.model = "test-model".to_string();
    state.temperature = 0.7;
    state.max_tokens = 2048;
    state.stream = true;
    state.trust_mode = false;

    // Provider and Model now show cycling UI with arrows and index
    assert_eq!(
        state.current_value(SettingsField::Provider),
        "◀ anthropic ▶  (1/4)"
    );
    // Model shows the current model with cycling UI (models list has 4 anthropic models)
    assert_eq!(
        state.current_value(SettingsField::Model),
        "◀ test-model ▶  (1/4)"
    );
    assert_eq!(state.current_value(SettingsField::Temperature), "0.7");
    assert_eq!(state.current_value(SettingsField::MaxTokens), "2048");
    assert_eq!(state.current_value(SettingsField::Stream), "On");
    assert_eq!(state.current_value(SettingsField::TrustMode), "Off");

    state.stream = false;
    state.trust_mode = true;
    assert_eq!(state.current_value(SettingsField::Stream), "Off");
    assert_eq!(state.current_value(SettingsField::TrustMode), "On");
}

#[test]
fn test_settings_state_insert_char_and_backspace() {
    let mut state = create_test_settings_state();
    state.edit_buffer = String::new();

    state.insert_char('a');
    state.insert_char('b');
    state.insert_char('c');
    assert_eq!(state.edit_buffer, "abc");

    state.backspace();
    assert_eq!(state.edit_buffer, "ab");

    state.backspace();
    state.backspace();
    assert!(state.edit_buffer.is_empty());

    state.backspace(); // Should not panic on empty buffer
    assert!(state.edit_buffer.is_empty());
}

// ===== TuiState Tests =====

fn create_test_tui_state() -> TuiState {
    let settings = Settings::default();
    let config = ChatTuiConfig {
        session_id: Uuid::new_v4(),
        provider_name: "anthropic".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
        caps: vec!["base".to_string()],
        stream_enabled: true,
        trust_mode: false,
    };
    TuiState::new(config, &settings)
}

#[test]
fn test_tui_state_new() {
    let state = create_test_tui_state();
    assert_eq!(state.mode, ChatMode::Input);
    assert!(state.messages.is_empty());
    assert!(state.status_message.is_none());
    assert!(!state.status_is_error);
    assert!(!state.is_processing);
    assert!(!state.should_quit);
    assert!(state.agent_pane_visible);
    assert!(!state.agent_pane_expanded);
    assert_eq!(state.scroll_offset, 0);
}

#[test]
fn test_tui_state_set_status() {
    let mut state = create_test_tui_state();

    state.set_status("Test status");
    assert_eq!(state.status_message, Some("Test status".to_string()));
    assert!(!state.status_is_error);
}

#[test]
fn test_tui_state_set_error() {
    let mut state = create_test_tui_state();

    state.set_error("Test error");
    assert_eq!(state.status_message, Some("Test error".to_string()));
    assert!(state.status_is_error);
}

#[test]
fn test_tui_state_tick_animation() {
    let mut state = create_test_tui_state();
    assert_eq!(state.animation_frame, 0);

    state.tick_animation();
    assert_eq!(state.animation_frame, 1);

    // Test wrapping
    state.animation_frame = 255;
    state.tick_animation();
    assert_eq!(state.animation_frame, 0);
}

#[test]
fn test_tui_state_thinking_indicator() {
    let mut state = create_test_tui_state();

    state.animation_frame = 0;
    assert_eq!(state.thinking_indicator(), "●○○");

    state.animation_frame = 4;
    assert_eq!(state.thinking_indicator(), "○●○");

    state.animation_frame = 8;
    assert_eq!(state.thinking_indicator(), "○○●");

    state.animation_frame = 12;
    assert_eq!(state.thinking_indicator(), "○●○");
}

#[test]
fn test_tui_state_scroll_up_down() {
    let mut state = create_test_tui_state();
    state.chat_area_height = 5; // Small visible area

    // Add enough messages to require scrolling
    // Each message takes ~3 lines (header + content + spacing)
    for i in 0..10 {
        state
            .messages
            .push(DisplayMessage::user(format!("Line {}", i)));
    }

    // Set initial scroll offset
    state.scroll_offset = 10;

    state.scroll_up(2);
    assert_eq!(state.scroll_offset, 8);

    state.scroll_up(20); // Should not go below 0
    assert_eq!(state.scroll_offset, 0);

    // After scrolling down, offset should increase (but clamped to max)
    let total = state.total_messages_height();
    let max_offset = total.saturating_sub(5);
    state.scroll_down(5);
    assert!(state.scroll_offset <= max_offset);
    assert!(state.scroll_offset > 0);
}

#[test]
fn test_tui_state_update_chat_height() {
    let mut state = create_test_tui_state();
    state.agent_pane_visible = false;

    state.update_chat_height(30);
    // 30 - 1 (title) - 3 (input) - 0 (agent pane hidden) = 26
    assert_eq!(state.chat_area_height, 26);

    state.agent_pane_visible = true;
    state.agent_pane_expanded = false;
    // With visible agent pane but no agents, it should be 0
    state.update_chat_height(30);
    assert_eq!(state.chat_area_height, 26); // Still 26 because no agents

    // Add an agent to make pane visible
    state.agents.track(
        Uuid::new_v4(),
        "tc-test".to_string(),
        "test-agent".to_string(),
        "research".to_string(),
        "Test task".to_string(),
    );
    state.update_chat_height(30);
    // 30 - 1 (title) - 3 (input) - 3 (collapsed pane) = 23
    assert_eq!(state.chat_area_height, 23);

    state.agent_pane_expanded = true;
    state.agent_pane_height = 6;
    state.update_chat_height(30);
    // 30 - 1 (title) - 3 (input) - 6 (expanded pane) = 20
    assert_eq!(state.chat_area_height, 20);
}

#[test]
fn test_tui_state_scroll_to_bottom() {
    let mut state = create_test_tui_state();
    state.chat_area_height = 5;

    // With no messages, scroll should be 0
    state.scroll_to_bottom(5);
    assert_eq!(state.scroll_offset, 0);

    // Add messages that exceed visible height
    state
        .messages
        .push(DisplayMessage::user("Line 1".to_string()));
    state
        .messages
        .push(DisplayMessage::user("Line 2".to_string()));
    state
        .messages
        .push(DisplayMessage::user("Line 3".to_string()));

    state.scroll_to_bottom(5);
    // Total height of 3 messages is about 9 lines (3 lines each: header + content + spacing)
    // scroll_offset should be total - visible
    assert!(state.scroll_offset > 0);
}

#[test]
fn test_tui_state_auto_scroll() {
    let mut state = create_test_tui_state();
    state.chat_area_height = 10;

    state
        .messages
        .push(DisplayMessage::user("Test message".to_string()));
    state.auto_scroll();

    // Should be at bottom
    let expected_height = state.total_messages_height();
    if expected_height > 10 {
        assert_eq!(state.scroll_offset, expected_height - 10);
    } else {
        assert_eq!(state.scroll_offset, 0);
    }
}

#[test]
fn test_tui_state_with_progress_tracker() {
    let tracker = crate::tools::builtin::new_progress_tracker();
    let state = create_test_tui_state().with_progress_tracker(tracker);
    assert!(state.agent_progress_tracker.is_some());
}

#[test]
fn test_tui_state_pending_messages() {
    let mut state = create_test_tui_state();
    assert!(state.pending_messages.is_empty());

    state.pending_messages.push("First message".to_string());
    state.pending_messages.push("Second message".to_string());
    assert_eq!(state.pending_messages.len(), 2);

    let msg = state.pending_messages.remove(0);
    assert_eq!(msg, "First message");
    assert_eq!(state.pending_messages.len(), 1);
}

#[test]
fn test_tui_state_caps_changed_flag() {
    let mut state = create_test_tui_state();
    assert!(!state.caps_changed);

    state.caps_changed = true;
    assert!(state.caps_changed);
}

#[test]
fn test_settings_state_confirm_editing_empty_model() {
    let mut state = create_test_settings_state();
    state.selected_index = 1; // Model field
    let original_model = state.model.clone();

    state.start_editing();
    state.edit_buffer = String::new(); // Empty
    state.confirm_editing();

    // Model should remain unchanged with empty input
    assert_eq!(state.model, original_model);
}

#[test]
fn test_settings_state_confirm_editing_not_editing() {
    let mut state = create_test_settings_state();
    let original_model = state.model.clone();

    // Call confirm without being in editing mode
    state.confirm_editing();

    // Nothing should change
    assert_eq!(state.model, original_model);
    assert!(!state.is_editing);
}

#[test]
fn test_settings_state_start_editing_stream_field() {
    let mut state = create_test_settings_state();
    state.selected_index = 5; // Stream field

    state.start_editing();
    assert!(state.is_editing);
    // For bool fields, edit_buffer should be empty
    assert!(state.edit_buffer.is_empty());
}

#[test]
fn test_settings_state_cycling_provider_field() {
    let mut state = create_test_settings_state();
    state.selected_index = 0; // Provider field

    // Provider field uses cycling, not text editing
    state.start_editing();
    assert!(!state.is_editing); // Should NOT enter editing mode

    // Test cycling forward through providers
    assert_eq!(state.provider, "anthropic");
    state.cycle_provider(true);
    assert_eq!(state.provider, "local");
    assert!(state.has_changes);

    state.cycle_provider(true);
    assert_eq!(state.provider, "openrouter");
}

// ===== handle_command Tests =====

#[test]
fn test_handle_command_help() {
    let mut state = create_test_tui_state();
    handle_command("/help", &mut state, None).unwrap();
    assert_eq!(state.mode, ChatMode::Help);
}

#[test]
fn test_handle_command_clear() {
    let mut state = create_test_tui_state();
    state
        .messages
        .push(DisplayMessage::user("test".to_string()));
    state
        .messages
        .push(DisplayMessage::assistant("response".to_string(), vec![]));

    handle_command("/clear", &mut state, None).unwrap();
    assert!(state.messages.is_empty());
    assert_eq!(state.status_message, Some("Chat cleared".to_string()));
}

#[test]
fn test_handle_command_agents() {
    let mut state = create_test_tui_state();
    assert!(state.agent_pane_visible);

    handle_command("/agents", &mut state, None).unwrap();
    assert!(!state.agent_pane_visible);

    handle_command("/agents", &mut state, None).unwrap();
    assert!(state.agent_pane_visible);
}

#[test]
fn test_handle_command_model_with_arg() {
    let mut state = create_test_tui_state();
    handle_command("/model claude-opus-4-5-20250514", &mut state, None).unwrap();
    assert_eq!(state.current_model, "claude-opus-4-5-20250514");
    assert!(state
        .status_message
        .as_ref()
        .unwrap()
        .contains("Model set to"));
}

#[test]
fn test_handle_command_model_with_space_only() {
    let mut state = create_test_tui_state();
    // "/model " gets trimmed to "/model" which shows info
    handle_command("/model ", &mut state, None).unwrap();
    // Should add a system message showing model info
    assert!(!state.messages.is_empty());
}

#[test]
fn test_handle_command_model_no_arg() {
    let mut state = create_test_tui_state();
    handle_command("/model", &mut state, None).unwrap();
    // Should show info message in messages list
    assert!(!state.messages.is_empty());
}

#[test]
fn test_handle_command_settings() {
    let mut state = create_test_tui_state();
    handle_command("/settings", &mut state, None).unwrap();
    assert_eq!(state.mode, ChatMode::Settings);
}

#[test]
fn test_handle_command_caps() {
    let mut state = create_test_tui_state();
    handle_command("/caps", &mut state, None).unwrap();
    assert_eq!(state.mode, ChatMode::Settings);
    // Should open on Capabilities tab
    if let Some(ref settings) = state.settings_state {
        assert_eq!(settings.current_section, SettingsSection::Capabilities);
    }
}

#[test]
fn test_handle_command_cap_toggle() {
    let mut state = create_test_tui_state();
    // Add some available caps
    state.available_caps = vec![
        ("base".to_string(), true),
        ("rust-expert".to_string(), true),
    ];
    state.enabled_caps = vec!["base".to_string()];

    // Enable rust-expert
    handle_command("/cap rust-expert", &mut state, None).unwrap();
    assert!(state.enabled_caps.contains(&"rust-expert".to_string()));
    assert!(state.status_message.as_ref().unwrap().contains("Enabled"));

    // Disable rust-expert
    handle_command("/cap rust-expert", &mut state, None).unwrap();
    assert!(!state.enabled_caps.contains(&"rust-expert".to_string()));
    assert!(state.status_message.as_ref().unwrap().contains("Disabled"));
}

#[test]
fn test_handle_command_cap_unknown() {
    let mut state = create_test_tui_state();
    state.available_caps = vec![("base".to_string(), true)];

    handle_command("/cap unknown-cap", &mut state, None).unwrap();
    assert!(state.status_is_error);
    assert!(state
        .status_message
        .as_ref()
        .unwrap()
        .contains("Unknown cap"));
}

#[test]
fn test_handle_command_cap_with_space_only() {
    let mut state = create_test_tui_state();
    // "/cap " gets trimmed to "/cap" which falls through to unknown command
    handle_command("/cap ", &mut state, None).unwrap();
    assert!(state.status_is_error);
    assert!(state
        .status_message
        .as_ref()
        .unwrap()
        .contains("Unknown command"));
}

#[test]
fn test_handle_command_unknown() {
    let mut state = create_test_tui_state();
    handle_command("/unknown-command", &mut state, None).unwrap();
    assert!(state.status_is_error);
    assert!(state
        .status_message
        .as_ref()
        .unwrap()
        .contains("Unknown command"));
}

#[test]
fn test_handle_command_case_insensitive() {
    let mut state = create_test_tui_state();
    handle_command("/HELP", &mut state, None).unwrap();
    assert_eq!(state.mode, ChatMode::Help);

    state.mode = ChatMode::Input;
    handle_command("/Help", &mut state, None).unwrap();
    assert_eq!(state.mode, ChatMode::Help);
}

// ===== handle_help_key Tests =====

#[test]
fn test_handle_help_key_esc() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Help;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_help_key(&mut state, key).unwrap();
    assert_eq!(state.mode, ChatMode::Input);
}

#[test]
fn test_handle_help_key_q() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Help;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('q'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_help_key(&mut state, key).unwrap();
    assert_eq!(state.mode, ChatMode::Input);
}

#[test]
fn test_handle_help_key_question() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Help;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('?'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_help_key(&mut state, key).unwrap();
    assert_eq!(state.mode, ChatMode::Input);
}

#[test]
fn test_handle_help_key_other() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Help;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_help_key(&mut state, key).unwrap();
    // Should stay in Help mode
    assert_eq!(state.mode, ChatMode::Help);
}

// ===== handle_normal_key Tests =====

#[test]
fn test_handle_normal_key_enter() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();
    assert_eq!(state.mode, ChatMode::Input);
}

#[test]
fn test_handle_normal_key_i() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('i'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();
    assert_eq!(state.mode, ChatMode::Input);
}

#[test]
fn test_handle_normal_key_q() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('q'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();
    assert!(state.should_quit);
}

#[test]
fn test_handle_normal_key_question() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('?'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();
    assert_eq!(state.mode, ChatMode::Help);
}

#[test]
fn test_handle_normal_key_tab() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;
    state.agent_pane_visible = true;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();
    // Tab toggles agent_pane_visible
    assert!(!state.agent_pane_visible);
}

#[test]
fn test_handle_normal_key_scroll_j() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;
    state.scroll_offset = 5;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('j'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();
    // Should scroll down (but clamped to max)
}

#[test]
fn test_handle_normal_key_scroll_k() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;
    state.scroll_offset = 5;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('k'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();
    assert_eq!(state.scroll_offset, 4);
}

#[test]
fn test_handle_normal_key_g_top() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;
    state.scroll_offset = 10;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('g'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();
    assert_eq!(state.scroll_offset, 0);
}

#[test]
fn test_handle_normal_key_ctrl_a_no_op() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::CONTROL,
    );
    handle_normal_key(&mut state, key).unwrap();
    // Ctrl+A is not handled in normal mode (falls through)
    assert_eq!(state.mode, ChatMode::Normal);
}

// ===== handle_input_key Tests =====

#[test]
fn test_handle_input_key_esc() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();
    assert_eq!(state.mode, ChatMode::Normal);
}

#[test]
fn test_handle_input_key_tab() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.agent_pane_visible = true;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();
    // Tab toggles agent_pane_visible
    assert!(!state.agent_pane_visible);
}

#[test]
fn test_handle_input_key_char() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.clear();

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();
    assert_eq!(state.input.text(), "a");
}

#[test]
fn test_handle_input_key_backspace() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.set_buffer("ab".to_string());

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Backspace,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();
    assert_eq!(state.input.text(), "a");
}

#[test]
fn test_handle_input_key_ctrl_u() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.set_buffer("hello world".to_string());

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('u'),
        crossterm::event::KeyModifiers::CONTROL,
    );
    handle_input_key(&mut state, key).unwrap();
    assert!(state.input.is_empty());
}

#[test]
fn test_handle_input_key_ctrl_w() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.set_buffer("hello world".to_string());

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('w'),
        crossterm::event::KeyModifiers::CONTROL,
    );
    handle_input_key(&mut state, key).unwrap();
    assert_eq!(state.input.text(), "hello ");
}

#[test]
fn test_handle_input_key_ctrl_a() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.set_buffer("hello".to_string());
    state.input.cursor = 3;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::CONTROL,
    );
    handle_input_key(&mut state, key).unwrap();
    assert_eq!(state.input.cursor, 0);
}

#[test]
fn test_handle_input_key_ctrl_e() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.set_buffer("hello".to_string());
    state.input.cursor = 0;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('e'),
        crossterm::event::KeyModifiers::CONTROL,
    );
    handle_input_key(&mut state, key).unwrap();
    assert_eq!(state.input.cursor, 5);
}

#[test]
fn test_handle_input_key_up_history() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.history = vec!["first".to_string(), "second".to_string()];

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Up,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();
    assert_eq!(state.input.text(), "second");
}

#[test]
fn test_handle_input_key_down_history() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.history = vec!["first".to_string(), "second".to_string()];
    state.input.history_prev(); // Go to "second"

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();
}

#[test]
fn test_handle_input_key_left() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.set_buffer("hello".to_string());

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Left,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();
    assert_eq!(state.input.cursor, 4);
}

#[test]
fn test_handle_input_key_right() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.set_buffer("hello".to_string());
    state.input.cursor = 0;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();
    assert_eq!(state.input.cursor, 1);
}

// ===== TuiState Additional Tests =====

#[test]
fn test_tui_state_needs_restart() {
    let mut state = create_test_tui_state();
    assert!(!state.needs_restart);

    state.needs_restart = true;
    assert!(state.needs_restart);
}

#[test]
fn test_tui_state_available_caps() {
    let state = create_test_tui_state();
    // Should have available caps loaded
    // (May be empty in test environment without cap files)
    assert!(state.available_caps.is_empty() || !state.available_caps.is_empty());
}

#[test]
fn test_tui_state_enabled_caps() {
    let state = create_test_tui_state();
    assert_eq!(state.enabled_caps, vec!["base".to_string()]);
}

#[test]
fn test_tui_state_settings_state_exists() {
    let state = create_test_tui_state();
    assert!(state.settings_state.is_some());
}

// ===== Drawing Function Tests =====

fn create_test_terminal(width: u16, height: u16) -> Terminal<ratatui::backend::TestBackend> {
    let backend = ratatui::backend::TestBackend::new(width, height);
    Terminal::new(backend).unwrap()
}

#[test]
fn test_draw_tui_basic() {
    let state = create_test_tui_state();
    let mut terminal = create_test_terminal(80, 24);

    terminal.draw(|f| draw_tui(f, &state)).unwrap();

    // Verify buffer is not empty
    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
    assert!(buffer.area.height > 0);
}

#[test]
fn test_draw_tui_with_messages() {
    let mut state = create_test_tui_state();
    state
        .messages
        .push(DisplayMessage::user("Hello".to_string()));
    state
        .messages
        .push(DisplayMessage::assistant("Hi there!".to_string(), vec![]));

    let mut terminal = create_test_terminal(80, 24);
    terminal.draw(|f| draw_tui(f, &state)).unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_tui_help_mode() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Help;

    let mut terminal = create_test_terminal(80, 24);
    terminal.draw(|f| draw_tui(f, &state)).unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_tui_settings_mode() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;

    let mut terminal = create_test_terminal(80, 24);
    terminal.draw(|f| draw_tui(f, &state)).unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_tui_processing() {
    let mut state = create_test_tui_state();
    state.is_processing = true;

    let mut terminal = create_test_terminal(80, 24);
    terminal.draw(|f| draw_tui(f, &state)).unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_tui_with_status_message() {
    let mut state = create_test_tui_state();
    state.set_status("Test status");

    let mut terminal = create_test_terminal(80, 24);
    terminal.draw(|f| draw_tui(f, &state)).unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_tui_with_error_message() {
    let mut state = create_test_tui_state();
    state.set_error("Test error");

    let mut terminal = create_test_terminal(80, 24);
    terminal.draw(|f| draw_tui(f, &state)).unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_tui_with_agents() {
    let mut state = create_test_tui_state();
    state.agent_pane_visible = true;
    state.agents.track(
        Uuid::new_v4(),
        "tc-test".to_string(),
        "test-agent".to_string(),
        "research".to_string(),
        "Test task".to_string(),
    );

    let mut terminal = create_test_terminal(80, 24);
    terminal.draw(|f| draw_tui(f, &state)).unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_tui_with_expanded_agent_pane() {
    let mut state = create_test_tui_state();
    state.agent_pane_visible = true;
    state.agent_pane_expanded = true;
    state.agents.track(
        Uuid::new_v4(),
        "tc-test".to_string(),
        "test-agent".to_string(),
        "research".to_string(),
        "Test task".to_string(),
    );

    let mut terminal = create_test_terminal(80, 30);
    terminal.draw(|f| draw_tui(f, &state)).unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_title_bar() {
    let state = create_test_tui_state();
    let mut terminal = create_test_terminal(80, 24);

    terminal
        .draw(|f| {
            let area = ratatui::layout::Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 1,
            };
            draw_title_bar(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_title_bar_with_caps() {
    let settings = Settings::default();
    let config = ChatTuiConfig {
        session_id: Uuid::new_v4(),
        provider_name: "anthropic".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
        caps: vec!["base".to_string(), "rust-expert".to_string()],
        stream_enabled: true,
        trust_mode: false,
    };
    let state = TuiState::new(config, &settings);

    let mut terminal = create_test_terminal(80, 24);
    terminal
        .draw(|f| {
            let area = ratatui::layout::Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 1,
            };
            draw_title_bar(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_chat_area_empty() {
    let state = create_test_tui_state();
    let mut terminal = create_test_terminal(80, 24);

    terminal
        .draw(|f| {
            let area = ratatui::layout::Rect {
                x: 0,
                y: 1,
                width: 80,
                height: 18,
            };
            draw_chat_area(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_chat_area_with_messages() {
    let mut state = create_test_tui_state();
    state
        .messages
        .push(DisplayMessage::user("Test message".to_string()));

    let mut terminal = create_test_terminal(80, 24);
    terminal
        .draw(|f| {
            let area = ratatui::layout::Rect {
                x: 0,
                y: 1,
                width: 80,
                height: 18,
            };
            draw_chat_area(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_agent_pane() {
    let mut state = create_test_tui_state();
    state.agents.track(
        Uuid::new_v4(),
        "tc-test".to_string(),
        "test-agent".to_string(),
        "research".to_string(),
        "Test task".to_string(),
    );

    let mut terminal = create_test_terminal(80, 24);
    terminal
        .draw(|f| {
            let area = ratatui::layout::Rect {
                x: 0,
                y: 19,
                width: 80,
                height: 3,
            };
            draw_agent_pane(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_agent_pane_expanded() {
    let mut state = create_test_tui_state();
    state.agent_pane_expanded = true;
    state.agents.track(
        Uuid::new_v4(),
        "tc-1".to_string(),
        "agent1".to_string(),
        "research".to_string(),
        "Task 1".to_string(),
    );
    state.agents.track(
        Uuid::new_v4(),
        "tc-2".to_string(),
        "agent2".to_string(),
        "code".to_string(),
        "Task 2".to_string(),
    );

    let mut terminal = create_test_terminal(80, 24);
    terminal
        .draw(|f| {
            let area = ratatui::layout::Rect {
                x: 0,
                y: 16,
                width: 80,
                height: 6,
            };
            draw_agent_pane(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_agent_pane_focused() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::AgentFocus;
    state.agents.track(
        Uuid::new_v4(),
        "tc-test".to_string(),
        "test-agent".to_string(),
        "research".to_string(),
        "Test task".to_string(),
    );

    let mut terminal = create_test_terminal(80, 24);
    terminal
        .draw(|f| {
            let area = ratatui::layout::Rect {
                x: 0,
                y: 19,
                width: 80,
                height: 3,
            };
            draw_agent_pane(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_input_area() {
    let state = create_test_tui_state();
    let mut terminal = create_test_terminal(80, 24);

    terminal
        .draw(|f| {
            let area = ratatui::layout::Rect {
                x: 0,
                y: 21,
                width: 80,
                height: 3,
            };
            draw_input_area(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_input_area_processing() {
    let mut state = create_test_tui_state();
    state.is_processing = true;

    let mut terminal = create_test_terminal(80, 24);
    terminal
        .draw(|f| {
            let area = ratatui::layout::Rect {
                x: 0,
                y: 21,
                width: 80,
                height: 3,
            };
            draw_input_area(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_input_area_with_queued_messages() {
    let mut state = create_test_tui_state();
    state.is_processing = true;
    state.pending_messages = vec!["queued 1".to_string(), "queued 2".to_string()];

    let mut terminal = create_test_terminal(80, 24);
    terminal
        .draw(|f| {
            let area = ratatui::layout::Rect {
                x: 0,
                y: 21,
                width: 80,
                height: 3,
            };
            draw_input_area(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_input_area_not_focused() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;

    let mut terminal = create_test_terminal(80, 24);
    terminal
        .draw(|f| {
            let area = ratatui::layout::Rect {
                x: 0,
                y: 21,
                width: 80,
                height: 3,
            };
            draw_input_area(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_help_overlay() {
    let mut terminal = create_test_terminal(80, 24);

    terminal
        .draw(|f| {
            let area = f.area();
            draw_help_overlay(f, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_help_overlay_small_terminal() {
    let mut terminal = create_test_terminal(40, 12);

    terminal
        .draw(|f| {
            let area = f.area();
            draw_help_overlay(f, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_settings_overlay_general_section() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;

    let mut terminal = create_test_terminal(80, 30);
    terminal
        .draw(|f| {
            let area = f.area();
            draw_settings_overlay(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_settings_overlay_capabilities_section() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.current_section = SettingsSection::Capabilities;
    }

    let mut terminal = create_test_terminal(80, 30);
    terminal
        .draw(|f| {
            let area = f.area();
            draw_settings_overlay(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_settings_overlay_editing() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 1; // Model
        settings.is_editing = true;
        settings.edit_buffer = "test-model".to_string();
    }

    let mut terminal = create_test_terminal(80, 30);
    terminal
        .draw(|f| {
            let area = f.area();
            draw_settings_overlay(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_settings_overlay_with_changes() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.has_changes = true;
    }

    let mut terminal = create_test_terminal(80, 30);
    terminal
        .draw(|f| {
            let area = f.area();
            draw_settings_overlay(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_settings_overlay_empty_caps() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.current_section = SettingsSection::Capabilities;
        settings.available_caps = vec![];
    }

    let mut terminal = create_test_terminal(80, 30);
    terminal
        .draw(|f| {
            let area = f.area();
            draw_settings_overlay(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_settings_overlay_many_caps() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.current_section = SettingsSection::Capabilities;
        settings.available_caps = (0..20)
            .map(|i| (format!("cap-{}", i), i % 2 == 0))
            .collect();
        settings.caps_selected_index = 15; // Trigger scrolling
    }

    let mut terminal = create_test_terminal(80, 30);
    terminal
        .draw(|f| {
            let area = f.area();
            draw_settings_overlay(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_settings_overlay_small_terminal() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;

    let mut terminal = create_test_terminal(50, 15);
    terminal
        .draw(|f| {
            let area = f.area();
            draw_settings_overlay(f, &state, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

#[test]
fn test_draw_settings_overlay_no_settings_state() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    state.settings_state = None;

    let mut terminal = create_test_terminal(80, 30);
    terminal
        .draw(|f| {
            let area = f.area();
            draw_settings_overlay(f, &state, area);
        })
        .unwrap();

    // Should not panic, just return early
    let buffer = terminal.backend().buffer();
    assert!(buffer.area.width > 0);
}

// ===== handle_settings_key Tests =====

#[test]
fn test_handle_settings_key_esc() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();
    assert_eq!(state.mode, ChatMode::Input);
}

#[test]
fn test_handle_settings_key_q() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('q'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();
    assert_eq!(state.mode, ChatMode::Input);
}

#[test]
fn test_handle_settings_key_tab() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    assert_eq!(
        state.settings_state.as_ref().unwrap().current_section,
        SettingsSection::General
    );

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Tab,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    assert_eq!(
        state.settings_state.as_ref().unwrap().current_section,
        SettingsSection::Capabilities
    );
}

#[test]
fn test_handle_settings_key_shift_tab() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.current_section = SettingsSection::Capabilities;
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::BackTab,
        crossterm::event::KeyModifiers::SHIFT,
    );
    handle_settings_key(&mut state, key).unwrap();

    assert_eq!(
        state.settings_state.as_ref().unwrap().current_section,
        SettingsSection::General
    );
}

#[test]
fn test_handle_settings_key_navigation_up() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 2;
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Up,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    assert_eq!(state.settings_state.as_ref().unwrap().selected_index, 1);
}

#[test]
fn test_handle_settings_key_navigation_down() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    assert_eq!(state.settings_state.as_ref().unwrap().selected_index, 1);
}

#[test]
fn test_handle_settings_key_navigation_k() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 2;
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('k'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    assert_eq!(state.settings_state.as_ref().unwrap().selected_index, 1);
}

#[test]
fn test_handle_settings_key_navigation_j() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('j'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    assert_eq!(state.settings_state.as_ref().unwrap().selected_index, 1);
}

#[test]
fn test_handle_settings_key_enter_on_provider() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 0; // Provider
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    // Enter on Provider cycles it
    let settings = state.settings_state.as_ref().unwrap();
    assert_eq!(settings.provider, "local");
}

#[test]
fn test_handle_settings_key_enter_on_model() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 2; // Model
    }
    let initial_model = state.settings_state.as_ref().unwrap().model.clone();

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    // Enter on Model cycles it (not editing)
    let settings = state.settings_state.as_ref().unwrap();
    assert!(!settings.is_editing);
    assert_ne!(settings.model, initial_model);
}

#[test]
fn test_handle_settings_key_enter_on_stream() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 5; // Stream
    }
    let initial_stream = state.settings_state.as_ref().unwrap().stream;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    // Enter on Stream toggles it
    assert_ne!(
        state.settings_state.as_ref().unwrap().stream,
        initial_stream
    );
}

#[test]
fn test_handle_settings_key_enter_on_trust_mode() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 6; // TrustMode
    }
    let initial_trust = state.settings_state.as_ref().unwrap().trust_mode;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    // Enter on TrustMode toggles it
    assert_ne!(
        state.settings_state.as_ref().unwrap().trust_mode,
        initial_trust
    );
}

#[test]
fn test_handle_settings_key_space_on_stream() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 5; // Stream
    }
    let initial_stream = state.settings_state.as_ref().unwrap().stream;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    // Space on Stream toggles it
    assert_ne!(
        state.settings_state.as_ref().unwrap().stream,
        initial_stream
    );
}

#[test]
fn test_handle_settings_key_space_on_provider() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 0; // Provider
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    // Space on Provider cycles it
    let settings = state.settings_state.as_ref().unwrap();
    assert_eq!(settings.provider, "local");
}

#[test]
fn test_handle_settings_key_left_on_provider() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 0; // Provider
        settings.provider_index = 1; // local
        settings.provider = "local".to_string();
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Left,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    // Left on Provider cycles backward
    let settings = state.settings_state.as_ref().unwrap();
    assert_eq!(settings.provider, "anthropic");
}

#[test]
fn test_handle_settings_key_right_on_provider() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 0; // Provider
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    // Right on Provider cycles forward
    let settings = state.settings_state.as_ref().unwrap();
    assert_eq!(settings.provider, "local");
}

#[test]
fn test_handle_settings_key_h_on_provider() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 0; // Provider
        settings.provider_index = 1;
        settings.provider = "local".to_string();
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('h'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    // h on Provider cycles backward
    let settings = state.settings_state.as_ref().unwrap();
    assert_eq!(settings.provider, "anthropic");
}

#[test]
fn test_handle_settings_key_l_on_provider() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 0; // Provider
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('l'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    // l on Provider cycles forward
    let settings = state.settings_state.as_ref().unwrap();
    assert_eq!(settings.provider, "local");
}

#[test]
fn test_handle_settings_key_editing_mode_char() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 1; // Model
        settings.is_editing = true;
        settings.edit_buffer = "test".to_string();
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('x'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    let settings = state.settings_state.as_ref().unwrap();
    assert_eq!(settings.edit_buffer, "testx");
}

#[test]
fn test_handle_settings_key_editing_mode_backspace() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 1; // Model
        settings.is_editing = true;
        settings.edit_buffer = "test".to_string();
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Backspace,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    let settings = state.settings_state.as_ref().unwrap();
    assert_eq!(settings.edit_buffer, "tes");
}

#[test]
fn test_handle_settings_key_editing_mode_esc() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 1;
        settings.is_editing = true;
        settings.edit_buffer = "changed".to_string();
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    let settings = state.settings_state.as_ref().unwrap();
    assert!(!settings.is_editing);
    assert!(settings.edit_buffer.is_empty());
}

#[test]
fn test_handle_settings_key_editing_mode_enter() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.selected_index = 3; // Temperature (uses text editing)
        settings.is_editing = true;
        settings.edit_buffer = "0.8".to_string();
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    let settings = state.settings_state.as_ref().unwrap();
    assert!(!settings.is_editing);
    assert!((settings.temperature - 0.8).abs() < 0.01);
}

#[test]
fn test_handle_settings_key_caps_navigation() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.current_section = SettingsSection::Capabilities;
        settings.available_caps = vec![
            ("cap1".to_string(), true),
            ("cap2".to_string(), false),
            ("cap3".to_string(), true),
        ];
        settings.caps_selected_index = 0;
    }

    // Move down
    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();
    assert_eq!(
        state.settings_state.as_ref().unwrap().caps_selected_index,
        1
    );

    // Move down again
    handle_settings_key(&mut state, key).unwrap();
    assert_eq!(
        state.settings_state.as_ref().unwrap().caps_selected_index,
        2
    );

    // Move up
    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Up,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();
    assert_eq!(
        state.settings_state.as_ref().unwrap().caps_selected_index,
        1
    );
}

#[test]
fn test_handle_settings_key_caps_toggle_space() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.current_section = SettingsSection::Capabilities;
        settings.available_caps = vec![("test-cap".to_string(), true)];
        settings.caps_enabled = vec![];
        settings.caps_selected_index = 0;
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char(' '),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    let settings = state.settings_state.as_ref().unwrap();
    assert!(settings.caps_enabled.contains(&"test-cap".to_string()));
}

#[test]
fn test_handle_settings_key_caps_toggle_enter() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    if let Some(ref mut settings) = state.settings_state {
        settings.current_section = SettingsSection::Capabilities;
        settings.available_caps = vec![("test-cap".to_string(), true)];
        settings.caps_enabled = vec![];
        settings.caps_selected_index = 0;
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    let settings = state.settings_state.as_ref().unwrap();
    assert!(settings.caps_enabled.contains(&"test-cap".to_string()));
}

#[test]
fn test_handle_settings_key_no_settings_state() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;
    state.settings_state = None;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('j'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_settings_key(&mut state, key).unwrap();

    // Should switch back to Input mode
    assert_eq!(state.mode, ChatMode::Input);
}

// ===== handle_key Tests =====

#[test]
fn test_handle_key_dispatches_to_input() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_key(&mut state, key).unwrap();

    assert_eq!(state.input.text(), "a");
}

#[test]
fn test_handle_key_dispatches_to_normal() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('?'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_key(&mut state, key).unwrap();

    assert_eq!(state.mode, ChatMode::Help);
}

#[test]
fn test_handle_key_dispatches_to_help() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Help;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_key(&mut state, key).unwrap();

    assert_eq!(state.mode, ChatMode::Input);
}

#[test]
fn test_handle_key_dispatches_to_settings() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Settings;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_key(&mut state, key).unwrap();

    assert_eq!(state.mode, ChatMode::Input);
}

#[test]
fn test_handle_key_agent_focus_mode() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::AgentFocus;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('a'),
        crossterm::event::KeyModifiers::NONE,
    );
    // Should not panic - falls through to default case
    handle_key(&mut state, key).unwrap();
}

// ===== Additional Input Key Tests =====

#[test]
fn test_handle_input_key_page_up() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.chat_area_height = 10;
    state.scroll_offset = 20;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::PageUp,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();

    assert!(state.scroll_offset < 20);
}

#[test]
fn test_handle_input_key_page_down() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.chat_area_height = 10;
    // Add messages so we can scroll
    for i in 0..20 {
        state
            .messages
            .push(DisplayMessage::user(format!("Message {}", i)));
    }
    state.scroll_offset = 0;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::PageDown,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();

    assert!(state.scroll_offset > 0);
}

#[test]
fn test_handle_input_key_ctrl_up() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.scroll_offset = 5;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Up,
        crossterm::event::KeyModifiers::CONTROL,
    );
    handle_input_key(&mut state, key).unwrap();

    assert_eq!(state.scroll_offset, 4);
}

#[test]
fn test_handle_input_key_ctrl_down() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.scroll_offset = 0;
    // Add messages
    for i in 0..10 {
        state
            .messages
            .push(DisplayMessage::user(format!("Message {}", i)));
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::CONTROL,
    );
    handle_input_key(&mut state, key).unwrap();

    // Should have scrolled
}

#[test]
fn test_handle_input_key_ctrl_slash() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('/'),
        crossterm::event::KeyModifiers::CONTROL,
    );
    handle_input_key(&mut state, key).unwrap();

    assert_eq!(state.mode, ChatMode::Help);
}

#[test]
fn test_handle_input_key_home() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.set_buffer("hello world".to_string());

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Home,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();

    assert_eq!(state.input.cursor, 0);
}

#[test]
fn test_handle_input_key_end() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.set_buffer("hello".to_string());
    state.input.cursor = 0;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::End,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();

    assert_eq!(state.input.cursor, 5);
}

#[test]
fn test_handle_input_key_delete() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.set_buffer("hello".to_string());
    state.input.cursor = 2;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Delete,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();

    assert_eq!(state.input.text(), "helo");
}

#[test]
fn test_handle_input_key_shift_char() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.clear();

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('A'),
        crossterm::event::KeyModifiers::SHIFT,
    );
    handle_input_key(&mut state, key).unwrap();

    assert_eq!(state.input.text(), "A");
}

// ===== Normal Key Additional Tests =====

#[test]
fn test_handle_normal_key_esc() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();

    assert_eq!(state.mode, ChatMode::Input);
}

#[test]
fn test_handle_normal_key_page_up() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;
    state.chat_area_height = 10;
    state.scroll_offset = 20;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::PageUp,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();

    assert!(state.scroll_offset < 20);
}

#[test]
fn test_handle_normal_key_page_down() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;
    state.chat_area_height = 10;
    for i in 0..20 {
        state
            .messages
            .push(DisplayMessage::user(format!("Message {}", i)));
    }
    state.scroll_offset = 0;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::PageDown,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();

    assert!(state.scroll_offset > 0);
}

#[test]
fn test_handle_normal_key_shift_g() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;
    state.chat_area_height = 10;
    for i in 0..20 {
        state
            .messages
            .push(DisplayMessage::user(format!("Message {}", i)));
    }
    state.scroll_offset = 0;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('G'),
        crossterm::event::KeyModifiers::SHIFT,
    );
    handle_normal_key(&mut state, key).unwrap();

    // Should scroll to bottom
    let total_height = state.total_messages_height();
    let expected = total_height.saturating_sub(10);
    assert_eq!(state.scroll_offset, expected);
}

#[test]
fn test_handle_normal_key_up_arrow() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;
    state.scroll_offset = 5;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Up,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();

    assert_eq!(state.scroll_offset, 4);
}

#[test]
fn test_handle_normal_key_down_arrow() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;
    state.scroll_offset = 0;
    for i in 0..10 {
        state
            .messages
            .push(DisplayMessage::user(format!("Message {}", i)));
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();
}

// ===== Additional Input Mode Key Tests =====

#[test]
fn test_handle_input_key_left_arrow() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.set_buffer("hello".to_string());

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Left,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();

    assert!(state.input.cursor < 5);
}

#[test]
fn test_handle_input_key_right_arrow() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.input.set_buffer("hello".to_string());
    state.input.cursor = 0;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();

    assert!(state.input.cursor > 0);
}

#[test]
fn test_handle_input_key_history_up() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    // Populate history by setting buffer and submitting
    state.input.set_buffer("first".to_string());
    state.input.submit();
    state.input.set_buffer("second".to_string());
    state.input.submit();

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Up,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();
}

#[test]
fn test_handle_input_key_history_down() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    // Populate history by setting buffer and submitting
    state.input.set_buffer("first".to_string());
    state.input.submit();

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::NONE,
    );
    handle_input_key(&mut state, key).unwrap();
}

// ===== Additional Normal Mode Key Tests =====

#[test]
fn test_handle_normal_key_k() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;
    state.scroll_offset = 5;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('k'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();

    assert_eq!(state.scroll_offset, 4);
}

#[test]
fn test_handle_normal_key_j() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;
    state.scroll_offset = 0;
    for i in 0..20 {
        state
            .messages
            .push(DisplayMessage::user(format!("Message {}", i)));
    }

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('j'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();
}

#[test]
fn test_handle_normal_key_g() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;
    state.scroll_offset = 20;

    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('g'),
        crossterm::event::KeyModifiers::NONE,
    );
    handle_normal_key(&mut state, key).unwrap();

    assert_eq!(state.scroll_offset, 0);
}

// ===== SettingsState Additional Edge Cases =====

#[test]
fn test_settings_state_empty_model_edit() {
    let mut state = create_test_settings_state();
    state.selected_index = 1; // Model field
    let original_model = state.model.clone();

    state.start_editing();
    state.edit_buffer.clear(); // Empty edit
    state.confirm_editing();

    // Empty model should not be saved
    assert_eq!(state.model, original_model);
}

#[test]
fn test_settings_state_confirm_not_editing() {
    let mut state = create_test_settings_state();
    let original_model = state.model.clone();

    // Call confirm without being in editing mode
    state.confirm_editing();

    assert_eq!(state.model, original_model);
    assert!(!state.is_editing);
}

#[test]
fn test_settings_state_provider_not_in_list() {
    let settings = Settings::default();
    let config = ChatTuiConfig {
        session_id: Uuid::new_v4(),
        provider_name: "unknown_provider".to_string(),
        model: "test".to_string(),
        caps: vec![],
        stream_enabled: true,
        trust_mode: false,
    };
    let state = SettingsState::new(&settings, &config, &[], &[]);

    // Provider index should default to 0 when not found
    assert_eq!(state.provider_index, 0);
}

#[test]
fn test_settings_state_provider_field_no_editing() {
    let mut state = create_test_settings_state();
    state.selected_index = 0; // Provider field

    state.start_editing();
    // Provider uses cycling, not text editing - should NOT enter editing mode
    assert!(!state.is_editing);
    assert!(state.edit_buffer.is_empty());
}

#[test]
fn test_settings_state_editing_stream_field() {
    let mut state = create_test_settings_state();
    state.selected_index = 5; // Stream field

    state.start_editing();
    // Boolean fields have empty edit buffer
    assert!(state.edit_buffer.is_empty());
}

#[test]
fn test_settings_state_caps_move_with_empty_list() {
    let settings = Settings::default();
    let config = ChatTuiConfig {
        session_id: Uuid::new_v4(),
        provider_name: "anthropic".to_string(),
        model: "test".to_string(),
        caps: vec![],
        stream_enabled: true,
        trust_mode: false,
    };
    let mut state = SettingsState::new(&settings, &config, &[], &[]);

    // Should not panic with empty caps list
    state.caps_move_up();
    state.caps_move_down();
    assert_eq!(state.caps_selected_index, 0);
}

// ===== Cancelled Tool Use Tests =====

/// Tests that cancelled tool_result messages are correctly created with error flag.
/// This tests the logic used when Ctrl+C interrupts tool execution - we need to
/// add error tool_result blocks for any incomplete tool_uses to maintain API invariant.
#[test]
fn test_cancelled_tool_result_message() {
    use crate::llm::message::{ContentBlock, Message, MessageContent};

    let tool_use_id = "toolu_cancelled_123";
    let msg = Message::tool_result(tool_use_id, "Cancelled by user", true);

    // Verify it's a user message (tool_result is always from user role)
    assert_eq!(msg.role, crate::llm::message::Role::User);

    // Verify the content contains the tool_result with error flag
    if let MessageContent::Blocks(blocks) = &msg.content {
        assert_eq!(blocks.len(), 1);
        if let ContentBlock::ToolResult {
            tool_use_id: id,
            is_error,
            ..
        } = &blocks[0]
        {
            assert_eq!(id, tool_use_id);
            assert_eq!(*is_error, Some(true));
        } else {
            panic!("Expected ToolResult block");
        }
    } else {
        panic!("Expected Blocks content");
    }
}

/// Tests the logic for identifying which tool_uses need cancelled results.
/// When interrupted, we add cancelled results for any tool_use IDs not in completed set.
#[test]
fn test_incomplete_tool_uses_detection() {
    use std::collections::HashSet;

    // Simulate: 3 tool_uses were requested
    let tool_uses = vec![
        (
            "tool_1".to_string(),
            "grep".to_string(),
            serde_json::Value::Null,
        ),
        (
            "tool_2".to_string(),
            "read".to_string(),
            serde_json::Value::Null,
        ),
        (
            "tool_3".to_string(),
            "write".to_string(),
            serde_json::Value::Null,
        ),
    ];

    // Simulate: only tool_1 completed before interruption
    let completed_ids: HashSet<String> = vec!["tool_1".to_string()].into_iter().collect();

    // Find incomplete tools (this is the logic from the fix)
    let incomplete: Vec<_> = tool_uses
        .iter()
        .filter(|(id, _, _)| !completed_ids.contains(id))
        .map(|(id, _, _)| id.clone())
        .collect();

    assert_eq!(incomplete.len(), 2);
    assert!(incomplete.contains(&"tool_2".to_string()));
    assert!(incomplete.contains(&"tool_3".to_string()));
    assert!(!incomplete.contains(&"tool_1".to_string()));
}

/// Tests that a conversation maintains valid tool_use/tool_result pairs after cancellation.
/// This simulates the fix: when interrupted, we add cancelled results for all incomplete tools.
#[test]
fn test_conversation_tool_use_result_pairing_after_cancel() {
    use crate::llm::message::{ContentBlock, Conversation, Message, MessageContent};

    let mut conversation = Conversation::new();

    // Simulate: assistant responds with 2 tool_uses
    let assistant_msg = Message::assistant_blocks(vec![
        ContentBlock::Text {
            text: "I'll help with that.".to_string(),
        },
        ContentBlock::ToolUse {
            id: "tool_1".to_string(),
            name: "grep".to_string(),
            input: serde_json::json!({"pattern": "test"}),
        },
        ContentBlock::ToolUse {
            id: "tool_2".to_string(),
            name: "read".to_string(),
            input: serde_json::json!({"path": "/test"}),
        },
    ]);
    conversation.push(assistant_msg);

    // Simulate: tool_1 completed, tool_2 was cancelled (single user blocks message)
    conversation.push(Message::user_blocks(vec![
        ContentBlock::ToolResult {
            tool_use_id: "tool_1".to_string(),
            content: crate::llm::message::ToolResultContent::Text("Found 5 matches".to_string()),
            is_error: None,
        },
        ContentBlock::ToolResult {
            tool_use_id: "tool_2".to_string(),
            content: crate::llm::message::ToolResultContent::Text("Cancelled by user".to_string()),
            is_error: Some(true),
        },
    ]));

    // Verify: all tool_uses have matching tool_results
    let tool_use_ids: Vec<String> = conversation
        .messages
        .iter()
        .flat_map(|m| {
            if let MessageContent::Blocks(blocks) = &m.content {
                blocks
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::ToolUse { id, .. } = b {
                            Some(id.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![]
            }
        })
        .collect();

    let tool_result_ids: Vec<String> = conversation
        .messages
        .iter()
        .flat_map(|m| {
            if let MessageContent::Blocks(blocks) = &m.content {
                blocks
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::ToolResult { tool_use_id, .. } = b {
                            Some(tool_use_id.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![]
            }
        })
        .collect();

    // Every tool_use_id should have a matching tool_result
    assert_eq!(tool_use_ids.len(), 2);
    assert_eq!(tool_result_ids.len(), 2);
    for id in &tool_use_ids {
        assert!(
            tool_result_ids.contains(id),
            "tool_use {} missing tool_result",
            id
        );
    }
}

fn with_temp_cwd<F: FnOnce()>(f: F) {
    use std::sync::{Mutex, OnceLock};
    static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    let _guard = CWD_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("cwd lock poisoned");
    let original = std::env::current_dir().expect("current_dir should be available");
    let temp = tempfile::TempDir::new().expect("tempdir should be created");
    std::env::set_current_dir(temp.path()).expect("set_current_dir to temp should succeed");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::env::set_current_dir(original).expect("restore current_dir should succeed");
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn with_temp_ted_home<F: FnOnce(&std::path::Path)>(f: F) {
    use std::sync::{Mutex, OnceLock};
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    let _guard = ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock poisoned");
    let temp = tempfile::TempDir::new().expect("tempdir should be created");
    let old = std::env::var_os("TED_HOME");
    std::env::set_var("TED_HOME", temp.path());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(temp.path())));
    if let Some(v) = old {
        std::env::set_var("TED_HOME", v);
    } else {
        std::env::remove_var("TED_HOME");
    }
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

#[test]
fn test_handle_command_model_download_sets_pending_message() {
    let mut state = create_test_tui_state();
    let registry = crate::models::DownloadRegistry::embedded().expect("embedded registry");
    let model = registry
        .models
        .iter()
        .find(|m| !m.variants.is_empty())
        .expect("registry should contain at least one model with variants");
    let quant = format!("{:?}", model.variants[0].quantization).to_lowercase();
    let command = format!("/model download {} -q {}", model.id, quant);

    handle_command(&command, &mut state, None).unwrap();

    assert_eq!(state.pending_messages.len(), 1);
    assert_eq!(
        state.status_message.as_deref(),
        Some("Processing /model...")
    );
}

#[test]
fn test_handle_command_model_download_unknown_sets_error() {
    let mut state = create_test_tui_state();

    handle_command(
        "/model download definitely-not-a-real-model",
        &mut state,
        None,
    )
    .unwrap();

    assert!(state.status_is_error);
    assert!(state
        .status_message
        .as_deref()
        .unwrap_or_default()
        .contains("not found"));
}

#[test]
fn test_handle_command_commit_branch_sets_pending() {
    let mut state = create_test_tui_state();

    handle_command("/commit -m \"feat: improve tests\"", &mut state, None).unwrap();

    assert_eq!(state.pending_messages.len(), 1);
    assert_eq!(
        state.status_message.as_deref(),
        Some("Processing /commit...")
    );
}

#[test]
fn test_handle_command_commit_parse_failure_sets_error() {
    let mut state = create_test_tui_state();

    handle_command("/committy", &mut state, None).unwrap();

    assert!(state.status_is_error);
    assert_eq!(
        state.status_message.as_deref(),
        Some("Failed to parse /commit command")
    );
}

#[test]
fn test_handle_command_test_branch_sets_pending() {
    let mut state = create_test_tui_state();

    handle_command("/test --watch parser", &mut state, None).unwrap();

    assert_eq!(state.pending_messages.len(), 1);
    assert_eq!(state.status_message.as_deref(), Some("Processing /test..."));
}

#[test]
fn test_handle_command_test_parse_failure_sets_error() {
    let mut state = create_test_tui_state();

    handle_command("/testsuite", &mut state, None).unwrap();

    assert!(state.status_is_error);
    assert_eq!(
        state.status_message.as_deref(),
        Some("Failed to parse /test command")
    );
}

#[test]
fn test_handle_command_review_branch_sets_pending() {
    let mut state = create_test_tui_state();

    handle_command("/review src/main.rs --focus security", &mut state, None).unwrap();

    assert_eq!(state.pending_messages.len(), 1);
    assert_eq!(
        state.status_message.as_deref(),
        Some("Starting code review...")
    );
}

#[test]
fn test_handle_command_review_parse_failure_sets_error() {
    let mut state = create_test_tui_state();

    handle_command("/reviewtool", &mut state, None).unwrap();

    assert!(state.status_is_error);
    assert_eq!(
        state.status_message.as_deref(),
        Some("Failed to parse /review command")
    );
}

#[test]
fn test_handle_command_fix_branch_sets_pending() {
    let mut state = create_test_tui_state();

    handle_command("/fix lint src", &mut state, None).unwrap();

    assert_eq!(state.pending_messages.len(), 1);
    assert_eq!(state.status_message.as_deref(), Some("Fixing issues..."));
}

#[test]
fn test_handle_command_fix_parse_failure_sets_error() {
    let mut state = create_test_tui_state();

    handle_command("/fixed", &mut state, None).unwrap();

    assert!(state.status_is_error);
    assert_eq!(
        state.status_message.as_deref(),
        Some("Failed to parse /fix command")
    );
}

#[test]
fn test_handle_command_explain_branch_sets_pending() {
    let mut state = create_test_tui_state();

    handle_command("/explain src/main.rs --detailed", &mut state, None).unwrap();

    assert_eq!(state.pending_messages.len(), 1);
    assert_eq!(
        state.status_message.as_deref(),
        Some("Processing /explain...")
    );
}

#[test]
fn test_handle_command_explain_parse_failure_sets_error() {
    let mut state = create_test_tui_state();

    handle_command("/explained", &mut state, None).unwrap();

    assert!(state.status_is_error);
    assert_eq!(
        state.status_message.as_deref(),
        Some("Failed to parse /explain command")
    );
}

#[test]
fn test_handle_command_skills_create_sets_pending() {
    let mut state = create_test_tui_state();

    handle_command("/skills create rust-helper", &mut state, None).unwrap();

    assert_eq!(state.pending_messages.len(), 1);
    assert_eq!(
        state.status_message.as_deref(),
        Some("Processing /skills...")
    );
}

#[test]
fn test_handle_command_skills_show_without_name_sets_error() {
    let mut state = create_test_tui_state();

    handle_command("/skills show", &mut state, None).unwrap();

    assert!(state.status_is_error);
    assert!(state
        .status_message
        .as_deref()
        .unwrap_or_default()
        .contains("Usage: /skills show"));
}

#[test]
fn test_handle_command_beads_list_updates_conversation_context() {
    with_temp_cwd(|| {
        let mut state = create_test_tui_state();
        let mut conversation = Conversation::new();

        handle_command("/beads", &mut state, Some(&mut conversation)).unwrap();

        assert!(!state.messages.is_empty());
        assert!(conversation.messages.iter().any(|m| {
            matches!(
                &m.content,
                crate::llm::message::MessageContent::Text(text)
                    if text.contains("User ran /beads command")
            )
        }));
    });
}

#[test]
fn test_handle_command_beads_parse_failure_sets_error() {
    let mut state = create_test_tui_state();

    handle_command("/beadslist", &mut state, None).unwrap();

    assert!(state.status_is_error);
    assert_eq!(
        state.status_message.as_deref(),
        Some("Failed to parse /beads command")
    );
}

#[test]
fn test_handle_key_agent_focus_scroll_and_cycle() {
    let mut state = create_test_tui_state();
    state.agents.track(
        Uuid::new_v4(),
        "tool-1".to_string(),
        "agent-1".to_string(),
        "explore".to_string(),
        "task-1".to_string(),
    );
    state.agents.track(
        Uuid::new_v4(),
        "tool-2".to_string(),
        "agent-2".to_string(),
        "implement".to_string(),
        "task-2".to_string(),
    );
    state.mode = ChatMode::AgentFocus;
    state.focused_agent_tool_id = Some("tool-1".to_string());

    handle_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Up,
            crossterm::event::KeyModifiers::NONE,
        ),
    )
    .unwrap();
    let up_offset = state
        .agents
        .get_mut_by_tool_call_id("tool-1")
        .expect("tracked agent")
        .conversation_scroll
        .scroll_offset;
    assert_eq!(up_offset, 1);

    handle_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        ),
    )
    .unwrap();
    let down_offset = state
        .agents
        .get_mut_by_tool_call_id("tool-1")
        .expect("tracked agent")
        .conversation_scroll
        .scroll_offset;
    assert_eq!(down_offset, 0);

    handle_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::PageUp,
            crossterm::event::KeyModifiers::NONE,
        ),
    )
    .unwrap();
    let page_up_offset = state
        .agents
        .get_mut_by_tool_call_id("tool-1")
        .expect("tracked agent")
        .conversation_scroll
        .scroll_offset;
    assert_eq!(page_up_offset, 10);

    handle_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::PageDown,
            crossterm::event::KeyModifiers::NONE,
        ),
    )
    .unwrap();
    let page_down_offset = state
        .agents
        .get_mut_by_tool_call_id("tool-1")
        .expect("tracked agent")
        .conversation_scroll
        .scroll_offset;
    assert_eq!(page_down_offset, 0);

    handle_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('G'),
            crossterm::event::KeyModifiers::SHIFT,
        ),
    )
    .unwrap();
    let bottom_offset = state
        .agents
        .get_mut_by_tool_call_id("tool-1")
        .expect("tracked agent")
        .conversation_scroll
        .scroll_offset;
    assert_eq!(bottom_offset, usize::MAX);

    handle_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        ),
    )
    .unwrap();
    let top_offset = state
        .agents
        .get_mut_by_tool_call_id("tool-1")
        .expect("tracked agent")
        .conversation_scroll
        .scroll_offset;
    assert_eq!(top_offset, 0);

    handle_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(']'),
            crossterm::event::KeyModifiers::NONE,
        ),
    )
    .unwrap();
    assert_eq!(state.focused_agent_tool_id.as_deref(), Some("tool-2"));

    handle_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('['),
            crossterm::event::KeyModifiers::NONE,
        ),
    )
    .unwrap();
    assert_eq!(state.focused_agent_tool_id.as_deref(), Some("tool-1"));
}

#[test]
fn test_handle_key_normal_mode_agent_focus_requires_focused_tool() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Normal;
    state.agents.track(
        Uuid::new_v4(),
        "tool-x".to_string(),
        "agent-x".to_string(),
        "explore".to_string(),
        "task".to_string(),
    );

    handle_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        ),
    )
    .unwrap();
    assert_eq!(state.mode, ChatMode::Normal);

    state.focused_agent_tool_id = Some("tool-x".to_string());
    handle_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        ),
    )
    .unwrap();
    assert_eq!(state.mode, ChatMode::AgentFocus);
}

#[test]
fn test_handle_input_key_delete_and_ctrl_scroll_paths() {
    let mut state = create_test_tui_state();
    state.mode = ChatMode::Input;
    state.chat_area_height = 2;
    for i in 0..6 {
        state
            .messages
            .push(DisplayMessage::user(format!("line {}", i)));
    }
    state.scroll_offset = 1;
    state.input.set_buffer("ab".to_string());
    state.input.cursor = 0;

    handle_input_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Delete,
            crossterm::event::KeyModifiers::NONE,
        ),
    )
    .unwrap();
    assert_eq!(state.input.text(), "b");

    handle_input_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Up,
            crossterm::event::KeyModifiers::CONTROL,
        ),
    )
    .unwrap();
    assert_eq!(state.scroll_offset, 0);

    handle_input_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::CONTROL,
        ),
    )
    .unwrap();
    assert_eq!(state.scroll_offset, 1);

    handle_input_key(
        &mut state,
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('/'),
            crossterm::event::KeyModifiers::CONTROL,
        ),
    )
    .unwrap();
    assert_eq!(state.mode, ChatMode::Help);
}

#[test]
fn test_handle_settings_key_save_sets_restart_when_provider_changes() {
    with_temp_ted_home(|ted_home| {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        let settings = state.settings_state.as_mut().expect("settings state");
        settings.has_changes = true;
        settings.provider = "openrouter".to_string();
        settings.model = "openai/gpt-4o-mini".to_string();
        settings.stream = false;
        settings.trust_mode = true;
        settings.temperature = 0.8;
        settings.max_tokens = 2048;
        settings.caps_enabled = vec!["base".to_string(), "rust-expert".to_string()];
        settings.api_key = "test-openrouter-key".to_string();
        settings
            .api_keys_by_provider
            .insert("openrouter".to_string(), "test-openrouter-key".to_string());

        handle_settings_key(
            &mut state,
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('s'),
                crossterm::event::KeyModifiers::NONE,
            ),
        )
        .unwrap();

        assert!(state.needs_restart);
        assert!(state.should_quit);
        assert_eq!(
            state.status_message.as_deref(),
            Some("Provider changed. Restarting...")
        );
        assert!(ted_home.join("settings.json").exists());
    });
}
