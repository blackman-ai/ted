// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Rust language parser for dependency extraction.
//!
//! Parses Rust source files to extract:
//! - `use` statements (imports)
//! - `mod` declarations (submodules)
//! - `pub` items (exports)

use regex::Regex;
use std::path::{Path, PathBuf};

use super::{ExportKind, ExportRef, ImportKind, ImportRef, LanguageParser};

/// Parser for Rust source files.
pub struct RustParser {
    /// Regex for `use` statements
    use_regex: Regex,
    /// Regex for `mod` declarations
    mod_regex: Regex,
    /// Regex for `pub fn`
    pub_fn_regex: Regex,
    /// Regex for `pub struct`
    pub_struct_regex: Regex,
    /// Regex for `pub enum`
    pub_enum_regex: Regex,
    /// Regex for `pub trait`
    pub_trait_regex: Regex,
    /// Regex for `pub const`
    pub_const_regex: Regex,
    /// Regex for `pub mod`
    pub_mod_regex: Regex,
    /// Regex for `pub use` (re-exports)
    pub_use_regex: Regex,
}

impl RustParser {
    /// Create a new Rust parser.
    pub fn new() -> Self {
        Self {
            // Match: use path::to::module; or use path::to::{item1, item2};
            use_regex: Regex::new(
                r"(?m)^\s*use\s+((?:crate|super|self|[a-zA-Z_][a-zA-Z0-9_]*)(?:::[a-zA-Z_][a-zA-Z0-9_]*)*(?:::\{[^}]+\}|::\*)?)\s*;"
            ).unwrap(),

            // Match: mod name; or mod name { ... }
            mod_regex: Regex::new(
                r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*[;{]"
            ).unwrap(),

            // Match: pub fn name
            pub_fn_regex: Regex::new(
                r#"(?m)^\s*pub(?:\([^)]*\))?\s+(?:async\s+)?(?:unsafe\s+)?(?:extern\s+(?:"[^"]+"\s+)?)?fn\s+([a-zA-Z_][a-zA-Z0-9_]*)"#
            ).unwrap(),

            // Match: pub struct Name
            pub_struct_regex: Regex::new(
                r"(?m)^\s*pub(?:\([^)]*\))?\s+struct\s+([a-zA-Z_][a-zA-Z0-9_]*)"
            ).unwrap(),

            // Match: pub enum Name
            pub_enum_regex: Regex::new(
                r"(?m)^\s*pub(?:\([^)]*\))?\s+enum\s+([a-zA-Z_][a-zA-Z0-9_]*)"
            ).unwrap(),

            // Match: pub trait Name
            pub_trait_regex: Regex::new(
                r"(?m)^\s*pub(?:\([^)]*\))?\s+trait\s+([a-zA-Z_][a-zA-Z0-9_]*)"
            ).unwrap(),

            // Match: pub const NAME
            pub_const_regex: Regex::new(
                r"(?m)^\s*pub(?:\([^)]*\))?\s+const\s+([a-zA-Z_][a-zA-Z0-9_]*)"
            ).unwrap(),

            // Match: pub mod name
            pub_mod_regex: Regex::new(
                r"(?m)^\s*pub(?:\([^)]*\))?\s+mod\s+([a-zA-Z_][a-zA-Z0-9_]*)"
            ).unwrap(),

            // Match: pub use path::to::item;
            pub_use_regex: Regex::new(
                r"(?m)^\s*pub(?:\([^)]*\))?\s+use\s+((?:crate|super|self|[a-zA-Z_][a-zA-Z0-9_]*)(?:::[a-zA-Z_][a-zA-Z0-9_]*)*)"
            ).unwrap(),
        }
    }

    /// Determine the import kind from a raw path.
    fn classify_import(raw_path: &str) -> ImportKind {
        if raw_path.starts_with("crate::") {
            ImportKind::CrateRoot
        } else if raw_path.starts_with("super::") {
            ImportKind::Parent
        } else if raw_path.starts_with("self::") {
            ImportKind::Current
        } else if raw_path.starts_with("std::")
            || raw_path.starts_with("core::")
            || raw_path.starts_with("alloc::")
        {
            ImportKind::External
        } else {
            // Could be external crate or local module - assume external for now
            // (resolution will determine actual path)
            ImportKind::External
        }
    }

    /// Get line number for a byte offset in content.
    fn line_number(content: &str, byte_offset: usize) -> u32 {
        content[..byte_offset].matches('\n').count() as u32 + 1
    }
}

impl Default for RustParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for RustParser {
    fn extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn parse_imports(&self, content: &str) -> Vec<ImportRef> {
        let mut imports = Vec::new();

        // Parse `use` statements
        for cap in self.use_regex.captures_iter(content) {
            if let Some(path_match) = cap.get(1) {
                let raw_path = path_match.as_str().to_string();
                let kind = Self::classify_import(&raw_path);
                let line = Self::line_number(content, path_match.start());

                // Handle brace expansion: use foo::{bar, baz}
                if raw_path.contains('{') {
                    // Extract base path and items
                    if let Some(brace_start) = raw_path.find('{') {
                        let base = &raw_path[..brace_start];
                        let items_str = &raw_path[brace_start + 1..raw_path.len() - 1];

                        for item in items_str.split(',') {
                            let item = item.trim();
                            if !item.is_empty() {
                                let full_path = format!("{}{}", base, item);
                                imports.push(ImportRef::new(full_path, kind, line));
                            }
                        }
                    }
                } else {
                    imports.push(ImportRef::new(raw_path, kind, line));
                }
            }
        }

        // Parse `mod` declarations (these are also imports of submodules)
        for cap in self.mod_regex.captures_iter(content) {
            if let Some(name_match) = cap.get(1) {
                let mod_name = name_match.as_str().to_string();
                let line = Self::line_number(content, name_match.start());

                // mod declarations are current-relative
                imports.push(ImportRef::new(
                    format!("self::{}", mod_name),
                    ImportKind::Current,
                    line,
                ));
            }
        }

        imports
    }

