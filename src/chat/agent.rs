// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Agent loop handling for chat
//!
//! This module provides the core agent loop logic for processing LLM responses
//! and tool executions. The code is structured to maximize testability.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::llm::message::{ContentBlock, Message, ToolResultContent};
use crate::llm::provider::{CompletionRequest, ContentBlockResponse, StopReason, ToolDefinition};

/// Configuration for the agent loop
#[derive(Debug, Clone)]
pub struct AgentLoopConfig {
    /// Maximum tokens for response
    pub max_tokens: u32,
    /// Temperature for sampling
    pub temperature: f32,
    /// Whether to use streaming
    pub stream: bool,
    /// Maximum consecutive identical tool calls before loop detection
    pub max_consecutive_identical_calls: usize,
    /// Maximum number of retries for rate limits
    pub max_retries: u32,
    /// Base delay for exponential backoff (seconds)
    pub base_retry_delay: u64,
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            max_tokens: 8192,
            temperature: 0.7,
            stream: true,
            max_consecutive_identical_calls: 2,
            max_retries: 3,
            base_retry_delay: 2,
        }
    }
}

/// Represents a detected tool use loop
#[derive(Debug, Clone)]
pub struct LoopDetection {
    pub tool_name: String,
    pub consecutive_count: usize,
}

/// Track recent tool calls for loop detection
#[derive(Debug, Default)]
pub struct ToolCallTracker {
    /// Recent tool calls as (tool_name, serialized_input)
    recent_calls: Vec<(String, String)>,
    /// Maximum calls to track
    max_tracked: usize,
}

impl ToolCallTracker {
    pub fn new(max_tracked: usize) -> Self {
        Self {
            recent_calls: Vec::new(),
            max_tracked,
        }
    }

    /// Check if the given call would be a loop
    pub fn check_loop(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
        max_consecutive: usize,
    ) -> Option<LoopDetection> {
        let input_str = serde_json::to_string(input).unwrap_or_default();
        let current_call = (tool_name.to_string(), input_str);

        // Count consecutive identical calls at the end
        let consecutive_matches = self
            .recent_calls
            .iter()
            .rev()
            .take_while(|call| *call == &current_call)
            .count();

        if consecutive_matches >= max_consecutive {
            Some(LoopDetection {
                tool_name: tool_name.to_string(),
                consecutive_count: consecutive_matches + 1,
            })
        } else {
            None
        }
    }

    /// Track a new tool call
    pub fn track(&mut self, tool_name: &str, input: &serde_json::Value) {
        let input_str = serde_json::to_string(input).unwrap_or_default();
        self.recent_calls.push((tool_name.to_string(), input_str));

        // Keep only the most recent calls
        if self.recent_calls.len() > self.max_tracked {
            self.recent_calls.remove(0);
        }
    }

    /// Clear the tracker (e.g., after loop detection)
    pub fn clear(&mut self) {
        self.recent_calls.clear();
    }
}

/// Convert response content blocks to message content blocks
pub fn response_to_message_blocks(response_content: &[ContentBlockResponse]) -> Vec<ContentBlock> {
    response_content
        .iter()
        .map(|block| match block {
            ContentBlockResponse::Text { text } => ContentBlock::Text { text: text.clone() },
            ContentBlockResponse::ToolUse { id, name, input } => ContentBlock::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            },
        })
        .collect()
}

/// Extract tool use requests from response content
pub fn extract_tool_uses(
    response_content: &[ContentBlockResponse],
) -> Vec<(String, String, serde_json::Value)> {
    response_content
        .iter()
        .filter_map(|block| {
            if let ContentBlockResponse::ToolUse { id, name, input } = block {
                Some((id.clone(), name.clone(), input.clone()))
            } else {
                None
            }
        })
        .collect()
}

/// Normalize tool input to an object-like JSON value.
///
/// Some providers may emit tool input as a JSON string (streaming assembly)
/// or null when empty; normalize these to parsed JSON / empty object.
pub fn normalize_tool_use_input(input: &serde_json::Value) -> serde_json::Value {
    match input {
        serde_json::Value::String(s) => {
            serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!({}))
        }
        serde_json::Value::Null => serde_json::json!({}),
        _ => input.clone(),
    }
}

