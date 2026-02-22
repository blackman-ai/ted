// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use super::super::app::{ChatApp, ChatMode};
use super::super::input::bindings_for_mode;
use super::super::widgets::message::render_messages;
use super::super::widgets::{AgentPane, StatusBar};

pub(super) fn render_title_bar(frame: &mut Frame, app: &ChatApp, area: Rect) {
    let session_id = app.session_id.to_string();
    // Filter out "base" cap - it's always applied silently and shouldn't be shown.
    let visible_caps: Vec<String> = app
        .caps
        .iter()
        .filter(|c| c.as_str() != "base")
        .cloned()
        .collect();
    let bar = StatusBar::new("ted", &app.provider_name, &app.model, &session_id)
        .caps(&visible_caps)
        .status(app.status_message.as_deref(), app.status_is_error)
        .processing(app.is_processing);

    frame.render_widget(bar, area);
}

pub(super) fn render_chat_area(frame: &mut Frame, app: &mut ChatApp, area: Rect) {
    let block = Block::default().borders(Borders::NONE);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Update scroll state with current dimensions.
    app.scroll_state.update_viewport_height(inner.height);
    app.update_message_area_width(inner.width);
    let total_height = app
        .scroll_state
        .calculate_total_height(&app.messages, inner.width);

    // Render messages with improved scrolling.
    let buf = frame.buffer_mut();
    render_messages(&app.messages, inner, buf, app.scroll_state.scroll_offset);

    // If no messages, show welcome text.
    if app.messages.is_empty() {
        let welcome = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Welcome to Ted!",
                Style::default().fg(Color::Cyan).bold(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Type a message to start chatting, or use /help for commands.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Keyboard shortcuts:",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  Tab    - Toggle agent pane",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  Esc    - Switch to scroll mode",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  ?      - Show help",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  Ctrl+C - Cancel/Quit",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .alignment(Alignment::Center);

        let welcome_area = Rect {
            x: area.x + area.width / 4,
            y: area.y + area.height / 3,
            width: area.width / 2,
            height: 12,
        };

        frame.render_widget(welcome, welcome_area);
    }

    // Render scroll indicator if needed.
    if !app.messages.is_empty() {
        render_scroll_indicator(frame, app, area, total_height);
    }
}

/// Render a scroll indicator showing current position.
pub(super) fn render_scroll_indicator(
    frame: &mut Frame,
    app: &mut ChatApp,
    area: Rect,
    total_height: usize,
) {
    if let Some((current_line, _viewport_end, total_lines)) =
        app.scroll_state.scroll_indicator(total_height)
    {
        // Only show indicator if there's content to scroll.
        if total_lines > app.scroll_state.viewport_height as usize {
            let indicator_text = if app.scroll_state.is_at_top() {
                "⬆ Top".to_string()
            } else if app.scroll_state.is_at_bottom(total_height) {
                "⬇ Bottom".to_string()
            } else {
                format!("↕ {}/{}", current_line, total_lines)
            };

            let indicator = Paragraph::new(indicator_text)
                .style(Style::default().fg(Color::DarkGray).bg(Color::Black))
                .alignment(Alignment::Right);

            // Position indicator in bottom-right corner.
            let indicator_area = Rect {
                x: area.x + area.width.saturating_sub(15),
                y: area.y + area.height.saturating_sub(1),
                width: 15,
                height: 1,
            };

            frame.render_widget(indicator, indicator_area);
        }
    }
}

pub(super) fn render_agent_pane(frame: &mut Frame, app: &ChatApp, area: Rect) {
    let pane = AgentPane::new(&app.agents)
        .expanded(app.agent_pane_expanded)
        .focused(app.mode == ChatMode::AgentFocus);

    frame.render_widget(pane, area);
}

pub(super) fn render_input_area(frame: &mut Frame, app: &ChatApp, area: Rect) {
    let focused = app.mode == ChatMode::Input;

    // Get hints for current mode.
    let hints: Vec<(&str, &str)> = bindings_for_mode(app.mode)
        .iter()
        .take(5)
        .map(|b| (b.keys, b.description))
        .collect();

    let buf = frame.buffer_mut();
    super::super::widgets::input_area::render_input_with_hints(
        &app.input, area, buf, focused, &hints,
    );
}
