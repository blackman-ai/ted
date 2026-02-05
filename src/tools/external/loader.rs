// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Tool discovery and loading
//!
//! Discovers and loads external tools from `~/.ted/tools/`.

use std::path::{Path, PathBuf};

use crate::error::{Result, TedError};

use super::manifest::ToolManifest;

/// Default tools directory relative to home.
pub const TOOLS_DIR: &str = ".ted/tools";

/// Loader for external tools.
pub struct ToolLoader {
    /// Directory to search for tool manifests
    tools_dir: PathBuf,
}

impl ToolLoader {
    /// Create a loader for the default tools directory (~/.ted/tools/).
    pub fn new() -> Self {
        let tools_dir = dirs::home_dir()
            .map(|h| h.join(TOOLS_DIR))
            .unwrap_or_else(|| PathBuf::from(TOOLS_DIR));

        Self { tools_dir }
    }

    /// Create a loader for a custom directory.
    pub fn with_dir(tools_dir: PathBuf) -> Self {
        Self { tools_dir }
    }

    /// Get the tools directory path.
    pub fn tools_dir(&self) -> &Path {
        &self.tools_dir
    }

    /// Check if the tools directory exists.
    pub fn exists(&self) -> bool {
        self.tools_dir.is_dir()
    }

    /// Create the tools directory if it doesn't exist.
    pub fn ensure_dir(&self) -> Result<()> {
        if !self.tools_dir.exists() {
            std::fs::create_dir_all(&self.tools_dir).map_err(|e| {
                TedError::Config(format!(
                    "Failed to create tools directory {}: {}",
                    self.tools_dir.display(),
                    e
                ))
            })?;
        }
        Ok(())
    }

