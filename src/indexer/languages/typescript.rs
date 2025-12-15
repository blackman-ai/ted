// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! TypeScript/JavaScript language parser for dependency extraction.
//!
//! Parses TypeScript and JavaScript source files to extract:
//! - `import` statements (ES modules)
//! - `require()` calls (CommonJS)
//! - `export` statements

use regex::Regex;
use std::path::{Path, PathBuf};

use super::{ExportKind, ExportRef, ImportKind, ImportRef, LanguageParser};

/// Parser for TypeScript and JavaScript source files.
pub struct TypeScriptParser {
    /// Regex for ES module imports: import x from 'y'
    import_from_regex: Regex,
    /// Regex for side-effect imports: import 'y'
    import_side_effect_regex: Regex,
    /// Regex for dynamic imports: import('y')
    import_dynamic_regex: Regex,
    /// Regex for CommonJS require: require('y')
    require_regex: Regex,
    /// Regex for named exports: export { x, y }
    export_named_regex: Regex,
    /// Regex for export declarations: export function/class/const/let/var
    export_decl_regex: Regex,
    /// Regex for default exports: export default
    export_default_regex: Regex,
    /// Regex for re-exports: export { x } from 'y' or export * from 'y'
    export_from_regex: Regex,
}

impl TypeScriptParser {
    /// Create a new TypeScript/JavaScript parser.
    pub fn new() -> Self {
        Self {
            // Match: import x from 'y'; import { x } from 'y'; import * as x from 'y'; import type { x } from 'y'
            import_from_regex: Regex::new(
                r#"(?m)^\s*import\s+(?:type\s+)?(?:(?:\{[^}]*\}|\*\s+as\s+\w+|\w+)(?:\s*,\s*(?:\{[^}]*\}|\*\s+as\s+\w+))?)\s+from\s+['"]([^'"]+)['"]"#
            ).unwrap(),

            // Match: import 'y'; import "y"
            import_side_effect_regex: Regex::new(
                r#"(?m)^\s*import\s+['"]([^'"]+)['"]"#
            ).unwrap(),

            // Match: import('y') - dynamic imports
            import_dynamic_regex: Regex::new(
                r#"import\s*\(\s*['"]([^'"]+)['"]\s*\)"#
            ).unwrap(),

            // Match: require('y') or require("y")
            require_regex: Regex::new(
                r#"require\s*\(\s*['"]([^'"]+)['"]\s*\)"#
            ).unwrap(),

            // Match: export { x, y, z }
            export_named_regex: Regex::new(
                r#"(?m)^\s*export\s+\{([^}]+)\}"#
            ).unwrap(),

            // Match: export function/class/const/let/var/type/interface/enum name
            export_decl_regex: Regex::new(
                r#"(?m)^\s*export\s+(?:async\s+)?(function|class|const|let|var|type|interface|enum)\s+(\w+)"#
            ).unwrap(),

            // Match: export default function/class name or export default expression
            export_default_regex: Regex::new(
                r#"(?m)^\s*export\s+default\s+(?:(?:async\s+)?(?:function|class)\s+(\w+)|(\w+))"#
            ).unwrap(),

            // Match: export { x } from 'y' or export * from 'y'
            export_from_regex: Regex::new(
                r#"(?m)^\s*export\s+(?:\{[^}]*\}|\*(?:\s+as\s+\w+)?)\s+from\s+['"]([^'"]+)['"]"#
            ).unwrap(),
        }
    }

    /// Classify import path kind.
    fn classify_import(path: &str) -> ImportKind {
        if path.starts_with("./") || path.starts_with("../") {
            ImportKind::Relative
        } else if path.starts_with('/') {
            ImportKind::Absolute
        } else if path.starts_with('@') {
            // Scoped npm packages (e.g., @types/node)
            ImportKind::External
        } else if path.contains('/') && !path.starts_with('.') {
            // Could be npm package with subpath (e.g., lodash/get)
            ImportKind::External
        } else {
            // Bare specifier - npm package or node built-in
            ImportKind::External
        }
    }

    /// Get line number for a byte offset in content.
    fn line_number(content: &str, byte_offset: usize) -> u32 {
        content[..byte_offset].matches('\n').count() as u32 + 1
    }

    /// Check if an import path looks like a local file.
    fn is_local_import(path: &str) -> bool {
        path.starts_with("./") || path.starts_with("../")
    }

