// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use super::*;

// ===== AppResult Tests =====

#[test]
fn test_app_result_continue() {
    let result = AppResult::Continue;
    matches!(result, AppResult::Continue);
}

#[test]
fn test_app_result_quit() {
    let result = AppResult::Quit;
    matches!(result, AppResult::Quit);
}

// ===== Screen Tests =====

#[test]
fn test_screen_equality() {
    assert_eq!(Screen::MainMenu, Screen::MainMenu);
    assert_eq!(Screen::Providers, Screen::Providers);
    assert_eq!(Screen::Caps, Screen::Caps);
    assert_eq!(Screen::Context, Screen::Context);
    assert_eq!(Screen::About, Screen::About);
}

#[test]
fn test_screen_inequality() {
    assert_ne!(Screen::MainMenu, Screen::Providers);
    assert_ne!(Screen::Caps, Screen::Context);
}

#[test]
fn test_screen_debug() {
    let screen = Screen::MainMenu;
    assert!(format!("{:?}", screen).contains("MainMenu"));
}

#[test]
fn test_screen_clone() {
    let screen = Screen::Providers;
    let cloned = screen;
    assert_eq!(screen, cloned);
}

// ===== InputMode Tests =====

#[test]
fn test_input_mode_equality() {
    assert_eq!(InputMode::Normal, InputMode::Normal);
    assert_eq!(InputMode::Editing, InputMode::Editing);
}

#[test]
fn test_input_mode_inequality() {
    assert_ne!(InputMode::Normal, InputMode::Editing);
}

#[test]
fn test_input_mode_debug() {
    let mode = InputMode::Editing;
    assert!(format!("{:?}", mode).contains("Editing"));
}

// ===== MainMenuItem Tests =====

#[test]
fn test_main_menu_item_all() {
    let items = MainMenuItem::all();
    assert_eq!(items.len(), 5);
    assert_eq!(items[0], MainMenuItem::Providers);
    assert_eq!(items[1], MainMenuItem::Caps);
    assert_eq!(items[2], MainMenuItem::Plans);
    assert_eq!(items[3], MainMenuItem::Context);
    assert_eq!(items[4], MainMenuItem::About);
}

#[test]
fn test_main_menu_item_label() {
    assert_eq!(MainMenuItem::Providers.label(), "Providers");
    assert_eq!(MainMenuItem::Caps.label(), "Caps");
    assert_eq!(MainMenuItem::Plans.label(), "Plans");
    assert_eq!(MainMenuItem::Context.label(), "Context");
    assert_eq!(MainMenuItem::About.label(), "About");
}

#[test]
fn test_main_menu_item_description() {
    assert!(!MainMenuItem::Providers.description().is_empty());
    assert!(MainMenuItem::Providers.description().contains("API"));
    assert!(MainMenuItem::Caps.description().contains("persona"));
    assert!(MainMenuItem::Plans.description().contains("plans"));
    assert!(MainMenuItem::Context.description().contains("Storage"));
    assert!(MainMenuItem::About.description().contains("Version"));
}

#[test]
fn test_main_menu_item_equality() {
    assert_eq!(MainMenuItem::Providers, MainMenuItem::Providers);
    assert_ne!(MainMenuItem::Providers, MainMenuItem::Caps);
}

// ===== ProviderItem Tests =====

#[test]
fn test_provider_item_all() {
    let items = ProviderItem::all();
    assert_eq!(items.len(), 11);
    assert_eq!(items[0], ProviderItem::DefaultProvider);
    assert_eq!(items[1], ProviderItem::AnthropicApiKey);
    assert_eq!(items[2], ProviderItem::AnthropicModel);
    assert_eq!(items[3], ProviderItem::LocalPort);
    assert_eq!(items[4], ProviderItem::LocalModel);
    assert_eq!(items[5], ProviderItem::OpenRouterApiKey);
    assert_eq!(items[6], ProviderItem::OpenRouterModel);
    assert_eq!(items[7], ProviderItem::BlackmanApiKey);
    assert_eq!(items[8], ProviderItem::BlackmanModel);
    assert_eq!(items[9], ProviderItem::TestConnection);
    assert_eq!(items[10], ProviderItem::Back);
}

#[test]
fn test_provider_item_label() {
    assert_eq!(ProviderItem::DefaultProvider.label(), "Default Provider");
    assert_eq!(ProviderItem::AnthropicApiKey.label(), "Anthropic API Key");
    assert_eq!(ProviderItem::AnthropicModel.label(), "Anthropic Model");
    assert_eq!(ProviderItem::LocalPort.label(), "Local Port");
    assert_eq!(ProviderItem::LocalModel.label(), "Local Model");
    assert_eq!(ProviderItem::TestConnection.label(), "Test Connection");
    assert_eq!(ProviderItem::Back.label(), "← Back");
}

#[test]
fn test_provider_item_equality() {
    assert_eq!(ProviderItem::DefaultProvider, ProviderItem::DefaultProvider);
    assert_ne!(ProviderItem::DefaultProvider, ProviderItem::AnthropicApiKey);
}

// ===== ContextItem Tests =====

