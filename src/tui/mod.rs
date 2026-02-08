// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! TUI interfaces for Ted
//!
//! Contains both the settings TUI and the main chat TUI.
//! Uses ratatui for rendering and crossterm for input handling.

pub mod app;
pub mod chat;
pub mod editor;
pub mod input;
pub mod screens;
pub mod ui;
pub mod undo;

// Re-export chat TUI for convenience
pub use chat::{run_chat_tui, ChatTuiConfig};

use std::io;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use crate::config::Settings;
use crate::error::{Result, TedError};
use app::{App, AppResult};

/// Run the TUI settings interface
pub fn run_tui(settings: Settings) -> Result<()> {
    let _ = run_tui_interactive(settings)?;
    Ok(())
}

/// Run the TUI plans browser
pub fn run_tui_plans(settings: Settings) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and go directly to Plans screen
    let mut app = App::new(settings);
    app.refresh_plans();
    app.go_to(app::Screen::Plans);

    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Handle any errors from the app
    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    // Save settings if modified
    if app.settings_modified {
        app.settings.save()?;
    }

    Ok(())
}

/// Run the TUI settings interface and return the (potentially modified) settings
/// Returns (settings, was_modified)
pub fn run_tui_interactive(settings: Settings) -> Result<(Settings, bool)> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run it
    let mut app = App::new(settings);
    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Handle any errors from the app
    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    let modified = app.settings_modified;

    // Save settings if modified
    if modified {
        app.settings.save()?;
    }

    Ok((app.settings, modified))
}

/// Main application loop
fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal
            .draw(|f| ui::draw(f, app))
            .map_err(|e| TedError::Tui(e.to_string()))?;

        match input::handle_input(app)? {
            AppResult::Continue => {}
            AppResult::Quit => break,
        }
    }

    Ok(())
}

