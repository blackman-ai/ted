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
