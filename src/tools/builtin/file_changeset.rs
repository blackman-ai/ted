// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! File change set tool for atomic/incremental multi-file editing

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

use crate::error::{Result, TedError};
use crate::llm::provider::{ToolDefinition, ToolInputSchema};
use crate::tools::{
    ChangeSetMode, FileChangeSet, FileOperation, PermissionRequest, Tool, ToolContext, ToolResult,
};

pub struct FileChangeSetTool;

#[derive(Debug, Serialize, Deserialize)]
struct FileChangeSetInput {
    /// Unique ID for this change set
    id: String,
    /// Description of what this accomplishes
    description: String,
    /// List of file operations
    operations: Vec<FileOperation>,
    /// Related files (optional)
    #[serde(default)]
    related_files: Vec<String>,
    /// Mode: "atomic" or "incremental"
    #[serde(default = "default_mode")]
    mode: ChangeSetMode,
}

fn default_mode() -> ChangeSetMode {
    ChangeSetMode::Atomic
}

#[async_trait]
impl Tool for FileChangeSetTool {
    fn name(&self) -> &str {
        "propose_file_changes"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "propose_file_changes".to_string(),
            description: "Propose multiple related file changes as a coordinated change set. Use 'atomic' mode for planned features (all changes applied together). Use 'incremental' mode for exploratory work (changes applied one at a time).".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                required: vec!["id".to_string(), "description".to_string(), "operations".to_string()],
                properties: json!({
                    "id": {
                        "type": "string",
                        "description": "Unique identifier for this change set (e.g., 'add-auth-system')"
                    },
                    "description": {
                        "type": "string",
                        "description": "Clear description of what this change set accomplishes"
                    },
                    "operations": {
                        "type": "array",
                        "description": "List of file operations to perform",
                        "items": {
                            "oneOf": [
                                {
                                    "type": "object",
                                    "required": ["type", "path"],
                                    "properties": {
                                        "type": { "const": "read" },
                                        "path": { "type": "string" }
                                    }
                                },
                                {
                                    "type": "object",
                                    "required": ["type", "path", "old_string", "new_string"],
                                    "properties": {
                                        "type": { "const": "edit" },
                                        "path": { "type": "string" },
                                        "old_string": { "type": "string" },
                                        "new_string": { "type": "string" }
                                    }
                                },
                                {
                                    "type": "object",
                                    "required": ["type", "path", "content"],
                                    "properties": {
                                        "type": { "const": "write" },
                                        "path": { "type": "string" },
                                        "content": { "type": "string" }
                                    }
                                },
                                {
                                    "type": "object",
                                    "required": ["type", "path"],
                                    "properties": {
                                        "type": { "const": "delete" },
                                        "path": { "type": "string" }
                                    }
                                }
                            ]
                        }
                    },
                    "related_files": {
                        "type": "array",
                        "description": "Other files that may be affected by these changes",
                        "items": { "type": "string" }
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["atomic", "incremental"],
                        "description": "Whether changes are atomic (all-or-nothing) or incremental",
                        "default": "atomic"
                    }
                }),
            },
        }
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let parsed: FileChangeSetInput = serde_json::from_value(input.clone()).ok()?;

        let mut affected_paths = Vec::new();
        for op in &parsed.operations {
            let path = match op {
                FileOperation::Read { path } => path,
                FileOperation::Edit { path, .. } => path,
                FileOperation::Write { path, .. } => path,
                FileOperation::Delete { path } => path,
            };
            affected_paths.push(path.clone());
        }

        Some(PermissionRequest {
            tool_name: "propose_file_changes".to_string(),
            action_description: format!(
                "Propose {} file changes ({}): {}",
                parsed.operations.len(),
                match parsed.mode {
                    ChangeSetMode::Atomic => "atomic",
                    ChangeSetMode::Incremental => "incremental",
                },
                parsed.description
            ),
            affected_paths,
            is_destructive: true,
        })
    }

    fn requires_permission(&self) -> bool {
        true // File operations require permission
    }

    async fn execute(
        &self,
        _tool_use_id: String,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let parsed: FileChangeSetInput = serde_json::from_value(input)
            .map_err(|e| TedError::ToolExecution(format!("Invalid input: {}", e)))?;

        // Create the change set
        let changeset = FileChangeSet {
            id: parsed.id.clone(),
            files: parsed.operations,
            description: parsed.description.clone(),
            related_files: parsed.related_files,
            mode: parsed.mode,
        };

        // Validate all file paths
        for op in &changeset.files {
            let path = match op {
                FileOperation::Read { path } => path,
                FileOperation::Edit { path, .. } => path,
                FileOperation::Write { path, .. } => path,
                FileOperation::Delete { path } => path,
            };

            // Make sure path is within project
            let full_path = if Path::new(path).is_absolute() {
                Path::new(path).to_path_buf()
            } else {
                context.working_directory.join(path)
            };

            // Basic validation
            if !full_path.starts_with(&context.working_directory) {
                if let Some(project_root) = &context.project_root {
                    if !full_path.starts_with(project_root) {
                        return Ok(ToolResult::error(
                            &parsed.id,
                            format!("Path outside project: {}", path),
                        ));
                    }
                }
            }
        }

        // Generate preview of changes
        let mut preview = String::new();
        preview.push_str(&format!("Change Set: {}\n", changeset.id));
        preview.push_str(&format!("Description: {}\n", changeset.description));
        preview.push_str(&format!("Mode: {:?}\n", changeset.mode));
        preview.push_str(&format!("Operations: {}\n\n", changeset.files.len()));

        for (i, op) in changeset.files.iter().enumerate() {
            preview.push_str(&format!("{}. ", i + 1));
            match op {
                FileOperation::Read { path } => {
                    preview.push_str(&format!("Read {}\n", path));
                }
                FileOperation::Edit {
                    path,
                    old_string,
                    new_string,
                } => {
                    preview.push_str(&format!("Edit {}\n", path));
                    let old_preview = if old_string.len() > 50 {
                        format!("{}...", &old_string[..50])
                    } else {
                        old_string.clone()
                    };
                    let new_preview = if new_string.len() > 50 {
                        format!("{}...", &new_string[..50])
                    } else {
                        new_string.clone()
                    };
                    preview.push_str(&format!("  - Old: {}\n", old_preview));
                    preview.push_str(&format!("  + New: {}\n", new_preview));
                }
                FileOperation::Write { path, content } => {
                    preview.push_str(&format!("Write {} ({} bytes)\n", path, content.len()));
                }
                FileOperation::Delete { path } => {
                    preview.push_str(&format!("Delete {}\n", path));
                }
            }
        }

        if !changeset.related_files.is_empty() {
            preview.push_str(&format!(
                "\nRelated files: {}\n",
                changeset.related_files.join(", ")
            ));
        }

        if changeset.mode == ChangeSetMode::Atomic {
            preview.push_str("\n⚠️  Atomic mode: All changes will be applied together or none will be applied.\n");
        } else {
            preview.push_str("\n📝 Incremental mode: Changes can be approved individually.\n");
        }

        Ok(ToolResult::success(&parsed.id, preview))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[tokio::test]
    async fn test_file_changeset_tool_creation() {
        let tool = FileChangeSetTool;
        assert_eq!(tool.name(), "propose_file_changes");
        assert!(tool.requires_permission());
    }

    #[tokio::test]
    async fn test_atomic_changeset() {
        let tool = FileChangeSetTool;
        let context = ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );

        let input = json!({
            "id": "test-changeset",
            "description": "Test atomic changes",
            "mode": "atomic",
            "operations": [
                {
                    "type": "write",
                    "path": "test1.txt",
                    "content": "Hello"
                },
                {
                    "type": "write",
                    "path": "test2.txt",
                    "content": "World"
                }
            ],
            "related_files": ["test3.txt"]
        });

        let result = tool.execute("tool_1".to_string(), input, &context).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(!tool_result.is_error());
        assert!(tool_result.output_text().contains("Atomic mode"));
        assert!(tool_result.output_text().contains("test-changeset"));
    }

    #[tokio::test]
    async fn test_incremental_changeset() {
        let tool = FileChangeSetTool;
        let context = ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );

        let input = json!({
            "id": "incremental-test",
            "description": "Test incremental changes",
            "mode": "incremental",
            "operations": [
                {
                    "type": "edit",
                    "path": "src/main.rs",
                    "old_string": "fn main()",
                    "new_string": "fn main_new()"
                }
            ]
        });

        let result = tool.execute("tool_2".to_string(), input, &context).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(!tool_result.is_error());
        assert!(tool_result.output_text().contains("Incremental mode"));
    }

    // ===== Tool Definition Tests =====

    #[test]
    fn test_definition() {
        let tool = FileChangeSetTool;
        let def = tool.definition();

        assert_eq!(def.name, "propose_file_changes");
        assert!(def.description.contains("change"));
        assert!(def.description.contains("atomic"));
        assert!(def.description.contains("incremental"));
    }

    #[test]
    fn test_definition_has_required_fields() {
        let tool = FileChangeSetTool;
        let def = tool.definition();

        // Check required fields
        assert!(def.input_schema.required.contains(&"id".to_string()));
        assert!(def
            .input_schema
            .required
            .contains(&"description".to_string()));
        assert!(def
            .input_schema
            .required
            .contains(&"operations".to_string()));
    }

    #[test]
    fn test_definition_has_all_properties() {
        let tool = FileChangeSetTool;
        let def = tool.definition();

        let properties = def.input_schema.properties.as_object().unwrap();
        assert!(properties.contains_key("id"));
        assert!(properties.contains_key("description"));
        assert!(properties.contains_key("operations"));
        assert!(properties.contains_key("related_files"));
        assert!(properties.contains_key("mode"));
    }

    // ===== Permission Request Tests =====

    #[test]
    fn test_permission_request_atomic() {
        let tool = FileChangeSetTool;
        let input = json!({
            "id": "test",
            "description": "Test changes",
            "mode": "atomic",
            "operations": [
                {"type": "write", "path": "file1.txt", "content": "hello"}
            ]
        });

        let request = tool.permission_request(&input).unwrap();
        assert_eq!(request.tool_name, "propose_file_changes");
        assert!(request.action_description.contains("atomic"));
        assert!(request.is_destructive);
        assert_eq!(request.affected_paths.len(), 1);
        assert!(request.affected_paths.contains(&"file1.txt".to_string()));
    }

    #[test]
    fn test_permission_request_incremental() {
        let tool = FileChangeSetTool;
        let input = json!({
            "id": "test",
            "description": "Test changes",
            "mode": "incremental",
            "operations": [
                {"type": "edit", "path": "src/main.rs", "old_string": "a", "new_string": "b"}
            ]
        });

        let request = tool.permission_request(&input).unwrap();
        assert!(request.action_description.contains("incremental"));
    }

    #[test]
    fn test_permission_request_multiple_operations() {
        let tool = FileChangeSetTool;
        let input = json!({
            "id": "multi",
            "description": "Multiple operations",
            "operations": [
                {"type": "read", "path": "file1.txt"},
                {"type": "write", "path": "file2.txt", "content": "content"},
                {"type": "edit", "path": "file3.txt", "old_string": "a", "new_string": "b"},
                {"type": "delete", "path": "file4.txt"}
            ]
        });

        let request = tool.permission_request(&input).unwrap();
        assert_eq!(request.affected_paths.len(), 4);
        assert!(request.affected_paths.contains(&"file1.txt".to_string()));
        assert!(request.affected_paths.contains(&"file2.txt".to_string()));
        assert!(request.affected_paths.contains(&"file3.txt".to_string()));
        assert!(request.affected_paths.contains(&"file4.txt".to_string()));
    }

    #[test]
    fn test_permission_request_invalid_input() {
        let tool = FileChangeSetTool;
        let input = json!({"invalid": "input"});

        let request = tool.permission_request(&input);
        assert!(request.is_none());
    }

    // ===== Execute Tests =====

    #[tokio::test]
    async fn test_execute_invalid_input() {
        let tool = FileChangeSetTool;
        let context = ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );

        let input = json!({"invalid": "input"});
        let result = tool.execute("test".to_string(), input, &context).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_read_operation() {
        let tool = FileChangeSetTool;
        let context = ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );

        let input = json!({
            "id": "read-test",
            "description": "Read a file",
            "operations": [
                {"type": "read", "path": "Cargo.toml"}
            ]
        });

        let result = tool.execute("test".to_string(), input, &context).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(tool_result.output_text().contains("Read"));
        assert!(tool_result.output_text().contains("Cargo.toml"));
    }

    #[tokio::test]
    async fn test_execute_delete_operation() {
        let tool = FileChangeSetTool;
        let context = ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );

        let input = json!({
            "id": "delete-test",
            "description": "Delete a file",
            "operations": [
                {"type": "delete", "path": "to_delete.txt"}
            ]
        });

        let result = tool.execute("test".to_string(), input, &context).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(tool_result.output_text().contains("Delete"));
        assert!(tool_result.output_text().contains("to_delete.txt"));
    }

    #[tokio::test]
    async fn test_execute_write_shows_byte_count() {
        let tool = FileChangeSetTool;
        let context = ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );

        let content = "Hello, World! This is test content.";
        let input = json!({
            "id": "write-test",
            "description": "Write file",
            "operations": [
                {"type": "write", "path": "test.txt", "content": content}
            ]
        });

        let result = tool.execute("test".to_string(), input, &context).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(tool_result.output_text().contains("bytes"));
        assert!(tool_result
            .output_text()
            .contains(&format!("{}", content.len())));
    }

    #[tokio::test]
    async fn test_execute_edit_truncates_long_strings() {
        let tool = FileChangeSetTool;
        let context = ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );

        let long_string = "a".repeat(100);
        let input = json!({
            "id": "edit-test",
            "description": "Edit with long strings",
            "operations": [
                {"type": "edit", "path": "test.txt", "old_string": long_string, "new_string": "short"}
            ]
        });

        let result = tool.execute("test".to_string(), input, &context).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        // The output should contain "..." indicating truncation
        assert!(tool_result.output_text().contains("..."));
    }

    #[tokio::test]
    async fn test_execute_no_related_files() {
        let tool = FileChangeSetTool;
        let context = ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );

        let input = json!({
            "id": "no-related",
            "description": "No related files",
            "operations": [
                {"type": "read", "path": "test.txt"}
            ]
        });

        let result = tool.execute("test".to_string(), input, &context).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        // Should NOT contain "Related files" when empty
        assert!(!tool_result.output_text().contains("Related files"));
    }

    #[tokio::test]
    async fn test_execute_with_related_files() {
        let tool = FileChangeSetTool;
        let context = ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );

        let input = json!({
            "id": "with-related",
            "description": "With related files",
            "operations": [
                {"type": "read", "path": "main.rs"}
            ],
            "related_files": ["lib.rs", "mod.rs"]
        });

        let result = tool.execute("test".to_string(), input, &context).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(tool_result.output_text().contains("Related files"));
        assert!(tool_result.output_text().contains("lib.rs"));
        assert!(tool_result.output_text().contains("mod.rs"));
    }

    // ===== Default Mode Tests =====

    #[test]
    fn test_default_mode() {
        assert_eq!(default_mode(), ChangeSetMode::Atomic);
    }

    #[tokio::test]
    async fn test_execute_uses_default_mode() {
        let tool = FileChangeSetTool;
        let context = ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );

        // Don't specify mode - should default to atomic
        let input = json!({
            "id": "default-mode",
            "description": "Test default mode",
            "operations": [
                {"type": "read", "path": "test.txt"}
            ]
        });

        let result = tool.execute("test".to_string(), input, &context).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(tool_result.output_text().contains("Atomic mode"));
    }

    // ===== FileChangeSetInput Deserialization Tests =====

    #[test]
    fn test_input_deserialization_minimal() {
        let json = json!({
            "id": "test",
            "description": "Test",
            "operations": []
        });

        let input: FileChangeSetInput = serde_json::from_value(json).unwrap();
        assert_eq!(input.id, "test");
        assert_eq!(input.description, "Test");
        assert!(input.operations.is_empty());
        assert!(input.related_files.is_empty());
        assert_eq!(input.mode, ChangeSetMode::Atomic);
    }

    #[test]
    fn test_input_deserialization_full() {
        let json = json!({
            "id": "full-test",
            "description": "Full test",
            "operations": [
                {"type": "read", "path": "file.txt"}
            ],
            "related_files": ["related.txt"],
            "mode": "incremental"
        });

        let input: FileChangeSetInput = serde_json::from_value(json).unwrap();
        assert_eq!(input.id, "full-test");
        assert_eq!(input.related_files.len(), 1);
        assert_eq!(input.mode, ChangeSetMode::Incremental);
    }

    // ===== requires_permission Tests =====

    #[test]
    fn test_requires_permission() {
        let tool = FileChangeSetTool;
        assert!(tool.requires_permission());
    }
}