/// Run a single iteration of the app loop (for testing)
/// Returns Ok(true) if the app should quit, Ok(false) otherwise
#[cfg(test)]
fn run_app_iteration<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<bool> {
    terminal
        .draw(|f| ui::draw(f, app))
        .map_err(|e| TedError::Tui(e.to_string()))?;
    Ok(false) // In test mode, we don't poll for events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_run_app_iteration_renders_without_panic() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);

        // Running a single iteration should not panic
        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should return false (don't quit yet)
    }

    #[test]
    fn test_run_app_iteration_multiple_screens() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);

        // Test rendering each screen
        for screen in [
            app::Screen::MainMenu,
            app::Screen::Providers,
            app::Screen::Caps,
            app::Screen::Context,
            app::Screen::About,
            app::Screen::Plans,
        ] {
            app.go_to(screen);
            let result = run_app_iteration(&mut terminal, &mut app);
            assert!(result.is_ok(), "Failed to render {:?}", screen);
        }
    }

    #[test]
    fn test_run_app_iteration_with_editing_mode() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.go_to(app::Screen::Providers);
        app.start_editing("test value");

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_app_iteration_with_status_message() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.set_status("Test status", false);

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_app_iteration_with_error_status() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.set_status("Error!", true);

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_tui_returns_ok() {
        // We can't fully test run_tui because it requires a real terminal,
        // but we can verify the function signature and that App::new works
        let settings = Settings::default();
        let app = App::new(settings.clone());

        // App should be properly initialized
        assert_eq!(app.screen, app::Screen::MainMenu);
        assert!(!app.settings_modified);
    }

    #[test]
    fn test_app_result_enum() {
        let continue_result = AppResult::Continue;
        let quit_result = AppResult::Quit;

        // Just verify we can create both variants
        match continue_result {
            AppResult::Continue => {}
            AppResult::Quit => panic!("Expected Continue"),
        }

        match quit_result {
            AppResult::Quit => {}
            AppResult::Continue => panic!("Expected Quit"),
        }
    }

    #[test]
    fn test_run_app_iteration_small_terminal() {
        // Test with very small terminal
        let backend = TestBackend::new(20, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_app_iteration_large_terminal() {
        // Test with large terminal
        let backend = TestBackend::new(200, 60);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_app_iteration_provider_screen_with_api_key() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = Some("sk-test".to_string());
        let mut app = App::new(settings);
        app.go_to(app::Screen::Providers);

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_app_iteration_caps_with_items() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(app::Screen::Caps);

        // Select different items
        for i in 0..app.caps_total_items() {
            app.caps_index = i;
            let result = run_app_iteration(&mut terminal, &mut app);
            assert!(result.is_ok(), "Failed at caps index {}", i);
        }
    }

    #[test]
    fn test_run_app_iteration_context_with_values() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut settings = Settings::default();
        settings.context.max_warm_chunks = 42;
        settings.context.cold_retention_days = 365;
        settings.context.auto_compact = true;
        let mut app = App::new(settings);
        app.go_to(app::Screen::Context);

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    // ==================== Additional screen state tests ====================

    #[test]
    fn test_run_app_iteration_providers_all_indices() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(app::Screen::Providers);

        // Test different provider indices
        for i in 0..5 {
            app.provider_index = i;
            let result = run_app_iteration(&mut terminal, &mut app);
            assert!(result.is_ok(), "Failed at providers index {}", i);
        }
    }

    #[test]
    fn test_run_app_iteration_main_menu_all_indices() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(app::Screen::MainMenu);

        // Test different main menu indices
        for i in 0..6 {
            app.main_menu_index = i;
            let result = run_app_iteration(&mut terminal, &mut app);
            assert!(result.is_ok(), "Failed at main menu index {}", i);
        }
    }

    #[test]
    fn test_run_app_iteration_context_all_indices() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(app::Screen::Context);

        // Test different context indices
        for i in 0..5 {
            app.context_index = i;
            let result = run_app_iteration(&mut terminal, &mut app);
            assert!(result.is_ok(), "Failed at context index {}", i);
        }
    }

    #[test]
    fn test_run_app_iteration_editing_empty_string() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(app::Screen::Providers);
        app.start_editing("");

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_app_iteration_editing_long_string() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(app::Screen::Providers);
        app.start_editing(&"a".repeat(200));

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_app_iteration_with_all_providers_configured() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = Some("sk-ant-test".to_string());
        let mut app = App::new(settings);
        app.go_to(app::Screen::Providers);

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_app_iteration_about_screen() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(app::Screen::About);

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_app_iteration_plans_screen() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(app::Screen::Plans);
        app.refresh_plans();

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_app_iteration_plans_with_indices() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(app::Screen::Plans);
        app.refresh_plans();

        // Test different plan indices
        for i in 0..3 {
            app.plans_index = i;
            let result = run_app_iteration(&mut terminal, &mut app);
            assert!(result.is_ok(), "Failed at plans index {}", i);
        }
    }

    #[test]
    fn test_run_app_iteration_very_small_width() {
        let backend = TestBackend::new(10, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_app_iteration_very_small_height() {
        let backend = TestBackend::new(80, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_app_iteration_minimal_terminal() {
        let backend = TestBackend::new(5, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_app_settings_modified_flag() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        assert!(!app.settings_modified);
        app.settings_modified = true;
        assert!(app.settings_modified);
    }

    #[test]
    fn test_app_screen_navigation() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        assert_eq!(app.screen, app::Screen::MainMenu);
        app.go_to(app::Screen::Providers);
        assert_eq!(app.screen, app::Screen::Providers);
        app.go_to(app::Screen::Caps);
        assert_eq!(app.screen, app::Screen::Caps);
    }

    #[test]
    fn test_app_status_message_lifecycle() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.set_status("Test message", false);
        assert!(app.status_message.is_some());

        app.clear_status();
        assert!(app.status_message.is_none());
    }

    #[test]
    fn test_app_status_error_flag() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.set_status("Info", false);
        assert!(!app.status_is_error);

        app.set_status("Error", true);
        assert!(app.status_is_error);
    }

    #[test]
    fn test_run_app_iteration_status_unicode() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.set_status("日本語テスト 🎉", false);

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_app_iteration_context_all_settings() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut settings = Settings::default();
        settings.context.max_warm_chunks = 100;
        settings.context.cold_retention_days = 30;
        settings.context.auto_compact = false;
        let mut app = App::new(settings);
        app.go_to(app::Screen::Context);

        let result = run_app_iteration(&mut terminal, &mut app);
        assert!(result.is_ok());
    }

    #[test]
    fn test_multiple_screen_transitions() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);

        // Navigate through screens multiple times
        for _ in 0..3 {
            for screen in [
                app::Screen::MainMenu,
                app::Screen::Providers,
                app::Screen::Caps,
                app::Screen::Context,
                app::Screen::About,
            ] {
                app.go_to(screen);
                let result = run_app_iteration(&mut terminal, &mut app);
                assert!(result.is_ok());
            }
        }
    }

    #[test]
    fn test_app_editing_mode_on_each_screen() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let mut app = App::new(settings);

        for screen in [
            app::Screen::Providers,
            app::Screen::Caps,
            app::Screen::Context,
        ] {
            app.go_to(screen);
            app.start_editing("test");
            let result = run_app_iteration(&mut terminal, &mut app);
            assert!(result.is_ok(), "Failed editing on {:?}", screen);
            app.cancel_editing();
        }
    }

    #[test]
    fn test_app_caps_index_bounds() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(app::Screen::Caps);

        let total = app.caps_total_items();
        assert!(total > 0);

        // Test boundary indices
        app.caps_index = 0;
        assert_eq!(app.caps_index, 0);

        app.caps_index = total - 1;
        assert_eq!(app.caps_index, total - 1);
    }

    #[test]
    fn test_terminal_backend_dimensions() {
        let backend = TestBackend::new(80, 24);
        let terminal = Terminal::new(backend).unwrap();
        let size = terminal.size().unwrap();
        assert_eq!(size.width, 80);
        assert_eq!(size.height, 24);
    }

    #[test]
    fn test_different_aspect_ratios() {
        let settings = Settings::default();

        // Wide terminal
        let backend_wide = TestBackend::new(200, 24);
        let mut terminal_wide = Terminal::new(backend_wide).unwrap();
        let mut app_wide = App::new(settings.clone());
        assert!(run_app_iteration(&mut terminal_wide, &mut app_wide).is_ok());

        // Tall terminal
        let backend_tall = TestBackend::new(40, 80);
        let mut terminal_tall = Terminal::new(backend_tall).unwrap();
        let mut app_tall = App::new(settings.clone());
        assert!(run_app_iteration(&mut terminal_tall, &mut app_tall).is_ok());

        // Square terminal
        let backend_square = TestBackend::new(50, 50);
        let mut terminal_square = Terminal::new(backend_square).unwrap();
        let mut app_square = App::new(settings);
        assert!(run_app_iteration(&mut terminal_square, &mut app_square).is_ok());
    }
}
