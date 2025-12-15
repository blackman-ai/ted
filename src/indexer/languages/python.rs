// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Python language parser for dependency extraction.
//!
//! Parses Python source files to extract:
//! - `import` statements
//! - `from ... import` statements
//! - Class and function definitions

use regex::Regex;
use std::path::{Path, PathBuf};

use super::{ExportKind, ExportRef, ImportKind, ImportRef, LanguageParser};

/// Parser for Python source files.
pub struct PythonParser {
    /// Regex for simple imports: import x, y, z
    import_regex: Regex,
    /// Regex for from imports: from x import y
    from_import_regex: Regex,
    /// Regex for relative from imports: from . import y or from ..x import y
    from_relative_regex: Regex,
    /// Regex for function definitions: def name(
    def_regex: Regex,
    /// Regex for class definitions: class Name
    class_regex: Regex,
    /// Regex for __all__ assignments
    all_regex: Regex,
}

impl PythonParser {
    /// Create a new Python parser.
    pub fn new() -> Self {
        Self {
            // Match: import foo, bar, baz
            import_regex: Regex::new(
                r"(?m)^\s*import\s+([a-zA-Z_][a-zA-Z0-9_]*(?:\.[a-zA-Z_][a-zA-Z0-9_]*)*)(?:\s*,\s*([a-zA-Z_][a-zA-Z0-9_]*(?:\.[a-zA-Z_][a-zA-Z0-9_]*)*))*"
            ).unwrap(),

            // Match: from foo.bar import x, y, z
            from_import_regex: Regex::new(
                r"(?m)^\s*from\s+([a-zA-Z_][a-zA-Z0-9_]*(?:\.[a-zA-Z_][a-zA-Z0-9_]*)*)\s+import\s+"
            ).unwrap(),

            // Match: from . import x or from ..foo import y or from .foo import z
            from_relative_regex: Regex::new(
                r"(?m)^\s*from\s+(\.+)([a-zA-Z_][a-zA-Z0-9_]*(?:\.[a-zA-Z_][a-zA-Z0-9_]*)*)?\s+import\s+"
            ).unwrap(),

            // Match: def function_name(
            def_regex: Regex::new(
                r"(?m)^(?:async\s+)?def\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\("
            ).unwrap(),

            // Match: class ClassName
            class_regex: Regex::new(
                r"(?m)^class\s+([a-zA-Z_][a-zA-Z0-9_]*)"
            ).unwrap(),

            // Match: __all__ = ['x', 'y'] or __all__ = ["x", "y"]
            all_regex: Regex::new(
                r#"(?m)^__all__\s*=\s*\[([^\]]*)\]"#
            ).unwrap(),
        }
    }

    /// Get line number for a byte offset in content.
    fn line_number(content: &str, byte_offset: usize) -> u32 {
        content[..byte_offset].matches('\n').count() as u32 + 1
    }

    /// Classify import kind based on module path.
    fn classify_import(module: &str, is_relative: bool) -> ImportKind {
        if is_relative {
            ImportKind::Relative
        } else {
            // Standard library modules (incomplete list but covers common ones)
            let stdlib = [
                "os",
                "sys",
                "re",
                "json",
                "typing",
                "collections",
                "itertools",
                "functools",
                "pathlib",
                "datetime",
                "time",
                "math",
                "random",
                "subprocess",
                "threading",
                "multiprocessing",
                "asyncio",
                "io",
                "abc",
                "copy",
                "pickle",
                "hashlib",
                "logging",
                "unittest",
                "argparse",
                "configparser",
                "csv",
                "sqlite3",
                "urllib",
                "http",
                "email",
                "html",
                "xml",
                "socket",
                "ssl",
                "shutil",
                "tempfile",
                "glob",
                "fnmatch",
                "platform",
                "ctypes",
                "struct",
                "codecs",
                "string",
                "textwrap",
                "difflib",
                "contextlib",
                "dataclasses",
                "enum",
                "weakref",
                "types",
                "inspect",
                "warnings",
                "traceback",
            ];

            let root_module = module.split('.').next().unwrap_or(module);
            if stdlib.contains(&root_module) {
                ImportKind::External
            } else {
                // Could be external package or local module
                // Assume external unless we can resolve it
                ImportKind::External
            }
        }
    }