    fn parse_exports(&self, content: &str) -> Vec<ExportRef> {
        let mut exports = Vec::new();

        // Parse pub fn
        for cap in self.pub_fn_regex.captures_iter(content) {
            if let Some(name_match) = cap.get(1) {
                let line = Self::line_number(content, name_match.start());
                exports.push(ExportRef::new(
                    name_match.as_str(),
                    ExportKind::Function,
                    line,
                ));
            }
        }

        // Parse pub struct
        for cap in self.pub_struct_regex.captures_iter(content) {
            if let Some(name_match) = cap.get(1) {
                let line = Self::line_number(content, name_match.start());
                exports.push(ExportRef::new(name_match.as_str(), ExportKind::Type, line));
            }
        }

        // Parse pub enum
        for cap in self.pub_enum_regex.captures_iter(content) {
            if let Some(name_match) = cap.get(1) {
                let line = Self::line_number(content, name_match.start());
                exports.push(ExportRef::new(name_match.as_str(), ExportKind::Type, line));
            }
        }

        // Parse pub trait
        for cap in self.pub_trait_regex.captures_iter(content) {
            if let Some(name_match) = cap.get(1) {
                let line = Self::line_number(content, name_match.start());
                exports.push(ExportRef::new(name_match.as_str(), ExportKind::Type, line));
            }
        }

        // Parse pub const
        for cap in self.pub_const_regex.captures_iter(content) {
            if let Some(name_match) = cap.get(1) {
                let line = Self::line_number(content, name_match.start());
                exports.push(ExportRef::new(
                    name_match.as_str(),
                    ExportKind::Constant,
                    line,
                ));
            }
        }

        // Parse pub mod
        for cap in self.pub_mod_regex.captures_iter(content) {
            if let Some(name_match) = cap.get(1) {
                let line = Self::line_number(content, name_match.start());
                exports.push(ExportRef::new(
                    name_match.as_str(),
                    ExportKind::Module,
                    line,
                ));
            }
        }

        // Parse pub use (re-exports)
        for cap in self.pub_use_regex.captures_iter(content) {
            if let Some(path_match) = cap.get(1) {
                let line = Self::line_number(content, path_match.start());
                // Extract the last segment as the exported name
                let path = path_match.as_str();
                let name = path.rsplit("::").next().unwrap_or(path);
                exports.push(ExportRef::new(name, ExportKind::ReExport, line));
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

        // Skip standard library and external crates
        if raw.starts_with("std::") || raw.starts_with("core::") || raw.starts_with("alloc::") {
            return None;
        }

        // Handle crate:: imports
        if let Some(rest) = raw.strip_prefix("crate::") {
            return self.resolve_module_path(rest, Path::new("src"), project_root);
        }

        // Handle super:: imports
        if let Some(rest) = raw.strip_prefix("super::") {
            let parent = from_file.parent()?.parent()?;
            return self.resolve_module_path(rest, parent, project_root);
        }

        // Handle self:: imports (mod declarations)
        if let Some(rest) = raw.strip_prefix("self::") {
            let current_dir = if from_file.file_name() == Some("mod.rs".as_ref())
                || from_file.file_name() == Some("lib.rs".as_ref())
                || from_file.file_name() == Some("main.rs".as_ref())
            {
                from_file.parent()?.to_path_buf()
            } else {
                // foo.rs -> foo/
                let stem = from_file.file_stem()?;
                from_file.parent()?.join(stem)
            };
            return self.resolve_module_path(rest, &current_dir, project_root);
        }

        // External crate - check if it's a local workspace member
        // For now, return None (external dependency)
        None
    }
}

impl RustParser {
    /// Resolve a module path to a file path.
    fn resolve_module_path(
        &self,
        module_path: &str,
        base_dir: &Path,
        project_root: &Path,
    ) -> Option<PathBuf> {
        // Split the path and take only the first segment for file resolution
        let first_segment = module_path.split("::").next()?;

        // Try: base_dir/module.rs
        let file_path = base_dir.join(format!("{}.rs", first_segment));
        let abs_path = project_root.join(&file_path);
        if abs_path.exists() {
            return Some(file_path);
        }

        // Try: base_dir/module/mod.rs
        let mod_path = base_dir.join(first_segment).join("mod.rs");
        let abs_mod_path = project_root.join(&mod_path);
        if abs_mod_path.exists() {
            return Some(mod_path);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_use() {
        let parser = RustParser::new();
        let content = r#"
use std::collections::HashMap;
use crate::utils::helpers;
use super::parent_mod;
"#;

        let imports = parser.parse_imports(content);

        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].raw_path, "std::collections::HashMap");
        assert_eq!(imports[0].kind, ImportKind::External);
        assert_eq!(imports[1].raw_path, "crate::utils::helpers");
        assert_eq!(imports[1].kind, ImportKind::CrateRoot);
        assert_eq!(imports[2].raw_path, "super::parent_mod");
        assert_eq!(imports[2].kind, ImportKind::Parent);
    }

    #[test]
    fn test_parse_brace_use() {
        let parser = RustParser::new();
        let content = "use std::collections::{HashMap, HashSet};";

        let imports = parser.parse_imports(content);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].raw_path, "std::collections::HashMap");
        assert_eq!(imports[1].raw_path, "std::collections::HashSet");
    }

