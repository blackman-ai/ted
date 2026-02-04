// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Input area widget for the chat TUI

use ratatui::{
    prelude::*,
    widgets::{Block, Borders},
};

use crate::tui::chat::state::InputState;

/// Widget for rendering the input area
pub struct InputArea<'a> {
    input: &'a InputState,
    focused: bool,
    placeholder: Option<&'a str>,
    processing: bool,
    processing_title: Option<String>,
}

impl<'a> InputArea<'a> {
    pub fn new(input: &'a InputState) -> Self {
        Self {
            input,
            focused: true,
            placeholder: None,
            processing: false,
            processing_title: None,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn placeholder(mut self, text: &'a str) -> Self {
        self.placeholder = Some(text);
        self
    }

    /// Set processing mode with a title suffix (e.g., " Processing (2 queued) ")
    pub fn processing(mut self, is_processing: bool, title: &str) -> Self {
        self.processing = is_processing;
        if is_processing {
            self.processing_title = Some(title.to_string());
        }
        self
    }

    /// Calculate cursor position in screen coordinates
    pub fn cursor_position(&self, area: Rect) -> (u16, u16) {
        // Account for border (1) and prompt "> " (2)
        let x = area.x + 1 + 2 + self.input.cursor_in_line() as u16;
        let y = area.y + 1 + self.input.current_line() as u16;
        (
            x.min(area.x + area.width - 1),
            y.min(area.y + area.height - 1),
        )
    }
}

impl<'a> Widget for InputArea<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (border_style, title_style) = if self.processing {
            // Yellow border when processing to indicate queued mode
            (
                Style::default().fg(Color::Yellow),
                Style::default().fg(Color::Yellow).bold(),
            )
        } else if self.focused {
            (
                Style::default().fg(Color::Cyan),
                Style::default().fg(Color::Cyan),
            )
        } else {
            (
                Style::default().fg(Color::DarkGray),
                Style::default().fg(Color::DarkGray),
            )
        };

        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style);

        // Add processing title if in processing mode
        if let Some(ref title) = self.processing_title {
            block = block.title(title.as_str()).title_style(title_style);
        }

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width < 4 {
            return;
        }

        // Render prompt
        buf.set_string(
            inner.x,
            inner.y,
            "> ",
            Style::default().fg(Color::Cyan).bold(),
        );

        // Render input text or placeholder
        let text_x = inner.x + 2;
        let text_width = inner.width.saturating_sub(2);

        if self.input.is_empty() {
            if let Some(placeholder) = self.placeholder {
                buf.set_string(
                    text_x,
                    inner.y,
                    placeholder,
                    Style::default().fg(Color::DarkGray).italic(),
                );
            }
        } else {
            // Render the input text
            let text = self.input.text();

            // Handle multiline input
            for (i, line) in text.lines().enumerate() {
                if i as u16 >= inner.height {
                    break;
                }

                let display_line = if line.len() > text_width as usize {
                    &line[..text_width as usize]
                } else {
                    line
                };

                buf.set_string(
                    text_x,
                    inner.y + i as u16,
                    display_line,
                    Style::default().fg(Color::White),
                );
            }
        }

        // Render cursor if focused
        if self.focused {
            let (cursor_x, cursor_y) = self.cursor_position(area);
            if cursor_x < area.x + area.width && cursor_y < area.y + area.height {
                // Highlight cursor position
                if let Some(cell) = buf.cell_mut(Position::new(cursor_x, cursor_y)) {
                    cell.set_style(Style::default().bg(Color::White).fg(Color::Black));
                }
            }
        }
    }
}

