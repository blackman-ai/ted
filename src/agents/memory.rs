// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Memory management strategies for subagents
//!
//! This module implements different strategies for managing conversation
//! context within subagents to balance between context retention and
//! token budget constraints.

use crate::error::Result;
use crate::llm::message::{Conversation, Message, Role};

use super::types::MemoryStrategy;

/// Apply a memory strategy to a conversation
pub fn apply_memory_strategy(
    conversation: &mut Conversation,
    strategy: &MemoryStrategy,
) -> Result<MemoryAction> {
    match strategy {
        MemoryStrategy::Full => apply_full_strategy(conversation),
        MemoryStrategy::Summarizing { threshold, target } => {
            apply_summarizing_strategy(conversation, *threshold, *target)
        }
        MemoryStrategy::Windowed { window_size } => {
            apply_windowed_strategy(conversation, *window_size)
        }
    }
}

/// Action to take after applying a memory strategy
#[derive(Debug, Clone)]
pub enum MemoryAction {
    /// No action needed, conversation is within limits
    None,
    /// Messages were trimmed from the conversation
    Trimmed { count: usize },
    /// Conversation needs summarization (returns messages to summarize)
    NeedsSummarization { messages: Vec<Message> },
}

/// Full strategy: keep all messages, only trim if absolutely necessary
fn apply_full_strategy(_conversation: &mut Conversation) -> Result<MemoryAction> {
    // Full strategy doesn't proactively trim
    // The caller handles hard limits via token budget
    Ok(MemoryAction::None)
}

/// Summarizing strategy: trigger summarization when threshold is exceeded
fn apply_summarizing_strategy(
    conversation: &mut Conversation,
    threshold: u32,
    target: u32,
) -> Result<MemoryAction> {
    let current_tokens = conversation.estimate_tokens();

    if current_tokens <= threshold {
        return Ok(MemoryAction::None);
    }

    // Find messages to summarize (older messages, keeping recent ones)
    let tokens_to_remove = current_tokens.saturating_sub(target);
    let mut removed_tokens = 0_u32;
    let mut messages_to_summarize = Vec::new();

    // Start from the beginning (oldest messages) but skip system messages
    let mut indices_to_remove = Vec::new();

    for (i, msg) in conversation.messages.iter().enumerate() {
        if msg.role == Role::System {
            continue; // Never summarize system messages
        }

        let msg_tokens = msg.estimate_tokens();

        // Stop if we've found enough to summarize
        if removed_tokens >= tokens_to_remove {
            break;
        }

        // Keep track of messages to summarize
        messages_to_summarize.push(msg.clone());
        indices_to_remove.push(i);
        removed_tokens += msg_tokens;
    }

    if messages_to_summarize.is_empty() {
        return Ok(MemoryAction::None);
    }

    // Remove the messages (in reverse order to maintain indices)
    for i in indices_to_remove.into_iter().rev() {
        conversation.messages.remove(i);
    }

    Ok(MemoryAction::NeedsSummarization {
        messages: messages_to_summarize,
    })
}

/// Windowed strategy: keep only the most recent N messages
fn apply_windowed_strategy(
    conversation: &mut Conversation,
    window_size: usize,
) -> Result<MemoryAction> {
    if conversation.messages.len() <= window_size {
        return Ok(MemoryAction::None);
    }

    let _to_remove = conversation.messages.len() - window_size;

    // Find first non-system message to start removal
    // Keep system messages at the beginning
    let mut system_count = 0;
    for msg in conversation.messages.iter() {
        if msg.role == Role::System {
            system_count += 1;
        } else {
            break;
        }
    }

    // Calculate how many non-system messages to remove
    let non_system_count = conversation.messages.len() - system_count;
    if non_system_count <= window_size {
        return Ok(MemoryAction::None);
    }

    let remove_count = non_system_count - window_size;

    // Remove messages starting after system messages
    conversation
        .messages
        .drain(system_count..system_count + remove_count);

    Ok(MemoryAction::Trimmed {
        count: remove_count,
    })
}

/// Generate a summary prompt for messages that need summarization
pub fn create_summary_prompt(messages: &[Message]) -> String {
    let mut content = String::from(
        "Summarize the following conversation exchanges concisely, \
         focusing on key decisions, findings, and actions taken:\n\n",
    );

    for msg in messages {
        let role = match msg.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::System => continue, // Skip system messages
        };

        if let Some(text) = msg.text() {
            content.push_str(&format!("{}: {}\n\n", role, text));
        }
    }

    content.push_str("\nProvide a concise summary (2-3 sentences) of the above exchanges:");

    content
}

/// Insert a summary into the conversation at the appropriate position
pub fn insert_summary(conversation: &mut Conversation, summary: &str) {
    // Find the position after system messages
    let mut insert_pos = 0;
    for (i, msg) in conversation.messages.iter().enumerate() {
        if msg.role != Role::System {
            insert_pos = i;
            break;
        }
        insert_pos = i + 1;
    }

    // Create a summary message as a system message
    let summary_msg = Message::system(format!("[Previous conversation summary]\n{}", summary));

    conversation.messages.insert(insert_pos, summary_msg);
}