    #[test]
    fn test_parse_mod_declarations() {
        let parser = RustParser::new();
        let content = r#"
mod utils;
pub mod helpers;
mod tests {
    // inline module
}
"#;

        let imports = parser.parse_imports(content);

        // Should find 3 mod declarations
        assert!(imports.len() >= 3);

        let mod_imports: Vec<_> = imports
            .iter()
            .filter(|i| i.kind == ImportKind::Current)
            .collect();
        assert_eq!(mod_imports.len(), 3);
    }

    #[test]
    fn test_parse_pub_fn() {
        let parser = RustParser::new();
        let content = r#"
pub fn my_function() {}
pub async fn async_fn() {}
fn private_fn() {}
pub(crate) fn crate_fn() {}
"#;

        let exports = parser.parse_exports(content);

        let fn_exports: Vec<_> = exports
            .iter()
            .filter(|e| e.kind == ExportKind::Function)
            .collect();

        assert_eq!(fn_exports.len(), 3);
        assert!(fn_exports.iter().any(|e| e.name == "my_function"));
        assert!(fn_exports.iter().any(|e| e.name == "async_fn"));
        assert!(fn_exports.iter().any(|e| e.name == "crate_fn"));
    }

    #[test]
    fn test_parse_pub_struct_enum() {
        let parser = RustParser::new();
        let content = r#"
pub struct MyStruct {
    field: i32,
}

pub enum MyEnum {
    Variant1,
    Variant2,
}

struct PrivateStruct {}
"#;

        let exports = parser.parse_exports(content);

        let type_exports: Vec<_> = exports
            .iter()
            .filter(|e| e.kind == ExportKind::Type)
            .collect();

        assert_eq!(type_exports.len(), 2);
        assert!(type_exports.iter().any(|e| e.name == "MyStruct"));
        assert!(type_exports.iter().any(|e| e.name == "MyEnum"));
    }

