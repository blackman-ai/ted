// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! File read tool
//!
//! Reads contents of a file from the filesystem.

use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;

use crate::error::Result;
use crate::llm::provider::ToolDefinition;
use crate::tools::{PermissionRequest, SchemaBuilder, Tool, ToolContext, ToolResult};

/// Tool for reading file contents
pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_read".to_string(),
            description: "Read the contents of a file from the filesystem. Returns the file contents with line numbers.".to_string(),
            input_schema: SchemaBuilder::new()
                .string("path", "The path to the file to read (absolute or relative to working directory)", true)
                .integer("offset", "Line number to start reading from (1-indexed, default: 1)", false)
                .integer("limit", "Maximum number of lines to read (default: 2000)", false)
                .build(),
        }
    }

    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let path_str = input["path"]
            .as_str()
            .ok_or_else(|| crate::error::TedError::InvalidInput("path is required".to_string()))?;

        let offset = input["offset"].as_u64().unwrap_or(1) as usize;
        let limit = input["limit"].as_u64().unwrap_or(2000) as usize;

        // Resolve path
        let path = if PathBuf::from(path_str).is_absolute() {
            PathBuf::from(path_str)
        } else {
            context.working_directory.join(path_str)
        };

        // Check if file exists
        if !path.exists() {
            return Ok(ToolResult::error(
                tool_use_id,
                format!("File not found: {}", path.display()),
            ));
        }

        // Check if it's a file (not a directory)
        if !path.is_file() {
            return Ok(ToolResult::error(
                tool_use_id,
                format!("Not a file: {}", path.display()),
            ));
        }

        // Read the file
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                // Emit recall event for memory tracking
                context.emit_file_read(&path);

                let lines: Vec<&str> = content.lines().collect();
                let start = (offset.saturating_sub(1)).min(lines.len());
                let end = (start + limit).min(lines.len());

                let mut output = String::new();

                // Add header with file info
                output.push_str(&format!(
                    "File: {} (lines {}-{} of {})\n",
                    path.display(),
                    start + 1,
                    end,
                    lines.len()
                ));
                output.push_str("─".repeat(40).as_str());
                output.push('\n');

                // Add lines with line numbers
                for (i, line) in lines[start..end].iter().enumerate() {
                    let line_num = start + i + 1;
                    // Truncate very long lines
                    let display_line = if line.len() > 500 {
                        format!("{}... (truncated)", &line[..500])
                    } else {
                        line.to_string()
                    };
                    output.push_str(&format!("{:>6}\t{}\n", line_num, display_line));
                }

                Ok(ToolResult::success(tool_use_id, output))
            }
            Err(e) => Ok(ToolResult::error(
                tool_use_id,
                format!("Failed to read file: {}", e),
            )),
        }
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let path = input["path"].as_str().unwrap_or("unknown");
        Some(PermissionRequest {
            tool_name: "file_read".to_string(),
            action_description: format!("Read file: {}", path),
            affected_paths: vec![path.to_string()],
            is_destructive: false,
        })
    }

    fn requires_permission(&self) -> bool {
        false // Reading is generally safe
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
    fn test_tool_name() {
        let tool = FileReadTool;
        assert_eq!(tool.name(), "file_read");
    }

    #[test]
    fn test_tool_definition() {
        let tool = FileReadTool;
        let def = tool.definition();
        assert_eq!(def.name, "file_read");
        assert!(def.description.contains("Read"));
    }

    #[test]
    fn test_requires_permission() {
        let tool = FileReadTool;
        assert!(!tool.requires_permission());
    }

    #[test]
    fn test_permission_request() {
        let tool = FileReadTool;
        let input = serde_json::json!({"path": "/test/file.txt"});
        let request = tool.permission_request(&input).unwrap();
        assert_eq!(request.tool_name, "file_read");
        assert!(request.action_description.contains("/test/file.txt"));
        assert!(!request.is_destructive);
    }

    #[tokio::test]
    async fn test_read_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Line 1\nLine 2\nLine 3").unwrap();

        let tool = FileReadTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"path": file_path.to_string_lossy().to_string()}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let output = result.output_text();
        assert!(output.contains("Line 1"));
        assert!(output.contains("Line 2"));
        assert!(output.contains("Line 3"));
    }

    #[tokio::test]
    async fn test_read_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileReadTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"path": "/nonexistent/file.txt"}),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("not found"));
    }

    #[tokio::test]
    async fn test_read_directory_fails() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileReadTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"path": temp_dir.path().to_string_lossy().to_string()}),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("Not a file"));
    }

    #[tokio::test]
    async fn test_read_with_offset_and_limit() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Line 1\nLine 2\nLine 3\nLine 4\nLine 5").unwrap();

        let tool = FileReadTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": file_path.to_string_lossy().to_string(),
                    "offset": 2,
                    "limit": 2
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let output = result.output_text();
        assert!(output.contains("Line 2"));
        assert!(output.contains("Line 3"));
    }

    #[tokio::test]
    async fn test_read_relative_path() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("relative.txt");
        std::fs::write(&file_path, "Relative content").unwrap();

        let tool = FileReadTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"path": "relative.txt"}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("Relative content"));
    }

    #[tokio::test]
    async fn test_read_missing_path_parameter() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileReadTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_long_line_truncation() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("long.txt");
        let long_line = "x".repeat(1000);
        std::fs::write(&file_path, &long_line).unwrap();

        let tool = FileReadTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"path": file_path.to_string_lossy().to_string()}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("truncated"));
    }
}