    /// Discover all tool manifests in the tools directory.
    ///
    /// Looks for `*.json` files in the tools directory.
    pub fn discover(&self) -> Vec<PathBuf> {
        if !self.exists() {
            return Vec::new();
        }

        let mut manifests = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.tools_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext == "json" {
                            manifests.push(path);
                        }
                    }
                }
            }
        }

        // Sort for deterministic ordering
        manifests.sort();
        manifests
    }

    /// Load all valid tool manifests.
    ///
    /// Invalid manifests are logged and skipped.
    pub fn load_all(&self) -> Vec<ToolManifest> {
        let manifest_paths = self.discover();
        let mut manifests = Vec::new();

        for path in manifest_paths {
            match ToolManifest::from_file(&path) {
                Ok(manifest) => {
                    manifests.push(manifest);
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to load tool manifest {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }

        manifests
    }

    /// Load a specific tool by name.
    pub fn load_by_name(&self, name: &str) -> Result<ToolManifest> {
        let manifest_path = self.tools_dir.join(format!("{}.json", name));

        if !manifest_path.exists() {
            return Err(TedError::ToolExecution(format!(
                "Tool manifest not found: {}",
                manifest_path.display()
            )));
        }

        ToolManifest::from_file(&manifest_path)
    }

    /// Get names of all available tools.
    pub fn available_tools(&self) -> Vec<String> {
        self.load_all().into_iter().map(|m| m.name).collect()
    }
}

impl Default for ToolLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_manifest(dir: &Path, name: &str) {
        let manifest = format!(
            r#"{{
                "name": "{}",
                "description": "Test tool {}",
                "command": ["echo", "{}"],
                "input_schema": {{"type": "object", "properties": {{}}}}
            }}"#,
            name, name, name
        );
        std::fs::write(dir.join(format!("{}.json", name)), manifest).unwrap();
    }

    #[test]
    fn test_loader_new() {
        let loader = ToolLoader::new();
        assert!(loader.tools_dir().to_string_lossy().contains(TOOLS_DIR));
    }

    #[test]
    fn test_loader_with_dir() {
        let custom_dir = PathBuf::from("/custom/tools");
        let loader = ToolLoader::with_dir(custom_dir.clone());
        assert_eq!(loader.tools_dir(), custom_dir);
    }

    #[test]
    fn test_loader_default() {
        let loader = ToolLoader::default();
        assert!(loader.tools_dir().to_string_lossy().contains(TOOLS_DIR));
    }

    #[test]
    fn test_loader_exists_no_dir() {
        let loader = ToolLoader::with_dir(PathBuf::from("/nonexistent/path"));
        assert!(!loader.exists());
    }

    #[test]
    fn test_loader_exists_with_dir() {
        let temp_dir = TempDir::new().unwrap();
        let loader = ToolLoader::with_dir(temp_dir.path().to_path_buf());
        assert!(loader.exists());
    }

    #[test]
    fn test_loader_ensure_dir() {
        let temp_dir = TempDir::new().unwrap();
        let tools_dir = temp_dir.path().join("tools");

        let loader = ToolLoader::with_dir(tools_dir.clone());
        assert!(!loader.exists());

        loader.ensure_dir().unwrap();
        assert!(loader.exists());
    }

    #[test]
    fn test_loader_discover_empty() {
        let temp_dir = TempDir::new().unwrap();
        let loader = ToolLoader::with_dir(temp_dir.path().to_path_buf());

        let manifests = loader.discover();
        assert!(manifests.is_empty());
    }

    #[test]
    fn test_loader_discover_with_manifests() {
        let temp_dir = TempDir::new().unwrap();

        // Create some manifest files
        create_test_manifest(temp_dir.path(), "tool_a");
        create_test_manifest(temp_dir.path(), "tool_b");

        // Create a non-json file (should be ignored)
        std::fs::write(temp_dir.path().join("readme.txt"), "not a manifest").unwrap();

        let loader = ToolLoader::with_dir(temp_dir.path().to_path_buf());
        let manifests = loader.discover();

        assert_eq!(manifests.len(), 2);
        // Should be sorted
        assert!(manifests[0].to_string_lossy().contains("tool_a"));
        assert!(manifests[1].to_string_lossy().contains("tool_b"));
    }

    #[test]
    fn test_loader_discover_nonexistent_dir() {
        let loader = ToolLoader::with_dir(PathBuf::from("/nonexistent"));
        let manifests = loader.discover();
        assert!(manifests.is_empty());
    }

    #[test]
    fn test_loader_load_all() {
        let temp_dir = TempDir::new().unwrap();

        create_test_manifest(temp_dir.path(), "tool_a");
        create_test_manifest(temp_dir.path(), "tool_b");

        let loader = ToolLoader::with_dir(temp_dir.path().to_path_buf());
        let manifests = loader.load_all();

        assert_eq!(manifests.len(), 2);
        assert!(manifests.iter().any(|m| m.name == "tool_a"));
        assert!(manifests.iter().any(|m| m.name == "tool_b"));
    }

    #[test]
    fn test_loader_load_all_skips_invalid() {
        let temp_dir = TempDir::new().unwrap();

        create_test_manifest(temp_dir.path(), "valid_tool");

        // Create an invalid manifest
        std::fs::write(temp_dir.path().join("invalid.json"), "{ not valid json").unwrap();

        let loader = ToolLoader::with_dir(temp_dir.path().to_path_buf());
        let manifests = loader.load_all();

        // Should only load the valid one
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].name, "valid_tool");
    }

    #[test]
    fn test_loader_load_by_name() {
        let temp_dir = TempDir::new().unwrap();
        create_test_manifest(temp_dir.path(), "my_tool");

        let loader = ToolLoader::with_dir(temp_dir.path().to_path_buf());
        let manifest = loader.load_by_name("my_tool").unwrap();

        assert_eq!(manifest.name, "my_tool");
    }

    #[test]
    fn test_loader_load_by_name_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let loader = ToolLoader::with_dir(temp_dir.path().to_path_buf());

        let result = loader.load_by_name("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_loader_available_tools() {
        let temp_dir = TempDir::new().unwrap();

        create_test_manifest(temp_dir.path(), "alpha");
        create_test_manifest(temp_dir.path(), "beta");

        let loader = ToolLoader::with_dir(temp_dir.path().to_path_buf());
        let tools = loader.available_tools();

        assert_eq!(tools.len(), 2);
        assert!(tools.contains(&"alpha".to_string()));
        assert!(tools.contains(&"beta".to_string()));
    }

    #[test]
    fn test_loader_available_tools_empty() {
        let temp_dir = TempDir::new().unwrap();
        let loader = ToolLoader::with_dir(temp_dir.path().to_path_buf());

        let tools = loader.available_tools();
        assert!(tools.is_empty());
    }

    #[test]
    fn test_loader_ignores_subdirectories() {
        let temp_dir = TempDir::new().unwrap();

        create_test_manifest(temp_dir.path(), "tool");

        // Create a subdirectory with a json file
        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();
        std::fs::write(subdir.join("nested.json"), "{}").unwrap();

        let loader = ToolLoader::with_dir(temp_dir.path().to_path_buf());
        let manifests = loader.discover();

        // Should only find the top-level manifest
        assert_eq!(manifests.len(), 1);
    }

    #[test]
    fn test_loader_ensure_dir_error() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file that will block directory creation
        let file_path = temp_dir.path().join("blocker");
        std::fs::write(&file_path, "I'm a file").unwrap();

        // Try to create a directory inside the file (should fail)
        let impossible_dir = file_path.join("subdir");
        let loader = ToolLoader::with_dir(impossible_dir);

        // ensure_dir should fail because we can't create a directory inside a file
        let result = loader.ensure_dir();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to create tools directory"));
    }

    #[test]
    fn test_loader_ensure_dir_already_exists() {
        let temp_dir = TempDir::new().unwrap();
        let loader = ToolLoader::with_dir(temp_dir.path().to_path_buf());

        // Directory already exists, ensure_dir should succeed
        let result = loader.ensure_dir();
        assert!(result.is_ok());
    }

    #[test]
    fn test_loader_discover_with_files_without_extension() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file without extension
        std::fs::write(temp_dir.path().join("no_extension"), "content").unwrap();

        // Create valid manifest
        create_test_manifest(temp_dir.path(), "valid");

        let loader = ToolLoader::with_dir(temp_dir.path().to_path_buf());
        let manifests = loader.discover();

        // Should only find the .json file
        assert_eq!(manifests.len(), 1);
    }
}
