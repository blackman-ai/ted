// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Message rendering widget

use ratatui::{
    prelude::*,
    widgets::{Paragraph, Wrap},
};

use crate::tui::chat::state::{
    truncate_string, DisplayMessage, DisplayToolCall, MessageRole, ToolCallStatus,
};

/// Widget for rendering a single message
pub struct MessageWidget<'a> {
    message: &'a DisplayMessage,
    #[allow(dead_code)]
    width: u16,
}

impl<'a> MessageWidget<'a> {
    pub fn new(message: &'a DisplayMessage, width: u16) -> Self {
        Self { message, width }
    }

    /// Calculate the height needed to render this message with proper text wrapping
    pub fn height(&self) -> u16 {
        calculate_message_height_with_wrapping(self.message, self.width)
    }

    /// Calculate height accounting for actual text wrapping
    pub fn height_with_wrapping(&self, width: u16) -> u16 {
        let content_width = width.saturating_sub(4); // Account for indentation and margins

        // Calculate wrapped content height
        let content_height = if self.message.content.is_empty() {
            1
        } else {
            self.message
                .content
                .lines()
                .map(|line| {
                    if line.is_empty() {
                        1
                    } else {
                        let chars = line.chars().count();
                        if chars == 0 {
                            1
                        } else {
                            // Calculate wrapped lines for this line
                            ((chars - 1) / content_width as usize) + 1
                        }
                    }
                })
                .sum::<usize>()
                .max(1)
        };

        // Calculate tool call heights
        let tool_call_height: usize = self
            .message
            .tool_calls
            .iter()
            .map(|tc| if tc.expanded { 5 } else { 2 })
            .sum();

        // Header (1) + content + tool calls + spacing (1)
        (1 + content_height + tool_call_height + 1) as u16
    }
}

impl<'a> Widget for MessageWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 {
            return;
        }

        let (role_style, role_label) = match self.message.role {
            MessageRole::User => (Style::default().fg(Color::Cyan).bold(), "you"),
            MessageRole::Assistant => (Style::default().fg(Color::White).bold(), "ted"),
            MessageRole::System => (Style::default().fg(Color::Yellow).bold(), "system"),
        };

        // Render role label with caps badges for assistant
        let mut header_line = vec![Span::styled(format!("  {}", role_label), role_style)];

        if self.message.role == MessageRole::Assistant && !self.message.active_caps.is_empty() {
            for cap in &self.message.active_caps {
                // Skip "base" cap - it's always applied silently and shouldn't be shown
                if cap == "base" {
                    continue;
                }
                header_line.push(Span::raw(" "));
                header_line.push(Span::styled(
                    format!(" {} ", cap),
                    Style::default().fg(Color::Black).bg(Color::Blue),
                ));
            }
        }

        if self.message.is_streaming {
            header_line.push(Span::styled(" ●", Style::default().fg(Color::Green)));
        }

        let header = Line::from(header_line);
        buf.set_line(area.x, area.y, &header, area.width);

        // Render content
        let content_area = Rect {
            x: area.x + 2,
            y: area.y + 1,
            width: area.width.saturating_sub(4),
            height: area.height.saturating_sub(2),
        };

        let content_style = match self.message.role {
            MessageRole::User => Style::default().fg(Color::Cyan),
            MessageRole::Assistant => Style::default().fg(Color::White),
            MessageRole::System => Style::default().fg(Color::Yellow),
        };

        let content = Paragraph::new(self.message.content.as_str())
            .style(content_style)
            .wrap(Wrap { trim: false });

        content.render(content_area, buf);

        // Render tool calls
        let content_lines = self.message.content.lines().count().max(1) as u16;
        let mut tool_y = area.y + 1 + content_lines + 1;

        for tool_call in &self.message.tool_calls {
            if tool_y >= area.y + area.height {
                break;
            }

            let remaining_height = (area.y + area.height).saturating_sub(tool_y);
            let tool_area = Rect {
                x: area.x + 4,
                y: tool_y,
                width: area.width.saturating_sub(6),
                height: remaining_height.min(if tool_call.expanded { 5 } else { 2 }),
            };

            render_tool_call(tool_call, tool_area, buf);
            tool_y += tool_area.height;
        }
    }
}

