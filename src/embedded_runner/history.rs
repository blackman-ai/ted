// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use crate::embedded::HistoryMessageData;
use crate::llm::message::{ContentBlock, Message, MessageContent};

/// Simple message struct for history serialization.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct HistoryMessage {
    pub role: String,
    pub content: String,
}

/// Extract history messages from a list of Messages for persistence.
/// Filters out internal enforcement messages (those starting with "STOP!").
pub(super) fn extract_history_messages(messages: &[Message]) -> Vec<HistoryMessageData> {
    messages
        .iter()
        .filter_map(|msg| {
            let role = match msg.role {
                crate::llm::message::Role::User => "user",
                crate::llm::message::Role::Assistant => "assistant",
                crate::llm::message::Role::System => return None,
            };

            let text = match &msg.content {
                MessageContent::Text(text) => text.clone(),
                MessageContent::Blocks(blocks) => blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
            };

            // Skip empty messages.
            if text.is_empty() {
                return None;
            }

            // Skip internal enforcement messages injected to guide the model.
            if text.starts_with("STOP!") {
                return None;
            }

            Some(HistoryMessageData {
                role: role.to_string(),
                content: text,
            })
        })
        .collect()
}
