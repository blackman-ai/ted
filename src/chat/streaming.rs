// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Streaming response handling
//!
//! This module provides testable logic for processing streaming LLM responses.
//! It separates the stream processing logic from the actual I/O operations.

use crate::llm::provider::{ContentBlockDelta, ContentBlockResponse, StopReason, StreamEvent};

/// Accumulator for streaming response content
#[derive(Debug, Default)]
pub struct StreamAccumulator {
    /// Accumulated content blocks
    content_blocks: Vec<ContentBlockResponse>,
    /// Current text being accumulated
    current_text: String,
    /// Current tool input JSON being accumulated
    current_tool_input: String,
    /// Stop reason from the stream
    stop_reason: Option<StopReason>,
    /// Whether any text has been output
    has_text_output: bool,
}

impl StreamAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the accumulated content blocks
    pub fn content_blocks(&self) -> &[ContentBlockResponse] {
        &self.content_blocks
    }

    /// Get the stop reason
    pub fn stop_reason(&self) -> Option<StopReason> {
        self.stop_reason
    }

    /// Check if any text has been output
    pub fn has_text_output(&self) -> bool {
        self.has_text_output
    }

    /// Process a stream event and return any text to display
    pub fn process_event(&mut self, event: StreamEvent) -> StreamEventResult {
        match event {
            StreamEvent::ContentBlockStart { content_block, .. } => {
                match &content_block {
                    ContentBlockResponse::Text { .. } => {
                        self.current_text.clear();
                    }
                    ContentBlockResponse::ToolUse { .. } => {
                        self.current_tool_input.clear();
                    }
                }
                self.content_blocks.push(content_block);
                StreamEventResult::BlockStarted
            }
            StreamEvent::ContentBlockDelta { index, delta } => match delta {
                ContentBlockDelta::TextDelta { text } => {
                    self.current_text.push_str(&text);
                    self.has_text_output = true;

                    // Update the content block
                    if let Some(ContentBlockResponse::Text { text: block_text }) =
                        self.content_blocks.get_mut(index)
                    {
                        block_text.push_str(&self.current_text);
                        self.current_text.clear();
                    }

                    StreamEventResult::TextDelta(text)
                }
                ContentBlockDelta::InputJsonDelta { partial_json } => {
                    self.current_tool_input.push_str(&partial_json);
                    StreamEventResult::ToolInputDelta
                }
            },
            StreamEvent::ContentBlockStop { index } => {
                // Finalize the content block
                if let Some(block) = self.content_blocks.get_mut(index) {
                    match block {
                        ContentBlockResponse::Text { text } => {
                            if !self.current_text.is_empty() {
                                text.push_str(&self.current_text);
                                self.current_text.clear();
                            }
                        }
                        ContentBlockResponse::ToolUse { input, .. } => {
                            if !self.current_tool_input.is_empty() {
                                if let Ok(parsed) = serde_json::from_str(&self.current_tool_input) {
                                    *input = parsed;
                                }
                                self.current_tool_input.clear();
                            }
                        }
                    }
                }
                StreamEventResult::BlockStopped
            }
            StreamEvent::MessageDelta {
                stop_reason: sr, ..
            } => {
                self.stop_reason = sr;
                StreamEventResult::MessageDelta(sr)
            }
            StreamEvent::MessageStop => StreamEventResult::MessageStop,
            StreamEvent::Error {
                error_type,
                message,
            } => StreamEventResult::Error {
                error_type,
                message,
            },
            StreamEvent::Ping => StreamEventResult::Ping,
            StreamEvent::MessageStart { .. } => StreamEventResult::MessageStart,
        }
    }

    /// Consume the accumulator and return the final results
    pub fn finish(self) -> (Vec<ContentBlockResponse>, Option<StopReason>) {
        (self.content_blocks, self.stop_reason)
    }
}

