// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Beads tools for task tracking
//!
//! Provides LLM-callable tools for managing beads (task tracking units).

use async_trait::async_trait;
use serde_json::Value;

use crate::beads::schema::{Bead, BeadId, BeadPriority};
use crate::beads::storage::BeadStore;
use crate::error::Result;
use crate::llm::provider::ToolDefinition;
use crate::tools::{PermissionRequest, SchemaBuilder, Tool, ToolContext, ToolResult};

/// Tool for creating new beads (tasks)
pub struct BeadsAddTool;

#[async_trait]
impl Tool for BeadsAddTool {
    fn name(&self) -> &str {
        "beads_add"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "beads_add".to_string(),
            description: "Create a new bead (task) in the project's task tracking system. \
                Beads are units of work with support for dependencies, priorities, and tags. \
                Use this to track tasks, features, bugs, or any work items."
                .to_string(),
            input_schema: SchemaBuilder::new()
                .string("title", "Short, descriptive title for the task", true)
                .string(
                    "description",
                    "Detailed description of what needs to be done",
                    false,
                )
                .string(
                    "priority",
                    "Priority level: low, medium (default), high, or critical",
                    false,
                )
                .array("tags", "Tags for categorization (e.g., 'feature', 'bug', 'refactor')", "string", false)
                .array(
                    "depends_on",
                    "IDs of beads that must be completed before this one (e.g., ['bd-abc123'])",
                    "string",
                    false,
                )
                .build(),
        }
    }

    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        // Extract title (required)
        let title = input["title"]
            .as_str()
            .or_else(|| input["name"].as_str())
            .or_else(|| input["task"].as_str())
            .ok_or_else(|| {
                crate::error::TedError::InvalidInput("title is required".to_string())
            })?;

        // Extract optional fields
        let description = input["description"]
            .as_str()
            .or_else(|| input["desc"].as_str())
            .or_else(|| input["details"].as_str())
            .unwrap_or("");

        let priority = match input["priority"].as_str() {
            Some("low") => BeadPriority::Low,
            Some("high") => BeadPriority::High,
            Some("critical") => BeadPriority::Critical,
            _ => BeadPriority::Medium,
        };

        let tags: Vec<String> = input["tags"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let depends_on: Vec<BeadId> = input["depends_on"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| BeadId::from(s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        // Get project root for bead storage
        let project_root = context.project_root.as_ref().unwrap_or(&context.working_directory);
        let beads_dir = project_root.join(".beads");

        // Initialize bead store
        let store = match BeadStore::new(beads_dir) {
            Ok(s) => s,
            Err(e) => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!("Failed to initialize bead storage: {}", e),
                ));
            }
        };

        // Create the bead
        let bead = Bead::new(title, description)
            .with_priority(priority)
            .with_tags(tags.clone())
            .with_depends_on(depends_on.clone());

        // Store it
        match store.create(bead) {
            Ok(id) => {
                let mut details = vec![format!("Created bead: {} - {}", id, title)];

                if !description.is_empty() {
                    details.push(format!("Description: {}", description));
                }
                if priority != BeadPriority::Medium {
                    details.push(format!("Priority: {:?}", priority));
                }
                if !tags.is_empty() {
                    details.push(format!("Tags: {}", tags.join(", ")));
                }
                if !depends_on.is_empty() {
                    let dep_strs: Vec<String> = depends_on.iter().map(|d| d.to_string()).collect();
                    details.push(format!("Depends on: {}", dep_strs.join(", ")));
                }

                Ok(ToolResult::success(tool_use_id, details.join("\n")))
            }
            Err(e) => Ok(ToolResult::error(
                tool_use_id,
                format!("Failed to create bead: {}", e),
            )),
        }
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let title = input["title"].as_str().unwrap_or("unknown");
        Some(PermissionRequest {
            tool_name: "beads_add".to_string(),
            action_description: format!("Create task bead: {}", title),
            affected_paths: vec![".beads/beads.jsonl".to_string()],
            is_destructive: false,
        })
    }

    fn requires_permission(&self) -> bool {
        true
    }
}