    /// Resolve a relative import path to a file path.
    fn resolve_relative_path(
        import_path: &str,
        from_file: &Path,
        project_root: &Path,
    ) -> Option<PathBuf> {
        let from_dir = from_file.parent()?;
        let mut resolved = from_dir.to_path_buf();

        // Handle ./ and ../ prefixes
        let path_parts: Vec<&str> = import_path.split('/').collect();
        for part in &path_parts {
            match *part {
                "." => {}
                ".." => {
                    resolved.pop();
                }
                segment => {
                    resolved.push(segment);
                }
            }
        }

        // Try various extensions
        let extensions = ["", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"];
        let index_files = ["index.ts", "index.tsx", "index.js", "index.jsx"];

        for ext in &extensions {
            let with_ext = if ext.is_empty() {
                resolved.clone()
            } else {
                PathBuf::from(format!("{}{}", resolved.display(), ext))
            };

            let abs_path = project_root.join(&with_ext);
            if abs_path.is_file() {
                return Some(with_ext);
            }
        }

        // Try as directory with index file
        for index in &index_files {
            let index_path = resolved.join(index);
            let abs_path = project_root.join(&index_path);
            if abs_path.is_file() {
                return Some(index_path);
            }
        }

        None
    }
}

impl Default for TypeScriptParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for TypeScriptParser {
    fn extensions(&self) -> &[&str] {
        &["ts", "tsx", "js", "jsx", "mjs", "cjs"]
    }

    fn parse_imports(&self, content: &str) -> Vec<ImportRef> {
        let mut imports = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Parse ES module imports: import x from 'y'
        for cap in self.import_from_regex.captures_iter(content) {
            if let Some(path_match) = cap.get(1) {
                let path = path_match.as_str().to_string();
                if seen.insert(path.clone()) {
                    let kind = Self::classify_import(&path);
                    let line = Self::line_number(content, path_match.start());
                    imports.push(ImportRef::new(path, kind, line));
                }
            }
        }

        // Parse side-effect imports: import 'y'
        for cap in self.import_side_effect_regex.captures_iter(content) {
            if let Some(path_match) = cap.get(1) {
                let path = path_match.as_str().to_string();
                if seen.insert(path.clone()) {
                    let kind = Self::classify_import(&path);
                    let line = Self::line_number(content, path_match.start());
                    imports.push(ImportRef::new(path, kind, line));
                }
            }
        }

        // Parse dynamic imports: import('y')
        for cap in self.import_dynamic_regex.captures_iter(content) {
            if let Some(path_match) = cap.get(1) {
                let path = path_match.as_str().to_string();
                if seen.insert(path.clone()) {
                    let kind = Self::classify_import(&path);
                    let line = Self::line_number(content, path_match.start());
                    imports.push(ImportRef::new(path, kind, line));
                }
            }
        }

        // Parse CommonJS require: require('y')
        for cap in self.require_regex.captures_iter(content) {
            if let Some(path_match) = cap.get(1) {
                let path = path_match.as_str().to_string();
                if seen.insert(path.clone()) {
                    let kind = Self::classify_import(&path);
                    let line = Self::line_number(content, path_match.start());
                    imports.push(ImportRef::new(path, kind, line));
                }
            }
        }

        imports
    }

