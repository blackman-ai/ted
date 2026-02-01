// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Tool execution engine
//!
//! Handles executing tools with permission checks and error handling.

use crate::error::{Result, TedError};
use crate::llm::message::{ContentBlock, Message};
use crate::llm::provider::ContentBlockResponse;

use super::{PermissionManager, PermissionResponse, ToolContext, ToolRegistry, ToolResult};

/// Tool executor that handles permission checks and execution
pub struct ToolExecutor {
    registry: ToolRegistry,
    permission_manager: PermissionManager,
    context: ToolContext,
}

impl ToolExecutor {
    /// Create a new executor
    pub fn new(context: ToolContext, trust_mode: bool) -> Self {
        let permission_manager = if trust_mode {
            PermissionManager::with_trust_mode()
        } else {
            PermissionManager::new()
        };

        Self {
            registry: ToolRegistry::with_builtins(),
            permission_manager,
            context,
        }
    }

    /// Get tool definitions for the LLM
    pub fn tool_definitions(&self) -> Vec<crate::llm::provider::ToolDefinition> {
        self.registry.definitions()
    }

    /// Get mutable access to the tool registry for registration of additional tools
    pub fn registry_mut(&mut self) -> &mut ToolRegistry {
        &mut self.registry
    }

    /// Execute a tool use from the LLM response
    pub async fn execute_tool_use(
        &mut self,
        tool_use_id: &str,
        tool_name: &str,
        input: serde_json::Value,
    ) -> Result<ToolResult> {
        // Find the tool
        let tool = self
            .registry
            .get(tool_name)
            .ok_or_else(|| TedError::ToolExecution(format!("Unknown tool: {}", tool_name)))?
            .clone();

        // Check permissions
        if tool.requires_permission() && self.permission_manager.needs_permission(tool_name) {
            if let Some(request) = tool.permission_request(&input) {
                match self.permission_manager.request_permission(&request) {
                    Ok(PermissionResponse::Deny) => {
                        return Ok(ToolResult::error(tool_use_id, "Permission denied by user"));
                    }
                    Err(e) => {
                        return Ok(ToolResult::error(
                            tool_use_id,
                            format!("Failed to get permission: {}", e),
                        ));
                    }
                    _ => {
                        // Permission granted
                    }
                }
            }
        }

        // Execute the tool
        match tool
            .execute(tool_use_id.to_string(), input, &self.context)
            .await
        {
            Ok(result) => Ok(result),
            Err(e) => Ok(ToolResult::error(tool_use_id, e.to_string())),
        }
    }

    /// Process all tool uses from a response and return tool result messages
    pub async fn process_tool_uses(
        &mut self,
        content_blocks: &[ContentBlockResponse],
    ) -> Result<Vec<ToolResult>> {
        let mut results = Vec::new();

        for block in content_blocks {
            if let ContentBlockResponse::ToolUse { id, name, input } = block {
                // Print what tool is being used
                println!("  → Using tool: {} ", name);

                let result = self.execute_tool_use(id, name, input.clone()).await?;

                // Print brief result
                if result.is_error() {
                    println!(
                        "    ✗ Error: {}",
                        truncate_output(result.output_text(), 100)
                    );
                } else {
                    println!("    ✓ Success");
                }

                results.push(result);
            }
        }

        Ok(results)
    }

    /// Convert tool results to a message for the conversation
    pub fn results_to_message(results: Vec<ToolResult>) -> Message {
        let blocks: Vec<ContentBlock> = results
            .into_iter()
            .map(|r| {
                let is_error = r.is_error();
                let output = r.output_text().to_string();
                ContentBlock::ToolResult {
                    tool_use_id: r.tool_use_id,
                    content: crate::llm::message::ToolResultContent::Text(output),
                    is_error: if is_error { Some(true) } else { None },
                }
            })
            .collect();

        Message::assistant_blocks(blocks)
    }
}

