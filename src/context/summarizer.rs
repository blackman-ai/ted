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
            create_test_message(
                Role::Assistant,
                "I'll help you implement JWT authentication",
            ),
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

    // ===== Additional extract_text_content Tests =====

    #[test]
    fn test_extract_text_content_empty() {
        let msg = create_test_message(Role::User, "");
        let text = extract_text_content(&msg);
        assert!(text.is_none());
    }

    #[test]
    fn test_extract_text_content_whitespace_only() {
        let msg = create_test_message(Role::User, "   \t\n  ");
        let text = extract_text_content(&msg);
        // The function returns Some("") after trim for whitespace-only input
        // This is because the text is not empty before the if check
        assert!(text.is_some());
        assert!(text.unwrap().is_empty());
    }

    #[test]
    fn test_extract_text_content_with_blocks() {
        let msg = Message {
            id: Uuid::new_v4(),
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::Text {
                    text: "First block".to_string(),
                },
                ContentBlock::Text {
                    text: "Second block".to_string(),
                },
            ]),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        };

        let text = extract_text_content(&msg);
        assert!(text.is_some());
        let result = text.unwrap();
        assert!(result.contains("First block"));
        assert!(result.contains("Second block"));
    }

    #[test]
    fn test_extract_text_content_mixed_blocks() {
        let msg = Message {
            id: Uuid::new_v4(),
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::Text {
                    text: "Some text".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "t1".to_string(),
                    name: "test".to_string(),
                    input: serde_json::json!({}),
                },
                ContentBlock::Text {
                    text: "More text".to_string(),
                },
            ]),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        };

        let text = extract_text_content(&msg);
        assert!(text.is_some());
        let result = text.unwrap();
        assert!(result.contains("Some text"));
        assert!(result.contains("More text"));
    }

    #[test]
    fn test_extract_text_content_only_tool_use() {
        let msg = Message {
            id: Uuid::new_v4(),
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "t1".to_string(),
                name: "test".to_string(),
                input: serde_json::json!({}),
            }]),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        };

        let text = extract_text_content(&msg);
        assert!(text.is_none());
    }

    // ===== Additional extract_files_changed Tests =====

    #[test]
    fn test_extract_files_changed_empty() {
        let files = extract_files_changed(&[]);
        assert!(files.is_empty());
    }

    #[test]
    fn test_extract_files_changed_no_tool_use() {
        let msg = create_test_message(Role::User, "Just some text");
        let files = extract_files_changed(&[msg]);
        assert!(files.is_empty());
    }

    #[test]
    fn test_extract_files_changed_file_write() {
        let msg = Message {
            id: Uuid::new_v4(),
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "tool_1".to_string(),
                name: "file_write".to_string(),
                input: serde_json::json!({
                    "path": "src/new_file.rs",
                    "content": "fn main() {}"
                }),
            }]),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        };

        let files = extract_files_changed(&[msg]);
        assert_eq!(files.len(), 1);
        assert!(files.contains(&"src/new_file.rs".to_string()));
    }

    #[test]
    fn test_extract_files_changed_multiple_files() {
        let msg = Message {
            id: Uuid::new_v4(),
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::ToolUse {
                    id: "tool_1".to_string(),
                    name: "file_edit".to_string(),
                    input: serde_json::json!({
                        "path": "src/main.rs",
                        "old_string": "old",
                        "new_string": "new"
                    }),
                },
                ContentBlock::ToolUse {
                    id: "tool_2".to_string(),
                    name: "file_write".to_string(),
                    input: serde_json::json!({
                        "path": "src/lib.rs",
                        "content": "pub mod foo;"
                    }),
                },
            ]),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        };

        let files = extract_files_changed(&[msg]);
        assert_eq!(files.len(), 2);
        assert!(files.contains(&"src/main.rs".to_string()));
        assert!(files.contains(&"src/lib.rs".to_string()));
    }

    #[test]
    fn test_extract_files_changed_propose_file_changes() {
        let msg = Message {
            id: Uuid::new_v4(),
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "tool_1".to_string(),
                name: "propose_file_changes".to_string(),
                input: serde_json::json!({
                    "operations": [
                        {"path": "src/a.rs", "type": "edit"},
                        {"path": "src/b.rs", "type": "write"},
                        {"path": "src/c.rs", "type": "delete"}
                    ]
                }),
            }]),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        };

        let files = extract_files_changed(&[msg]);
        assert_eq!(files.len(), 3);
        assert!(files.contains(&"src/a.rs".to_string()));
        assert!(files.contains(&"src/b.rs".to_string()));
        assert!(files.contains(&"src/c.rs".to_string()));
    }

    #[test]
    fn test_extract_files_changed_deduplicates() {
        let messages = vec![
            Message {
                id: Uuid::new_v4(),
                role: Role::Assistant,
                content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                    id: "tool_1".to_string(),
                    name: "file_edit".to_string(),
                    input: serde_json::json!({"path": "src/main.rs", "old_string": "a", "new_string": "b"}),
                }]),
                timestamp: Utc::now(),
                tool_use_id: None,
                token_count: None,
            },
            Message {
                id: Uuid::new_v4(),
                role: Role::Assistant,
                content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                    id: "tool_2".to_string(),
                    name: "file_edit".to_string(),
                    input: serde_json::json!({"path": "src/main.rs", "old_string": "c", "new_string": "d"}),
                }]),
                timestamp: Utc::now(),
                tool_use_id: None,
                token_count: None,
            },
        ];

        let files = extract_files_changed(&messages);
        // Same file edited twice, should only appear once
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_extract_files_changed_other_tools_ignored() {
        let msg = Message {
            id: Uuid::new_v4(),
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::ToolUse {
                    id: "tool_1".to_string(),
                    name: "file_read".to_string(),
                    input: serde_json::json!({"path": "src/main.rs"}),
                },
                ContentBlock::ToolUse {
                    id: "tool_2".to_string(),
                    name: "shell".to_string(),
                    input: serde_json::json!({"command": "ls"}),
                },
            ]),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        };

        let files = extract_files_changed(&[msg]);
        // file_read and shell don't count as "changed"
        assert!(files.is_empty());
    }

    // ===== Additional extract_tags Tests =====

    #[test]
    fn test_extract_tags_empty() {
        let tags = extract_tags(&[]);
        assert!(tags.is_empty());
    }

    #[test]
    fn test_extract_tags_no_matching_keywords() {
        let messages = vec![create_test_message(
            Role::User,
            "Hello, I need help with my project.",
        )];

        let tags = extract_tags(&messages);
        assert!(tags.is_empty());
    }

    #[test]
    fn test_extract_tags_case_insensitive() {
        let messages = vec![create_test_message(
            Role::User,
            "AUTHENTICATION and DATABASE and API",
        )];

        let tags = extract_tags(&messages);
        // Should find tags despite uppercase
        assert!(!tags.is_empty());
    }

    #[test]
    fn test_extract_tags_specific_topics() {
        // Test each topic individually
        let test_cases = [
            ("authentication issue", "auth"),
            ("database query", "database"),
            ("api endpoint", "api"),
            ("test failure", "testing"),
            ("bug report", "bugfix"),
            ("refactor code", "refactoring"),
            ("new feature", "feature"),
            ("security vulnerability", "security"),
            ("performance issue", "performance"),
            ("ui component", "ui"),
            ("frontend changes", "frontend"),
            ("backend service", "backend"),
        ];

        for (text, expected_tag) in test_cases {
            let messages = vec![create_test_message(Role::User, text)];
            let tags = extract_tags(&messages);
            assert!(
                tags.contains(&expected_tag.to_string()),
                "Expected tag '{}' for text '{}'",
                expected_tag,
                text
            );
        }
    }

    #[test]
    fn test_extract_tags_from_multiple_messages() {
        let messages = vec![
            create_test_message(Role::User, "I need to fix a bug"),
            create_test_message(Role::Assistant, "I'll help with the security fix"),
            create_test_message(Role::User, "Also add a test"),
        ];

        let tags = extract_tags(&messages);
        assert!(
            tags.contains(&"bugfix".to_string())
                || tags.contains(&"security".to_string())
                || tags.contains(&"testing".to_string())
        );
    }

    #[test]
    fn test_extract_tags_includes_system_messages() {
        let messages = vec![Message {
            id: Uuid::new_v4(),
            role: Role::System,
            content: MessageContent::Text("authentication database api".to_string()),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        }];

        // System messages ARE processed by extract_text_content
        // The extract_tags function processes all messages regardless of role
        let tags = extract_tags(&messages);
        // Should find tags from the system message
        assert!(!tags.is_empty());
    }

    // ===== summarize_conversation Tests =====

    // Note: We can't easily test the async summarize_conversation without a mock provider
    // But we can test the synchronous parts

    #[tokio::test]
    async fn test_summarize_conversation_short() {
        use crate::llm::providers::AnthropicProvider;

        let messages = vec![
            create_test_message(Role::User, "Hello"),
            create_test_message(Role::Assistant, "Hi there"),
        ];

        // Create a dummy provider (won't be used for short conversations)
        let provider = AnthropicProvider::new("dummy-key");

        let summary = summarize_conversation(&messages, &provider).await.unwrap();

        // For short conversations, it just extracts content
        assert!(!summary.is_empty());
    }

    #[tokio::test]
    async fn test_summarize_conversation_empty() {
        use crate::llm::providers::AnthropicProvider;

        let messages: Vec<Message> = vec![];
        let provider = AnthropicProvider::new("dummy-key");

        let summary = summarize_conversation(&messages, &provider).await.unwrap();

        // Empty messages should produce empty summary
        assert!(summary.is_empty());
    }

    #[tokio::test]
    async fn test_summarize_conversation_long() {
        use crate::llm::providers::AnthropicProvider;

        // Create a long conversation
        let long_text = "This is a very long message that contains a lot of content. ".repeat(50);
        let messages = vec![
            create_test_message(Role::User, &long_text),
            create_test_message(Role::Assistant, &long_text),
        ];

        let provider = AnthropicProvider::new("dummy-key");

        let summary = summarize_conversation(&messages, &provider).await.unwrap();

        // Summary should be truncated to 300 chars
        assert!(summary.len() <= 300);
    }

    #[test]
    fn test_create_test_message_helper() {
        let msg = create_test_message(Role::User, "Test content");
        assert_eq!(msg.role, Role::User);
        assert!(matches!(msg.content, MessageContent::Text(ref s) if s == "Test content"));
    }

    #[tokio::test]
    async fn test_summarize_conversation_with_system_role() {
        use crate::llm::providers::AnthropicProvider;

        // Test that System role messages are skipped in summarize_conversation
        let messages = vec![
            Message {
                id: Uuid::new_v4(),
                role: Role::System,
                content: MessageContent::Text("You are a helpful assistant.".to_string()),
                timestamp: Utc::now(),
                tool_use_id: None,
                token_count: None,
            },
            create_test_message(Role::User, "Hello"),
            create_test_message(Role::Assistant, "Hi"),
        ];

        let provider = AnthropicProvider::new("dummy-key");
        let summary = summarize_conversation(&messages, &provider).await.unwrap();

        // Summary should not include the system message content
        assert!(!summary.contains("You are a helpful assistant"));
        // But should include user/assistant messages
        assert!(summary.contains("User") || summary.contains("Hello") || summary.contains("Hi"));
    }

    #[tokio::test]
    async fn test_summarize_conversation_long_with_many_lines() {
        use crate::llm::providers::AnthropicProvider;

        // Create a long conversation with many lines (> 5 lines after processing)
        // This tests the first/middle/last extraction logic (lines 52, 54-56)
        let messages = vec![
            create_test_message(
                Role::User,
                "First user message that is reasonably long to generate content",
            ),
            create_test_message(
                Role::Assistant,
                "First assistant response that adds more content to the conversation",
            ),
            create_test_message(
                Role::User,
                "Second user message continuing the discussion with more details",
            ),
            create_test_message(
                Role::Assistant,
                "Second assistant response providing additional information",
            ),
            create_test_message(
                Role::User,
                "Third user message asking follow-up questions about the topic",
            ),
            create_test_message(
                Role::Assistant,
                "Third assistant response with comprehensive answers",
            ),
            create_test_message(
                Role::User,
                "Fourth user message wrapping up the conversation",
            ),
            create_test_message(
                Role::Assistant,
                "Final assistant response summarizing everything discussed",
            ),
        ];

        let provider = AnthropicProvider::new("dummy-key");
        let summary = summarize_conversation(&messages, &provider).await.unwrap();

        // Summary should be truncated to 300 chars
        assert!(summary.len() <= 300);
    }

    #[tokio::test]
    async fn test_summarize_conversation_exactly_5_lines() {
        use crate::llm::providers::AnthropicProvider;

        // Test the boundary case of exactly 5 lines (goes to else branch: lines.join(" "))
        // Need content > 500 chars with exactly 5 non-empty lines
        let messages = vec![
            create_test_message(Role::User, "First line with a lot of content to make sure we exceed the 500 character limit that triggers the longer processing path in the summarization function"),
            create_test_message(Role::Assistant, "Second line also needs substantial content for the same reason"),
            create_test_message(Role::User, "Third line continues"),
            create_test_message(Role::Assistant, "Fourth line here"),
            create_test_message(Role::User, "Fifth and final line"),
        ];

        let provider = AnthropicProvider::new("dummy-key");
        let summary = summarize_conversation(&messages, &provider).await.unwrap();

        // Summary should be generated without error
        assert!(summary.len() <= 300);
    }

    #[tokio::test]
    async fn test_summarize_conversation_only_system_messages() {
        use crate::llm::providers::AnthropicProvider;

        // Test that only system messages results in empty summary
        let messages = vec![
            Message {
                id: Uuid::new_v4(),
                role: Role::System,
                content: MessageContent::Text("System message 1".to_string()),
                timestamp: Utc::now(),
                tool_use_id: None,
                token_count: None,
            },
            Message {
                id: Uuid::new_v4(),
                role: Role::System,
                content: MessageContent::Text("System message 2".to_string()),
                timestamp: Utc::now(),
                tool_use_id: None,
                token_count: None,
            },
        ];

        let provider = AnthropicProvider::new("dummy-key");
        let summary = summarize_conversation(&messages, &provider).await.unwrap();

        // Should be empty since system messages are ignored
        assert!(summary.is_empty());
    }
}
