// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use ratatui::prelude::*;

use crate::tui::chat::app::ChatMode;
use crate::tui::chat::state::agents::TrackedAgent;

use super::{SettingsField, SettingsSection, TuiState};

const MIN_SPLIT_WIDTH: u16 = 100;
/// Left pane gets 55% of width in split mode
const LEFT_PANE_RATIO: f32 = 0.55;

/// Draw the TUI
pub(super) fn draw_tui(frame: &mut Frame, state: &TuiState) {
    let area = frame.area();

    // Show split layout whenever there's a focused agent with enough terminal width.
    // AgentFocus mode controls keyboard input routing (agent scrolling), not visibility.
    let has_focused_agent = state.focused_agent_tool_id.is_some()
        && state
            .focused_agent_tool_id
            .as_ref()
            .and_then(|id| state.agents.get_by_tool_call_id(id))
            .is_some();

    if has_focused_agent && area.width >= MIN_SPLIT_WIDTH {
        draw_split_layout(frame, state, area);
    } else if state.mode == ChatMode::AgentFocus
        && has_focused_agent
        && area.width < MIN_SPLIT_WIDTH
    {
        // Narrow terminal + AgentFocus: draw normal layout + agent overlay
        draw_normal_layout(frame, state, area, false);
        draw_agent_overlay(frame, state, area);
    } else {
        draw_normal_layout(frame, state, area, false);
    }

    // Overlays (on top of everything)
    match state.mode {
        ChatMode::Help => draw_help_overlay(frame, area),
        ChatMode::Settings => draw_settings_overlay(frame, state, area),
        _ => {}
    }
}

/// Standard full-width vertical layout.
/// When `hide_agent_pane` is true, the bottom agent status bar is suppressed
/// (used in split layout where the right pane already shows agent info).
fn draw_normal_layout(
    frame: &mut Frame,
    state: &TuiState,
    area: ratatui::layout::Rect,
    hide_agent_pane: bool,
) {
    let title_height = 1;
    let input_height = 3;
    let show_agent_pane =
        !hide_agent_pane && state.agent_pane_visible && state.agents.total_count() > 0;
    let agent_height = if show_agent_pane {
        if state.agent_pane_expanded {
            state.agent_pane_height.min(area.height / 3)
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

    // Title bar
    let title_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: title_height,
    };
    draw_title_bar(frame, state, title_area);

    // Chat area
    let chat_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y + title_height,
        width: area.width,
        height: chat_height,
    };
    draw_chat_area(frame, state, chat_area);

    // Agent pane
    if show_agent_pane {
        let agents_area = ratatui::layout::Rect {
            x: area.x,
            y: area.y + title_height + chat_height,
            width: area.width,
            height: agent_height,
        };
        draw_agent_pane(frame, state, agents_area);
    }

    // Input area
    let input_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y + title_height + chat_height + agent_height,
        width: area.width,
        height: input_height,
    };
    draw_input_area(frame, state, input_area);
}

/// Horizontal split layout: left pane (main chat) | right pane (agent conversation)
fn draw_split_layout(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    let left_width = ((area.width as f32) * LEFT_PANE_RATIO) as u16;
    let separator_width: u16 = 1;
    let right_width = area
        .width
        .saturating_sub(left_width)
        .saturating_sub(separator_width);

    // Left pane: normal layout within left portion
    let left_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y,
        width: left_width,
        height: area.height,
    };
    draw_normal_layout(frame, state, left_area, true);

    // Vertical separator
    let sep_area = ratatui::layout::Rect {
        x: area.x + left_width,
        y: area.y,
        width: separator_width,
        height: area.height,
    };
    draw_vertical_separator(frame, sep_area);

    // Right pane: agent conversation
    let right_area = ratatui::layout::Rect {
        x: area.x + left_width + separator_width,
        y: area.y,
        width: right_width,
        height: area.height,
    };
    draw_agent_conversation_pane(frame, state, right_area);
}

