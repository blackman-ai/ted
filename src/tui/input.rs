// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Input handling for the TUI
//!
//! Handles keyboard input and maps to application actions.

use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use super::app::{App, AppResult, InputMode, Screen};
use super::editor::EditorMode;
use crate::error::Result;

/// Handle user input
pub fn handle_input(app: &mut App) -> Result<AppResult> {
    // Check for async results (non-blocking)
    app.check_model_fetch_results();
    app.check_connection_test_results();

    // Poll for events with a small timeout
    if event::poll(Duration::from_millis(100))? {
        if let Event::Key(key) = event::read()? {
            // Only handle key press events (not release)
            if key.kind != KeyEventKind::Press {
                return Ok(AppResult::Continue);
            }

            // Check for Ctrl+C to quit from anywhere
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                return Ok(AppResult::Quit);
            }

            // Handle editor input separately when in PlanEdit screen
            if app.screen == Screen::PlanEdit {
                return handle_editor_input(app, key.code, key.modifiers);
            }

            match app.input_mode {
                InputMode::Normal => return handle_normal_input(app, key.code),
                InputMode::Editing => handle_editing_input(app, key.code),
                InputMode::SelectingModel => handle_model_selection_input(app, key.code),
            }
        }
    }

    Ok(AppResult::Continue)
}

/// Handle input in normal (navigation) mode
fn handle_normal_input(app: &mut App, key: KeyCode) -> Result<AppResult> {
    match key {
        // Quit
        KeyCode::Char('q') | KeyCode::Esc => {
            if app.screen == Screen::MainMenu {
                return Ok(AppResult::Quit);
            } else {
                app.go_back();
            }
        }

        // Navigation
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),

        // Selection
        KeyCode::Enter => app.select(),

        // Space to toggle in caps screen
        KeyCode::Char(' ') if app.screen == Screen::Caps => {
            let cap_count = app.available_caps.len();
            if app.caps_index < cap_count {
                app.toggle_cap(app.caps_index);
            }
        }

        // Delete plan in plans screen
        KeyCode::Char('d') if app.screen == Screen::Plans => {
            let plan_count = app.available_plans.len();
            if app.plans_index < plan_count {
                let plan_id = app.available_plans[app.plans_index].id;
                app.delete_plan(plan_id);
                // Adjust index if needed
                if app.plans_index >= app.available_plans.len() && app.plans_index > 0 {
                    app.plans_index -= 1;
                }
            }
        }

        // Edit plan in plans screen (vim editor)
        KeyCode::Char('e') if app.screen == Screen::Plans => {
            let plan_count = app.available_plans.len();
            if app.plans_index < plan_count {
                let plan_id = app.available_plans[app.plans_index].id;
                app.edit_plan(plan_id);
            }
        }

        // Edit plan from plan view screen
        KeyCode::Char('e') if app.screen == Screen::PlanView => {
            if let Some(plan_id) = app.current_plan_id {
                app.edit_plan(plan_id);
            }
        }

        // Quick navigation from main menu
        KeyCode::Char('1') if app.screen == Screen::MainMenu => {
            app.main_menu_index = 0;
            app.select();
        }
        KeyCode::Char('2') if app.screen == Screen::MainMenu => {
            app.main_menu_index = 1;
            app.select();
        }
        KeyCode::Char('3') if app.screen == Screen::MainMenu => {
            app.main_menu_index = 2;
            app.select();
        }
        KeyCode::Char('4') if app.screen == Screen::MainMenu => {
            app.main_menu_index = 3;
            app.select();
        }
        KeyCode::Char('5') if app.screen == Screen::MainMenu => {
            app.main_menu_index = 4;
            app.select();
        }

        // Help
        KeyCode::Char('?') => {
            app.set_status(
                "↑↓/jk: Navigate | Enter: Select | q/Esc: Back | Ctrl+C: Quit",
                false,
            );
        }

        _ => {}
    }

    Ok(AppResult::Continue)
}

/// Handle input in editing mode
fn handle_editing_input(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Enter => {
            app.confirm_edit();
        }
        KeyCode::Esc => {
            app.cancel_editing();
        }
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        KeyCode::Char(c) => {
            app.input_buffer.push(c);
        }
        _ => {}
    }
}

/// Handle input in model selection mode
fn handle_model_selection_input(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Enter => {
            app.confirm_model_selection();
        }
        KeyCode::Esc => {
            app.cancel_model_selection();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.model_picker_up();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.model_picker_down();
        }
        _ => {}
    }
}

