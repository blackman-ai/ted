// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Status bar widget for the chat TUI

use ratatui::prelude::*;

use crate::tui::chat::state::truncate_string;

/// Widget for rendering the title/status bar
pub struct StatusBar<'a> {
    title: &'a str,
    provider: &'a str,
    model: &'a str,
    session_id: &'a str,
    caps: &'a [String],
    status_message: Option<&'a str>,
    status_is_error: bool,
    is_processing: bool,
}

impl<'a> StatusBar<'a> {
    pub fn new(title: &'a str, provider: &'a str, model: &'a str, session_id: &'a str) -> Self {
        Self {
            title,
            provider,
            model,
            session_id,
            caps: &[],
            status_message: None,
            status_is_error: false,
            is_processing: false,
        }
    }

    pub fn caps(mut self, caps: &'a [String]) -> Self {
        self.caps = caps;
        self
    }

    pub fn status(mut self, message: Option<&'a str>, is_error: bool) -> Self {
        self.status_message = message;
        self.status_is_error = is_error;
        self
    }

    pub fn processing(mut self, is_processing: bool) -> Self {
        self.is_processing = is_processing;
        self
    }
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

        // Clear the line with dark background
        let bg_style = Style::default().bg(Color::DarkGray);
        for x in area.x..area.x + area.width {
            buf.set_string(x, area.y, " ", bg_style);
        }

        let mut x = area.x + 1;

        // Title
        let title_style = Style::default().fg(Color::White).bold().bg(Color::DarkGray);
        buf.set_string(x, area.y, self.title, title_style);
        x += self.title.len() as u16 + 1;

        // Separator
        buf.set_string(
            x,
            area.y,
            "─",
            Style::default().fg(Color::Gray).bg(Color::DarkGray),
        );
        x += 2;

        // Provider and model
        let info = format!("{} / {}", self.provider, self.model);
        buf.set_string(
            x,
            area.y,
            &info,
            Style::default().fg(Color::Cyan).bg(Color::DarkGray),
        );
        x += info.len() as u16 + 2;

        // Session ID (short)
        let session_short = &self.session_id[..8.min(self.session_id.len())];
        buf.set_string(
            x,
            area.y,
            session_short,
            Style::default().fg(Color::Gray).bg(Color::DarkGray),
        );
        x += session_short.len() as u16 + 2;

        // Caps badges
        for cap in self.caps {
            if x + cap.len() as u16 + 4 > area.x + area.width {
                break;
            }
            buf.set_string(x, area.y, " ", Style::default().bg(Color::Blue));
            x += 1;
            buf.set_string(
                x,
                area.y,
                cap,
                Style::default().fg(Color::White).bg(Color::Blue),
            );
            x += cap.len() as u16;
            buf.set_string(x, area.y, " ", Style::default().bg(Color::Blue));
            x += 2;
        }

        // Right-aligned: status message or processing indicator
        if self.is_processing {
            let indicator = "● Processing...";
            let indicator_x = area.x + area.width - indicator.len() as u16 - 1;
            if indicator_x > x {
                buf.set_string(
                    indicator_x,
                    area.y,
                    indicator,
                    Style::default().fg(Color::Green).bg(Color::DarkGray),
                );
            }
        } else if let Some(status) = self.status_message {
            let status_style = if self.status_is_error {
                Style::default().fg(Color::Red).bg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Yellow).bg(Color::DarkGray)
            };

            let status_truncated = truncate_string(status, 30);

            let status_x = area.x + area.width - status_truncated.len() as u16 - 1;
            if status_x > x {
                buf.set_string(status_x, area.y, &status_truncated, status_style);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_status_bar_render() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let caps = vec!["rust-expert".to_string()];

        terminal
            .draw(|f| {
                let bar = StatusBar::new("ted", "anthropic", "claude-sonnet-4", "12345678")
                    .caps(&caps)
                    .processing(true);

                f.render_widget(bar, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_status_bar_new() {
        let bar = StatusBar::new("ted", "anthropic", "claude-3", "abcd1234");
        assert_eq!(bar.title, "ted");
        assert_eq!(bar.provider, "anthropic");
        assert_eq!(bar.model, "claude-3");
        assert_eq!(bar.session_id, "abcd1234");
        assert!(bar.caps.is_empty());
        assert!(bar.status_message.is_none());
        assert!(!bar.status_is_error);
        assert!(!bar.is_processing);
    }

    #[test]
    fn test_status_bar_caps_builder() {
        let caps = vec!["rust-expert".to_string(), "code-review".to_string()];
        let bar = StatusBar::new("ted", "anthropic", "claude-3", "abcd1234").caps(&caps);
        assert_eq!(bar.caps.len(), 2);
    }

    #[test]
    fn test_status_bar_status_builder() {
        let bar = StatusBar::new("ted", "anthropic", "claude-3", "abcd1234")
            .status(Some("Rate limited"), false);
        assert_eq!(bar.status_message, Some("Rate limited"));
        assert!(!bar.status_is_error);
    }

    #[test]
    fn test_status_bar_status_error() {
        let bar = StatusBar::new("ted", "anthropic", "claude-3", "abcd1234")
            .status(Some("Error occurred"), true);
        assert_eq!(bar.status_message, Some("Error occurred"));
        assert!(bar.status_is_error);
    }

    #[test]
    fn test_status_bar_processing_builder() {
        let bar = StatusBar::new("ted", "anthropic", "claude-3", "abcd1234").processing(true);
        assert!(bar.is_processing);
    }

    #[test]
    fn test_status_bar_render_with_error() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let bar = StatusBar::new("ted", "anthropic", "claude-sonnet-4", "12345678")
                    .status(Some("Connection error"), true);

                f.render_widget(bar, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_status_bar_render_with_warning() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let bar = StatusBar::new("ted", "anthropic", "claude-sonnet-4", "12345678")
                    .status(Some("Rate limited, waiting..."), false);

                f.render_widget(bar, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_status_bar_render_multiple_caps() {
        let backend = TestBackend::new(120, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let caps = vec![
            "rust-expert".to_string(),
            "code-review".to_string(),
            "security".to_string(),
        ];

        terminal
            .draw(|f| {
                let bar =
                    StatusBar::new("ted", "anthropic", "claude-sonnet-4", "12345678").caps(&caps);

                f.render_widget(bar, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_status_bar_render_narrow() {
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let caps = vec!["rust".to_string()];

        terminal
            .draw(|f| {
                let bar = StatusBar::new("ted", "anthropic", "claude-3", "12345678")
                    .caps(&caps)
                    .status(Some("Testing"), false);

                f.render_widget(bar, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_status_bar_render_zero_height() {
        let backend = TestBackend::new(80, 0);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let bar = StatusBar::new("ted", "anthropic", "claude-3", "12345678");
                // Render with zero height area
                f.render_widget(bar, Rect::new(0, 0, 80, 0));
            })
            .unwrap();
        // Should not panic
    }

    #[test]
    fn test_status_bar_long_status_truncates() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let bar = StatusBar::new("ted", "anthropic", "claude-sonnet-4", "12345678").status(
                    Some("This is a very long status message that should be truncated"),
                    false,
                );

                f.render_widget(bar, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_status_bar_short_session_id() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let bar = StatusBar::new("ted", "anthropic", "claude-3", "abc");
                f.render_widget(bar, f.area());
            })
            .unwrap();
    }
}