/// Result of processing a stream event
#[derive(Debug, Clone)]
pub enum StreamEventResult {
    /// A new content block started
    BlockStarted,
    /// Text delta received (contains the text to display)
    TextDelta(String),
    /// Tool input JSON delta received
    ToolInputDelta,
    /// A content block stopped
    BlockStopped,
    /// Message delta with optional stop reason
    MessageDelta(Option<StopReason>),
    /// Message stopped
    MessageStop,
    /// Error occurred
    Error { error_type: String, message: String },
    /// Ping event (keep-alive)
    Ping,
    /// Message started
    MessageStart,
}

impl StreamEventResult {
    /// Check if this result contains displayable text
    pub fn text(&self) -> Option<&str> {
        match self {
            StreamEventResult::TextDelta(text) => Some(text),
            _ => None,
        }
    }

    /// Check if this is an error
    pub fn is_error(&self) -> bool {
        matches!(self, StreamEventResult::Error { .. })
    }

    /// Get error details if this is an error
    pub fn error(&self) -> Option<(&str, &str)> {
        match self {
            StreamEventResult::Error {
                error_type,
                message,
            } => Some((error_type, message)),
            _ => None,
        }
    }
}

/// Statistics about a streaming response
#[derive(Debug, Clone, Default)]
pub struct StreamStats {
    /// Total text characters received
    pub total_text_chars: usize,
    /// Number of text deltas received
    pub text_delta_count: usize,
    /// Number of tool uses in the response
    pub tool_use_count: usize,
    /// Number of content blocks
    pub content_block_count: usize,
}

impl StreamStats {
    /// Update stats from a stream event result
    pub fn update(&mut self, result: &StreamEventResult) {
        match result {
            StreamEventResult::TextDelta(text) => {
                self.total_text_chars += text.len();
                self.text_delta_count += 1;
            }
            StreamEventResult::BlockStarted => {
                self.content_block_count += 1;
            }
            _ => {}
        }
    }

    /// Update stats from final content blocks
    pub fn finalize(&mut self, content_blocks: &[ContentBlockResponse]) {
        self.tool_use_count = content_blocks
            .iter()
            .filter(|b| matches!(b, ContentBlockResponse::ToolUse { .. }))
            .count();
    }
}

/// Builder for simulating stream events in tests
#[cfg(test)]
pub struct StreamEventBuilder;

#[cfg(test)]
impl StreamEventBuilder {
    /// Create a text block start event
    pub fn text_block_start(text: &str) -> StreamEvent {
        StreamEvent::ContentBlockStart {
            index: 0,
            content_block: ContentBlockResponse::Text {
                text: text.to_string(),
            },
        }
    }