#[test]
fn test_context_item_all() {
    let items = ContextItem::all();
    assert_eq!(items.len(), 4);
    assert_eq!(items[0], ContextItem::MaxWarmChunks);
    assert_eq!(items[1], ContextItem::ColdRetentionDays);
    assert_eq!(items[2], ContextItem::AutoCompact);
    assert_eq!(items[3], ContextItem::Back);
}

#[test]
fn test_context_item_label() {
    assert_eq!(ContextItem::MaxWarmChunks.label(), "Max Warm Chunks");
    assert_eq!(
        ContextItem::ColdRetentionDays.label(),
        "Cold Retention (days)"
    );
    assert_eq!(ContextItem::AutoCompact.label(), "Auto Compact");
    assert_eq!(ContextItem::Back.label(), "← Back");
}

#[test]
fn test_context_item_equality() {
    assert_eq!(ContextItem::MaxWarmChunks, ContextItem::MaxWarmChunks);
    assert_ne!(ContextItem::MaxWarmChunks, ContextItem::Back);
}

// ===== CapDisplayInfo Tests =====

#[test]
fn test_cap_display_info_creation() {
    let cap = CapDisplayInfo {
        name: "test-cap".to_string(),
        description: "A test cap".to_string(),
        is_builtin: true,
        is_enabled: false,
    };
    assert_eq!(cap.name, "test-cap");
    assert_eq!(cap.description, "A test cap");
    assert!(cap.is_builtin);
    assert!(!cap.is_enabled);
}

#[test]
fn test_cap_display_info_clone() {
    let cap = CapDisplayInfo {
        name: "my-cap".to_string(),
        description: "My cap".to_string(),
        is_builtin: false,
        is_enabled: true,
    };
    let cloned = cap.clone();
    assert_eq!(cloned.name, cap.name);
    assert_eq!(cloned.description, cap.description);
    assert_eq!(cloned.is_builtin, cap.is_builtin);
    assert_eq!(cloned.is_enabled, cap.is_enabled);
}

#[test]
fn test_cap_display_info_debug() {
    let cap = CapDisplayInfo {
        name: "test".to_string(),
        description: "desc".to_string(),
        is_builtin: true,
        is_enabled: true,
    };
    let debug = format!("{:?}", cap);
    assert!(debug.contains("CapDisplayInfo"));
    assert!(debug.contains("test"));
}

// ===== CapsMenuItem Tests =====

#[test]
fn test_caps_menu_item_equality() {
    assert_eq!(CapsMenuItem::CreateNew, CapsMenuItem::CreateNew);
    assert_eq!(CapsMenuItem::Back, CapsMenuItem::Back);
    assert_ne!(CapsMenuItem::CreateNew, CapsMenuItem::Back);
}

// ===== App Tests =====

#[test]
fn test_app_new() {
    let settings = Settings::default();
    let app = App::new(settings);

    assert_eq!(app.screen, Screen::MainMenu);
    assert_eq!(app.input_mode, InputMode::Normal);
    assert_eq!(app.main_menu_index, 0);
    assert_eq!(app.provider_index, 0);
    assert_eq!(app.context_index, 0);
    assert_eq!(app.caps_index, 0);
    assert!(app.input_buffer.is_empty());
    assert!(app.status_message.is_none());
    assert!(!app.status_is_error);
    assert!(!app.settings_modified);
}

#[test]
fn test_app_go_to() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.go_to(Screen::Providers);
    assert_eq!(app.screen, Screen::Providers);
    assert_eq!(app.input_mode, InputMode::Normal);

    app.go_to(Screen::Caps);
    assert_eq!(app.screen, Screen::Caps);

    app.go_to(Screen::Context);
    assert_eq!(app.screen, Screen::Context);

    app.go_to(Screen::About);
    assert_eq!(app.screen, Screen::About);
}

#[test]
fn test_app_go_back() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.go_to(Screen::Providers);
    app.go_back();
    assert_eq!(app.screen, Screen::MainMenu);
}

#[test]
fn test_app_set_status() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.set_status("Test message", false);
    assert_eq!(app.status_message.as_ref().unwrap(), "Test message");
    assert!(!app.status_is_error);

    app.set_status("Error message", true);
    assert_eq!(app.status_message.as_ref().unwrap(), "Error message");
    assert!(app.status_is_error);
}

#[test]
fn test_app_clear_status() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.set_status("Test", false);
    app.clear_status();
    assert!(app.status_message.is_none());
    assert!(!app.status_is_error);
}

#[test]
fn test_app_start_editing() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.start_editing("initial value");
    assert_eq!(app.input_mode, InputMode::Editing);
    assert_eq!(app.input_buffer, "initial value");
}

#[test]
fn test_app_cancel_editing() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.start_editing("some value");
    app.cancel_editing();
    assert_eq!(app.input_mode, InputMode::Normal);
    assert!(app.input_buffer.is_empty());
}

#[test]
fn test_app_mark_modified() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    assert!(!app.settings_modified);
    app.mark_modified();
    assert!(app.settings_modified);
    assert!(app.status_message.is_some());
}

