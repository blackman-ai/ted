// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Main UI rendering for the chat TUI

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::app::{ChatApp, ChatMode};
use super::input::bindings_for_mode;
use super::widgets::message::render_messages;
use super::widgets::{AgentPane, InputArea, StatusBar};

/// Main draw function for the chat TUI
pub fn draw(frame: &mut Frame, app: &mut ChatApp) {
    let area = frame.area();

    // Calculate layout
    let layout = calculate_layout(area, app);

    // Update scroll state with current viewport height
    app.scroll_state.update_viewport_height(layout.chat.height);

    // Render title bar
    render_title_bar(frame, app, layout.title_bar);

    // Render chat messages
    render_chat_area(frame, app, layout.chat);

    // Render agent pane if visible
    if app.agent_pane_visible && app.agents.total_count() > 0 {
        render_agent_pane(frame, app, layout.agents);
    }

    // Render input area
    render_input_area(frame, app, layout.input);

    // Render overlays (help, confirm dialogs, etc.)
    match app.mode {
        ChatMode::Help => render_help_overlay(frame, area),
        ChatMode::Confirm => render_confirm_dialog(frame, app, area),
        _ => {}
    }

    // Position cursor
    if app.mode == ChatMode::Input {
        let cursor_pos = InputArea::new(&app.input).cursor_position(layout.input);
        frame.set_cursor_position(cursor_pos);
    }
}

/// Layout regions
struct Layout {
    title_bar: Rect,
    chat: Rect,
    agents: Rect,
    input: Rect,
}

fn calculate_layout(area: Rect, app: &ChatApp) -> Layout {
    // Title bar: 1 line
    // Input area: 3 lines
    // Agent pane: 0-10 lines (depending on state)
    // Chat: remaining space

    let title_height = 1;
    let input_height = 3;
    let agent_height = if app.agent_pane_visible && app.agents.total_count() > 0 {
        if app.agent_pane_expanded {
            app.agent_pane_height.min(area.height / 3)
        } else {
            3
        }
    } else {
        0
    };

    let chat_height = area
        .height
        .saturating_sub(title_height)
        .saturating_sub(input_height)
        .saturating_sub(agent_height);

    Layout {
        title_bar: Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: title_height,
        },
        chat: Rect {
            x: area.x,
            y: area.y + title_height,
            width: area.width,
            height: chat_height,
        },
        agents: Rect {
            x: area.x,
            y: area.y + title_height + chat_height,
            width: area.width,
            height: agent_height,
        },
        input: Rect {
            x: area.x,
            y: area.y + title_height + chat_height + agent_height,
            width: area.width,
            height: input_height,
        },
    }
}

fn render_title_bar(frame: &mut Frame, app: &ChatApp, area: Rect) {
    let session_id = app.session_id.to_string();
    let bar = StatusBar::new("ted", &app.provider_name, &app.model, &session_id)
        .caps(&app.caps)
        .status(app.status_message.as_deref(), app.status_is_error)
        .processing(app.is_processing);

    frame.render_widget(bar, area);
}

fn render_chat_area(frame: &mut Frame, app: &mut ChatApp, area: Rect) {
    let block = Block::default().borders(Borders::NONE);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Update scroll state with current dimensions
    app.scroll_state.update_viewport_height(inner.height);
    let total_height = app
        .scroll_state
        .calculate_total_height(&app.messages, inner.width);

    // Render messages with improved scrolling
    let buf = frame.buffer_mut();
    render_messages(&app.messages, inner, buf, app.scroll_state.scroll_offset);

    // If no messages, show welcome text
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

    // Render scroll indicator if needed
    if !app.messages.is_empty() {
        render_scroll_indicator(frame, app, area, total_height);
    }
}

