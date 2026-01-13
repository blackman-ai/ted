// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Integration tests for TUI components
//!
//! These tests exercise the TUI logic without requiring an actual terminal.

use ted::config::Settings;
use ted::tui::app::{App, AppResult, InputMode, Screen};
use ted::tui::app::{CapDisplayInfo, CapsMenuItem, ContextItem, MainMenuItem, ProviderItem};

// ===== Full Flow Integration Tests =====

#[test]
fn test_full_navigation_flow_through_all_screens() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // Start at main menu
    assert_eq!(app.screen, Screen::MainMenu);

    // Navigate to Providers
    app.main_menu_index = 0;
    app.select();
    assert_eq!(app.screen, Screen::Providers);

    // Go back
    app.go_back();
    assert_eq!(app.screen, Screen::MainMenu);

    // Navigate to Caps
    app.main_menu_index = 1;
    app.select();
    assert_eq!(app.screen, Screen::Caps);

    // Go back
    app.go_back();
    assert_eq!(app.screen, Screen::MainMenu);

    // Navigate to Plans
    app.main_menu_index = 2;
    app.select();
    assert_eq!(app.screen, Screen::Plans);

    // Go back
    app.go_back();
    assert_eq!(app.screen, Screen::MainMenu);

    // Navigate to Context
    app.main_menu_index = 3;
    app.select();
    assert_eq!(app.screen, Screen::Context);

    // Go back
    app.go_back();
    assert_eq!(app.screen, Screen::MainMenu);

    // Navigate to About
    app.main_menu_index = 4;
    app.select();
    assert_eq!(app.screen, Screen::About);

    // Go back
    app.go_back();
    assert_eq!(app.screen, Screen::MainMenu);
}

#[test]
fn test_provider_settings_modification_flow() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // Navigate to Providers screen
    app.go_to(Screen::Providers);

    // Select API Key (index 1 - after DefaultProvider)
    app.provider_index = 1;
    app.select();
    assert_eq!(app.input_mode, InputMode::Editing);

    // Type a new API key
    app.input_buffer = "sk-ant-api03-test-key-12345".to_string();
    app.confirm_edit();

    // Verify the change was applied
    assert_eq!(
        app.settings.providers.anthropic.api_key,
        Some("sk-ant-api03-test-key-12345".to_string())
    );
    assert!(app.settings_modified);
    assert_eq!(app.input_mode, InputMode::Normal);
}

#[test]
fn test_context_settings_modification_flow() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // Navigate to Context screen
    app.go_to(Screen::Context);

    // Modify max warm chunks
    app.context_index = 0;
    app.select();
    app.input_buffer = "500".to_string();
    app.confirm_edit();

    assert_eq!(app.settings.context.max_warm_chunks, 500);
    assert!(app.settings_modified);

    // Modify cold retention days
    app.context_index = 1;
    app.select();
    app.input_buffer = "90".to_string();
    app.confirm_edit();

    assert_eq!(app.settings.context.cold_retention_days, 90);

    // Toggle auto compact
    let original_auto_compact = app.settings.context.auto_compact;
    app.context_index = 2;
    app.select();
    assert_ne!(app.settings.context.auto_compact, original_auto_compact);
}

#[test]
fn test_caps_toggle_flow() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.go_to(Screen::Caps);

    // If there are caps, test toggling them
    if !app.available_caps.is_empty() {
        let cap_name = app.available_caps[0].name.clone();
        let original_enabled = app.available_caps[0].is_enabled;

        // Toggle the cap
        app.caps_index = 0;
        app.toggle_cap(0);

        // Verify it toggled
        assert_ne!(app.available_caps[0].is_enabled, original_enabled);

        // Verify settings were updated
        let is_in_defaults = app.settings.defaults.caps.contains(&cap_name);
        if original_enabled {
            assert!(!is_in_defaults);
        } else {
            assert!(is_in_defaults);
        }
    }
}

#[test]
fn test_editing_cancellation_preserves_original_value() {
    let mut settings = Settings::default();
    settings.providers.anthropic.api_key = Some("original-key".to_string());
    let mut app = App::new(settings);

    app.go_to(Screen::Providers);
    app.provider_index = 1; // AnthropicApiKey
    app.select();

    // Start typing a new value
    app.input_buffer = "new-key-that-will-be-cancelled".to_string();

    // Cancel editing
    app.cancel_editing();

    // Original value should be preserved
    assert_eq!(
        app.settings.providers.anthropic.api_key,
        Some("original-key".to_string())
    );
    assert!(!app.settings_modified);
}