/// Compact a conversation to fit within a token budget
///
/// This is a fallback when other strategies aren't enough.
/// It aggressively removes older messages to fit.
pub fn compact_to_budget(conversation: &mut Conversation, max_tokens: u32) -> usize {
    let mut removed = 0;
    let mut current_tokens = conversation.estimate_tokens();

    // Keep removing oldest non-system messages until we're under budget
    while current_tokens > max_tokens && !conversation.messages.is_empty() {
        // Find first non-system message
        let mut found = None;
        for (i, msg) in conversation.messages.iter().enumerate() {
            if msg.role != Role::System {
                found = Some(i);
                break;
            }
        }

        if let Some(i) = found {
            let msg_tokens = conversation.messages[i].estimate_tokens();
            conversation.messages.remove(i);
            current_tokens = current_tokens.saturating_sub(msg_tokens);
            removed += 1;
        } else {
            // Only system messages left
            break;
        }
    }

    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_conversation(message_count: usize) -> Conversation {
        let mut conv = Conversation::with_system("You are a test agent.");
        for i in 0..message_count {
            if i % 2 == 0 {
                conv.push(Message::user(format!("Message {}", i)));
            } else {
                conv.push(Message::assistant(format!("Response {}", i)));
            }
        }
        conv
    }

    #[test]
    fn test_full_strategy_no_action() {
        let mut conv = create_test_conversation(10);
        let result = apply_memory_strategy(&mut conv, &MemoryStrategy::Full).unwrap();

        assert!(matches!(result, MemoryAction::None));
        assert_eq!(conv.messages.len(), 10);
    }

    #[test]
    fn test_windowed_strategy_under_limit() {
        let mut conv = create_test_conversation(5);
        let result =
            apply_memory_strategy(&mut conv, &MemoryStrategy::Windowed { window_size: 10 })
                .unwrap();

        assert!(matches!(result, MemoryAction::None));
        assert_eq!(conv.messages.len(), 5);
    }

    #[test]
    fn test_windowed_strategy_over_limit() {
        let mut conv = create_test_conversation(10);
        let result =
            apply_memory_strategy(&mut conv, &MemoryStrategy::Windowed { window_size: 5 }).unwrap();

        if let MemoryAction::Trimmed { count } = result {
            assert_eq!(count, 5);
        } else {
            panic!("Expected Trimmed action");
        }
        assert_eq!(conv.messages.len(), 5);
    }

    #[test]
    fn test_summarizing_strategy_under_threshold() {
        let mut conv = create_test_conversation(3);
        let result = apply_memory_strategy(
            &mut conv,
            &MemoryStrategy::Summarizing {
                threshold: 100_000,
                target: 50_000,
            },
        )
        .unwrap();

        assert!(matches!(result, MemoryAction::None));
    }

    #[test]
    fn test_summarizing_strategy_over_threshold() {
        // Create a conversation with enough content to exceed threshold
        let mut conv = Conversation::with_system("Test");
        for i in 0..100 {
            // Add long messages to exceed threshold
            conv.push(Message::user(format!(
                "This is a very long message {} {}",
                i,
                "x".repeat(1000)
            )));
            conv.push(Message::assistant(format!(
                "This is a very long response {} {}",
                i,
                "y".repeat(1000)
            )));
        }

        let initial_count = conv.messages.len();

        let result = apply_memory_strategy(
            &mut conv,
            &MemoryStrategy::Summarizing {
                threshold: 10_000,
                target: 5_000,
            },
        )
        .unwrap();

        match result {
            MemoryAction::NeedsSummarization { messages } => {
                assert!(!messages.is_empty());
                assert!(conv.messages.len() < initial_count);
            }
            _ => panic!("Expected NeedsSummarization action"),
        }
    }

    #[test]
    fn test_create_summary_prompt() {
        let messages = vec![
            Message::user("Find the auth files"),
            Message::assistant("I found 5 auth files in src/auth/"),
        ];

        let prompt = create_summary_prompt(&messages);

        assert!(prompt.contains("Find the auth files"));
        assert!(prompt.contains("I found 5 auth files"));
        assert!(prompt.contains("Summarize"));
    }

    #[test]
    fn test_insert_summary() {
        let mut conv = Conversation::with_system("System prompt");
        conv.push(Message::user("Hello"));
        conv.push(Message::assistant("Hi"));

        insert_summary(&mut conv, "Previous: user greeted assistant");

        // Summary should be after system messages, before user messages
        assert_eq!(conv.messages.len(), 3);
        assert_eq!(conv.messages[0].role, Role::System);
    }

    #[test]
    fn test_compact_to_budget() {
        let mut conv = create_test_conversation(20);
        let initial_len = conv.messages.len();

        // Set a very low budget to force compaction
        let removed = compact_to_budget(&mut conv, 100);

        assert!(removed > 0);
        assert!(conv.messages.len() < initial_len);
    }

    #[test]
    fn test_compact_preserves_system_messages() {
        let mut conv = Conversation::with_system("Important system prompt");
        for i in 0..5 {
            conv.push(Message::user(format!("Message {}", i)));
        }

        // The system prompt should be preserved
        let initial_system_prompt = conv.system_prompt.clone();

        compact_to_budget(&mut conv, 10);

        assert_eq!(conv.system_prompt, initial_system_prompt);
    }

    #[test]
    fn test_windowed_preserves_system_messages() {
        let mut conv = Conversation::with_system("System");
        // Add a system message in the conversation
        conv.messages
            .insert(0, Message::system("Initial instruction"));
        for i in 0..10 {
            conv.push(Message::user(format!("Msg {}", i)));
        }

        let result =
            apply_memory_strategy(&mut conv, &MemoryStrategy::Windowed { window_size: 3 }).unwrap();

        // System message should be preserved
        assert_eq!(conv.messages[0].role, Role::System);

        if let MemoryAction::Trimmed { count } = result {
            assert!(count > 0);
        }
    }
}
