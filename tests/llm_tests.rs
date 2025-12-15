// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use ted::llm::message::{ContentBlock, Conversation, Message, MessageContent, Role};
use ted::llm::provider::{CompletionRequest, ToolDefinition, ToolInputSchema, Usage};

#[test]
fn test_message_user_creation() {
    let message = Message::user("Hello, world!");

    assert_eq!(message.role, Role::User);
    match &message.content {
        MessageContent::Text(text) => assert_eq!(text, "Hello, world!"),
        _ => panic!("Expected text content"),
    }
}

#[test]
fn test_message_assistant_creation() {
    let message = Message::assistant("I can help with that.");

    assert_eq!(message.role, Role::Assistant);
    match &message.content {
        MessageContent::Text(text) => assert_eq!(text, "I can help with that."),
        _ => panic!("Expected text content"),
    }
}

#[test]
fn test_conversation_push() {
    let mut conversation = Conversation::new();
    conversation.push(Message::user("First message"));
    conversation.push(Message::assistant("Response"));

    assert_eq!(conversation.messages.len(), 2);
}

#[test]
fn test_conversation_set_system() {
    let mut conversation = Conversation::new();
    conversation.set_system("You are a helpful assistant.");

    assert_eq!(
        conversation.system_prompt,
        Some("You are a helpful assistant.".to_string())
    );
}

#[test]
fn test_conversation_clear() {
    let mut conversation = Conversation::new();
    conversation.push(Message::user("Hello"));
    conversation.push(Message::assistant("Hi"));
    conversation.set_system("System prompt");

    conversation.clear();

    assert!(conversation.messages.is_empty());
    assert!(conversation.system_prompt.is_some()); // System prompt is preserved
}

#[test]
fn test_content_block_text() {
    let block = ContentBlock::Text {
        text: "Hello".to_string(),
    };

    match block {
        ContentBlock::Text { text } => assert_eq!(text, "Hello"),
        _ => panic!("Expected Text block"),
    }
}

#[test]
fn test_content_block_tool_use() {
    let block = ContentBlock::ToolUse {
        id: "tool-123".to_string(),
        name: "file_read".to_string(),
        input: serde_json::json!({"path": "/tmp/test.txt"}),
    };

    match block {
        ContentBlock::ToolUse { id, name, input } => {
            assert_eq!(id, "tool-123");
            assert_eq!(name, "file_read");
            assert_eq!(input["path"], "/tmp/test.txt");
        }
        _ => panic!("Expected ToolUse block"),
    }
}

#[test]
fn test_completion_request_builder() {
    let messages = vec![Message::user("Hello")];
    let request = CompletionRequest::new("claude-3-5-sonnet-20241022", messages)
        .with_max_tokens(1000)
        .with_temperature(0.5);

    assert_eq!(request.model, "claude-3-5-sonnet-20241022");
    assert_eq!(request.max_tokens, 1000);
    assert!((request.temperature - 0.5).abs() < f32::EPSILON);
}

#[test]
fn test_completion_request_with_system() {
    let messages = vec![Message::user("Hello")];
    let request =
        CompletionRequest::new("claude-3-5-sonnet-20241022", messages).with_system("Be helpful");

    assert_eq!(request.system, Some("Be helpful".to_string()));
}

#[test]
fn test_completion_request_with_tools() {
    let messages = vec![Message::user("Hello")];
    let tools = vec![ToolDefinition {
        name: "test_tool".to_string(),
        description: "A test tool".to_string(),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({}),
            required: vec![],
        },
    }];

    let request =
        CompletionRequest::new("claude-3-5-sonnet-20241022", messages).with_tools(tools.clone());

    assert_eq!(request.tools.len(), 1);
    assert_eq!(request.tools[0].name, "test_tool");
}

#[test]
fn test_usage_tracking() {
    let usage = Usage {
        input_tokens: 100,
        output_tokens: 50,
        cache_creation_input_tokens: 10,
        cache_read_input_tokens: 5,
    };

    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 50);
    assert_eq!(usage.cache_creation_input_tokens, 10);
    assert_eq!(usage.cache_read_input_tokens, 5);
}

#[test]
fn test_role_enum() {
    assert_eq!(format!("{:?}", Role::User), "User");
    assert_eq!(format!("{:?}", Role::Assistant), "Assistant");
}

#[test]
fn test_message_with_blocks() {
    let blocks = vec![
        ContentBlock::Text {
            text: "Here's the file:".to_string(),
        },
        ContentBlock::ToolUse {
            id: "tool-1".to_string(),
            name: "file_read".to_string(),
            input: serde_json::json!({"path": "/tmp/test"}),
        },
    ];

    let message = Message {
        id: uuid::Uuid::new_v4(),
        role: Role::Assistant,
        content: MessageContent::Blocks(blocks),
        timestamp: chrono::Utc::now(),
        tool_use_id: None,
        token_count: None,
    };

    match message.content {
        MessageContent::Blocks(blocks) => assert_eq!(blocks.len(), 2),
        _ => panic!("Expected Blocks content"),
    }
}
