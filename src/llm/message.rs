// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Message types for LLM interactions
//!
//! Defines the message structures used to communicate with LLMs.

use crate::config::settings::ConversationConfig;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique identifier for the message
    pub id: Uuid,

    /// Role of the message sender
    pub role: Role,

    /// Content of the message
    pub content: MessageContent,

    /// When the message was created
    pub timestamp: DateTime<Utc>,

    /// Tool use ID if this is a tool result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,

    /// Estimated token count (if calculated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<u32>,
}

/// Role of the message sender
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// User message
    User,
    /// Assistant response
    Assistant,
    /// System prompt
    System,
}

/// Content of a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple text content
    Text(String),
    /// Multiple content blocks (text, tool use, tool result)
    Blocks(Vec<ContentBlock>),
}

/// A block of content within a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text content
    Text { text: String },

    /// Tool use request from assistant
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// Tool result from user
    ToolResult {
        tool_use_id: String,
        content: ToolResultContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// Content of a tool result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    /// Simple text result
    Text(String),
    /// Multiple content blocks
    Blocks(Vec<ToolResultBlock>),
}

/// A block within a tool result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolResultBlock {
    /// Text content
    Text { text: String },
    /// Image content (base64)
    Image { source: ImageSource },
}

/// Source of an image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String, // "base64"
    pub media_type: String, // "image/png", "image/jpeg", etc.
    pub data: String,       // base64 encoded
}

impl Message {
    /// Create a new user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: Role::User,
            content: MessageContent::Text(content.into()),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        }
    }

    /// Create a new assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: Role::Assistant,
            content: MessageContent::Text(content.into()),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        }
    }

    /// Create a new system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: Role::System,
            content: MessageContent::Text(content.into()),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        }
    }

    /// Create a new assistant message with content blocks
    pub fn assistant_blocks(blocks: Vec<ContentBlock>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: Role::Assistant,
            content: MessageContent::Blocks(blocks),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        }
    }

    /// Create a tool result message
    pub fn tool_result(
        tool_use_id: impl Into<String>,
        content: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: Role::User,
            content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.into(),
                content: ToolResultContent::Text(content.into()),
                is_error: if is_error { Some(true) } else { None },
            }]),
            timestamp: Utc::now(),
            tool_use_id: None,
            token_count: None,
        }
    }

    /// Get the text content of the message (if it's a simple text message)
    pub fn text(&self) -> Option<&str> {
        match &self.content {
            MessageContent::Text(text) => Some(text),
            MessageContent::Blocks(blocks) => {
                // Return first text block
                blocks.iter().find_map(|block| {
                    if let ContentBlock::Text { text } = block {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
            }
        }
    }

    /// Get all tool use blocks from the message
    pub fn tool_uses(&self) -> Vec<&ContentBlock> {
        match &self.content {
            MessageContent::Text(_) => vec![],
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .filter(|block| matches!(block, ContentBlock::ToolUse { .. }))
                .collect(),
        }
    }

    /// Check if message has any tool use
    pub fn has_tool_use(&self) -> bool {
        !self.tool_uses().is_empty()
    }
}

impl MessageContent {
    /// Convert content to blocks format
    pub fn into_blocks(self) -> Vec<ContentBlock> {
        match self {
            MessageContent::Text(text) => vec![ContentBlock::Text { text }],
            MessageContent::Blocks(blocks) => blocks,
        }
    }

    /// Get as text if it's a simple text content
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(text) => Some(text),
            MessageContent::Blocks(_) => None,
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::System => write!(f, "system"),
        }
    }
}

/// Conversation history
#[derive(Debug, Clone, Default)]
pub struct Conversation {
    /// All messages in the conversation
    pub messages: Vec<Message>,

    /// System prompt (if any)
    pub system_prompt: Option<String>,

    /// Token estimation configuration
    config: ConversationConfig,
}

