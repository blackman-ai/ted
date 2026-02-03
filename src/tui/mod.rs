// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! TUI settings interface
//!
//! A minimalist ASCII-style terminal UI for managing Ted settings.
//! Uses ratatui for rendering and crossterm for input handling.

pub mod app;
pub mod editor;
pub mod input;
pub mod screens;
pub mod ui;
pub mod undo;

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
}