/// Tool for listing beads
pub struct BeadsListTool;

#[async_trait]
impl Tool for BeadsListTool {
    fn name(&self) -> &str {
        "beads_list"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "beads_list".to_string(),
            description: "List beads (tasks) in the project. \
                Can filter by status or show all beads with their current state."
                .to_string(),
            input_schema: SchemaBuilder::new()
                .string(
                    "status",
                    "Filter by status: pending, ready, in_progress, blocked, done, cancelled, or all (default)",
                    false,
                )
                .build(),
        }
    }

    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let project_root = context.project_root.as_ref().unwrap_or(&context.working_directory);
        let beads_dir = project_root.join(".beads");

        let store = match BeadStore::new(beads_dir) {
            Ok(s) => s,
            Err(e) => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!("Failed to initialize bead storage: {}", e),
                ));
            }
        };

        let status_filter = input["status"].as_str().unwrap_or("all");

        let beads = match status_filter {
            "pending" => store.by_status(&crate::beads::schema::BeadStatus::Pending),
            "ready" => store.ready(),
            "in_progress" => store.in_progress(),
            "done" => store.completed(),
            _ => store.all(),
        };

        if beads.is_empty() {
            return Ok(ToolResult::success(
                tool_use_id,
                format!("No beads found{}",
                    if status_filter != "all" { format!(" with status '{}'", status_filter) } else { String::new() }
                ),
            ));
        }

        let mut output = vec![format!("Found {} bead(s):\n", beads.len())];

        for bead in beads {
            let status_str = match &bead.status {
                crate::beads::schema::BeadStatus::Pending => "pending",
                crate::beads::schema::BeadStatus::Ready => "ready",
                crate::beads::schema::BeadStatus::InProgress => "in_progress",
                crate::beads::schema::BeadStatus::Blocked { .. } => "blocked",
                crate::beads::schema::BeadStatus::Done => "done",
                crate::beads::schema::BeadStatus::Cancelled { .. } => "cancelled",
            };

            output.push(format!(
                "• {} - {} [{}] (priority: {:?})",
                bead.id, bead.title, status_str, bead.priority
            ));

            if !bead.description.is_empty() {
                output.push(format!("  Description: {}", bead.description));
            }
            if !bead.tags.is_empty() {
                output.push(format!("  Tags: {}", bead.tags.join(", ")));
            }
        }

        Ok(ToolResult::success(tool_use_id, output.join("\n")))
    }

    fn permission_request(&self, _input: &Value) -> Option<PermissionRequest> {
        None // Read-only operation
    }

    fn requires_permission(&self) -> bool {
        false // Reading beads doesn't require permission
    }
}

/// Tool for updating bead status
pub struct BeadsStatusTool;