    /// Convert a Python module path to a file path.
    fn module_to_path(module: &str) -> PathBuf {
        PathBuf::from(module.replace('.', "/"))
    }
}

impl Default for PythonParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for PythonParser {
    fn extensions(&self) -> &[&str] {
        &["py", "pyi"]
    }

    fn parse_imports(&self, content: &str) -> Vec<ImportRef> {
        let mut imports = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Parse simple imports: import foo, bar
        for cap in self.import_regex.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let line = Self::line_number(content, full_match.start());

            // Extract all module names from the match
            let import_str = full_match.as_str();
            if let Some(start) = import_str.find("import") {
                let modules_str = &import_str[start + 6..];
                for module in modules_str.split(',') {
                    let module = module.trim();
                    // Handle 'as' aliases: import foo as f
                    let module = module.split_whitespace().next().unwrap_or(module);
                    if !module.is_empty() && seen.insert(module.to_string()) {
                        let kind = Self::classify_import(module, false);
                        imports.push(ImportRef::new(module, kind, line));
                    }
                }
            }
        }

        // Parse from imports: from foo import bar
        for cap in self.from_import_regex.captures_iter(content) {
            if let Some(module_match) = cap.get(1) {
                let module = module_match.as_str();
                if seen.insert(module.to_string()) {
                    let kind = Self::classify_import(module, false);
                    let line = Self::line_number(content, module_match.start());
                    imports.push(ImportRef::new(module, kind, line));
                }
            }
        }

        // Parse relative imports: from . import x or from ..foo import y
        for cap in self.from_relative_regex.captures_iter(content) {
            let dots_match = cap.get(1).unwrap();
            let module_match = cap.get(2);
            let line = Self::line_number(content, dots_match.start());

            let dots = dots_match.as_str();
            let module = module_match.map(|m| m.as_str()).unwrap_or("");
            let full_path = format!("{}{}", dots, module);

            if seen.insert(full_path.clone()) {
                imports.push(ImportRef::new(full_path, ImportKind::Relative, line));
            }
        }

