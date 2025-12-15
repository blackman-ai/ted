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
}
