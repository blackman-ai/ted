// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Glob pattern matching tool
//!
//! Finds files matching a glob pattern.

use async_trait::async_trait;
use glob::glob as glob_match;
use serde_json::Value;
use std::path::PathBuf;

use crate::error::Result;
use crate::llm::provider::ToolDefinition;
use crate::tools::{PermissionRequest, SchemaBuilder, Tool, ToolContext, ToolResult};

/// Tool for finding files by glob pattern
pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "glob".to_string(),
            description: "Find files matching a glob pattern (e.g., '**/*.rs', 'src/**/*.ts'). Returns matching file paths.".to_string(),
            input_schema: SchemaBuilder::new()
                .string("pattern", "Glob pattern to match (e.g., '**/*.rs', 'src/**/*.ts')", true)
                .string("path", "Base directory to search in (default: working directory)", false)
                .integer("limit", "Maximum number of results (default: 100)", false)
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
        let pattern = input["pattern"]
            .as_str()
            .or_else(|| input["glob"].as_str())
            .or_else(|| input["query"].as_str())
            .or_else(|| input["search"].as_str())
            .ok_or_else(|| {
                crate::error::TedError::InvalidInput("pattern is required".to_string())
            })?;

        let limit = input["limit"]
            .as_u64()
            .or_else(|| input["max"].as_u64())
            .or_else(|| input["count"].as_u64())
            .unwrap_or(100) as usize;

        // Resolve base path with flexible parameter names
        let path_str = input["path"]
            .as_str()
            .or_else(|| input["dir"].as_str())
            .or_else(|| input["directory"].as_str())
            .or_else(|| input["base"].as_str());
        let base_path = if let Some(path_str) = path_str {
            if PathBuf::from(path_str).is_absolute() {
                PathBuf::from(path_str)
            } else {
                context.working_directory.join(path_str)
            }
        } else {
            context.working_directory.clone()
        };

        // Build full pattern
        let full_pattern = base_path.join(pattern);
        let pattern_str = full_pattern.to_string_lossy();

        // Execute glob
        let entries = match glob_match(&pattern_str) {
            Ok(paths) => paths,
            Err(e) => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!("Invalid glob pattern: {}", e),
                ));
            }
        };

        let mut results: Vec<String> = Vec::new();
        let mut count = 0;
        let mut errors = 0;

        for entry in entries {
            match entry {
                Ok(path) => {
                    // Make path relative to working directory if possible
                    let display_path = path
                        .strip_prefix(&context.working_directory)
                        .map(|p| p.to_path_buf())
                        .unwrap_or(path);

                    results.push(display_path.to_string_lossy().to_string());
                    count += 1;

                    if count >= limit {
                        break;
                    }
                }
                Err(_) => {
                    errors += 1;
                }
            }
        }

        // Sort results for consistent output
        results.sort();

        // Emit recall event for memory tracking with found file paths
        if !results.is_empty() {
            let paths: Vec<PathBuf> = results.iter().map(PathBuf::from).collect();
            context.emit_search_match(paths);
        }

        let mut output = String::new();
        output.push_str(&format!(
            "Found {} files matching '{}'",
            results.len(),
            pattern
        ));

        if count >= limit {
            output.push_str(&format!(" (limited to {})", limit));
        }
        if errors > 0 {
            output.push_str(&format!(" ({} paths had errors)", errors));
        }
        output.push_str(":\n\n");

        for path in &results {
            output.push_str(path);
            output.push('\n');
        }

        Ok(ToolResult::success(tool_use_id, output))
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let pattern = input["pattern"].as_str().unwrap_or("*");
        Some(PermissionRequest {
            tool_name: "glob".to_string(),
            action_description: format!("Search for files matching: {}", pattern),
            affected_paths: vec![],
            is_destructive: false,
        })
    }

    fn requires_permission(&self) -> bool {
        false // Glob is read-only
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
        let tool = GlobTool;
        assert_eq!(tool.name(), "glob");
    }

    #[test]
    fn test_tool_definition() {
        let tool = GlobTool;
        let def = tool.definition();
        assert_eq!(def.name, "glob");
        assert!(def.description.contains("glob"));
    }

    #[test]
    fn test_requires_permission() {
        let tool = GlobTool;
        assert!(!tool.requires_permission());
    }

    #[test]
    fn test_permission_request() {
        let tool = GlobTool;
        let input = serde_json::json!({"pattern": "**/*.rs"});
        let request = tool.permission_request(&input).unwrap();
        assert_eq!(request.tool_name, "glob");
        assert!(request.action_description.contains("**/*.rs"));
        assert!(!request.is_destructive);
    }

    #[tokio::test]
    async fn test_glob_finds_files() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test1.rs"), "fn main() {}").unwrap();
        std::fs::write(temp_dir.path().join("test2.rs"), "fn test() {}").unwrap();
        std::fs::write(temp_dir.path().join("other.txt"), "text").unwrap();

        let tool = GlobTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"pattern": "*.rs"}),
                &context,
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
    async fn test_glob_recursive() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();
        std::fs::write(temp_dir.path().join("root.rs"), "fn root() {}").unwrap();
        std::fs::write(
            temp_dir.path().join("subdir").join("nested.rs"),
            "fn nested() {}",
        )
        .unwrap();

        let tool = GlobTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"pattern": "**/*.rs"}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let output = result.output_text();
        assert!(output.contains("root.rs"));
        assert!(output.contains("nested.rs"));
    }

    #[tokio::test]
    async fn test_glob_with_limit() {
        let temp_dir = TempDir::new().unwrap();
        for i in 0..10 {
            std::fs::write(temp_dir.path().join(format!("file{}.txt", i)), "content").unwrap();
        }

        let tool = GlobTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "pattern": "*.txt",
                    "limit": 3
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("limited to 3"));
    }

    #[tokio::test]
    async fn test_glob_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let tool = GlobTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"pattern": "*.rs"}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("Found 0 files"));
    }

    #[tokio::test]
    async fn test_glob_with_custom_path() {
        let temp_dir = TempDir::new().unwrap();
        let subdir = temp_dir.path().join("src");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(subdir.join("lib.rs"), "mod lib;").unwrap();
        std::fs::write(temp_dir.path().join("main.rs"), "fn main() {}").unwrap();

        let tool = GlobTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "pattern": "*.rs",
                    "path": "src"
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let output = result.output_text();
        assert!(output.contains("lib.rs"));
    }

    #[tokio::test]
    async fn test_glob_missing_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let tool = GlobTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_glob_sorted_output() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("zebra.txt"), "z").unwrap();
        std::fs::write(temp_dir.path().join("apple.txt"), "a").unwrap();
        std::fs::write(temp_dir.path().join("middle.txt"), "m").unwrap();

        let tool = GlobTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"pattern": "*.txt"}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let output = result.output_text();
        let apple_pos = output.find("apple.txt").unwrap();
        let middle_pos = output.find("middle.txt").unwrap();
        let zebra_pos = output.find("zebra.txt").unwrap();
        assert!(apple_pos < middle_pos);
        assert!(middle_pos < zebra_pos);
    }
}
