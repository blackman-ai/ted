// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! File tree generation and caching
//!
//! Generates a tree representation of the project directory structure
//! that can be included in context to help the LLM understand the codebase.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::Result;

/// Configuration for file tree generation
#[derive(Debug, Clone)]
pub struct FileTreeConfig {
    /// Maximum depth to traverse
    pub max_depth: usize,
    /// Maximum number of files to include
    pub max_files: usize,
    /// Directories to ignore
    pub ignore_dirs: HashSet<String>,
    /// File extensions to include (empty = all)
    pub include_extensions: HashSet<String>,
}

impl Default for FileTreeConfig {
    fn default() -> Self {
        let mut ignore_dirs = HashSet::new();
        // Common directories to ignore
        for dir in &[
            "target",
            "node_modules",
            ".git",
            "__pycache__",
            ".venv",
            "venv",
            "dist",
            "build",
            ".next",
            ".cache",
            "coverage",
            ".pytest_cache",
            ".mypy_cache",
            "vendor",
            "Pods",
        ] {
            ignore_dirs.insert((*dir).to_string());
        }

        Self {
            max_depth: 5,
            max_files: 500,
            ignore_dirs,
            include_extensions: HashSet::new(), // Include all by default
        }
    }
}

/// A cached file tree representation
#[derive(Debug, Clone)]
pub struct FileTree {
    /// Root directory of the tree
    root: PathBuf,
    /// Pre-rendered tree string
    tree_string: String,
    /// Total file count
    file_count: usize,
    /// Total directory count
    dir_count: usize,
    /// Whether the tree was truncated
    truncated: bool,
}

impl FileTree {
    /// Generate a new file tree from a directory
    pub fn generate(root: &Path, config: &FileTreeConfig) -> Result<Self> {
        let mut tree_string = String::new();
        let mut file_count = 0;
        let mut dir_count = 0;

        // Generate the tree
        let result = Self::build_tree(
            root,
            "",
            0,
            config,
            &mut tree_string,
            &mut file_count,
            &mut dir_count,
        );

        let truncated = result.is_err() || file_count >= config.max_files;

        if truncated {
            tree_string.push_str("\n... (truncated)\n");
        }

        Ok(Self {
            root: root.to_path_buf(),
            tree_string,
            file_count,
            dir_count,
            truncated,
        })
    }