    /// Create a tool use block start event
    pub fn tool_use_start(id: &str, name: &str) -> StreamEvent {
        StreamEvent::ContentBlockStart {
            index: 0,
            content_block: ContentBlockResponse::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input: serde_json::json!({}),
            },
        }
    }

    /// Create a text delta event
    pub fn text_delta(index: usize, text: &str) -> StreamEvent {
        StreamEvent::ContentBlockDelta {
            index,
            delta: ContentBlockDelta::TextDelta {
                text: text.to_string(),
            },
        }
    }

    /// Create a tool input delta event
    pub fn input_delta(index: usize, json: &str) -> StreamEvent {
        StreamEvent::ContentBlockDelta {
            index,
            delta: ContentBlockDelta::InputJsonDelta {
                partial_json: json.to_string(),
            },
        }
    }

    /// Create a block stop event
    pub fn block_stop(index: usize) -> StreamEvent {
        StreamEvent::ContentBlockStop { index }
    }

    /// Create a message delta event with stop reason
    pub fn message_delta(stop_reason: Option<StopReason>) -> StreamEvent {
        StreamEvent::MessageDelta {
            stop_reason,
            usage: None,
        }
    }

    /// Create a message stop event
    pub fn message_stop() -> StreamEvent {
        StreamEvent::MessageStop
    }

    /// Create an error event
    pub fn error(error_type: &str, message: &str) -> StreamEvent {
        StreamEvent::Error {
            error_type: error_type.to_string(),
            message: message.to_string(),
        }
    }

    /// Create a ping event
    pub fn ping() -> StreamEvent {
        StreamEvent::Ping
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== StreamAccumulator tests ====================

    #[test]
    fn test_stream_accumulator_new() {
        let acc = StreamAccumulator::new();
        assert!(acc.content_blocks().is_empty());
        assert!(acc.stop_reason().is_none());
        assert!(!acc.has_text_output());
    }

    #[test]
    fn test_stream_accumulator_text_block() {
        let mut acc = StreamAccumulator::new();

        // Start a text block
        let result = acc.process_event(StreamEventBuilder::text_block_start(""));
        assert!(matches!(result, StreamEventResult::BlockStarted));

        // Add text deltas
        let result = acc.process_event(StreamEventBuilder::text_delta(0, "Hello "));
        assert!(matches!(result, StreamEventResult::TextDelta(_)));
        if let StreamEventResult::TextDelta(text) = result {
            assert_eq!(text, "Hello ");
        }

        let result = acc.process_event(StreamEventBuilder::text_delta(0, "World"));
        if let StreamEventResult::TextDelta(text) = result {
            assert_eq!(text, "World");
        }

        // Stop the block
        let result = acc.process_event(StreamEventBuilder::block_stop(0));
        assert!(matches!(result, StreamEventResult::BlockStopped));

        // Verify final content
        let (blocks, _) = acc.finish();
        assert_eq!(blocks.len(), 1);
        if let ContentBlockResponse::Text { text } = &blocks[0] {
            assert_eq!(text, "Hello World");
        } else {
            panic!("Expected Text block");
        }
    }

    #[test]
    fn test_stream_accumulator_tool_use_block() {
        let mut acc = StreamAccumulator::new();

        // Start a tool use block
        acc.process_event(StreamEventBuilder::tool_use_start("t1", "file_read"));

        // Add JSON input deltas
        acc.process_event(StreamEventBuilder::input_delta(0, "{\"path\":"));
        acc.process_event(StreamEventBuilder::input_delta(0, "\"/test\"}"));

        // Stop the block
        acc.process_event(StreamEventBuilder::block_stop(0));

        // Verify final content
        let (blocks, _) = acc.finish();
        assert_eq!(blocks.len(), 1);
        if let ContentBlockResponse::ToolUse { id, name, input } = &blocks[0] {
            assert_eq!(id, "t1");
            assert_eq!(name, "file_read");
            assert_eq!(input["path"], "/test");
        } else {
            panic!("Expected ToolUse block");
        }
    }

    #[test]
    fn test_stream_accumulator_stop_reason() {
        let mut acc = StreamAccumulator::new();

        // Process message delta with stop reason
        let result =
            acc.process_event(StreamEventBuilder::message_delta(Some(StopReason::EndTurn)));
        if let StreamEventResult::MessageDelta(reason) = result {
            assert_eq!(reason, Some(StopReason::EndTurn));
        }

        assert_eq!(acc.stop_reason(), Some(StopReason::EndTurn));
    }

    #[test]
    fn test_stream_accumulator_error() {
        let mut acc = StreamAccumulator::new();

        let result =
            acc.process_event(StreamEventBuilder::error("rate_limit", "Too many requests"));
        assert!(matches!(result, StreamEventResult::Error { .. }));
        if let StreamEventResult::Error {
            error_type,
            message,
        } = result
        {
            assert_eq!(error_type, "rate_limit");
            assert_eq!(message, "Too many requests");
        }
    }

    #[test]
    fn test_stream_accumulator_ping() {
        let mut acc = StreamAccumulator::new();
        let result = acc.process_event(StreamEventBuilder::ping());
        assert!(matches!(result, StreamEventResult::Ping));
    }

    #[test]
    fn test_stream_accumulator_has_text_output() {
        let mut acc = StreamAccumulator::new();
        assert!(!acc.has_text_output());

        acc.process_event(StreamEventBuilder::text_block_start(""));
        assert!(!acc.has_text_output());

        acc.process_event(StreamEventBuilder::text_delta(0, "text"));
        assert!(acc.has_text_output());
    }

    #[test]
    fn test_stream_accumulator_multiple_blocks() {
        let mut acc = StreamAccumulator::new();

        // First text block
        acc.process_event(StreamEvent::ContentBlockStart {
            index: 0,
            content_block: ContentBlockResponse::Text {
                text: String::new(),
            },
        });
        acc.process_event(StreamEvent::ContentBlockDelta {
            index: 0,
            delta: ContentBlockDelta::TextDelta {
                text: "Hello".to_string(),
            },
        });
        acc.process_event(StreamEvent::ContentBlockStop { index: 0 });

        // Second tool use block
        acc.process_event(StreamEvent::ContentBlockStart {
            index: 1,
            content_block: ContentBlockResponse::ToolUse {
                id: "t1".to_string(),
                name: "test".to_string(),
                input: serde_json::json!({}),
            },
        });
        acc.process_event(StreamEvent::ContentBlockDelta {
            index: 1,
            delta: ContentBlockDelta::InputJsonDelta {
                partial_json: "{}".to_string(),
            },
        });
        acc.process_event(StreamEvent::ContentBlockStop { index: 1 });

        let (blocks, _) = acc.finish();
        assert_eq!(blocks.len(), 2);
    }

    // ==================== StreamEventResult tests ====================

    #[test]
    fn test_stream_event_result_text() {
        let result = StreamEventResult::TextDelta("hello".to_string());
        assert_eq!(result.text(), Some("hello"));

        let result = StreamEventResult::BlockStarted;
        assert_eq!(result.text(), None);
    }

    #[test]
    fn test_stream_event_result_is_error() {
        let result = StreamEventResult::Error {
            error_type: "test".to_string(),
            message: "msg".to_string(),
        };
        assert!(result.is_error());

        let result = StreamEventResult::TextDelta("text".to_string());
        assert!(!result.is_error());
    }

    #[test]
    fn test_stream_event_result_error_details() {
        let result = StreamEventResult::Error {
            error_type: "type".to_string(),
            message: "msg".to_string(),
        };
        let (etype, emsg) = result.error().unwrap();
        assert_eq!(etype, "type");
        assert_eq!(emsg, "msg");

        let result = StreamEventResult::Ping;
        assert!(result.error().is_none());
    }

    // ==================== StreamStats tests ====================

    #[test]
    fn test_stream_stats_default() {
        let stats = StreamStats::default();
        assert_eq!(stats.total_text_chars, 0);
        assert_eq!(stats.text_delta_count, 0);
        assert_eq!(stats.tool_use_count, 0);
        assert_eq!(stats.content_block_count, 0);
    }

    #[test]
    fn test_stream_stats_update_text_delta() {
        let mut stats = StreamStats::default();
        stats.update(&StreamEventResult::TextDelta("hello".to_string()));
        assert_eq!(stats.total_text_chars, 5);
        assert_eq!(stats.text_delta_count, 1);

        stats.update(&StreamEventResult::TextDelta(" world".to_string()));
        assert_eq!(stats.total_text_chars, 11);
        assert_eq!(stats.text_delta_count, 2);
    }

    #[test]
    fn test_stream_stats_update_block_started() {
        let mut stats = StreamStats::default();
        stats.update(&StreamEventResult::BlockStarted);
        assert_eq!(stats.content_block_count, 1);

        stats.update(&StreamEventResult::BlockStarted);
        assert_eq!(stats.content_block_count, 2);
    }

    #[test]
    fn test_stream_stats_finalize() {
        let mut stats = StreamStats::default();
        let blocks = vec![
            ContentBlockResponse::Text {
                text: "hello".to_string(),
            },
            ContentBlockResponse::ToolUse {
                id: "t1".to_string(),
                name: "test".to_string(),
                input: serde_json::json!({}),
            },
            ContentBlockResponse::ToolUse {
                id: "t2".to_string(),
                name: "test2".to_string(),
                input: serde_json::json!({}),
            },
        ];
        stats.finalize(&blocks);
        assert_eq!(stats.tool_use_count, 2);
    }

    // ==================== Integration tests ====================

    #[test]
    fn test_full_stream_processing() {
        let mut acc = StreamAccumulator::new();
        let mut stats = StreamStats::default();

        // Simulate a full streaming response
        let events = vec![
            StreamEventBuilder::text_block_start(""),
            StreamEventBuilder::text_delta(0, "I will "),
            StreamEventBuilder::text_delta(0, "read the file."),
            StreamEventBuilder::block_stop(0),
            StreamEvent::ContentBlockStart {
                index: 1,
                content_block: ContentBlockResponse::ToolUse {
                    id: "t1".to_string(),
                    name: "file_read".to_string(),
                    input: serde_json::json!({}),
                },
            },
            StreamEventBuilder::input_delta(1, "{\"path\":\"/test\"}"),
            StreamEventBuilder::block_stop(1),
            StreamEventBuilder::message_delta(Some(StopReason::ToolUse)),
            StreamEventBuilder::message_stop(),
        ];

        for event in events {
            let result = acc.process_event(event);
            stats.update(&result);
        }

        let (blocks, stop_reason) = acc.finish();
        stats.finalize(&blocks);

        assert_eq!(blocks.len(), 2);
        assert_eq!(stop_reason, Some(StopReason::ToolUse));
        assert_eq!(stats.tool_use_count, 1);
        assert!(stats.total_text_chars > 0);
    }

    // ==================== Edge cases ====================

    #[test]
    fn test_accumulator_empty_text_delta() {
        let mut acc = StreamAccumulator::new();
        acc.process_event(StreamEventBuilder::text_block_start(""));
        acc.process_event(StreamEventBuilder::text_delta(0, ""));
        acc.process_event(StreamEventBuilder::block_stop(0));

        let (blocks, _) = acc.finish();
        if let ContentBlockResponse::Text { text } = &blocks[0] {
            assert_eq!(text, "");
        }
    }

    #[test]
    fn test_accumulator_invalid_json_input() {
        let mut acc = StreamAccumulator::new();
        acc.process_event(StreamEventBuilder::tool_use_start("t1", "test"));
        acc.process_event(StreamEventBuilder::input_delta(0, "{invalid json}"));
        acc.process_event(StreamEventBuilder::block_stop(0));

        let (blocks, _) = acc.finish();
        // Should not crash, but input may remain empty/default
        if let ContentBlockResponse::ToolUse { input, .. } = &blocks[0] {
            // Input should be the empty default since JSON was invalid
            assert_eq!(*input, serde_json::json!({}));
        }
    }

    #[test]
    fn test_accumulator_out_of_order_index() {
        let mut acc = StreamAccumulator::new();

        // Process events with non-zero index before adding blocks
        // This simulates potential edge cases
        let result = acc.process_event(StreamEvent::ContentBlockDelta {
            index: 5, // Index doesn't exist
            delta: ContentBlockDelta::TextDelta {
                text: "text".to_string(),
            },
        });

        // Should not crash, just not update anything
        assert!(matches!(result, StreamEventResult::TextDelta(_)));
    }

    #[test]
    fn test_accumulator_unicode_text() {
        let mut acc = StreamAccumulator::new();
        acc.process_event(StreamEventBuilder::text_block_start(""));
        acc.process_event(StreamEventBuilder::text_delta(0, "Hello "));
        acc.process_event(StreamEventBuilder::text_delta(0, "世界 "));
        acc.process_event(StreamEventBuilder::text_delta(0, "\u{1F600}"));
        acc.process_event(StreamEventBuilder::block_stop(0));

        let (blocks, _) = acc.finish();
        if let ContentBlockResponse::Text { text } = &blocks[0] {
            assert!(text.contains("Hello"));
            assert!(text.contains("世界"));
            assert!(text.contains("\u{1F600}"));
        }
    }
}
