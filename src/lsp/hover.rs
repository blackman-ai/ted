// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::path::Path;

use anyhow::Result;
use tower_lsp::lsp_types::*;

use super::server::DocumentState;

/// Provide hover information for a position
pub async fn provide_hover(
    doc: &DocumentState,
    position: Position,
    _workspace: Option<&Path>,
) -> Result<Option<Hover>> {
    let lines: Vec<&str> = doc.content.lines().collect();
    let line_idx = position.line as usize;

    if line_idx >= lines.len() {
        return Ok(None);
    }

    let line = lines[line_idx];
    let col = position.character as usize;

    // Get the word at the cursor
    let (word, word_start, word_end) = get_word_with_bounds(line, col);
    if word.is_empty() {
        return Ok(None);
    }

    // Try to find documentation for this word
    if let Some(docs) = find_documentation(doc, &word, line_idx) {
        return Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: docs,
            }),
            range: Some(Range {
                start: Position {
                    line: position.line,
                    character: word_start as u32,
                },
                end: Position {
                    line: position.line,
                    character: word_end as u32,
                },
            }),
        }));
    }

    Ok(None)
}

/// Get word at position with start and end bounds
fn get_word_with_bounds(line: &str, col: usize) -> (String, usize, usize) {
    let chars: Vec<char> = line.chars().collect();
    if col >= chars.len() {
        return (String::new(), col, col);
    }

    let mut start = col;
    while start > 0 && is_identifier_char(chars[start - 1]) {
        start -= 1;
    }

    let mut end = col;
    while end < chars.len() && is_identifier_char(chars[end]) {
        end += 1;
    }

    (chars[start..end].iter().collect(), start, end)
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Find documentation for a symbol
fn find_documentation(doc: &DocumentState, word: &str, _current_line: usize) -> Option<String> {
    let lines: Vec<&str> = doc.content.lines().collect();

    // Language-specific patterns
    match doc.language_id.as_str() {
        "javascript" | "typescript" | "javascriptreact" | "typescriptreact" => {
            find_js_documentation(&lines, word)
        }
        "rust" => find_rust_documentation(&lines, word),
        "python" => find_python_documentation(&lines, word),
        _ => None,
    }
}

/// Find JSDoc documentation for a JavaScript/TypeScript symbol
fn find_js_documentation(lines: &[&str], word: &str) -> Option<String> {
    for (i, line) in lines.iter().enumerate() {
        // Look for function/class/const definitions
        if line.contains(&format!("function {}(", word))
            || line.contains(&format!("const {} =", word))
            || line.contains(&format!("let {} =", word))
            || line.contains(&format!("class {} ", word))
            || line.contains(&format!("interface {} ", word))
        {
            // Look for JSDoc comment above
            if i > 0 {
                let mut doc_lines = Vec::new();
                let mut j = i - 1;

                // Check if previous line ends JSDoc
                while j > 0 {
                    let prev = lines[j].trim();
                    if prev.ends_with("*/") {
                        // Found end of JSDoc, collect lines
                        doc_lines.push(prev.trim_end_matches("*/").trim());
                        j = j.saturating_sub(1);

                        while j > 0 {
                            let l = lines[j].trim();
                            if l.starts_with("/**") {
                                doc_lines.push(l.trim_start_matches("/**").trim());
                                break;
                            } else if l.starts_with("*") {
                                doc_lines.push(l.trim_start_matches("*").trim());
                            }
                            j = j.saturating_sub(1);
                        }
                        break;
                    } else if !prev.is_empty() && !prev.starts_with("//") && !prev.starts_with("*")
                    {
                        break;
                    }
                    j = j.saturating_sub(1);
                }

                if !doc_lines.is_empty() {
                    doc_lines.reverse();
                    return Some(format!("**{}**\n\n{}", word, doc_lines.join("\n")));
                }
            }

            // Return basic signature info
            let signature = extract_signature(line, word);
            return Some(format!(
                "**{}**\n\n```{}\n{}\n```",
                word, "typescript", signature
            ));
        }
    }

    None
}

/// Find Rust documentation
fn find_rust_documentation(lines: &[&str], word: &str) -> Option<String> {
    for (i, line) in lines.iter().enumerate() {
        // Look for function/struct/enum definitions
        if line.contains(&format!("fn {}(", word))
            || line.contains(&format!("struct {} ", word))
            || line.contains(&format!("enum {} ", word))
            || line.contains(&format!("trait {} ", word))
        {
            // Look for doc comments above
            if i > 0 {
                let mut doc_lines = Vec::new();
                let mut j = i - 1;

                while j > 0 {
                    let prev = lines[j].trim();
                    if prev.starts_with("///") {
                        doc_lines.push(prev.trim_start_matches("///").trim());
                    } else if !prev.starts_with("#[") && !prev.is_empty() {
                        break;
                    }
                    if j == 0 {
                        break;
                    }
                    j -= 1;
                }

                if !doc_lines.is_empty() {
                    doc_lines.reverse();
                    return Some(format!("**{}**\n\n{}", word, doc_lines.join("\n")));
                }
            }

            // Return basic signature
            return Some(format!("**{}**\n\n```rust\n{}\n```", word, line.trim()));
        }
    }

    None
}

/// Find Python documentation
fn find_python_documentation(lines: &[&str], word: &str) -> Option<String> {
    for (i, line) in lines.iter().enumerate() {
        // Look for function/class definitions
        if line.contains(&format!("def {}(", word)) || line.contains(&format!("class {}:", word)) {
            // Look for docstring below
            if i + 1 < lines.len() {
                let next = lines[i + 1].trim();
                if next.starts_with("\"\"\"") || next.starts_with("'''") {
                    let quote = if next.starts_with("\"\"\"") {
                        "\"\"\""
                    } else {
                        "'''"
                    };

                    let mut doc_lines = Vec::new();

                    // Single line docstring
                    if next.ends_with(quote) && next.len() > 6 {
                        let content = next
                            .trim_start_matches(quote)
                            .trim_end_matches(quote)
                            .trim();
                        return Some(format!("**{}**\n\n{}", word, content));
                    }

                    // Multi-line docstring
                    doc_lines.push(next.trim_start_matches(quote).trim());
                    let mut j = i + 2;
                    while j < lines.len() {
                        let l = lines[j].trim();
                        if l.contains(quote) {
                            doc_lines.push(l.trim_end_matches(quote).trim());
                            break;
                        }
                        doc_lines.push(l);
                        j += 1;
                    }

                    if !doc_lines.is_empty() {
                        return Some(format!("**{}**\n\n{}", word, doc_lines.join("\n")));
                    }
                }
            }

            // Return basic signature
            return Some(format!("**{}**\n\n```python\n{}\n```", word, line.trim()));
        }
    }

    None
}

/// Extract function/method signature from a line
fn extract_signature(line: &str, _word: &str) -> String {
    // Just return the line trimmed for now
    line.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests for get_word_with_bounds
    #[test]
    fn test_get_word_with_bounds_simple() {
        let (word, start, end) = get_word_with_bounds("hello world", 0);
        assert_eq!(word, "hello");
        assert_eq!(start, 0);
        assert_eq!(end, 5);
    }

    #[test]
    fn test_get_word_with_bounds_middle_of_word() {
        let (word, start, end) = get_word_with_bounds("hello world", 2);
        assert_eq!(word, "hello");
        assert_eq!(start, 0);
        assert_eq!(end, 5);
    }

    #[test]
    fn test_get_word_with_bounds_second_word() {
        let (word, start, end) = get_word_with_bounds("hello world", 6);
        assert_eq!(word, "world");
        assert_eq!(start, 6);
        assert_eq!(end, 11);
    }

    #[test]
    fn test_get_word_with_bounds_at_space() {
        let (word, start, end) = get_word_with_bounds("hello world", 5);
        // At space, returns word before it (backwards scan from col)
        assert_eq!(word, "hello");
        assert_eq!(start, 0);
        assert_eq!(end, 5);
    }

    #[test]
    fn test_get_word_with_bounds_col_out_of_range() {
        let (word, start, end) = get_word_with_bounds("hello", 100);
        assert!(word.is_empty());
        assert_eq!(start, end);
    }

    #[test]
    fn test_get_word_with_bounds_underscore() {
        let (word, start, end) = get_word_with_bounds("my_function()", 5);
        assert_eq!(word, "my_function");
        assert_eq!(start, 0);
        assert_eq!(end, 11);
    }

    #[test]
    fn test_get_word_with_bounds_dollar() {
        let (word, start, end) = get_word_with_bounds("$element.foo", 0);
        assert_eq!(word, "$element");
        assert_eq!(start, 0);
        assert_eq!(end, 8);
    }

    // Tests for is_identifier_char
    #[test]
    fn test_is_identifier_char_alphanumeric() {
        assert!(is_identifier_char('a'));
        assert!(is_identifier_char('Z'));
        assert!(is_identifier_char('5'));
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
    fn test_is_identifier_char_special() {
        assert!(!is_identifier_char(' '));
        assert!(!is_identifier_char('.'));
        assert!(!is_identifier_char('('));
        assert!(!is_identifier_char('-'));
    }

    // Tests for find_rust_documentation
    #[test]
    fn test_find_rust_documentation_function() {
        let lines = vec![
            "/// This is a doc comment",
            "/// More docs",
            "fn my_func() {}",
        ];
        let result = find_rust_documentation(&lines, "my_func");
        assert!(result.is_some());
        let doc = result.unwrap();
        assert!(doc.contains("my_func"));
        assert!(doc.contains("doc comment") || doc.contains("More docs"));
    }

    #[test]
    fn test_find_rust_documentation_struct() {
        let lines = vec!["/// Struct documentation", "struct MyStruct {}"];
        let result = find_rust_documentation(&lines, "MyStruct");
        assert!(result.is_some());
        assert!(result.unwrap().contains("MyStruct"));
    }

    #[test]
    fn test_find_rust_documentation_no_docs() {
        let lines = vec!["fn no_docs() {}"];
        let result = find_rust_documentation(&lines, "no_docs");
        assert!(result.is_some());
        // Should still return basic signature
        assert!(result.unwrap().contains("no_docs"));
    }

    #[test]
    fn test_find_rust_documentation_not_found() {
        let lines = vec!["fn other_func() {}"];
        let result = find_rust_documentation(&lines, "nonexistent");
        assert!(result.is_none());
    }

    // Tests for find_python_documentation
    #[test]
    fn test_find_python_documentation_function() {
        let lines = vec![
            "def my_func():",
            "    \"\"\"This is a docstring.\"\"\"",
            "    pass",
        ];
        let result = find_python_documentation(&lines, "my_func");
        assert!(result.is_some());
        let doc = result.unwrap();
        assert!(doc.contains("my_func"));
    }

    #[test]
    fn test_find_python_documentation_class() {
        let lines = vec!["class MyClass:", "    '''Class docstring'''", "    pass"];
        let result = find_python_documentation(&lines, "MyClass");
        assert!(result.is_some());
        assert!(result.unwrap().contains("MyClass"));
    }

    #[test]
    fn test_find_python_documentation_multiline() {
        let lines = vec![
            "def my_func():",
            "    \"\"\"",
            "    This is a multiline",
            "    docstring.",
            "    \"\"\"",
            "    pass",
        ];
        let result = find_python_documentation(&lines, "my_func");
        assert!(result.is_some());
    }

    #[test]
    fn test_find_python_documentation_not_found() {
        let lines = vec!["def other_func():", "    pass"];
        let result = find_python_documentation(&lines, "nonexistent");
        assert!(result.is_none());
    }

    // Tests for find_js_documentation
    #[test]
    fn test_find_js_documentation_function() {
        let lines = vec!["/**", " * This is JSDoc", " */", "function myFunc() {}"];
        let result = find_js_documentation(&lines, "myFunc");
        assert!(result.is_some());
        let doc = result.unwrap();
        assert!(doc.contains("myFunc"));
    }

    #[test]
    fn test_find_js_documentation_const() {
        let lines = vec!["const myConst = 42;"];
        let result = find_js_documentation(&lines, "myConst");
        assert!(result.is_some());
        assert!(result.unwrap().contains("myConst"));
    }

    #[test]
    fn test_find_js_documentation_class() {
        let lines = vec!["class MyClass {}"];
        let result = find_js_documentation(&lines, "MyClass");
        assert!(result.is_some());
        assert!(result.unwrap().contains("MyClass"));
    }

    #[test]
    fn test_find_js_documentation_not_found() {
        let lines = vec!["const other = 1;"];
        let result = find_js_documentation(&lines, "nonexistent");
        assert!(result.is_none());
    }

    // Tests for extract_signature
    #[test]
    fn test_extract_signature_trims() {
        assert_eq!(extract_signature("  fn foo()  ", "foo"), "fn foo()");
    }

    #[test]
    fn test_extract_signature_preserves_content() {
        let sig = extract_signature("function myFunc(a, b) {}", "myFunc");
        assert!(sig.contains("myFunc"));
        assert!(sig.contains("a, b"));
    }
}
