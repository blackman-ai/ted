// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Language-specific parsers for dependency extraction.
//!
//! This module provides a plugin architecture for parsing imports/exports
//! from different programming languages. Each parser implements the
//! `LanguageParser` trait.
//!
//! # Supported Languages
//!
//! - **Rust**: `use`, `mod`, `crate::`, `super::`, `self::`
//! - **TypeScript/JavaScript**: ES modules, CommonJS require, dynamic imports
//! - **Python**: `import`, `from ... import`, relative imports
//! - **Go**: `import` statements, exported symbols (capitalized names)
//! - **Generic**: Fallback regex-based parser for common patterns
//!
//! # Adding a New Language
//!
//! 1. Create a new file in `src/indexer/languages/` (e.g., `ruby.rs`)
//! 2. Implement the `LanguageParser` trait
//! 3. Register in `ParserRegistry::new()`

pub mod generic;
pub mod go;
pub mod python;
pub mod rust;
pub mod typescript;

use std::path::Path;

use super::memory::Language;

/// A reference to an imported module/file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportRef {
    /// The raw import path as written in source (e.g., "crate::utils::helpers")
    pub raw_path: String,
    /// Import kind for resolution hints
    pub kind: ImportKind,
    /// Line number where import appears (1-indexed)
    pub line: u32,
}

impl ImportRef {
    /// Create a new import reference.
    pub fn new(raw_path: impl Into<String>, kind: ImportKind, line: u32) -> Self {
        Self {
            raw_path: raw_path.into(),
            kind,
            line,
        }
    }
}

/// Kind of import for resolution hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportKind {
    /// Relative to crate root (Rust: `crate::`)
    CrateRoot,
    /// Relative to parent module (Rust: `super::`)
    Parent,
    /// Relative to current module (Rust: `self::`)
    Current,
    /// External dependency (Rust: external crate, JS: npm package)
    External,
    /// Relative path (JS: `./foo`, `../bar`)
    Relative,
    /// Absolute/unknown
    Absolute,
}

/// A reference to an exported symbol.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExportRef {
    /// The symbol name being exported
    pub name: String,
    /// Export kind
    pub kind: ExportKind,
    /// Line number where export appears (1-indexed)
    pub line: u32,
}

impl ExportRef {
    /// Create a new export reference.
    pub fn new(name: impl Into<String>, kind: ExportKind, line: u32) -> Self {
        Self {
            name: name.into(),
            kind,
            line,
        }
    }
}

/// Kind of export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportKind {
    /// Public function
    Function,
    /// Public struct/class/type
    Type,
    /// Public constant
    Constant,
    /// Public module
    Module,
    /// Re-export from another module
    ReExport,
    /// Default export (JS/TS)
    Default,
    /// Unknown/other
    Other,
}

/// Trait for language-specific import/export parsing.
///
/// Implementations should be stateless and thread-safe.
pub trait LanguageParser: Send + Sync {
    /// File extensions this parser handles.
    fn extensions(&self) -> &[&str];

    /// Parse import statements from source content.
    fn parse_imports(&self, content: &str) -> Vec<ImportRef>;

    /// Parse export statements from source content.
    fn parse_exports(&self, content: &str) -> Vec<ExportRef>;

    /// Resolve a raw import path to a file path relative to project root.
    ///
    /// # Arguments
    /// * `import` - The import reference to resolve
    /// * `from_file` - The file containing the import (relative to project root)
    /// * `project_root` - Absolute path to project root
    ///
    /// # Returns
    /// Resolved file path relative to project root, or None if unresolvable
    fn resolve_import(
        &self,
        import: &ImportRef,
        from_file: &Path,
        project_root: &Path,
    ) -> Option<std::path::PathBuf>;
}

/// Registry of language parsers.
pub struct ParserRegistry {
    parsers: Vec<Box<dyn LanguageParser>>,
}

impl ParserRegistry {
    /// Create a new registry with all built-in parsers.
    pub fn new() -> Self {
        Self {
            parsers: vec![
                Box::new(rust::RustParser::new()),
                Box::new(typescript::TypeScriptParser::new()),
                Box::new(python::PythonParser::new()),
                Box::new(go::GoParser::new()),
                Box::new(generic::GenericParser::new()),
            ],
        }
    }

    /// Get a parser for a file extension.
    pub fn parser_for_extension(&self, ext: &str) -> Option<&dyn LanguageParser> {
        let ext_lower = ext.to_lowercase();
        for parser in &self.parsers {
            if parser.extensions().iter().any(|e| *e == ext_lower) {
                return Some(parser.as_ref());
            }
        }
        // Fall back to generic parser
        self.parsers.last().map(|p| p.as_ref())
    }