#[test]
fn test_app_move_up_main_menu() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    assert_eq!(app.main_menu_index, 0);
    app.move_up(); // Should wrap to last item
    assert_eq!(app.main_menu_index, MainMenuItem::all().len() - 1);

    app.move_up();
    assert_eq!(app.main_menu_index, MainMenuItem::all().len() - 2);
}

#[test]
fn test_app_move_down_main_menu() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    assert_eq!(app.main_menu_index, 0);
    app.move_down();
    assert_eq!(app.main_menu_index, 1);

    app.move_down();
    assert_eq!(app.main_menu_index, 2);
}

#[test]
fn test_app_move_down_wraps() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.main_menu_index = MainMenuItem::all().len() - 1;
    app.move_down(); // Should wrap to 0
    assert_eq!(app.main_menu_index, 0);
}

#[test]
fn test_app_move_up_providers() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;

    assert_eq!(app.provider_index, 0);
    app.move_up(); // Should wrap to last
    assert_eq!(app.provider_index, ProviderItem::all().len() - 1);
}

#[test]
fn test_app_move_down_providers() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;

    app.move_down();
    assert_eq!(app.provider_index, 1);
}

#[test]
fn test_app_move_up_context() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Context;

    assert_eq!(app.context_index, 0);
    app.move_up(); // Should wrap
    assert_eq!(app.context_index, ContextItem::all().len() - 1);
}

#[test]
fn test_app_move_down_context() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Context;

    app.move_down();
    assert_eq!(app.context_index, 1);
}

#[test]
fn test_app_move_up_caps() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Caps;

    // Caps screen includes available_caps + 2 menu items
    let total = app.caps_total_items();
    assert_eq!(app.caps_index, 0);
    app.move_up(); // Should wrap to last
    assert_eq!(app.caps_index, total - 1);
}

#[test]
fn test_app_move_down_caps() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Caps;

    app.move_down();
    assert_eq!(app.caps_index, 1);
}

#[test]
fn test_app_move_up_about_no_change() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::About;

    // About screen doesn't have navigation
    app.move_up();
    app.move_down();
    // Just verify it doesn't panic
}

#[test]
fn test_app_caps_total_items() {
    let settings = Settings::default();
    let app = App::new(settings);

    // Total items = available_caps.len() + 2 (Create New, Back)
    let expected = app.available_caps.len() + 2;
    assert_eq!(app.caps_total_items(), expected);
}

#[test]
fn test_app_select_main_menu_providers() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.main_menu_index = 0; // Providers
    app.select();
    assert_eq!(app.screen, Screen::Providers);
}

#[test]
fn test_app_select_main_menu_caps() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.main_menu_index = 1; // Caps
    app.select();
    assert_eq!(app.screen, Screen::Caps);
}

#[test]
fn test_app_select_main_menu_plans() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.main_menu_index = 2; // Plans
    app.select();
    assert_eq!(app.screen, Screen::Plans);
}

#[test]
fn test_app_select_main_menu_context() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.main_menu_index = 3; // Context
    app.select();
    assert_eq!(app.screen, Screen::Context);
}

#[test]
fn test_app_select_main_menu_about() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.main_menu_index = 4; // About
    app.select();
    assert_eq!(app.screen, Screen::About);
}

#[test]
fn test_app_select_providers_back() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;

    app.provider_index = 10; // Back
    app.select();
    assert_eq!(app.screen, Screen::MainMenu);
}

#[test]
fn test_app_select_providers_default_provider_toggles() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;

    app.provider_index = 0; // DefaultProvider
    let original = app.settings.defaults.provider.clone();
    app.select();
    // Should toggle provider
    assert_ne!(app.settings.defaults.provider, original);
    assert!(app.settings_modified);
}

#[test]
fn test_app_select_providers_api_key_starts_editing() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;

    app.provider_index = 1; // AnthropicApiKey
    app.select();
    assert_eq!(app.input_mode, InputMode::Editing);
}

#[test]
fn test_app_select_providers_model_opens_picker() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;

    app.provider_index = 2; // AnthropicModel
    app.select();
    assert_eq!(app.input_mode, InputMode::SelectingModel);
    assert_eq!(
        app.model_selection_target,
        Some(ModelSelectionTarget::Anthropic)
    );
    assert!(!app.available_models.is_empty());
}

#[test]
fn test_app_select_providers_local_port_starts_editing() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;

    app.provider_index = 3; // LocalPort
    app.select();
    assert_eq!(app.input_mode, InputMode::Editing);
}

#[test]
fn test_app_select_providers_local_model_opens_picker() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;

    app.provider_index = 4; // LocalModel
    app.select();
    assert_eq!(app.input_mode, InputMode::SelectingModel);
    assert_eq!(
        app.model_selection_target,
        Some(ModelSelectionTarget::Local)
    );
    // Uses registry models directly
    assert!(!app.available_models.is_empty());
}

#[test]
fn test_app_select_providers_test_connection() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;

    app.provider_index = 9; // Test Connection
    app.select();
    // Should set a status message
    assert!(app.status_message.is_some());
}

#[test]
fn test_app_select_context_back() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Context;

    app.context_index = 3; // Back
    app.select();
    assert_eq!(app.screen, Screen::MainMenu);
}