/// Draw the vertical separator between split panes
fn draw_vertical_separator(frame: &mut Frame, area: ratatui::layout::Rect) {
    use ratatui::widgets::Paragraph;

    let lines: Vec<ratatui::text::Line> = (0..area.height)
        .map(|_| {
            ratatui::text::Line::from(ratatui::text::Span::styled(
                "│",
                Style::default().fg(Color::DarkGray),
            ))
        })
        .collect();

    let widget = Paragraph::new(lines);
    frame.render_widget(widget, area);
}

/// Draw the agent conversation pane (right side of split layout)
fn draw_agent_conversation_pane(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    use super::super::widgets::message::render_messages;

    let agent = state
        .focused_agent_tool_id
        .as_ref()
        .and_then(|id| state.agents.get_by_tool_call_id(id));

    let agent = match agent {
        Some(a) => a,
        None => return,
    };

    // Title bar (1 line)
    let title_height: u16 = 1;
    let title_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: title_height,
    };
    let is_focused = state.mode == ChatMode::AgentFocus;
    draw_agent_pane_title(frame, agent, title_area, is_focused);

    // Conversation area (remaining height)
    let conv_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y + title_height,
        width: area.width,
        height: area.height.saturating_sub(title_height),
    };

    if agent.messages.is_empty() {
        use ratatui::widgets::Paragraph;

        let waiting = Paragraph::new(ratatui::text::Line::from(ratatui::text::Span::styled(
            " Waiting for agent output...",
            Style::default().fg(Color::DarkGray).italic(),
        )));
        frame.render_widget(waiting, conv_area);
    } else {
        let buf = frame.buffer_mut();
        let scroll = agent.conversation_scroll.scroll_offset;
        render_messages(&agent.messages, conv_area, buf, scroll);
    }
}

/// Draw the agent pane title bar
pub(super) fn draw_agent_pane_title(
    frame: &mut Frame,
    agent: &TrackedAgent,
    area: ratatui::layout::Rect,
    focused: bool,
) {
    use ratatui::widgets::Paragraph;

    let status_indicator = agent.status.indicator();

    let elapsed = agent.elapsed_display();
    let focus_hint = if focused {
        " [Esc to unfocus]"
    } else {
        " [Ctrl+A to scroll]"
    };
    let title = format!(
        " {} {} {} ─ {} ─ {}{}",
        status_indicator,
        agent.agent_type,
        agent.name,
        elapsed,
        agent.status_display(),
        focus_hint
    );

    let bg = if focused {
        Color::Blue
    } else {
        Color::DarkGray
    };
    let widget = Paragraph::new(ratatui::text::Line::from(ratatui::text::Span::styled(
        title,
        Style::default().fg(Color::White).bg(bg),
    )))
    .style(Style::default().bg(bg));
    frame.render_widget(widget, area);
}

/// Draw agent conversation as a full-screen overlay (for narrow terminals)
fn draw_agent_overlay(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    use super::super::widgets::message::render_messages;
    use ratatui::widgets::{Block, Borders, Clear, Paragraph};

    let agent = state
        .focused_agent_tool_id
        .as_ref()
        .and_then(|id| state.agents.get_by_tool_call_id(id));

    let agent = match agent {
        Some(a) => a,
        None => return,
    };

    // Use most of the screen
    let overlay_area = ratatui::layout::Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    // Clear the overlay area
    frame.render_widget(Clear, overlay_area);

    // Border
    let status_indicator = agent.status.indicator();
    let title = format!(
        " {} {} ─ {} ",
        status_indicator, agent.agent_type, agent.name
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(title);
    let inner = block.inner(overlay_area);
    frame.render_widget(block, overlay_area);

    if agent.messages.is_empty() {
        let waiting = Paragraph::new(ratatui::text::Line::from(ratatui::text::Span::styled(
            "Waiting for agent output...",
            Style::default().fg(Color::DarkGray).italic(),
        )));
        frame.render_widget(waiting, inner);
    } else {
        let buf = frame.buffer_mut();
        let scroll = agent.conversation_scroll.scroll_offset;
        render_messages(&agent.messages, inner, buf, scroll);
    }
}

pub(super) fn draw_title_bar(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    use ratatui::widgets::Paragraph;

    let session_short = &state.config.session_id.to_string()[..8];
    let title = format!(
        " ted ─ {} / {} ─ {} ",
        state.config.provider_name, state.current_model, session_short
    );

    let mut title_spans = vec![ratatui::text::Span::styled(
        &title,
        Style::default().fg(Color::White).bg(Color::DarkGray),
    )];

    // Add caps badges (filter out "base" - it's always applied silently)
    for cap in &state.config.caps {
        if cap == "base" {
            continue;
        }
        title_spans.push(ratatui::text::Span::styled(
            format!(" {} ", cap),
            Style::default().fg(Color::White).bg(Color::Blue),
        ));
        title_spans.push(ratatui::text::Span::raw(" "));
    }

    // Add status
    if state.is_processing {
        title_spans.push(ratatui::text::Span::styled(
            " ● Processing... ",
            Style::default().fg(Color::Green).bg(Color::DarkGray),
        ));
    } else if let Some(status) = &state.status_message {
        let style = if state.status_is_error {
            Style::default().fg(Color::Red).bg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Yellow).bg(Color::DarkGray)
        };
        title_spans.push(ratatui::text::Span::styled(format!(" {} ", status), style));
    }

    let title_line = ratatui::text::Line::from(title_spans);
    let widget = Paragraph::new(title_line).style(Style::default().bg(Color::DarkGray));
    frame.render_widget(widget, area);
}

