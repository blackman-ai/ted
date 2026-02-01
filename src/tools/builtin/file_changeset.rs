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
}