#[test]
fn test_app_select_context_max_warm_chunks() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Context;

    app.context_index = 0; // MaxWarmChunks
    app.select();
    assert_eq!(app.input_mode, InputMode::Editing);
}

#[test]
fn test_app_select_context_cold_retention() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Context;

    app.context_index = 1; // ColdRetentionDays
    app.select();
    assert_eq!(app.input_mode, InputMode::Editing);
}

#[test]
fn test_app_select_context_auto_compact_toggle() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Context;

    let original = app.settings.context.auto_compact;
    app.context_index = 2; // AutoCompact
    app.select();
    assert_ne!(app.settings.context.auto_compact, original);
    assert!(app.settings_modified);
}

#[test]
fn test_app_select_about_goes_back() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::About;

    app.select();
    assert_eq!(app.screen, Screen::MainMenu);
}

#[test]
fn test_app_confirm_edit_api_key() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;
    app.provider_index = 1; // AnthropicApiKey

    app.start_editing("sk-test-key");
    app.confirm_edit();

    assert_eq!(
        app.settings.providers.anthropic.api_key,
        Some("sk-test-key".to_string())
    );
    assert!(app.settings_modified);
    assert_eq!(app.input_mode, InputMode::Normal);
}

#[test]
fn test_app_confirm_edit_api_key_empty_clears() {
    let mut settings = Settings::default();
    settings.providers.anthropic.api_key = Some("old-key".to_string());
    let mut app = App::new(settings);
    app.screen = Screen::Providers;
    app.provider_index = 1; // AnthropicApiKey

    app.start_editing("");
    app.confirm_edit();

    assert_eq!(app.settings.providers.anthropic.api_key, None);
}

#[test]
fn test_app_confirm_edit_model() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;
    app.provider_index = 2; // AnthropicModel

    app.start_editing("claude-3-5-haiku-20241022");
    app.confirm_edit();

    assert_eq!(
        app.settings.providers.anthropic.default_model,
        "claude-3-5-haiku-20241022"
    );
    assert!(app.settings_modified);
}

#[test]
fn test_app_confirm_edit_model_empty_ignored() {
    let settings = Settings::default();
    let original_model = settings.providers.anthropic.default_model.clone();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;
    app.provider_index = 2; // AnthropicModel

    app.start_editing("");
    app.confirm_edit();

    // Empty model should be ignored
    assert_eq!(
        app.settings.providers.anthropic.default_model,
        original_model
    );
}

#[test]
fn test_app_confirm_edit_local_port() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;
    app.provider_index = 3; // LocalPort

    app.start_editing("9090");
    app.confirm_edit();

    assert_eq!(app.settings.providers.local.port, 9090);
    assert!(app.settings_modified);
}

#[test]
fn test_app_confirm_edit_local_model() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;
    app.provider_index = 4; // LocalModel

    app.start_editing("llama3.2:latest");
    app.confirm_edit();

    assert_eq!(
        app.settings.providers.local.default_model,
        "llama3.2:latest"
    );
    assert!(app.settings_modified);
}

#[test]
fn test_app_confirm_edit_max_warm_chunks() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Context;
    app.context_index = 0; // MaxWarmChunks

    app.start_editing("200");
    app.confirm_edit();

    assert_eq!(app.settings.context.max_warm_chunks, 200);
    assert!(app.settings_modified);
}

#[test]
fn test_app_confirm_edit_max_warm_chunks_invalid() {
    let settings = Settings::default();
    let original = settings.context.max_warm_chunks;
    let mut app = App::new(settings);
    app.screen = Screen::Context;
    app.context_index = 0; // MaxWarmChunks

    app.start_editing("not a number");
    app.confirm_edit();

    // Should not change and should show error
    assert_eq!(app.settings.context.max_warm_chunks, original);
    assert!(app.status_is_error);
}

#[test]
fn test_app_confirm_edit_cold_retention() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Context;
    app.context_index = 1; // ColdRetentionDays

    app.start_editing("60");
    app.confirm_edit();

    assert_eq!(app.settings.context.cold_retention_days, 60);
    assert!(app.settings_modified);
}

#[test]
fn test_app_confirm_edit_cold_retention_invalid() {
    let settings = Settings::default();
    let original = settings.context.cold_retention_days;
    let mut app = App::new(settings);
    app.screen = Screen::Context;
    app.context_index = 1; // ColdRetentionDays

    app.start_editing("invalid");
    app.confirm_edit();

    assert_eq!(app.settings.context.cold_retention_days, original);
    assert!(app.status_is_error);
}

#[test]
fn test_app_toggle_cap() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // Ensure we have at least one cap
    if !app.available_caps.is_empty() {
        let original_enabled = app.available_caps[0].is_enabled;
        app.toggle_cap(0);
        assert_ne!(app.available_caps[0].is_enabled, original_enabled);
        assert!(app.settings_modified);
    }
}

#[test]
fn test_app_toggle_cap_updates_settings() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    if !app.available_caps.is_empty() {
        let cap_name = app.available_caps[0].name.clone();
        let was_in_defaults = app.settings.defaults.caps.contains(&cap_name);

        app.toggle_cap(0);

        let is_in_defaults = app.settings.defaults.caps.contains(&cap_name);
        // Should have toggled
        if was_in_defaults {
            assert!(!is_in_defaults);
        } else {
            assert!(is_in_defaults);
        }
    }
}