impl Conversation {
    /// Create a new empty conversation
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a conversation with custom configuration
    pub fn with_config(config: ConversationConfig) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: None,
            config,
        }
    }

    /// Create a conversation with a system prompt
    pub fn with_system(system_prompt: impl Into<String>) -> Self {
        Self {
            messages: vec![],
            system_prompt: Some(system_prompt.into()),
            config: ConversationConfig::default(),
        }
    }

    /// Create a conversation with both a system prompt and config
    pub fn with_system_and_config(
        system_prompt: impl Into<String>,
        config: ConversationConfig,
    ) -> Self {
        Self {
            messages: vec![],
            system_prompt: Some(system_prompt.into()),
            config,
        }
    }

    /// Get the conversation config
    pub fn config(&self) -> &ConversationConfig {
        &self.config
    }

    /// Set the system prompt
    pub fn set_system(&mut self, system_prompt: impl Into<String>) {
        self.system_prompt = Some(system_prompt.into());
    }

    /// Add a message to the conversation
    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Get the last message
    pub fn last(&self) -> Option<&Message> {
        self.messages.last()
    }

    /// Get the last assistant message
    pub fn last_assistant(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == Role::Assistant)
    }

    /// Check if the conversation is empty
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get message count
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Clear all messages
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Estimate the total token count for the conversation
    /// Uses a configurable heuristic of characters per token (default: 4)
    pub fn estimate_tokens(&self) -> u32 {
        let chars_per_token = self.config.chars_per_token as usize;
        let system_tokens = self
            .system_prompt
            .as_ref()
            .map(|s| (s.len() / chars_per_token) as u32)
            .unwrap_or(0);

        let message_tokens: u32 = self
            .messages
            .iter()
            .map(|m| m.estimate_tokens_with_config(&self.config))
            .sum();

        system_tokens + message_tokens
    }

    /// Get a truncated version of the conversation that fits within the token limit
    /// Keeps the system prompt and most recent messages, dropping older ones
    /// Returns (truncated_messages, was_truncated)
    pub fn truncate_to_fit(&self, max_tokens: u32) -> (Vec<Message>, bool) {
        let chars_per_token = self.config.chars_per_token as usize;
        let system_tokens = self
            .system_prompt
            .as_ref()
            .map(|s| (s.len() / chars_per_token) as u32)
            .unwrap_or(0);

        // Reserve space for system prompt and buffer for response
        let available_for_messages = max_tokens
            .saturating_sub(system_tokens)
            .saturating_sub(self.config.response_buffer_tokens);

        if available_for_messages == 0 {
            return (vec![], true);
        }

        // Work backwards from most recent messages
        let mut kept_messages: Vec<Message> = Vec::new();
        let mut total_tokens = 0_u32;

        for message in self.messages.iter().rev() {
            let msg_tokens = message.estimate_tokens_with_config(&self.config);
            if total_tokens + msg_tokens > available_for_messages {
                break;
            }
            total_tokens += msg_tokens;
            kept_messages.push(message.clone());
        }

        // Reverse to get chronological order
        kept_messages.reverse();

        let was_truncated = kept_messages.len() < self.messages.len();
        (kept_messages, was_truncated)
    }

    /// Trim the conversation in-place to fit within the token limit
    /// Returns the number of messages removed
    pub fn trim_to_fit(&mut self, max_tokens: u32) -> usize {
        let chars_per_token = self.config.chars_per_token as usize;
        let system_tokens = self
            .system_prompt
            .as_ref()
            .map(|s| (s.len() / chars_per_token) as u32)
            .unwrap_or(0);

        // Reserve space for system prompt and buffer for response
        let available_for_messages = max_tokens
            .saturating_sub(system_tokens)
            .saturating_sub(self.config.response_buffer_tokens);

        if available_for_messages == 0 {
            let removed = self.messages.len();
            self.messages.clear();
            return removed;
        }

        // Calculate how many messages we can keep from the end
        let mut total_tokens = 0_u32;
        let mut keep_from_index = self.messages.len();

        for (i, message) in self.messages.iter().enumerate().rev() {
            let msg_tokens = message.estimate_tokens_with_config(&self.config);
            if total_tokens + msg_tokens > available_for_messages {
                break;
            }
            total_tokens += msg_tokens;
            keep_from_index = i;
        }

        // Remove old messages
        let removed = keep_from_index;
        if removed > 0 {
            self.messages.drain(0..removed);
        }

        removed
    }

    /// Check if the conversation needs trimming to fit within the token limit
    /// Returns true if current token count exceeds the configured threshold
    pub fn needs_trimming(&self, max_tokens: u32) -> bool {
        let threshold = (max_tokens as f64 * self.config.trimming_threshold) as u32;
        self.estimate_tokens() > threshold
    }
}

