// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use regex::Regex;
use std::collections::HashSet;
use std::sync::Arc;

use crate::chat;
use crate::error::Result;
use crate::tools::ToolExecutor;

/// Create a hash key for deduplicating tool calls.
pub(super) fn tool_call_key(name: &str, input: &serde_json::Value) -> String {
    format!("{}:{}", name, input)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolCallAdapter {
    GenericJson,
    QwenXml,
}

fn adapters_for_context(provider_name: &str, model: &str) -> Vec<ToolCallAdapter> {
    let mut adapters = vec![ToolCallAdapter::GenericJson];
    let context = format!("{} {}", provider_name.to_lowercase(), model.to_lowercase());

    if context.contains("qwen") {
        adapters.push(ToolCallAdapter::QwenXml);
    }

    adapters
}

pub(super) fn extract_tool_uses_from_text_with_adapters(
    text: &str,
    provider_name: &str,
    model: &str,
) -> Vec<(String, serde_json::Value)> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut tool_uses = Vec::new();
    let mut seen = HashSet::new();

    for adapter in adapters_for_context(provider_name, model) {
        let extracted = match adapter {
            ToolCallAdapter::GenericJson => extract_json_tool_uses(trimmed),
            ToolCallAdapter::QwenXml => extract_qwen_xml_tool_uses(trimmed),
        };

        for (name, input) in extracted {
            let key = tool_call_key(&name, &input);
            if seen.insert(key) {
                tool_uses.push((name, input));
            }
        }
    }

    tool_uses
}

fn extract_json_tool_uses(text: &str) -> Vec<(String, serde_json::Value)> {
    let mut candidates = Vec::new();
    let mut seen_candidates = HashSet::new();

    let mut add_candidate = |candidate: &str| {
        let c = candidate.trim();
        if c.is_empty() || seen_candidates.contains(c) {
            return;
        }
        seen_candidates.insert(c.to_string());
        candidates.push(c.to_string());
    };

    add_candidate(text);

    let code_fence_re =
        Regex::new(r"(?s)```(?:json)?\s*(.*?)\s*```").expect("code fence regex should be valid");
    for capture in code_fence_re.captures_iter(text) {
        if let Some(body) = capture.get(1) {
            add_candidate(body.as_str());
        }
    }

    let mut tool_uses: Vec<(String, serde_json::Value)> = Vec::new();
    let mut seen_tool_calls = HashSet::new();

    for candidate in candidates {
        if let Some(value) = parse_json_candidate(&candidate) {
            collect_tool_uses_from_value(&value, &mut tool_uses, &mut seen_tool_calls);
        }
    }

    tool_uses
}

fn parse_json_candidate(candidate: &str) -> Option<serde_json::Value> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(candidate) {
        return Some(value);
    }

    let first_brace = candidate.find('{')?;
    let last_brace = candidate.rfind('}')?;
    if last_brace <= first_brace {
        return None;
    }

    serde_json::from_str::<serde_json::Value>(&candidate[first_brace..=last_brace]).ok()
}

fn collect_tool_uses_from_value(
    value: &serde_json::Value,
    tool_uses: &mut Vec<(String, serde_json::Value)>,
    seen_tool_calls: &mut HashSet<String>,
) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                collect_tool_uses_from_value(item, tool_uses, seen_tool_calls);
            }
        }
        serde_json::Value::Object(obj) => {
            if let Some((name, input)) = parse_tool_from_json(value) {
                let key = tool_call_key(&name, &input);
                if seen_tool_calls.insert(key) {
                    tool_uses.push((name, input));
                }
            }

            // OpenAI-compatible tool call chunk: {"function":{"name":"...","arguments":...}}
            if let Some(function) = obj.get("function").and_then(|v| v.as_object()) {
                if let Some(name) = function.get("name").and_then(|v| v.as_str()) {
                    let synthetic = serde_json::json!({
                        "name": name,
                        "arguments": function.get("arguments").cloned().unwrap_or_else(|| serde_json::json!({}))
                    });
                    if let Some((mapped_name, mapped_input)) = parse_tool_from_json(&synthetic) {
                        let key = tool_call_key(&mapped_name, &mapped_input);
                        if seen_tool_calls.insert(key) {
                            tool_uses.push((mapped_name, mapped_input));
                        }
                    }
                }
            }

            for key in ["tool_calls", "calls", "actions", "content"] {
                if let Some(nested) = obj.get(key) {
                    collect_tool_uses_from_value(nested, tool_uses, seen_tool_calls);
                }
            }
        }
        _ => {}
    }
}

fn extract_qwen_xml_tool_uses(text: &str) -> Vec<(String, serde_json::Value)> {
    if !text.contains("<function=") {
        return Vec::new();
    }

    let tool_call_re =
        Regex::new(r"(?is)<tool_call>\s*(.*?)\s*</tool_call>").expect("valid tool_call regex");
    let function_re =
        Regex::new(r#"(?is)<function\s*=\s*["']?([^>\s"']+)["']?\s*>(.*?)</function>"#)
            .expect("valid function regex");
    let parameter_re =
        Regex::new(r#"(?is)<parameter\s*=\s*["']?([^>\s"']+)["']?\s*>(.*?)</parameter>"#)
            .expect("valid parameter regex");

    let mut scopes = Vec::new();
    for capture in tool_call_re.captures_iter(text) {
        if let Some(scope) = capture.get(1) {
            scopes.push(scope.as_str().to_string());
        }
    }
    if scopes.is_empty() {
        scopes.push(text.to_string());
    }

    let mut extracted = Vec::new();
    for scope in scopes {
        for function_capture in function_re.captures_iter(&scope) {
            let Some(name_match) = function_capture.get(1) else {
                continue;
            };
            let Some(body_match) = function_capture.get(2) else {
                continue;
            };

            let function_name = name_match.as_str().trim();
            if function_name.is_empty() {
                continue;
            }

            let mut arguments = serde_json::Map::new();
            for parameter_capture in parameter_re.captures_iter(body_match.as_str()) {
                let Some(param_name_match) = parameter_capture.get(1) else {
                    continue;
                };
                let Some(param_value_match) = parameter_capture.get(2) else {
                    continue;
                };

                let param_name = param_name_match.as_str().trim();
                if param_name.is_empty() {
                    continue;
                }

                arguments.insert(
                    param_name.to_string(),
                    parse_qwen_parameter_value(param_value_match.as_str()),
                );
            }

            let synthetic = serde_json::json!({
                "name": function_name,
                "arguments": serde_json::Value::Object(arguments.clone()),
            });

            if let Some((mapped_name, mapped_input)) = parse_tool_from_json(&synthetic) {
                extracted.push((mapped_name, mapped_input));
            } else {
                extracted.push((
                    function_name.to_string(),
                    serde_json::Value::Object(arguments),
                ));
            }
        }
    }

    extracted
}

fn parse_qwen_parameter_value(raw_value: &str) -> serde_json::Value {
    let trimmed = raw_value.trim();
    if trimmed.is_empty() {
        return serde_json::Value::String(String::new());
    }

    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return parsed;
    }

    serde_json::Value::String(trimmed.to_string())
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