#[test]
fn test_app_toggle_cap_invalid_index() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // Toggle an invalid index - should not panic
    app.toggle_cap(9999);
}

#[test]
fn test_app_refresh_caps() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // Should not panic
    app.refresh_caps();
    // available_caps should still be populated
    assert!(!app.available_caps.is_empty());
}

#[test]
fn test_app_go_to_clears_status() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.set_status("Some status", false);
    app.go_to(Screen::Providers);
    assert!(app.status_message.is_none());
}

#[test]
fn test_app_select_caps_back() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Caps;

    // Back is the last item
    app.caps_index = app.caps_total_items() - 1;
    app.select();
    assert_eq!(app.screen, Screen::MainMenu);
}

#[test]
fn test_app_select_caps_create_new() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Caps;

    // Create New is second to last
    app.caps_index = app.available_caps.len();
    app.select();
    assert_eq!(app.input_mode, InputMode::Editing);
    assert!(app.input_buffer.is_empty());
}

#[test]
fn test_app_provider_index_wraps() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;

    app.provider_index = ProviderItem::all().len() - 1;
    app.move_down();
    assert_eq!(app.provider_index, 0);
}

#[test]
fn test_app_context_index_wraps() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Context;

    app.context_index = ContextItem::all().len() - 1;
    app.move_down();
    assert_eq!(app.context_index, 0);
}

#[test]
fn test_app_caps_index_wraps() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Caps;

    app.caps_index = app.caps_total_items() - 1;
    app.move_down();
    assert_eq!(app.caps_index, 0);
}

// ===== Model Selection Tests =====

#[test]
fn test_cancel_model_selection_clears_state() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // Set up as if we're in the middle of model selection
    app.model_selection_target = Some(ModelSelectionTarget::Local);
    app.input_mode = InputMode::SelectingModel;

    // Cancel
    app.cancel_model_selection();

    // Should have cleared everything
    assert!(app.model_selection_target.is_none());
    assert_eq!(app.input_mode, InputMode::Normal);
}

#[test]
fn test_start_model_selection_local_uses_registry() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.start_model_selection(ModelSelectionTarget::Local);

    // Should have used registry models directly
    assert!(!app.available_models.is_empty());
    assert_eq!(app.input_mode, InputMode::SelectingModel);
}

#[test]
fn test_start_model_selection_anthropic_uses_registry() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.start_model_selection(ModelSelectionTarget::Anthropic);

    // Should have used registry models
    assert!(!app.available_models.is_empty());
    assert_eq!(app.input_mode, InputMode::SelectingModel);
    assert_eq!(
        app.model_selection_target,
        Some(ModelSelectionTarget::Anthropic)
    );
}

#[test]
fn test_start_model_selection_finds_current_model() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    // First, see what models are available and pick one
    app.start_model_selection(ModelSelectionTarget::Anthropic);
    let available_model = app.available_models[1].id.clone(); // Pick the second model
    app.cancel_model_selection();

    // Now set that model as the default
    app.settings.providers.anthropic.default_model = available_model.clone();

    // Start selection again
    app.start_model_selection(ModelSelectionTarget::Anthropic);

    // Should have found the current model at index 1
    let selected_model = &app.available_models[app.model_picker_index];
    assert_eq!(selected_model.id, available_model);
    assert_eq!(app.model_picker_index, 1);
}

// ===== Plan Management Tests =====

#[test]
fn test_plans_total_items() {
    let settings = Settings::default();
    let app = App::new(settings);
    // Total = plans count + 1 (Back button)
    assert_eq!(app.plans_total_items(), app.available_plans.len() + 1);
}

#[test]
fn test_refresh_plans() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    // Should not panic
    app.refresh_plans();
}

#[test]
fn test_view_plan_nonexistent() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    let fake_id = uuid::Uuid::new_v4();

    // Should not panic, should stay on current screen
    let original_screen = app.screen;
    app.view_plan(fake_id);
    // If plan doesn't exist, screen should not change
    assert_eq!(app.screen, original_screen);
}

#[test]
fn test_set_plan_status_nonexistent() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    let fake_id = uuid::Uuid::new_v4();

    // Should not panic
    app.set_plan_status(fake_id, PlanStatus::Active);
}

#[test]
fn test_delete_plan_nonexistent() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    let fake_id = uuid::Uuid::new_v4();

    // Should not panic
    app.delete_plan(fake_id);
}

#[test]
fn test_edit_plan_nonexistent() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    let fake_id = uuid::Uuid::new_v4();

    let original_screen = app.screen;
    app.edit_plan(fake_id);
    // If plan doesn't exist, screen should not change
    assert_eq!(app.screen, original_screen);
}

// ===== Editor Tests =====

#[test]
fn test_editor_mode_none() {
    let settings = Settings::default();
    let app = App::new(settings);
    assert!(app.editor_mode().is_none());
}