/// Render a single tool call
fn render_tool_call(tool_call: &DisplayToolCall, area: Rect, buf: &mut Buffer) {
    if area.height < 1 {
        return;
    }

    let status_char = tool_call.status.indicator();
    let status_style = match tool_call.status {
        ToolCallStatus::Running => Style::default().fg(Color::Yellow),
        ToolCallStatus::Success => Style::default().fg(Color::Green),
        ToolCallStatus::Failed => Style::default().fg(Color::Red),
        ToolCallStatus::Cancelled => Style::default().fg(Color::DarkGray),
    };

    // First line: ╭─ tool_name → summary
    let header = Line::from(vec![
        Span::styled("╭─ ", Style::default().fg(Color::DarkGray)),
        Span::styled(&tool_call.name, Style::default().fg(Color::Magenta)),
        Span::styled(" → ", Style::default().fg(Color::DarkGray)),
        Span::styled(&tool_call.input_summary, Style::default().fg(Color::Cyan)),
    ]);
    buf.set_line(area.x, area.y, &header, area.width);

    if area.height < 2 {
        return;
    }

    // Second line: ╰─ status result_preview
    let mut result_line = vec![
        Span::styled("╰─ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{} ", status_char), status_style),
    ];

    if let Some(preview) = &tool_call.result_preview {
        let max_len = area.width as usize - 10;
        let preview_text = truncate_string(preview, max_len);
        result_line.push(Span::styled(
            preview_text,
            Style::default().fg(Color::DarkGray),
        ));
    } else if tool_call.status == ToolCallStatus::Running {
        let elapsed = tool_call.elapsed_secs();
        result_line.push(Span::styled(
            format!("Running... ({:.1}s)", elapsed),
            Style::default().fg(Color::DarkGray),
        ));
    }

    let result = Line::from(result_line);
    buf.set_line(area.x, area.y + 1, &result, area.width);

    // If expanded, show more details
    if tool_call.expanded && area.height > 2 {
        if let Some(full) = &tool_call.result_full {
            let detail_area = Rect {
                x: area.x + 2,
                y: area.y + 2,
                width: area.width.saturating_sub(4),
                height: area.height.saturating_sub(2),
            };

            let detail = Paragraph::new(full.as_str())
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: false });

            detail.render(detail_area, buf);
        }
    }
}

/// Render a list of messages with improved scrolling support
pub fn render_messages(
    messages: &[DisplayMessage],
    area: Rect,
    buf: &mut Buffer,
    scroll_offset: usize,
) {
    render_messages_with_scroll_state(messages, area, buf, scroll_offset, None)
}

/// Render a list of messages with advanced scrolling and viewport tracking
pub fn render_messages_with_scroll_state(
    messages: &[DisplayMessage],
    area: Rect,
    buf: &mut Buffer,
    scroll_offset: usize,
    scroll_state: Option<&mut crate::tui::chat::state::ScrollState>,
) {
    if messages.is_empty() {
        return;
    }

    let mut current_y = area.y;
    let mut lines_skipped = 0;
    let viewport_end = area.y + area.height;

    for message in messages.iter() {
        if current_y >= viewport_end {
            break;
        }

        let widget = MessageWidget::new(message, area.width);
        let msg_height = calculate_message_height_with_wrapping(message, area.width);

        // Check if we need to skip this message entirely
        if lines_skipped + msg_height as usize <= scroll_offset {
            lines_skipped += msg_height as usize;
            continue;
        }

        // Calculate how many lines of this message to skip from the top
        let lines_to_skip_in_message = scroll_offset.saturating_sub(lines_skipped);
        lines_skipped += msg_height as usize;

        // Handle partial message rendering
        if lines_to_skip_in_message > 0 {
            render_partial_message(
                message,
                area.width,
                area,
                buf,
                &mut current_y,
                lines_to_skip_in_message,
                viewport_end,
            );
        } else {
            // Render full message
            let remaining_height = viewport_end.saturating_sub(current_y);
            let msg_area = Rect {
                x: area.x,
                y: current_y,
                width: area.width,
                height: msg_height.min(remaining_height),
            };

            widget.render(msg_area, buf);
            current_y += msg_height;
        }
    }

    // Update scroll state if provided
    if let Some(state) = scroll_state {
        state.update_viewport_height(area.height);
    }
}

