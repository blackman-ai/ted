// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use ratatui::prelude::*;

use super::super::app::ChatApp;

/// Layout regions.
#[derive(Clone, Copy, Debug)]
pub(super) struct Layout {
    pub(super) title_bar: Rect,
    pub(super) chat: Rect,
    pub(super) agents: Rect,
    pub(super) input: Rect,
}

pub(super) fn calculate_layout(area: Rect, app: &ChatApp) -> Layout {
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