#[test]
fn test_editor_mode_some() {
    use crate::tui::editor::Editor;

    let settings = Settings::default();
    let mut app = App::new(settings);
    app.editor = Some(Editor::new("test content"));

    assert!(app.editor_mode().is_some());
    assert_eq!(app.editor_mode().unwrap(), EditorMode::Normal);
}

#[test]
fn test_editor_modified_no_editor() {
    let settings = Settings::default();
    let app = App::new(settings);
    assert!(!app.editor_modified());
}

#[test]
fn test_editor_modified_unmodified() {
    use crate::tui::editor::Editor;

    let settings = Settings::default();
    let mut app = App::new(settings);
    app.editor = Some(Editor::new("test content"));

    assert!(!app.editor_modified());
}

#[test]
fn test_save_editor_no_plan_id() {
    use crate::tui::editor::Editor;

    let settings = Settings::default();
    let mut app = App::new(settings);
    app.editor = Some(Editor::new("test content"));
    app.current_plan_id = None;

    // Should fail and set error status
    let result = app.save_editor();
    assert!(!result);
    assert!(app.status_is_error);
}

#[test]
fn test_save_editor_no_editor() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.current_plan_id = Some(uuid::Uuid::new_v4());
    app.editor = None;

    // Should fail and set error status
    let result = app.save_editor();
    assert!(!result);
    assert!(app.status_is_error);
}

#[test]
fn test_handle_editor_command_continue() {
    use crate::tui::editor::CommandResult;

    let settings = Settings::default();
    let mut app = App::new(settings);

    let result = app.handle_editor_command(CommandResult::Continue);
    assert!(matches!(result, AppResult::Continue));
}

#[test]
fn test_handle_editor_command_quit() {
    use crate::tui::editor::CommandResult;

    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::PlanEdit;

    let result = app.handle_editor_command(CommandResult::Quit);
    assert!(matches!(result, AppResult::Continue));
    assert!(app.editor.is_none());
    assert_eq!(app.screen, Screen::Plans);
}

#[test]
fn test_handle_editor_command_invalid() {
    use crate::tui::editor::CommandResult;

    let settings = Settings::default();
    let mut app = App::new(settings);

    let result = app.handle_editor_command(CommandResult::Invalid("Unknown command".to_string()));
    assert!(matches!(result, AppResult::Continue));
    assert!(app.status_is_error);
    assert!(app
        .status_message
        .as_ref()
        .unwrap()
        .contains("Unknown command"));
}

// ===== Plan View Navigation Tests =====

#[test]
fn test_move_up_plan_view_scrolls() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::PlanView;
    app.plan_scroll = 5;

    app.move_up();
    assert_eq!(app.plan_scroll, 4);
}

#[test]
fn test_move_up_plan_view_at_top() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::PlanView;
    app.plan_scroll = 0;

    app.move_up();
    assert_eq!(app.plan_scroll, 0); // Should not go negative
}

#[test]
fn test_move_down_plan_view_scrolls() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::PlanView;
    app.plan_scroll = 0;

    app.move_down();
    assert_eq!(app.plan_scroll, 1);
}

#[test]
fn test_move_up_plans() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Plans;
    app.plans_index = 0;

    app.move_up();
    // Should wrap to last item
    assert_eq!(app.plans_index, app.plans_total_items() - 1);
}

#[test]
fn test_move_down_plans() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Plans;
    app.plans_index = 0;

    app.move_down();
    if app.plans_total_items() > 1 {
        assert_eq!(app.plans_index, 1);
    }
}

#[test]
fn test_move_down_plans_wraps() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Plans;
    app.plans_index = app.plans_total_items() - 1;

    app.move_down();
    assert_eq!(app.plans_index, 0);
}

#[test]
fn test_select_plan_view_goes_back() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::PlanView;

    app.select();
    assert_eq!(app.screen, Screen::Plans);
}

#[test]
fn test_select_plans_back() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Plans;
    // Select "Back" which is at the end
    app.plans_index = app.available_plans.len();

    app.select();
    assert_eq!(app.screen, Screen::MainMenu);
}

// ===== create_live_model_info Tests =====

#[test]
fn test_create_live_model_info_unknown_model() {
    let settings = Settings::default();
    let app = App::new(settings);

    let info = app.create_live_model_info("totally-unknown-model:latest");

    assert_eq!(info.id, "totally-unknown-model:latest");
    assert_eq!(info.name, "totally-unknown-model:latest");
    assert_eq!(info.tier, "Unknown");
    assert!(!info.recommended);
}

#[test]
fn test_create_live_model_info_known_model() {
    let settings = Settings::default();
    let app = App::new(settings);

    // qwen2.5-coder:14b should be in the registry
    let info = app.create_live_model_info("qwen2.5-coder:14b");

    assert_eq!(info.id, "qwen2.5-coder:14b");
    // Should have metadata from registry
    assert!(!info.description.is_empty() || info.tier != "Unknown");
}

// ===== Model Selection Confirmation Tests =====

