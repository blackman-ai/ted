// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Input state for the chat TUI
//!
//! Manages the input buffer, cursor position, and history navigation.

/// Input state for the text input area
#[derive(Debug, Clone)]
pub struct InputState {
    /// Current input buffer
    pub buffer: String,
    /// Cursor position (character index)
    pub cursor: usize,
    /// History of previous inputs
    pub history: Vec<String>,
    /// Current history index (None = new input, Some(i) = browsing history)
    pub history_index: Option<usize>,
    /// Saved buffer when browsing history
    saved_buffer: Option<String>,
    /// Whether multiline input is enabled
    pub multiline: bool,
    /// Maximum history entries to keep
    max_history: usize,
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    /// Create a new input state
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
            saved_buffer: None,
            multiline: false,
            max_history: 100,
        }
    }

    /// Get the current input text
    pub fn text(&self) -> &str {
        &self.buffer
    }

    /// Check if the input is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Insert a string at the cursor position
    pub fn insert_str(&mut self, s: &str) {
        self.buffer.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Delete the character before the cursor (backspace)
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.buffer.remove(self.cursor);
        }
    }

    /// Delete the character at the cursor (delete)
    pub fn delete(&mut self) {
        if self.cursor < self.buffer.len() {
            self.buffer.remove(self.cursor);
        }
    }

    /// Delete the word before the cursor
    pub fn delete_word(&mut self) {
        // Skip trailing whitespace
        while self.cursor > 0 && self.buffer.chars().nth(self.cursor - 1) == Some(' ') {
            self.cursor -= 1;
            self.buffer.remove(self.cursor);
        }
        // Delete until whitespace or start
        while self.cursor > 0 && self.buffer.chars().nth(self.cursor - 1) != Some(' ') {
            self.cursor -= 1;
            self.buffer.remove(self.cursor);
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.len() {
            self.cursor += 1;
        }
    }

    /// Move cursor to start of line/input
    pub fn move_home(&mut self) {
        if self.multiline {
            // Move to start of current line
            let before_cursor = &self.buffer[..self.cursor];
            if let Some(pos) = before_cursor.rfind('\n') {
                self.cursor = pos + 1;
            } else {
                self.cursor = 0;
            }
        } else {
            self.cursor = 0;
        }
    }

    /// Move cursor to end of line/input
    pub fn move_end(&mut self) {
        if self.multiline {
            // Move to end of current line
            let after_cursor = &self.buffer[self.cursor..];
            if let Some(pos) = after_cursor.find('\n') {
                self.cursor += pos;
            } else {
                self.cursor = self.buffer.len();
            }
        } else {
            self.cursor = self.buffer.len();
        }
    }

    /// Clear the input buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
        self.history_index = None;
        self.saved_buffer = None;
    }

    /// Submit the current input and return it
    /// Adds to history if non-empty
    pub fn submit(&mut self) -> String {
        let text = std::mem::take(&mut self.buffer);
        self.cursor = 0;
        self.history_index = None;
        self.saved_buffer = None;

        // Add to history if non-empty and different from last
        if !text.trim().is_empty() && self.history.last().map(|s| s.as_str()) != Some(&text) {
            self.history.push(text.clone());
            // Trim history if too long
            if self.history.len() > self.max_history {
                self.history.remove(0);
            }
        }

        text
    }

    /// Navigate to previous history entry
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_index {
            None => {
                // Save current buffer and go to most recent history
                self.saved_buffer = Some(self.buffer.clone());
                self.history_index = Some(self.history.len() - 1);
                self.buffer = self.history[self.history.len() - 1].clone();
                self.cursor = self.buffer.len();
            }
            Some(0) => {
                // Already at oldest entry, do nothing
            }
            Some(i) => {
                // Go to older entry
                self.history_index = Some(i - 1);
                self.buffer = self.history[i - 1].clone();
                self.cursor = self.buffer.len();
            }
        }
    }

    /// Navigate to next history entry
    pub fn history_next(&mut self) {
        match self.history_index {
            None => {
                // Not in history mode, do nothing
            }
            Some(i) if i >= self.history.len() - 1 => {
                // At most recent entry, restore saved buffer
                self.history_index = None;
                if let Some(saved) = self.saved_buffer.take() {
                    self.buffer = saved;
                    self.cursor = self.buffer.len();
                }
            }
            Some(i) => {
                // Go to newer entry
                self.history_index = Some(i + 1);
                self.buffer = self.history[i + 1].clone();
                self.cursor = self.buffer.len();
            }
        }
    }

    /// Set the buffer content directly
    pub fn set_buffer(&mut self, text: String) {
        self.buffer = text;
        self.cursor = self.buffer.len();
        self.history_index = None;
    }

    /// Get the number of lines in the input
    pub fn line_count(&self) -> usize {
        self.buffer.lines().count().max(1)
    }

    /// Get the current line number (0-indexed)
    pub fn current_line(&self) -> usize {
        self.buffer[..self.cursor].matches('\n').count()
    }

    /// Get cursor position within current line
    pub fn cursor_in_line(&self) -> usize {
        let before_cursor = &self.buffer[..self.cursor];
        if let Some(pos) = before_cursor.rfind('\n') {
            self.cursor - pos - 1
        } else {
            self.cursor
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_basic() {
        let mut input = InputState::new();
        assert!(input.is_empty());

        input.insert_char('H');
        input.insert_char('i');
        assert_eq!(input.text(), "Hi");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_input_cursor_movement() {
        let mut input = InputState::new();
        input.set_buffer("Hello".to_string());

        input.move_left();
        assert_eq!(input.cursor, 4);

        input.move_home();
        assert_eq!(input.cursor, 0);

        input.move_end();
        assert_eq!(input.cursor, 5);
    }

    #[test]
    fn test_input_backspace() {
        let mut input = InputState::new();
        input.set_buffer("Hello".to_string());

        input.backspace();
        assert_eq!(input.text(), "Hell");
    }

    #[test]
    fn test_input_history() {
        let mut input = InputState::new();

        // Submit some entries
        input.set_buffer("first".to_string());
        input.submit();
        input.set_buffer("second".to_string());
        input.submit();
        input.set_buffer("third".to_string());
        input.submit();

        assert_eq!(input.history.len(), 3);

        // Navigate back
        input.set_buffer("current".to_string());
        input.history_prev();
        assert_eq!(input.text(), "third");

        input.history_prev();
        assert_eq!(input.text(), "second");

        // Navigate forward
        input.history_next();
        assert_eq!(input.text(), "third");

        // Back to current
        input.history_next();
        assert_eq!(input.text(), "current");
    }

    #[test]
    fn test_input_delete_word() {
        let mut input = InputState::new();
        input.set_buffer("hello world test".to_string());

        input.delete_word();
        assert_eq!(input.text(), "hello world ");

        input.delete_word();
        assert_eq!(input.text(), "hello ");
    }

    #[test]
    fn test_submit_deduplicates_history() {
        let mut input = InputState::new();

        input.set_buffer("same".to_string());
        input.submit();
        input.set_buffer("same".to_string());
        input.submit();
        input.set_buffer("same".to_string());
        input.submit();

        // Should only have one entry
        assert_eq!(input.history.len(), 1);
    }

    #[test]
    fn test_default_implementation() {
        let input = InputState::default();
        assert!(input.is_empty());
        assert_eq!(input.cursor, 0);
        assert!(input.history.is_empty());
        assert!(input.history_index.is_none());
        assert!(!input.multiline);
    }

    #[test]
    fn test_insert_str() {
        let mut input = InputState::new();
        input.insert_str("Hello");
        assert_eq!(input.text(), "Hello");
        assert_eq!(input.cursor, 5);

        // Insert in the middle
        input.cursor = 2;
        input.insert_str("XX");
        assert_eq!(input.text(), "HeXXllo");
        assert_eq!(input.cursor, 4);
    }

    #[test]
    fn test_delete() {
        let mut input = InputState::new();
        input.set_buffer("Hello".to_string());
        input.cursor = 2;

        input.delete();
        assert_eq!(input.text(), "Helo");
        assert_eq!(input.cursor, 2);

        // Delete at end (should do nothing)
        input.cursor = 4;
        input.delete();
        assert_eq!(input.text(), "Helo");
    }

    #[test]
    fn test_move_right() {
        let mut input = InputState::new();
        input.set_buffer("Hello".to_string());
        input.cursor = 0;

        input.move_right();
        assert_eq!(input.cursor, 1);

        input.move_right();
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_move_right_at_end() {
        let mut input = InputState::new();
        input.set_buffer("Hi".to_string());
        // cursor is at end after set_buffer

        input.move_right();
        assert_eq!(input.cursor, 2); // Should stay at end
    }

    #[test]
    fn test_move_left_at_start() {
        let mut input = InputState::new();
        input.set_buffer("Hello".to_string());
        input.cursor = 0;

        input.move_left();
        assert_eq!(input.cursor, 0); // Should stay at start
    }

    #[test]
    fn test_backspace_at_start() {
        let mut input = InputState::new();
        input.set_buffer("Hello".to_string());
        input.cursor = 0;

        input.backspace();
        assert_eq!(input.text(), "Hello"); // No change
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_multiline_move_home() {
        let mut input = InputState::new();
        input.multiline = true;
        input.set_buffer("first line\nsecond line".to_string());
        // cursor is at the end

        input.move_home();
        // Should move to start of "second line" (position 11)
        assert_eq!(input.cursor, 11);

        // Move home again should stay at start of current line
        input.move_home();
        assert_eq!(input.cursor, 11);
    }

    #[test]
    fn test_multiline_move_home_first_line() {
        let mut input = InputState::new();
        input.multiline = true;
        input.set_buffer("first line\nsecond line".to_string());
        input.cursor = 5; // middle of first line

        input.move_home();
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_multiline_move_end() {
        let mut input = InputState::new();
        input.multiline = true;
        input.set_buffer("first line\nsecond line".to_string());
        input.cursor = 0;

        input.move_end();
        // Should move to end of "first line" (position 10, before newline)
        assert_eq!(input.cursor, 10);
    }

    #[test]
    fn test_multiline_move_end_last_line() {
        let mut input = InputState::new();
        input.multiline = true;
        input.set_buffer("first line\nsecond line".to_string());
        input.cursor = 11; // start of second line

        input.move_end();
        // Should move to end of buffer
        assert_eq!(input.cursor, 22);
    }

    #[test]
    fn test_clear() {
        let mut input = InputState::new();
        input.set_buffer("Hello".to_string());
        input.history.push("old".to_string());
        input.history_index = Some(0);

        input.clear();
        assert!(input.is_empty());
        assert_eq!(input.cursor, 0);
        assert!(input.history_index.is_none());
        // History should still exist
        assert!(!input.history.is_empty());
    }

    #[test]
    fn test_line_count() {
        let mut input = InputState::new();
        assert_eq!(input.line_count(), 1); // Empty buffer = 1 line

        input.set_buffer("single line".to_string());
        assert_eq!(input.line_count(), 1);

        input.set_buffer("line1\nline2\nline3".to_string());
        assert_eq!(input.line_count(), 3);
    }

    #[test]
    fn test_current_line() {
        let mut input = InputState::new();
        input.set_buffer("line1\nline2\nline3".to_string());

        input.cursor = 0;
        assert_eq!(input.current_line(), 0);

        input.cursor = 5; // End of first line
        assert_eq!(input.current_line(), 0);

        input.cursor = 6; // Start of second line (after \n)
        assert_eq!(input.current_line(), 1);

        input.cursor = 12; // In third line
        assert_eq!(input.current_line(), 2);
    }

    #[test]
    fn test_cursor_in_line() {
        let mut input = InputState::new();
        input.set_buffer("line1\nline2\nline3".to_string());

        input.cursor = 0;
        assert_eq!(input.cursor_in_line(), 0);

        input.cursor = 3;
        assert_eq!(input.cursor_in_line(), 3);

        input.cursor = 6; // Start of "line2"
        assert_eq!(input.cursor_in_line(), 0);

        input.cursor = 8; // "ne" in "line2"
        assert_eq!(input.cursor_in_line(), 2);
    }

    #[test]
    fn test_history_next_not_in_history() {
        let mut input = InputState::new();
        input.set_buffer("current".to_string());
        input.history.push("old".to_string());

        // Not in history mode
        input.history_next();
        assert_eq!(input.text(), "current"); // Should stay as current
    }

    #[test]
    fn test_history_prev_empty_history() {
        let mut input = InputState::new();
        input.set_buffer("current".to_string());

        input.history_prev();
        assert_eq!(input.text(), "current"); // Should stay as current
    }

    #[test]
    fn test_history_prev_at_oldest() {
        let mut input = InputState::new();
        input.history.push("first".to_string());
        input.history.push("second".to_string());
        input.set_buffer("current".to_string());

        // Go to oldest
        input.history_prev();
        input.history_prev();
        assert_eq!(input.text(), "first");

        // Try to go further back
        input.history_prev();
        assert_eq!(input.text(), "first"); // Should stay at oldest
    }

    #[test]
    fn test_submit_empty_not_in_history() {
        let mut input = InputState::new();
        input.set_buffer("".to_string());
        input.submit();

        assert!(input.history.is_empty());
    }

    #[test]
    fn test_submit_whitespace_not_in_history() {
        let mut input = InputState::new();
        input.set_buffer("   ".to_string());
        input.submit();

        assert!(input.history.is_empty());
    }

    #[test]
    fn test_delete_word_no_trailing_whitespace() {
        let mut input = InputState::new();
        input.set_buffer("hello world".to_string());

        input.delete_word();
        assert_eq!(input.text(), "hello ");
    }

    #[test]
    fn test_delete_word_at_start() {
        let mut input = InputState::new();
        input.set_buffer("hello".to_string());
        input.cursor = 0;

        input.delete_word();
        assert_eq!(input.text(), "hello"); // No change
    }

    #[test]
    fn test_insert_char_in_middle() {
        let mut input = InputState::new();
        input.set_buffer("Hllo".to_string());
        input.cursor = 1;

        input.insert_char('e');
        assert_eq!(input.text(), "Hello");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_text_getter() {
        let mut input = InputState::new();
        input.set_buffer("test".to_string());
        assert_eq!(input.text(), "test");
    }

    #[test]
    fn test_debug_and_clone() {
        let mut input = InputState::new();
        input.set_buffer("test".to_string());

        let debug_str = format!("{:?}", input);
        assert!(debug_str.contains("test"));

        let cloned = input.clone();
        assert_eq!(cloned.text(), "test");
        assert_eq!(cloned.cursor, input.cursor);
    }
}