/// Extract tool use requests with normalized input payloads.
pub fn extract_tool_uses_normalized(
    response_content: &[ContentBlockResponse],
) -> Vec<(String, String, serde_json::Value)> {
    response_content
        .iter()
        .filter_map(|block| {
            if let ContentBlockResponse::ToolUse { id, name, input } = block {
                Some((id.clone(), name.clone(), normalize_tool_use_input(input)))
            } else {
                None
            }
        })
        .collect()
}

/// Extract text content from response
pub fn extract_text_content(response_content: &[ContentBlockResponse]) -> String {
    response_content
        .iter()
        .filter_map(|block| {
            if let ContentBlockResponse::Text { text } = block {
                Some(text.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Check if the response indicates tool use should continue
pub fn should_continue_tool_loop(
    tool_uses: &[(String, String, serde_json::Value)],
    stop_reason: Option<StopReason>,
) -> bool {
    !tool_uses.is_empty() || stop_reason == Some(StopReason::ToolUse)
}

/// Build a completion request from conversation state
pub fn build_completion_request(
    model: &str,
    messages: Vec<Message>,
    system_prompt: Option<&str>,
    tools: Vec<ToolDefinition>,
    config: &AgentLoopConfig,
) -> CompletionRequest {
    let mut request = CompletionRequest::new(model, messages)
        .with_max_tokens(config.max_tokens)
        .with_temperature(config.temperature)
        .with_tools(tools);

    if let Some(system) = system_prompt {
        request = request.with_system(system);
    }

    request
}

/// Calculate retry delay for rate limiting
pub fn calculate_retry_delay(retry_after: i32, attempt: u32, base_delay: u64) -> u64 {
    if retry_after > 0 {
        retry_after as u64
    } else {
        base_delay.pow(attempt)
    }
}

/// Check if an interrupt signal has been received
pub fn is_interrupted(flag: &Arc<AtomicBool>) -> bool {
    flag.load(Ordering::SeqCst)
}

/// Generate a loop detection error message
pub fn format_loop_error(tool_name: &str, count: usize) -> String {
    format!(
        "LOOP DETECTED: You have called '{}' {} times in a row with the same arguments. \
        This appears to be a loop. Please try a DIFFERENT approach or tool. \
        If you were searching, try reading a specific file instead. \
        If you need more information, try asking the user for clarification.",
        tool_name, count
    )
}

/// Build a tool_result content block.
pub fn tool_result_block(
    tool_use_id: impl Into<String>,
    output: impl Into<String>,
    is_error: bool,
) -> ContentBlock {
    ContentBlock::ToolResult {
        tool_use_id: tool_use_id.into(),
        content: ToolResultContent::Text(output.into()),
        is_error: if is_error { Some(true) } else { None },
    }
}

/// Calculate target token count for context trimming (70% of window)
pub fn calculate_trim_target(context_window: u32) -> u32 {
    (context_window as f64 * 0.7) as u32
}

/// Check if conversation needs trimming based on threshold
pub fn needs_trimming(estimated_tokens: u32, context_window: u32, threshold: f64) -> bool {
    let threshold_tokens = (context_window as f64 * threshold) as u32;
    estimated_tokens > threshold_tokens
}

/// Represents the state of an agent loop iteration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentLoopState {
    /// Continue processing (more tool calls expected)
    Continue,
    /// Loop completed successfully
    Complete,
    /// Loop was interrupted
    Interrupted,
    /// Loop detected and broken
    LoopBroken,
}

/// Result of processing a single tool call
#[derive(Debug)]
pub struct ToolCallResult {
    pub tool_use_id: String,
    pub is_error: bool,
    pub output: String,
}

impl ToolCallResult {
    pub fn success(tool_use_id: String, output: String) -> Self {
        Self {
            tool_use_id,
            is_error: false,
            output,
        }
    }

    pub fn error(tool_use_id: String, error: String) -> Self {
        Self {
            tool_use_id,
            is_error: true,
            output: error,
        }
    }
}

/// Convert tool call results to message content blocks
pub fn results_to_content_blocks(results: Vec<ToolCallResult>) -> Vec<ContentBlock> {
    results
        .into_iter()
        .map(|r| tool_result_block(r.tool_use_id, r.output, r.is_error))
        .collect()
}

/// Create an assistant message from response content
pub fn create_assistant_message(content_blocks: Vec<ContentBlock>) -> Message {
    Message::assistant_blocks(content_blocks)
}

/// Create a user message containing tool results
pub fn create_tool_result_message(result_blocks: Vec<ContentBlock>) -> Message {
    Message::user_blocks(result_blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== AgentLoopConfig tests ====================

    #[test]
    fn test_agent_loop_config_default() {
        let config = AgentLoopConfig::default();
        assert_eq!(config.max_tokens, 8192);
        assert!((config.temperature - 0.7).abs() < 0.001);
        assert!(config.stream);
        assert_eq!(config.max_consecutive_identical_calls, 2);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_retry_delay, 2);
    }

    #[test]
    fn test_agent_loop_config_custom() {
        let config = AgentLoopConfig {
            max_tokens: 4096,
            temperature: 0.5,
            stream: false,
            max_consecutive_identical_calls: 3,
            max_retries: 5,
            base_retry_delay: 1,
        };
        assert_eq!(config.max_tokens, 4096);
        assert!(!config.stream);
    }

    // ==================== ToolCallTracker tests ====================

    #[test]
    fn test_tool_call_tracker_new() {
        let tracker = ToolCallTracker::new(10);
        assert!(tracker.recent_calls.is_empty());
        assert_eq!(tracker.max_tracked, 10);
    }

    #[test]
    fn test_tool_call_tracker_track() {
        let mut tracker = ToolCallTracker::new(10);
        let input = serde_json::json!({"path": "/test"});
        tracker.track("file_read", &input);
        assert_eq!(tracker.recent_calls.len(), 1);
    }

    #[test]
    fn test_tool_call_tracker_max_tracked() {
        let mut tracker = ToolCallTracker::new(3);
        for i in 0..5 {
            let input = serde_json::json!({"index": i});
            tracker.track("test", &input);
        }
        assert_eq!(tracker.recent_calls.len(), 3);
    }

    #[test]
    fn test_tool_call_tracker_check_loop_no_loop() {
        let mut tracker = ToolCallTracker::new(10);
        let input1 = serde_json::json!({"path": "/file1"});
        let input2 = serde_json::json!({"path": "/file2"});
        tracker.track("file_read", &input1);

        let result = tracker.check_loop("file_read", &input2, 2);
        assert!(result.is_none());
    }

    #[test]
    fn test_tool_call_tracker_check_loop_detected() {
        let mut tracker = ToolCallTracker::new(10);
        let input = serde_json::json!({"path": "/same/file"});

        tracker.track("file_read", &input);
        tracker.track("file_read", &input);

        let result = tracker.check_loop("file_read", &input, 2);
        assert!(result.is_some());
        let detection = result.unwrap();
        assert_eq!(detection.tool_name, "file_read");
        assert_eq!(detection.consecutive_count, 3);
    }

    #[test]
    fn test_tool_call_tracker_check_loop_different_tool() {
        let mut tracker = ToolCallTracker::new(10);
        let input = serde_json::json!({"path": "/same/file"});

        tracker.track("file_read", &input);
        tracker.track("file_read", &input);

        // Different tool name should not trigger loop
        let result = tracker.check_loop("file_write", &input, 2);
        assert!(result.is_none());
    }

    #[test]
    fn test_tool_call_tracker_clear() {
        let mut tracker = ToolCallTracker::new(10);
        let input = serde_json::json!({});
        tracker.track("test", &input);
        tracker.track("test", &input);

        tracker.clear();
        assert!(tracker.recent_calls.is_empty());
    }

    // ==================== extract_tool_uses tests ====================

    #[test]
    fn test_extract_tool_uses_empty() {
        let content: Vec<ContentBlockResponse> = vec![];
        let uses = extract_tool_uses(&content);
        assert!(uses.is_empty());
    }

    #[test]
    fn test_extract_tool_uses_text_only() {
        let content = vec![ContentBlockResponse::Text {
            text: "Hello".to_string(),
        }];
        let uses = extract_tool_uses(&content);
        assert!(uses.is_empty());
    }

    #[test]
    fn test_extract_tool_uses_single() {
        let content = vec![ContentBlockResponse::ToolUse {
            id: "tool_1".to_string(),
            name: "file_read".to_string(),
            input: serde_json::json!({"path": "/test"}),
        }];
        let uses = extract_tool_uses(&content);
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].0, "tool_1");
        assert_eq!(uses[0].1, "file_read");
    }

    #[test]
    fn test_extract_tool_uses_multiple() {
        let content = vec![
            ContentBlockResponse::Text {
                text: "Let me help".to_string(),
            },
            ContentBlockResponse::ToolUse {
                id: "tool_1".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({}),
            },
            ContentBlockResponse::ToolUse {
                id: "tool_2".to_string(),
                name: "shell".to_string(),
                input: serde_json::json!({}),
            },
        ];
        let uses = extract_tool_uses(&content);
        assert_eq!(uses.len(), 2);
    }

    // ==================== extract_text_content tests ====================

    #[test]
    fn test_extract_text_content_empty() {
        let content: Vec<ContentBlockResponse> = vec![];
        let text = extract_text_content(&content);
        assert!(text.is_empty());
    }

    #[test]
    fn test_extract_text_content_single() {
        let content = vec![ContentBlockResponse::Text {
            text: "Hello world".to_string(),
        }];
        let text = extract_text_content(&content);
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn test_extract_text_content_multiple() {
        let content = vec![
            ContentBlockResponse::Text {
                text: "First".to_string(),
            },
            ContentBlockResponse::Text {
                text: "Second".to_string(),
            },
        ];
        let text = extract_text_content(&content);
        assert_eq!(text, "First\nSecond");
    }

    #[test]
    fn test_extract_text_content_mixed() {
        let content = vec![
            ContentBlockResponse::Text {
                text: "Before".to_string(),
            },
            ContentBlockResponse::ToolUse {
                id: "t1".to_string(),
                name: "test".to_string(),
                input: serde_json::json!({}),
            },
            ContentBlockResponse::Text {
                text: "After".to_string(),
            },
        ];
        let text = extract_text_content(&content);
        assert_eq!(text, "Before\nAfter");
    }

    // ==================== should_continue_tool_loop tests ====================

    #[test]
    fn test_should_continue_tool_loop_with_tools() {
        let tools = vec![("t1".to_string(), "test".to_string(), serde_json::json!({}))];
        assert!(should_continue_tool_loop(&tools, None));
    }

    #[test]
    fn test_should_continue_tool_loop_with_stop_reason() {
        let tools: Vec<(String, String, serde_json::Value)> = vec![];
        assert!(should_continue_tool_loop(&tools, Some(StopReason::ToolUse)));
    }

    #[test]
    fn test_should_continue_tool_loop_end_turn() {
        let tools: Vec<(String, String, serde_json::Value)> = vec![];
        assert!(!should_continue_tool_loop(
            &tools,
            Some(StopReason::EndTurn)
        ));
    }

    #[test]
    fn test_should_continue_tool_loop_no_reason() {
        let tools: Vec<(String, String, serde_json::Value)> = vec![];
        assert!(!should_continue_tool_loop(&tools, None));
    }

    // ==================== calculate_retry_delay tests ====================

    #[test]
    fn test_calculate_retry_delay_with_retry_after() {
        assert_eq!(calculate_retry_delay(10, 1, 2), 10);
        assert_eq!(calculate_retry_delay(30, 3, 5), 30);
    }

    #[test]
    fn test_calculate_retry_delay_exponential() {
        assert_eq!(calculate_retry_delay(0, 1, 2), 2);
        assert_eq!(calculate_retry_delay(0, 2, 2), 4);
        assert_eq!(calculate_retry_delay(0, 3, 2), 8);
    }

    #[test]
    fn test_calculate_retry_delay_zero_retry_after() {
        assert_eq!(calculate_retry_delay(-1, 1, 3), 3);
    }

    // ==================== is_interrupted tests ====================

    #[test]
    fn test_is_interrupted_false() {
        let flag = Arc::new(AtomicBool::new(false));
        assert!(!is_interrupted(&flag));
    }

    #[test]
    fn test_is_interrupted_true() {
        let flag = Arc::new(AtomicBool::new(true));
        assert!(is_interrupted(&flag));
    }

    #[test]
    fn test_is_interrupted_changed() {
        let flag = Arc::new(AtomicBool::new(false));
        assert!(!is_interrupted(&flag));
        flag.store(true, Ordering::SeqCst);
        assert!(is_interrupted(&flag));
    }

    // ==================== format_loop_error tests ====================

    #[test]
    fn test_format_loop_error() {
        let error = format_loop_error("file_read", 3);
        assert!(error.contains("LOOP DETECTED"));
        assert!(error.contains("file_read"));
        assert!(error.contains("3 times"));
        assert!(error.contains("DIFFERENT approach"));
    }

    #[test]
    fn test_tool_result_block() {
        let block = tool_result_block("tool_1", "ok", false);
        match block {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                assert_eq!(tool_use_id, "tool_1");
                assert!(matches!(content, ToolResultContent::Text(ref t) if t == "ok"));
                assert_eq!(is_error, None);
            }
            _ => panic!("expected tool result block"),
        }
    }

    #[test]
    fn test_tool_result_block_error() {
        let block = tool_result_block("tool_2", "failed", true);
        match block {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                assert_eq!(tool_use_id, "tool_2");
                assert!(matches!(content, ToolResultContent::Text(ref t) if t == "failed"));
                assert_eq!(is_error, Some(true));
            }
            _ => panic!("expected tool result block"),
        }
    }

    #[test]
    fn test_normalize_tool_use_input() {
        let parsed = normalize_tool_use_input(&serde_json::Value::String(
            "{\"path\":\"/tmp/a\"}".to_string(),
        ));
        assert_eq!(parsed["path"], "/tmp/a");

        let null_normalized = normalize_tool_use_input(&serde_json::Value::Null);
        assert_eq!(null_normalized, serde_json::json!({}));
    }

    #[test]
    fn test_extract_tool_uses_normalized() {
        let response_content = vec![ContentBlockResponse::ToolUse {
            id: "tool_1".to_string(),
            name: "file_read".to_string(),
            input: serde_json::Value::String("{\"path\":\"/tmp/a\"}".to_string()),
        }];

        let tool_uses = extract_tool_uses_normalized(&response_content);
        assert_eq!(tool_uses.len(), 1);
        assert_eq!(tool_uses[0].2["path"], "/tmp/a");
    }

    // ==================== calculate_trim_target tests ====================

    #[test]
    fn test_calculate_trim_target() {
        assert_eq!(calculate_trim_target(100000), 70000);
        assert_eq!(calculate_trim_target(200000), 140000);
        assert_eq!(calculate_trim_target(0), 0);
    }

    #[test]
    fn test_calculate_trim_target_small() {
        assert_eq!(calculate_trim_target(100), 70);
        assert_eq!(calculate_trim_target(10), 7);
    }

    // ==================== needs_trimming tests ====================

    #[test]
    fn test_needs_trimming_below_threshold() {
        assert!(!needs_trimming(50000, 100000, 0.8));
    }

    #[test]
    fn test_needs_trimming_above_threshold() {
        assert!(needs_trimming(85000, 100000, 0.8));
    }

    #[test]
    fn test_needs_trimming_exact_threshold() {
        // At exactly the threshold, should not need trimming
        assert!(!needs_trimming(80000, 100000, 0.8));
    }

    // ==================== ToolCallResult tests ====================

    #[test]
    fn test_tool_call_result_success() {
        let result = ToolCallResult::success("t1".to_string(), "output".to_string());
        assert_eq!(result.tool_use_id, "t1");
        assert!(!result.is_error);
        assert_eq!(result.output, "output");
    }

    #[test]
    fn test_tool_call_result_error() {
        let result = ToolCallResult::error("t1".to_string(), "error msg".to_string());
        assert_eq!(result.tool_use_id, "t1");
        assert!(result.is_error);
        assert_eq!(result.output, "error msg");
    }

    // ==================== AgentLoopState tests ====================

    #[test]
    fn test_agent_loop_state_eq() {
        assert_eq!(AgentLoopState::Continue, AgentLoopState::Continue);
        assert_eq!(AgentLoopState::Complete, AgentLoopState::Complete);
        assert_ne!(AgentLoopState::Continue, AgentLoopState::Complete);
    }

    #[test]
    fn test_agent_loop_state_debug() {
        let state = AgentLoopState::Interrupted;
        let debug = format!("{:?}", state);
        assert!(debug.contains("Interrupted"));
    }

    // ==================== response_to_message_blocks tests ====================

    #[test]
    fn test_response_to_message_blocks_empty() {
        let content: Vec<ContentBlockResponse> = vec![];
        let blocks = response_to_message_blocks(&content);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_response_to_message_blocks_text() {
        let content = vec![ContentBlockResponse::Text {
            text: "Hello".to_string(),
        }];
        let blocks = response_to_message_blocks(&content);
        assert_eq!(blocks.len(), 1);
        if let ContentBlock::Text { text } = &blocks[0] {
            assert_eq!(text, "Hello");
        } else {
            panic!("Expected Text block");
        }
    }

    #[test]
    fn test_response_to_message_blocks_tool_use() {
        let content = vec![ContentBlockResponse::ToolUse {
            id: "t1".to_string(),
            name: "test".to_string(),
            input: serde_json::json!({"key": "value"}),
        }];
        let blocks = response_to_message_blocks(&content);
        assert_eq!(blocks.len(), 1);
        if let ContentBlock::ToolUse { id, name, input } = &blocks[0] {
            assert_eq!(id, "t1");
            assert_eq!(name, "test");
            assert_eq!(input["key"], "value");
        } else {
            panic!("Expected ToolUse block");
        }
    }

    // ==================== results_to_content_blocks tests ====================

    #[test]
    fn test_results_to_content_blocks_empty() {
        let results: Vec<ToolCallResult> = vec![];
        let blocks = results_to_content_blocks(results);
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_results_to_content_blocks_success() {
        let results = vec![ToolCallResult::success(
            "t1".to_string(),
            "output".to_string(),
        )];
        let blocks = results_to_content_blocks(results);
        assert_eq!(blocks.len(), 1);
        if let ContentBlock::ToolResult {
            tool_use_id,
            is_error,
            ..
        } = &blocks[0]
        {
            assert_eq!(tool_use_id, "t1");
            assert!(is_error.is_none());
        } else {
            panic!("Expected ToolResult block");
        }
    }

    #[test]
    fn test_results_to_content_blocks_error() {
        let results = vec![ToolCallResult::error("t1".to_string(), "error".to_string())];
        let blocks = results_to_content_blocks(results);
        assert_eq!(blocks.len(), 1);
        if let ContentBlock::ToolResult { is_error, .. } = &blocks[0] {
            assert_eq!(*is_error, Some(true));
        } else {
            panic!("Expected ToolResult block");
        }
    }

    // ==================== build_completion_request tests ====================

    #[test]
    fn test_build_completion_request_basic() {
        let messages = vec![];
        let config = AgentLoopConfig::default();
        let request = build_completion_request("test-model", messages, None, vec![], &config);
        assert_eq!(request.model, "test-model");
    }

    #[test]
    fn test_build_completion_request_with_system() {
        let messages = vec![];
        let config = AgentLoopConfig::default();
        let request = build_completion_request(
            "test-model",
            messages,
            Some("You are helpful"),
            vec![],
            &config,
        );
        assert_eq!(request.system, Some("You are helpful".to_string()));
    }

    // ==================== create_assistant_message tests ====================

    #[test]
    fn test_create_assistant_message() {
        let blocks = vec![ContentBlock::Text {
            text: "Hello".to_string(),
        }];
        let msg = create_assistant_message(blocks);
        assert_eq!(msg.role, crate::llm::message::Role::Assistant);
    }

    // ==================== create_tool_result_message tests ====================

    #[test]
    fn test_create_tool_result_message() {
        let blocks = vec![];
        let msg = create_tool_result_message(blocks);
        assert_eq!(msg.role, crate::llm::message::Role::User);
    }

    // ==================== Edge cases ====================

    #[test]
    fn test_tracker_with_empty_input() {
        let mut tracker = ToolCallTracker::new(10);
        let input = serde_json::json!(null);
        tracker.track("test", &input);
        assert_eq!(tracker.recent_calls.len(), 1);
    }

    #[test]
    fn test_tracker_with_complex_input() {
        let mut tracker = ToolCallTracker::new(10);
        let input = serde_json::json!({
            "nested": {"deeply": {"value": [1, 2, 3]}},
            "array": [{"a": 1}, {"b": 2}]
        });
        tracker.track("test", &input);

        // Same complex input should be detected
        let result = tracker.check_loop("test", &input, 1);
        assert!(result.is_some());
    }
}