/// Calculate message height with proper text wrapping consideration
fn calculate_message_height_with_wrapping(message: &DisplayMessage, width: u16) -> u16 {
    let content_width = width.saturating_sub(4); // Account for indentation and margins

    // Calculate wrapped content height
    let content_height = if message.content.is_empty() {
        1
    } else {
        message
            .content
            .lines()
            .map(|line| {
                if line.is_empty() {
                    1
                } else {
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
    (1 + content_height + tool_call_height + 1) as u16
}

/// Render a message that is partially clipped by the scroll offset
fn render_partial_message(
    message: &DisplayMessage,
    width: u16,
    area: Rect,
    buf: &mut Buffer,
    current_y: &mut u16,
    lines_to_skip: usize,
    viewport_end: u16,
) {
    // For now, implement simple partial rendering by adjusting the render area
    // This is a simplified approach - full implementation would need to track
    // which specific lines within the message to skip

    let widget = MessageWidget::new(message, width);
    let msg_height = calculate_message_height_with_wrapping(message, width);

    let visible_height = msg_height.saturating_sub(lines_to_skip as u16);
    let remaining_viewport = viewport_end.saturating_sub(*current_y);
    let render_height = visible_height.min(remaining_viewport);

    if render_height > 0 {
        // Create a clipped render area
        let msg_area = Rect {
            x: area.x,
            y: *current_y,
            width,
            height: render_height,
        };

        // Render the widget but with adjusted positioning to account for skipped lines
        // This is a simplified approach - ideally we'd modify the widget to handle
        // partial rendering internally
        widget.render(msg_area, buf);
        *current_y += render_height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_message_widget_height() {
        let msg = DisplayMessage::user("Hello world".to_string());
        let widget = MessageWidget::new(&msg, 80);
        assert!(widget.height() >= 2);
    }

    #[test]
    fn test_message_widget_multiline() {
        let msg = DisplayMessage::user("Line 1\nLine 2\nLine 3".to_string());
        let widget = MessageWidget::new(&msg, 80);
        assert!(widget.height() >= 4);
    }

    #[test]
    fn test_message_widget_assistant() {
        let msg = DisplayMessage::assistant(
            "I can help with that".to_string(),
            vec!["rust-expert".to_string()],
        );
        let widget = MessageWidget::new(&msg, 80);
        assert!(widget.height() >= 2);
    }

    #[test]
    fn test_message_widget_system() {
        let msg = DisplayMessage::system("Welcome to Ted".to_string());
        let widget = MessageWidget::new(&msg, 80);
        assert!(widget.height() >= 2);
    }

    #[test]
    fn test_message_widget_streaming() {
        let msg = DisplayMessage::assistant_streaming(vec!["base".to_string()]);
        let widget = MessageWidget::new(&msg, 80);
        assert!(widget.height() >= 2);
    }

    #[test]
    fn test_message_widget_with_tool_calls() {
        let mut msg = DisplayMessage::assistant_streaming(vec![]);
        let tc = DisplayToolCall::new(
            "tc1".to_string(),
            "file_read".to_string(),
            serde_json::json!({"path": "/src/main.rs"}),
        );
        msg.add_tool_call(tc);

        let widget = MessageWidget::new(&msg, 80);
        // Should have more height for tool call
        assert!(widget.height() >= 4);
    }

    #[test]
    fn test_message_widget_with_expanded_tool_call() {
        let mut msg = DisplayMessage::assistant_streaming(vec![]);
        let mut tc = DisplayToolCall::new(
            "tc1".to_string(),
            "file_read".to_string(),
            serde_json::json!({"path": "/src/main.rs"}),
        );
        tc.expanded = true;
        msg.add_tool_call(tc);

        let widget = MessageWidget::new(&msg, 80);
        // Expanded tool call uses 5 lines instead of 2
        assert!(widget.height() >= 7);
    }

    #[test]
    fn test_message_widget_render_user() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let msg = DisplayMessage::user("Hello world".to_string());

        terminal
            .draw(|f| {
                let widget = MessageWidget::new(&msg, f.area().width);
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_message_widget_render_assistant_with_caps() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let msg = DisplayMessage::assistant(
            "Sure, let me help!".to_string(),
            vec!["rust-expert".to_string(), "code-review".to_string()],
        );

        terminal
            .draw(|f| {
                let widget = MessageWidget::new(&msg, f.area().width);
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_message_widget_render_streaming() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let msg = DisplayMessage::assistant_streaming(vec!["base".to_string()]);

        terminal
            .draw(|f| {
                let widget = MessageWidget::new(&msg, f.area().width);
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_message_widget_render_system() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let msg = DisplayMessage::system("Connection established".to_string());

        terminal
            .draw(|f| {
                let widget = MessageWidget::new(&msg, f.area().width);
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_message_widget_render_with_tool_call() {
        let backend = TestBackend::new(80, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut msg = DisplayMessage::assistant_streaming(vec![]);
        msg.append_content("Let me read that file for you.");

        let tc = DisplayToolCall::new(
            "tc1".to_string(),
            "file_read".to_string(),
            serde_json::json!({"path": "/src/main.rs"}),
        );
        msg.add_tool_call(tc);

        terminal
            .draw(|f| {
                let widget = MessageWidget::new(&msg, f.area().width);
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_message_widget_render_tool_call_success() {
        let backend = TestBackend::new(80, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut msg = DisplayMessage::assistant_streaming(vec![]);

        let mut tc = DisplayToolCall::new(
            "tc1".to_string(),
            "shell".to_string(),
            serde_json::json!({"command": "cargo test"}),
        );
        tc.complete_success(Some("All tests passed".to_string()), None);
        msg.add_tool_call(tc);

        terminal
            .draw(|f| {
                let widget = MessageWidget::new(&msg, f.area().width);
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_message_widget_render_tool_call_failed() {
        let backend = TestBackend::new(80, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut msg = DisplayMessage::assistant_streaming(vec![]);

        let mut tc = DisplayToolCall::new(
            "tc1".to_string(),
            "shell".to_string(),
            serde_json::json!({"command": "cargo test"}),
        );
        tc.complete_failed("Command not found".to_string());
        msg.add_tool_call(tc);

        terminal
            .draw(|f| {
                let widget = MessageWidget::new(&msg, f.area().width);
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_message_widget_render_tiny_area() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let msg = DisplayMessage::user("Hello".to_string());

        terminal
            .draw(|f| {
                let widget = MessageWidget::new(&msg, f.area().width);
                f.render_widget(widget, f.area());
            })
            .unwrap();
        // Should not panic
    }

    #[test]
    fn test_render_messages() {
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let messages = vec![
            DisplayMessage::user("Hello".to_string()),
            DisplayMessage::assistant("Hi there!".to_string(), vec![]),
            DisplayMessage::user("How are you?".to_string()),
        ];

        terminal
            .draw(|f| {
                render_messages(&messages, f.area(), f.buffer_mut(), 0);
            })
            .unwrap();
    }

    #[test]
    fn test_render_messages_with_scroll() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let messages = vec![
            DisplayMessage::user("Message 1".to_string()),
            DisplayMessage::assistant("Reply 1".to_string(), vec![]),
            DisplayMessage::user("Message 2".to_string()),
            DisplayMessage::assistant("Reply 2".to_string(), vec![]),
        ];

        terminal
            .draw(|f| {
                render_messages(&messages, f.area(), f.buffer_mut(), 2);
            })
            .unwrap();
    }

    #[test]
    fn test_render_messages_empty() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let messages: Vec<DisplayMessage> = vec![];

        terminal
            .draw(|f| {
                render_messages(&messages, f.area(), f.buffer_mut(), 0);
            })
            .unwrap();
    }

    #[test]
    fn test_render_tool_call_cancelled() {
        let backend = TestBackend::new(80, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut msg = DisplayMessage::assistant_streaming(vec![]);

        let mut tc = DisplayToolCall::new(
            "tc1".to_string(),
            "shell".to_string(),
            serde_json::json!({"command": "cargo test"}),
        );
        tc.status = ToolCallStatus::Cancelled;
        tc.completed_at = Some(std::time::Instant::now());
        msg.add_tool_call(tc);

        terminal
            .draw(|f| {
                let widget = MessageWidget::new(&msg, f.area().width);
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_render_tool_call_expanded() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut msg = DisplayMessage::assistant_streaming(vec![]);

        let mut tc = DisplayToolCall::new(
            "tc1".to_string(),
            "file_read".to_string(),
            serde_json::json!({"path": "/src/main.rs"}),
        );
        tc.expanded = true;
        tc.complete_success(
            Some("fn main() {}".to_string()),
            Some("fn main() {\n    println!(\"Hello\");\n}".to_string()),
        );
        msg.add_tool_call(tc);

        terminal
            .draw(|f| {
                let widget = MessageWidget::new(&msg, f.area().width);
                f.render_widget(widget, f.area());
            })
            .unwrap();
    }
}
