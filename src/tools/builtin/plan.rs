// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Plan management tool
//!
//! Allows Ted to create, update, and manage plans during conversations.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{Result, TedError};
use crate::llm::provider::ToolDefinition;
use crate::plans::{PlanStatus, PlanStore};
use crate::tools::{PermissionRequest, SchemaBuilder, Tool, ToolContext, ToolResult};

/// Tool for managing plans
pub struct PlanUpdateTool;

#[async_trait]
impl Tool for PlanUpdateTool {
    fn name(&self) -> &str {
        "plan_update"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "plan_update".to_string(),
            description: r#"Create or update work plans for complex tasks. Use this to:
- Create a new plan when starting a complex multi-step task
- Update an existing plan's content as work progresses
- Mark tasks as complete
- Add progress log entries
- Change plan status (pause, complete, archive)"#
                .to_string(),
            input_schema: SchemaBuilder::new()
                .string(
                    "action",
                    "Action to perform: 'create', 'update', 'set_status', or 'add_log'",
                    true,
                )
                .string("title", "Plan title (required for 'create' action)", false)
                .string(
                    "content",
                    "Full markdown content with tasks as '- [ ] Task' or '- [x] Task' (for 'create' or 'update')",
                    false,
                )
                .string(
                    "plan_id",
                    "UUID of existing plan (required for 'update', 'set_status', 'add_log')",
                    false,
                )
                .string(
                    "status",
                    "New status: 'active', 'paused', 'complete', or 'archived' (for 'set_status')",
                    false,
                )
                .string(
                    "log_entry",
                    "Progress note to append (for 'add_log')",
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
        let action = input["action"]
            .as_str()
            .ok_or_else(|| TedError::InvalidInput("action is required".to_string()))?;

        match action {
            "create" => self.create_plan(tool_use_id, &input, context).await,
            "update" => self.update_plan(tool_use_id, &input).await,
            "set_status" => self.set_status(tool_use_id, &input).await,
            "add_log" => self.add_log(tool_use_id, &input).await,
            _ => Ok(ToolResult::error(
                tool_use_id,
                format!(
                    "Unknown action: {}. Valid actions: create, update, set_status, add_log",
                    action
                ),
            )),
        }
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let action = input["action"].as_str().unwrap_or("unknown");
        let title = input["title"].as_str().unwrap_or("plan");

        Some(PermissionRequest {
            tool_name: "plan_update".to_string(),
            action_description: match action {
                "create" => format!("Create plan: {}", title),
                "update" => "Update plan content".to_string(),
                "set_status" => "Change plan status".to_string(),
                "add_log" => "Add progress log entry".to_string(),
                _ => format!("Plan action: {}", action),
            },
            affected_paths: vec![],
            is_destructive: false,
        })
    }

    fn requires_permission(&self) -> bool {
        false // Plan operations are generally safe
    }
}

impl PlanUpdateTool {
    /// Create a new plan
    async fn create_plan(
        &self,
        tool_use_id: String,
        input: &Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let title = match input["title"].as_str() {
            Some(t) if !t.is_empty() => t,
            _ => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    "title is required for 'create' action",
                ))
            }
        };

        let content = input["content"].as_str().unwrap_or("");

        let mut store = PlanStore::open()?;
        let mut plan = store.create(title, content)?;

        // Link to current session
        store.link_session(plan.info.id, context.session_id)?;

        // Set project path if available
        if let Some(ref project_root) = context.project_root {
            // Re-fetch the mutable reference after the borrow ends
            if let Some(info) = store.list().iter().find(|p| p.id == plan.info.id).cloned() {
                plan.info = info;
            }
            // Update the project path in a separate operation
            let plan_id = plan.info.id;
            let content_str = plan.content.clone();

            // Get the plan, update it, and save
            if let Ok(Some(mut p)) = store.get(plan_id) {
                p.info.set_project_path(project_root.clone());
                // Re-serialize with updated info
                let serialized = crate::plans::serialize_plan(&p)?;
                std::fs::write(
                    crate::plans::plans_dir().join(format!("{}.md", plan_id)),
                    serialized,
                )?;
            }
            drop(content_str);
        }

        let task_info = if plan.info.task_count > 0 {
            format!(
                " ({} tasks, {} completed)",
                plan.info.task_count, plan.info.completed_count
            )
        } else {
            String::new()
        };

        // Provide explicit instructions to the model about what to do next
        Ok(ToolResult::success(
            tool_use_id,
            format!(
                "Created plan '{}' (ID: {}){}\n\n\
                 IMPORTANT: The plan is now created. Your next step is to START EXECUTING the plan.\n\
                 Use file_write to create files, shell to run commands, etc.\n\
                 Do NOT create another plan - start working on the first task!",
                title, plan.info.id, task_info
            ),
        ))
    }

    /// Update an existing plan
    async fn update_plan(&self, tool_use_id: String, input: &Value) -> Result<ToolResult> {
        let plan_id = match input["plan_id"].as_str() {
            Some(id) => id
                .parse()
                .map_err(|_| TedError::InvalidInput(format!("Invalid plan_id: {}", id)))?,
            None => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    "plan_id is required for 'update' action",
                ))
            }
        };

        let content = match input["content"].as_str() {
            Some(c) => c,
            None => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    "content is required for 'update' action",
                ))
            }
        };

        let mut store = PlanStore::open()?;

        // Check plan exists
        if store.get_info(plan_id).is_none() {
            return Ok(ToolResult::error(
                tool_use_id,
                format!("Plan not found: {}", plan_id),
            ));
        }

        store.update(plan_id, content)?;

        // Get updated info
        let info = store.get_info(plan_id).unwrap();

        Ok(ToolResult::success(
            tool_use_id,
            format!(
                "Updated plan '{}' ({}/{} tasks complete)",
                info.title, info.completed_count, info.task_count
            ),
        ))
    }

    /// Set plan status
    async fn set_status(&self, tool_use_id: String, input: &Value) -> Result<ToolResult> {
        let plan_id = match input["plan_id"].as_str() {
            Some(id) => id
                .parse()
                .map_err(|_| TedError::InvalidInput(format!("Invalid plan_id: {}", id)))?,
            None => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    "plan_id is required for 'set_status' action",
                ))
            }
        };

        let status_str = match input["status"].as_str() {
            Some(s) => s,
            None => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    "status is required for 'set_status' action",
                ))
            }
        };

        let status = match status_str.to_lowercase().as_str() {
            "active" => PlanStatus::Active,
            "paused" => PlanStatus::Paused,
            "complete" | "completed" => PlanStatus::Complete,
            "archived" => PlanStatus::Archived,
            _ => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!(
                        "Invalid status: {}. Valid values: active, paused, complete, archived",
                        status_str
                    ),
                ))
            }
        };

        let mut store = PlanStore::open()?;

        // Check plan exists and get title
        let title = match store.get_info(plan_id) {
            Some(info) => info.title.clone(),
            None => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!("Plan not found: {}", plan_id),
                ))
            }
        };

        store.set_status(plan_id, status)?;

        Ok(ToolResult::success(
            tool_use_id,
            format!("Set plan '{}' status to {}", title, status.label()),
        ))
    }

    /// Add a progress log entry
    async fn add_log(&self, tool_use_id: String, input: &Value) -> Result<ToolResult> {
        let plan_id = match input["plan_id"].as_str() {
            Some(id) => id
                .parse()
                .map_err(|_| TedError::InvalidInput(format!("Invalid plan_id: {}", id)))?,
            None => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    "plan_id is required for 'add_log' action",
                ))
            }
        };

        let log_entry = match input["log_entry"].as_str() {
            Some(e) if !e.is_empty() => e,
            _ => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    "log_entry is required for 'add_log' action",
                ))
            }
        };

        let mut store = PlanStore::open()?;

        // Get the plan
        let mut plan = match store.get(plan_id)? {
            Some(p) => p,
            None => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!("Plan not found: {}", plan_id),
                ))
            }
        };

        // Add log entry and update
        plan.add_log_entry(log_entry);
        store.update(plan_id, &plan.content)?;

        Ok(ToolResult::success(
            tool_use_id,
            format!("Added progress log to plan '{}'", plan.info.title),
        ))
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
        let tool = PlanUpdateTool;
        assert_eq!(tool.name(), "plan_update");
    }

    #[test]
    fn test_tool_definition() {
        let tool = PlanUpdateTool;
        let def = tool.definition();
        assert_eq!(def.name, "plan_update");
        assert!(def.description.contains("plan"));
    }

    #[test]
    fn test_requires_permission() {
        let tool = PlanUpdateTool;
        assert!(!tool.requires_permission());
    }

    #[test]
    fn test_permission_request_create() {
        let tool = PlanUpdateTool;
        let input = serde_json::json!({
            "action": "create",
            "title": "Test Plan"
        });
        let request = tool.permission_request(&input).unwrap();
        assert!(request.action_description.contains("Create plan"));
        assert!(request.action_description.contains("Test Plan"));
    }

    #[test]
    fn test_permission_request_update() {
        let tool = PlanUpdateTool;
        let input = serde_json::json!({
            "action": "update",
            "plan_id": "123"
        });
        let request = tool.permission_request(&input).unwrap();
        assert!(request.action_description.contains("Update"));
    }

    #[tokio::test]
    async fn test_create_plan_missing_title() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "create"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("title is required"));
    }

    #[tokio::test]
    async fn test_update_plan_missing_plan_id() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "update",
                    "content": "test"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("plan_id is required"));
    }

    #[tokio::test]
    async fn test_unknown_action() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "unknown_action"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("Unknown action"));
    }

    #[tokio::test]
    async fn test_missing_action() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_set_status_invalid_status() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "set_status",
                    "plan_id": "550e8400-e29b-41d4-a716-446655440000",
                    "status": "invalid_status"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("Invalid status"));
    }

    #[tokio::test]
    async fn test_add_log_missing_entry() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "add_log",
                    "plan_id": "550e8400-e29b-41d4-a716-446655440000"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("log_entry is required"));
    }

    // ===== Additional permission_request tests =====

    #[test]
    fn test_permission_request_set_status() {
        let tool = PlanUpdateTool;
        let input = serde_json::json!({
            "action": "set_status",
            "plan_id": "123",
            "status": "complete"
        });
        let request = tool.permission_request(&input).unwrap();
        assert!(request.action_description.contains("status"));
        assert!(!request.is_destructive);
        assert!(request.affected_paths.is_empty());
    }

    #[test]
    fn test_permission_request_add_log() {
        let tool = PlanUpdateTool;
        let input = serde_json::json!({
            "action": "add_log",
            "plan_id": "123",
            "log_entry": "Made progress"
        });
        let request = tool.permission_request(&input).unwrap();
        assert!(request.action_description.contains("log"));
        assert_eq!(request.tool_name, "plan_update");
    }

    #[test]
    fn test_permission_request_unknown_action() {
        let tool = PlanUpdateTool;
        let input = serde_json::json!({
            "action": "foobar"
        });
        let request = tool.permission_request(&input).unwrap();
        assert!(request.action_description.contains("foobar"));
    }

    #[test]
    fn test_permission_request_missing_action() {
        let tool = PlanUpdateTool;
        let input = serde_json::json!({});
        let request = tool.permission_request(&input).unwrap();
        // Should default to "unknown"
        assert!(request.action_description.contains("unknown"));
    }

    #[test]
    fn test_permission_request_missing_title() {
        let tool = PlanUpdateTool;
        let input = serde_json::json!({
            "action": "create"
        });
        let request = tool.permission_request(&input).unwrap();
        // Should default to "plan"
        assert!(request.action_description.contains("plan"));
    }

    // ===== Additional execute validation tests =====

    #[tokio::test]
    async fn test_update_plan_missing_content() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "update",
                    "plan_id": "550e8400-e29b-41d4-a716-446655440000"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("content is required"));
    }

    #[tokio::test]
    async fn test_set_status_missing_plan_id() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "set_status",
                    "status": "complete"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("plan_id is required"));
    }

    #[tokio::test]
    async fn test_set_status_missing_status() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "set_status",
                    "plan_id": "550e8400-e29b-41d4-a716-446655440000"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("status is required"));
    }

    #[tokio::test]
    async fn test_add_log_missing_plan_id() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "add_log",
                    "log_entry": "Some progress"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("plan_id is required"));
    }

    #[tokio::test]
    async fn test_add_log_empty_entry() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "add_log",
                    "plan_id": "550e8400-e29b-41d4-a716-446655440000",
                    "log_entry": ""
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("log_entry is required"));
    }

    #[tokio::test]
    async fn test_create_plan_empty_title() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "create",
                    "title": ""
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("title is required"));
    }

    #[tokio::test]
    async fn test_update_plan_invalid_plan_id() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        // Invalid plan_id returns Err, not ToolResult::error
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "update",
                    "plan_id": "not-a-valid-uuid",
                    "content": "test content"
                }),
                &context,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_set_status_invalid_plan_id() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        // Invalid plan_id returns Err, not ToolResult::error
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "set_status",
                    "plan_id": "not-a-valid-uuid",
                    "status": "complete"
                }),
                &context,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_add_log_invalid_plan_id() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        // Invalid plan_id returns Err, not ToolResult::error
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "add_log",
                    "plan_id": "not-a-valid-uuid",
                    "log_entry": "some log"
                }),
                &context,
            )
            .await;

        assert!(result.is_err());
    }

    // ===== Status value tests =====

    #[tokio::test]
    async fn test_set_status_valid_statuses() {
        // Test that all valid status strings are recognized as valid
        // (plan not found is the expected error, not invalid status)
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);
        let valid_plan_id = "550e8400-e29b-41d4-a716-446655440000";

        for status in &["active", "paused", "complete", "completed", "archived"] {
            let result = tool
                .execute(
                    "test-id".to_string(),
                    serde_json::json!({
                        "action": "set_status",
                        "plan_id": valid_plan_id,
                        "status": status
                    }),
                    &context,
                )
                .await
                .unwrap();

            // Should fail with "Plan not found", not "Invalid status"
            assert!(result.is_error());
            assert!(
                result.output_text().contains("Plan not found"),
                "Status '{}' should be valid, got: {}",
                status,
                result.output_text()
            );
        }
    }

    #[tokio::test]
    async fn test_set_status_case_insensitive() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);
        let valid_plan_id = "550e8400-e29b-41d4-a716-446655440000";

        for status in &[
            "ACTIVE", "Active", "PAUSED", "Paused", "COMPLETE", "Complete",
        ] {
            let result = tool
                .execute(
                    "test-id".to_string(),
                    serde_json::json!({
                        "action": "set_status",
                        "plan_id": valid_plan_id,
                        "status": status
                    }),
                    &context,
                )
                .await
                .unwrap();

            // Should fail with "Plan not found", not "Invalid status"
            assert!(result.is_error());
            assert!(
                result.output_text().contains("Plan not found"),
                "Status '{}' should be valid (case insensitive), got: {}",
                status,
                result.output_text()
            );
        }
    }

    #[tokio::test]
    async fn test_update_plan_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "update",
                    "plan_id": "550e8400-e29b-41d4-a716-446655440000",
                    "content": "new content"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("Plan not found"));
    }

    #[tokio::test]
    async fn test_add_log_plan_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "add_log",
                    "plan_id": "550e8400-e29b-41d4-a716-446655440000",
                    "log_entry": "some log entry"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("Plan not found"));
    }

    // ===== Tool definition tests =====

    #[test]
    fn test_definition_has_all_parameters() {
        let tool = PlanUpdateTool;
        let def = tool.definition();

        // Check that the input schema contains expected properties
        let schema = &def.input_schema;
        let properties = schema.properties.as_object().unwrap();

        assert!(properties.contains_key("action"));
        assert!(properties.contains_key("title"));
        assert!(properties.contains_key("content"));
        assert!(properties.contains_key("plan_id"));
        assert!(properties.contains_key("status"));
        assert!(properties.contains_key("log_entry"));
    }

    #[test]
    fn test_definition_action_is_required() {
        let tool = PlanUpdateTool;
        let def = tool.definition();

        // required is a Vec<String> on ToolInputSchema
        assert!(def.input_schema.required.contains(&"action".to_string()));
    }

    #[test]
    fn test_definition_description_mentions_actions() {
        let tool = PlanUpdateTool;
        let def = tool.definition();

        // The description should mention the different capabilities
        // Check for case-insensitive matches since descriptions may vary
        let desc_lower = def.description.to_lowercase();
        assert!(
            desc_lower.contains("create") || desc_lower.contains("new"),
            "Description should mention creating: {}",
            def.description
        );
        assert!(
            desc_lower.contains("update") || desc_lower.contains("modify"),
            "Description should mention updating: {}",
            def.description
        );
    }

    // ===== Edge case tests =====

    #[tokio::test]
    async fn test_action_with_extra_fields() {
        let temp_dir = TempDir::new().unwrap();
        let tool = PlanUpdateTool;
        let context = create_test_context(&temp_dir);

        // Extra fields should be ignored
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "action": "create",
                    "title": "",
                    "extra_field": "should be ignored",
                    "another_extra": 123
                }),
                &context,
            )
            .await
            .unwrap();

        // Should still fail for empty title, not for extra fields
        assert!(result.is_error());
        assert!(result.output_text().contains("title is required"));
    }

    #[test]
    fn test_permission_request_fields() {
        let tool = PlanUpdateTool;
        let input = serde_json::json!({
            "action": "create",
            "title": "My Plan"
        });
        let request = tool.permission_request(&input).unwrap();

        // Verify all fields are set correctly
        assert_eq!(request.tool_name, "plan_update");
        assert!(!request.is_destructive);
        assert!(request.affected_paths.is_empty());
    }
}