/// Render a scroll indicator showing current position
fn render_scroll_indicator(frame: &mut Frame, app: &mut ChatApp, area: Rect, total_height: usize) {
    if let Some((current_line, _viewport_end, total_lines)) =
        app.scroll_state.scroll_indicator(total_height)
    {
        // Only show indicator if there's content to scroll
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

            // Position indicator in bottom-right corner
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

fn render_agent_pane(frame: &mut Frame, app: &ChatApp, area: Rect) {
    let pane = AgentPane::new(&app.agents)
        .expanded(app.agent_pane_expanded)
        .focused(app.mode == ChatMode::AgentFocus);

    frame.render_widget(pane, area);
}

fn render_input_area(frame: &mut Frame, app: &ChatApp, area: Rect) {
    let focused = app.mode == ChatMode::Input;

    // Get hints for current mode
    let hints: Vec<(&str, &str)> = bindings_for_mode(app.mode)
        .iter()
        .take(5)
        .map(|b| (b.keys, b.description))
        .collect();

    let buf = frame.buffer_mut();
    super::widgets::input_area::render_input_with_hints(&app.input, area, buf, focused, &hints);
}

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    // Semi-transparent overlay
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

fn render_confirm_dialog(frame: &mut Frame, app: &ChatApp, area: Rect) {
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

/// Helper to create a centered rect with percentage-based sizing
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::context::ContextManager;
    use crate::llm::mock_provider::MockProvider;
    use crate::llm::provider::LlmProvider;
    use crate::tools::{ToolContext, ToolExecutor};
    use crate::tui::chat::ChatTuiConfig;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::sync::Arc;
    use uuid::Uuid;

    // ==================== Helper Functions ====================

    /// Helper function to create a test ChatApp
    async fn create_test_chat_app() -> ChatApp {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let storage_path = temp_dir.path().to_path_buf();

        let config = ChatTuiConfig {
            session_id: Uuid::new_v4(),
            provider_name: "mock".to_string(),
            model: "test-model".to_string(),
            caps: vec!["test-cap".to_string()],
            trust_mode: false,
            stream_enabled: true,
        };

        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::new());

        let tool_context = ToolContext::new(
            storage_path.clone(),
            Some(storage_path.clone()),
            config.session_id,
            false,
        );
        let tool_executor = ToolExecutor::new(tool_context, false);

        let context_manager = ContextManager::new_session(storage_path).await.unwrap();
        let settings = Settings::default();

        ChatApp::new(
            config,
            event_tx,
            event_rx,
            provider,
            tool_executor,
            context_manager,
            settings,
        )
    }

    fn create_test_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
        let backend = TestBackend::new(width, height);
        Terminal::new(backend).unwrap()
    }

    // ==================== centered_rect Tests ====================

    #[test]
    fn test_centered_rect() {
        let area = Rect::new(0, 0, 100, 50);
        let centered = centered_rect(50, 50, area);

        assert_eq!(centered.width, 50);
        assert_eq!(centered.height, 25);
        assert_eq!(centered.x, 25);
        assert_eq!(centered.y, 12);
    }

    #[test]
    fn test_centered_rect_full_size() {
        let area = Rect::new(0, 0, 100, 50);
        let centered = centered_rect(100, 100, area);

        assert_eq!(centered.width, 100);
        assert_eq!(centered.height, 50);
        assert_eq!(centered.x, 0);
        assert_eq!(centered.y, 0);
    }

    #[test]
    fn test_centered_rect_small() {
        let area = Rect::new(0, 0, 100, 50);
        let centered = centered_rect(10, 10, area);

        assert_eq!(centered.width, 10);
        assert_eq!(centered.height, 5);
        assert_eq!(centered.x, 45);
        assert_eq!(centered.y, 22);
    }

    #[test]
    fn test_centered_rect_with_offset() {
        let area = Rect::new(10, 5, 100, 50);
        let centered = centered_rect(50, 50, area);

        // Should be centered within the area, offset preserved
        assert_eq!(centered.width, 50);
        assert_eq!(centered.height, 25);
        assert_eq!(centered.x, 35); // 10 + 25
        assert_eq!(centered.y, 17); // 5 + 12
    }

    #[test]
    fn test_centered_rect_zero_percent() {
        let area = Rect::new(0, 0, 100, 50);
        let centered = centered_rect(0, 0, area);

        assert_eq!(centered.width, 0);
        assert_eq!(centered.height, 0);
        assert_eq!(centered.x, 50);
        assert_eq!(centered.y, 25);
    }

    #[test]
    fn test_centered_rect_asymmetric() {
        let area = Rect::new(0, 0, 200, 100);
        let centered = centered_rect(80, 40, area);

        // 80% of 200 = 160
        assert_eq!(centered.width, 160);
        // 40% of 100 = 40
        assert_eq!(centered.height, 40);
        // x centered: (200 - 160) / 2 = 20
        assert_eq!(centered.x, 20);
        // y centered: (100 - 40) / 2 = 30
        assert_eq!(centered.y, 30);
    }

    #[test]
    fn test_centered_rect_tiny_area() {
        let area = Rect::new(0, 0, 10, 5);
        let centered = centered_rect(50, 50, area);

        assert_eq!(centered.width, 5);
        assert_eq!(centered.height, 2);
        assert_eq!(centered.x, 2);
        assert_eq!(centered.y, 1);
    }

    // ==================== Layout Struct Tests ====================

    #[test]
    fn test_layout_struct_fields() {
        let layout = Layout {
            title_bar: Rect::new(0, 0, 80, 1),
            chat: Rect::new(0, 1, 80, 20),
            agents: Rect::new(0, 21, 80, 3),
            input: Rect::new(0, 24, 80, 3),
        };

        assert_eq!(layout.title_bar.height, 1);
        assert_eq!(layout.chat.height, 20);
        assert_eq!(layout.agents.height, 3);
        assert_eq!(layout.input.height, 3);
    }

    #[test]
    fn test_layout_struct_positioning() {
        let layout = Layout {
            title_bar: Rect::new(0, 0, 100, 1),
            chat: Rect::new(0, 1, 100, 50),
            agents: Rect::new(0, 51, 100, 5),
            input: Rect::new(0, 56, 100, 3),
        };

        // Title bar at top
        assert_eq!(layout.title_bar.y, 0);

        // Chat below title bar
        assert_eq!(layout.chat.y, layout.title_bar.y + layout.title_bar.height);

        // Agents below chat
        assert_eq!(layout.agents.y, layout.chat.y + layout.chat.height);

        // Input at bottom
        assert_eq!(layout.input.y, layout.agents.y + layout.agents.height);
    }

    #[test]
    fn test_layout_all_same_width() {
        let layout = Layout {
            title_bar: Rect::new(0, 0, 120, 1),
            chat: Rect::new(0, 1, 120, 30),
            agents: Rect::new(0, 31, 120, 4),
            input: Rect::new(0, 35, 120, 3),
        };

        // All sections should have the same width
        assert_eq!(layout.title_bar.width, 120);
        assert_eq!(layout.chat.width, 120);
        assert_eq!(layout.agents.width, 120);
        assert_eq!(layout.input.width, 120);
    }

    #[test]
    fn test_layout_minimum_sizes() {
        let layout = Layout {
            title_bar: Rect::new(0, 0, 40, 1),
            chat: Rect::new(0, 1, 40, 5),
            agents: Rect::new(0, 6, 40, 0),
            input: Rect::new(0, 6, 40, 3),
        };

        // Title bar always 1 line
        assert_eq!(layout.title_bar.height, 1);
        // Input always 3 lines
        assert_eq!(layout.input.height, 3);
        // Agents can be 0 if hidden
        assert_eq!(layout.agents.height, 0);
    }

    // ==================== calculate_layout Tests ====================

    #[tokio::test]
    async fn test_calculate_layout_no_agents() {
        let app = create_test_chat_app().await;
        let area = Rect::new(0, 0, 80, 24);

        let layout = calculate_layout(area, &app);

        // Title bar: 1 line
        assert_eq!(layout.title_bar.height, 1);
        assert_eq!(layout.title_bar.y, 0);

        // Input: 3 lines
        assert_eq!(layout.input.height, 3);

        // No agents visible (empty tracker), so agent height = 0
        assert_eq!(layout.agents.height, 0);

        // Chat gets remaining space: 24 - 1 - 3 - 0 = 20
        assert_eq!(layout.chat.height, 20);
    }

    #[tokio::test]
    async fn test_calculate_layout_with_agents() {
        let mut app = create_test_chat_app().await;

        // Add an agent to make agent pane visible
        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            "TestAgent".to_string(),
            "explore".to_string(),
            "Find files".to_string(),
        );

        let area = Rect::new(0, 0, 80, 24);
        let layout = calculate_layout(area, &app);

        // With agents, there should be agent pane space
        // Not expanded = 3 lines for agent pane
        assert!(layout.agents.height > 0);

        // Chat height reduced by agent pane
        let expected_chat = 24 - 1 - 3 - layout.agents.height;
        assert_eq!(layout.chat.height, expected_chat);
    }

    #[tokio::test]
    async fn test_calculate_layout_expanded_agents() {
        let mut app = create_test_chat_app().await;

        // Add an agent
        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            "TestAgent".to_string(),
            "explore".to_string(),
            "Find files".to_string(),
        );

        // Manually expand the agent pane
        app.agent_pane_expanded = true;
        app.agent_pane_height = 8;

        let area = Rect::new(0, 0, 80, 30);
        let layout = calculate_layout(area, &app);

        // Expanded pane uses configured height (up to 1/3 of screen)
        // 30 / 3 = 10, so 8 is within limits
        assert_eq!(layout.agents.height, 8);
    }

    #[tokio::test]
    async fn test_calculate_layout_agent_pane_capped() {
        let mut app = create_test_chat_app().await;

        // Add an agent
        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            "TestAgent".to_string(),
            "explore".to_string(),
            "Find files".to_string(),
        );

        // Set agent pane to very large value
        app.agent_pane_expanded = true;
        app.agent_pane_height = 50;

        let area = Rect::new(0, 0, 80, 24);
        let layout = calculate_layout(area, &app);

        // Should be capped at 1/3 of height (24 / 3 = 8)
        assert_eq!(layout.agents.height, 8);
    }

    #[tokio::test]
    async fn test_calculate_layout_hidden_agent_pane() {
        let mut app = create_test_chat_app().await;

        // Add an agent but hide the pane
        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            "TestAgent".to_string(),
            "explore".to_string(),
            "Find files".to_string(),
        );

        app.agent_pane_visible = false;

        let area = Rect::new(0, 0, 80, 24);
        let layout = calculate_layout(area, &app);

        // Agent pane hidden
        assert_eq!(layout.agents.height, 0);
    }

    #[tokio::test]
    async fn test_calculate_layout_small_terminal() {
        let app = create_test_chat_app().await;
        let area = Rect::new(0, 0, 40, 10);

        let layout = calculate_layout(area, &app);

        // Even with small terminal, title and input take their space
        assert_eq!(layout.title_bar.height, 1);
        assert_eq!(layout.input.height, 3);

        // Chat gets what's left: 10 - 1 - 3 = 6
        assert_eq!(layout.chat.height, 6);
    }

    #[tokio::test]
    async fn test_calculate_layout_large_terminal() {
        let app = create_test_chat_app().await;
        let area = Rect::new(0, 0, 200, 60);

        let layout = calculate_layout(area, &app);

        // All widths should match terminal width
        assert_eq!(layout.title_bar.width, 200);
        assert_eq!(layout.chat.width, 200);
        assert_eq!(layout.agents.width, 200);
        assert_eq!(layout.input.width, 200);
    }

    #[tokio::test]
    async fn test_calculate_layout_with_offset() {
        let app = create_test_chat_app().await;
        let area = Rect::new(10, 5, 80, 24);

        let layout = calculate_layout(area, &app);

        // X positions should respect offset
        assert_eq!(layout.title_bar.x, 10);
        assert_eq!(layout.chat.x, 10);
        assert_eq!(layout.agents.x, 10);
        assert_eq!(layout.input.x, 10);

        // Y positions should be relative to area.y
        assert_eq!(layout.title_bar.y, 5);
        assert_eq!(layout.chat.y, 6); // 5 + 1
    }

    // ==================== draw Function Tests ====================

    #[tokio::test]
    async fn test_draw_basic() {
        let mut app = create_test_chat_app().await;
        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();

        // Just verify it doesn't panic
    }

    #[tokio::test]
    async fn test_draw_with_messages() {
        let mut app = create_test_chat_app().await;
        app.messages
            .push(crate::tui::chat::state::DisplayMessage::user(
                "Hello!".to_string(),
            ));
        app.messages
            .push(crate::tui::chat::state::DisplayMessage::assistant(
                "Hi there!".to_string(),
                vec![],
            ));

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_draw_with_agents() {
        let mut app = create_test_chat_app().await;

        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            "TestAgent".to_string(),
            "explore".to_string(),
            "Find files".to_string(),
        );

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_draw_help_mode() {
        let mut app = create_test_chat_app().await;
        app.mode = ChatMode::Help;

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_draw_confirm_mode() {
        let mut app = create_test_chat_app().await;
        app.mode = ChatMode::Confirm;
        app.confirm_message = Some("Are you sure?".to_string());

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_draw_input_mode_sets_cursor() {
        let mut app = create_test_chat_app().await;
        app.mode = ChatMode::Input;
        app.input.insert_char('H');
        app.input.insert_char('i');

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();

        // In input mode, cursor should be positioned
    }

    #[tokio::test]
    async fn test_draw_normal_mode() {
        let mut app = create_test_chat_app().await;
        app.mode = ChatMode::Normal;

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_draw_agent_focus_mode() {
        let mut app = create_test_chat_app().await;

        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            "TestAgent".to_string(),
            "explore".to_string(),
            "Find files".to_string(),
        );

        app.mode = ChatMode::AgentFocus;

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_draw_with_status_message() {
        let mut app = create_test_chat_app().await;
        app.status_message = Some("Processing...".to_string());
        app.status_is_error = false;

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_draw_with_error_message() {
        let mut app = create_test_chat_app().await;
        app.status_message = Some("Something went wrong".to_string());
        app.status_is_error = true;

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_draw_processing_indicator() {
        let mut app = create_test_chat_app().await;
        app.is_processing = true;

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    // ==================== render_title_bar Tests ====================

    #[tokio::test]
    async fn test_render_title_bar() {
        let app = create_test_chat_app().await;
        let mut terminal = create_test_terminal(80, 1);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_title_bar(frame, &app, area);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_render_title_bar_with_status() {
        let mut app = create_test_chat_app().await;
        app.status_message = Some("Ready".to_string());
        app.status_is_error = false;

        let mut terminal = create_test_terminal(80, 1);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_title_bar(frame, &app, area);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_render_title_bar_processing() {
        let mut app = create_test_chat_app().await;
        app.is_processing = true;

        let mut terminal = create_test_terminal(80, 1);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_title_bar(frame, &app, area);
            })
            .unwrap();
    }

    // ==================== render_chat_area Tests ====================

    #[tokio::test]
    async fn test_render_chat_area_empty() {
        let mut app = create_test_chat_app().await;
        let mut terminal = create_test_terminal(80, 20);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_chat_area(frame, &mut app, area);
            })
            .unwrap();

        // Should show welcome message when empty
    }

    #[tokio::test]
    async fn test_render_chat_area_with_messages() {
        let mut app = create_test_chat_app().await;
        app.messages
            .push(crate::tui::chat::state::DisplayMessage::user(
                "Hello".to_string(),
            ));
        app.messages
            .push(crate::tui::chat::state::DisplayMessage::assistant(
                "World".to_string(),
                vec![],
            ));

        let mut terminal = create_test_terminal(80, 20);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_chat_area(frame, &mut app, area);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_render_chat_area_with_scroll() {
        let mut app = create_test_chat_app().await;

        // Add multiple messages
        for i in 0..20 {
            app.messages
                .push(crate::tui::chat::state::DisplayMessage::user(format!(
                    "Message {}",
                    i
                )));
        }

        app.scroll_state.scroll_offset = 5;

        let mut terminal = create_test_terminal(80, 20);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_chat_area(frame, &mut app, area);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_render_chat_area_streaming() {
        let mut app = create_test_chat_app().await;
        app.messages
            .push(crate::tui::chat::state::DisplayMessage::assistant_streaming(vec![]));

        let mut terminal = create_test_terminal(80, 20);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_chat_area(frame, &mut app, area);
            })
            .unwrap();
    }

    // ==================== render_agent_pane Tests ====================

    #[tokio::test]
    async fn test_render_agent_pane() {
        let mut app = create_test_chat_app().await;

        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            "Explorer".to_string(),
            "explore".to_string(),
            "Find relevant files".to_string(),
        );

        let mut terminal = create_test_terminal(80, 5);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_agent_pane(frame, &app, area);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_render_agent_pane_expanded() {
        let mut app = create_test_chat_app().await;

        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            "Explorer".to_string(),
            "explore".to_string(),
            "Find relevant files".to_string(),
        );

        app.agent_pane_expanded = true;

        let mut terminal = create_test_terminal(80, 10);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_agent_pane(frame, &app, area);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_render_agent_pane_focused() {
        let mut app = create_test_chat_app().await;

        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            "Explorer".to_string(),
            "explore".to_string(),
            "Find relevant files".to_string(),
        );

        app.mode = ChatMode::AgentFocus;

        let mut terminal = create_test_terminal(80, 5);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_agent_pane(frame, &app, area);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_render_agent_pane_multiple_agents() {
        let mut app = create_test_chat_app().await;

        for i in 0..3 {
            let agent_id = uuid::Uuid::new_v4();
            app.agents.track(
                agent_id,
                format!("Agent{}", i),
                "explore".to_string(),
                format!("Task {}", i),
            );
        }

        let mut terminal = create_test_terminal(80, 10);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_agent_pane(frame, &app, area);
            })
            .unwrap();
    }

    // ==================== render_input_area Tests ====================

    #[tokio::test]
    async fn test_render_input_area_empty() {
        let app = create_test_chat_app().await;
        let mut terminal = create_test_terminal(80, 3);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_input_area(frame, &app, area);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_render_input_area_with_text() {
        let mut app = create_test_chat_app().await;
        app.input.insert_char('H');
        app.input.insert_char('e');
        app.input.insert_char('l');
        app.input.insert_char('l');
        app.input.insert_char('o');

        let mut terminal = create_test_terminal(80, 3);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_input_area(frame, &app, area);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_render_input_area_focused() {
        let mut app = create_test_chat_app().await;
        app.mode = ChatMode::Input;

        let mut terminal = create_test_terminal(80, 3);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_input_area(frame, &app, area);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_render_input_area_unfocused() {
        let mut app = create_test_chat_app().await;
        app.mode = ChatMode::Normal;

        let mut terminal = create_test_terminal(80, 3);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_input_area(frame, &app, area);
            })
            .unwrap();
    }

    // ==================== render_help_overlay Tests ====================

    #[test]
    fn test_render_help_overlay() {
        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_help_overlay(frame, area);
            })
            .unwrap();
    }

    #[test]
    fn test_render_help_overlay_small_terminal() {
        let mut terminal = create_test_terminal(40, 12);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_help_overlay(frame, area);
            })
            .unwrap();
    }

    #[test]
    fn test_render_help_overlay_large_terminal() {
        let mut terminal = create_test_terminal(200, 60);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_help_overlay(frame, area);
            })
            .unwrap();
    }

    // ==================== render_confirm_dialog Tests ====================

    #[tokio::test]
    async fn test_render_confirm_dialog() {
        let mut app = create_test_chat_app().await;
        app.confirm_message = Some("Delete file?".to_string());

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_confirm_dialog(frame, &app, area);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_render_confirm_dialog_no_message() {
        let app = create_test_chat_app().await;

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_confirm_dialog(frame, &app, area);
            })
            .unwrap();

        // Should not render anything when no message
    }

    #[tokio::test]
    async fn test_render_confirm_dialog_long_message() {
        let mut app = create_test_chat_app().await;
        app.confirm_message = Some(
            "Are you absolutely sure you want to delete this very important file?".to_string(),
        );

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_confirm_dialog(frame, &app, area);
            })
            .unwrap();
    }

    // ==================== Edge Cases ====================

    #[tokio::test]
    async fn test_draw_tiny_terminal() {
        let mut app = create_test_chat_app().await;
        let mut terminal = create_test_terminal(10, 5);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_draw_very_wide_terminal() {
        let mut app = create_test_chat_app().await;
        let mut terminal = create_test_terminal(300, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_draw_very_tall_terminal() {
        let mut app = create_test_chat_app().await;
        let mut terminal = create_test_terminal(80, 100);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_draw_all_modes() {
        let mut app = create_test_chat_app().await;
        let modes = [
            ChatMode::Normal,
            ChatMode::Input,
            ChatMode::Help,
            ChatMode::Confirm,
            ChatMode::CommandPalette,
            ChatMode::Settings,
        ];

        for mode in modes {
            app.mode = mode;
            if mode == ChatMode::Confirm {
                app.confirm_message = Some("Test?".to_string());
            }

            let mut terminal = create_test_terminal(80, 24);
            terminal
                .draw(|frame| {
                    draw(frame, &mut app);
                })
                .unwrap();
        }
    }

    #[tokio::test]
    async fn test_draw_with_tool_calls() {
        let mut app = create_test_chat_app().await;

        // Create a message with tool calls
        let mut msg = crate::tui::chat::state::DisplayMessage::assistant(
            "Reading file...".to_string(),
            vec![],
        );
        msg.add_tool_call(crate::tui::chat::state::DisplayToolCall::new(
            "tool-1".to_string(),
            "file_read".to_string(),
            serde_json::json!({"path": "/test.txt"}),
        ));
        app.messages.push(msg);

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_draw_with_completed_agent() {
        let mut app = create_test_chat_app().await;

        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            "TestAgent".to_string(),
            "explore".to_string(),
            "Find files".to_string(),
        );

        // Complete the agent
        app.agents.set_completed(
            &agent_id,
            vec!["file.rs".to_string()],
            Some("Found 5 files".to_string()),
        );

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }

    #[tokio::test]
    async fn test_draw_with_failed_agent() {
        let mut app = create_test_chat_app().await;

        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            "TestAgent".to_string(),
            "explore".to_string(),
            "Find files".to_string(),
        );

        // Fail the agent
        app.agents.set_failed(&agent_id, "Connection timeout");

        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|frame| {
                draw(frame, &mut app);
            })
            .unwrap();
    }
}
