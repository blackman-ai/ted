// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::path::Path;

use anyhow::Result;
use tower_lsp::lsp_types::*;

use super::server::DocumentState;

/// Provide completion items for a position in a document
pub async fn provide_completions(
    doc: &DocumentState,
    position: Position,
    workspace: Option<&Path>,
) -> Result<Vec<CompletionItem>> {
    let mut items = Vec::new();

    // Get the line content up to the cursor
    let lines: Vec<&str> = doc.content.lines().collect();
    let line_idx = position.line as usize;

    if line_idx >= lines.len() {
        return Ok(items);
    }

    let line = lines[line_idx];
    let col = position.character as usize;
    let prefix = if col <= line.len() {
        &line[..col]
    } else {
        line
    };

    // Determine what kind of completion to provide based on context
    match doc.language_id.as_str() {
        "javascript" | "typescript" | "javascriptreact" | "typescriptreact" => {
            items.extend(js_completions(prefix, &doc.content, workspace).await?);
        }
        "rust" => {
            items.extend(rust_completions(prefix, &doc.content, workspace).await?);
        }
        "python" => {
            items.extend(python_completions(prefix, &doc.content, workspace).await?);
        }
        _ => {
            // Generic completions based on words in the document
            items.extend(generic_completions(prefix, &doc.content)?);
        }
    }

    Ok(items)
}

/// JavaScript/TypeScript completions
async fn js_completions(
    prefix: &str,
    content: &str,
    _workspace: Option<&Path>,
) -> Result<Vec<CompletionItem>> {
    let mut items = Vec::new();

    // Import path completions
    if prefix.contains("import") || prefix.contains("from") || prefix.contains("require") {
        // Check if we're in a string
        if prefix.ends_with("'") || prefix.ends_with("\"") || prefix.ends_with("/") {
            // TODO: Provide file path completions from workspace
        }
    }

    // Extract function/variable names from the document for local completions
    let word_prefix = get_word_at_end(prefix);
    if !word_prefix.is_empty() {
        // Find all identifiers in the document
        let identifiers = extract_js_identifiers(content);
        for ident in identifiers {
            if ident.starts_with(word_prefix) && ident != word_prefix {
                items.push(CompletionItem {
                    label: ident.clone(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some("Local".to_string()),
                    ..Default::default()
                });
            }
        }
    }

    // Common JavaScript snippets
    if prefix.trim().is_empty() || word_prefix.is_empty() {
        items.extend(js_snippets());
    }

    Ok(items)
}

/// Extract JavaScript identifiers from content
fn extract_js_identifiers(content: &str) -> Vec<String> {
    let mut identifiers = Vec::new();
    let mut current = String::new();

    for ch in content.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '$' {
            current.push(ch);
        } else {
            if !current.is_empty()
                && !current.chars().next().unwrap().is_numeric()
                && !identifiers.contains(&current)
            {
                identifiers.push(current.clone());
            }
            current.clear();
        }
    }

    if !current.is_empty()
        && !current.chars().next().unwrap().is_numeric()
        && !identifiers.contains(&current)
    {
        identifiers.push(current);
    }

    identifiers
}

