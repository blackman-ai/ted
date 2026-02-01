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
    let editor = app.editor.as_mut().unwrap();

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
    let editor = app.editor.as_mut().unwrap();

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
    let editor = app.editor.as_mut().unwrap();

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
}
