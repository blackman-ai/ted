// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use super::*;
use crate::llm::message::Role;
use crate::llm::provider::{ModelInfo, StreamEvent};
use futures::stream;
use std::pin::Pin;

// ==================== Mock Provider ====================

/// Mock LLM provider for testing
struct MockProvider;

#[async_trait::async_trait]
impl LlmProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![ModelInfo {
            id: "test-model".to_string(),
            display_name: "Test Model".to_string(),
            context_window: 4096,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_vision: false,
            input_cost_per_1k: 0.0,
            output_cost_per_1k: 0.0,
        }]
    }

    fn supports_model(&self, model: &str) -> bool {
        model == "test-model"
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> crate::error::Result<crate::llm::provider::CompletionResponse> {
        Ok(crate::llm::provider::CompletionResponse {
            id: "mock-response-id".to_string(),
            model: "test-model".to_string(),
            content: vec![ContentBlockResponse::Text {
                text: "Mock response".to_string(),
            }],
            usage: crate::llm::provider::Usage {
                input_tokens: 10,
                output_tokens: 5,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
            stop_reason: Some(StopReason::EndTurn),
        })
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> crate::error::Result<
        Pin<Box<dyn futures::Stream<Item = crate::error::Result<StreamEvent>> + Send>>,
    > {
        Ok(Box::pin(stream::empty()))
    }

    fn count_tokens(&self, _text: &str, _model: &str) -> crate::error::Result<u32> {
        Ok(10)
    }
}

fn make_test_runner() -> AgentRunner {
    AgentRunner::new(Arc::new(MockProvider))
}

// ==================== truncate_str Tests ====================

#[test]
fn test_truncate_str_short() {
    let short = "Hello";
    assert_eq!(truncate_str(short, 100), "Hello");
}

#[test]
fn test_truncate_str_long() {
    let long = "A".repeat(150);
    let result = truncate_str(&long, 100);
    assert!(result.ends_with("..."));
    assert!(result.len() <= 104);
}

#[test]
fn test_truncate_str_newlines() {
    let with_newlines = "Line 1\nLine 2\nLine 3";
    let result = truncate_str(with_newlines, 100);
    assert!(!result.contains('\n'));
}

#[test]
fn test_truncate_str_exact_length() {
    let exact = "A".repeat(100);
    let result = truncate_str(&exact, 100);
    assert_eq!(result.len(), 100);
    assert!(!result.ends_with("..."));
}

#[test]
fn test_truncate_str_empty() {
    let empty = "";
    assert_eq!(truncate_str(empty, 100), "");
}

#[test]
fn test_truncate_str_one_over() {
    let one_over = "A".repeat(101);
    let result = truncate_str(&one_over, 100);
    assert!(result.ends_with("..."));
}

#[test]
fn test_truncate_str_replaces_all_newlines() {
    let multi_newline = "A\nB\nC\nD\nE";
    let result = truncate_str(multi_newline, 100);
    assert_eq!(result.matches(' ').count(), 4); // Newlines become spaces
}

#[test]
fn test_truncate_str_preserves_content_before_truncation() {
    let content = "Hello World";
    let result = truncate_str(content, 100);
    assert_eq!(result, "Hello World");
}

#[test]
fn test_truncate_str_zero_max_len() {
    let content = "Hello";
    let result = truncate_str(content, 0);
    assert!(result.ends_with("..."));
}

// ==================== RunnerConfig Tests ====================

#[test]
fn test_runner_config_default() {
    let config = RunnerConfig::default();
    assert_eq!(config.max_response_tokens, 4096);
    assert_eq!(config.temperature, 0.7);
    assert!(!config.verbose);
    assert_eq!(config.max_rate_limit_retries, 3);
    assert_eq!(config.base_retry_delay_secs, 2);
}

#[test]
fn test_runner_config_custom() {
    let config = RunnerConfig {
        max_response_tokens: 8192,
        temperature: 0.5,
        verbose: true,
        quiet: false,
        max_rate_limit_retries: 5,
        base_retry_delay_secs: 3,
    };
    assert_eq!(config.max_response_tokens, 8192);
    assert_eq!(config.temperature, 0.5);
    assert!(config.verbose);
    assert!(!config.quiet);
    assert_eq!(config.max_rate_limit_retries, 5);
    assert_eq!(config.base_retry_delay_secs, 3);
}

#[test]
fn test_runner_config_clone() {
    let config = RunnerConfig::default();
    let cloned = config.clone();
    assert_eq!(cloned.max_response_tokens, config.max_response_tokens);
    assert_eq!(cloned.temperature, config.temperature);
}

#[test]
fn test_runner_config_debug() {
    let config = RunnerConfig::default();
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("RunnerConfig"));
    assert!(debug_str.contains("4096"));
}

// ==================== generate_summary Tests ====================

#[test]
fn test_generate_summary_short() {
    let runner = make_test_runner();
    let summary = runner.generate_summary("Short text");
    assert_eq!(summary, "Short text");
}

#[test]
fn test_generate_summary_long() {
    let runner = make_test_runner();
    let long = "A".repeat(300);
    let summary = runner.generate_summary(&long);
    assert!(summary.len() <= 204); // 200 chars + "..."
    assert!(summary.ends_with("..."));
}

#[test]
fn test_generate_summary_paragraph_break() {
    let runner = make_test_runner();
    let text = "First paragraph.\n\nSecond paragraph.";
    let summary = runner.generate_summary(text);
    // First paragraph is taken, but "..." is added since summary < original length
    assert_eq!(summary, "First paragraph....");
}

#[test]
fn test_generate_summary_multiple_paragraphs() {
    let runner = make_test_runner();
    let text = "Para 1.\n\nPara 2.\n\nPara 3.";
    let summary = runner.generate_summary(text);
    // Should only take first paragraph, with "..." since summary < original
    assert_eq!(summary, "Para 1....");
}

#[test]
fn test_generate_summary_trims_whitespace() {
    let runner = make_test_runner();
    let summary = runner.generate_summary("  trimmed  ");
    assert_eq!(summary, "trimmed");
}

#[test]
fn test_generate_summary_empty() {
    let runner = make_test_runner();
    let summary = runner.generate_summary("");
    assert_eq!(summary, "");
}

#[test]
fn test_generate_summary_long_first_paragraph() {
    let runner = make_test_runner();
    let long_para = "A".repeat(250);
    let text = format!("{}\n\nSecond para.", long_para);
    let summary = runner.generate_summary(&text);
    // Should truncate to 200 chars
    assert!(summary.len() <= 204);
    assert!(summary.ends_with("..."));
}

#[test]
fn test_generate_summary_no_paragraph_break() {
    let runner = make_test_runner();
    let text = "Single line of text without paragraph breaks";
    let summary = runner.generate_summary(text);
    assert_eq!(summary, text);
}

#[test]
fn test_generate_summary_only_whitespace() {
    let runner = make_test_runner();
    let summary = runner.generate_summary("   \n\n   ");
    // Whitespace splits to "   ", trims to "", but since summary < original, adds "..."
    assert_eq!(summary, "...");
}

// ==================== response_to_message Tests ====================

#[test]
fn test_response_to_message_text_only() {
    let runner = make_test_runner();
    let content = vec![ContentBlockResponse::Text {
        text: "Hello".to_string(),
    }];
    let msg = runner.response_to_message(&content);
    assert_eq!(msg.role, Role::Assistant);

    if let MessageContent::Blocks(blocks) = &msg.content {
        assert_eq!(blocks.len(), 1);
        if let ContentBlock::Text { text } = &blocks[0] {
            assert_eq!(text, "Hello");
        } else {
            panic!("Expected Text block");
        }
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_response_to_message_tool_use() {
    let runner = make_test_runner();
    let content = vec![ContentBlockResponse::ToolUse {
        id: "tool-1".to_string(),
        name: "file_read".to_string(),
        input: serde_json::json!({"path": "/test"}),
    }];
    let msg = runner.response_to_message(&content);
    assert_eq!(msg.role, Role::Assistant);

    if let MessageContent::Blocks(blocks) = &msg.content {
        assert_eq!(blocks.len(), 1);
        if let ContentBlock::ToolUse { id, name, input } = &blocks[0] {
            assert_eq!(id, "tool-1");
            assert_eq!(name, "file_read");
            assert_eq!(input["path"], "/test");
        } else {
            panic!("Expected ToolUse block");
        }
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_response_to_message_mixed() {
    let runner = make_test_runner();
    let content = vec![
        ContentBlockResponse::Text {
            text: "I'll read the file".to_string(),
        },
        ContentBlockResponse::ToolUse {
            id: "tool-1".to_string(),
            name: "file_read".to_string(),
            input: serde_json::json!({}),
        },
    ];
    let msg = runner.response_to_message(&content);

    if let MessageContent::Blocks(blocks) = &msg.content {
        assert_eq!(blocks.len(), 2);
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_response_to_message_empty() {
    let runner = make_test_runner();
    let content: Vec<ContentBlockResponse> = vec![];
    let msg = runner.response_to_message(&content);

    if let MessageContent::Blocks(blocks) = &msg.content {
        assert!(blocks.is_empty());
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_response_to_message_multiple_tool_uses() {
    let runner = make_test_runner();
    let content = vec![
        ContentBlockResponse::ToolUse {
            id: "tool-1".to_string(),
            name: "file_read".to_string(),
            input: serde_json::json!({"path": "/a"}),
        },
        ContentBlockResponse::ToolUse {
            id: "tool-2".to_string(),
            name: "file_write".to_string(),
            input: serde_json::json!({"path": "/b"}),
        },
    ];
    let msg = runner.response_to_message(&content);

    if let MessageContent::Blocks(blocks) = &msg.content {
        assert_eq!(blocks.len(), 2);
    } else {
        panic!("Expected Blocks content");
    }
}

// ==================== tool_results_to_message Tests ====================

#[test]
fn test_tool_results_to_message_success() {
    let runner = make_test_runner();
    let results = vec![ToolResult {
        tool_use_id: "tool-1".to_string(),
        output: ToolOutput::Success("File contents".to_string()),
    }];
    let msg = runner.tool_results_to_message(&results);
    assert_eq!(msg.role, Role::User);

    if let MessageContent::Blocks(blocks) = &msg.content {
        assert_eq!(blocks.len(), 1);
        if let ContentBlock::ToolResult {
            tool_use_id,
            is_error,
            ..
        } = &blocks[0]
        {
            assert_eq!(tool_use_id, "tool-1");
            assert!(is_error.is_none() || *is_error == Some(false));
        } else {
            panic!("Expected ToolResult block");
        }
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_tool_results_to_message_error() {
    let runner = make_test_runner();
    let results = vec![ToolResult {
        tool_use_id: "tool-1".to_string(),
        output: ToolOutput::Error("Failed to read".to_string()),
    }];
    let msg = runner.tool_results_to_message(&results);

    if let MessageContent::Blocks(blocks) = &msg.content {
        if let ContentBlock::ToolResult { is_error, .. } = &blocks[0] {
            assert_eq!(*is_error, Some(true));
        } else {
            panic!("Expected ToolResult block");
        }
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_tool_results_to_message_multiple() {
    let runner = make_test_runner();
    let results = vec![
        ToolResult {
            tool_use_id: "tool-1".to_string(),
            output: ToolOutput::Success("Result 1".to_string()),
        },
        ToolResult {
            tool_use_id: "tool-2".to_string(),
            output: ToolOutput::Success("Result 2".to_string()),
        },
    ];
    let msg = runner.tool_results_to_message(&results);

    if let MessageContent::Blocks(blocks) = &msg.content {
        assert_eq!(blocks.len(), 2);
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_tool_results_to_message_empty() {
    let runner = make_test_runner();
    let results: Vec<ToolResult> = vec![];
    let msg = runner.tool_results_to_message(&results);

    if let MessageContent::Blocks(blocks) = &msg.content {
        assert!(blocks.is_empty());
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_tool_results_to_message_mixed() {
    let runner = make_test_runner();
    let results = vec![
        ToolResult {
            tool_use_id: "tool-1".to_string(),
            output: ToolOutput::Success("OK".to_string()),
        },
        ToolResult {
            tool_use_id: "tool-2".to_string(),
            output: ToolOutput::Error("Failed".to_string()),
        },
    ];
    let msg = runner.tool_results_to_message(&results);

    if let MessageContent::Blocks(blocks) = &msg.content {
        assert_eq!(blocks.len(), 2);

        // First should be success
        if let ContentBlock::ToolResult { is_error, .. } = &blocks[0] {
            assert!(is_error.is_none() || *is_error == Some(false));
        }

        // Second should be error
        if let ContentBlock::ToolResult { is_error, .. } = &blocks[1] {
            assert_eq!(*is_error, Some(true));
        }
    } else {
        panic!("Expected Blocks content");
    }
}

// ==================== AgentRunner Creation Tests ====================

#[test]
fn test_agent_runner_new() {
    let runner = AgentRunner::new(Arc::new(MockProvider));
    assert_eq!(runner.config.max_response_tokens, 4096);
    assert!(!runner.config.verbose);
}

#[test]
fn test_agent_runner_with_config() {
    let config = RunnerConfig {
        max_response_tokens: 8192,
        temperature: 0.9,
        verbose: true,
        ..Default::default()
    };
    let runner = AgentRunner::with_config(Arc::new(MockProvider), config);
    assert_eq!(runner.config.max_response_tokens, 8192);
    assert!(runner.config.verbose);
}

// ==================== BackgroundAgentHandle Tests ====================

#[test]
fn test_background_agent_handle_fields() {
    // Just test the struct fields are accessible
    let id = Uuid::new_v4();
    let name = "test-agent".to_string();
    // Can't easily test the handle without spawning a real task
    // but we can verify the struct is correctly defined
    assert!(!name.is_empty());
    assert!(!id.is_nil());
}

// ==================== Additional comprehensive tests ====================

// ===== RunnerConfig additional tests =====

#[test]
fn test_runner_config_extreme_values() {
    let config = RunnerConfig {
        max_response_tokens: u32::MAX,
        temperature: 0.0,
        verbose: false,
        ..Default::default()
    };
    assert_eq!(config.max_response_tokens, u32::MAX);
    assert_eq!(config.temperature, 0.0);
}

#[test]
fn test_runner_config_high_temperature() {
    let config = RunnerConfig {
        max_response_tokens: 4096,
        temperature: 2.0, // Some APIs allow up to 2.0
        verbose: true,
        ..Default::default()
    };
    assert_eq!(config.temperature, 2.0);
}

#[test]
fn test_runner_config_zero_tokens() {
    let config = RunnerConfig {
        max_response_tokens: 0,
        temperature: 0.7,
        verbose: false,
        ..Default::default()
    };
    assert_eq!(config.max_response_tokens, 0);
}

// ===== generate_summary additional tests =====

#[test]
fn test_generate_summary_exactly_200_chars() {
    let runner = make_test_runner();
    let text = "A".repeat(200);
    let summary = runner.generate_summary(&text);
    // Exactly 200 chars, no paragraph break, should return as-is (after trim)
    assert_eq!(summary.len(), 200);
    assert!(!summary.ends_with("..."));
}

#[test]
fn test_generate_summary_201_chars() {
    let runner = make_test_runner();
    let text = "A".repeat(201);
    let summary = runner.generate_summary(&text);
    // Over 200, should be truncated with "..."
    assert!(summary.len() <= 204);
    assert!(summary.ends_with("..."));
}

#[test]
fn test_generate_summary_with_leading_whitespace() {
    let runner = make_test_runner();
    let text = "   Leading whitespace";
    let summary = runner.generate_summary(text);
    assert_eq!(summary, "Leading whitespace");
}

#[test]
fn test_generate_summary_with_trailing_whitespace() {
    let runner = make_test_runner();
    let text = "Trailing whitespace   ";
    let summary = runner.generate_summary(text);
    assert_eq!(summary, "Trailing whitespace");
}

#[test]
fn test_generate_summary_multiple_newlines() {
    let runner = make_test_runner();
    let text = "First\n\nSecond\n\n\n\nThird";
    let summary = runner.generate_summary(text);
    // First paragraph only, with "..." since it's shorter than original
    assert_eq!(summary, "First...");
}

#[test]
fn test_generate_summary_single_newline_not_paragraph() {
    let runner = make_test_runner();
    let text = "Line one\nLine two\nLine three";
    let summary = runner.generate_summary(text);
    // Single newlines don't count as paragraph breaks
    assert_eq!(summary, text);
}

#[test]
fn test_generate_summary_paragraph_longer_than_200() {
    let runner = make_test_runner();
    let long_para = "B".repeat(250);
    let text = format!("{}\n\nSecond paragraph", long_para);
    let summary = runner.generate_summary(&text);
    // First paragraph is long, so it gets truncated
    assert!(summary.len() <= 204);
    assert!(summary.ends_with("..."));
}

// ===== truncate_str additional tests =====

#[test]
fn test_truncate_str_mixed_content() {
    let text = "Hello\nWorld\tTabs\rCarriage";
    let result = truncate_str(text, 100);
    assert!(result.contains(' ')); // \n replaced with space
    assert!(!result.contains('\n'));
}

#[test]
fn test_truncate_str_unicode() {
    let text = "日本語テスト🎉";
    let result = truncate_str(text, 100);
    assert_eq!(result, text);
}

#[test]
fn test_truncate_str_unicode_truncation() {
    // Be careful with Unicode truncation
    let text = "A".repeat(50) + "日本語" + &"B".repeat(50);
    let result = truncate_str(&text, 60);
    // Should truncate somewhere in the middle
    assert!(result.len() <= 63); // 60 + "..."
}

// ===== response_to_message additional tests =====

#[test]
fn test_response_to_message_many_text_blocks() {
    let runner = make_test_runner();
    let content: Vec<ContentBlockResponse> = (0..10)
        .map(|i| ContentBlockResponse::Text {
            text: format!("Block {}", i),
        })
        .collect();

    let msg = runner.response_to_message(&content);
    if let MessageContent::Blocks(blocks) = &msg.content {
        assert_eq!(blocks.len(), 10);
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_response_to_message_many_tool_uses() {
    let runner = make_test_runner();
    let content: Vec<ContentBlockResponse> = (0..5)
        .map(|i| ContentBlockResponse::ToolUse {
            id: format!("tool-{}", i),
            name: "file_read".to_string(),
            input: serde_json::json!({"path": format!("/file{}.txt", i)}),
        })
        .collect();

    let msg = runner.response_to_message(&content);
    if let MessageContent::Blocks(blocks) = &msg.content {
        assert_eq!(blocks.len(), 5);
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_response_to_message_preserves_tool_input() {
    let runner = make_test_runner();
    let complex_input = serde_json::json!({
        "path": "/test.txt",
        "options": {
            "encoding": "utf-8",
            "nested": {"value": 42}
        }
    });

    let content = vec![ContentBlockResponse::ToolUse {
        id: "tool-1".to_string(),
        name: "file_read".to_string(),
        input: complex_input.clone(),
    }];

    let msg = runner.response_to_message(&content);
    if let MessageContent::Blocks(blocks) = &msg.content {
        if let ContentBlock::ToolUse { input, .. } = &blocks[0] {
            assert_eq!(input, &complex_input);
        } else {
            panic!("Expected ToolUse block");
        }
    } else {
        panic!("Expected Blocks content");
    }
}

// ===== tool_results_to_message additional tests =====

#[test]
fn test_tool_results_to_message_large_output() {
    let runner = make_test_runner();
    let large_output = "X".repeat(10000);
    let results = vec![ToolResult {
        tool_use_id: "tool-1".to_string(),
        output: ToolOutput::Success(large_output.clone()),
    }];

    let msg = runner.tool_results_to_message(&results);
    if let MessageContent::Blocks(blocks) = &msg.content {
        if let ContentBlock::ToolResult { content, .. } = &blocks[0] {
            // Content should preserve the large output
            match content {
                crate::llm::message::ToolResultContent::Text(text) => {
                    assert_eq!(text.len(), large_output.len());
                }
                _ => panic!("Expected Text content"),
            }
        } else {
            panic!("Expected ToolResult block");
        }
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_tool_results_to_message_unicode_output() {
    let runner = make_test_runner();
    let results = vec![ToolResult {
        tool_use_id: "tool-1".to_string(),
        output: ToolOutput::Success("日本語コンテンツ 🎉".to_string()),
    }];

    let msg = runner.tool_results_to_message(&results);
    if let MessageContent::Blocks(blocks) = &msg.content {
        if let ContentBlock::ToolResult { content, .. } = &blocks[0] {
            match content {
                crate::llm::message::ToolResultContent::Text(text) => {
                    assert!(text.contains("日本語"));
                    assert!(text.contains("🎉"));
                }
                _ => panic!("Expected Text content"),
            }
        }
    }
}

#[test]
fn test_tool_results_to_message_has_correct_role() {
    let runner = make_test_runner();
    let results = vec![ToolResult {
        tool_use_id: "tool-1".to_string(),
        output: ToolOutput::Success("OK".to_string()),
    }];

    let msg = runner.tool_results_to_message(&results);
    // Tool results are sent as user messages in Claude API
    assert_eq!(msg.role, Role::User);
}

#[test]
fn test_tool_results_to_message_has_uuid() {
    let runner = make_test_runner();
    let results = vec![ToolResult {
        tool_use_id: "tool-1".to_string(),
        output: ToolOutput::Success("OK".to_string()),
    }];

    let msg = runner.tool_results_to_message(&results);
    assert!(!msg.id.is_nil());
}

// ===== AgentRunner construction tests =====

#[test]
fn test_agent_runner_with_config_preserves_all_fields() {
    let config = RunnerConfig {
        max_response_tokens: 16384,
        temperature: 0.5,
        verbose: true,
        quiet: false,
        max_rate_limit_retries: 5,
        base_retry_delay_secs: 4,
    };

    let runner = AgentRunner::with_config(Arc::new(MockProvider), config.clone());
    assert_eq!(runner.config.max_response_tokens, 16384);
    assert_eq!(runner.config.temperature, 0.5);
    assert!(runner.config.verbose);
    assert_eq!(runner.config.max_rate_limit_retries, 5);
    assert_eq!(runner.config.base_retry_delay_secs, 4);
}

#[test]
fn test_agent_runner_has_tool_registry() {
    let runner = make_test_runner();
    // The runner should have a tool registry with builtins
    let definitions = runner.tool_registry.definitions();
    assert!(!definitions.is_empty());
}

// ===== ToolOutput tests =====

#[test]
fn test_tool_output_success_is_error() {
    let result = ToolResult {
        tool_use_id: "id".to_string(),
        output: ToolOutput::Success("OK".to_string()),
    };
    assert!(!result.is_error());
}

#[test]
fn test_tool_output_error_is_error() {
    let result = ToolResult {
        tool_use_id: "id".to_string(),
        output: ToolOutput::Error("Failed".to_string()),
    };
    assert!(result.is_error());
}

#[test]
fn test_tool_output_success_text() {
    let result = ToolResult {
        tool_use_id: "id".to_string(),
        output: ToolOutput::Success("Success message".to_string()),
    };
    assert_eq!(result.output_text(), "Success message");
}

#[test]
fn test_tool_output_error_text() {
    let result = ToolResult {
        tool_use_id: "id".to_string(),
        output: ToolOutput::Error("Error message".to_string()),
    };
    assert_eq!(result.output_text(), "Error message");
}

// ===== MockProvider tests =====

#[test]
fn test_mock_provider_name() {
    let provider = MockProvider;
    assert_eq!(provider.name(), "mock");
}

#[test]
fn test_mock_provider_available_models() {
    let provider = MockProvider;
    let models = provider.available_models();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "test-model");
}

#[test]
fn test_mock_provider_supports_model() {
    let provider = MockProvider;
    assert!(provider.supports_model("test-model"));
    assert!(!provider.supports_model("other-model"));
}

#[tokio::test]
async fn test_mock_provider_complete() {
    let provider = MockProvider;
    let request = CompletionRequest {
        model: "test-model".to_string(),
        messages: vec![],
        system: None,
        max_tokens: 1000,
        temperature: 0.7,
        tools: vec![],
        tool_choice: ToolChoice::Auto,
    };

    let response = provider.complete(request).await.unwrap();
    assert_eq!(response.model, "test-model");
    assert!(!response.content.is_empty());
}

#[tokio::test]
async fn test_mock_provider_complete_stream() {
    use futures::StreamExt;

    let provider = MockProvider;
    let request = CompletionRequest {
        model: "test-model".to_string(),
        messages: vec![],
        system: None,
        max_tokens: 1000,
        temperature: 0.7,
        tools: vec![],
        tool_choice: ToolChoice::Auto,
    };

    let mut stream = provider.complete_stream(request).await.unwrap();
    // Stream should be empty for mock
    let mut items = Vec::new();
    while let Some(item) = stream.next().await {
        items.push(item);
    }
    assert!(items.is_empty());
}

#[test]
fn test_mock_provider_count_tokens() {
    let provider = MockProvider;
    let count = provider.count_tokens("some text", "test-model").unwrap();
    assert_eq!(count, 10); // Mock always returns 10
}

// ===== ContentBlockResponse processing tests =====

#[test]
fn test_content_block_response_text_variant() {
    let block = ContentBlockResponse::Text {
        text: "Hello".to_string(),
    };

    if let ContentBlockResponse::Text { text } = block {
        assert_eq!(text, "Hello");
    } else {
        panic!("Expected Text variant");
    }
}

#[test]
fn test_content_block_response_tool_use_variant() {
    let block = ContentBlockResponse::ToolUse {
        id: "123".to_string(),
        name: "tool".to_string(),
        input: serde_json::json!({}),
    };

    if let ContentBlockResponse::ToolUse { id, name, input } = block {
        assert_eq!(id, "123");
        assert_eq!(name, "tool");
        assert!(input.is_object());
    } else {
        panic!("Expected ToolUse variant");
    }
}

// ===== UUID and timestamp tests =====

#[test]
fn test_uuid_formatting() {
    let id = Uuid::new_v4();
    let formatted = id.to_string();
    assert_eq!(formatted.len(), 36); // Standard UUID format
    assert!(formatted.contains('-'));
}

#[test]
fn test_timestamp_is_current() {
    use chrono::Utc;

    let before = Utc::now();
    std::thread::sleep(std::time::Duration::from_millis(1));
    let now = Utc::now();
    std::thread::sleep(std::time::Duration::from_millis(1));
    let after = Utc::now();

    assert!(now > before);
    assert!(now < after);
}

// ==================== AgentRunner::run() Tests ====================

use super::super::context::AgentContext;
use super::super::types::AgentConfig;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Mock provider that returns configurable responses
struct ConfigurableMockProvider {
    /// Number of calls to complete()
    call_count: AtomicUsize,
    /// Stop reason to return
    stop_reason: Option<StopReason>,
    /// Whether to return a tool use
    return_tool_use: bool,
    /// Whether to fail
    should_fail: bool,
}

impl ConfigurableMockProvider {
    fn new() -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            stop_reason: Some(StopReason::EndTurn),
            return_tool_use: false,
            should_fail: false,
        }
    }

    fn with_stop_reason(mut self, reason: StopReason) -> Self {
        self.stop_reason = Some(reason);
        self
    }

    fn with_tool_use(mut self) -> Self {
        self.return_tool_use = true;
        self
    }

    fn failing(mut self) -> Self {
        self.should_fail = true;
        self
    }
}

#[async_trait::async_trait]
impl LlmProvider for ConfigurableMockProvider {
    fn name(&self) -> &str {
        "configurable-mock"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![ModelInfo {
            id: "test-model".to_string(),
            display_name: "Test Model".to_string(),
            context_window: 4096,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_vision: false,
            input_cost_per_1k: 0.0,
            output_cost_per_1k: 0.0,
        }]
    }

    fn supports_model(&self, _model: &str) -> bool {
        true
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> crate::error::Result<crate::llm::provider::CompletionResponse> {
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);

        if self.should_fail {
            return Err(crate::error::TedError::ToolExecution(
                "Mock API failure".to_string(),
            ));
        }

        let content = if self.return_tool_use && count == 0 {
            vec![
                ContentBlockResponse::Text {
                    text: "I'll use a tool".to_string(),
                },
                ContentBlockResponse::ToolUse {
                    id: "tool-123".to_string(),
                    name: "file_read".to_string(),
                    input: serde_json::json!({"path": "/test.txt"}),
                },
            ]
        } else {
            vec![ContentBlockResponse::Text {
                text: format!("Response iteration {}", count),
            }]
        };

        // After first tool use call, return end_turn
        let stop_reason = if self.return_tool_use && count == 0 {
            Some(StopReason::ToolUse)
        } else {
            self.stop_reason
        };

        Ok(crate::llm::provider::CompletionResponse {
            id: format!("response-{}", count),
            model: "test-model".to_string(),
            content,
            usage: crate::llm::provider::Usage {
                input_tokens: 100,
                output_tokens: 50,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
            stop_reason,
        })
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> crate::error::Result<
        Pin<Box<dyn futures::Stream<Item = crate::error::Result<StreamEvent>> + Send>>,
    > {
        Ok(Box::pin(stream::empty()))
    }

    fn count_tokens(&self, _text: &str, _model: &str) -> crate::error::Result<u32> {
        Ok(10)
    }
}

#[tokio::test]
async fn test_run_simple_completion() {
    let provider = Arc::new(ConfigurableMockProvider::new());
    let runner = AgentRunner::new(provider);

    let config = AgentConfig::new("explore", "Find test files", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    assert!(result.success);
    assert!(result.errors.is_empty());
    assert!(result.output.contains("Response"));
}

#[tokio::test]
async fn test_run_exceeded_iterations() {
    let provider =
        Arc::new(ConfigurableMockProvider::new().with_stop_reason(StopReason::MaxTokens));
    let runner = AgentRunner::new(provider);

    // Set max iterations to 2
    let config =
        AgentConfig::new("explore", "Find files", PathBuf::from("/tmp")).with_max_iterations(2);
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    // Should fail due to exceeded iterations
    assert!(!result.success);
    assert!(result
        .errors
        .iter()
        .any(|e| e.contains("Exceeded maximum iterations")));
}

#[tokio::test]
async fn test_run_exceeded_token_budget() {
    let provider =
        Arc::new(ConfigurableMockProvider::new().with_stop_reason(StopReason::MaxTokens));
    let runner = AgentRunner::new(provider);

    // Set very low token budget
    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"))
        .with_token_budget(1) // Very low budget
        .with_max_iterations(100);
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    // Should fail due to exceeded token budget
    assert!(!result.success);
    assert!(result
        .errors
        .iter()
        .any(|e| e.contains("Exceeded token budget")));
}

#[tokio::test]
async fn test_run_api_error() {
    let provider = Arc::new(ConfigurableMockProvider::new().failing());
    let runner = AgentRunner::new(provider);

    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    // Should fail due to API error
    assert!(!result.success);
    assert!(result.errors.iter().any(|e| e.contains("LLM API error")));
}

/// Mock provider that returns rate limit errors for first N calls, then succeeds
struct RateLimitMockProvider {
    /// Number of calls to complete()
    call_count: AtomicUsize,
    /// Number of rate limit errors before success
    rate_limit_count: usize,
    /// Retry-after value to return
    retry_after_secs: u32,
}

impl RateLimitMockProvider {
    fn new(rate_limit_count: usize, retry_after_secs: u32) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            rate_limit_count,
            retry_after_secs,
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for RateLimitMockProvider {
    fn name(&self) -> &str {
        "rate-limit-mock"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![ModelInfo {
            id: "test-model".to_string(),
            display_name: "Test Model".to_string(),
            context_window: 4096,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_vision: false,
            input_cost_per_1k: 0.0,
            output_cost_per_1k: 0.0,
        }]
    }

    fn supports_model(&self, _model: &str) -> bool {
        true
    }

    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> crate::error::Result<crate::llm::provider::CompletionResponse> {
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);

        if count < self.rate_limit_count {
            return Err(TedError::Api(ApiError::RateLimited(self.retry_after_secs)));
        }

        Ok(crate::llm::provider::CompletionResponse {
            id: format!("response-{}", count),
            model: "test-model".to_string(),
            content: vec![ContentBlockResponse::Text {
                text: "Success after rate limit".to_string(),
            }],
            usage: crate::llm::provider::Usage {
                input_tokens: 100,
                output_tokens: 50,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
            stop_reason: Some(StopReason::EndTurn),
        })
    }

    async fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> crate::error::Result<
        Pin<Box<dyn futures::Stream<Item = crate::error::Result<StreamEvent>> + Send>>,
    > {
        Ok(Box::pin(stream::empty()))
    }

    fn count_tokens(&self, _text: &str, _model: &str) -> crate::error::Result<u32> {
        Ok(10)
    }
}

#[tokio::test]
async fn test_run_with_rate_limit_retry_success() {
    // Provider returns 2 rate limit errors, then succeeds
    let provider = Arc::new(RateLimitMockProvider::new(2, 1)); // 1 second retry
    let config_runner = RunnerConfig {
        max_rate_limit_retries: 3, // Allow up to 3 retries
        base_retry_delay_secs: 1,
        ..Default::default()
    };
    let runner = AgentRunner::with_config(provider, config_runner);

    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    // Should succeed after retries
    assert!(result.success);
    assert!(result.output.contains("Success after rate limit"));
}

#[tokio::test]
async fn test_run_with_rate_limit_exhaust_retries() {
    // Provider returns more rate limits than allowed retries
    let provider = Arc::new(RateLimitMockProvider::new(10, 1)); // 10 rate limits
    let config_runner = RunnerConfig {
        max_rate_limit_retries: 2, // Only allow 2 retries
        base_retry_delay_secs: 1,
        ..Default::default()
    };
    let runner = AgentRunner::with_config(provider, config_runner);

    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    // Should fail after exhausting retries
    assert!(!result.success);
    assert!(result.errors.iter().any(|e| e.contains("LLM API error")));
}

#[tokio::test]
async fn test_run_with_tool_use() {
    let provider = Arc::new(ConfigurableMockProvider::new().with_tool_use());
    let runner = AgentRunner::new(provider);

    // Use implement agent type which allows file_read
    let config = AgentConfig::new("implement", "Read a file", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    // Should complete (tool use followed by end_turn)
    assert!(result.success);
    assert!(result.iterations >= 1);
}

#[tokio::test]
async fn test_run_with_verbose_config() {
    let provider = Arc::new(ConfigurableMockProvider::new());
    let config_runner = RunnerConfig {
        max_response_tokens: 4096,
        temperature: 0.7,
        verbose: true,
        ..Default::default()
    };
    let runner = AgentRunner::with_config(provider, config_runner);

    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    assert!(result.success);
}

#[tokio::test]
async fn test_run_with_bead_id() {
    let provider = Arc::new(ConfigurableMockProvider::new());
    let runner = AgentRunner::new(provider);

    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"))
        .with_bead("bead-123".to_string());
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    assert!(result.success);
    assert_eq!(result.bead_id, Some("bead-123".to_string()));
}

#[tokio::test]
async fn test_run_with_custom_model() {
    let provider = Arc::new(ConfigurableMockProvider::new());
    let runner = AgentRunner::new(provider);

    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"))
        .with_model("custom-model".to_string());
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    assert!(result.success);
}

#[tokio::test]
async fn test_run_tracks_iterations() {
    let provider = Arc::new(ConfigurableMockProvider::new());
    let runner = AgentRunner::new(provider);

    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    assert!(result.iterations >= 1);
}

#[tokio::test]
async fn test_run_format_for_parent() {
    let provider = Arc::new(ConfigurableMockProvider::new());
    let runner = AgentRunner::new(provider);

    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    let formatted = result.format_for_parent();
    assert!(formatted.contains("explore"));
    assert!(formatted.contains("Completed") || formatted.contains("Success"));
}

// ==================== get_filtered_tools Tests ====================

#[test]
fn test_get_filtered_tools_explore() {
    let runner = make_test_runner();
    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);

    let tools = runner.get_filtered_tools(&context);

    // Explore agent should have limited tools (read-only)
    // file_read, glob, grep should be present
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(tool_names.contains(&"file_read") || tool_names.contains(&"glob"));
}

#[test]
fn test_get_filtered_tools_implement() {
    let runner = make_test_runner();
    let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);

    let tools = runner.get_filtered_tools(&context);

    // Implement agent should have more tools including write
    let _tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    // Should have both read and write tools
    assert!(!tools.is_empty());
}

// ==================== execute_tools Tests ====================

#[tokio::test]
async fn test_execute_tools_disallowed_tool() {
    let runner = make_test_runner();
    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);
    let tool_context = ToolContext::new(
        PathBuf::from("/tmp"),
        Some(PathBuf::from("/tmp")),
        Uuid::new_v4(),
        true,
    );

    // Try to execute a tool that explore agent doesn't have access to
    let content = vec![ContentBlockResponse::ToolUse {
        id: "tool-1".to_string(),
        name: "shell".to_string(), // Explore doesn't have shell access
        input: serde_json::json!({"command": "ls"}),
    }];

    let results = runner
        .execute_tools(&content, &context, &tool_context, &None)
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert!(results[0].is_error());
    assert!(results[0].output_text().contains("not allowed"));
}

#[tokio::test]
async fn test_execute_tools_unknown_tool() {
    let runner = make_test_runner();
    let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/tmp"));
    // Extend permissions to allow the unknown tool name
    let mut context = AgentContext::new(config);
    context.extend_permissions(&crate::agents::types::ToolPermissions::allow(&[
        "nonexistent_tool_xyz",
    ]));

    let tool_context = ToolContext::new(
        PathBuf::from("/tmp"),
        Some(PathBuf::from("/tmp")),
        Uuid::new_v4(),
        true,
    );

    let content = vec![ContentBlockResponse::ToolUse {
        id: "tool-1".to_string(),
        name: "nonexistent_tool_xyz".to_string(),
        input: serde_json::json!({}),
    }];

    let results = runner
        .execute_tools(&content, &context, &tool_context, &None)
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert!(results[0].is_error());
    assert!(results[0].output_text().contains("Unknown tool"));
}

#[tokio::test]
async fn test_execute_tools_text_block_ignored() {
    let runner = make_test_runner();
    let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);
    let tool_context = ToolContext::new(
        PathBuf::from("/tmp"),
        Some(PathBuf::from("/tmp")),
        Uuid::new_v4(),
        true,
    );

    // Only text blocks, no tool use
    let content = vec![
        ContentBlockResponse::Text {
            text: "Just some text".to_string(),
        },
        ContentBlockResponse::Text {
            text: "More text".to_string(),
        },
    ];

    let results = runner
        .execute_tools(&content, &context, &tool_context, &None)
        .await
        .unwrap();

    // No tool results should be returned
    assert!(results.is_empty());
}

// ==================== track_file_access Tests ====================

#[test]
fn test_track_file_access() {
    let runner = make_test_runner();
    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"));
    let mut context = AgentContext::new(config);

    let result = ToolResult {
        tool_use_id: "tool-1".to_string(),
        output: ToolOutput::Success("File content".to_string()),
    };

    // This is a no-op in the current implementation
    runner.track_file_access(&result, &mut context);

    // Just verify it doesn't panic
    assert!(context.files_read().is_empty());
}

// ==================== BackgroundAgentHandle Tests ====================

#[tokio::test]
async fn test_spawn_background_agent() {
    let provider = Arc::new(ConfigurableMockProvider::new());
    let runner = Arc::new(AgentRunner::new(provider));

    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);

    let handle = spawn_background_agent(runner, context);

    assert!(!handle.name.is_empty());
    assert!(!handle.id.is_nil());

    // Wait for completion
    let result = handle.wait().await.unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn test_background_agent_is_running() {
    let provider = Arc::new(ConfigurableMockProvider::new());
    let runner = Arc::new(AgentRunner::new(provider));

    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"));
    let context = AgentContext::new(config);

    let handle = spawn_background_agent(runner, context);

    // Initially might be running (depending on timing)
    // After wait, it should be finished
    let result = handle.wait().await.unwrap();
    assert!(result.success);
}

// ==================== Memory Strategy Tests ====================

#[tokio::test]
async fn test_run_with_windowed_memory_strategy() {
    let provider = Arc::new(ConfigurableMockProvider::new());
    let runner = AgentRunner::new(provider);

    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"))
        .with_memory_strategy(crate::agents::types::MemoryStrategy::windowed(5));
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    assert!(result.success);
}

#[tokio::test]
async fn test_run_with_summarizing_memory_strategy() {
    let provider = Arc::new(ConfigurableMockProvider::new());
    let runner = AgentRunner::new(provider);

    let config = AgentConfig::new("explore", "Find files", PathBuf::from("/tmp"))
        .with_memory_strategy(crate::agents::types::MemoryStrategy::summarizing());
    let context = AgentContext::new(config);

    let result = runner.run(context).await.unwrap();

    assert!(result.success);
}
