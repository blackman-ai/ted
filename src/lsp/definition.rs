// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::path::Path;

use anyhow::Result;
use tower_lsp::lsp_types::*;

use super::server::DocumentState;

/// Provide go-to-definition for a position
pub async fn provide_definition(
    doc: &DocumentState,
    position: Position,
    workspace: Option<&Path>,
) -> Result<Option<Location>> {
    // Get the word at the cursor position
    let lines: Vec<&str> = doc.content.lines().collect();
    let line_idx = position.line as usize;

    if line_idx >= lines.len() {
        return Ok(None);
    }

    let line = lines[line_idx];
    let col = position.character as usize;

    // Extract the word at the cursor
    let word = get_word_at_position(line, col);
    if word.is_empty() {
        return Ok(None);
    }

    tracing::debug!("Looking for definition of: {}", word);

    // Search for the definition in the document
    if let Some(location) = find_definition_in_document(doc, &word) {
        return Ok(Some(location));
    }

    // Search for the definition in the workspace
    if let Some(workspace_path) = workspace {
        if let Some(location) = find_definition_in_workspace(workspace_path, &word, doc).await? {
            return Ok(Some(location));
        }
    }

    Ok(None)
}

/// Get the word at a given column position
fn get_word_at_position(line: &str, col: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if col >= chars.len() {
        return String::new();
    }

    // Find start of word
    let mut start = col;
    while start > 0 {
        let prev = start - 1;
        if !is_identifier_char(chars[prev]) {
            break;
        }
        start = prev;
    }

    // Find end of word
    let mut end = col;
    while end < chars.len() && is_identifier_char(chars[end]) {
        end += 1;
    }

    chars[start..end].iter().collect()
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Find definition within the same document
fn find_definition_in_document(doc: &DocumentState, word: &str) -> Option<Location> {
    let lines: Vec<&str> = doc.content.lines().collect();

    // Language-specific definition patterns
    let patterns: Vec<String> = match doc.language_id.as_str() {
        "javascript" | "typescript" | "javascriptreact" | "typescriptreact" => {
            vec![
                format!("function {}(", word),
                format!("const {} =", word),
                format!("let {} =", word),
                format!("var {} =", word),
                format!("class {} ", word),
                format!("interface {} ", word),
                format!("type {} =", word),
                format!("export function {}(", word),
                format!("export const {} =", word),
                format!("export class {} ", word),
                format!("export interface {} ", word),
                format!("export type {} =", word),
                format!("{}: function", word), // Object method
                format!("{}(", word),          // Method definition (arrow)
            ]
        }
        "rust" => {
            vec![
                format!("fn {}(", word),
                format!("struct {} ", word),
                format!("struct {}<", word),
                format!("enum {} ", word),
                format!("enum {}<", word),
                format!("trait {} ", word),
                format!("type {} =", word),
                format!("const {}: ", word),
                format!("static {}: ", word),
                format!("let {} =", word),
                format!("let mut {} =", word),
                format!("mod {} ", word),
                format!("pub fn {}(", word),
                format!("pub struct {} ", word),
                format!("pub enum {} ", word),
                format!("pub trait {} ", word),
            ]
        }
        "python" => {
            vec![
                format!("def {}(", word),
                format!("class {}:", word),
                format!("class {}(", word),
                format!("{} =", word),
            ]
        }
        _ => {
            // Generic patterns
            vec![
                format!("function {}(", word),
                format!("{} =", word),
                format!("class {} ", word),
                format!("def {}(", word),
                format!("fn {}(", word),
            ]
        }
    };

    for (line_idx, line) in lines.iter().enumerate() {
        for pattern in &patterns {
            if line.contains(pattern.as_str()) {
                // Find the column where the word starts
                if let Some(col) = line.find(word) {
                    return Some(Location {
                        uri: doc.uri.clone(),
                        range: Range {
                            start: Position {
                                line: line_idx as u32,
                                character: col as u32,
                            },
                            end: Position {
                                line: line_idx as u32,
                                character: (col + word.len()) as u32,
                            },
                        },
                    });
                }
            }
        }
    }

    None
}

/// Find definition in the workspace
async fn find_definition_in_workspace(
    workspace: &Path,
    word: &str,
    current_doc: &DocumentState,
) -> Result<Option<Location>> {
    use walkdir::WalkDir;

    // File extensions to search based on current document type
    let extensions: Vec<&str> = match current_doc.language_id.as_str() {
        "javascript" | "javascriptreact" => vec!["js", "jsx", "mjs"],
        "typescript" | "typescriptreact" => vec!["ts", "tsx", "js", "jsx"],
        "rust" => vec!["rs"],
        "python" => vec!["py"],
        _ => vec!["js", "ts", "jsx", "tsx", "rs", "py"],
    };

    // Walk through workspace files
    for entry in WalkDir::new(workspace)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Skip directories and non-matching extensions
        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !extensions.contains(&ext) {
            continue;
        }

        // Skip common directories
        let path_str = path.to_string_lossy();
        if path_str.contains("node_modules")
            || path_str.contains(".git")
            || path_str.contains("target")
            || path_str.contains("__pycache__")
            || path_str.contains(".venv")
        {
            continue;
        }

        // Skip the current file
        if let Ok(file_uri) = Url::from_file_path(path) {
            if file_uri == current_doc.uri {
                continue;
            }
        }

        // Read and search the file
        if let Ok(content) = std::fs::read_to_string(path) {
            let temp_doc = DocumentState {
                uri: Url::from_file_path(path).unwrap_or_else(|_| current_doc.uri.clone()),
                content,
                version: 0,
                language_id: current_doc.language_id.clone(),
            };

            if let Some(location) = find_definition_in_document(&temp_doc, word) {
                return Ok(Some(location));
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_doc(content: &str, language_id: &str) -> DocumentState {
        DocumentState {
            uri: Url::parse("file:///test/file.rs").unwrap(),
            content: content.to_string(),
            version: 1,
            language_id: language_id.to_string(),
        }
    }

    // Tests for get_word_at_position
    #[test]
    fn test_get_word_at_position_simple() {
        let word = get_word_at_position("hello world", 0);
        assert_eq!(word, "hello");
    }

    #[test]
    fn test_get_word_at_position_middle() {
        let word = get_word_at_position("hello world", 2);
        assert_eq!(word, "hello");
    }

    #[test]
    fn test_get_word_at_position_second_word() {
        let word = get_word_at_position("hello world", 7);
        assert_eq!(word, "world");
    }

    #[test]
    fn test_get_word_at_position_col_out_of_range() {
        let word = get_word_at_position("hello", 100);
        assert!(word.is_empty());
    }

    #[test]
    fn test_get_word_at_position_underscore() {
        let word = get_word_at_position("my_function()", 3);
        assert_eq!(word, "my_function");
    }

    #[test]
    fn test_get_word_at_position_dollar_sign() {
        let word = get_word_at_position("$variable = 1", 3);
        assert_eq!(word, "$variable");
    }

    #[test]
    fn test_get_word_at_position_at_boundary() {
        let word = get_word_at_position("foo.bar", 4);
        assert_eq!(word, "bar");
    }

    // Tests for is_identifier_char
    #[test]
    fn test_is_identifier_char_letters() {
        assert!(is_identifier_char('a'));
        assert!(is_identifier_char('Z'));
    }

    #[test]
    fn test_is_identifier_char_numbers() {
        assert!(is_identifier_char('0'));
        assert!(is_identifier_char('9'));
    }

    #[test]
    fn test_is_identifier_char_underscore() {
        assert!(is_identifier_char('_'));
    }

    #[test]
    fn test_is_identifier_char_dollar() {
        assert!(is_identifier_char('$'));
    }

    #[test]
    fn test_is_identifier_char_special_chars() {
        assert!(!is_identifier_char(' '));
        assert!(!is_identifier_char('.'));
        assert!(!is_identifier_char('('));
        assert!(!is_identifier_char(')'));
        assert!(!is_identifier_char('{'));
        assert!(!is_identifier_char(':'));
    }

    // Tests for find_definition_in_document - Rust
    #[test]
    fn test_find_definition_rust_function() {
        let doc = make_doc("fn my_func() {}\nmy_func();", "rust");
        let location = find_definition_in_document(&doc, "my_func");
        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.range.start.line, 0);
    }

    #[test]
    fn test_find_definition_rust_struct() {
        let doc = make_doc("struct MyStruct {}\nlet s: MyStruct;", "rust");
        let location = find_definition_in_document(&doc, "MyStruct");
        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.range.start.line, 0);
    }

    #[test]
    fn test_find_definition_rust_enum() {
        let doc = make_doc(
            "enum MyEnum { A, B }\nmatch val { MyEnum::A => {} }",
            "rust",
        );
        let location = find_definition_in_document(&doc, "MyEnum");
        assert!(location.is_some());
    }

    #[test]
    fn test_find_definition_rust_pub_fn() {
        let doc = make_doc("pub fn public_func() {}", "rust");
        let location = find_definition_in_document(&doc, "public_func");
        assert!(location.is_some());
    }

    // Tests for find_definition_in_document - JavaScript
    #[test]
    fn test_find_definition_js_function() {
        let doc = make_doc("function myFunc() {}\nmyFunc();", "javascript");
        let location = find_definition_in_document(&doc, "myFunc");
        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.range.start.line, 0);
    }

    #[test]
    fn test_find_definition_js_const() {
        let doc = make_doc("const myConst = 42;\nconsole.log(myConst);", "javascript");
        let location = find_definition_in_document(&doc, "myConst");
        assert!(location.is_some());
    }

    #[test]
    fn test_find_definition_js_class() {
        let doc = make_doc("class MyClass {}\nconst c = new MyClass();", "typescript");
        let location = find_definition_in_document(&doc, "MyClass");
        assert!(location.is_some());
    }

    #[test]
    fn test_find_definition_js_export_function() {
        let doc = make_doc("export function exported() {}", "typescript");
        let location = find_definition_in_document(&doc, "exported");
        assert!(location.is_some());
    }

    // Tests for find_definition_in_document - Python
    #[test]
    fn test_find_definition_python_def() {
        let doc = make_doc("def my_func():\n    pass\nmy_func()", "python");
        let location = find_definition_in_document(&doc, "my_func");
        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.range.start.line, 0);
    }

    #[test]
    fn test_find_definition_python_class() {
        let doc = make_doc("class MyClass:\n    pass\nobj = MyClass()", "python");
        let location = find_definition_in_document(&doc, "MyClass");
        assert!(location.is_some());
    }

    // Tests for not found
    #[test]
    fn test_find_definition_not_found() {
        let doc = make_doc("fn other_func() {}", "rust");
        let location = find_definition_in_document(&doc, "nonexistent");
        assert!(location.is_none());
    }

    #[test]
    fn test_find_definition_empty_document() {
        let doc = make_doc("", "rust");
        let location = find_definition_in_document(&doc, "anything");
        assert!(location.is_none());
    }

    // Tests for location accuracy
    #[test]
    fn test_find_definition_correct_column() {
        let doc = make_doc("    fn indented() {}", "rust");
        let location = find_definition_in_document(&doc, "indented");
        assert!(location.is_some());
        let loc = location.unwrap();
        // Word should start at column 7 (after "    fn ")
        assert_eq!(loc.range.start.character, 7);
        assert_eq!(loc.range.end.character, 15); // "indented" is 8 chars
    }
}
