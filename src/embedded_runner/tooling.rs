// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::sync::Arc;

use crate::chat;
use crate::error::Result;
use crate::tools::ToolExecutor;

/// Create a hash key for deduplicating tool calls.
#[cfg(test)]
pub(super) fn tool_call_key(name: &str, input: &serde_json::Value) -> String {
    format!("{}:{}", name, input)
}

/// Parse a tool call from a JSON value.
pub(super) fn parse_tool_from_json(
    value: &serde_json::Value,
) -> Option<(String, serde_json::Value)> {
    let obj = value.as_object()?;

    // Look for {"name": "...", "arguments": {...}} or {"name": "...", "input": {...}} format.
    let name = obj.get("name")?.as_str()?;

    // Try "arguments" first, then "input" as fallback (different LLM output formats).
    let arguments = obj.get("arguments").or_else(|| obj.get("input")).cloned()?;

    // Map tool names: normalize various formats to our internal tool names.
    let mapped_name = match name {
        "file_read" | "read_file" => "file_read",
        "file_edit" | "edit_file" => "file_edit",
        "file_create" | "create_file" | "file_write" | "write_file" => "file_write",
        "file_delete" | "delete_file" => "file_delete",
        _ => name,
    };

    // Map argument names for various tools: different models use different names.
    let mapped_arguments = match mapped_name {
        "file_read" => map_file_read_arguments(&arguments),
        "file_edit" => map_file_edit_arguments(&arguments),
        "file_write" => map_file_write_arguments(&arguments),
        "shell" => map_shell_arguments(&arguments),
        _ => arguments,
    };

    // Special case: file_edit with empty old_string should become file_write.
    let (final_name, final_args) = if mapped_name == "file_edit" {
        let old_string = mapped_arguments
            .get("old_string")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if old_string.is_empty() || old_string.trim().is_empty() {
            // Convert to file_write: rename new_string to content.
            let mut write_args = serde_json::Map::new();
            if let Some(path) = mapped_arguments.get("path") {
                write_args.insert("path".to_string(), path.clone());
            }
            if let Some(new_content) = mapped_arguments.get("new_string") {
                write_args.insert("content".to_string(), new_content.clone());
            }
            (
                "file_write".to_string(),
                serde_json::Value::Object(write_args),
            )
        } else {
            (mapped_name.to_string(), mapped_arguments)
        }
    } else {
        (mapped_name.to_string(), mapped_arguments)
    };

    Some((final_name, final_args))
}

/// Map file_edit argument names from various LLM output formats to our expected format.
pub(super) fn map_file_edit_arguments(args: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = args.as_object() else {
        return args.clone();
    };

    let mut mapped = serde_json::Map::new();

    for (key, value) in obj {
        let mapped_key = match key.as_str() {
            "old_text" | "oldText" | "old_content" | "oldContent" | "find" | "search"
            | "original" | "old" | "before" | "pattern" | "target" | "match" => "old_string",
            "new_text" | "newText" | "new_content" | "newContent" | "replace" | "replacement"
            | "modified" | "new" | "after" | "content" | "updated" | "with" => "new_string",
            "file" | "file_path" | "filepath" | "filename" | "file_name" => "path",
            "old_string" => "old_string",
            "new_string" => "new_string",
            "path" => "path",
            _ => key.as_str(),
        };

        let mapped_value =
            if (mapped_key == "old_string" || mapped_key == "new_string") && value.is_array() {
                if let Some(arr) = value.as_array() {
                    let joined = arr
                        .iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    serde_json::Value::String(joined)
                } else {
                    value.clone()
                }
            } else {
                value.clone()
            };

        mapped.insert(mapped_key.to_string(), mapped_value);
    }

    serde_json::Value::Object(mapped)
}

/// Map file_read/read_file argument names from various LLM output formats.
pub(super) fn map_file_read_arguments(args: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = args.as_object() else {
        return args.clone();
    };

    let mut mapped = serde_json::Map::new();

    for (key, value) in obj {
        let mapped_key = match key.as_str() {
            "file" | "file_path" | "filepath" | "filename" | "name" | "file_name" => "path",
            _ => key.as_str(),
        };
        mapped.insert(mapped_key.to_string(), value.clone());
    }

    serde_json::Value::Object(mapped)
}

/// Map file_write argument names from various LLM output formats.
pub(super) fn map_file_write_arguments(args: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = args.as_object() else {
        return args.clone();
    };

    let mut mapped = serde_json::Map::new();

    for (key, value) in obj {
        let mapped_key = match key.as_str() {
            "file" | "file_path" | "filepath" | "filename" | "name" | "file_name" => "path",
            "text" | "data" | "contents" | "file_content" | "code" | "body" => "content",
            _ => key.as_str(),
        };
        mapped.insert(mapped_key.to_string(), value.clone());
    }

    serde_json::Value::Object(mapped)
}

/// Map shell/command argument names from various LLM output formats.
pub(super) fn map_shell_arguments(args: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = args.as_object() else {
        return args.clone();
    };

    let mut mapped = serde_json::Map::new();

    for (key, value) in obj {
        let mapped_key = match key.as_str() {
            "cmd" | "shell_command" | "bash" | "exec" | "run" => "command",
            _ => key.as_str(),
        };
        mapped.insert(mapped_key.to_string(), value.clone());
    }

    serde_json::Value::Object(mapped)
}

pub(super) fn is_file_mod_tool(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "file_write" | "file_edit" | "file_delete" | "create_file" | "edit_file" | "delete_file"
    )
}

fn review_mode_mock_result(name: &str, input: &serde_json::Value) -> String {
    match name {
        "file_write" | "create_file" => {
            let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("file");
            format!("Successfully created {} (pending review)", path)
        }
        "file_edit" | "edit_file" => {
            let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("file");
            format!("Successfully edited {} (pending review)", path)
        }
        "file_delete" | "delete_file" => {
            let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("file");
            format!("Successfully deleted {} (pending review)", path)
        }
        _ => "Operation completed (pending review)".to_string(),
    }
}

pub(super) struct EmbeddedToolExecutionStrategy {
    pub review_mode: bool,
}

#[async_trait::async_trait(?Send)]
impl chat::engine::ToolExecutionStrategy for EmbeddedToolExecutionStrategy {
    async fn execute_tool_calls(
        &mut self,
        tool_executor: &mut ToolExecutor,
        calls: &[chat::engine::ToolUse],
        _interrupted: &Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<chat::engine::ToolExecutionBatch> {
        let mut results = Vec::with_capacity(calls.len());

        for (id, name, input) in calls {
            if self.review_mode && is_file_mod_tool(name) {
                results.push(crate::tools::ToolResult::success(
                    id.clone(),
                    review_mode_mock_result(name, input),
                ));
                continue;
            }

            results.push(
                tool_executor
                    .execute_tool_use(id, name, input.clone())
                    .await?,
            );
        }

        Ok(chat::engine::ToolExecutionBatch {
            results,
            cancelled_tool_use_ids: Vec::new(),
        })
    }
}
