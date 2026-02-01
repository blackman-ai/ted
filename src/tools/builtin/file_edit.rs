// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! File edit tool
//!
//! Edits existing files using string replacement.

use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;

use crate::error::Result;
use crate::llm::provider::ToolDefinition;
use crate::tools::{PermissionRequest, SchemaBuilder, Tool, ToolContext, ToolResult};

/// Tool for editing existing files
pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_edit".to_string(),
            description: "Edit an existing file by replacing a specific string with new content. The old_string must match exactly (including whitespace and indentation). Use file_read first to see the exact content.".to_string(),
            input_schema: SchemaBuilder::new()
                .string("path", "The path to the file to edit", true)
                .string("old_string", "The exact string to find and replace (must be unique in the file)", true)
                .string("new_string", "The string to replace it with", true)
                .boolean("replace_all", "If true, replace all occurrences (default: false, fails if not unique)", false)
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

        let old_string = input["old_string"]
            .as_str()
            .or_else(|| input["old"].as_str())
            .or_else(|| input["old_text"].as_str())
            .or_else(|| input["search"].as_str())
            .or_else(|| input["find"].as_str())
            .or_else(|| input["original"].as_str())
            .or_else(|| input["from"].as_str())
            .ok_or_else(|| {
                crate::error::TedError::InvalidInput("old_string is required".to_string())
            })?;

        let new_string = input["new_string"]
            .as_str()
            .or_else(|| input["new"].as_str())
            .or_else(|| input["new_text"].as_str())
            .or_else(|| input["replace"].as_str())
            .or_else(|| input["replacement"].as_str())
            .or_else(|| input["to"].as_str())
            .ok_or_else(|| {
                crate::error::TedError::InvalidInput("new_string is required".to_string())
            })?;

        let replace_all = input["replace_all"].as_bool().unwrap_or(false);

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

        // Read current content
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!("Failed to read file: {}", e),
                ));
            }
        };

        // Check how many occurrences exist
        let occurrences = content.matches(old_string).count();

        if occurrences == 0 {
            return Ok(ToolResult::error(
                tool_use_id,
                format!(
                    "String not found in file. Make sure old_string matches exactly (including whitespace).\n\nSearched for:\n{}\n",
                    old_string
                ),
            ));
        }

        if occurrences > 1 && !replace_all {
            return Ok(ToolResult::error(
                tool_use_id,
                format!(
                    "Found {} occurrences of the string. Use replace_all=true to replace all, or provide a more specific string.",
                    occurrences
                ),
            ));
        }

        // Perform replacement
        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        // Write back
        match std::fs::write(&path, &new_content) {
            Ok(_) => {
                // Emit recall event for memory tracking
                context.emit_file_edit(&path);

                let lines_before = content.lines().count();
                let lines_after = new_content.lines().count();
                let line_diff = lines_after as i32 - lines_before as i32;

                let diff_str = if line_diff > 0 {
                    format!("+{} lines", line_diff)
                } else if line_diff < 0 {
                    format!("{} lines", line_diff)
                } else {
                    "no line change".to_string()
                };

                Ok(ToolResult::success(
                    tool_use_id,
                    format!(
                        "Successfully edited {} ({} occurrence(s) replaced, {})",
                        path.display(),
                        if replace_all { occurrences } else { 1 },
                        diff_str
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
            tool_name: "file_edit".to_string(),
            action_description: format!("Edit file: {}", path),
            affected_paths: vec![path.to_string()],
            is_destructive: true,
        })
    }

    fn requires_permission(&self) -> bool {
        true // Editing requires permission
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
        let tool = FileEditTool;
        assert_eq!(tool.name(), "file_edit");
    }

    #[test]
    fn test_tool_definition() {
        let tool = FileEditTool;
        let def = tool.definition();
        assert_eq!(def.name, "file_edit");
        assert!(def.description.contains("Edit"));
    }

    #[test]
    fn test_requires_permission() {
        let tool = FileEditTool;
        assert!(tool.requires_permission());
    }

    #[test]
    fn test_permission_request() {
        let tool = FileEditTool;
        let input = serde_json::json!({
            "path": "/test/file.txt",
            "old_string": "old",
            "new_string": "new"
        });
        let request = tool.permission_request(&input).unwrap();
        assert_eq!(request.tool_name, "file_edit");
        assert!(request.action_description.contains("/test/file.txt"));
        assert!(request.is_destructive);
    }

    #[tokio::test]
    async fn test_edit_single_occurrence() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit_test.txt");
        std::fs::write(&file_path, "Hello World").unwrap();

        let tool = FileEditTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": file_path.to_string_lossy().to_string(),
                    "old_string": "World",
                    "new_string": "Rust"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "Hello Rust");
    }

    #[tokio::test]
    async fn test_edit_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileEditTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": "/nonexistent/file.txt",
                    "old_string": "old",
                    "new_string": "new"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("not found"));
    }

    #[tokio::test]
    async fn test_edit_string_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit_test.txt");
        std::fs::write(&file_path, "Hello World").unwrap();

        let tool = FileEditTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": file_path.to_string_lossy().to_string(),
                    "old_string": "NotFound",
                    "new_string": "New"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("not found"));
    }

    #[tokio::test]
    async fn test_edit_multiple_occurrences_fails_without_replace_all() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit_test.txt");
        std::fs::write(&file_path, "foo bar foo").unwrap();

        let tool = FileEditTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": file_path.to_string_lossy().to_string(),
                    "old_string": "foo",
                    "new_string": "baz"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("2 occurrences"));
    }

    #[tokio::test]
    async fn test_edit_replace_all() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit_test.txt");
        std::fs::write(&file_path, "foo bar foo").unwrap();

        let tool = FileEditTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": file_path.to_string_lossy().to_string(),
                    "old_string": "foo",
                    "new_string": "baz",
                    "replace_all": true
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "baz bar baz");
    }

    #[tokio::test]
    async fn test_edit_adds_lines() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit_test.txt");
        std::fs::write(&file_path, "Line 1").unwrap();

        let tool = FileEditTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": file_path.to_string_lossy().to_string(),
                    "old_string": "Line 1",
                    "new_string": "Line 1\nLine 2\nLine 3"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("+2 lines"));
    }

    #[tokio::test]
    async fn test_edit_removes_lines() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit_test.txt");
        std::fs::write(&file_path, "Line 1\nLine 2\nLine 3").unwrap();

        let tool = FileEditTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": file_path.to_string_lossy().to_string(),
                    "old_string": "Line 1\nLine 2\nLine 3",
                    "new_string": "Single Line"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("-2 lines"));
    }

    #[tokio::test]
    async fn test_edit_relative_path() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("relative_edit.txt");
        std::fs::write(&file_path, "original").unwrap();

        let tool = FileEditTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": "relative_edit.txt",
                    "old_string": "original",
                    "new_string": "modified"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "modified");
    }

    #[tokio::test]
    async fn test_edit_missing_path() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileEditTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "old_string": "old",
                    "new_string": "new"
                }),
                &context,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_edit_missing_old_string() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileEditTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": "test.txt",
                    "new_string": "new"
                }),
                &context,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_edit_missing_new_string() {
        let temp_dir = TempDir::new().unwrap();
        let tool = FileEditTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": "test.txt",
                    "old_string": "old"
                }),
                &context,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_edit_alternative_param_names() {
        // Test flexible parameter names for better model compatibility
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello World").unwrap();

        let tool = FileEditTool;
        let context = create_test_context(&temp_dir);

        // Test using "file" instead of "path"
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "file": file_path.to_string_lossy().to_string(),
                    "old_string": "World",
                    "new_string": "Rust"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello Rust");

        // Reset file
        std::fs::write(&file_path, "Hello Rust").unwrap();

        // Test using "old" and "new" instead of "old_string" and "new_string"
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "path": file_path.to_string_lossy().to_string(),
                    "old": "Rust",
                    "new": "World"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello World");

        // Reset file
        std::fs::write(&file_path, "Hello World").unwrap();

        // Test using "search" and "replace"
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "file_path": file_path.to_string_lossy().to_string(),
                    "search": "World",
                    "replace": "Universe"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello Universe");
    }
}