    fn parse_exports(&self, content: &str) -> Vec<ExportRef> {
        let mut exports = Vec::new();

        // Parse named exports: export { x, y }
        for cap in self.export_named_regex.captures_iter(content) {
            if let Some(names_match) = cap.get(1) {
                let line = Self::line_number(content, names_match.start());
                let names_str = names_match.as_str();

                for name in names_str.split(',') {
                    let name = name.trim();
                    // Handle 'x as y' syntax
                    let export_name = if let Some(pos) = name.find(" as ") {
                        name[pos + 4..].trim()
                    } else {
                        name
                    };

                    if !export_name.is_empty() {
                        exports.push(ExportRef::new(export_name, ExportKind::Other, line));
                    }
                }
            }
        }

        // Parse export declarations: export function/class/const/etc
        for cap in self.export_decl_regex.captures_iter(content) {
            if let (Some(kind_match), Some(name_match)) = (cap.get(1), cap.get(2)) {
                let line = Self::line_number(content, name_match.start());
                let kind = match kind_match.as_str() {
                    "function" => ExportKind::Function,
                    "class" | "type" | "interface" | "enum" => ExportKind::Type,
                    "const" | "let" | "var" => ExportKind::Constant,
                    _ => ExportKind::Other,
                };
                exports.push(ExportRef::new(name_match.as_str(), kind, line));
            }
        }

        // Parse default exports
        for cap in self.export_default_regex.captures_iter(content) {
            let line = Self::line_number(content, cap.get(0).unwrap().start());
            let name = cap
                .get(1)
                .or(cap.get(2))
                .map(|m| m.as_str())
                .unwrap_or("default");
            exports.push(ExportRef::new(name, ExportKind::Default, line));
        }

        // Parse re-exports: export * from 'y'
        for cap in self.export_from_regex.captures_iter(content) {
            if let Some(path_match) = cap.get(1) {
                let line = Self::line_number(content, path_match.start());
                exports.push(ExportRef::new(
                    path_match.as_str(),
                    ExportKind::ReExport,
                    line,
                ));
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
        let path = &import.raw_path;

        // Only resolve local imports
        if !Self::is_local_import(path) {
            return None;
        }

        Self::resolve_relative_path(path, from_file, project_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_es_imports() {
        let parser = TypeScriptParser::new();
        let content = r#"
import React from 'react';
import { useState, useEffect } from 'react';
import * as lodash from 'lodash';
import type { Config } from './config';
"#;

        let imports = parser.parse_imports(content);

        // 'react' appears twice but is deduplicated, so we get 3 unique imports
        assert_eq!(imports.len(), 3);
        assert!(imports.iter().any(|i| i.raw_path == "react"));
        assert!(imports.iter().any(|i| i.raw_path == "lodash"));
        assert!(imports.iter().any(|i| i.raw_path == "./config"));
    }

    #[test]
    fn test_parse_side_effect_imports() {
        let parser = TypeScriptParser::new();
        let content = r#"
import './styles.css';
import 'polyfill';
"#;

        let imports = parser.parse_imports(content);

        assert_eq!(imports.len(), 2);
        assert!(imports.iter().any(|i| i.raw_path == "./styles.css"));
        assert!(imports.iter().any(|i| i.raw_path == "polyfill"));
    }

    #[test]
    fn test_parse_dynamic_imports() {
        let parser = TypeScriptParser::new();
        let content = r#"
const module = await import('./dynamic-module');
const lazy = import('lodash');
"#;

        let imports = parser.parse_imports(content);

        assert_eq!(imports.len(), 2);
        assert!(imports.iter().any(|i| i.raw_path == "./dynamic-module"));
        assert!(imports.iter().any(|i| i.raw_path == "lodash"));
    }

    #[test]
    fn test_parse_require() {
        let parser = TypeScriptParser::new();
        let content = r#"
const fs = require('fs');
const path = require("path");
const local = require('./local');
"#;

        let imports = parser.parse_imports(content);

        assert_eq!(imports.len(), 3);
        assert!(imports.iter().any(|i| i.raw_path == "fs"));
        assert!(imports.iter().any(|i| i.raw_path == "path"));
        assert!(imports.iter().any(|i| i.raw_path == "./local"));
    }

    #[test]
    fn test_classify_imports() {
        assert_eq!(
            TypeScriptParser::classify_import("./local"),
            ImportKind::Relative
        );
        assert_eq!(
            TypeScriptParser::classify_import("../parent"),
            ImportKind::Relative
        );
        assert_eq!(
            TypeScriptParser::classify_import("react"),
            ImportKind::External
        );
        assert_eq!(
            TypeScriptParser::classify_import("@types/node"),
            ImportKind::External
        );
        assert_eq!(
            TypeScriptParser::classify_import("lodash/get"),
            ImportKind::External
        );
    }

    #[test]
    fn test_parse_named_exports() {
        let parser = TypeScriptParser::new();
        let content = r#"
export { foo, bar, baz };
export { internal as external };
"#;

        let exports = parser.parse_exports(content);

        assert!(exports.iter().any(|e| e.name == "foo"));
        assert!(exports.iter().any(|e| e.name == "bar"));
        assert!(exports.iter().any(|e| e.name == "baz"));
        assert!(exports.iter().any(|e| e.name == "external"));
    }

    #[test]
    fn test_parse_export_declarations() {
        let parser = TypeScriptParser::new();
        let content = r#"
export function myFunction() {}
export async function asyncFn() {}
export class MyClass {}
export const MY_CONST = 42;
export let myVar = 'hello';
export type MyType = string;
export interface MyInterface {}
export enum MyEnum { A, B }
"#;

        let exports = parser.parse_exports(content);

        assert!(exports
            .iter()
            .any(|e| e.name == "myFunction" && e.kind == ExportKind::Function));
        assert!(exports
            .iter()
            .any(|e| e.name == "asyncFn" && e.kind == ExportKind::Function));
        assert!(exports
            .iter()
            .any(|e| e.name == "MyClass" && e.kind == ExportKind::Type));
        assert!(exports
            .iter()
            .any(|e| e.name == "MY_CONST" && e.kind == ExportKind::Constant));
        assert!(exports
            .iter()
            .any(|e| e.name == "MyType" && e.kind == ExportKind::Type));
        assert!(exports
            .iter()
            .any(|e| e.name == "MyInterface" && e.kind == ExportKind::Type));
        assert!(exports
            .iter()
            .any(|e| e.name == "MyEnum" && e.kind == ExportKind::Type));
    }

    #[test]
    fn test_parse_default_exports() {
        let parser = TypeScriptParser::new();
        let content = r#"
export default function myDefault() {}
"#;

        let exports = parser.parse_exports(content);

        assert!(exports
            .iter()
            .any(|e| e.name == "myDefault" && e.kind == ExportKind::Default));
    }

    #[test]
    fn test_parse_reexports() {
        let parser = TypeScriptParser::new();
        let content = r#"
export * from './utils';
export { foo, bar } from './helpers';
export * as namespace from './namespace';
"#;

        let exports = parser.parse_exports(content);

        let reexports: Vec<_> = exports
            .iter()
            .filter(|e| e.kind == ExportKind::ReExport)
            .collect();

        assert_eq!(reexports.len(), 3);
    }

    #[test]
    fn test_line_numbers() {
        let parser = TypeScriptParser::new();
        let content = r#"// line 1
// line 2
import foo from 'foo'; // line 3
// line 4
import bar from './bar'; // line 5
"#;

        let imports = parser.parse_imports(content);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].line, 3);
        assert_eq!(imports[1].line, 5);
    }

    #[test]
    fn test_resolve_relative_import() {
        let parser = TypeScriptParser::new();
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
    fn test_resolve_parent_import() {
        let parser = TypeScriptParser::new();
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        // Create src/utils.ts
        std::fs::create_dir_all(project_root.join("src/components")).unwrap();
        std::fs::write(project_root.join("src/utils.ts"), "").unwrap();

        let import = ImportRef::new("../utils", ImportKind::Relative, 1);
        let from_file = Path::new("src/components/Button.tsx");

        let resolved = parser.resolve_import(&import, from_file, project_root);

        assert_eq!(resolved, Some(PathBuf::from("src/utils.ts")));
    }

    #[test]
    fn test_resolve_index_import() {
        let parser = TypeScriptParser::new();
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        // Create src/utils/index.ts
        std::fs::create_dir_all(project_root.join("src/utils")).unwrap();
        std::fs::write(project_root.join("src/utils/index.ts"), "").unwrap();

        let import = ImportRef::new("./utils", ImportKind::Relative, 1);
        let from_file = Path::new("src/index.ts");

        let resolved = parser.resolve_import(&import, from_file, project_root);

        assert_eq!(resolved, Some(PathBuf::from("src/utils/index.ts")));
    }

    #[test]
    fn test_external_import_returns_none() {
        let parser = TypeScriptParser::new();
        let temp = tempfile::TempDir::new().unwrap();

        let import = ImportRef::new("react", ImportKind::External, 1);
        let from_file = Path::new("src/App.tsx");

        let resolved = parser.resolve_import(&import, from_file, temp.path());

        assert!(resolved.is_none());
    }

    #[test]
    fn test_deduplicate_imports() {
        let parser = TypeScriptParser::new();
        let content = r#"
import React from 'react';
import { useState } from 'react';
"#;

        let imports = parser.parse_imports(content);

        // Should deduplicate 'react'
        let react_count = imports.iter().filter(|i| i.raw_path == "react").count();
        assert_eq!(react_count, 1);
    }
}
