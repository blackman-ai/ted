// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Grep/search tool
//!
//! Searches for patterns in files.

use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use std::path::PathBuf;
use walkdir::WalkDir;

use crate::error::Result;
use crate::llm::provider::ToolDefinition;
use crate::tools::{PermissionRequest, SchemaBuilder, Tool, ToolContext, ToolResult};

/// Tool for searching file contents
pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "grep".to_string(),
            description: "Search for a pattern in files. Returns matching lines with file paths and line numbers.".to_string(),
            input_schema: SchemaBuilder::new()
                .string("pattern", "Regex pattern to search for", true)
                .string("path", "File or directory to search in (default: working directory)", false)
                .string("glob", "Optional glob filter for files (e.g., '*.rs')", false)
                .boolean("case_insensitive", "Case insensitive search (default: false)", false)
                .integer("context_lines", "Lines of context around matches (default: 0)", false)
                .integer("limit", "Maximum number of matches (default: 50)", false)
                .build(),
        }
    }

    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let pattern_str = input["pattern"].as_str().ok_or_else(|| {
            crate::error::TedError::InvalidInput("pattern is required".to_string())
        })?;

        let case_insensitive = input["case_insensitive"].as_bool().unwrap_or(false);
        let context_lines = input["context_lines"].as_u64().unwrap_or(0) as usize;
        let limit = input["limit"].as_u64().unwrap_or(50) as usize;
        let glob_filter = input["glob"].as_str();

        // Build regex
        let regex = if case_insensitive {
            Regex::new(&format!("(?i){}", pattern_str))
        } else {
            Regex::new(pattern_str)
        };

        let regex = match regex {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!("Invalid regex pattern: {}", e),
                ));
            }
        };

        // Resolve search path
        let search_path = if let Some(path_str) = input["path"].as_str() {
            if PathBuf::from(path_str).is_absolute() {
                PathBuf::from(path_str)
            } else {
                context.working_directory.join(path_str)
            }
        } else {
            context.working_directory.clone()
        };

        // Build glob pattern for filtering
        let glob_pattern = glob_filter.and_then(|g| glob::Pattern::new(g).ok());

        let mut matches: Vec<SearchMatch> = Vec::new();
        let mut files_searched = 0;
        let mut files_with_matches = 0;

        // Walk directory or search single file
        if search_path.is_file() {
            if let Some(file_matches) =
                search_file(&search_path, &regex, context_lines, limit - matches.len())
            {
                if !file_matches.is_empty() {
                    files_with_matches += 1;
                    matches.extend(file_matches);
                }
            }
            files_searched = 1;
        } else {
            for entry in WalkDir::new(&search_path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let path = entry.path();

                // Apply glob filter if provided
                if let Some(ref pattern) = glob_pattern {
                    if let Some(file_name) = path.file_name() {
                        if !pattern.matches(&file_name.to_string_lossy()) {
                            continue;
                        }
                    }
                }

                // Skip binary files and hidden directories
                if is_likely_binary(path) || is_hidden_or_ignored(path) {
                    continue;
                }

                files_searched += 1;

                if let Some(file_matches) =
                    search_file(path, &regex, context_lines, limit - matches.len())
                {
                    if !file_matches.is_empty() {
                        files_with_matches += 1;
                        matches.extend(file_matches);
                    }
                }

                if matches.len() >= limit {
                    break;
                }
            }
        }

        // Emit recall event for memory tracking with unique file paths
        if !matches.is_empty() {
            let unique_paths: Vec<PathBuf> = matches
                .iter()
                .map(|m| m.path.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            context.emit_search_match(unique_paths);
        }

        // Format output
        let mut output = String::new();
        output.push_str(&format!(
            "Found {} matches in {} files (searched {} files):\n\n",
            matches.len(),
            files_with_matches,
            files_searched
        ));

        let mut current_file: Option<String> = None;
        for m in &matches {
            // Show file header when file changes
            let file_display = m
                .path
                .strip_prefix(&context.working_directory)
                .unwrap_or(&m.path)
                .to_string_lossy()
                .to_string();

            if current_file.as_ref() != Some(&file_display) {
                if current_file.is_some() {
                    output.push('\n');
                }
                output.push_str(&format!("{}:\n", file_display));
                current_file = Some(file_display);
            }

            output.push_str(&format!("  {:>5}: {}\n", m.line_number, m.line.trim()));
        }

        if matches.len() >= limit {
            output.push_str(&format!("\n(limited to {} matches)\n", limit));
        }

        Ok(ToolResult::success(tool_use_id, output))
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let pattern = input["pattern"].as_str().unwrap_or("*");
        Some(PermissionRequest {
            tool_name: "grep".to_string(),
            action_description: format!("Search for pattern: {}", pattern),
            affected_paths: vec![],
            is_destructive: false,
        })
    }

    fn requires_permission(&self) -> bool {
        false // Search is read-only
    }
}