pub(super) fn draw_chat_area(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    use super::super::widgets::message::render_messages;

    let buf = frame.buffer_mut();
    render_messages(&state.messages, area, buf, state.scroll_offset);

    // Welcome message if no messages
    if state.messages.is_empty() {
        use ratatui::widgets::Paragraph;

        let welcome = Paragraph::new(vec![
            ratatui::text::Line::from(""),
            ratatui::text::Line::from(ratatui::text::Span::styled(
                "Ted - The Coding Assistant you always wanted",
                Style::default().fg(Color::Cyan).bold(),
            )),
            ratatui::text::Line::from(""),
            ratatui::text::Line::from("Type a message and press Enter to chat."),
            ratatui::text::Line::from("Press Ctrl+/ for help, or /quit to exit."),
        ])
        .alignment(ratatui::layout::Alignment::Center);

        let welcome_area = ratatui::layout::Rect {
            x: area.x + area.width / 4,
            y: area.y + area.height / 3,
            width: area.width / 2,
            height: 6,
        };
        frame.render_widget(welcome, welcome_area);
    }
}

pub(super) fn draw_agent_pane(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    use super::super::widgets::AgentPane;

    let pane = AgentPane::new(&state.agents)
        .expanded(state.agent_pane_expanded)
        .focused(state.mode == ChatMode::AgentFocus);

    frame.render_widget(pane, area);
}

pub(super) fn draw_input_area(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    use super::super::widgets::InputArea;

    let focused = state.mode == ChatMode::Input;

    // When processing, allow typing but indicate messages will be queued
    let (placeholder, title_suffix) = if state.is_processing {
        let indicator = state.thinking_indicator();
        let queued_count = state.pending_messages.len();
        let queued_text = if queued_count > 0 {
            format!(" ({} queued)", queued_count)
        } else {
            String::new()
        };
        (
            format!("{} Processing... Type to queue a message", indicator),
            format!(" Processing{} ", queued_text),
        )
    } else {
        (
            "Type a message or /help for commands...".to_string(),
            String::new(),
        )
    };

    let mut widget = InputArea::new(&state.input)
        .focused(focused)
        .placeholder(&placeholder);

    // Visual indicator when processing
    if state.is_processing {
        widget = widget.processing(true, &title_suffix);
    }

    // Calculate cursor position BEFORE rendering (which consumes widget)
    let cursor_pos = if focused {
        Some(widget.cursor_position(area))
    } else {
        None
    };

    frame.render_widget(widget, area);

    // Position cursor
    if let Some(pos) = cursor_pos {
        frame.set_cursor_position(pos);
    }
}