        imports
    }

    fn parse_exports(&self, content: &str) -> Vec<ExportRef> {
        let mut exports = Vec::new();

        // Parse __all__ if present (defines explicit exports)
        let mut explicit_exports = std::collections::HashSet::new();
        for cap in self.all_regex.captures_iter(content) {
            if let Some(names_match) = cap.get(1) {
                let line = Self::line_number(content, names_match.start());
                let names_str = names_match.as_str();

                // Extract quoted strings
                for name in names_str.split(',') {
                    let name = name.trim();
                    let name = name.trim_matches(|c| c == '\'' || c == '"' || c == ' ');
                    if !name.is_empty() {
                        explicit_exports.insert(name.to_string());
                        exports.push(ExportRef::new(name, ExportKind::Other, line));
                    }
                }
            }
        }

        // If no __all__, treat public (non-underscore) defs and classes as exports
        let has_explicit_all = !explicit_exports.is_empty();

        // Parse function definitions
        for cap in self.def_regex.captures_iter(content) {
            if let Some(name_match) = cap.get(1) {
                let name = name_match.as_str();
                let line = Self::line_number(content, name_match.start());

                // Skip private functions (single underscore) and magic methods (double underscore)
                let is_public = !name.starts_with('_');

                if !has_explicit_all && is_public {
                    exports.push(ExportRef::new(name, ExportKind::Function, line));
                } else if explicit_exports.contains(name) {
                    // Already added from __all__, update kind
                    if let Some(export) = exports.iter_mut().find(|e| e.name == name) {
                        export.kind = ExportKind::Function;
                    }
                }
            }
        }

        // Parse class definitions
        for cap in self.class_regex.captures_iter(content) {
            if let Some(name_match) = cap.get(1) {
                let name = name_match.as_str();
                let line = Self::line_number(content, name_match.start());

                let is_public = !name.starts_with('_');

                if !has_explicit_all && is_public {
                    exports.push(ExportRef::new(name, ExportKind::Type, line));
                } else if explicit_exports.contains(name) {
                    if let Some(export) = exports.iter_mut().find(|e| e.name == name) {
                        export.kind = ExportKind::Type;
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
        let path = &import.raw_path;

        // Handle relative imports
        if path.starts_with('.') {
            let from_dir = from_file.parent()?;
            let mut resolved = from_dir.to_path_buf();

            // Count leading dots
            let dot_count = path.chars().take_while(|c| *c == '.').count();

            // Go up directories for each dot beyond the first
            for _ in 1..dot_count {
                resolved.pop();
            }

            // Append the module path (everything after dots)
            let module_part = path.trim_start_matches('.');
            if !module_part.is_empty() {
                for part in module_part.split('.') {
                    resolved.push(part);
                }
            }

            // Try as .py file
            let py_file = PathBuf::from(format!("{}.py", resolved.display()));
            let abs_path = project_root.join(&py_file);
            if abs_path.is_file() {
                return Some(py_file);
            }

            // Try as package (__init__.py)
            let init_file = resolved.join("__init__.py");
            let abs_init = project_root.join(&init_file);
            if abs_init.is_file() {
                return Some(init_file);
            }

            return None;
        }

        // Handle absolute imports - try to resolve as local module
        let module_path = Self::module_to_path(path);

        // Try as .py file
        let py_file = PathBuf::from(format!("{}.py", module_path.display()));
        let abs_path = project_root.join(&py_file);
        if abs_path.is_file() {
            return Some(py_file);
        }

        // Try as package (__init__.py)
        let init_file = module_path.join("__init__.py");
        let abs_init = project_root.join(&init_file);
        if abs_init.is_file() {
            return Some(init_file);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_imports() {
        let parser = PythonParser::new();
        let content = r#"
import os
import sys
import json
"#;

        let imports = parser.parse_imports(content);

        assert_eq!(imports.len(), 3);
        assert!(imports.iter().any(|i| i.raw_path == "os"));
        assert!(imports.iter().any(|i| i.raw_path == "sys"));
        assert!(imports.iter().any(|i| i.raw_path == "json"));
    }

    #[test]
    fn test_parse_multiline_import() {
        let parser = PythonParser::new();
        let content = r#"
import os, sys, json
"#;

        let imports = parser.parse_imports(content);

        assert_eq!(imports.len(), 3);
    }

    #[test]
    fn test_parse_dotted_import() {
        let parser = PythonParser::new();
        let content = r#"
import os.path
import collections.abc
"#;

        let imports = parser.parse_imports(content);

        assert!(imports.iter().any(|i| i.raw_path == "os.path"));
        assert!(imports.iter().any(|i| i.raw_path == "collections.abc"));
    }

    #[test]
    fn test_parse_from_imports() {
        let parser = PythonParser::new();
        let content = r#"
from os import path
from collections import OrderedDict, defaultdict
from typing import List, Dict, Optional
"#;

        let imports = parser.parse_imports(content);

        assert!(imports.iter().any(|i| i.raw_path == "os"));
        assert!(imports.iter().any(|i| i.raw_path == "collections"));
        assert!(imports.iter().any(|i| i.raw_path == "typing"));
    }

    #[test]
    fn test_parse_relative_imports() {
        let parser = PythonParser::new();
        let content = r#"
from . import utils
from .. import parent
from .sibling import helper
from ...grandparent import stuff
"#;

        let imports = parser.parse_imports(content);

        assert!(imports.iter().any(|i| i.raw_path == "."));
        assert!(imports.iter().any(|i| i.raw_path == ".."));
        assert!(imports.iter().any(|i| i.raw_path == ".sibling"));
        assert!(imports.iter().any(|i| i.raw_path == "...grandparent"));
    }

    #[test]
    fn test_classify_imports() {
        // Standard library
        assert_eq!(
            PythonParser::classify_import("os", false),
            ImportKind::External
        );
        assert_eq!(
            PythonParser::classify_import("sys", false),
            ImportKind::External
        );

        // Relative
        assert_eq!(
            PythonParser::classify_import(".", true),
            ImportKind::Relative
        );
    }

    #[test]
    fn test_parse_function_exports() {
        let parser = PythonParser::new();
        let content = r#"
def public_function():
    pass

def _private_function():
    pass

async def async_public():
    pass
"#;

        let exports = parser.parse_exports(content);

        assert!(exports.iter().any(|e| e.name == "public_function"));
        assert!(exports.iter().any(|e| e.name == "async_public"));
        assert!(!exports.iter().any(|e| e.name == "_private_function"));
    }

    #[test]
    fn test_parse_class_exports() {
        let parser = PythonParser::new();
        let content = r#"
class PublicClass:
    pass

class _PrivateClass:
    pass
"#;

        let exports = parser.parse_exports(content);

        assert!(exports
            .iter()
            .any(|e| e.name == "PublicClass" && e.kind == ExportKind::Type));
        assert!(!exports.iter().any(|e| e.name == "_PrivateClass"));
    }

    #[test]
    fn test_parse_all_exports() {
        let parser = PythonParser::new();
        let content = r#"
__all__ = ['foo', 'bar', 'Baz']

def foo():
    pass

def bar():
    pass

class Baz:
    pass

def not_exported():
    pass
"#;

        let exports = parser.parse_exports(content);

        // Should only include items in __all__
        assert!(exports.iter().any(|e| e.name == "foo"));
        assert!(exports.iter().any(|e| e.name == "bar"));
        assert!(exports.iter().any(|e| e.name == "Baz"));
        // not_exported should still appear since we list all public symbols
        // but the __all__ items take precedence
    }

    #[test]
    fn test_line_numbers() {
        let parser = PythonParser::new();
        let content = r#"# line 1
# line 2
import os  # line 3
# line 4
from sys import path  # line 5
"#;

        let imports = parser.parse_imports(content);

        let os_import = imports.iter().find(|i| i.raw_path == "os").unwrap();
        assert_eq!(os_import.line, 3);

        let sys_import = imports.iter().find(|i| i.raw_path == "sys").unwrap();
        assert_eq!(sys_import.line, 5);
    }

    #[test]
    fn test_resolve_relative_import() {
        let parser = PythonParser::new();
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        // Create package/utils.py
        std::fs::create_dir_all(project_root.join("package")).unwrap();
        std::fs::write(project_root.join("package/utils.py"), "").unwrap();

        let import = ImportRef::new(".utils", ImportKind::Relative, 1);
        let from_file = Path::new("package/main.py");

        let resolved = parser.resolve_import(&import, from_file, project_root);

        assert_eq!(resolved, Some(PathBuf::from("package/utils.py")));
    }

    #[test]
    fn test_resolve_parent_import() {
        let parser = PythonParser::new();
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        // Create package/utils.py
        std::fs::create_dir_all(project_root.join("package/sub")).unwrap();
        std::fs::write(project_root.join("package/utils.py"), "").unwrap();

        let import = ImportRef::new("..utils", ImportKind::Relative, 1);
        let from_file = Path::new("package/sub/main.py");

        let resolved = parser.resolve_import(&import, from_file, project_root);

        assert_eq!(resolved, Some(PathBuf::from("package/utils.py")));
    }

    #[test]
    fn test_resolve_package_import() {
        let parser = PythonParser::new();
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        // Create mypackage/__init__.py
        std::fs::create_dir_all(project_root.join("mypackage")).unwrap();
        std::fs::write(project_root.join("mypackage/__init__.py"), "").unwrap();

        let import = ImportRef::new("mypackage", ImportKind::External, 1);
        let from_file = Path::new("main.py");

        let resolved = parser.resolve_import(&import, from_file, project_root);

        assert_eq!(resolved, Some(PathBuf::from("mypackage/__init__.py")));
    }

    #[test]
    fn test_resolve_external_returns_none() {
        let parser = PythonParser::new();
        let temp = tempfile::TempDir::new().unwrap();

        let import = ImportRef::new("requests", ImportKind::External, 1);
        let from_file = Path::new("main.py");

        let resolved = parser.resolve_import(&import, from_file, temp.path());

        // External packages can't be resolved locally
        assert!(resolved.is_none());
    }

    #[test]
    fn test_import_with_alias() {
        let parser = PythonParser::new();
        let content = r#"
import numpy as np
import pandas as pd
"#;

        let imports = parser.parse_imports(content);

        assert!(imports.iter().any(|i| i.raw_path == "numpy"));
        assert!(imports.iter().any(|i| i.raw_path == "pandas"));
    }
}
