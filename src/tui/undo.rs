// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Undo/redo history for the editor
//!
//! Provides a simple undo/redo stack for text editing operations.

/// Maximum number of undo states to keep
const MAX_UNDO_HISTORY: usize = 100;

/// A snapshot of editor state
#[derive(Debug, Clone)]
pub struct EditorSnapshot {
    /// The text content
    pub content: String,
    /// Cursor position (line, column)
    pub cursor: (usize, usize),
}

/// Undo/redo history manager
#[derive(Debug)]
pub struct UndoHistory {
    /// Past states (for undo)
    undo_stack: Vec<EditorSnapshot>,
    /// Future states (for redo)
    redo_stack: Vec<EditorSnapshot>,
}

impl UndoHistory {
    /// Create a new empty history
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Push a new state onto the undo stack
    /// This clears the redo stack (new changes invalidate redo history)
    pub fn push(&mut self, snapshot: EditorSnapshot) {
        // Limit history size
        if self.undo_stack.len() >= MAX_UNDO_HISTORY {
            self.undo_stack.remove(0);
        }

        self.undo_stack.push(snapshot);
        self.redo_stack.clear();
    }

    /// Undo: pop from undo stack, push current state to redo stack
    /// Returns the state to restore, or None if no undo history
    pub fn undo(&mut self, current: EditorSnapshot) -> Option<EditorSnapshot> {
        if let Some(snapshot) = self.undo_stack.pop() {
            self.redo_stack.push(current);
            Some(snapshot)
        } else {
            None
        }
    }

    /// Redo: pop from redo stack, push current state to undo stack
    /// Returns the state to restore, or None if no redo history
    pub fn redo(&mut self, current: EditorSnapshot) -> Option<EditorSnapshot> {
        if let Some(snapshot) = self.redo_stack.pop() {
            self.undo_stack.push(current);
            Some(snapshot)
        } else {
            None
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Get the number of undo states available
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Get the number of redo states available
    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot(content: &str, cursor: (usize, usize)) -> EditorSnapshot {
        EditorSnapshot {
            content: content.to_string(),
            cursor,
        }
    }

    #[test]
    fn test_new_history_is_empty() {
        let history = UndoHistory::new();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_count(), 0);
        assert_eq!(history.redo_count(), 0);
    }

    #[test]
    fn test_push_enables_undo() {
        let mut history = UndoHistory::new();
        history.push(make_snapshot("hello", (0, 0)));

        assert!(history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_count(), 1);
    }

    #[test]
    fn test_undo_restores_state() {
        let mut history = UndoHistory::new();
        let original = make_snapshot("hello", (0, 0));
        history.push(original.clone());

        let current = make_snapshot("hello world", (0, 6));
        let restored = history.undo(current).unwrap();

        assert_eq!(restored.content, "hello");
        assert_eq!(restored.cursor, (0, 0));
    }

    #[test]
    fn test_undo_enables_redo() {
        let mut history = UndoHistory::new();
        history.push(make_snapshot("hello", (0, 0)));

        let current = make_snapshot("hello world", (0, 6));
        history.undo(current);

        assert!(history.can_redo());
        assert_eq!(history.redo_count(), 1);
    }

    #[test]
    fn test_redo_restores_state() {
        let mut history = UndoHistory::new();
        history.push(make_snapshot("hello", (0, 0)));

        let modified = make_snapshot("hello world", (0, 6));
        let after_undo = history.undo(modified.clone()).unwrap();

        let restored = history.redo(after_undo).unwrap();
        assert_eq!(restored.content, "hello world");
        assert_eq!(restored.cursor, (0, 6));
    }

    #[test]
    fn test_push_clears_redo() {
        let mut history = UndoHistory::new();
        history.push(make_snapshot("hello", (0, 0)));

        let current = make_snapshot("hello world", (0, 6));
        history.undo(current);
        assert!(history.can_redo());

        // New change should clear redo
        history.push(make_snapshot("hello there", (0, 6)));
        assert!(!history.can_redo());
    }

    #[test]
    fn test_multiple_undos() {
        let mut history = UndoHistory::new();
        history.push(make_snapshot("a", (0, 0)));
        history.push(make_snapshot("ab", (0, 1)));
        history.push(make_snapshot("abc", (0, 2)));

        let current = make_snapshot("abcd", (0, 3));

        let restored = history.undo(current).unwrap();
        assert_eq!(restored.content, "abc");

        let restored = history.undo(restored).unwrap();
        assert_eq!(restored.content, "ab");

        let restored = history.undo(restored).unwrap();
        assert_eq!(restored.content, "a");

        assert!(!history.can_undo());
    }

    #[test]
    fn test_undo_redo_cycle() {
        let mut history = UndoHistory::new();
        history.push(make_snapshot("start", (0, 0)));

        let mut current = make_snapshot("end", (0, 3));

        // Undo
        current = history.undo(current).unwrap();
        assert_eq!(current.content, "start");

        // Redo
        current = history.redo(current).unwrap();
        assert_eq!(current.content, "end");

        // Undo again
        current = history.undo(current).unwrap();
        assert_eq!(current.content, "start");
    }

    #[test]
    fn test_clear_history() {
        let mut history = UndoHistory::new();
        history.push(make_snapshot("a", (0, 0)));
        history.push(make_snapshot("b", (0, 1)));

        history.clear();

        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_max_history_limit() {
        let mut history = UndoHistory::new();

        // Push more than MAX_UNDO_HISTORY items
        for i in 0..150 {
            history.push(make_snapshot(&format!("state{}", i), (0, i)));
        }

        // Should be limited to MAX_UNDO_HISTORY
        assert_eq!(history.undo_count(), MAX_UNDO_HISTORY);
    }

    #[test]
    fn test_undo_empty_history_returns_none() {
        let mut history = UndoHistory::new();
        let current = make_snapshot("hello", (0, 0));

        assert!(history.undo(current).is_none());
    }

    #[test]
    fn test_redo_empty_history_returns_none() {
        let mut history = UndoHistory::new();
        let current = make_snapshot("hello", (0, 0));

        assert!(history.redo(current).is_none());
    }

    #[test]
    fn test_default_impl() {
        let history = UndoHistory::default();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }
}