pub(super) fn draw_help_overlay(frame: &mut Frame, area: ratatui::layout::Rect) {
    use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

    let popup_width = area.width * 60 / 100;
    let popup_height = area.height * 80 / 100;
    let popup_x = (area.width - popup_width) / 2;
    let popup_y = (area.height - popup_height) / 2;

    let popup_area = ratatui::layout::Rect {
        x: area.x + popup_x,
        y: area.y + popup_y,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    let help_text = vec![
        ratatui::text::Line::from(ratatui::text::Span::styled(
            " Ted Help ",
            Style::default().fg(Color::Cyan).bold(),
        )),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(ratatui::text::Span::styled(
            "Input Mode:",
            Style::default().bold(),
        )),
        ratatui::text::Line::from("  Enter       Send message"),
        ratatui::text::Line::from("  ↑/↓         History navigation"),
        ratatui::text::Line::from("  PgUp/PgDn   Scroll chat"),
        ratatui::text::Line::from("  Ctrl+↑/↓    Scroll one line"),
        ratatui::text::Line::from("  Tab         Toggle agent pane"),
        ratatui::text::Line::from("  Ctrl+/      Show this help"),
        ratatui::text::Line::from("  Ctrl+C      Cancel/Quit"),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(ratatui::text::Span::styled(
            "Scroll Mode (Esc):",
            Style::default().bold(),
        )),
        ratatui::text::Line::from("  j/k or ↑/↓  Scroll one line"),
        ratatui::text::Line::from("  g/G         Jump to top/bottom"),
        ratatui::text::Line::from("  ?           Show this help"),
        ratatui::text::Line::from("  Esc/i/Enter Back to input"),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(ratatui::text::Span::styled(
            "Commands:",
            Style::default().bold(),
        )),
        ratatui::text::Line::from("  /help       Show this help"),
        ratatui::text::Line::from("  /settings   Open settings (General & Caps)"),
        ratatui::text::Line::from("  /model X    Quick switch model"),
        ratatui::text::Line::from("  /agents     Toggle agent pane"),
        ratatui::text::Line::from("  /clear      Clear chat history"),
        ratatui::text::Line::from("  /quit       Exit Ted"),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(ratatui::text::Span::styled(
            "Press Esc to close",
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

    frame.render_widget(help, popup_area);
}

pub(super) fn draw_settings_overlay(
    frame: &mut Frame,
    state: &TuiState,
    area: ratatui::layout::Rect,
) {
    use ratatui::widgets::{Block, Borders, Clear, Paragraph};

    let popup_width = area.width.clamp(45, 65);
    // Clamp height to available space (leave at least 2 lines margin)
    let desired_height = 22_u16;
    let popup_height = desired_height.min(area.height.saturating_sub(2)).max(10);
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = ratatui::layout::Rect {
        x: area.x + popup_x,
        y: area.y + popup_y,
        width: popup_width.min(area.width.saturating_sub(area.x)),
        height: popup_height.min(area.height.saturating_sub(popup_y)),
    };

    frame.render_widget(Clear, popup_area);

    // Get settings state
    let settings = match &state.settings_state {
        Some(s) => s,
        None => return,
    };

    // Build lines
    let mut lines = Vec::new();

    // Tab bar
    let mut tab_spans = vec![ratatui::text::Span::raw("  ")];
    for section in SettingsSection::all() {
        let is_active = *section == settings.current_section;
        let style = if is_active {
            Style::default().fg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let prefix = if is_active { "▶ " } else { "  " };
        let suffix = if is_active { " ◀" } else { "  " };
        tab_spans.push(ratatui::text::Span::styled(prefix, style));
        tab_spans.push(ratatui::text::Span::styled(section.label(), style));
        tab_spans.push(ratatui::text::Span::styled(suffix, style));
        tab_spans.push(ratatui::text::Span::raw("  "));
    }
    lines.push(ratatui::text::Line::from(tab_spans));
    lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
        "─".repeat(popup_width.saturating_sub(2) as usize),
        Style::default().fg(Color::DarkGray),
    )));

    // Section content
    match settings.current_section {
        SettingsSection::General => {
            for (i, field) in SettingsField::all().iter().enumerate() {
                let is_selected = i == settings.selected_index;
                let is_editing = settings.is_editing && is_selected;

                let label = format!("{:12}", field.label());
                let value = if is_editing {
                    format!("{}▏", settings.edit_buffer)
                } else {
                    settings.current_value(*field)
                };

                let (label_style, value_style) = if is_selected {
                    (
                        Style::default().fg(Color::Cyan).bold(),
                        if is_editing {
                            Style::default().fg(Color::Yellow).bold()
                        } else {
                            Style::default().fg(Color::White).bold()
                        },
                    )
                } else {
                    (
                        Style::default().fg(Color::DarkGray),
                        Style::default().fg(Color::White),
                    )
                };

                let prefix = if is_selected { "▶ " } else { "  " };

                lines.push(ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled(prefix, Style::default().fg(Color::Cyan)),
                    ratatui::text::Span::styled(label, label_style),
                    ratatui::text::Span::raw(" "),
                    ratatui::text::Span::styled(value, value_style),
                ]));
            }
        }
        SettingsSection::Capabilities => {
            if settings.available_caps.is_empty() {
                lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                    "  No capabilities available",
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                // Show visible caps with scroll
                let visible_height = 10_usize; // Number of caps visible at once
                let total_caps = settings.available_caps.len();

                // Calculate scroll offset to keep selected item visible
                let scroll_offset = if settings.caps_selected_index >= visible_height {
                    settings.caps_selected_index - visible_height + 1
                } else {
                    0
                }
                .min(total_caps.saturating_sub(visible_height));

                for (i, (name, is_builtin)) in settings
                    .available_caps
                    .iter()
                    .enumerate()
                    .skip(scroll_offset)
                    .take(visible_height)
                {
                    let is_selected = i == settings.caps_selected_index;
                    let is_enabled = settings.caps_enabled.contains(name);

                    let checkbox = if is_enabled { "[✓]" } else { "[ ]" };
                    let builtin_tag = if *is_builtin { " (builtin)" } else { "" };

                    let style = if is_selected {
                        Style::default().fg(Color::Cyan).bold()
                    } else if is_enabled {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::White)
                    };

                    let prefix = if is_selected { "▶ " } else { "  " };

                    lines.push(ratatui::text::Line::from(vec![
                        ratatui::text::Span::styled(prefix, Style::default().fg(Color::Cyan)),
                        ratatui::text::Span::styled(checkbox, style),
                        ratatui::text::Span::raw(" "),
                        ratatui::text::Span::styled(name, style),
                        ratatui::text::Span::styled(
                            builtin_tag,
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                }

                // Show scroll indicator if needed
                if total_caps > visible_height {
                    let indicator = format!(
                        "  ({}-{} of {})",
                        scroll_offset + 1,
                        (scroll_offset + visible_height).min(total_caps),
                        total_caps
                    );
                    lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                        indicator,
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
        }
    }

    // Pad to consistent height (account for borders, help, save hint, and potential changes indicator)
    // popup_height includes borders (2), so inner content area is popup_height - 2
    // Reserve 4 lines at bottom for separator, help, save hint, and changes indicator
    let target_content_lines = popup_height.saturating_sub(6) as usize;
    while lines.len() < target_content_lines {
        lines.push(ratatui::text::Line::from(""));
    }

    // Separator
    lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
        "─".repeat(popup_width.saturating_sub(2) as usize),
        Style::default().fg(Color::DarkGray),
    )));

    // Help text
    let help_text = match settings.current_section {
        SettingsSection::General if settings.is_editing => "Enter: confirm │ Esc: cancel",
        SettingsSection::General => "Tab: section │ ↑/↓: nav │ ←/→: select │ Enter: edit/cycle",
        SettingsSection::Capabilities => "Tab: switch section │ ↑/↓: navigate │ Space: toggle",
    };
    lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
        help_text,
        Style::default().fg(Color::DarkGray),
    )));

    // Save hint
    lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
        "S: save │ Esc/q: close",
        Style::default().fg(Color::DarkGray),
    )));

    // Show change indicator
    if settings.has_changes {
        lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
            "* Unsaved changes",
            Style::default().fg(Color::Yellow),
        )));
    }

    let settings_widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Settings ")
            .title_style(Style::default().fg(Color::White).bold()),
    );

    frame.render_widget(settings_widget, popup_area);
}