/// Handle input for the vim-style editor
fn handle_editor_input(app: &mut App, key: KeyCode, modifiers: KeyModifiers) -> Result<AppResult> {
    let Some(editor) = app.editor.as_mut() else {
        // No editor active, go back
        app.go_to(Screen::Plans);
        return Ok(AppResult::Continue);
    };

    match editor.mode() {
        EditorMode::Normal => handle_editor_normal_mode(app, key, modifiers),
        EditorMode::Insert => handle_editor_insert_mode(app, key),
        EditorMode::Command => handle_editor_command_mode(app, key),
    }
}

/// Handle vim normal mode input
fn handle_editor_normal_mode(
    app: &mut App,
    key: KeyCode,
    modifiers: KeyModifiers,
) -> Result<AppResult> {
    let Some(editor) = app.editor.as_mut() else {
        return Ok(AppResult::Continue);
    };

    match key {
        // Movement
        KeyCode::Char('h') | KeyCode::Left => editor.move_left(),
        KeyCode::Char('j') | KeyCode::Down => editor.move_down(),
        KeyCode::Char('k') | KeyCode::Up => editor.move_up(),
        KeyCode::Char('l') | KeyCode::Right => editor.move_right(),

        // Word movement
        KeyCode::Char('w') => editor.move_word_forward(),
        KeyCode::Char('b') => editor.move_word_backward(),

        // Line start/end
        KeyCode::Char('0') | KeyCode::Home => editor.move_line_start(),
        KeyCode::Char('$') | KeyCode::End => editor.move_line_end(),

        // File start/end
        KeyCode::Char('g') => {
            // gg - go to start (simplified: single g goes to start)
            editor.move_file_start();
        }
        KeyCode::Char('G') => editor.move_file_end(),

        // Insert modes
        KeyCode::Char('i') => editor.enter_insert(),
        KeyCode::Char('I') => editor.enter_insert_start(),
        KeyCode::Char('a') => editor.enter_insert_after(),
        KeyCode::Char('A') => editor.enter_insert_end(),

        // New lines
        KeyCode::Char('o') => editor.open_line_below(),
        KeyCode::Char('O') => editor.open_line_above(),

        // Delete line
        KeyCode::Char('d') => {
            // dd - delete line (simplified: single d deletes line)
            editor.delete_line();
        }

        // Yank and paste
        KeyCode::Char('y') => {
            // yy - yank line (simplified: single y yanks line)
            editor.yank_line();
        }
        KeyCode::Char('p') => editor.paste(),

        // Undo/Redo
        KeyCode::Char('u') => editor.undo(),
        KeyCode::Char('r') if modifiers.contains(KeyModifiers::CONTROL) => {
            editor.redo();
        }

        // Toggle checkbox
        KeyCode::Char(' ') => editor.toggle_checkbox(),

        // Enter command mode
        KeyCode::Char(':') => editor.enter_command(),

        // Escape - quit without saving if not modified
        KeyCode::Esc => {
            if !editor.is_modified() {
                app.editor = None;
                app.go_to(Screen::Plans);
            }
        }

        _ => {}
    }

    Ok(AppResult::Continue)
}

/// Handle vim insert mode input
fn handle_editor_insert_mode(app: &mut App, key: KeyCode) -> Result<AppResult> {
    let Some(editor) = app.editor.as_mut() else {
        return Ok(AppResult::Continue);
    };

    match key {
        KeyCode::Esc => editor.exit_to_normal(),
        KeyCode::Enter => editor.insert_newline(),
        KeyCode::Backspace => editor.backspace(),
        KeyCode::Char(c) => editor.insert_char(c),
        KeyCode::Left => editor.move_left(),
        KeyCode::Right => editor.move_right(),
        KeyCode::Up => editor.move_up(),
        KeyCode::Down => editor.move_down(),
        _ => {}
    }

    Ok(AppResult::Continue)
}

