// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! File write tool
//!
//! Writes content to a new file.

use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;

use crate::error::Result;
use crate::llm::provider::ToolDefinition;
use crate::tools::{PermissionRequest, SchemaBuilder, Tool, ToolContext, ToolResult};

/// Tool for writing new files
pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_write".to_string(),
            description: "Write content to a new file. Will create parent directories if needed. Fails if file already exists (use file_edit to modify existing files).".to_string(),
            input_schema: SchemaBuilder::new()
                .string("path", "The path where the file should be created", true)
                .string("content", "The content to write to the file", true)
                .build(),
        }
    }

    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        // Flexible parameter name lookup - support common alternatives models might use
        let path_str = input["path"]
            .as_str()
            .or_else(|| input["file"].as_str())
            .or_else(|| input["file_path"].as_str())
            .or_else(|| input["filepath"].as_str())
            .ok_or_else(|| crate::error::TedError::InvalidInput("path is required".to_string()))?;

        let content = input["content"]
            .as_str()
            .or_else(|| input["text"].as_str())
            .or_else(|| input["body"].as_str())
            .or_else(|| input["data"].as_str())
            .ok_or_else(|| {
                crate::error::TedError::InvalidInput("content is required".to_string())
            })?;

        // Resolve path
        let path = if PathBuf::from(path_str).is_absolute() {
            PathBuf::from(path_str)
        } else {
            context.working_directory.join(path_str)
        };

        // Check if file already exists
        if path.exists() {
            return Ok(ToolResult::error(
                tool_use_id,
                format!(
                    "File already exists: {}. Use file_edit to modify existing files.",
                    path.display()
                ),
            ));
        }

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return Ok(ToolResult::error(
                        tool_use_id,
                        format!("Failed to create parent directories: {}", e),
                    ));
                }
            }
        }

        // Write the file
        match std::fs::write(&path, content) {
            Ok(_) => {
                // Emit recall event for memory tracking
                context.emit_file_write(&path);

                let line_count = content.lines().count();
                Ok(ToolResult::success(
                    tool_use_id,
                    format!(
                        "Successfully created {} ({} lines, {} bytes)",
                        path.display(),
                        line_count,
                        content.len()
                    ),
                ))
            }
            Err(e) => Ok(ToolResult::error(
                tool_use_id,
                format!("Failed to write file: {}", e),
            )),
        }
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let path = input["path"].as_str().unwrap_or("unknown");
        Some(PermissionRequest {
            tool_name: "file_write".to_string(),
            action_description: format!("Create new file: {}", path),
            affected_paths: vec![path.to_string()],
            is_destructive: false,
        })
    }

    fn requires_permission(&self) -> bool {
        true // Writing requires permission
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
        let tool = FileWriteTool;
        assert_eq!(tool.name(), "file_write");
    }

    #[test]
    fn test_tool_definition() {
        let tool = FileWriteTool;
        let def = tool.definition();
        assert_eq!(def.name, "file_write");
        assert!(def.description.contains("Write"));
    }

    #[test]
    fn test_requires_permission() {
        let tool = FileWriteTool;
        assert!(tool.requires_permission());
    }

    #[test]
    fn test_permission_request() {
        let tool = FileWriteTool;
        let input = serde_json::json!({"path": "/test/file.txt", "content": "hello"});
        let request = tool.permission_request(&input).unwrap();
        assert_eq!(request.tool_name, "file_write");
        assert!(request.action_description.contains("/test/file.txt"));
        assert!(!request.is_destructive);
    }

    #[tokio::test]
    async fn test_write_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("new_file.txt");

        let tool = FileWriteTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": file_path.to_string_lossy().to_string(),
                    "content": "Hello, world!"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(file_path.exists());
        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap(),
            "Hello, world!"
        );
    }

    #[tokio::test]
    async fn test_write_existing_file_fails() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("existing.txt");
        std::fs::write(&file_path, "existing content").unwrap();

        let tool = FileWriteTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": file_path.to_string_lossy().to_string(),
                    "content": "new content"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("already exists"));
    }

    #[tokio::test]
    async fn test_write_creates_parent_directories() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nested").join("dir").join("file.txt");

        let tool = FileWriteTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": file_path.to_string_lossy().to_string(),
                    "content": "nested content"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(file_path.exists());
        assert_eq!(
            std::fs::read_to_string(&file_path).unwrap(),
            "nested content"
        );
    }

    #[tokio::test]
    async fn test_write_relative_path() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileWriteTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": "relative_new.txt",
                    "content": "relative content"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let expected_path = temp_dir.path().join("relative_new.txt");
        assert!(expected_path.exists());
    }

    #[tokio::test]
    async fn test_write_missing_path() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileWriteTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"content": "hello"}),
                &context,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_write_missing_content() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileWriteTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"path": "test.txt"}),
                &context,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_write_multiline_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("multiline.txt");

        let tool = FileWriteTool;
        let context = create_test_context(&temp_dir);

        let content = "Line 1\nLine 2\nLine 3";
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": file_path.to_string_lossy().to_string(),
                    "content": content
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("3 lines"));
    }
}