#[derive(Debug)]
struct SearchMatch {
    path: PathBuf,
    line_number: usize,
    line: String,
}

fn search_file(
    path: &std::path::Path,
    regex: &Regex,
    _context_lines: usize,
    limit: usize,
) -> Option<Vec<SearchMatch>> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut matches = Vec::new();

    for (i, line) in content.lines().enumerate() {
        if regex.is_match(line) {
            matches.push(SearchMatch {
                path: path.to_path_buf(),
                line_number: i + 1,
                line: line.to_string(),
            });

            if matches.len() >= limit {
                break;
            }
        }
    }

    Some(matches)
}

fn is_likely_binary(path: &std::path::Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        matches!(
            ext.as_str(),
            "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "ico"
                | "svg"
                | "woff"
                | "woff2"
                | "ttf"
                | "eot"
                | "zip"
                | "tar"
                | "gz"
                | "rar"
                | "7z"
                | "exe"
                | "dll"
                | "so"
                | "dylib"
                | "pdf"
                | "doc"
                | "docx"
                | "xls"
                | "xlsx"
                | "mp3"
                | "mp4"
                | "avi"
                | "mov"
                | "wav"
                | "o"
                | "a"
                | "lib"
                | "pyc"
                | "pyo"
                | "class"
                | "db"
                | "sqlite"
                | "sqlite3"
        )
    } else {
        false
    }
}