    /// Build tree recursively
    #[allow(clippy::too_many_arguments)]
    fn build_tree(
        current: &Path,
        prefix: &str,
        depth: usize,
        config: &FileTreeConfig,
        output: &mut String,
        file_count: &mut usize,
        dir_count: &mut usize,
    ) -> Result<()> {
        if depth > config.max_depth || *file_count >= config.max_files {
            return Err(crate::error::TedError::Context("Tree truncated".into()));
        }

        // Read directory entries
        let mut entries: Vec<_> = std::fs::read_dir(current)?.filter_map(|e| e.ok()).collect();

        // Sort: directories first, then files, alphabetically
        entries.sort_by(|a, b| {
            let a_is_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let b_is_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);

            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.file_name().cmp(&b.file_name()),
            }
        });

        let total = entries.len();
        for (i, entry) in entries.into_iter().enumerate() {
            if *file_count >= config.max_files {
                return Err(crate::error::TedError::Context("Tree truncated".into()));
            }

            let is_last = i == total - 1;
            let connector = if is_last { "└── " } else { "├── " };
            let child_prefix = if is_last { "    " } else { "│   " };

            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let path = entry.path();
            let is_dir = path.is_dir();

            // Skip ignored directories
            if is_dir && config.ignore_dirs.contains(name_str.as_ref()) {
                continue;
            }

            // Filter by extension if configured
            if !is_dir && !config.include_extensions.is_empty() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if !config.include_extensions.contains(ext) {
                        continue;
                    }
                } else {
                    continue; // No extension, skip if filtering
                }
            }

            // Add to output
            output.push_str(prefix);
            output.push_str(connector);
            output.push_str(&name_str);

            if is_dir {
                output.push('/');
                *dir_count += 1;
            } else {
                *file_count += 1;
            }
            output.push('\n');

            // Recurse into directories
            if is_dir {
                let new_prefix = format!("{}{}", prefix, child_prefix);
                Self::build_tree(
                    &path,
                    &new_prefix,
                    depth + 1,
                    config,
                    output,
                    file_count,
                    dir_count,
                )?;
            }
        }

        Ok(())
    }

    /// Get the tree as a string
    pub fn as_string(&self) -> &str {
        &self.tree_string
    }

    /// Get the root directory name for display
    pub fn root_name(&self) -> String {
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string())
    }

    /// Get formatted output suitable for context
    pub fn to_context_string(&self) -> String {
        let mut result = format!("Project structure ({}):\n", self.root_name());
        result.push_str(&self.tree_string);
        if !self.truncated {
            result.push_str(&format!(
                "\n({} files, {} directories)\n",
                self.file_count, self.dir_count
            ));
        }
        result
    }

    /// Check if a path exists in the tree (for quick validation)
    pub fn file_count(&self) -> usize {
        self.file_count
    }

    /// Get directory count
    pub fn dir_count(&self) -> usize {
        self.dir_count
    }

    /// Check if tree was truncated
    pub fn is_truncated(&self) -> bool {
        self.truncated
    }

    /// Regenerate the tree (useful after file operations)
    pub fn refresh(&mut self, config: &FileTreeConfig) -> Result<()> {
        let new_tree = Self::generate(&self.root, config)?;
        *self = new_tree;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_structure() -> TempDir {
        let temp = TempDir::new().unwrap();

        // Create directories
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::create_dir_all(temp.path().join("src/utils")).unwrap();
        std::fs::create_dir_all(temp.path().join("tests")).unwrap();

        // Create files
        std::fs::write(temp.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(temp.path().join("README.md"), "# Test").unwrap();
        std::fs::write(temp.path().join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(temp.path().join("src/lib.rs"), "pub mod utils;").unwrap();
        std::fs::write(temp.path().join("src/utils/mod.rs"), "").unwrap();
        std::fs::write(temp.path().join("tests/test.rs"), "#[test]").unwrap();

        temp
    }

    #[test]
    fn test_generate_tree() {
        let temp = create_test_structure();
        let config = FileTreeConfig::default();

        let tree = FileTree::generate(temp.path(), &config).unwrap();

        assert!(tree.file_count() > 0);
        assert!(tree.dir_count() > 0);
        assert!(tree.as_string().contains("src/"));
        assert!(tree.as_string().contains("main.rs"));
    }

    #[test]
    fn test_ignore_directories() {
        let temp = TempDir::new().unwrap();

        // Create a node_modules directory
        std::fs::create_dir_all(temp.path().join("node_modules")).unwrap();
        std::fs::write(temp.path().join("node_modules/test.js"), "").unwrap();
        std::fs::write(temp.path().join("index.js"), "").unwrap();

        let config = FileTreeConfig::default();
        let tree = FileTree::generate(temp.path(), &config).unwrap();

        assert!(!tree.as_string().contains("node_modules"));
        assert!(tree.as_string().contains("index.js"));
    }

    #[test]
    fn test_max_depth() {
        let temp = TempDir::new().unwrap();

        // Create deeply nested structure
        let deep_path = temp.path().join("a/b/c/d/e/f/g/h");
        std::fs::create_dir_all(&deep_path).unwrap();
        std::fs::write(deep_path.join("deep.txt"), "").unwrap();

        let config = FileTreeConfig {
            max_depth: 3,
            ..Default::default()
        };

        let tree = FileTree::generate(temp.path(), &config).unwrap();

        // Should be truncated
        assert!(tree.is_truncated() || !tree.as_string().contains("deep.txt"));
    }

    #[test]
    fn test_max_files() {
        let temp = TempDir::new().unwrap();

        // Create many files
        for i in 0..20 {
            std::fs::write(temp.path().join(format!("file{}.txt", i)), "").unwrap();
        }

        let config = FileTreeConfig {
            max_files: 5,
            ..Default::default()
        };

        let tree = FileTree::generate(temp.path(), &config).unwrap();

        assert!(tree.is_truncated());
        assert!(tree.file_count() <= 5);
    }

    #[test]
    fn test_extension_filter() {
        let temp = TempDir::new().unwrap();

        std::fs::write(temp.path().join("main.rs"), "").unwrap();
        std::fs::write(temp.path().join("lib.rs"), "").unwrap();
        std::fs::write(temp.path().join("README.md"), "").unwrap();
        std::fs::write(temp.path().join("data.json"), "").unwrap();

        let config = FileTreeConfig {
            include_extensions: ["rs".to_string()].into_iter().collect(),
            ..Default::default()
        };

        let tree = FileTree::generate(temp.path(), &config).unwrap();

        assert!(tree.as_string().contains("main.rs"));
        assert!(tree.as_string().contains("lib.rs"));
        assert!(!tree.as_string().contains("README.md"));
        assert!(!tree.as_string().contains("data.json"));
    }

    #[test]
    fn test_to_context_string() {
        let temp = create_test_structure();
        let config = FileTreeConfig::default();

        let tree = FileTree::generate(temp.path(), &config).unwrap();
        let context = tree.to_context_string();

        assert!(context.starts_with("Project structure"));
        assert!(context.contains("files"));
        assert!(context.contains("directories"));
    }

    #[test]
    fn test_refresh() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("original.txt"), "").unwrap();

        let config = FileTreeConfig::default();
        let mut tree = FileTree::generate(temp.path(), &config).unwrap();

        assert!(tree.as_string().contains("original.txt"));
        assert!(!tree.as_string().contains("new_file.txt"));

        // Add a new file
        std::fs::write(temp.path().join("new_file.txt"), "").unwrap();

        // Refresh
        tree.refresh(&config).unwrap();

        assert!(tree.as_string().contains("new_file.txt"));
    }

    #[test]
    fn test_empty_directory() {
        let temp = TempDir::new().unwrap();
        let config = FileTreeConfig::default();

        let tree = FileTree::generate(temp.path(), &config).unwrap();

        assert_eq!(tree.file_count(), 0);
        assert_eq!(tree.dir_count(), 0);
    }

    #[test]
    fn test_sorting() {
        let temp = TempDir::new().unwrap();

        std::fs::create_dir_all(temp.path().join("zebra")).unwrap();
        std::fs::create_dir_all(temp.path().join("alpha")).unwrap();
        std::fs::write(temp.path().join("middle.txt"), "").unwrap();
        std::fs::write(temp.path().join("aardvark.txt"), "").unwrap();

        let config = FileTreeConfig::default();
        let tree = FileTree::generate(temp.path(), &config).unwrap();
        let output = tree.as_string();

        // Directories should come before files
        let alpha_pos = output.find("alpha/").unwrap();
        let zebra_pos = output.find("zebra/").unwrap();
        let aardvark_pos = output.find("aardvark.txt").unwrap();

        // Directories come first (alphabetically)
        assert!(alpha_pos < zebra_pos);
        // Then files
        assert!(zebra_pos < aardvark_pos);
    }

    #[test]
    fn test_file_tree_config_default() {
        let config = FileTreeConfig::default();

        assert_eq!(config.max_depth, 5);
        assert_eq!(config.max_files, 500);
        assert!(config.ignore_dirs.contains("node_modules"));
        assert!(config.ignore_dirs.contains(".git"));
        assert!(config.include_extensions.is_empty());
    }
}
