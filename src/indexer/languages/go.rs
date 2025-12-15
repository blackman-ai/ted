// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Go language parser for dependency extraction.
//!
//! Parses Go source files to extract:
//! - `import` statements
//! - Exported functions, types, and constants (capitalized names)

use regex::Regex;
use std::path::{Path, PathBuf};

use super::{ExportKind, ExportRef, ImportKind, ImportRef, LanguageParser};

/// Parser for Go source files.
pub struct GoParser {
    /// Regex for single import: import "path"
    import_single_regex: Regex,
    /// Regex for import block: import ( ... )
    import_block_regex: Regex,
    /// Regex for individual imports within a block
    import_line_regex: Regex,
    /// Regex for exported functions: func Name(
    func_regex: Regex,
    /// Regex for exported types: type Name struct/interface/etc
    type_regex: Regex,
    /// Regex for exported variables: var Name =
    var_regex: Regex,
}

impl GoParser {
    /// Create a new Go parser.
    pub fn new() -> Self {
        Self {
            // Match: import "fmt" or import . "fmt" or import alias "fmt"
            import_single_regex: Regex::new(r#"(?m)^\s*import\s+(?:(\w+|\.)\s+)?["']([^"']+)["']"#)
                .unwrap(),

            // Match: import ( ... )
            import_block_regex: Regex::new(r#"(?ms)import\s*\(([^)]+)\)"#).unwrap(),

            // Match: individual imports within block (with optional alias)
            import_line_regex: Regex::new(r#"(?m)^\s*(?:(\w+|\.)\s+)?["']([^"']+)["']"#).unwrap(),

            // Match: func Name( or func (r *Receiver) Name(
            func_regex: Regex::new(r#"(?m)^func\s+(?:\([^)]*\)\s+)?([A-Z][a-zA-Z0-9_]*)\s*\("#)
                .unwrap(),

            // Match: type Name struct/interface/etc or type Name = alias
            type_regex: Regex::new(
                r#"(?m)^type\s+([A-Z][a-zA-Z0-9_]*)\s+(?:struct|interface|func|map|chan|\[|=)"#,
            )
            .unwrap(),

            // Match: var Name =
            var_regex: Regex::new(r#"(?m)^var\s+([A-Z][a-zA-Z0-9_]*)\s+"#).unwrap(),
        }
    }

    /// Get line number for a byte offset in content.
    fn line_number(content: &str, byte_offset: usize) -> u32 {
        content[..byte_offset].matches('\n').count() as u32 + 1
    }

    /// Classify an import path.
    fn classify_import(path: &str) -> ImportKind {
        // Standard library packages don't contain dots (usually)
        // except for some like "net/http", "encoding/json"
        let stdlib_roots = [
            "archive",
            "bufio",
            "bytes",
            "compress",
            "container",
            "context",
            "crypto",
            "database",
            "debug",
            "embed",
            "encoding",
            "errors",
            "expvar",
            "flag",
            "fmt",
            "go",
            "hash",
            "html",
            "image",
            "index",
            "io",
            "log",
            "math",
            "mime",
            "net",
            "os",
            "path",
            "plugin",
            "reflect",
            "regexp",
            "runtime",
            "sort",
            "strconv",
            "strings",
            "sync",
            "syscall",
            "testing",
            "text",
            "time",
            "unicode",
            "unsafe",
        ];

        let root = path.split('/').next().unwrap_or(path);

        // Check if it's a standard library package
        if stdlib_roots.contains(&root) {
            return ImportKind::External;
        }

        // If it contains a dot, it's likely a module path (external dependency)
        if root.contains('.') {
            return ImportKind::External;
        }

        // Otherwise, could be a local package
        ImportKind::Relative
    }
}

impl Default for GoParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageParser for GoParser {
    fn extensions(&self) -> &[&str] {
        &["go"]
    }

    fn parse_imports(&self, content: &str) -> Vec<ImportRef> {
        let mut imports = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Parse single imports: import "fmt"
        for cap in self.import_single_regex.captures_iter(content) {
            if let Some(path_match) = cap.get(2) {
                let path = path_match.as_str().to_string();
                if seen.insert(path.clone()) {
                    let kind = Self::classify_import(&path);
                    let line = Self::line_number(content, path_match.start());
                    imports.push(ImportRef::new(path, kind, line));
                }
            }
        }

        // Parse import blocks: import ( ... )
        for cap in self.import_block_regex.captures_iter(content) {
            if let Some(block_match) = cap.get(1) {
                let block_start = cap.get(0).unwrap().start();
                let block_content = block_match.as_str();

                for line_cap in self.import_line_regex.captures_iter(block_content) {
                    if let Some(path_match) = line_cap.get(2) {
                        let path = path_match.as_str().to_string();
                        if seen.insert(path.clone()) {
                            let kind = Self::classify_import(&path);
                            // Calculate line number within the block
                            let offset_in_block = path_match.start();
                            let line = Self::line_number(content, block_start + offset_in_block);
                            imports.push(ImportRef::new(path, kind, line));
                        }
                    }
                }
            }
        }

        imports
    }

    fn parse_exports(&self, content: &str) -> Vec<ExportRef> {
        let mut exports = Vec::new();

        // Parse exported functions (capitalized names)
        for cap in self.func_regex.captures_iter(content) {
            if let Some(name_match) = cap.get(1) {
                let name = name_match.as_str();
                let line = Self::line_number(content, name_match.start());
                exports.push(ExportRef::new(name, ExportKind::Function, line));
            }
        }

        // Parse exported types
        for cap in self.type_regex.captures_iter(content) {
            if let Some(name_match) = cap.get(1) {
                let name = name_match.as_str();
                let line = Self::line_number(content, name_match.start());
                exports.push(ExportRef::new(name, ExportKind::Type, line));
            }
        }

        // Parse exported variables
        for cap in self.var_regex.captures_iter(content) {
            if let Some(name_match) = cap.get(1) {
                let name = name_match.as_str();
                let line = Self::line_number(content, name_match.start());
                exports.push(ExportRef::new(name, ExportKind::Constant, line));
            }
        }

        // Note: const parsing is more complex due to const blocks
        // We're keeping it simple for now and relying on the other patterns

        exports
    }

    fn resolve_import(
        &self,
        import: &ImportRef,
        _from_file: &Path,
        project_root: &Path,
    ) -> Option<PathBuf> {
        let path = &import.raw_path;

        // Skip standard library imports
        let stdlib_roots = [
            "archive",
            "bufio",
            "bytes",
            "compress",
            "container",
            "context",
            "crypto",
            "database",
            "debug",
            "embed",
            "encoding",
            "errors",
            "expvar",
            "flag",
            "fmt",
            "go",
            "hash",
            "html",
            "image",
            "index",
            "io",
            "log",
            "math",
            "mime",
            "net",
            "os",
            "path",
            "plugin",
            "reflect",
            "regexp",
            "runtime",
            "sort",
            "strconv",
            "strings",
            "sync",
            "syscall",
            "testing",
            "text",
            "time",
            "unicode",
            "unsafe",
        ];

        let root = path.split('/').next().unwrap_or(path);
        if stdlib_roots.contains(&root) {
            return None;
        }

        // Try to resolve as local package
        // In Go, packages are directories, so we look for a directory
        // that contains .go files

        // First, try as a direct path from project root
        let pkg_dir = project_root.join(path);
        if pkg_dir.is_dir() {
            // Find first .go file in directory (not _test.go)
            if let Ok(entries) = std::fs::read_dir(&pkg_dir) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if let Some(ext) = entry_path.extension() {
                        if ext == "go" {
                            let name = entry_path.file_name()?.to_str()?;
                            if !name.ends_with("_test.go") {
                                // Return path relative to project root
                                let relative = entry_path.strip_prefix(project_root).ok()?;
                                return Some(relative.to_path_buf());
                            }
                        }
                    }
                }
            }
        }

        // Try to extract module name from go.mod and resolve relative to it
        let go_mod_path = project_root.join("go.mod");
        if go_mod_path.exists() {
            if let Ok(go_mod_content) = std::fs::read_to_string(&go_mod_path) {
                // Extract module path: module github.com/user/project
                if let Some(module_line) = go_mod_content.lines().find(|l| l.starts_with("module "))
                {
                    let module_path = module_line.strip_prefix("module ")?.trim();

                    // Check if import starts with module path
                    if let Some(local_path) = path.strip_prefix(module_path) {
                        let local_path = local_path.trim_start_matches('/');
                        let pkg_dir = project_root.join(local_path);

                        if pkg_dir.is_dir() {
                            if let Ok(entries) = std::fs::read_dir(&pkg_dir) {
                                for entry in entries.flatten() {
                                    let entry_path = entry.path();
                                    if let Some(ext) = entry_path.extension() {
                                        if ext == "go" {
                                            let name = entry_path.file_name()?.to_str()?;
                                            if !name.ends_with("_test.go") {
                                                let relative =
                                                    entry_path.strip_prefix(project_root).ok()?;
                                                return Some(relative.to_path_buf());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
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
    fn test_parse_single_import() {
        let parser = GoParser::new();
        let content = r#"
package main

import "fmt"
import "os"
"#;

        let imports = parser.parse_imports(content);

        assert_eq!(imports.len(), 2);
        assert!(imports.iter().any(|i| i.raw_path == "fmt"));
        assert!(imports.iter().any(|i| i.raw_path == "os"));
    }

    #[test]
    fn test_parse_import_with_alias() {
        let parser = GoParser::new();
        let content = r#"
package main

import f "fmt"
import . "strings"
"#;

        let imports = parser.parse_imports(content);

        assert!(imports.iter().any(|i| i.raw_path == "fmt"));
        assert!(imports.iter().any(|i| i.raw_path == "strings"));
    }

    #[test]
    fn test_parse_import_block() {
        let parser = GoParser::new();
        let content = r#"
package main

import (
    "fmt"
    "os"
    "strings"
)
"#;

        let imports = parser.parse_imports(content);

        assert_eq!(imports.len(), 3);
        assert!(imports.iter().any(|i| i.raw_path == "fmt"));
        assert!(imports.iter().any(|i| i.raw_path == "os"));
        assert!(imports.iter().any(|i| i.raw_path == "strings"));
    }

    #[test]
    fn test_parse_import_block_with_aliases() {
        let parser = GoParser::new();
        let content = r#"
package main

import (
    f "fmt"
    "os"
    _ "net/http/pprof"
)
"#;

        let imports = parser.parse_imports(content);

        assert!(imports.iter().any(|i| i.raw_path == "fmt"));
        assert!(imports.iter().any(|i| i.raw_path == "os"));
        assert!(imports.iter().any(|i| i.raw_path == "net/http/pprof"));
    }

    #[test]
    fn test_parse_external_imports() {
        let parser = GoParser::new();
        let content = r#"
package main

import (
    "github.com/gin-gonic/gin"
    "golang.org/x/sync/errgroup"
)
"#;

        let imports = parser.parse_imports(content);

        assert!(imports
            .iter()
            .any(|i| i.raw_path == "github.com/gin-gonic/gin"));
        assert!(imports
            .iter()
            .any(|i| i.raw_path == "golang.org/x/sync/errgroup"));

        // Both should be classified as external
        for import in &imports {
            assert_eq!(import.kind, ImportKind::External);
        }
    }

    #[test]
    fn test_classify_imports() {
        // Standard library
        assert_eq!(GoParser::classify_import("fmt"), ImportKind::External);
        assert_eq!(GoParser::classify_import("net/http"), ImportKind::External);
        assert_eq!(
            GoParser::classify_import("encoding/json"),
            ImportKind::External
        );

        // External packages (contain dots)
        assert_eq!(
            GoParser::classify_import("github.com/user/pkg"),
            ImportKind::External
        );
        assert_eq!(
            GoParser::classify_import("golang.org/x/tools"),
            ImportKind::External
        );

        // Potentially local (no dots, not stdlib)
        assert_eq!(GoParser::classify_import("mypackage"), ImportKind::Relative);
        assert_eq!(
            GoParser::classify_import("internal/utils"),
            ImportKind::Relative
        );
    }

    #[test]
    fn test_parse_exported_functions() {
        let parser = GoParser::new();
        let content = r#"
package main

func PublicFunc() {}
func privateFunc() {}
func AnotherPublic(x int) string {}
"#;

        let exports = parser.parse_exports(content);

        assert!(exports
            .iter()
            .any(|e| e.name == "PublicFunc" && e.kind == ExportKind::Function));
        assert!(exports
            .iter()
            .any(|e| e.name == "AnotherPublic" && e.kind == ExportKind::Function));
        assert!(!exports.iter().any(|e| e.name == "privateFunc"));
    }

    #[test]
    fn test_parse_method_exports() {
        let parser = GoParser::new();
        let content = r#"
package main

func (s *Server) Start() error {}
func (s *Server) stop() {}
func (s Server) Handle(req Request) {}
"#;

        let exports = parser.parse_exports(content);

        assert!(exports.iter().any(|e| e.name == "Start"));
        assert!(exports.iter().any(|e| e.name == "Handle"));
        assert!(!exports.iter().any(|e| e.name == "stop"));
    }

    #[test]
    fn test_parse_exported_types() {
        let parser = GoParser::new();
        let content = r#"
package main

type Server struct {
    port int
}

type Handler interface {
    Handle()
}

type privateType struct {}

type Config = map[string]string
"#;

        let exports = parser.parse_exports(content);

        assert!(exports
            .iter()
            .any(|e| e.name == "Server" && e.kind == ExportKind::Type));
        assert!(exports
            .iter()
            .any(|e| e.name == "Handler" && e.kind == ExportKind::Type));
        assert!(exports
            .iter()
            .any(|e| e.name == "Config" && e.kind == ExportKind::Type));
        assert!(!exports.iter().any(|e| e.name == "privateType"));
    }

    #[test]
    fn test_parse_exported_vars() {
        let parser = GoParser::new();
        let content = r#"
package main

var GlobalConfig = Config{}
var privateVar = "secret"
"#;

        let exports = parser.parse_exports(content);

        assert!(exports.iter().any(|e| e.name == "GlobalConfig"));
        assert!(!exports.iter().any(|e| e.name == "privateVar"));
    }

    #[test]
    fn test_line_numbers() {
        let parser = GoParser::new();
        let content = r#"// line 1
package main // line 2
// line 3
import "fmt" // line 4
"#;

        let imports = parser.parse_imports(content);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].line, 4);
    }

    #[test]
    fn test_resolve_local_import() {
        let parser = GoParser::new();
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        // Create pkg/utils/utils.go
        std::fs::create_dir_all(project_root.join("pkg/utils")).unwrap();
        std::fs::write(project_root.join("pkg/utils/utils.go"), "package utils").unwrap();

        let import = ImportRef::new("pkg/utils", ImportKind::Relative, 1);
        let from_file = Path::new("cmd/main.go");

        let resolved = parser.resolve_import(&import, from_file, project_root);

        assert_eq!(resolved, Some(PathBuf::from("pkg/utils/utils.go")));
    }

    #[test]
    fn test_resolve_with_go_mod() {
        let parser = GoParser::new();
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        // Create go.mod
        std::fs::write(
            project_root.join("go.mod"),
            "module github.com/user/project\n\ngo 1.21\n",
        )
        .unwrap();

        // Create internal/utils/utils.go
        std::fs::create_dir_all(project_root.join("internal/utils")).unwrap();
        std::fs::write(
            project_root.join("internal/utils/utils.go"),
            "package utils",
        )
        .unwrap();

        let import = ImportRef::new(
            "github.com/user/project/internal/utils",
            ImportKind::External,
            1,
        );
        let from_file = Path::new("cmd/main.go");

        let resolved = parser.resolve_import(&import, from_file, project_root);

        assert_eq!(resolved, Some(PathBuf::from("internal/utils/utils.go")));
    }

    #[test]
    fn test_resolve_stdlib_returns_none() {
        let parser = GoParser::new();
        let temp = tempfile::TempDir::new().unwrap();

        let import = ImportRef::new("fmt", ImportKind::External, 1);
        let from_file = Path::new("main.go");

        let resolved = parser.resolve_import(&import, from_file, temp.path());

        assert!(resolved.is_none());
    }

    #[test]
    fn test_skip_test_files() {
        let parser = GoParser::new();
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path();

        // Create pkg/utils/ with only a test file and one regular file
        std::fs::create_dir_all(project_root.join("pkg/utils")).unwrap();
        std::fs::write(
            project_root.join("pkg/utils/utils_test.go"),
            "package utils",
        )
        .unwrap();
        std::fs::write(project_root.join("pkg/utils/utils.go"), "package utils").unwrap();

        let import = ImportRef::new("pkg/utils", ImportKind::Relative, 1);
        let from_file = Path::new("main.go");

        let resolved = parser.resolve_import(&import, from_file, project_root);

        // Should resolve to utils.go, not utils_test.go
        assert_eq!(resolved, Some(PathBuf::from("pkg/utils/utils.go")));
    }
}