/// Common JavaScript snippets
fn js_snippets() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "function".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("function ${1:name}(${2:params}) {\n\t$0\n}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Function declaration".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "async function".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("async function ${1:name}(${2:params}) {\n\t$0\n}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Async function".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "arrow".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("(${1:params}) => ${0}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Arrow function".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "for".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some(
                "for (let ${1:i} = 0; ${1:i} < ${2:array}.length; ${1:i}++) {\n\t$0\n}".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("For loop".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "forof".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("for (const ${1:item} of ${2:array}) {\n\t$0\n}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("For...of loop".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "if".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("if (${1:condition}) {\n\t$0\n}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("If statement".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "try".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some(
                "try {\n\t$0\n} catch (${1:error}) {\n\tconsole.error(${1:error});\n}".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Try-catch block".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "console.log".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("console.log($0);".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Log to console".to_string()),
            ..Default::default()
        },
    ]
}

/// Rust completions
async fn rust_completions(
    prefix: &str,
    content: &str,
    _workspace: Option<&Path>,
) -> Result<Vec<CompletionItem>> {
    let mut items = Vec::new();

    let word_prefix = get_word_at_end(prefix);

    // Extract identifiers from the document
    if !word_prefix.is_empty() {
        let identifiers = extract_rust_identifiers(content);
        for ident in identifiers {
            if ident.starts_with(word_prefix) && ident != word_prefix {
                items.push(CompletionItem {
                    label: ident.clone(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some("Local".to_string()),
                    ..Default::default()
                });
            }
        }
    }

    // Rust snippets
    if prefix.trim().is_empty() || word_prefix.is_empty() {
        items.extend(rust_snippets());
    }

    Ok(items)
}

/// Extract Rust identifiers
fn extract_rust_identifiers(content: &str) -> Vec<String> {
    let mut identifiers = Vec::new();
    let mut current = String::new();

    for ch in content.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch);
        } else {
            if !current.is_empty()
                && !current.chars().next().unwrap().is_numeric()
                && !identifiers.contains(&current)
            {
                identifiers.push(current.clone());
            }
            current.clear();
        }
    }

    identifiers
}

/// Rust snippets
fn rust_snippets() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "fn".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some(
                "fn ${1:name}(${2:params}) ${3:-> ${4:ReturnType} }{\n\t$0\n}".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Function".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "impl".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("impl ${1:Type} {\n\t$0\n}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Impl block".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "struct".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("struct ${1:Name} {\n\t${2:field}: ${3:Type},\n}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Struct".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "enum".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("enum ${1:Name} {\n\t${2:Variant},\n}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Enum".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "match".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("match ${1:value} {\n\t${2:pattern} => $0,\n}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Match expression".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "if let".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("if let ${1:pattern} = ${2:value} {\n\t$0\n}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("If let".to_string()),
            ..Default::default()
        },
    ]
}

/// Python completions
async fn python_completions(
    prefix: &str,
    content: &str,
    _workspace: Option<&Path>,
) -> Result<Vec<CompletionItem>> {
    let mut items = Vec::new();

    let word_prefix = get_word_at_end(prefix);

    // Extract identifiers
    if !word_prefix.is_empty() {
        let identifiers = extract_python_identifiers(content);
        for ident in identifiers {
            if ident.starts_with(word_prefix) && ident != word_prefix {
                items.push(CompletionItem {
                    label: ident.clone(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some("Local".to_string()),
                    ..Default::default()
                });
            }
        }
    }

    // Python snippets
    if prefix.trim().is_empty() || word_prefix.is_empty() {
        items.extend(python_snippets());
    }

    Ok(items)
}

/// Extract Python identifiers
fn extract_python_identifiers(content: &str) -> Vec<String> {
    let mut identifiers = Vec::new();
    let mut current = String::new();

    for ch in content.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch);
        } else {
            if !current.is_empty()
                && !current.chars().next().unwrap().is_numeric()
                && !identifiers.contains(&current)
            {
                identifiers.push(current.clone());
            }
            current.clear();
        }
    }

    identifiers
}

/// Python snippets
fn python_snippets() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "def".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("def ${1:name}(${2:params}):\n\t$0".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Function".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "class".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("class ${1:Name}:\n\tdef __init__(self):\n\t\t$0".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Class".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "for".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("for ${1:item} in ${2:iterable}:\n\t$0".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("For loop".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "if".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("if ${1:condition}:\n\t$0".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("If statement".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "try".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("try:\n\t$0\nexcept ${1:Exception} as e:\n\tprint(e)".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("Try-except".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "with".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("with ${1:context} as ${2:var}:\n\t$0".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("With statement".to_string()),
            ..Default::default()
        },
    ]
}

/// Generic completions (word-based)
fn generic_completions(prefix: &str, content: &str) -> Result<Vec<CompletionItem>> {
    let mut items = Vec::new();
    let word_prefix = get_word_at_end(prefix);

    if word_prefix.is_empty() || word_prefix.len() < 2 {
        return Ok(items);
    }

    // Collect all words in the document
    let words: Vec<String> = content
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 2)
        .map(|s| s.to_string())
        .collect();

    // Deduplicate and filter by prefix
    let mut seen = std::collections::HashSet::new();
    for word in words {
        if word.starts_with(word_prefix) && word != word_prefix && !seen.contains(&word) {
            seen.insert(word.clone());
            items.push(CompletionItem {
                label: word,
                kind: Some(CompletionItemKind::TEXT),
                ..Default::default()
            });
        }
    }

    Ok(items)
}

/// Get the word at the end of a string
fn get_word_at_end(s: &str) -> &str {
    let mut start = s.len();
    for (i, ch) in s.char_indices().rev() {
        if ch.is_alphanumeric() || ch == '_' || ch == '$' {
            start = i;
        } else {
            break;
        }
    }
    &s[start..]
}
