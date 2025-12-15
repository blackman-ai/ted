// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Vim-style editor for the TUI
//!
//! Provides a simple vim-like text editor with modal editing.

use super::undo::{EditorSnapshot, UndoHistory};

/// Vim editing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    /// Normal mode (navigation)
    Normal,
    /// Insert mode (typing)
    Insert,
    /// Command mode (ex commands like :w, :q)
    Command,
}

/// Result of executing a command
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResult {
    /// Continue editing
    Continue,
    /// Save and quit
    SaveQuit,
    /// Quit without saving
    Quit,
    /// Save but continue editing
    Save,
    /// Invalid command
    Invalid(String),
}

/// Vim-style text editor
#[derive(Debug)]
pub struct Editor {
    /// The text content as lines
    lines: Vec<String>,
    /// Cursor position (line, column)
    cursor: (usize, usize),
    /// Current editing mode
    mode: EditorMode,
    /// Command buffer for ex commands
    command_buffer: String,
    /// Undo/redo history
    history: UndoHistory,
    /// Whether the content has been modified
    modified: bool,
    /// Yank (copy) buffer for line operations
    yank_buffer: Option<String>,
    /// Scroll offset for display
    scroll_offset: usize,
}

impl Editor {
    /// Create a new editor with the given content
    pub fn new(content: &str) -> Self {
        let lines: Vec<String> = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(|s| s.to_string()).collect()
        };

