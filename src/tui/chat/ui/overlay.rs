// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::super::app::ChatApp;

pub(super) fn render_help_overlay(frame: &mut Frame, area: Rect) {
    // Semi-transparent overlay.
    let overlay_area = centered_rect(60, 80, area);
    frame.render_widget(Clear, overlay_area);

    let help_text = vec![
        Line::from(Span::styled(
            " Ted Help ",
            Style::default().fg(Color::Cyan).bold(),
        )),
        Line::from(""),
        Line::from(Span::styled("Input Mode:", Style::default().bold())),
        Line::from("  Enter       Send message"),
        Line::from("  ↑/↓         History navigation"),
        Line::from("  Tab         Toggle agent pane"),
        Line::from("  Esc         Switch to scroll mode"),
        Line::from("  Ctrl+C      Cancel operation / Quit"),
        Line::from(""),
        Line::from(Span::styled("Scroll Mode:", Style::default().bold())),
        Line::from("  j/k or ↑/↓  Scroll messages"),
        Line::from("  g/G         Go to top/bottom"),
        Line::from("  Enter/i     Return to input mode"),
        Line::from("  Ctrl+A      Focus agent pane"),
        Line::from("  q           Quit"),
        Line::from(""),
        Line::from(Span::styled("Commands:", Style::default().bold())),
        Line::from("  /help       Show this help"),
        Line::from("  /clear      Clear chat history"),
        Line::from("  /agents     Toggle agent pane"),
        Line::from("  /quit       Exit Ted"),
        Line::from(""),
        Line::from(Span::styled(
            "Press Esc or ? to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Help ")
                .title_style(Style::default().fg(Color::White).bold()),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(help, overlay_area);
}

pub(super) fn render_confirm_dialog(frame: &mut Frame, app: &ChatApp, area: Rect) {
    if let Some(message) = &app.confirm_message {
        let dialog_area = centered_rect(40, 20, area);
        frame.render_widget(Clear, dialog_area);

        let dialog = Paragraph::new(vec![
            Line::from(""),
            Line::from(message.as_str()),
            Line::from(""),
            Line::from(vec![
                Span::styled("[Y]es", Style::default().fg(Color::Green)),
                Span::raw("  "),
                Span::styled("[N]o", Style::default().fg(Color::Red)),
            ]),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" Confirm ")
                .title_style(Style::default().fg(Color::White).bold()),
        )
        .alignment(Alignment::Center);

        frame.render_widget(dialog, dialog_area);
    }
}

/// Helper to create a centered rect with percentage-based sizing.
pub(super) fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_width = r.width * percent_x / 100;
    let popup_height = r.height * percent_y / 100;
    let popup_x = (r.width - popup_width) / 2;
    let popup_y = (r.height - popup_height) / 2;

    Rect {
        x: r.x + popup_x,
        y: r.y + popup_y,
        width: popup_width,
        height: popup_height,
    }
}