#[async_trait]
impl Tool for BeadsStatusTool {
    fn name(&self) -> &str {
        "beads_status"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "beads_status".to_string(),
            description: "Update the status of a bead. Use this to mark tasks as ready, \
                in progress, done, blocked, or cancelled."
                .to_string(),
            input_schema: SchemaBuilder::new()
                .string("id", "The bead ID (e.g., 'bd-abc123')", true)
                .string(
                    "status",
                    "New status: ready, in_progress, done, blocked, or cancelled",
                    true,
                )
                .string(
                    "reason",
                    "Reason for blocked/cancelled status (required for those statuses)",
                    false,
                )
                .build(),
        }
    }

    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let id_str = input["id"]
            .as_str()
            .or_else(|| input["bead_id"].as_str())
            .ok_or_else(|| {
                crate::error::TedError::InvalidInput("id is required".to_string())
            })?;

        let new_status = input["status"]
            .as_str()
            .ok_or_else(|| {
                crate::error::TedError::InvalidInput("status is required".to_string())
            })?;

        let reason = input["reason"].as_str().unwrap_or("").to_string();

        let project_root = context.project_root.as_ref().unwrap_or(&context.working_directory);
        let beads_dir = project_root.join(".beads");

        let store = match BeadStore::new(beads_dir) {
            Ok(s) => s,
            Err(e) => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!("Failed to initialize bead storage: {}", e),
                ));
            }
        };

        let id = BeadId::from(id_str.to_string());

        let status = match new_status {
            "ready" => crate::beads::schema::BeadStatus::Ready,
            "in_progress" | "inprogress" | "in-progress" => {
                crate::beads::schema::BeadStatus::InProgress
            }
            "done" | "complete" | "completed" => crate::beads::schema::BeadStatus::Done,
            "blocked" => {
                if reason.is_empty() {
                    return Ok(ToolResult::error(
                        tool_use_id,
                        "reason is required when setting status to blocked".to_string(),
                    ));
                }
                crate::beads::schema::BeadStatus::Blocked { reason: reason.clone() }
            }
            "cancelled" | "canceled" => {
                if reason.is_empty() {
                    return Ok(ToolResult::error(
                        tool_use_id,
                        "reason is required when setting status to cancelled".to_string(),
                    ));
                }
                crate::beads::schema::BeadStatus::Cancelled { reason: reason.clone() }
            }
            _ => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!(
                        "Invalid status '{}'. Valid options: ready, in_progress, done, blocked, cancelled",
                        new_status
                    ),
                ));
            }
        };

        match store.set_status(&id, status) {
            Ok(()) => Ok(ToolResult::success(
                tool_use_id,
                format!("Updated bead {} status to '{}'", id, new_status),
            )),
            Err(e) => Ok(ToolResult::error(
                tool_use_id,
                format!("Failed to update bead status: {}", e),
            )),
        }
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let id = input["id"].as_str().unwrap_or("unknown");
        let status = input["status"].as_str().unwrap_or("unknown");
        Some(PermissionRequest {
            tool_name: "beads_status".to_string(),
            action_description: format!("Update bead {} status to '{}'", id, status),
            affected_paths: vec![".beads/beads.jsonl".to_string()],
            is_destructive: false,
        })
    }

    fn requires_permission(&self) -> bool {
        true
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
    fn test_beads_add_tool_name() {
        let tool = BeadsAddTool;
        assert_eq!(tool.name(), "beads_add");
    }

    #[test]
    fn test_beads_add_tool_definition() {
        let tool = BeadsAddTool;
        let def = tool.definition();
        assert_eq!(def.name, "beads_add");
        assert!(def.description.contains("task"));
    }

    #[test]
    fn test_beads_add_requires_permission() {
        let tool = BeadsAddTool;
        assert!(tool.requires_permission());
    }

    #[test]
    fn test_beads_add_permission_request() {
        let tool = BeadsAddTool;
        let input = serde_json::json!({"title": "Test task"});
        let request = tool.permission_request(&input).unwrap();
        assert_eq!(request.tool_name, "beads_add");
        assert!(request.action_description.contains("Test task"));
        assert!(!request.is_destructive);
    }

    #[tokio::test]
    async fn test_beads_add_creates_bead() {
        let temp_dir = TempDir::new().unwrap();
        let tool = BeadsAddTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "title": "Test task",
                    "description": "A test description",
                    "priority": "high",
                    "tags": ["feature", "test"]
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("Created bead"));
        assert!(result.output_text().contains("Test task"));
    }

    #[tokio::test]
    async fn test_beads_add_missing_title() {
        let temp_dir = TempDir::new().unwrap();
        let tool = BeadsAddTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"description": "No title"}),
                &context,
            )
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_beads_list_tool_name() {
        let tool = BeadsListTool;
        assert_eq!(tool.name(), "beads_list");
    }

    #[test]
    fn test_beads_list_no_permission_required() {
        let tool = BeadsListTool;
        assert!(!tool.requires_permission());
    }

    #[tokio::test]
    async fn test_beads_list_empty() {
        let temp_dir = TempDir::new().unwrap();
        let tool = BeadsListTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("No beads found"));
    }

    #[test]
    fn test_beads_status_tool_name() {
        let tool = BeadsStatusTool;
        assert_eq!(tool.name(), "beads_status");
    }

    #[test]
    fn test_beads_status_requires_permission() {
        let tool = BeadsStatusTool;
        assert!(tool.requires_permission());
    }
}