impl Message {
    /// Estimate token count for this message using default config
    /// Uses a simple heuristic of ~4 characters per token
    pub fn estimate_tokens(&self) -> u32 {
        self.estimate_tokens_with_config(&ConversationConfig::default())
    }

    /// Estimate token count for this message with custom config
    pub fn estimate_tokens_with_config(&self, config: &ConversationConfig) -> u32 {
        let content_len = match &self.content {
            MessageContent::Text(text) => text.len(),
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => text.len(),
                    ContentBlock::ToolUse { name, input, .. } => {
                        name.len() + input.to_string().len()
                    }
                    ContentBlock::ToolResult { content, .. } => {
                        content.estimate_tokens_with_config(config)
                    }
                })
                .sum(),
        };

        // Add overhead for role and structure
        let chars_per_token = config.chars_per_token as usize;
        let overhead = config.message_overhead_tokens as usize;
        ((content_len + overhead) / chars_per_token) as u32
    }
}

impl ToolResultContent {
    /// Estimate token count for tool result content with config
    fn estimate_tokens_with_config(&self, config: &ConversationConfig) -> usize {
        match self {
            ToolResultContent::Text(text) => text.len(),
            ToolResultContent::Blocks(blocks) => blocks
                .iter()
                .map(|b| match b {
                    ToolResultBlock::Text { text } => text.len(),
                    ToolResultBlock::Image { .. } => config.image_token_estimate as usize,
                })
                .sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_user() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, Role::User);
        assert!(matches!(msg.content, MessageContent::Text(ref s) if s == "Hello"));
        assert!(msg.tool_use_id.is_none());
        assert!(msg.token_count.is_none());
    }

    #[test]
    fn test_message_assistant() {
        let msg = Message::assistant("Hi there");
        assert_eq!(msg.role, Role::Assistant);
        assert!(matches!(msg.content, MessageContent::Text(ref s) if s == "Hi there"));
    }

    #[test]
    fn test_message_system() {
        let msg = Message::system("You are a helpful assistant");
        assert_eq!(msg.role, Role::System);
    }

    #[test]
    fn test_message_assistant_blocks() {
        let blocks = vec![
            ContentBlock::Text {
                text: "Let me help".to_string(),
            },
            ContentBlock::ToolUse {
                id: "tool1".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({"path": "/test"}),
            },
        ];
        let msg = Message::assistant_blocks(blocks);
        assert_eq!(msg.role, Role::Assistant);
        assert!(matches!(msg.content, MessageContent::Blocks(_)));
    }

    #[test]
    fn test_message_tool_result() {
        let msg = Message::tool_result("tool1", "File contents here", false);
        assert_eq!(msg.role, Role::User);
        assert!(matches!(msg.content, MessageContent::Blocks(_)));
    }

    #[test]
    fn test_message_tool_result_error() {
        let msg = Message::tool_result("tool1", "Error: file not found", true);
        assert_eq!(msg.role, Role::User);
        if let MessageContent::Blocks(blocks) = &msg.content {
            if let ContentBlock::ToolResult { is_error, .. } = &blocks[0] {
                assert_eq!(*is_error, Some(true));
            }
        }
    }

    #[test]
    fn test_message_text() {
        let msg = Message::user("Hello");
        assert_eq!(msg.text(), Some("Hello"));

        let msg_blocks = Message::assistant_blocks(vec![ContentBlock::Text {
            text: "First".to_string(),
        }]);
        assert_eq!(msg_blocks.text(), Some("First"));
    }

    #[test]
    fn test_message_tool_uses() {
        let msg = Message::assistant_blocks(vec![
            ContentBlock::Text {
                text: "Let me help".to_string(),
            },
            ContentBlock::ToolUse {
                id: "tool1".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({}),
            },
        ]);

        let tool_uses = msg.tool_uses();
        assert_eq!(tool_uses.len(), 1);
    }

    #[test]
    fn test_message_has_tool_use() {
        let msg_no_tools = Message::user("Hello");
        assert!(!msg_no_tools.has_tool_use());

        let msg_with_tools = Message::assistant_blocks(vec![ContentBlock::ToolUse {
            id: "tool1".to_string(),
            name: "test".to_string(),
            input: serde_json::json!({}),
        }]);
        assert!(msg_with_tools.has_tool_use());
    }