    #[test]
    fn test_parse_pub_trait() {
        let parser = RustParser::new();
        let content = r#"
pub trait MyTrait {
    fn method(&self);
}

trait PrivateTrait {}
"#;

        let exports = parser.parse_exports(content);

        let trait_exports: Vec<_> = exports
            .iter()
            .filter(|e| e.kind == ExportKind::Type && e.name == "MyTrait")
            .collect();

        assert_eq!(trait_exports.len(), 1);
    }

    #[test]
    fn test_parse_pub_const() {
        let parser = RustParser::new();
        let content = r#"
pub const MAX_VALUE: i32 = 100;
const PRIVATE_CONST: i32 = 50;
"#;

        let exports = parser.parse_exports(content);

        let const_exports: Vec<_> = exports
            .iter()
            .filter(|e| e.kind == ExportKind::Constant)
            .collect();

        assert_eq!(const_exports.len(), 1);
        assert_eq!(const_exports[0].name, "MAX_VALUE");
    }

    #[test]
    fn test_parse_pub_mod() {
        let parser = RustParser::new();
        let content = r#"
pub mod utils;
mod private_mod;
pub mod helpers {
    // inline
}
"#;

        let exports = parser.parse_exports(content);

        let mod_exports: Vec<_> = exports
            .iter()
            .filter(|e| e.kind == ExportKind::Module)
            .collect();

        assert_eq!(mod_exports.len(), 2);
    }

    #[test]
    fn test_parse_pub_use() {
        let parser = RustParser::new();
        let content = r#"
pub use crate::error::Error;
pub use std::collections::HashMap;
"#;

        let exports = parser.parse_exports(content);

        let reexports: Vec<_> = exports
            .iter()
            .filter(|e| e.kind == ExportKind::ReExport)
            .collect();

        assert_eq!(reexports.len(), 2);
        assert!(reexports.iter().any(|e| e.name == "Error"));
        assert!(reexports.iter().any(|e| e.name == "HashMap"));
    }

    #[test]
    fn test_line_numbers() {
        let parser = RustParser::new();
        let content = r#"// line 1
// line 2
use std::io; // line 3
// line 4
use crate::utils; // line 5
"#;

        let imports = parser.parse_imports(content);

        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].line, 3);
        assert_eq!(imports[1].line, 5);
    }

    #[test]
    fn test_classify_import() {
        assert_eq!(
            RustParser::classify_import("crate::utils"),
            ImportKind::CrateRoot
        );
        assert_eq!(
            RustParser::classify_import("super::parent"),
            ImportKind::Parent
        );
        assert_eq!(
            RustParser::classify_import("self::current"),
            ImportKind::Current
        );
        assert_eq!(
            RustParser::classify_import("std::collections"),
            ImportKind::External
        );
        assert_eq!(
            RustParser::classify_import("some_crate::module"),
            ImportKind::External
        );
    }

    #[test]
    fn test_resolve_crate_import() {
        let parser = RustParser::new();
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        // Create src/utils.rs
        std::fs::create_dir_all(project_root.join("src")).unwrap();
        std::fs::write(project_root.join("src/utils.rs"), "").unwrap();

        let import = ImportRef::new("crate::utils", ImportKind::CrateRoot, 1);
        let from_file = Path::new("src/main.rs");

        let resolved = parser.resolve_import(&import, from_file, project_root);

        assert_eq!(resolved, Some(PathBuf::from("src/utils.rs")));
    }

    #[test]
    fn test_resolve_mod_import() {
        let parser = RustParser::new();
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        // Create src/utils/mod.rs
        std::fs::create_dir_all(project_root.join("src/utils")).unwrap();
        std::fs::write(project_root.join("src/utils/mod.rs"), "").unwrap();

        let import = ImportRef::new("crate::utils", ImportKind::CrateRoot, 1);
        let from_file = Path::new("src/lib.rs");

        let resolved = parser.resolve_import(&import, from_file, project_root);

        assert_eq!(resolved, Some(PathBuf::from("src/utils/mod.rs")));
    }

    #[test]
    fn test_resolve_external_returns_none() {
        let parser = RustParser::new();
        let temp = tempfile::TempDir::new().unwrap();

        let import = ImportRef::new("std::collections::HashMap", ImportKind::External, 1);
        let from_file = Path::new("src/main.rs");

        let resolved = parser.resolve_import(&import, from_file, temp.path());

        assert!(resolved.is_none());
    }
}