#[test]
fn test_status_message_lifecycle() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // Set a status message
    app.set_status("Test status", false);
    assert_eq!(app.status_message.as_ref().unwrap(), "Test status");
    assert!(!app.status_is_error);

    // Set an error status
    app.set_status("Error occurred", true);
    assert_eq!(app.status_message.as_ref().unwrap(), "Error occurred");
    assert!(app.status_is_error);

    // Clear status
    app.clear_status();
    assert!(app.status_message.is_none());
    assert!(!app.status_is_error);
}

#[test]
fn test_navigation_wrapping_in_all_screens() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // Main menu wrapping
    app.screen = Screen::MainMenu;
    app.main_menu_index = 0;
    app.move_up();
    assert_eq!(app.main_menu_index, MainMenuItem::all().len() - 1);
    app.move_down();
    assert_eq!(app.main_menu_index, 0);

    // Providers screen wrapping
    app.screen = Screen::Providers;
    app.provider_index = 0;
    app.move_up();
    assert_eq!(app.provider_index, ProviderItem::all().len() - 1);
    app.move_down();
    assert_eq!(app.provider_index, 0);

    // Context screen wrapping
    app.screen = Screen::Context;
    app.context_index = 0;
    app.move_up();
    assert_eq!(app.context_index, ContextItem::all().len() - 1);
    app.move_down();
    assert_eq!(app.context_index, 0);

    // Caps screen wrapping
    app.screen = Screen::Caps;
    app.caps_index = 0;
    let total = app.caps_total_items();
    app.move_up();
    assert_eq!(app.caps_index, total - 1);
    app.move_down();
    assert_eq!(app.caps_index, 0);
}

#[test]
fn test_invalid_input_handling() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.go_to(Screen::Context);

    // Try to set max_warm_chunks to invalid value
    app.context_index = 0;
    app.select();
    app.input_buffer = "not-a-number".to_string();
    let original = app.settings.context.max_warm_chunks;
    app.confirm_edit();

    // Value should be unchanged
    assert_eq!(app.settings.context.max_warm_chunks, original);
    // Should show error
    assert!(app.status_is_error);
}

#[test]
fn test_empty_input_handling() {
    let mut settings = Settings::default();
    settings.providers.anthropic.api_key = Some("existing-key".to_string());
    let mut app = App::new(settings);

    app.go_to(Screen::Providers);

    // Clear the API key with empty input
    app.provider_index = 1; // AnthropicApiKey
    app.select();
    app.input_buffer = String::new();
    app.confirm_edit();

    // Empty should clear the key
    assert_eq!(app.settings.providers.anthropic.api_key, None);
}

#[test]
fn test_model_empty_input_ignored() {
    let settings = Settings::default();
    let original_model = settings.providers.anthropic.default_model.clone();
    let mut app = App::new(settings);

    app.go_to(Screen::Providers);

    // Try to set model to empty
    app.provider_index = 2; // AnthropicModel
    app.select();
    app.input_buffer = String::new();
    app.confirm_edit();

    // Empty model should be ignored (model is required)
    assert_eq!(
        app.settings.providers.anthropic.default_model,
        original_model
    );
}

// ===== AppResult Tests =====

#[test]
fn test_app_result_variants() {
    let continue_result = AppResult::Continue;
    let quit_result = AppResult::Quit;

    assert!(matches!(continue_result, AppResult::Continue));
    assert!(matches!(quit_result, AppResult::Quit));
}

// ===== CapDisplayInfo Tests =====

#[test]
fn test_cap_display_info_full_lifecycle() {
    let cap = CapDisplayInfo {
        name: "test-cap".to_string(),
        description: "A test capability".to_string(),
        is_builtin: true,
        is_enabled: false,
    };

    assert_eq!(cap.name, "test-cap");
    assert_eq!(cap.description, "A test capability");
    assert!(cap.is_builtin);
    assert!(!cap.is_enabled);

    // Test clone
    let cloned = cap.clone();
    assert_eq!(cloned.name, cap.name);
    assert_eq!(cloned.is_builtin, cap.is_builtin);
}

// ===== Menu Item Tests =====

#[test]
fn test_main_menu_items_exhaustive() {
    let items = MainMenuItem::all();
    assert_eq!(items.len(), 5); // Providers, Caps, Plans, Context, About

    for item in items {
        // Each item should have a non-empty label and description
        assert!(!item.label().is_empty());
        assert!(!item.description().is_empty());
    }
}