/// Render input area with a help hint line below
pub fn render_input_with_hints(
    input: &InputState,
    area: Rect,
    buf: &mut Buffer,
    focused: bool,
    hints: &[(&str, &str)], // (key, description) pairs
) {
    if area.height < 2 {
        return;
    }

    // Split area: input area (main) + hints line (1 line)
    let input_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(1),
    };

    let hints_area = Rect {
        x: area.x,
        y: area.y + area.height - 1,
        width: area.width,
        height: 1,
    };

    // Render input
    InputArea::new(input)
        .focused(focused)
        .placeholder("Type a message or /help for commands...")
        .render(input_area, buf);

    // Render hints
    let mut x = hints_area.x + 1;
    for (key, desc) in hints {
        if x + (key.len() + desc.len() + 4) as u16 > hints_area.x + hints_area.width {
            break;
        }

        buf.set_string(x, hints_area.y, key, Style::default().fg(Color::Yellow));
        x += key.len() as u16;
        buf.set_string(x, hints_area.y, " ", Style::default());
        x += 1;
        buf.set_string(x, hints_area.y, desc, Style::default().fg(Color::DarkGray));
        x += desc.len() as u16;
        buf.set_string(x, hints_area.y, " │ ", Style::default().fg(Color::DarkGray));
        x += 3;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_input_area_empty() {
        let input = InputState::new();
        let widget = InputArea::new(&input);
        assert!(widget.focused);
    }

    #[test]
    fn test_input_area_cursor_position() {
        let mut input = InputState::new();
        input.set_buffer("Hello".to_string());

        let area = Rect::new(0, 0, 80, 3);
        let widget = InputArea::new(&input);

        let (x, _y) = widget.cursor_position(area);
        // x = border(1) + prompt(2) + cursor(5) = 8
        assert_eq!(x, 8);
    }

    #[test]
    fn test_input_area_focused_builder() {
        let input = InputState::new();
        let widget = InputArea::new(&input).focused(false);
        assert!(!widget.focused);
    }

    #[test]
    fn test_input_area_placeholder_builder() {
        let input = InputState::new();
        let widget = InputArea::new(&input).placeholder("Type here...");
        assert_eq!(widget.placeholder, Some("Type here..."));
    }

    #[test]
    fn test_input_area_processing_builder() {
        let input = InputState::new();
        let widget = InputArea::new(&input).processing(true, " Processing (2 queued) ");
        assert!(widget.processing);
        assert_eq!(
            widget.processing_title,
            Some(" Processing (2 queued) ".to_string())
        );
    }

    #[test]
    fn test_input_area_processing_false() {
        let input = InputState::new();
        let widget = InputArea::new(&input).processing(false, "Ignored");
        assert!(!widget.processing);
        assert!(widget.processing_title.is_none());
    }

    #[test]
    fn test_input_area_cursor_at_start() {
        let mut input = InputState::new();
        input.set_buffer("Hello".to_string());
        input.cursor = 0;

        let area = Rect::new(0, 0, 80, 3);
        let widget = InputArea::new(&input);

        let (x, _y) = widget.cursor_position(area);
        // x = border(1) + prompt(2) + cursor(0) = 3
        assert_eq!(x, 3);
    }

    #[test]
    fn test_input_area_cursor_clamped() {
        let mut input = InputState::new();
        input.set_buffer("Hello".to_string());

        // Very small area
        let area = Rect::new(0, 0, 4, 2);
        let widget = InputArea::new(&input);

        let (x, y) = widget.cursor_position(area);
        // Should be clamped to area bounds
        assert!(x < area.x + area.width);
        assert!(y < area.y + area.height);
    }

    #[test]
    fn test_input_area_render_empty() {
        let backend = TestBackend::new(80, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = InputState::new();

        terminal
            .draw(|f| {
                let widget = InputArea::new(&input).placeholder("Type a message...");
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_input_area_render_with_text() {
        let backend = TestBackend::new(80, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut input = InputState::new();
        input.set_buffer("Hello world".to_string());

        terminal
            .draw(|f| {
                let widget = InputArea::new(&input);
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_input_area_render_multiline() {
        let backend = TestBackend::new(80, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut input = InputState::new();
        input.multiline = true;
        input.set_buffer("Line 1\nLine 2".to_string());

        terminal
            .draw(|f| {
                let widget = InputArea::new(&input);
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_input_area_render_processing() {
        let backend = TestBackend::new(80, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = InputState::new();

        terminal
            .draw(|f| {
                let widget = InputArea::new(&input).processing(true, " Processing ");
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_input_area_render_tiny_area() {
        let backend = TestBackend::new(5, 2);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = InputState::new();

        terminal
            .draw(|f| {
                let widget = InputArea::new(&input);
                f.render_widget(widget, f.area());
            })
            .unwrap();
        // Should not panic
    }

    #[test]
    fn test_input_area_render_unfocused() {
        let backend = TestBackend::new(80, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = InputState::new();

        terminal
            .draw(|f| {
                let widget = InputArea::new(&input).focused(false);
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_render_input_with_hints() {
        let backend = TestBackend::new(80, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = InputState::new();

        terminal
            .draw(|f| {
                let hints = &[("Enter", "Send"), ("Shift+Enter", "Newline")];
                render_input_with_hints(&input, f.area(), f.buffer_mut(), true, hints);
            })
            .unwrap();
    }

    #[test]
    fn test_render_input_with_hints_too_small() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let input = InputState::new();

        terminal
            .draw(|f| {
                let hints = &[("Enter", "Send")];
                render_input_with_hints(&input, f.area(), f.buffer_mut(), true, hints);
            })
            .unwrap();
        // Should not panic
    }

    #[test]
    fn test_input_area_multiline_cursor_position() {
        let mut input = InputState::new();
        input.multiline = true;
        input.set_buffer("Line1\nLine2".to_string());
        input.cursor = 8; // In the middle of Line2

        let area = Rect::new(0, 0, 80, 5);
        let widget = InputArea::new(&input);

        let (_x, y) = widget.cursor_position(area);
        // Should be on second line
        assert_eq!(y, area.y + 2); // border + line 2
    }
}