/// Truncate output for display
fn truncate_output(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.replace('\n', " ")
    } else {
        format!("{}...", s[..max_len].replace('\n', " "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn create_test_context(temp_dir: &TempDir) -> ToolContext {
        ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            Uuid::new_v4(),
            true,
        )
    }

    #[test]
    fn test_executor_new() {
        let temp_dir = TempDir::new().unwrap();
        let context = create_test_context(&temp_dir);
        let executor = ToolExecutor::new(context, false);

        // Executor should have built-in tools
        let definitions = executor.tool_definitions();
        assert!(!definitions.is_empty());
    }

    #[test]
    fn test_executor_new_with_trust_mode() {
        let temp_dir = TempDir::new().unwrap();
        let context = create_test_context(&temp_dir);
        let executor = ToolExecutor::new(context, true);

        // Executor in trust mode
        assert!(executor.permission_manager.is_trust_mode());
    }

    #[test]
    fn test_executor_tool_definitions() {
        let temp_dir = TempDir::new().unwrap();
        let context = create_test_context(&temp_dir);
        let executor = ToolExecutor::new(context, false);

        let definitions = executor.tool_definitions();

        // Check that we have the expected built-in tools
        let tool_names: Vec<&str> = definitions.iter().map(|d| d.name.as_str()).collect();

        assert!(tool_names.contains(&"file_read"));
        assert!(tool_names.contains(&"file_write"));
        assert!(tool_names.contains(&"file_edit"));
        assert!(tool_names.contains(&"shell"));
        assert!(tool_names.contains(&"glob"));
        assert!(tool_names.contains(&"grep"));
    }

    #[tokio::test]
    async fn test_execute_file_read() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "Hello, world!").unwrap();

        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true); // Trust mode

        let result = executor
            .execute_tool_use(
                "test-id-1",
                "file_read",
                serde_json::json!({
                    "path": test_file.to_string_lossy().to_string()
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("Hello, world!"));
    }

    #[tokio::test]
    async fn test_execute_file_read_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let result = executor
            .execute_tool_use(
                "test-id-1",
                "file_read",
                serde_json::json!({
                    "path": "/nonexistent/path/file.txt"
                }),
            )
            .await
            .unwrap();

        assert!(result.is_error());
    }

    #[tokio::test]
    async fn test_execute_glob() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test1.rs"), "fn main() {}").unwrap();
        std::fs::write(temp_dir.path().join("test2.rs"), "fn test() {}").unwrap();
        std::fs::write(temp_dir.path().join("other.txt"), "text").unwrap();

        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let result = executor
            .execute_tool_use(
                "test-id-1",
                "glob",
                serde_json::json!({
                    "pattern": "**/*.rs",
                    "path": temp_dir.path().to_string_lossy().to_string()
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let output = result.output_text();
        assert!(output.contains("test1.rs"));
        assert!(output.contains("test2.rs"));
        assert!(!output.contains("other.txt"));
    }

    #[tokio::test]
    async fn test_execute_unknown_tool() {
        let temp_dir = TempDir::new().unwrap();
        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let result = executor
            .execute_tool_use("test-id-1", "unknown_tool", serde_json::json!({}))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_file_write() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("new_file.txt");

        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let result = executor
            .execute_tool_use(
                "test-id-1",
                "file_write",
                serde_json::json!({
                    "path": test_file.to_string_lossy().to_string(),
                    "content": "New content here"
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(test_file.exists());
        assert_eq!(
            std::fs::read_to_string(&test_file).unwrap(),
            "New content here"
        );
    }

    #[tokio::test]
    async fn test_results_to_message() {
        let results = vec![
            ToolResult {
                tool_use_id: "id1".to_string(),
                output: super::super::ToolOutput::Success("Success output".to_string()),
            },
            ToolResult {
                tool_use_id: "id2".to_string(),
                output: super::super::ToolOutput::Error("Error message".to_string()),
            },
        ];

        let message = ToolExecutor::results_to_message(results);

        // Should be an assistant message with blocks
        match &message.content {
            crate::llm::message::MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 2);
            }
            _ => panic!("Expected blocks content"),
        }
    }

    #[test]
    fn test_truncate_output_short() {
        let short = "Hello";
        assert_eq!(truncate_output(short, 100), "Hello");
    }

    #[test]
    fn test_truncate_output_long() {
        let long = "A".repeat(150);
        let truncated = truncate_output(&long, 100);
        assert!(truncated.ends_with("..."));
        assert!(truncated.len() <= 104); // 100 + "..."
    }

    #[test]
    fn test_truncate_output_with_newlines() {
        let with_newlines = "Line 1\nLine 2\nLine 3";
        let result = truncate_output(with_newlines, 100);
        assert!(!result.contains('\n'));
        assert!(result.contains("Line 1 Line 2 Line 3"));
    }

    #[test]
    fn test_truncate_output_exact_length() {
        let exact = "A".repeat(100);
        let result = truncate_output(&exact, 100);
        assert_eq!(result.len(), 100);
        assert!(!result.ends_with("..."));
    }

    #[tokio::test]
    async fn test_execute_grep() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(
            temp_dir.path().join("test.txt"),
            "Hello World\nFoo Bar\nHello Again",
        )
        .unwrap();

        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let result = executor
            .execute_tool_use(
                "test-id-1",
                "grep",
                serde_json::json!({
                    "pattern": "Hello",
                    "path": temp_dir.path().to_string_lossy().to_string()
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let output = result.output_text();
        // Grep should find matches - output format may vary
        assert!(
            output.contains("Hello") || output.contains("test.txt") || output.contains("match")
        );
    }

    #[tokio::test]
    async fn test_execute_shell_echo() {
        let temp_dir = TempDir::new().unwrap();
        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let result = executor
            .execute_tool_use(
                "test-id-1",
                "shell",
                serde_json::json!({
                    "command": "echo 'test output'"
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("test output"));
    }

    #[tokio::test]
    async fn test_execute_file_edit() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("edit_test.txt");
        std::fs::write(&test_file, "Hello World").unwrap();

        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let result = executor
            .execute_tool_use(
                "test-id-1",
                "file_edit",
                serde_json::json!({
                    "path": test_file.to_string_lossy().to_string(),
                    "old_string": "World",
                    "new_string": "Universe"
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert_eq!(
            std::fs::read_to_string(&test_file).unwrap(),
            "Hello Universe"
        );
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("test-id", "Something went wrong");
        assert!(result.is_error());
        assert_eq!(result.output_text(), "Something went wrong");
    }

    #[tokio::test]
    async fn test_process_tool_uses_empty() {
        let temp_dir = TempDir::new().unwrap();
        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let results = executor.process_tool_uses(&[]).await.unwrap();
        assert!(results.is_empty());
    }

    // ===== Additional Executor Tests =====

    #[tokio::test]
    async fn test_process_tool_uses_with_text_blocks() {
        let temp_dir = TempDir::new().unwrap();
        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        // Process blocks that include non-tool-use blocks
        let blocks = vec![ContentBlockResponse::Text {
            text: "Let me read that file for you".to_string(),
        }];

        let results = executor.process_tool_uses(&blocks).await.unwrap();
        // Text blocks should be ignored
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_process_tool_uses_with_tool_use() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "Hello!").unwrap();

        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let blocks = vec![ContentBlockResponse::ToolUse {
            id: "tool-123".to_string(),
            name: "file_read".to_string(),
            input: serde_json::json!({
                "path": test_file.to_string_lossy().to_string()
            }),
        }];

        let results = executor.process_tool_uses(&blocks).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].is_error());
    }

    #[tokio::test]
    async fn test_process_tool_uses_multiple_tools() {
        let temp_dir = TempDir::new().unwrap();
        let test_file1 = temp_dir.path().join("file1.txt");
        let test_file2 = temp_dir.path().join("file2.txt");
        std::fs::write(&test_file1, "Content 1").unwrap();
        std::fs::write(&test_file2, "Content 2").unwrap();

        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let blocks = vec![
            ContentBlockResponse::ToolUse {
                id: "tool-1".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({
                    "path": test_file1.to_string_lossy().to_string()
                }),
            },
            ContentBlockResponse::ToolUse {
                id: "tool-2".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({
                    "path": test_file2.to_string_lossy().to_string()
                }),
            },
        ];

        let results = executor.process_tool_uses(&blocks).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(!results[0].is_error());
        assert!(!results[1].is_error());
    }

    #[tokio::test]
    async fn test_results_to_message_single_success() {
        let results = vec![ToolResult {
            tool_use_id: "id1".to_string(),
            output: super::super::ToolOutput::Success("Success!".to_string()),
        }];

        let message = ToolExecutor::results_to_message(results);

        match &message.content {
            crate::llm::message::MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                if let ContentBlock::ToolResult {
                    tool_use_id,
                    is_error,
                    ..
                } = &blocks[0]
                {
                    assert_eq!(tool_use_id, "id1");
                    assert!(is_error.is_none()); // Success doesn't have is_error set
                } else {
                    panic!("Expected ToolResult block");
                }
            }
            _ => panic!("Expected blocks content"),
        }
    }

    #[tokio::test]
    async fn test_results_to_message_single_error() {
        let results = vec![ToolResult {
            tool_use_id: "id1".to_string(),
            output: super::super::ToolOutput::Error("Failed!".to_string()),
        }];

        let message = ToolExecutor::results_to_message(results);

        match &message.content {
            crate::llm::message::MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                if let ContentBlock::ToolResult {
                    tool_use_id,
                    is_error,
                    ..
                } = &blocks[0]
                {
                    assert_eq!(tool_use_id, "id1");
                    assert_eq!(*is_error, Some(true)); // Error has is_error set
                } else {
                    panic!("Expected ToolResult block");
                }
            }
            _ => panic!("Expected blocks content"),
        }
    }

    #[tokio::test]
    async fn test_results_to_message_empty() {
        let results: Vec<ToolResult> = vec![];
        let message = ToolExecutor::results_to_message(results);

        match &message.content {
            crate::llm::message::MessageContent::Blocks(blocks) => {
                assert!(blocks.is_empty());
            }
            _ => panic!("Expected blocks content"),
        }
    }

    #[test]
    fn test_truncate_output_empty() {
        let empty = "";
        assert_eq!(truncate_output(empty, 100), "");
    }

    #[test]
    fn test_truncate_output_only_newlines() {
        let newlines = "\n\n\n";
        let result = truncate_output(newlines, 100);
        assert_eq!(result, "   ");
    }

    #[test]
    fn test_truncate_output_unicode() {
        let unicode = "Hello 你好 🌍";
        let result = truncate_output(unicode, 100);
        assert!(result.contains("Hello"));
        assert!(result.contains("你好"));
        assert!(result.contains("🌍"));
    }

    #[test]
    fn test_truncate_output_max_len_zero() {
        let text = "Hello";
        let result = truncate_output(text, 0);
        assert_eq!(result, "...");
    }

    #[test]
    fn test_truncate_output_one_char() {
        let text = "AB";
        let result = truncate_output(text, 1);
        assert_eq!(result, "A...");
    }

    #[tokio::test]
    async fn test_execute_file_write_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let nested_file = temp_dir.path().join("subdir/nested/file.txt");

        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let result = executor
            .execute_tool_use(
                "test-id",
                "file_write",
                serde_json::json!({
                    "path": nested_file.to_string_lossy().to_string(),
                    "content": "Nested content"
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(nested_file.exists());
    }

    #[tokio::test]
    async fn test_execute_file_edit_string_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("edit_test.txt");
        std::fs::write(&test_file, "Hello World").unwrap();

        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let result = executor
            .execute_tool_use(
                "test-id",
                "file_edit",
                serde_json::json!({
                    "path": test_file.to_string_lossy().to_string(),
                    "old_string": "NotFound",
                    "new_string": "Replacement"
                }),
            )
            .await
            .unwrap();

        // Should be an error since the string wasn't found
        assert!(result.is_error());
    }

    #[tokio::test]
    async fn test_execute_shell_with_working_dir() {
        let temp_dir = TempDir::new().unwrap();
        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let result = executor
            .execute_tool_use(
                "test-id",
                "shell",
                serde_json::json!({
                    "command": "pwd"
                }),
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        // The output should contain the temp directory path
        let output = result.output_text();
        assert!(output.contains(temp_dir.path().file_name().unwrap().to_str().unwrap()));
    }

    #[tokio::test]
    async fn test_execute_glob_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        // Create a file that won't match
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let context = create_test_context(&temp_dir);
        let mut executor = ToolExecutor::new(context, true);

        let result = executor
            .execute_tool_use(
                "test-id",
                "glob",
                serde_json::json!({
                    "pattern": "**/*.rs",
                    "path": temp_dir.path().to_string_lossy().to_string()
                }),
            )
            .await
            .unwrap();

        // Should succeed but find no matches
        assert!(!result.is_error());
    }

    #[test]
    fn test_executor_trust_mode_vs_non_trust() {
        let temp_dir = TempDir::new().unwrap();
        let context1 = create_test_context(&temp_dir);
        let context2 = create_test_context(&temp_dir);

        let executor_trust = ToolExecutor::new(context1, true);
        let executor_no_trust = ToolExecutor::new(context2, false);

        assert!(executor_trust.permission_manager.is_trust_mode());
        assert!(!executor_no_trust.permission_manager.is_trust_mode());
    }

    #[test]
    fn test_tool_definitions_have_required_fields() {
        let temp_dir = TempDir::new().unwrap();
        let context = create_test_context(&temp_dir);
        let executor = ToolExecutor::new(context, true);

        let definitions = executor.tool_definitions();

        for def in definitions {
            assert!(!def.name.is_empty());
            assert!(!def.description.is_empty());
            assert_eq!(def.input_schema.schema_type, "object");
        }
    }
}
