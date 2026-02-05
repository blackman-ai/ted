// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! State management for the chat TUI

pub mod agents;
pub mod input;
pub mod messages;
pub mod scroll;

pub use agents::{AgentStatus, AgentTracker, TrackedAgent};
pub use input::InputState;
pub use messages::{truncate_string, DisplayMessage, DisplayToolCall, MessageRole, ToolCallStatus};
pub use scroll::ScrollState;