#[test]
fn test_provider_items_exhaustive() {
    let items = ProviderItem::all();
    assert_eq!(items.len(), 7); // DefaultProvider, AnthropicApiKey, AnthropicModel, OllamaBaseUrl, OllamaModel, TestConnection, Back

    for item in items {
        assert!(!item.label().is_empty());
    }
}

#[test]
fn test_context_items_exhaustive() {
    let items = ContextItem::all();
    assert_eq!(items.len(), 4);

    for item in items {
        assert!(!item.label().is_empty());
    }
}

#[test]
fn test_caps_menu_items() {
    // CapsMenuItem should have CreateNew and Back
    assert!(matches!(CapsMenuItem::CreateNew, CapsMenuItem::CreateNew));
    assert!(matches!(CapsMenuItem::Back, CapsMenuItem::Back));
    assert_ne!(
        std::mem::discriminant(&CapsMenuItem::CreateNew),
        std::mem::discriminant(&CapsMenuItem::Back)
    );
}

// ===== Screen Tests =====

#[test]
fn test_screen_all_variants() {
    let screens = [
        Screen::MainMenu,
        Screen::Providers,
        Screen::Caps,
        Screen::Context,
        Screen::About,
        Screen::Plans,
        Screen::PlanView,
    ];

    for screen in screens {
        // Each screen should be copyable and comparable
        let copied = screen;
        assert_eq!(screen, copied);
    }
}

// ===== InputMode Tests =====

#[test]
fn test_input_mode_transitions() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    assert_eq!(app.input_mode, InputMode::Normal);

    app.start_editing("test");
    assert_eq!(app.input_mode, InputMode::Editing);

    app.cancel_editing();
    assert_eq!(app.input_mode, InputMode::Normal);

    app.start_editing("another test");
    assert_eq!(app.input_mode, InputMode::Editing);

    // Simulate confirming edit on providers screen
    app.screen = Screen::Providers;
    app.provider_index = 1; // AnthropicApiKey
    app.confirm_edit();
    assert_eq!(app.input_mode, InputMode::Normal);
}

// ===== Multiple Modifications Test =====

#[test]
fn test_multiple_settings_modifications() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // Modify API key
    app.go_to(Screen::Providers);
    app.provider_index = 1; // AnthropicApiKey
    app.start_editing("test-api-key");
    app.confirm_edit();

    // Modify model
    app.provider_index = 2; // AnthropicModel
    app.start_editing("claude-3-5-haiku-20241022");
    app.confirm_edit();

    // Modify context settings
    app.go_to(Screen::Context);
    app.context_index = 0;
    app.start_editing("250");
    app.confirm_edit();

    // Verify all changes
    assert_eq!(
        app.settings.providers.anthropic.api_key,
        Some("test-api-key".to_string())
    );
    assert_eq!(
        app.settings.providers.anthropic.default_model,
        "claude-3-5-haiku-20241022"
    );
    assert_eq!(app.settings.context.max_warm_chunks, 250);
    assert!(app.settings_modified);
}

// ===== Edge Cases =====

#[test]
fn test_rapid_navigation() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // Rapidly navigate through all screens
    for _ in 0..10 {
        app.move_down();
        app.move_down();
        app.select();
        app.go_back();
        app.move_up();
    }

    // Should still be in a valid state
    assert_eq!(app.screen, Screen::MainMenu);
}

#[test]
fn test_navigation_from_about_screen() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.go_to(Screen::About);

    // About screen select should go back
    app.select();
    assert_eq!(app.screen, Screen::MainMenu);
}

#[test]
fn test_go_to_resets_input_mode() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // Start editing
    app.start_editing("test");
    assert_eq!(app.input_mode, InputMode::Editing);

    // Go to another screen
    app.go_to(Screen::Caps);

    // Input mode should be reset
    assert_eq!(app.input_mode, InputMode::Normal);
}

#[test]
fn test_go_to_resets_state_on_screen_change() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // Start editing with some content
    app.start_editing("test content");
    assert_eq!(app.input_buffer, "test content");
    assert_eq!(app.input_mode, InputMode::Editing);

    // Go to another screen - resets input mode
    app.go_to(Screen::Providers);

    // Input mode should be reset to Normal
    assert_eq!(app.input_mode, InputMode::Normal);
    // The buffer content is preserved (this is the actual behavior)
    // User can clear it via cancel_editing if needed
}
