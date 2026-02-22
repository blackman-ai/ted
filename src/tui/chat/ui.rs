// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Main UI rendering for the chat TUI

use ratatui::prelude::*;

use super::app::{ChatApp, ChatMode};
use super::widgets::InputArea;

mod layout;
mod overlay;
mod view;

use layout::calculate_layout;
#[cfg(test)]
use layout::Layout;
#[cfg(test)]
use overlay::centered_rect;
use overlay::{render_confirm_dialog, render_help_overlay};
use view::{render_agent_pane, render_chat_area, render_input_area, render_title_bar};

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

#[cfg(test)]
mod tests;
