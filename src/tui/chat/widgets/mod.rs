// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! UI widgets for the chat TUI

pub mod agent_pane;
pub mod input_area;
pub mod message;
pub mod status_bar;

pub use agent_pane::AgentPane;
pub use input_area::InputArea;
pub use message::MessageWidget;
pub use status_bar::StatusBar;