    #[test]
    fn test_message_content_into_blocks() {
        let text = MessageContent::Text("Hello".to_string());
        let blocks = text.into_blocks();
        assert_eq!(blocks.len(), 1);

        let existing_blocks = MessageContent::Blocks(vec![
            ContentBlock::Text {
                text: "A".to_string(),
            },
            ContentBlock::Text {
                text: "B".to_string(),
            },
        ]);
        let blocks = existing_blocks.into_blocks();
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn test_message_content_as_text() {
        let text = MessageContent::Text("Hello".to_string());
        assert_eq!(text.as_text(), Some("Hello"));

        let blocks = MessageContent::Blocks(vec![]);
        assert_eq!(blocks.as_text(), None);
    }

    #[test]
    fn test_role_display() {
        assert_eq!(format!("{}", Role::User), "user");
        assert_eq!(format!("{}", Role::Assistant), "assistant");
        assert_eq!(format!("{}", Role::System), "system");
    }

    #[test]
    fn test_conversation_new() {
        let conv = Conversation::new();
        assert!(conv.is_empty());
        assert!(conv.system_prompt.is_none());
    }

    #[test]
    fn test_conversation_with_system() {
        let conv = Conversation::with_system("You are helpful");
        assert!(conv.is_empty());
        assert_eq!(conv.system_prompt, Some("You are helpful".to_string()));
    }

    #[test]
    fn test_conversation_set_system() {
        let mut conv = Conversation::new();
        conv.set_system("New system prompt");
        assert_eq!(conv.system_prompt, Some("New system prompt".to_string()));
    }

    #[test]
    fn test_conversation_push() {
        let mut conv = Conversation::new();
        conv.push(Message::user("Hello"));
        conv.push(Message::assistant("Hi"));

        assert_eq!(conv.len(), 2);
        assert!(!conv.is_empty());
    }

    #[test]
    fn test_conversation_last() {
        let mut conv = Conversation::new();
        assert!(conv.last().is_none());

        conv.push(Message::user("Hello"));
        conv.push(Message::assistant("Hi"));

        let last = conv.last().unwrap();
        assert_eq!(last.role, Role::Assistant);
    }

    #[test]
    fn test_conversation_last_assistant() {
        let mut conv = Conversation::new();
        assert!(conv.last_assistant().is_none());

        conv.push(Message::user("Hello"));
        conv.push(Message::assistant("Hi"));
        conv.push(Message::user("How are you?"));

        let last_assistant = conv.last_assistant().unwrap();
        assert_eq!(last_assistant.role, Role::Assistant);
    }

    #[test]
    fn test_conversation_clear() {
        let mut conv = Conversation::new();
        conv.push(Message::user("Hello"));
        conv.push(Message::assistant("Hi"));

        assert_eq!(conv.len(), 2);

        conv.clear();
        assert!(conv.is_empty());
        assert_eq!(conv.len(), 0);
    }

    #[test]
    fn test_content_block_serialization() {
        let text_block = ContentBlock::Text {
            text: "Hello".to_string(),
        };
        let json = serde_json::to_string(&text_block).unwrap();
        assert!(json.contains("text"));

        let tool_use = ContentBlock::ToolUse {
            id: "id1".to_string(),
            name: "test".to_string(),
            input: serde_json::json!({"key": "value"}),
        };
        let json = serde_json::to_string(&tool_use).unwrap();
        assert!(json.contains("tool_use"));
    }

    #[test]
    fn test_tool_result_content() {
        let text_content = ToolResultContent::Text("Result".to_string());
        let json = serde_json::to_string(&text_content).unwrap();
        assert_eq!(json, "\"Result\"");
    }

    #[test]
    fn test_image_source() {
        let source = ImageSource {
            source_type: "base64".to_string(),
            media_type: "image/png".to_string(),
            data: "base64data".to_string(),
        };

        assert_eq!(source.source_type, "base64");
        assert_eq!(source.media_type, "image/png");
    }

    // ===== Token Estimation Tests =====

    #[test]
    fn test_message_estimate_tokens_simple() {
        let msg = Message::user("Hello world");
        let tokens = msg.estimate_tokens();
        // "Hello world" is 11 chars + 20 overhead = 31, /4 = 7
        assert!(tokens > 0);
        assert!(tokens < 100); // Sanity check
    }

    #[test]
    fn test_message_estimate_tokens_long() {
        let long_text = "a".repeat(1000);
        let msg = Message::user(&long_text);
        let tokens = msg.estimate_tokens();
        // 1000 chars + 20 overhead = 1020, /4 = 255
        assert!(tokens > 200);
        assert!(tokens < 300);
    }

    #[test]
    fn test_message_estimate_tokens_with_tool_use() {
        let msg = Message::assistant_blocks(vec![
            ContentBlock::Text {
                text: "Let me help".to_string(),
            },
            ContentBlock::ToolUse {
                id: "tool1".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({"path": "/very/long/path/to/some/file.txt"}),
            },
        ]);
        let tokens = msg.estimate_tokens();
        assert!(tokens > 0);
    }

    #[test]
    fn test_message_estimate_tokens_tool_result() {
        let msg = Message::tool_result("tool1", "File contents here", false);
        let tokens = msg.estimate_tokens();
        assert!(tokens > 0);
    }

    // ===== Conversation Token Tests =====

    #[test]
    fn test_conversation_estimate_tokens_empty() {
        let conv = Conversation::new();
        assert_eq!(conv.estimate_tokens(), 0);
    }

    #[test]
    fn test_conversation_estimate_tokens_with_system() {
        let conv = Conversation::with_system("You are a helpful assistant");
        let tokens = conv.estimate_tokens();
        // ~27 chars / 4 = ~6 tokens for system prompt
        assert!(tokens > 0);
    }

    #[test]
    fn test_conversation_estimate_tokens_with_messages() {
        let mut conv = Conversation::new();
        conv.push(Message::user("Hello"));
        conv.push(Message::assistant("Hi there, how can I help?"));
        let tokens = conv.estimate_tokens();
        assert!(tokens > 0);
    }

    // ===== Conversation Truncation Tests =====

    #[test]
    fn test_conversation_truncate_to_fit_no_truncation_needed() {
        let mut conv = Conversation::with_system("System");
        conv.push(Message::user("Hello"));
        conv.push(Message::assistant("Hi"));

        let (messages, was_truncated) = conv.truncate_to_fit(100000);
        assert!(!was_truncated);
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_conversation_truncate_to_fit_truncation_needed() {
        let mut conv = Conversation::with_system("System prompt");
        for i in 0..100 {
            conv.push(Message::user(format!("Message {} with some content", i)));
            conv.push(Message::assistant(format!(
                "Response {} with more content",
                i
            )));
        }

        // Very low limit should truncate
        let (messages, was_truncated) = conv.truncate_to_fit(1000);
        assert!(was_truncated);
        assert!(messages.len() < 200);
    }

    #[test]
    fn test_conversation_truncate_to_fit_zero_available() {
        let mut conv = Conversation::with_system("System");
        conv.push(Message::user("Hello"));

        // Very small limit (less than system + buffer)
        let (messages, was_truncated) = conv.truncate_to_fit(10);
        assert!(was_truncated);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_conversation_trim_to_fit_no_trimming() {
        let mut conv = Conversation::new();
        conv.push(Message::user("Hello"));
        conv.push(Message::assistant("Hi"));

        let removed = conv.trim_to_fit(100000);
        assert_eq!(removed, 0);
        assert_eq!(conv.len(), 2);
    }

    #[test]
    fn test_conversation_trim_to_fit_trimming_needed() {
        let mut conv = Conversation::new();
        for i in 0..50 {
            conv.push(Message::user(format!("Message {} with content", i)));
            conv.push(Message::assistant(format!("Response {} with content", i)));
        }

        let initial_len = conv.len();
        let removed = conv.trim_to_fit(1000);

        assert!(removed > 0);
        assert!(conv.len() < initial_len);
        assert_eq!(conv.len() + removed, initial_len);
    }

    #[test]
    fn test_conversation_trim_to_fit_zero_budget() {
        let mut conv = Conversation::new();
        conv.push(Message::user("Hello"));
        conv.push(Message::assistant("Hi"));

        let removed = conv.trim_to_fit(0);
        assert_eq!(removed, 2);
        assert!(conv.is_empty());
    }

    #[test]
    fn test_conversation_needs_trimming_false() {
        let conv = Conversation::new();
        assert!(!conv.needs_trimming(100000));
    }

    #[test]
    fn test_conversation_needs_trimming_true() {
        let mut conv = Conversation::new();
        for i in 0..1000 {
            conv.push(Message::user(format!(
                "Long message {} with lots of content to increase token count",
                i
            )));
        }

        // With very low limit, should need trimming
        assert!(conv.needs_trimming(100));
    }

    // ===== ToolResultContent Tests =====

    #[test]
    fn test_tool_result_content_text_estimate() {
        let content = ToolResultContent::Text("Hello world".to_string());
        let len = match content {
            ToolResultContent::Text(text) => text.len(),
            _ => 0,
        };
        assert_eq!(len, 11);
    }

    #[test]
    fn test_tool_result_content_blocks() {
        let content = ToolResultContent::Blocks(vec![
            ToolResultBlock::Text {
                text: "First block".to_string(),
            },
            ToolResultBlock::Text {
                text: "Second block".to_string(),
            },
        ]);

        if let ToolResultContent::Blocks(blocks) = &content {
            assert_eq!(blocks.len(), 2);
        }
    }

    #[test]
    fn test_tool_result_block_image() {
        let block = ToolResultBlock::Image {
            source: ImageSource {
                source_type: "base64".to_string(),
                media_type: "image/jpeg".to_string(),
                data: "SGVsbG8=".to_string(),
            },
        };

        if let ToolResultBlock::Image { source } = block {
            assert_eq!(source.media_type, "image/jpeg");
        }
    }

    // ===== Serialization Tests =====

    #[test]
    fn test_message_serialization() {
        let msg = Message::user("Test message");
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.role, parsed.role);
        assert_eq!(msg.text(), parsed.text());
    }

    #[test]
    fn test_message_with_blocks_serialization() {
        let msg = Message::assistant_blocks(vec![
            ContentBlock::Text {
                text: "Hello".to_string(),
            },
            ContentBlock::ToolUse {
                id: "t1".to_string(),
                name: "test".to_string(),
                input: serde_json::json!({}),
            },
        ]);

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: Message = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.role, parsed.role);
    }

    #[test]
    fn test_role_serialization() {
        let roles = [Role::User, Role::Assistant, Role::System];
        for role in roles {
            let json = serde_json::to_string(&role).unwrap();
            let parsed: Role = serde_json::from_str(&json).unwrap();
            assert_eq!(role, parsed);
        }
    }

    // ===== Edge Cases =====

    #[test]
    fn test_message_text_empty_blocks() {
        let msg = Message::assistant_blocks(vec![]);
        assert!(msg.text().is_none());
    }

    #[test]
    fn test_message_text_only_tool_use() {
        let msg = Message::assistant_blocks(vec![ContentBlock::ToolUse {
            id: "t1".to_string(),
            name: "test".to_string(),
            input: serde_json::json!({}),
        }]);
        assert!(msg.text().is_none());
    }

    #[test]
    fn test_conversation_default() {
        let conv = Conversation::default();
        assert!(conv.is_empty());
        assert!(conv.system_prompt.is_none());
    }

    #[test]
    fn test_conversation_iterator() {
        let mut conv = Conversation::new();
        conv.push(Message::user("1"));
        conv.push(Message::assistant("2"));
        conv.push(Message::user("3"));

        let count = conv.messages.len();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_message_unique_ids() {
        let msg1 = Message::user("Hello");
        let msg2 = Message::user("Hello");
        assert_ne!(msg1.id, msg2.id);
    }

    #[test]
    fn test_message_timestamp() {
        let before = Utc::now();
        let msg = Message::user("Hello");
        let after = Utc::now();

        assert!(msg.timestamp >= before);
        assert!(msg.timestamp <= after);
    }

    #[test]
    fn test_content_block_tool_result_with_error() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "t1".to_string(),
            content: ToolResultContent::Text("Error occurred".to_string()),
            is_error: Some(true),
        };

        if let ContentBlock::ToolResult { is_error, .. } = block {
            assert_eq!(is_error, Some(true));
        }
    }

    #[test]
    fn test_content_block_tool_result_no_error() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "t1".to_string(),
            content: ToolResultContent::Text("Success".to_string()),
            is_error: None,
        };

        if let ContentBlock::ToolResult { is_error, .. } = block {
            assert!(is_error.is_none());
        }
    }
}