#[test]
fn test_confirm_model_selection_anthropic() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.start_model_selection(ModelSelectionTarget::Anthropic);
    assert!(!app.available_models.is_empty());

    // Select a model
    app.model_picker_index = 0;
    let expected_model = app.available_models[0].id.clone();

    app.confirm_model_selection();

    assert_eq!(
        app.settings.providers.anthropic.default_model,
        expected_model
    );
    assert!(app.settings_modified);
    assert_eq!(app.input_mode, InputMode::Normal);
}

#[test]
fn test_confirm_model_selection_local() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.start_model_selection(ModelSelectionTarget::Local);
    assert!(!app.available_models.is_empty());

    app.model_picker_index = 0;
    let expected_model = app.available_models[0].id.clone();

    app.confirm_model_selection();

    assert_eq!(app.settings.providers.local.default_model, expected_model);
    assert!(app.settings_modified);
}

#[test]
fn test_confirm_model_selection_openrouter() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.start_model_selection(ModelSelectionTarget::OpenRouter);
    if !app.available_models.is_empty() {
        app.model_picker_index = 0;
        let expected_model = app.available_models[0].id.clone();

        app.confirm_model_selection();

        assert_eq!(
            app.settings.providers.openrouter.default_model,
            expected_model
        );
    }
}

#[test]
fn test_confirm_model_selection_blackman() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.start_model_selection(ModelSelectionTarget::Blackman);
    if !app.available_models.is_empty() {
        app.model_picker_index = 0;
        let expected_model = app.available_models[0].id.clone();

        app.confirm_model_selection();

        assert_eq!(
            app.settings.providers.blackman.default_model,
            expected_model
        );
    }
}

#[test]
fn test_confirm_model_selection_no_target() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.model_selection_target = None;
    app.available_models = vec![ModelDisplayInfo {
        id: "test".to_string(),
        name: "Test".to_string(),
        tier: "Standard".to_string(),
        description: "".to_string(),
        recommended: false,
    }];

    // Should not panic
    app.confirm_model_selection();
    assert_eq!(app.input_mode, InputMode::Normal);
}

#[test]
fn test_confirm_model_selection_empty_models() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.model_selection_target = Some(ModelSelectionTarget::Anthropic);
    app.available_models = vec![];

    // Should not panic
    app.confirm_model_selection();
}

// ===== Model Picker Navigation Tests =====

#[test]
fn test_model_picker_up_not_at_top() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.available_models = vec![
        ModelDisplayInfo {
            id: "m1".to_string(),
            name: "M1".to_string(),
            tier: "S".to_string(),
            description: "".to_string(),
            recommended: false,
        },
        ModelDisplayInfo {
            id: "m2".to_string(),
            name: "M2".to_string(),
            tier: "S".to_string(),
            description: "".to_string(),
            recommended: false,
        },
    ];
    app.model_picker_index = 1;

    app.model_picker_up();
    assert_eq!(app.model_picker_index, 0);
}

#[test]
fn test_model_picker_up_at_top_wraps() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.available_models = vec![
        ModelDisplayInfo {
            id: "m1".to_string(),
            name: "M1".to_string(),
            tier: "S".to_string(),
            description: "".to_string(),
            recommended: false,
        },
        ModelDisplayInfo {
            id: "m2".to_string(),
            name: "M2".to_string(),
            tier: "S".to_string(),
            description: "".to_string(),
            recommended: false,
        },
    ];
    app.model_picker_index = 0;

    app.model_picker_up();
    assert_eq!(app.model_picker_index, 1); // Wraps to last
}

#[test]
fn test_model_picker_up_empty() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.available_models = vec![];
    app.model_picker_index = 0;

    app.model_picker_up();
    assert_eq!(app.model_picker_index, 0); // No change
}

#[test]
fn test_model_picker_down_not_at_bottom() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.available_models = vec![
        ModelDisplayInfo {
            id: "m1".to_string(),
            name: "M1".to_string(),
            tier: "S".to_string(),
            description: "".to_string(),
            recommended: false,
        },
        ModelDisplayInfo {
            id: "m2".to_string(),
            name: "M2".to_string(),
            tier: "S".to_string(),
            description: "".to_string(),
            recommended: false,
        },
    ];
    app.model_picker_index = 0;

    app.model_picker_down();
    assert_eq!(app.model_picker_index, 1);
}

#[test]
fn test_model_picker_down_at_bottom_wraps() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.available_models = vec![
        ModelDisplayInfo {
            id: "m1".to_string(),
            name: "M1".to_string(),
            tier: "S".to_string(),
            description: "".to_string(),
            recommended: false,
        },
        ModelDisplayInfo {
            id: "m2".to_string(),
            name: "M2".to_string(),
            tier: "S".to_string(),
            description: "".to_string(),
            recommended: false,
        },
    ];
    app.model_picker_index = 1;

    app.model_picker_down();
    assert_eq!(app.model_picker_index, 0); // Wraps to first
}

#[test]
fn test_model_picker_down_empty() {
    let settings = Settings::default();
    let mut app = App::new(settings);

    app.available_models = vec![];
    app.model_picker_index = 0;

    app.model_picker_down();
    assert_eq!(app.model_picker_index, 0); // No change
}

// ===== Provider Navigation Edge Cases =====

