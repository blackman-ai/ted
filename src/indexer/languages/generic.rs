// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Generic fallback parser for dependency extraction.
//!
//! Uses regex patterns to detect common import patterns across languages.
//! This parser is used when no language-specific parser is available.

use regex::Regex;
use std::path::{Path, PathBuf};

use super::{ExportKind, ExportRef, ImportKind, ImportRef, LanguageParser};

/// Generic fallback parser using common patterns.
pub struct GenericParser {
    /// Common import patterns across languages
    import_patterns: Vec<(Regex, ImportPatternKind)>,
    /// Common export patterns
    export_patterns: Vec<(Regex, ExportKind)>,
}

/// Helper for classifying matched imports
#[derive(Debug, Clone, Copy)]
enum ImportPatternKind {
    /// ES6/TypeScript: import ... from "path"
    EsModule,
    /// CommonJS: require("path")
    CommonJs,
    /// Python: import module / from module import ...
    Python,
    /// Go: import "path"
    Go,
    /// Generic include/require
    Generic,
}

impl GenericParser {
    /// Create a new generic parser.
    pub fn new() -> Self {
        Self {
            import_patterns: vec![
                // ES6/TypeScript: import { x } from "path" or import x from "path"
                (
                    Regex::new(r#"(?m)^\s*import\s+(?:[\w\s{},*]+\s+from\s+)?["']([^"']+)["']"#)
                        .unwrap(),
                    ImportPatternKind::EsModule,
                ),
                // CommonJS: require("path") or require('path')
                (
                    Regex::new(r#"(?m)require\s*\(\s*["']([^"']+)["']\s*\)"#).unwrap(),
                    ImportPatternKind::CommonJs,
                ),
                // Python: import module or from module import ...
                (
                    Regex::new(r"(?m)^\s*(?:from\s+([a-zA-Z_][\w.]*)|import\s+([a-zA-Z_][\w.]*))")
                        .unwrap(),
                    ImportPatternKind::Python,
                ),
                // Go: import "path" or import ( "path" )
                (
                    Regex::new(r#"(?m)import\s+(?:\(\s*)?["']?([^"'\s)]+)["']?"#).unwrap(),
                    ImportPatternKind::Go,
                ),
                // C/C++: #include <path> or #include "path"
                (
                    Regex::new(r#"(?m)^\s*#\s*include\s*[<"]([^>"]+)[>"]"#).unwrap(),
                    ImportPatternKind::Generic,
                ),
            ],
            export_patterns: vec![
                // ES6: export function/class/const
                (
                    Regex::new(r"(?m)^\s*export\s+(?:default\s+)?(?:function|class|const|let|var)\s+([a-zA-Z_]\w*)").unwrap(),
                    ExportKind::Function,
                ),
                // ES6: export { name }
                (
                    Regex::new(r"(?m)^\s*export\s+\{([^}]+)\}").unwrap(),
                    ExportKind::Other,
                ),
                // Python: def function (at module level, no indentation)
                (
                    Regex::new(r"(?m)^def\s+([a-zA-Z_]\w*)\s*\(").unwrap(),
                    ExportKind::Function,
                ),
                // Python: class Name
                (
                    Regex::new(r"(?m)^class\s+([a-zA-Z_]\w*)").unwrap(),
                    ExportKind::Type,
                ),
                // Go: func Name (exported if capitalized)
                (
                    Regex::new(r"(?m)^func\s+([A-Z]\w*)\s*\(").unwrap(),
                    ExportKind::Function,
                ),
                // Go: type Name
                (
                    Regex::new(r"(?m)^type\s+([A-Z]\w*)").unwrap(),
                    ExportKind::Type,
                ),
            ],
        }
    }

    /// Determine import kind from matched path.
    fn classify_import(path: &str, kind: ImportPatternKind) -> ImportKind {
        match kind {
            ImportPatternKind::EsModule | ImportPatternKind::CommonJs => {
                if path.starts_with("./") || path.starts_with("../") {
                    ImportKind::Relative
                } else if path.starts_with('/') {
                    ImportKind::Absolute
                } else {
                    ImportKind::External
                }
            }
            ImportPatternKind::Python => {
                if path.starts_with('.') {
                    ImportKind::Relative
                } else {
                    ImportKind::External
                }
            }
            ImportPatternKind::Go => {
                if path.contains('/') && !path.contains('.') {
                    // Likely a local package
                    ImportKind::Relative
                } else {
                    ImportKind::External
                }
            }
            ImportPatternKind::Generic => {
                if path.starts_with("./") || path.starts_with("../") {
                    ImportKind::Relative
                } else {
                    ImportKind::External
                }
            }
        }
    }

    /// Get line number for a byte offset in content.
    fn line_number(content: &str, byte_offset: usize) -> u32 {
        content[..byte_offset].matches('\n').count() as u32 + 1
    }
}

impl Default for GenericParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for GenericParser {
    fn extensions(&self) -> &[&str] {
        // This is a fallback parser, so it handles everything else
        &[
            "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "c", "cpp", "h", "hpp",
        ]
    }

    fn parse_imports(&self, content: &str) -> Vec<ImportRef> {
        let mut imports = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for (regex, pattern_kind) in &self.import_patterns {
            for cap in regex.captures_iter(content) {
                // Try to get the captured path from various groups
                let path = cap
                    .get(1)
                    .or_else(|| cap.get(2))
                    .map(|m| m.as_str().to_string());

                if let Some(raw_path) = path {
                    // Deduplicate
                    if seen.contains(&raw_path) {
                        continue;
                    }
                    seen.insert(raw_path.clone());

                    let kind = Self::classify_import(&raw_path, *pattern_kind);
                    let line =
                        Self::line_number(content, cap.get(0).map(|m| m.start()).unwrap_or(0));

                    imports.push(ImportRef::new(raw_path, kind, line));
                }
            }
        }

        imports
    }

    fn parse_exports(&self, content: &str) -> Vec<ExportRef> {
        let mut exports = Vec::new();

        for (regex, export_kind) in &self.export_patterns {
            for cap in regex.captures_iter(content) {
                if let Some(name_match) = cap.get(1) {
                    let name = name_match.as_str();

                    // Handle brace exports: export { a, b, c }
                    if name.contains(',') {
                        for item in name.split(',') {
                            let item = item.trim();
                            if !item.is_empty() {
                                let line = Self::line_number(content, name_match.start());
                                exports.push(ExportRef::new(item, *export_kind, line));
                            }
                        }
                    } else {
                        let line = Self::line_number(content, name_match.start());
                        exports.push(ExportRef::new(name, *export_kind, line));
                    }
                }
            }
        }

        exports
    }

    fn resolve_import(
        &self,
        import: &ImportRef,
        from_file: &Path,
        project_root: &Path,
    ) -> Option<PathBuf> {
        let raw = &import.raw_path;

        // Skip external dependencies
        if import.kind == ImportKind::External {
            return None;
        }

        // Handle relative paths
        if raw.starts_with("./") || raw.starts_with("../") {
            let from_dir = from_file.parent()?;
            let resolved = from_dir.join(raw);

            // Try with common extensions
            for ext in &["", ".ts", ".tsx", ".js", ".jsx", ".py", ".go"] {
                let with_ext = if ext.is_empty() {
                    resolved.clone()
                } else {
                    resolved.with_extension(&ext[1..])
                };

                let abs_path = project_root.join(&with_ext);
                if abs_path.exists() {
                    return Some(with_ext);
                }

                // Also try index files
                let index_path = resolved.join(format!("index{}", ext));
                let abs_index = project_root.join(&index_path);
                if abs_index.exists() {
                    return Some(index_path);
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_es6_imports() {
        let parser = GenericParser::new();
        let content = r#"
import React from "react";
import { useState } from "react";
import * as utils from "./utils";
import "./styles.css";
"#;

        let imports = parser.parse_imports(content);

        assert!(imports.len() >= 3);
        assert!(imports.iter().any(|i| i.raw_path == "react"));
        assert!(imports.iter().any(|i| i.raw_path == "./utils"));
        assert!(imports.iter().any(|i| i.raw_path == "./styles.css"));
    }

    #[test]
    fn test_parse_commonjs_require() {
        let parser = GenericParser::new();
        let content = r#"
const fs = require("fs");
const utils = require("./utils");
const pkg = require('../package.json');
"#;

        let imports = parser.parse_imports(content);

        assert!(imports.len() >= 3);
        assert!(imports.iter().any(|i| i.raw_path == "fs"));
        assert!(imports.iter().any(|i| i.raw_path == "./utils"));
        assert!(imports.iter().any(|i| i.raw_path == "../package.json"));
    }

    #[test]
    fn test_parse_python_imports() {
        let parser = GenericParser::new();
        let content = r#"
import os
import sys
from pathlib import Path
from .utils import helper
"#;

        let imports = parser.parse_imports(content);

        assert!(imports.len() >= 3);
        assert!(imports.iter().any(|i| i.raw_path == "os"));
        assert!(imports.iter().any(|i| i.raw_path == "pathlib"));
    }

    #[test]
    fn test_parse_c_includes() {
        let parser = GenericParser::new();
        let content = r#"
#include <stdio.h>
#include "myheader.h"
#include "../utils/helpers.h"
"#;

        let imports = parser.parse_imports(content);

        assert!(imports.len() >= 3);
        assert!(imports.iter().any(|i| i.raw_path == "stdio.h"));
        assert!(imports.iter().any(|i| i.raw_path == "myheader.h"));
        assert!(imports.iter().any(|i| i.raw_path == "../utils/helpers.h"));
    }

    #[test]
    fn test_parse_es6_exports() {
        let parser = GenericParser::new();
        let content = r#"
export function myFunction() {}
export class MyClass {}
export const MY_CONST = 42;
export default function defaultFn() {}
"#;

        let exports = parser.parse_exports(content);

        assert!(exports.len() >= 3);
        assert!(exports.iter().any(|e| e.name == "myFunction"));
        assert!(exports.iter().any(|e| e.name == "MyClass"));
        assert!(exports.iter().any(|e| e.name == "MY_CONST"));
    }

    #[test]
    fn test_parse_python_exports() {
        let parser = GenericParser::new();
        let content = r#"
def public_function():
    pass

class MyClass:
    pass

def _private_function():
    pass
"#;

        let exports = parser.parse_exports(content);

        assert!(exports.iter().any(|e| e.name == "public_function"));
        assert!(exports.iter().any(|e| e.name == "MyClass"));
        assert!(exports.iter().any(|e| e.name == "_private_function"));
    }

    #[test]
    fn test_parse_go_exports() {
        let parser = GenericParser::new();
        let content = r#"
func PublicFunction() {}

func privateFunction() {}

type PublicStruct struct {}

type privateStruct struct {}
"#;

        let exports = parser.parse_exports(content);

        // Only capitalized names are exported in Go
        assert!(exports.iter().any(|e| e.name == "PublicFunction"));
        assert!(exports.iter().any(|e| e.name == "PublicStruct"));
        assert!(!exports.iter().any(|e| e.name == "privateFunction"));
    }

    #[test]
    fn test_classify_import_relative() {
        assert_eq!(
            GenericParser::classify_import("./utils", ImportPatternKind::EsModule),
            ImportKind::Relative
        );
        assert_eq!(
            GenericParser::classify_import("../parent", ImportPatternKind::CommonJs),
            ImportKind::Relative
        );
    }

    #[test]
    fn test_classify_import_external() {
        assert_eq!(
            GenericParser::classify_import("react", ImportPatternKind::EsModule),
            ImportKind::External
        );
        assert_eq!(
            GenericParser::classify_import("lodash", ImportPatternKind::CommonJs),
            ImportKind::External
        );
    }

    #[test]
    fn test_classify_import_additional_kinds() {
        assert_eq!(
            GenericParser::classify_import("/absolute/path", ImportPatternKind::EsModule),
            ImportKind::Absolute
        );
        assert_eq!(
            GenericParser::classify_import(".local.module", ImportPatternKind::Python),
            ImportKind::Relative
        );
        assert_eq!(
            GenericParser::classify_import("internal/pkg", ImportPatternKind::Go),
            ImportKind::Relative
        );
    }

    #[test]
    fn test_default_parser_constructor() {
        let parser = GenericParser::default();
        assert!(!parser.extensions().is_empty());
    }

    #[test]
    fn test_resolve_relative_import() {
        let parser = GenericParser::new();
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        // Create src/utils.ts
        std::fs::create_dir_all(project_root.join("src")).unwrap();
        std::fs::write(project_root.join("src/utils.ts"), "").unwrap();

        let import = ImportRef::new("./utils", ImportKind::Relative, 1);
        let from_file = Path::new("src/index.ts");

        let resolved = parser.resolve_import(&import, from_file, project_root);

        assert_eq!(resolved, Some(PathBuf::from("src/utils.ts")));
    }

    #[test]
    fn test_resolve_external_returns_none() {
        let parser = GenericParser::new();
        let temp = tempfile::TempDir::new().unwrap();

        let import = ImportRef::new("react", ImportKind::External, 1);
        let from_file = Path::new("src/index.ts");

        let resolved = parser.resolve_import(&import, from_file, temp.path());

        assert!(resolved.is_none());
    }

    #[test]
    fn test_deduplication() {
        let parser = GenericParser::new();
        let content = r#"
import utils from "./utils";
const utils2 = require("./utils");
"#;

        let imports = parser.parse_imports(content);

        // Should only have one "./utils" entry
        let utils_count = imports.iter().filter(|i| i.raw_path == "./utils").count();
        assert_eq!(utils_count, 1);
    }

    #[test]
    fn test_parse_brace_exports() {
        let parser = GenericParser::new();
        let content = "export { foo, bar , baz }";
        let exports = parser.parse_exports(content);

        assert!(exports.iter().any(|e| e.name == "foo"));
        assert!(exports.iter().any(|e| e.name == "bar"));
        assert!(exports.iter().any(|e| e.name == "baz"));
    }

    #[test]
    fn test_resolve_relative_import_to_directory_path() {
        let parser = GenericParser::new();
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        std::fs::create_dir_all(project_root.join("src/utils")).unwrap();
        std::fs::write(project_root.join("src/utils/index.ts"), "").unwrap();

        let import = ImportRef::new("./utils", ImportKind::Relative, 1);
        let from_file = Path::new("src/main.ts");

        let resolved = parser.resolve_import(&import, from_file, project_root);
        assert_eq!(resolved, Some(PathBuf::from("src/./utils")));
    }

    #[test]
    fn test_resolve_relative_import_missing_returns_none() {
        let parser = GenericParser::new();
        let temp = tempfile::TempDir::new().unwrap();

        let import = ImportRef::new("./does-not-exist", ImportKind::Relative, 1);
        let from_file = Path::new("src/main.ts");

        let resolved = parser.resolve_import(&import, from_file, temp.path());
        assert!(resolved.is_none());
    }
}