        Self {
            lines,
            cursor: (0, 0),
            mode: EditorMode::Normal,
            command_buffer: String::new(),
            history: UndoHistory::new(),
            modified: false,
            yank_buffer: None,
            scroll_offset: 0,
        }
    }

    /// Get the current mode
    pub fn mode(&self) -> EditorMode {
        self.mode
    }

    /// Get cursor position (line, column)
    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    /// Get scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Get whether content has been modified
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Get the content as a string
    pub fn content(&self) -> String {
        self.lines.join("\n")
    }

    /// Get the lines for display
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Get the line count
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Get the command buffer (for display in command mode)
    pub fn command_buffer(&self) -> &str {
        &self.command_buffer
    }

    /// Save current state to undo history
    fn save_to_history(&mut self) {
        let snapshot = EditorSnapshot {
            content: self.content(),
            cursor: self.cursor,
        };
        self.history.push(snapshot);
    }

    /// Restore state from a snapshot
    fn restore_snapshot(&mut self, snapshot: EditorSnapshot) {
        self.lines = if snapshot.content.is_empty() {
            vec![String::new()]
        } else {
            snapshot.content.lines().map(|s| s.to_string()).collect()
        };
        self.cursor = snapshot.cursor;
        self.clamp_cursor();
    }

    /// Clamp cursor to valid position
    fn clamp_cursor(&mut self) {
        // Clamp line
        if self.cursor.0 >= self.lines.len() {
            self.cursor.0 = self.lines.len().saturating_sub(1);
        }

        // Clamp column
        let line_len = self.current_line_len();
        let max_col = if self.mode == EditorMode::Insert {
            line_len
        } else {
            line_len.saturating_sub(1)
        };

        if self.cursor.1 > max_col && max_col > 0 {
            self.cursor.1 = max_col;
        } else if line_len == 0 {
            self.cursor.1 = 0;
        }
    }

    /// Get current line length
    fn current_line_len(&self) -> usize {
        self.lines.get(self.cursor.0).map(|l| l.len()).unwrap_or(0)
    }

    /// Adjust scroll to keep cursor visible
    pub fn adjust_scroll(&mut self, visible_lines: usize) {
        if self.cursor.0 < self.scroll_offset {
            self.scroll_offset = self.cursor.0;
        } else if self.cursor.0 >= self.scroll_offset + visible_lines {
            self.scroll_offset = self.cursor.0 - visible_lines + 1;
        }
    }

    // ===== Mode transitions =====

    /// Enter insert mode (at cursor)
    pub fn enter_insert(&mut self) {
        self.mode = EditorMode::Insert;
    }

    /// Enter insert mode at end of line
    pub fn enter_insert_end(&mut self) {
        self.cursor.1 = self.current_line_len();
        self.mode = EditorMode::Insert;
    }

    /// Enter insert mode at start of line
    pub fn enter_insert_start(&mut self) {
        self.cursor.1 = 0;
        self.mode = EditorMode::Insert;
    }

    /// Enter insert mode after cursor
    pub fn enter_insert_after(&mut self) {
        let line_len = self.current_line_len();
        if line_len > 0 && self.cursor.1 < line_len {
            self.cursor.1 += 1;
        }
        self.mode = EditorMode::Insert;
    }

    /// Enter command mode
    pub fn enter_command(&mut self) {
        self.command_buffer.clear();
        self.mode = EditorMode::Command;
    }

    /// Exit to normal mode
    pub fn exit_to_normal(&mut self) {
        self.mode = EditorMode::Normal;
        self.command_buffer.clear();
        self.clamp_cursor();
    }

    // ===== Normal mode movements =====

    /// Move cursor left (h)
    pub fn move_left(&mut self) {
        if self.cursor.1 > 0 {
            self.cursor.1 -= 1;
        }
    }

    /// Move cursor right (l)
    pub fn move_right(&mut self) {
        let max_col = self.current_line_len().saturating_sub(1);
        if self.cursor.1 < max_col || (self.mode == EditorMode::Insert && self.cursor.1 < self.current_line_len()) {
            self.cursor.1 += 1;
        }
    }

    /// Move cursor up (k)
    pub fn move_up(&mut self) {
        if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
            self.clamp_cursor();
        }
    }

    /// Move cursor down (j)
    pub fn move_down(&mut self) {
        if self.cursor.0 < self.lines.len() - 1 {
            self.cursor.0 += 1;
            self.clamp_cursor();
        }
    }

    /// Move to start of line (0)
    pub fn move_line_start(&mut self) {
        self.cursor.1 = 0;
    }

    /// Move to end of line ($)
    pub fn move_line_end(&mut self) {
        let len = self.current_line_len();
        if self.mode == EditorMode::Insert {
            self.cursor.1 = len;
        } else {
            self.cursor.1 = len.saturating_sub(1);
        }
    }

    /// Move to first line (gg)
    pub fn move_file_start(&mut self) {
        self.cursor = (0, 0);
    }

    /// Move to last line (G)
    pub fn move_file_end(&mut self) {
        self.cursor.0 = self.lines.len().saturating_sub(1);
        self.cursor.1 = 0;
    }

    /// Move forward one word (w)
    pub fn move_word_forward(&mut self) {
        let line = &self.lines[self.cursor.0];
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor.1;

        // Skip current word
        while col < chars.len() && !chars[col].is_whitespace() {
            col += 1;
        }

        // Skip whitespace
        while col < chars.len() && chars[col].is_whitespace() {
            col += 1;
        }

        if col >= chars.len() && self.cursor.0 < self.lines.len() - 1 {
            // Move to next line
            self.cursor.0 += 1;
            self.cursor.1 = 0;
        } else {
            self.cursor.1 = col.min(chars.len().saturating_sub(1));
        }
    }

    /// Move backward one word (b)
    pub fn move_word_backward(&mut self) {
        if self.cursor.1 == 0 {
            if self.cursor.0 > 0 {
                self.cursor.0 -= 1;
                self.cursor.1 = self.current_line_len().saturating_sub(1);
            }
            return;
        }

        let line = &self.lines[self.cursor.0];
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor.1.saturating_sub(1);

        // Skip whitespace
        while col > 0 && chars[col].is_whitespace() {
            col -= 1;
        }

        // Skip to start of word
        while col > 0 && !chars[col - 1].is_whitespace() {
            col -= 1;
        }

        self.cursor.1 = col;
    }

    // ===== Insert mode operations =====

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, c: char) {
        if self.mode != EditorMode::Insert {
            return;
        }

        self.save_to_history();

        if let Some(line) = self.lines.get_mut(self.cursor.0) {
            if self.cursor.1 >= line.len() {
                line.push(c);
            } else {
                line.insert(self.cursor.1, c);
            }
            self.cursor.1 += 1;
            self.modified = true;
        }
    }

    /// Delete character before cursor (backspace)
    pub fn backspace(&mut self) {
        if self.mode != EditorMode::Insert {
            return;
        }

        self.save_to_history();

        if self.cursor.1 > 0 {
            if let Some(line) = self.lines.get_mut(self.cursor.0) {
                line.remove(self.cursor.1 - 1);
                self.cursor.1 -= 1;
                self.modified = true;
            }
        } else if self.cursor.0 > 0 {
            // Join with previous line
            let current_line = self.lines.remove(self.cursor.0);
            self.cursor.0 -= 1;
            let prev_len = self.lines[self.cursor.0].len();
            self.lines[self.cursor.0].push_str(&current_line);
            self.cursor.1 = prev_len;
            self.modified = true;
        }
    }

    /// Insert a newline
    pub fn insert_newline(&mut self) {
        if self.mode != EditorMode::Insert {
            return;
        }

        self.save_to_history();

        let current_line = &self.lines[self.cursor.0];
        let rest = current_line[self.cursor.1..].to_string();
        self.lines[self.cursor.0] = current_line[..self.cursor.1].to_string();
        self.cursor.0 += 1;
        self.cursor.1 = 0;
        self.lines.insert(self.cursor.0, rest);
        self.modified = true;
    }

    // ===== Normal mode operations =====

    /// Delete current line (dd)
    pub fn delete_line(&mut self) {
        if self.lines.len() <= 1 && self.lines[0].is_empty() {
            return;
        }

        self.save_to_history();

        // Store in yank buffer
        self.yank_buffer = Some(self.lines[self.cursor.0].clone());

        if self.lines.len() == 1 {
            self.lines[0].clear();
        } else {
            self.lines.remove(self.cursor.0);
            if self.cursor.0 >= self.lines.len() {
                self.cursor.0 = self.lines.len().saturating_sub(1);
            }
        }
        self.cursor.1 = 0;
        self.modified = true;
    }

    /// Yank (copy) current line (yy)
    pub fn yank_line(&mut self) {
        self.yank_buffer = Some(self.lines[self.cursor.0].clone());
    }

    /// Paste yanked line (p)
    pub fn paste(&mut self) {
        if let Some(ref yanked) = self.yank_buffer.clone() {
            self.save_to_history();
            self.cursor.0 += 1;
            if self.cursor.0 > self.lines.len() {
                self.lines.push(yanked.clone());
            } else {
                self.lines.insert(self.cursor.0, yanked.clone());
            }
            self.cursor.1 = 0;
            self.modified = true;
        }
    }

    /// Open new line below (o)
    pub fn open_line_below(&mut self) {
        self.save_to_history();
        self.cursor.0 += 1;
        self.lines.insert(self.cursor.0, String::new());
        self.cursor.1 = 0;
        self.mode = EditorMode::Insert;
        self.modified = true;
    }

    /// Open new line above (O)
    pub fn open_line_above(&mut self) {
        self.save_to_history();
        self.lines.insert(self.cursor.0, String::new());
        self.cursor.1 = 0;
        self.mode = EditorMode::Insert;
        self.modified = true;
    }

    /// Toggle checkbox under cursor (Space in normal mode)
    pub fn toggle_checkbox(&mut self) {
        let line = self.lines[self.cursor.0].clone();

        // Check for checkbox patterns
        if let Some(idx) = line.find("- [ ]") {
            self.save_to_history();
            let mut new_line = line;
            new_line.replace_range(idx..idx + 5, "- [x]");
            self.lines[self.cursor.0] = new_line;
            self.modified = true;
        } else if let Some(idx) = line.find("- [x]") {
            self.save_to_history();
            let mut new_line = line;
            new_line.replace_range(idx..idx + 5, "- [ ]");
            self.lines[self.cursor.0] = new_line;
            self.modified = true;
        } else if let Some(idx) = line.find("- [X]") {
            self.save_to_history();
            let mut new_line = line;
            new_line.replace_range(idx..idx + 5, "- [ ]");
            self.lines[self.cursor.0] = new_line;
            self.modified = true;
        }
    }

    // ===== Undo/Redo =====

    /// Undo last change (u)
    pub fn undo(&mut self) {
        let current = EditorSnapshot {
            content: self.content(),
            cursor: self.cursor,
        };

        if let Some(snapshot) = self.history.undo(current) {
            self.restore_snapshot(snapshot);
        }
    }

    /// Redo last undone change (Ctrl+R)
    pub fn redo(&mut self) {
        let current = EditorSnapshot {
            content: self.content(),
            cursor: self.cursor,
        };

        if let Some(snapshot) = self.history.redo(current) {
            self.restore_snapshot(snapshot);
        }
    }

    // ===== Command mode =====

    /// Add character to command buffer
    pub fn command_input(&mut self, c: char) {
        if self.mode == EditorMode::Command {
            self.command_buffer.push(c);
        }
    }

    /// Remove character from command buffer
    pub fn command_backspace(&mut self) {
        if self.mode == EditorMode::Command {
            self.command_buffer.pop();
            if self.command_buffer.is_empty() {
                self.exit_to_normal();
            }
        }
    }

    /// Execute the current command
    pub fn execute_command(&mut self) -> CommandResult {
        let cmd = self.command_buffer.trim();

        let result = match cmd {
            "w" => CommandResult::Save,
            "q" => {
                if self.modified {
                    CommandResult::Invalid("No write since last change (use :q! to force)".to_string())
                } else {
                    CommandResult::Quit
                }
            }
            "q!" => CommandResult::Quit,
            "wq" | "x" => CommandResult::SaveQuit,
            _ => CommandResult::Invalid(format!("Unknown command: {}", cmd)),
        };

        self.exit_to_normal();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_editor_empty() {
        let editor = Editor::new("");
        assert_eq!(editor.lines.len(), 1);
        assert_eq!(editor.lines[0], "");
        assert_eq!(editor.cursor, (0, 0));
        assert_eq!(editor.mode, EditorMode::Normal);
    }

    #[test]
    fn test_new_editor_with_content() {
        let editor = Editor::new("line1\nline2\nline3");
        assert_eq!(editor.lines.len(), 3);
        assert_eq!(editor.lines[0], "line1");
        assert_eq!(editor.lines[1], "line2");
        assert_eq!(editor.lines[2], "line3");
    }

    #[test]
    fn test_content_returns_joined_lines() {
        let editor = Editor::new("hello\nworld");
        assert_eq!(editor.content(), "hello\nworld");
    }

    #[test]
    fn test_enter_insert_mode() {
        let mut editor = Editor::new("hello");
        editor.enter_insert();
        assert_eq!(editor.mode, EditorMode::Insert);
    }

    #[test]
    fn test_enter_insert_end() {
        let mut editor = Editor::new("hello");
        editor.enter_insert_end();
        assert_eq!(editor.mode, EditorMode::Insert);
        assert_eq!(editor.cursor.1, 5);
    }

    #[test]
    fn test_enter_insert_start() {
        let mut editor = Editor::new("hello");
        editor.cursor.1 = 3;
        editor.enter_insert_start();
        assert_eq!(editor.mode, EditorMode::Insert);
        assert_eq!(editor.cursor.1, 0);
    }

    #[test]
    fn test_exit_to_normal() {
        let mut editor = Editor::new("hello");
        editor.enter_insert();
        editor.exit_to_normal();
        assert_eq!(editor.mode, EditorMode::Normal);
    }

    #[test]
    fn test_move_hjkl() {
        let mut editor = Editor::new("hello\nworld\ntest");
        editor.cursor = (1, 2);

        editor.move_left();
        assert_eq!(editor.cursor, (1, 1));

        editor.move_right();
        assert_eq!(editor.cursor, (1, 2));

        editor.move_up();
        assert_eq!(editor.cursor, (0, 2));

        editor.move_down();
        assert_eq!(editor.cursor, (1, 2));
    }

    #[test]
    fn test_move_line_start_end() {
        let mut editor = Editor::new("hello world");
        editor.cursor.1 = 5;

        editor.move_line_start();
        assert_eq!(editor.cursor.1, 0);

        editor.move_line_end();
        assert_eq!(editor.cursor.1, 10); // In normal mode, cursor stops before last char
    }

    #[test]
    fn test_move_file_start_end() {
        let mut editor = Editor::new("line1\nline2\nline3");
        editor.cursor = (1, 2);

        editor.move_file_start();
        assert_eq!(editor.cursor, (0, 0));

        editor.move_file_end();
        assert_eq!(editor.cursor.0, 2);
    }

    #[test]
    fn test_insert_char() {
        let mut editor = Editor::new("hello");
        editor.enter_insert();
        editor.cursor.1 = 5;
        editor.insert_char('!');
        assert_eq!(editor.content(), "hello!");
        assert!(editor.modified);
    }

    #[test]
    fn test_backspace() {
        let mut editor = Editor::new("hello");
        editor.enter_insert();
        editor.cursor.1 = 5;
        editor.backspace();
        assert_eq!(editor.content(), "hell");
    }

    #[test]
    fn test_backspace_join_lines() {
        let mut editor = Editor::new("hello\nworld");
        editor.enter_insert();
        editor.cursor = (1, 0);
        editor.backspace();
        assert_eq!(editor.content(), "helloworld");
        assert_eq!(editor.cursor, (0, 5));
    }

    #[test]
    fn test_insert_newline() {
        let mut editor = Editor::new("helloworld");
        editor.enter_insert();
        editor.cursor.1 = 5;
        editor.insert_newline();
        assert_eq!(editor.lines.len(), 2);
        assert_eq!(editor.lines[0], "hello");
        assert_eq!(editor.lines[1], "world");
        assert_eq!(editor.cursor, (1, 0));
    }

    #[test]
    fn test_delete_line() {
        let mut editor = Editor::new("line1\nline2\nline3");
        editor.cursor = (1, 0);
        editor.delete_line();
        assert_eq!(editor.lines.len(), 2);
        assert_eq!(editor.lines[0], "line1");
        assert_eq!(editor.lines[1], "line3");
    }

    #[test]
    fn test_yank_and_paste() {
        let mut editor = Editor::new("line1\nline2");
        editor.cursor = (0, 0);
        editor.yank_line();
        editor.paste();
        assert_eq!(editor.lines.len(), 3);
        assert_eq!(editor.lines[1], "line1");
    }

    #[test]
    fn test_open_line_below() {
        let mut editor = Editor::new("line1\nline2");
        editor.cursor = (0, 0);
        editor.open_line_below();
        assert_eq!(editor.lines.len(), 3);
        assert_eq!(editor.lines[1], "");
        assert_eq!(editor.cursor, (1, 0));
        assert_eq!(editor.mode, EditorMode::Insert);
    }

    #[test]
    fn test_open_line_above() {
        let mut editor = Editor::new("line1\nline2");
        editor.cursor = (1, 0);
        editor.open_line_above();
        assert_eq!(editor.lines.len(), 3);
        assert_eq!(editor.lines[1], "");
        assert_eq!(editor.cursor, (1, 0));
        assert_eq!(editor.mode, EditorMode::Insert);
    }

    #[test]
    fn test_toggle_checkbox_unchecked() {
        let mut editor = Editor::new("- [ ] Task");
        editor.toggle_checkbox();
        assert_eq!(editor.content(), "- [x] Task");
    }

    #[test]
    fn test_toggle_checkbox_checked() {
        let mut editor = Editor::new("- [x] Task");
        editor.toggle_checkbox();
        assert_eq!(editor.content(), "- [ ] Task");
    }

    #[test]
    fn test_undo_redo() {
        let mut editor = Editor::new("hello");
        editor.enter_insert();
        editor.cursor.1 = 5;
        editor.insert_char('!');
        assert_eq!(editor.content(), "hello!");

        editor.undo();
        assert_eq!(editor.content(), "hello");

        editor.redo();
        assert_eq!(editor.content(), "hello!");
    }

    #[test]
    fn test_command_mode_write() {
        let mut editor = Editor::new("test");
        editor.enter_command();
        editor.command_input('w');
        let result = editor.execute_command();
        assert_eq!(result, CommandResult::Save);
        assert_eq!(editor.mode, EditorMode::Normal);
    }

    #[test]
    fn test_command_mode_quit() {
        let mut editor = Editor::new("test");
        editor.enter_command();
        editor.command_input('q');
        let result = editor.execute_command();
        assert_eq!(result, CommandResult::Quit);
    }

    #[test]
    fn test_command_mode_quit_modified() {
        let mut editor = Editor::new("test");
        editor.enter_insert();
        editor.insert_char('!');
        editor.exit_to_normal();
        editor.enter_command();
        editor.command_input('q');
        let result = editor.execute_command();
        assert!(matches!(result, CommandResult::Invalid(_)));
    }

    #[test]
    fn test_command_mode_force_quit() {
        let mut editor = Editor::new("test");
        editor.enter_insert();
        editor.insert_char('!');
        editor.exit_to_normal();
        editor.enter_command();
        editor.command_input('q');
        editor.command_input('!');
        let result = editor.execute_command();
        assert_eq!(result, CommandResult::Quit);
    }

    #[test]
    fn test_command_mode_wq() {
        let mut editor = Editor::new("test");
        editor.enter_command();
        editor.command_input('w');
        editor.command_input('q');
        let result = editor.execute_command();
        assert_eq!(result, CommandResult::SaveQuit);
    }

    #[test]
    fn test_command_backspace() {
        let mut editor = Editor::new("test");
        editor.enter_command();
        editor.command_input('w');
        editor.command_input('q');
        editor.command_backspace();
        assert_eq!(editor.command_buffer, "w");
    }

    #[test]
    fn test_command_backspace_exits_on_empty() {
        let mut editor = Editor::new("test");
        editor.enter_command();
        editor.command_input('w');
        editor.command_backspace();
        editor.command_backspace();
        assert_eq!(editor.mode, EditorMode::Normal);
    }

    #[test]
    fn test_move_word_forward() {
        let mut editor = Editor::new("hello world test");
        editor.cursor.1 = 0;
        editor.move_word_forward();
        assert_eq!(editor.cursor.1, 6);
        editor.move_word_forward();
        assert_eq!(editor.cursor.1, 12);
    }

    #[test]
    fn test_move_word_backward() {
        let mut editor = Editor::new("hello world test");
        editor.cursor.1 = 12;
        editor.move_word_backward();
        assert_eq!(editor.cursor.1, 6);
        editor.move_word_backward();
        assert_eq!(editor.cursor.1, 0);
    }

    #[test]
    fn test_adjust_scroll() {
        let mut editor = Editor::new("1\n2\n3\n4\n5\n6\n7\n8\n9\n10");
        editor.cursor = (7, 0);
        editor.adjust_scroll(5);
        assert!(editor.scroll_offset > 0);
    }
}