#[test]
fn test_move_up_providers_not_at_top() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::Providers;
    app.provider_index = 2;

    app.move_up();
    assert_eq!(app.provider_index, 1);
}

// ===== PlanEdit Navigation =====

#[test]
fn test_move_up_plan_edit_does_nothing() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::PlanEdit;

    // Should not panic - handled by editor
    app.move_up();
}

#[test]
fn test_move_down_plan_edit_does_nothing() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::PlanEdit;

    // Should not panic - handled by editor
    app.move_down();
}

#[test]
fn test_select_plan_edit_does_nothing() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.screen = Screen::PlanEdit;

    // Should not panic - handled by editor (insert newline)
    app.select();
    assert_eq!(app.screen, Screen::PlanEdit);
}

// ===== Connection Test Tests =====

#[test]
fn test_connection_test_fields_initialized() {
    let settings = Settings::default();
    let app = App::new(settings);
    assert!(!app.testing_connection);
    assert!(app.connection_test_rx.is_none());
}

#[test]
fn test_start_connection_test_local_provider() {
    let mut settings = Settings::default();
    settings.defaults.provider = "local".to_string();
    let mut app = App::new(settings);

    app.start_connection_test();

    // Should set a status message about local provider configuration
    assert!(app.status_message.is_some());
}

#[test]
fn test_start_connection_test_anthropic_with_key() {
    let mut settings = Settings::default();
    settings.defaults.provider = "anthropic".to_string();
    settings.providers.anthropic.api_key = Some("sk-test-key".to_string());
    let mut app = App::new(settings);

    app.start_connection_test();

    assert!(!app.status_is_error);
    assert!(app.status_message.as_ref().unwrap().contains("configured"));
}

#[test]
fn test_start_connection_test_anthropic_no_key() {
    let mut settings = Settings::default();
    settings.defaults.provider = "anthropic".to_string();
    settings.providers.anthropic.api_key = None;
    let mut app = App::new(settings);

    app.start_connection_test();

    assert!(app.status_is_error);
    assert!(app
        .status_message
        .as_ref()
        .unwrap()
        .contains("No Anthropic API key"));
}

#[test]
fn test_start_connection_test_unknown_provider() {
    let mut settings = Settings::default();
    settings.defaults.provider = "unknown".to_string();
    let mut app = App::new(settings);

    app.start_connection_test();

    assert!(app.status_is_error);
    assert!(app
        .status_message
        .as_ref()
        .unwrap()
        .contains("not available"));
}

#[test]
fn test_check_connection_test_results_no_receiver() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.connection_test_rx = None;

    // Should not panic
    app.check_connection_test_results();
}

#[test]
fn test_check_connection_test_results_success() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.testing_connection = true;

    // Create a channel and send success
    let (tx, rx) = mpsc::channel();
    app.connection_test_rx = Some(rx);
    tx.send(Ok("Connected successfully!".to_string())).unwrap();

    app.check_connection_test_results();

    assert!(!app.testing_connection);
    assert!(app.connection_test_rx.is_none());
    assert!(!app.status_is_error);
    assert!(app.status_message.as_ref().unwrap().contains("Connected"));
}

#[test]
fn test_check_connection_test_results_error() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.testing_connection = true;

    // Create a channel and send error
    let (tx, rx) = mpsc::channel();
    app.connection_test_rx = Some(rx);
    tx.send(Err("Connection refused".to_string())).unwrap();

    app.check_connection_test_results();

    assert!(!app.testing_connection);
    assert!(app.connection_test_rx.is_none());
    assert!(app.status_is_error);
    assert!(app
        .status_message
        .as_ref()
        .unwrap()
        .contains("Connection refused"));
}

#[test]
fn test_check_connection_test_results_empty() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.testing_connection = true;

    // Create a channel but don't send anything
    let (_tx, rx) = mpsc::channel::<Result<String, String>>();
    app.connection_test_rx = Some(rx);

    app.check_connection_test_results();

    // Should still be testing
    assert!(app.testing_connection);
    assert!(app.connection_test_rx.is_some());
}

#[test]
fn test_check_connection_test_results_disconnected() {
    let settings = Settings::default();
    let mut app = App::new(settings);
    app.testing_connection = true;

    // Create a channel and drop sender
    let (tx, rx) = mpsc::channel::<Result<String, String>>();
    app.connection_test_rx = Some(rx);
    drop(tx);

    app.check_connection_test_results();

    assert!(!app.testing_connection);
    assert!(app.connection_test_rx.is_none());
    assert!(app.status_is_error);
    assert!(app.status_message.as_ref().unwrap().contains("interrupted"));
}

#[test]
fn test_select_providers_test_connection_calls_method() {
    let mut settings = Settings::default();
    settings.defaults.provider = "anthropic".to_string();
    settings.providers.anthropic.api_key = Some("test-key".to_string());
    let mut app = App::new(settings);
    app.screen = Screen::Providers;

    // Find TestConnection index
    let test_conn_index = ProviderItem::all()
        .iter()
        .position(|&p| p == ProviderItem::TestConnection)
        .unwrap();
    app.provider_index = test_conn_index;

    app.select();

    // Should have set a status message
    assert!(app.status_message.is_some());
}