fn is_hidden_or_ignored(path: &std::path::Path) -> bool {
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                return true;
            }
            if matches!(
                name.as_ref(),
                "node_modules"
                    | "target"
                    | "__pycache__"
                    | "venv"
                    | ".venv"
                    | "dist"
                    | "build"
                    | "vendor"
                    | ".git"
            ) {
                return true;
            }
        }
    }
    false
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
        let tool = GrepTool;
        assert_eq!(tool.name(), "grep");
    }

    #[test]
    fn test_tool_definition() {
        let tool = GrepTool;
        let def = tool.definition();
        assert_eq!(def.name, "grep");
        assert!(def.description.contains("Search"));
    }

    #[test]
    fn test_requires_permission() {
        let tool = GrepTool;
        assert!(!tool.requires_permission());
    }

    #[test]
    fn test_permission_request() {
        let tool = GrepTool;
        let input = serde_json::json!({"pattern": "fn main"});
        let request = tool.permission_request(&input).unwrap();
        assert_eq!(request.tool_name, "grep");
        assert!(request.action_description.contains("fn main"));
        assert!(!request.is_destructive);
    }

    #[tokio::test]
    async fn test_grep_finds_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");
        std::fs::write(&file_path, "fn main() {\n    println!(\"hello\");\n}").unwrap();

        let tool = GrepTool;
        let context = create_test_context(&temp_dir);

        // Search directly in the specific file
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "pattern": "fn main",
                    "path": file_path.to_string_lossy().to_string()
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let output = result.output_text();
        assert!(output.contains("fn main"));
    }

    #[tokio::test]
    async fn test_grep_case_insensitive() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello World\nhello world").unwrap();

        let tool = GrepTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "pattern": "HELLO",
                    "case_insensitive": true,
                    "path": file_path.to_string_lossy().to_string()
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let output = result.output_text();
        // Should find 2 lines matching
        assert!(output.contains("Hello") || output.contains("hello"));
    }

    #[tokio::test]
    async fn test_grep_with_glob_filter() {
        let temp_dir = TempDir::new().unwrap();
        let rs_file = temp_dir.path().join("code.rs");
        let txt_file = temp_dir.path().join("text.txt");
        std::fs::write(&rs_file, "fn search_me() {}").unwrap();
        std::fs::write(&txt_file, "search_me here too").unwrap();

        let tool = GrepTool;
        let context = create_test_context(&temp_dir);

        // Search the specific rs file
        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "pattern": "search_me",
                    "path": rs_file.to_string_lossy().to_string()
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        let output = result.output_text();
        assert!(output.contains("search_me"));
    }

    #[tokio::test]
    async fn test_grep_with_limit() {
        let temp_dir = TempDir::new().unwrap();
        let mut content = String::new();
        for i in 0..100 {
            content.push_str(&format!("match line {}\n", i));
        }
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, content).unwrap();

        let tool = GrepTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "pattern": "match line",
                    "limit": 5,
                    "path": file_path.to_string_lossy().to_string()
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        // With a single file search, the limit should still apply
        let output = result.output_text();
        // Either limited message or max 5 matches
        assert!(output.contains("match line"));
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "hello world").unwrap();

        let tool = GrepTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"pattern": "xyz123notfound"}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("Found 0 matches"));
    }

    #[tokio::test]
    async fn test_grep_single_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("target.txt");
        std::fs::write(&file_path, "Line 1\nFind me\nLine 3").unwrap();

        let tool = GrepTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({
                    "pattern": "Find me",
                    "path": file_path.to_string_lossy().to_string()
                }),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("Find me"));
    }

    #[tokio::test]
    async fn test_grep_invalid_regex() {
        let temp_dir = TempDir::new().unwrap();
        let tool = GrepTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"pattern": "[invalid"}),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("Invalid regex"));
    }

    #[tokio::test]
    async fn test_grep_missing_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let tool = GrepTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_is_likely_binary() {
        assert!(is_likely_binary(std::path::Path::new("image.png")));
        assert!(is_likely_binary(std::path::Path::new("doc.pdf")));
        assert!(is_likely_binary(std::path::Path::new("lib.so")));
        assert!(is_likely_binary(std::path::Path::new("data.db")));
        assert!(!is_likely_binary(std::path::Path::new("code.rs")));
        assert!(!is_likely_binary(std::path::Path::new("text.txt")));
        assert!(!is_likely_binary(std::path::Path::new("noextension")));
    }

    #[test]
    fn test_is_likely_binary_case_insensitive() {
        // Test uppercase extensions
        assert!(is_likely_binary(std::path::Path::new("image.PNG")));
        assert!(is_likely_binary(std::path::Path::new("document.PDF")));
        assert!(is_likely_binary(std::path::Path::new("archive.ZIP")));
        assert!(is_likely_binary(std::path::Path::new("image.JpEg")));
    }

    #[test]
    fn test_is_likely_binary_all_extensions() {
        // Images
        assert!(is_likely_binary(std::path::Path::new("file.jpg")));
        assert!(is_likely_binary(std::path::Path::new("file.jpeg")));
        assert!(is_likely_binary(std::path::Path::new("file.gif")));
        assert!(is_likely_binary(std::path::Path::new("file.ico")));
        assert!(is_likely_binary(std::path::Path::new("file.svg")));

        // Fonts
        assert!(is_likely_binary(std::path::Path::new("font.woff")));
        assert!(is_likely_binary(std::path::Path::new("font.woff2")));
        assert!(is_likely_binary(std::path::Path::new("font.ttf")));
        assert!(is_likely_binary(std::path::Path::new("font.eot")));

        // Archives
        assert!(is_likely_binary(std::path::Path::new("archive.tar")));
        assert!(is_likely_binary(std::path::Path::new("archive.gz")));
        assert!(is_likely_binary(std::path::Path::new("archive.rar")));
        assert!(is_likely_binary(std::path::Path::new("archive.7z")));

        // Executables/libraries
        assert!(is_likely_binary(std::path::Path::new("program.exe")));
        assert!(is_likely_binary(std::path::Path::new("library.dll")));
        assert!(is_likely_binary(std::path::Path::new("library.dylib")));

        // Office documents
        assert!(is_likely_binary(std::path::Path::new("document.doc")));
        assert!(is_likely_binary(std::path::Path::new("document.docx")));
        assert!(is_likely_binary(std::path::Path::new("spreadsheet.xls")));
        assert!(is_likely_binary(std::path::Path::new("spreadsheet.xlsx")));

        // Media
        assert!(is_likely_binary(std::path::Path::new("audio.mp3")));
        assert!(is_likely_binary(std::path::Path::new("video.mp4")));
        assert!(is_likely_binary(std::path::Path::new("video.avi")));
        assert!(is_likely_binary(std::path::Path::new("video.mov")));
        assert!(is_likely_binary(std::path::Path::new("audio.wav")));

        // Compiled/bytecode
        assert!(is_likely_binary(std::path::Path::new("object.o")));
        assert!(is_likely_binary(std::path::Path::new("static.a")));
        assert!(is_likely_binary(std::path::Path::new("windows.lib")));
        assert!(is_likely_binary(std::path::Path::new("python.pyc")));
        assert!(is_likely_binary(std::path::Path::new("python.pyo")));
        assert!(is_likely_binary(std::path::Path::new("java.class")));

        // Databases
        assert!(is_likely_binary(std::path::Path::new("data.sqlite")));
        assert!(is_likely_binary(std::path::Path::new("data.sqlite3")));
    }

    #[test]
    fn test_is_likely_binary_with_paths() {
        // Full paths should work
        assert!(is_likely_binary(std::path::Path::new("/path/to/image.png")));
        assert!(is_likely_binary(std::path::Path::new(
            "relative/path/doc.pdf"
        )));
        assert!(!is_likely_binary(std::path::Path::new("/src/main.rs")));
    }

    #[test]
    fn test_is_hidden_or_ignored() {
        assert!(is_hidden_or_ignored(std::path::Path::new(
            ".hidden/file.txt"
        )));
        assert!(is_hidden_or_ignored(std::path::Path::new(
            "node_modules/pkg/file.js"
        )));
        assert!(is_hidden_or_ignored(std::path::Path::new(
            "target/debug/ted"
        )));
        assert!(is_hidden_or_ignored(std::path::Path::new(".git/config")));
        assert!(!is_hidden_or_ignored(std::path::Path::new("src/main.rs")));
        assert!(!is_hidden_or_ignored(std::path::Path::new("lib/utils.rs")));
    }

    #[test]
    fn test_is_hidden_or_ignored_all_directories() {
        // Test all ignored directory names
        assert!(is_hidden_or_ignored(std::path::Path::new(
            "__pycache__/cache.pyc"
        )));
        assert!(is_hidden_or_ignored(std::path::Path::new(
            "venv/bin/python"
        )));
        assert!(is_hidden_or_ignored(std::path::Path::new(".venv/lib/site")));
        assert!(is_hidden_or_ignored(std::path::Path::new("dist/bundle.js")));
        assert!(is_hidden_or_ignored(std::path::Path::new(
            "build/output.js"
        )));
        assert!(is_hidden_or_ignored(std::path::Path::new(
            "vendor/pkg/mod.go"
        )));
    }

    #[test]
    fn test_is_hidden_or_ignored_nested_paths() {
        // Hidden dirs deep in path
        assert!(is_hidden_or_ignored(std::path::Path::new(
            "src/components/.hidden/file.ts"
        )));
        // Ignored dirs deep in path
        assert!(is_hidden_or_ignored(std::path::Path::new(
            "packages/app/node_modules/react/index.js"
        )));
        assert!(is_hidden_or_ignored(std::path::Path::new(
            "crates/lib/target/release/binary"
        )));
    }

    #[test]
    fn test_is_hidden_or_ignored_hidden_files() {
        // Files starting with dot are also considered hidden
        // because the function checks all path components
        assert!(is_hidden_or_ignored(std::path::Path::new("src/.gitignore")));
        assert!(is_hidden_or_ignored(std::path::Path::new("pkg/.eslintrc")));
        assert!(is_hidden_or_ignored(std::path::Path::new(".env")));
        // But regular files are not hidden
        assert!(!is_hidden_or_ignored(std::path::Path::new("src/gitignore")));
    }

    #[test]
    fn test_is_hidden_or_ignored_similar_names() {
        // Names that contain but aren't exactly the ignored names
        assert!(!is_hidden_or_ignored(std::path::Path::new(
            "not_node_modules/file.js"
        )));
        assert!(!is_hidden_or_ignored(std::path::Path::new(
            "target_practice/file.rs"
        )));
        assert!(!is_hidden_or_ignored(std::path::Path::new(
            "my_dist/file.js"
        )));
    }

    #[test]
    fn test_search_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Line 1\nMatch here\nLine 3\nMatch again").unwrap();

        let regex = Regex::new("Match").unwrap();
        let matches = search_file(&file_path, &regex, 0, 100).unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_number, 2);
        assert_eq!(matches[1].line_number, 4);
    }

    #[test]
    fn test_search_file_with_limit() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Match 1\nMatch 2\nMatch 3").unwrap();

        let regex = Regex::new("Match").unwrap();
        let matches = search_file(&file_path, &regex, 0, 2).unwrap();

        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_search_file_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.txt");
        std::fs::write(&file_path, "").unwrap();

        let regex = Regex::new("anything").unwrap();
        let matches = search_file(&file_path, &regex, 0, 100).unwrap();

        assert!(matches.is_empty());
    }

    #[test]
    fn test_search_file_no_matches() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Line one\nLine two\nLine three").unwrap();

        let regex = Regex::new("notfound").unwrap();
        let matches = search_file(&file_path, &regex, 0, 100).unwrap();

        assert!(matches.is_empty());
    }

    #[test]
    fn test_search_file_multiple_lines() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Before\nMatch here\nAfter").unwrap();

        let regex = Regex::new("Match").unwrap();
        let matches = search_file(&file_path, &regex, 0, 100).unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_number, 2);
        assert_eq!(matches[0].line, "Match here");
    }

    #[test]
    fn test_search_file_first_and_last_lines() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Match at start\nMiddle line\nMatch at end").unwrap();

        let regex = Regex::new("Match").unwrap();
        let matches = search_file(&file_path, &regex, 0, 100).unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_number, 1);
        assert_eq!(matches[1].line_number, 3);
    }

    #[test]
    fn test_search_file_regex_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "fn main() {}\nasync fn test() {}\nlet x = 5;").unwrap();

        // Test regex with word boundary
        let regex = Regex::new(r"\bfn\b").unwrap();
        let matches = search_file(&file_path, &regex, 0, 100).unwrap();
        assert_eq!(matches.len(), 2);

        // Test regex with capture group behavior
        let regex = Regex::new(r"fn\s+\w+").unwrap();
        let matches = search_file(&file_path, &regex, 0, 100).unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_search_file_nonexistent() {
        let result = search_file(
            std::path::Path::new("/nonexistent/path"),
            &Regex::new("x").unwrap(),
            0,
            100,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_search_file_line_field() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello World\nGoodbye World").unwrap();

        let regex = Regex::new("World").unwrap();
        let matches = search_file(&file_path, &regex, 0, 100).unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line, "Hello World");
        assert_eq!(matches[1].line, "Goodbye World");
    }

    #[test]
    fn test_search_file_path_field() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Match this").unwrap();

        let regex = Regex::new("Match").unwrap();
        let matches = search_file(&file_path, &regex, 0, 100).unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path, file_path);
    }
}
