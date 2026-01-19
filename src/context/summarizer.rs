// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Conversation summarization for memory storage
//!
//! This module provides functionality to summarize conversations for storage
//! in the conversation memory system.

use crate::error::Result;
use crate::llm::message::{ContentBlock, Message, MessageContent, Role};
use crate::llm::provider::LlmProvider;

/// Generate a concise summary of a conversation
///
/// Returns a 2-3 sentence summary suitable for semantic search
pub async fn summarize_conversation(
    messages: &[Message],
    _provider: &dyn LlmProvider,
) -> Result<String> {
    // Extract key information from messages
    let mut content = String::new();

    for msg in messages {
        match msg.role {
            Role::User => {
                if let Some(text) = extract_text_content(msg) {
                    content.push_str("User: ");
                    content.push_str(&text);
                    content.push('\n');
                }
            }
            Role::Assistant => {
                if let Some(text) = extract_text_content(msg) {
                    content.push_str("Assistant: ");
                    content.push_str(&text);
                    content.push('\n');
                }
            }
            _ => {}
        }
    }

    // If conversation is short, just return it as-is
    if content.len() < 500 {
        return Ok(content.lines().take(3).collect::<Vec<_>>().join(" "));
    }

    // For now, extract the first few meaningful lines
    // In the future, we could use the LLM to generate a better summary
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    let summary = if lines.len() > 5 {
        format!(
            "{} {} {}",
            lines.first().unwrap_or(&""),
            lines.get(lines.len() / 2).unwrap_or(&""),
            lines.last().unwrap_or(&"")
        )
    } else {
        lines.join(" ")
    };

    Ok(summary.chars().take(300).collect())
}

/// Extract text content from a message
fn extract_text_content(message: &Message) -> Option<String> {
    let mut text = String::new();

    match &message.content {
        MessageContent::Text(t) => {
            text.push_str(t);
        }
        MessageContent::Blocks(blocks) => {
            for block in blocks {
                if let ContentBlock::Text { text: t } = block {
                    text.push_str(t);
                    text.push(' ');
                }
            }
        }
    }

    if text.is_empty() {
        None
    } else {
        Some(text.trim().to_string())
    }
}

/// Extract files changed from messages
pub fn extract_files_changed(messages: &[Message]) -> Vec<String> {
    use std::collections::HashSet;

    let mut files = HashSet::new();

    for msg in messages {
        if let MessageContent::Blocks(blocks) = &msg.content {
            for block in blocks {
                if let ContentBlock::ToolUse { name, input, .. } = block {
                    match name.as_str() {
                        "file_edit" | "file_write" => {
                            if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                                files.insert(path.to_string());
                            }
                        }
                        "propose_file_changes" => {
                            if let Some(ops) = input.get("operations").and_then(|v| v.as_array()) {
                                for op in ops {
                                    if let Some(path) = op.get("path").and_then(|v| v.as_str()) {
                                        files.insert(path.to_string());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    files.into_iter().collect()
}

/// Extract relevant tags from the conversation
pub fn extract_tags(messages: &[Message]) -> Vec<String> {
    use std::collections::HashSet;

    let mut tags = HashSet::new();
    let content = messages
        .iter()
        .filter_map(extract_text_content)
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    // Common programming topics as tags
    let topics = [
        ("authentication", "auth"),
        ("database", "database"),
        ("api", "api"),
        ("test", "testing"),
        ("bug", "bugfix"),
        ("refactor", "refactoring"),
        ("feature", "feature"),
        ("security", "security"),
        ("performance", "performance"),
        ("ui", "ui"),
        ("frontend", "frontend"),
        ("backend", "backend"),
    ];

    for (keyword, tag) in &topics {
        if content.contains(keyword) {
            tags.insert(tag.to_string());
        }
    }

    // Limit to 5 most relevant tags
    tags.into_iter().take(5).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_message(role: Role, text: &str) -> Message {
        Message {
            id: Uuid::new_v4(),
            role,
            content: MessageContent::Text(text.to_string()),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        }
    }

    #[test]
    fn test_extract_text_content() {
        let msg = create_test_message(Role::User, "Hello, world!");
        let text = extract_text_content(&msg);

        assert!(text.is_some());
        assert_eq!(text.unwrap(), "Hello, world!");
    }

    #[test]
    fn test_extract_files_changed() {
        let msg = Message {
            id: Uuid::new_v4(),
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "tool_1".to_string(),
                name: "file_edit".to_string(),
                input: serde_json::json!({
                    "path": "src/main.rs",
                    "old_string": "old",
                    "new_string": "new"
                }),
            }]),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        };

        let files = extract_files_changed(&[msg]);
        assert_eq!(files.len(), 1);
        assert!(files.contains(&"src/main.rs".to_string()));
    }

    #[test]
    fn test_extract_tags() {
        let messages = vec![
            create_test_message(Role::User, "I need to add authentication to my API"),
            create_test_message(Role::Assistant, "I'll help you implement JWT authentication"),
        ];

        let tags = extract_tags(&messages);
        assert!(tags.contains(&"auth".to_string()) || tags.contains(&"api".to_string()));
    }

    #[test]
    fn test_extract_tags_limits() {
        let messages = vec![create_test_message(
            Role::User,
            "authentication database api test bug refactor feature security performance ui",
        )];

        let tags = extract_tags(&messages);
        assert!(tags.len() <= 5);
    }
}