    /// Get a parser for a language.
    pub fn parser_for_language(&self, lang: Language) -> Option<&dyn LanguageParser> {
        match lang {
            Language::Rust => self.parser_for_extension("rs"),
            Language::TypeScript | Language::JavaScript => self.parser_for_extension("ts"),
            Language::Python => self.parser_for_extension("py"),
            Language::Go => self.parser_for_extension("go"),
            _ => self.parsers.last().map(|p| p.as_ref()),
        }
    }

    /// Get a parser for a file path.
    pub fn parser_for_path(&self, path: &Path) -> Option<&dyn LanguageParser> {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(|ext| self.parser_for_extension(ext))
    }

    /// Parse imports from a file.
    pub fn parse_imports(&self, path: &Path, content: &str) -> Vec<ImportRef> {
        self.parser_for_path(path)
            .map(|p| p.parse_imports(content))
            .unwrap_or_default()
    }

    /// Parse exports from a file.
    pub fn parse_exports(&self, path: &Path, content: &str) -> Vec<ExportRef> {
        self.parser_for_path(path)
            .map(|p| p.parse_exports(content))
            .unwrap_or_default()
    }
}

impl Default for ParserRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_ref_creation() {
        let import = ImportRef::new("crate::utils", ImportKind::CrateRoot, 5);
        assert_eq!(import.raw_path, "crate::utils");
        assert_eq!(import.kind, ImportKind::CrateRoot);
        assert_eq!(import.line, 5);
    }

    #[test]
    fn test_export_ref_creation() {
        let export = ExportRef::new("MyStruct", ExportKind::Type, 10);
        assert_eq!(export.name, "MyStruct");
        assert_eq!(export.kind, ExportKind::Type);
        assert_eq!(export.line, 10);
    }

    #[test]
    fn test_parser_registry_creation() {
        let registry = ParserRegistry::new();
        assert!(registry.parser_for_extension("rs").is_some());
    }

    #[test]
    fn test_parser_for_extension() {
        let registry = ParserRegistry::new();

        // Rust extensions
        assert!(registry.parser_for_extension("rs").is_some());

        // TypeScript/JavaScript extensions
        assert!(registry.parser_for_extension("ts").is_some());
        assert!(registry.parser_for_extension("tsx").is_some());
        assert!(registry.parser_for_extension("js").is_some());
        assert!(registry.parser_for_extension("jsx").is_some());

        // Python extensions
        assert!(registry.parser_for_extension("py").is_some());
        assert!(registry.parser_for_extension("pyi").is_some());

        // Go extensions
        assert!(registry.parser_for_extension("go").is_some());

        // Unknown extensions fall back to generic
        assert!(registry.parser_for_extension("xyz").is_some());
    }

    #[test]
    fn test_parser_for_language() {
        let registry = ParserRegistry::new();

        assert!(registry.parser_for_language(Language::Rust).is_some());
        assert!(registry.parser_for_language(Language::Unknown).is_some());
    }

    #[test]
    fn test_parser_for_path() {
        let registry = ParserRegistry::new();

        assert!(registry.parser_for_path(Path::new("src/main.rs")).is_some());
        assert!(registry
            .parser_for_path(Path::new("lib/utils.py"))
            .is_some());
    }

    // ===== Additional Coverage Tests =====

    #[test]
    fn test_parser_for_language_typescript() {
        let registry = ParserRegistry::new();
        assert!(registry.parser_for_language(Language::TypeScript).is_some());
    }

    #[test]
    fn test_parser_for_language_javascript() {
        let registry = ParserRegistry::new();
        assert!(registry.parser_for_language(Language::JavaScript).is_some());
    }

    #[test]
    fn test_parser_for_language_python() {
        let registry = ParserRegistry::new();
        assert!(registry.parser_for_language(Language::Python).is_some());
    }

    #[test]
    fn test_parser_for_language_go() {
        let registry = ParserRegistry::new();
        assert!(registry.parser_for_language(Language::Go).is_some());
    }

    #[test]
    fn test_parse_imports_registry() {
        let registry = ParserRegistry::new();
        let rust_content = r#"
use std::collections::HashMap;
use crate::utils::helpers;
"#;
        let imports = registry.parse_imports(Path::new("src/main.rs"), rust_content);
        assert!(!imports.is_empty());
    }

    #[test]
    fn test_parse_exports_registry() {
        let registry = ParserRegistry::new();
        let rust_content = r#"
pub fn my_function() {}
pub struct MyStruct {}
"#;
        let exports = registry.parse_exports(Path::new("src/lib.rs"), rust_content);
        assert!(!exports.is_empty());
    }

    #[test]
    fn test_parse_imports_no_extension() {
        let registry = ParserRegistry::new();
        // Path without extension should fall back to generic parser
        let imports = registry.parse_imports(Path::new("Makefile"), "include other.mk");
        // Generic parser may or may not find imports, but shouldn't crash
        let _ = imports;
    }

    #[test]
    fn test_parse_exports_no_extension() {
        let registry = ParserRegistry::new();
        let exports = registry.parse_exports(Path::new("Makefile"), "target: deps");
        let _ = exports;
    }

    #[test]
    fn test_parser_registry_default() {
        let registry = ParserRegistry::default();
        // Should have the same parsers as new()
        assert!(registry.parser_for_extension("rs").is_some());
        assert!(registry.parser_for_extension("py").is_some());
        assert!(registry.parser_for_extension("go").is_some());
    }

    #[test]
    fn test_parser_for_path_no_extension() {
        let registry = ParserRegistry::new();
        // Path without extension returns None (no extension to match)
        let parser = registry.parser_for_path(Path::new("Makefile"));
        assert!(parser.is_none());
    }

    #[test]
    fn test_import_kind_variants() {
        // Test all ImportKind variants
        let kinds = [
            ImportKind::CrateRoot,
            ImportKind::Parent,
            ImportKind::Current,
            ImportKind::External,
            ImportKind::Relative,
            ImportKind::Absolute,
        ];

        for kind in kinds {
            let import = ImportRef::new("test", kind, 1);
            assert_eq!(import.kind, kind);
        }
    }

    #[test]
    fn test_export_kind_variants() {
        // Test all ExportKind variants
        let kinds = [
            ExportKind::Function,
            ExportKind::Type,
            ExportKind::Constant,
            ExportKind::Module,
            ExportKind::ReExport,
            ExportKind::Default,
            ExportKind::Other,
        ];

        for kind in kinds {
            let export = ExportRef::new("test", kind, 1);
            assert_eq!(export.kind, kind);
        }
    }

    #[test]
    fn test_import_ref_clone() {
        let import = ImportRef::new("crate::utils", ImportKind::CrateRoot, 5);
        let cloned = import.clone();
        assert_eq!(import.raw_path, cloned.raw_path);
        assert_eq!(import.kind, cloned.kind);
        assert_eq!(import.line, cloned.line);
    }

    #[test]
    fn test_export_ref_clone() {
        let export = ExportRef::new("MyFunc", ExportKind::Function, 10);
        let cloned = export.clone();
        assert_eq!(export.name, cloned.name);
        assert_eq!(export.kind, cloned.kind);
        assert_eq!(export.line, cloned.line);
    }

    #[test]
    fn test_import_ref_hash() {
        use std::collections::HashSet;
        let import1 = ImportRef::new("crate::utils", ImportKind::CrateRoot, 5);
        let import2 = ImportRef::new("crate::utils", ImportKind::CrateRoot, 5);
        let import3 = ImportRef::new("crate::other", ImportKind::CrateRoot, 5);

        let mut set = HashSet::new();
        set.insert(import1.clone());
        set.insert(import2);
        set.insert(import3);

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_export_ref_hash() {
        use std::collections::HashSet;
        let export1 = ExportRef::new("MyFunc", ExportKind::Function, 10);
        let export2 = ExportRef::new("MyFunc", ExportKind::Function, 10);
        let export3 = ExportRef::new("OtherFunc", ExportKind::Function, 10);

        let mut set = HashSet::new();
        set.insert(export1.clone());
        set.insert(export2);
        set.insert(export3);

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_import_ref_debug() {
        let import = ImportRef::new("crate::utils", ImportKind::CrateRoot, 5);
        let debug_str = format!("{:?}", import);
        assert!(debug_str.contains("ImportRef"));
        assert!(debug_str.contains("crate::utils"));
    }

    #[test]
    fn test_export_ref_debug() {
        let export = ExportRef::new("MyFunc", ExportKind::Function, 10);
        let debug_str = format!("{:?}", export);
        assert!(debug_str.contains("ExportRef"));
        assert!(debug_str.contains("MyFunc"));
    }
}