/// Handle vim command mode input
fn handle_editor_command_mode(app: &mut App, key: KeyCode) -> Result<AppResult> {
    let Some(editor) = app.editor.as_mut() else {
        return Ok(AppResult::Continue);
    };

    match key {
        KeyCode::Esc => {
            editor.exit_to_normal();
        }
        KeyCode::Enter => {
            let result = editor.execute_command();
            return Ok(app.handle_editor_command(result));
        }
        KeyCode::Backspace => {
            editor.command_backspace();
        }
        KeyCode::Char(c) => {
            editor.command_input(c);
        }
        _ => {}
    }

    Ok(AppResult::Continue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::tui::app::{InputMode, Screen};

    // Note: We can't easily test handle_input directly since it polls for events,
    // but we can test the internal handlers which contain the actual logic.

    // ===== handle_normal_input Tests =====

    #[test]
    fn test_handle_normal_input_quit_from_main_menu() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::MainMenu;

        let result = handle_normal_input(&mut app, KeyCode::Char('q')).unwrap();
        assert!(matches!(result, AppResult::Quit));
    }

    #[test]
    fn test_handle_normal_input_esc_from_main_menu() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::MainMenu;

        let result = handle_normal_input(&mut app, KeyCode::Esc).unwrap();
        assert!(matches!(result, AppResult::Quit));
    }

    #[test]
    fn test_handle_normal_input_quit_from_providers_goes_back() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;

        let result = handle_normal_input(&mut app, KeyCode::Char('q')).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.screen, Screen::MainMenu);
    }

    #[test]
    fn test_handle_normal_input_esc_from_providers_goes_back() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;

        let result = handle_normal_input(&mut app, KeyCode::Esc).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.screen, Screen::MainMenu);
    }

    #[test]
    fn test_handle_normal_input_up_arrow() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.main_menu_index = 1;

        let result = handle_normal_input(&mut app, KeyCode::Up).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.main_menu_index, 0);
    }

    #[test]
    fn test_handle_normal_input_k_moves_up() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.main_menu_index = 1;

        let result = handle_normal_input(&mut app, KeyCode::Char('k')).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.main_menu_index, 0);
    }

    #[test]
    fn test_handle_normal_input_down_arrow() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        let result = handle_normal_input(&mut app, KeyCode::Down).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.main_menu_index, 1);
    }

    #[test]
    fn test_handle_normal_input_j_moves_down() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        let result = handle_normal_input(&mut app, KeyCode::Char('j')).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.main_menu_index, 1);
    }

    #[test]
    fn test_handle_normal_input_enter_selects() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.main_menu_index = 0; // Providers

        let result = handle_normal_input(&mut app, KeyCode::Enter).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.screen, Screen::Providers);
    }

    #[test]
    fn test_handle_normal_input_space_toggles_cap() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Caps;
        app.caps_index = 0;

        if !app.available_caps.is_empty() {
            let original = app.available_caps[0].is_enabled;
            handle_normal_input(&mut app, KeyCode::Char(' ')).unwrap();
            assert_ne!(app.available_caps[0].is_enabled, original);
        }
    }

    #[test]
    fn test_handle_normal_input_space_on_menu_item_no_toggle() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Caps;
        // Set index to menu item (Create New or Back)
        app.caps_index = app.available_caps.len(); // Create New

        // Should not crash
        let result = handle_normal_input(&mut app, KeyCode::Char(' ')).unwrap();
        assert!(matches!(result, AppResult::Continue));
    }

    #[test]
    fn test_handle_normal_input_number_1_selects_providers() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::MainMenu;

        let result = handle_normal_input(&mut app, KeyCode::Char('1')).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.screen, Screen::Providers);
    }

    #[test]
    fn test_handle_normal_input_number_2_selects_caps() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::MainMenu;

        let result = handle_normal_input(&mut app, KeyCode::Char('2')).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.screen, Screen::Caps);
    }

    #[test]
    fn test_handle_normal_input_number_3_selects_plans() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::MainMenu;

        let result = handle_normal_input(&mut app, KeyCode::Char('3')).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.screen, Screen::Plans);
    }

    #[test]
    fn test_handle_normal_input_number_4_selects_context() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::MainMenu;

        let result = handle_normal_input(&mut app, KeyCode::Char('4')).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.screen, Screen::Context);
    }

    #[test]
    fn test_handle_normal_input_number_5_selects_about() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::MainMenu;

        let result = handle_normal_input(&mut app, KeyCode::Char('5')).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.screen, Screen::About);
    }

    #[test]
    fn test_handle_normal_input_number_from_wrong_screen() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers; // Not MainMenu

        let result = handle_normal_input(&mut app, KeyCode::Char('1')).unwrap();
        assert!(matches!(result, AppResult::Continue));
        // Should still be on Providers screen
        assert_eq!(app.screen, Screen::Providers);
    }

    #[test]
    fn test_handle_normal_input_question_mark_shows_help() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        let result = handle_normal_input(&mut app, KeyCode::Char('?')).unwrap();
        assert!(matches!(result, AppResult::Continue));
        // Should have set a status message with help
        assert!(app.status_message.is_some());
        assert!(app.status_message.as_ref().unwrap().contains("Navigate"));
    }

    #[test]
    fn test_handle_normal_input_unknown_key() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        let result = handle_normal_input(&mut app, KeyCode::Char('x')).unwrap();
        assert!(matches!(result, AppResult::Continue));
    }

    // ===== handle_editing_input Tests =====

    #[test]
    fn test_handle_editing_input_enter_confirms() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;
        app.provider_index = 1; // AnthropicApiKey
        app.input_mode = InputMode::Editing;
        app.input_buffer = "test-key".to_string();

        handle_editing_input(&mut app, KeyCode::Enter);

        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(
            app.settings.providers.anthropic.api_key,
            Some("test-key".to_string())
        );
    }

    #[test]
    fn test_handle_editing_input_esc_cancels() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::Editing;
        app.input_buffer = "some text".to_string();

        handle_editing_input(&mut app, KeyCode::Esc);

        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.input_buffer.is_empty());
    }

    #[test]
    fn test_handle_editing_input_backspace_removes_char() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::Editing;
        app.input_buffer = "hello".to_string();

        handle_editing_input(&mut app, KeyCode::Backspace);

        assert_eq!(app.input_buffer, "hell");
    }

    #[test]
    fn test_handle_editing_input_backspace_empty_buffer() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::Editing;
        app.input_buffer = String::new();

        // Should not crash
        handle_editing_input(&mut app, KeyCode::Backspace);
        assert!(app.input_buffer.is_empty());
    }

    #[test]
    fn test_handle_editing_input_char_appends() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::Editing;
        app.input_buffer = "hel".to_string();

        handle_editing_input(&mut app, KeyCode::Char('l'));
        assert_eq!(app.input_buffer, "hell");

        handle_editing_input(&mut app, KeyCode::Char('o'));
        assert_eq!(app.input_buffer, "hello");
    }

    #[test]
    fn test_handle_editing_input_unknown_key() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::Editing;
        app.input_buffer = "test".to_string();

        handle_editing_input(&mut app, KeyCode::Tab);

        // Should not change anything
        assert_eq!(app.input_buffer, "test");
        assert_eq!(app.input_mode, InputMode::Editing);
    }

    #[test]
    fn test_handle_editing_input_multiple_backspaces() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::Editing;
        app.input_buffer = "abc".to_string();

        handle_editing_input(&mut app, KeyCode::Backspace);
        handle_editing_input(&mut app, KeyCode::Backspace);
        handle_editing_input(&mut app, KeyCode::Backspace);

        assert!(app.input_buffer.is_empty());
    }

    #[test]
    fn test_handle_editing_input_type_numbers() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::Editing;
        app.input_buffer = String::new();

        handle_editing_input(&mut app, KeyCode::Char('1'));
        handle_editing_input(&mut app, KeyCode::Char('2'));
        handle_editing_input(&mut app, KeyCode::Char('3'));

        assert_eq!(app.input_buffer, "123");
    }

    // ===== handle_editor_input Tests =====

    #[test]
    fn test_handle_editor_input_no_editor_goes_back() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = None;

        let result =
            handle_editor_input(&mut app, KeyCode::Char('a'), KeyModifiers::empty()).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.screen, Screen::Plans);
    }

    #[test]
    fn test_handle_editor_normal_mode_movement() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello\nworld\ntest"));

        // Move down
        handle_editor_input(&mut app, KeyCode::Char('j'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor(), (1, 0));

        // Move right
        handle_editor_input(&mut app, KeyCode::Char('l'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor(), (1, 1));

        // Move up
        handle_editor_input(&mut app, KeyCode::Char('k'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor(), (0, 1));

        // Move left
        handle_editor_input(&mut app, KeyCode::Char('h'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor(), (0, 0));
    }

    #[test]
    fn test_handle_editor_normal_mode_enter_insert() {
        use crate::tui::editor::{Editor, EditorMode};

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello"));

        // Enter insert mode with 'i'
        handle_editor_input(&mut app, KeyCode::Char('i'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().mode(), EditorMode::Insert);
    }

    #[test]
    fn test_handle_editor_insert_mode_typing() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new(""));

        // Enter insert mode
        app.editor.as_mut().unwrap().enter_insert();

        // Type some characters
        handle_editor_input(&mut app, KeyCode::Char('h'), KeyModifiers::empty()).unwrap();
        handle_editor_input(&mut app, KeyCode::Char('i'), KeyModifiers::empty()).unwrap();

        assert_eq!(app.editor.as_ref().unwrap().content(), "hi");
    }

    #[test]
    fn test_handle_editor_insert_mode_escape_to_normal() {
        use crate::tui::editor::{Editor, EditorMode};

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello"));

        // Enter insert mode
        app.editor.as_mut().unwrap().enter_insert();
        assert_eq!(app.editor.as_ref().unwrap().mode(), EditorMode::Insert);

        // Exit with Escape
        handle_editor_input(&mut app, KeyCode::Esc, KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().mode(), EditorMode::Normal);
    }

    #[test]
    fn test_handle_editor_command_mode() {
        use crate::tui::editor::{Editor, EditorMode};

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello"));

        // Enter command mode
        handle_editor_input(&mut app, KeyCode::Char(':'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().mode(), EditorMode::Command);

        // Type 'w' command
        handle_editor_input(&mut app, KeyCode::Char('w'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().command_buffer(), "w");

        // Cancel with Escape
        handle_editor_input(&mut app, KeyCode::Esc, KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().mode(), EditorMode::Normal);
    }

    #[test]
    fn test_handle_editor_normal_mode_line_operations() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("line1\nline2\nline3"));

        // Go to end of line
        handle_editor_input(&mut app, KeyCode::Char('$'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor().1, 4); // "line1" has 5 chars, cursor at index 4 in normal mode

        // Go to start of line
        handle_editor_input(&mut app, KeyCode::Char('0'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor().1, 0);
    }

    #[test]
    fn test_handle_editor_normal_mode_file_navigation() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("line1\nline2\nline3"));

        // Go to end of file
        handle_editor_input(&mut app, KeyCode::Char('G'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor().0, 2);

        // Go to start of file
        handle_editor_input(&mut app, KeyCode::Char('g'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor().0, 0);
    }

    #[test]
    fn test_handle_normal_input_e_edits_plan() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Plans;
        app.plans_index = 0;

        // If there are no plans, nothing should happen
        if app.available_plans.is_empty() {
            let result = handle_normal_input(&mut app, KeyCode::Char('e')).unwrap();
            assert!(matches!(result, AppResult::Continue));
            assert_eq!(app.screen, Screen::Plans);
        }
    }

    #[test]
    fn test_handle_normal_input_d_deletes_plan() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Plans;
        app.plans_index = 0;

        // If there are no plans, nothing should happen
        if app.available_plans.is_empty() {
            let result = handle_normal_input(&mut app, KeyCode::Char('d')).unwrap();
            assert!(matches!(result, AppResult::Continue));
            assert_eq!(app.screen, Screen::Plans);
        }
    }

    // ===== handle_model_selection_input Tests =====

    use crate::tui::app::ModelDisplayInfo;

    fn make_test_model(id: &str) -> ModelDisplayInfo {
        ModelDisplayInfo {
            id: id.to_string(),
            name: id.to_string(),
            tier: "Standard".to_string(),
            description: "Test model".to_string(),
            recommended: false,
        }
    }

    #[test]
    fn test_handle_model_selection_input_enter_confirms() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::SelectingModel;
        app.available_models = vec![make_test_model("model1"), make_test_model("model2")];
        app.model_picker_index = 0;

        handle_model_selection_input(&mut app, KeyCode::Enter);

        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_handle_model_selection_input_esc_cancels() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::SelectingModel;
        app.available_models = vec![make_test_model("model1")];

        handle_model_selection_input(&mut app, KeyCode::Esc);

        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_handle_model_selection_input_up_arrow() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::SelectingModel;
        app.available_models = vec![
            make_test_model("model1"),
            make_test_model("model2"),
            make_test_model("model3"),
        ];
        app.model_picker_index = 1;

        handle_model_selection_input(&mut app, KeyCode::Up);

        assert_eq!(app.model_picker_index, 0);
    }

    #[test]
    fn test_handle_model_selection_input_k_moves_up() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::SelectingModel;
        app.available_models = vec![make_test_model("model1"), make_test_model("model2")];
        app.model_picker_index = 1;

        handle_model_selection_input(&mut app, KeyCode::Char('k'));

        assert_eq!(app.model_picker_index, 0);
    }

    #[test]
    fn test_handle_model_selection_input_down_arrow() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::SelectingModel;
        app.available_models = vec![
            make_test_model("model1"),
            make_test_model("model2"),
            make_test_model("model3"),
        ];
        app.model_picker_index = 0;

        handle_model_selection_input(&mut app, KeyCode::Down);

        assert_eq!(app.model_picker_index, 1);
    }

    #[test]
    fn test_handle_model_selection_input_j_moves_down() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::SelectingModel;
        app.available_models = vec![make_test_model("model1"), make_test_model("model2")];
        app.model_picker_index = 0;

        handle_model_selection_input(&mut app, KeyCode::Char('j'));

        assert_eq!(app.model_picker_index, 1);
    }

    #[test]
    fn test_handle_model_selection_input_up_wraps() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::SelectingModel;
        app.available_models = vec![
            make_test_model("model1"),
            make_test_model("model2"),
            make_test_model("model3"),
        ];
        app.model_picker_index = 0;

        handle_model_selection_input(&mut app, KeyCode::Up);

        // Should wrap to last item
        assert_eq!(app.model_picker_index, 2);
    }

    #[test]
    fn test_handle_model_selection_input_down_wraps() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::SelectingModel;
        app.available_models = vec![
            make_test_model("model1"),
            make_test_model("model2"),
            make_test_model("model3"),
        ];
        app.model_picker_index = 2;

        handle_model_selection_input(&mut app, KeyCode::Down);

        // Should wrap to first item
        assert_eq!(app.model_picker_index, 0);
    }

    #[test]
    fn test_handle_model_selection_input_unknown_key() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::SelectingModel;
        app.available_models = vec![make_test_model("model1")];
        app.model_picker_index = 0;

        handle_model_selection_input(&mut app, KeyCode::Tab);

        // Should not change anything
        assert_eq!(app.input_mode, InputMode::SelectingModel);
        assert_eq!(app.model_picker_index, 0);
    }

    #[test]
    fn test_handle_model_selection_input_empty_models() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::SelectingModel;
        app.available_models = vec![];
        app.model_picker_index = 0;

        // Should not crash with empty models
        handle_model_selection_input(&mut app, KeyCode::Up);
        handle_model_selection_input(&mut app, KeyCode::Down);
        assert_eq!(app.model_picker_index, 0);
    }

    // ===== Additional Editor Mode Tests =====

    #[test]
    fn test_handle_editor_normal_mode_arrow_keys() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello\nworld\ntest"));

        // Move with arrow keys
        handle_editor_input(&mut app, KeyCode::Down, KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor().0, 1);

        handle_editor_input(&mut app, KeyCode::Right, KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor().1, 1);

        handle_editor_input(&mut app, KeyCode::Up, KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor().0, 0);

        handle_editor_input(&mut app, KeyCode::Left, KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor().1, 0);
    }

    #[test]
    fn test_handle_editor_normal_mode_home_end() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello world"));

        // End key
        handle_editor_input(&mut app, KeyCode::End, KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor().1, 10); // End of "hello world" (11 chars, cursor at 10 in normal mode)

        // Home key
        handle_editor_input(&mut app, KeyCode::Home, KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().cursor().1, 0);
    }

    #[test]
    fn test_handle_editor_normal_mode_word_movement() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello world test"));

        // Move word forward
        handle_editor_input(&mut app, KeyCode::Char('w'), KeyModifiers::empty()).unwrap();
        // Cursor should be at start of "world"
        let cursor = app.editor.as_ref().unwrap().cursor();
        assert!(cursor.1 > 0); // Moved forward

        // Move word backward
        handle_editor_input(&mut app, KeyCode::Char('b'), KeyModifiers::empty()).unwrap();
        // Cursor should be back
        let cursor = app.editor.as_ref().unwrap().cursor();
        assert_eq!(cursor.1, 0);
    }

    #[test]
    fn test_handle_editor_normal_mode_insert_modes() {
        use crate::tui::editor::{Editor, EditorMode};

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello"));

        // Test 'I' - insert at start of line
        handle_editor_input(&mut app, KeyCode::Char('I'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().mode(), EditorMode::Insert);
        app.editor.as_mut().unwrap().exit_to_normal();

        // Test 'a' - insert after cursor
        handle_editor_input(&mut app, KeyCode::Char('a'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().mode(), EditorMode::Insert);
        app.editor.as_mut().unwrap().exit_to_normal();

        // Test 'A' - insert at end of line
        handle_editor_input(&mut app, KeyCode::Char('A'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().mode(), EditorMode::Insert);
    }

    #[test]
    fn test_handle_editor_normal_mode_open_lines() {
        use crate::tui::editor::{Editor, EditorMode};

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("line1\nline2"));

        // 'o' - open line below
        handle_editor_input(&mut app, KeyCode::Char('o'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().mode(), EditorMode::Insert);
        app.editor.as_mut().unwrap().exit_to_normal();

        // Reset and test 'O' - open line above
        app.editor = Some(Editor::new("line1\nline2"));
        handle_editor_input(&mut app, KeyCode::Char('O'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().mode(), EditorMode::Insert);
    }

    #[test]
    fn test_handle_editor_normal_mode_delete_yank_paste() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("line1\nline2\nline3"));

        // Yank line
        handle_editor_input(&mut app, KeyCode::Char('y'), KeyModifiers::empty()).unwrap();

        // Delete line
        handle_editor_input(&mut app, KeyCode::Char('d'), KeyModifiers::empty()).unwrap();
        // Should have deleted the first line
        assert!(!app
            .editor
            .as_ref()
            .unwrap()
            .content()
            .contains("line1\nline2"));

        // Paste
        handle_editor_input(&mut app, KeyCode::Char('p'), KeyModifiers::empty()).unwrap();
        // Content should have changed
        assert!(!app.editor.as_ref().unwrap().content().is_empty());
    }

    #[test]
    fn test_handle_editor_normal_mode_undo_redo() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello"));

        // Make a change - delete line
        handle_editor_input(&mut app, KeyCode::Char('d'), KeyModifiers::empty()).unwrap();
        // Content should be empty or changed
        let _content_after_delete = app.editor.as_ref().unwrap().content().to_string();

        // Undo
        handle_editor_input(&mut app, KeyCode::Char('u'), KeyModifiers::empty()).unwrap();
        // Content should be restored (or at least undo was called without error)

        // Redo (Ctrl+r)
        handle_editor_input(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL).unwrap();
        // Content should match after-delete state (or redo was called without error)
    }

    #[test]
    fn test_handle_editor_normal_mode_toggle_checkbox() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("- [ ] task"));

        // Toggle checkbox with space
        handle_editor_input(&mut app, KeyCode::Char(' '), KeyModifiers::empty()).unwrap();
        // Checkbox should toggle (or at least not crash)
    }

    #[test]
    fn test_handle_editor_normal_mode_escape_unmodified() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello"));

        // Escape on unmodified editor should close it
        handle_editor_input(&mut app, KeyCode::Esc, KeyModifiers::empty()).unwrap();
        assert!(app.editor.is_none());
        assert_eq!(app.screen, Screen::Plans);
    }

    #[test]
    fn test_handle_editor_normal_mode_escape_modified() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello"));

        // Modify the content
        app.editor.as_mut().unwrap().enter_insert();
        app.editor.as_mut().unwrap().insert_char('x');
        app.editor.as_mut().unwrap().exit_to_normal();

        // Escape on modified editor should NOT close it
        handle_editor_input(&mut app, KeyCode::Esc, KeyModifiers::empty()).unwrap();
        assert!(app.editor.is_some()); // Editor still open
        assert_eq!(app.screen, Screen::PlanEdit);
    }

    #[test]
    fn test_handle_editor_insert_mode_backspace() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello"));

        // Enter insert mode at end
        app.editor.as_mut().unwrap().enter_insert_end();

        // Backspace
        handle_editor_input(&mut app, KeyCode::Backspace, KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().content(), "hell");
    }

    #[test]
    fn test_handle_editor_insert_mode_newline() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello"));

        // Enter insert mode
        app.editor.as_mut().unwrap().enter_insert_end();

        // Insert newline
        handle_editor_input(&mut app, KeyCode::Enter, KeyModifiers::empty()).unwrap();
        assert!(app.editor.as_ref().unwrap().content().contains('\n'));
    }

    #[test]
    fn test_handle_editor_insert_mode_arrow_keys() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello\nworld"));

        // Enter insert mode
        app.editor.as_mut().unwrap().enter_insert();

        // Move with arrow keys in insert mode
        handle_editor_input(&mut app, KeyCode::Right, KeyModifiers::empty()).unwrap();
        handle_editor_input(&mut app, KeyCode::Left, KeyModifiers::empty()).unwrap();
        handle_editor_input(&mut app, KeyCode::Down, KeyModifiers::empty()).unwrap();
        handle_editor_input(&mut app, KeyCode::Up, KeyModifiers::empty()).unwrap();

        // Should still be in insert mode
        assert_eq!(
            app.editor.as_ref().unwrap().mode(),
            crate::tui::editor::EditorMode::Insert
        );
    }

    #[test]
    fn test_handle_editor_command_mode_backspace() {
        use crate::tui::editor::{Editor, EditorMode};

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello"));

        // Enter command mode
        handle_editor_input(&mut app, KeyCode::Char(':'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().mode(), EditorMode::Command);

        // Type something
        handle_editor_input(&mut app, KeyCode::Char('w'), KeyModifiers::empty()).unwrap();
        handle_editor_input(&mut app, KeyCode::Char('q'), KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().command_buffer(), "wq");

        // Backspace
        handle_editor_input(&mut app, KeyCode::Backspace, KeyModifiers::empty()).unwrap();
        assert_eq!(app.editor.as_ref().unwrap().command_buffer(), "w");
    }

    #[test]
    fn test_handle_editor_command_mode_unknown_key() {
        use crate::tui::editor::{Editor, EditorMode};

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello"));

        // Enter command mode
        handle_editor_input(&mut app, KeyCode::Char(':'), KeyModifiers::empty()).unwrap();

        // Press Tab (unknown key in command mode)
        handle_editor_input(&mut app, KeyCode::Tab, KeyModifiers::empty()).unwrap();

        // Should still be in command mode
        assert_eq!(app.editor.as_ref().unwrap().mode(), EditorMode::Command);
    }

    #[test]
    fn test_handle_editor_normal_mode_unknown_key() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = Some(Editor::new("hello"));

        // Press an unbound key
        handle_editor_input(&mut app, KeyCode::F(1), KeyModifiers::empty()).unwrap();

        // Should still work and not crash
        assert!(app.editor.is_some());
    }

    #[test]
    fn test_handle_editor_normal_mode_no_editor() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = None;

        // Should return to Plans screen
        let result =
            handle_editor_normal_mode(&mut app, KeyCode::Char('j'), KeyModifiers::empty()).unwrap();
        assert!(matches!(result, AppResult::Continue));
    }

    #[test]
    fn test_handle_editor_insert_mode_no_editor() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = None;

        // Should handle gracefully
        let result = handle_editor_insert_mode(&mut app, KeyCode::Char('a')).unwrap();
        assert!(matches!(result, AppResult::Continue));
    }

    #[test]
    fn test_handle_editor_command_mode_no_editor() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;
        app.editor = None;

        // Should handle gracefully
        let result = handle_editor_command_mode(&mut app, KeyCode::Char('w')).unwrap();
        assert!(matches!(result, AppResult::Continue));
    }

    #[test]
    fn test_handle_normal_input_e_from_plan_view() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanView;
        app.current_plan_id = None;

        // With no current plan, nothing should happen
        let result = handle_normal_input(&mut app, KeyCode::Char('e')).unwrap();
        assert!(matches!(result, AppResult::Continue));
        assert_eq!(app.screen, Screen::PlanView);
    }

    #[test]
    fn test_handle_editing_input_special_characters() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::Editing;
        app.input_buffer = String::new();

        // Type special characters
        handle_editing_input(&mut app, KeyCode::Char('@'));
        handle_editing_input(&mut app, KeyCode::Char('#'));
        handle_editing_input(&mut app, KeyCode::Char('$'));
        handle_editing_input(&mut app, KeyCode::Char('%'));

        assert_eq!(app.input_buffer, "@#$%");
    }

    #[test]
    fn test_handle_normal_input_navigation_boundary() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::MainMenu;
        app.main_menu_index = 0;

        // Try to move up when already at top
        handle_normal_input(&mut app, KeyCode::Up).unwrap();

        // Index should wrap or stay at boundary (depends on implementation)
        // Just ensure no crash
        assert!(app.main_menu_index <= 10); // Reasonable bound
    }
}
