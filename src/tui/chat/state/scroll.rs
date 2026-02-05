// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Scroll state management for the chat TUI

use super::messages::DisplayMessage;

/// Represents the scroll state and viewport for the message area
#[derive(Debug, Clone)]
pub struct ScrollState {
    /// Current scroll position in lines from the top
    pub scroll_offset: usize,
    /// Height of the viewport in lines
    pub viewport_height: u16,
    /// Whether auto-scroll is enabled (follows new messages)
    pub auto_scroll_enabled: bool,
    /// Cached total content height in lines
    cached_total_height: Option<usize>,
    /// Width used for last height calculation (for cache invalidation)
    cached_width: Option<u16>,
}

impl ScrollState {
    /// Create a new scroll state with auto-scroll enabled
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            viewport_height: 20,
            auto_scroll_enabled: true,
            cached_total_height: None,
            cached_width: None,
        }
    }

    /// Update the viewport height (called when terminal is resized)
    pub fn update_viewport_height(&mut self, height: u16) {
        let old_height = self.viewport_height;
        self.viewport_height = height;

        // Maintain relative scroll position during resize
        if old_height > 0 && height != old_height {
            self.maintain_scroll_position_on_resize(old_height, height);
        }
    }

    /// Maintain scroll position when viewport height changes
    fn maintain_scroll_position_on_resize(&mut self, old_height: u16, new_height: u16) {
        if let Some(total_height) = self.cached_total_height {
            let old_max_offset = total_height.saturating_sub(old_height as usize);
            let new_max_offset = total_height.saturating_sub(new_height as usize);

            if old_max_offset > 0 {
                // Calculate relative position (0.0 = top, 1.0 = bottom)
                let relative_pos = self.scroll_offset as f64 / old_max_offset as f64;
                self.scroll_offset = (relative_pos * new_max_offset as f64) as usize;
            }
        }
    }

    /// Calculate the total height of all messages with proper text wrapping
    pub fn calculate_total_height(&mut self, messages: &[DisplayMessage], width: u16) -> usize {
        // Use cache if width hasn't changed
        if let (Some(cached_height), Some(cached_width)) =
            (self.cached_total_height, self.cached_width)
        {
            if cached_width == width {
                return cached_height;
            }
        }

        let total_height = messages
            .iter()
            .map(|message| self.calculate_message_height(message, width))
            .sum();

        // Update cache
        self.cached_total_height = Some(total_height);
        self.cached_width = Some(width);

        total_height
    }

    /// Calculate the height of a single message with proper text wrapping
    fn calculate_message_height(&self, message: &DisplayMessage, width: u16) -> usize {
        // Account for message widget indentation and borders
        let content_width = width.saturating_sub(4); // 2 chars indent + 2 chars margin

        // Calculate wrapped content height
        let content_height = if message.content.is_empty() {
            1 // At least one line for empty content
        } else {
            message
                .content
                .lines()
                .map(|line| {
                    if line.is_empty() {
                        1
                    } else {
                        // Calculate how many lines this will wrap to
                        let chars = line.chars().count();
                        if chars == 0 {
                            1
                        } else {
                            ((chars - 1) / content_width as usize) + 1
                        }
                    }
                })
                .sum::<usize>()
                .max(1)
        };

        // Calculate tool call heights
        let tool_call_height: usize = message
            .tool_calls
            .iter()
            .map(|tc| if tc.expanded { 5 } else { 2 })
            .sum();

        // Header (1) + content + tool calls + spacing (1)
        1 + content_height + tool_call_height + 1
    }

    /// Scroll up by the specified number of lines
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);

        // Disable auto-scroll when user manually scrolls up
        if lines > 0 {
            self.auto_scroll_enabled = false;
        }
    }

    /// Scroll down by the specified number of lines
    pub fn scroll_down(&mut self, lines: usize, total_height: usize) {
        let max_offset = total_height.saturating_sub(self.viewport_height as usize);
        let old_offset = self.scroll_offset;
        self.scroll_offset = (self.scroll_offset + lines).min(max_offset);

        // Re-enable auto-scroll if user scrolled to the bottom
        if self.scroll_offset >= max_offset && old_offset < max_offset {
            self.auto_scroll_enabled = true;
        }
    }

    /// Scroll to the top
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
        self.auto_scroll_enabled = false;
    }

    /// Scroll to the bottom and enable auto-scroll
    pub fn scroll_to_bottom(&mut self, total_height: usize) {
        let max_offset = total_height.saturating_sub(self.viewport_height as usize);
        self.scroll_offset = max_offset;
        self.auto_scroll_enabled = true;
    }

    /// Auto-scroll to bottom if auto-scroll is enabled
    pub fn maybe_auto_scroll(&mut self, total_height: usize) {
        if self.auto_scroll_enabled {
            let max_offset = total_height.saturating_sub(self.viewport_height as usize);
            self.scroll_offset = max_offset;
        }
    }

    /// Check if we're at the bottom of the content
    pub fn is_at_bottom(&self, total_height: usize) -> bool {
        let max_offset = total_height.saturating_sub(self.viewport_height as usize);
        self.scroll_offset >= max_offset
    }

    /// Check if we're at the top of the content
    pub fn is_at_top(&self) -> bool {
        self.scroll_offset == 0
    }

    /// Get the current scroll position as a percentage (0.0 = top, 1.0 = bottom)
    pub fn scroll_percentage(&self, total_height: usize) -> f64 {
        if total_height <= self.viewport_height as usize {
            1.0 // All content fits in viewport
        } else {
            let max_offset = total_height.saturating_sub(self.viewport_height as usize);
            if max_offset == 0 {
                1.0
            } else {
                self.scroll_offset as f64 / max_offset as f64
            }
        }
    }

    /// Invalidate height cache (call when messages change)
    pub fn invalidate_cache(&mut self) {
        self.cached_total_height = None;
        self.cached_width = None;
    }

    /// Handle page up/down scrolling
    pub fn page_up(&mut self) {
        let page_size = (self.viewport_height / 2).max(1) as usize;
        self.scroll_up(page_size);
    }

    /// Handle page up/down scrolling
    pub fn page_down(&mut self, total_height: usize) {
        let page_size = (self.viewport_height / 2).max(1) as usize;
        self.scroll_down(page_size, total_height);
    }

    /// Get scroll indicator info for UI display
    pub fn scroll_indicator(&self, total_height: usize) -> Option<(usize, usize, usize)> {
        if total_height <= self.viewport_height as usize {
            None // All content fits, no indicator needed
        } else {
            let current_line = self.scroll_offset + 1;
            let total_lines = total_height;
            let viewport_end =
                (self.scroll_offset + self.viewport_height as usize).min(total_height);
            Some((current_line, viewport_end, total_lines))
        }
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::chat::state::DisplayMessage;

    #[test]
    fn test_scroll_state_new() {
        let state = ScrollState::new();
        assert_eq!(state.scroll_offset, 0);
        assert_eq!(state.viewport_height, 20);
        assert!(state.auto_scroll_enabled);
    }

    #[test]
    fn test_scroll_up() {
        let mut state = ScrollState::new();
        state.scroll_offset = 10;

        state.scroll_up(3);
        assert_eq!(state.scroll_offset, 7);
        assert!(!state.auto_scroll_enabled); // Should disable auto-scroll

        state.scroll_up(10);
        assert_eq!(state.scroll_offset, 0); // Can't go below 0
    }

    #[test]
    fn test_scroll_down() {
        let mut state = ScrollState::new();
        let total_height = 50;

        state.scroll_down(5, total_height);
        assert_eq!(state.scroll_offset, 5);

        // Scroll to bottom should enable auto-scroll
        state.scroll_down(100, total_height);
        let max_offset = total_height - state.viewport_height as usize;
        assert_eq!(state.scroll_offset, max_offset);
        assert!(state.auto_scroll_enabled);
    }

    #[test]
    fn test_scroll_to_top() {
        let mut state = ScrollState::new();
        state.scroll_offset = 10;

        state.scroll_to_top();
        assert_eq!(state.scroll_offset, 0);
        assert!(!state.auto_scroll_enabled);
    }

    #[test]
    fn test_scroll_to_bottom() {
        let mut state = ScrollState::new();
        let total_height = 50;

        state.scroll_to_bottom(total_height);
        let max_offset = total_height - state.viewport_height as usize;
        assert_eq!(state.scroll_offset, max_offset);
        assert!(state.auto_scroll_enabled);
    }

    #[test]
    fn test_auto_scroll() {
        let mut state = ScrollState::new();
        let total_height = 50;

        // Auto-scroll should work when enabled
        state.maybe_auto_scroll(total_height);
        let max_offset = total_height - state.viewport_height as usize;
        assert_eq!(state.scroll_offset, max_offset);

        // Disable auto-scroll and verify it doesn't change position
        state.auto_scroll_enabled = false;
        state.scroll_offset = 5;
        state.maybe_auto_scroll(total_height);
        assert_eq!(state.scroll_offset, 5);
    }

    #[test]
    fn test_is_at_bottom() {
        let mut state = ScrollState::new();
        let total_height = 50;

        state.scroll_to_bottom(total_height);
        assert!(state.is_at_bottom(total_height));

        state.scroll_up(1);
        assert!(!state.is_at_bottom(total_height));
    }

    #[test]
    fn test_is_at_top() {
        let mut state = ScrollState::new();

        assert!(state.is_at_top());

        state.scroll_offset = 5;
        assert!(!state.is_at_top());

        state.scroll_to_top();
        assert!(state.is_at_top());
    }

    #[test]
    fn test_scroll_percentage() {
        let mut state = ScrollState::new();
        state.viewport_height = 10;
        let total_height = 30;

        // At top
        state.scroll_offset = 0;
        assert_eq!(state.scroll_percentage(total_height), 0.0);

        // At middle
        state.scroll_offset = 10; // max_offset = 30 - 10 = 20, so 10/20 = 0.5
        assert_eq!(state.scroll_percentage(total_height), 0.5);

        // At bottom
        state.scroll_offset = 20;
        assert_eq!(state.scroll_percentage(total_height), 1.0);
    }

    #[test]
    fn test_viewport_resize() {
        let mut state = ScrollState::new();
        state.viewport_height = 20;
        state.scroll_offset = 10;
        state.cached_total_height = Some(50);

        // Resize viewport
        state.update_viewport_height(10);
        assert_eq!(state.viewport_height, 10);
        // Scroll position should be adjusted proportionally
    }

    #[test]
    fn test_page_navigation() {
        let mut state = ScrollState::new();
        state.viewport_height = 20;
        let total_height = 100;

        state.page_down(total_height);
        assert_eq!(state.scroll_offset, 10); // viewport_height / 2

        state.page_up();
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn test_scroll_indicator() {
        let mut state = ScrollState::new();
        state.viewport_height = 10;
        let total_height = 30;

        state.scroll_offset = 5;
        let indicator = state.scroll_indicator(total_height).unwrap();
        assert_eq!(indicator, (6, 15, 30)); // current_line, viewport_end, total_lines

        // No indicator when all content fits
        let small_height = 5;
        assert!(state.scroll_indicator(small_height).is_none());
    }

    #[test]
    fn test_calculate_message_height() {
        let state = ScrollState::new();
        let message = DisplayMessage::user("Hello\nWorld".to_string());

        let height = state.calculate_message_height(&message, 80);
        assert!(height >= 4); // Header + 2 content lines + spacing
    }

    #[test]
    fn test_cache_invalidation() {
        let mut state = ScrollState::new();
        state.cached_total_height = Some(100);
        state.cached_width = Some(80);

        state.invalidate_cache();
        assert!(state.cached_total_height.is_none());
        assert!(state.cached_width.is_none());
    }
}
