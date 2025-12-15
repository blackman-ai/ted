// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Cap loader
//!
//! Handles loading caps from filesystem and built-in sources.

use std::path::{Path, PathBuf};

use super::builtin;
use super::schema::Cap;
use crate::config::Settings;
use crate::error::{Result, TedError};

/// Loader for caps from various sources
#[derive(Clone)]
pub struct CapLoader {
    /// Project-local caps directory (./.ted/caps/)
    project_caps_dir: Option<PathBuf>,
    /// User-global caps directory (~/.ted/caps/)
    user_caps_dir: PathBuf,
}

impl CapLoader {
    /// Create a new cap loader
    pub fn new() -> Self {
        let user_caps_dir = Settings::caps_dir();

        // Try to find project-local .ted/caps/
        let project_caps_dir = std::env::current_dir()
            .ok()
            .map(|cwd| cwd.join(".ted").join("caps"))
            .filter(|p| p.exists());

        Self {
            project_caps_dir,
            user_caps_dir,
        }
    }

    /// Create with explicit paths (for testing)
    pub fn with_paths(project_dir: Option<PathBuf>, user_dir: PathBuf) -> Self {
        Self {
            project_caps_dir: project_dir,
            user_caps_dir: user_dir,
        }
    }

    /// Load a cap by name
    pub fn load(&self, name: &str) -> Result<Cap> {
        // 1. Check project-local caps
        if let Some(ref project_dir) = self.project_caps_dir {
            if let Some(cap) = self.load_from_dir(project_dir, name)? {
                return Ok(cap);
            }
        }

        // 2. Check user-global caps
        if let Some(cap) = self.load_from_dir(&self.user_caps_dir, name)? {
            return Ok(cap);
        }

        // 3. Check built-in caps
        if let Some(cap) = builtin::get_builtin(name) {
            return Ok(cap);
        }

        Err(TedError::Cap(format!("Cap not found: {}", name)))
    }

    /// Load a cap from a directory
    fn load_from_dir(&self, dir: &Path, name: &str) -> Result<Option<Cap>> {
        let toml_path = dir.join(format!("{}.toml", name));

        if !toml_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&toml_path)?;
        let mut cap: Cap = toml::from_str(&content)
            .map_err(|e| TedError::Cap(format!("Failed to parse cap '{}': {}", name, e)))?;

        cap.source_path = Some(toml_path);

        Ok(Some(cap))
    }

    /// List all available cap names with their builtin status
    /// Returns Vec<(name, is_builtin)>
    /// Note: "base" is excluded from this list as it is always applied silently
    pub fn list_available(&self) -> Result<Vec<(String, bool)>> {
        let mut caps: Vec<(String, bool)> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Built-in caps (excluding "base" which is always applied silently)
        for name in builtin::list_builtins() {
            if name == "base" {
                continue; // base is always applied, don't show in list
            }
            if !seen.contains(&name) {
                seen.insert(name.clone());
                caps.push((name, true));
            }
        }

        // User-global caps
        if self.user_caps_dir.exists() {
            for name in self.list_caps_in_dir(&self.user_caps_dir)? {
                if !seen.contains(&name) {
                    seen.insert(name.clone());
                    caps.push((name, false));
                }
            }
        }

        // Project-local caps (highest priority, not builtin)
        if let Some(ref project_dir) = self.project_caps_dir {
            for name in self.list_caps_in_dir(project_dir)? {
                if !seen.contains(&name) {
                    seen.insert(name.clone());
                    caps.push((name, false));
                }
            }
        }

        caps.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(caps)
    }

    /// List cap files in a directory
    fn list_caps_in_dir(&self, dir: &PathBuf) -> Result<Vec<String>> {
        let mut names = Vec::new();

        if !dir.exists() {
            return Ok(names);
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                    names.push(name.to_string());
                }
            }
        }

        Ok(names)
    }

    /// Check if a cap exists
    pub fn exists(&self, name: &str) -> bool {
        // Check project-local
        if let Some(ref project_dir) = self.project_caps_dir {
            if project_dir.join(format!("{}.toml", name)).exists() {
                return true;
            }
        }

        // Check user-global
        if self.user_caps_dir.join(format!("{}.toml", name)).exists() {
            return true;
        }

        // Check built-in
        builtin::get_builtin(name).is_some()
    }
}

impl Default for CapLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_cap_loader_new() {
        let loader = CapLoader::new();
        // Should at least have the user caps dir set
        assert!(!loader.user_caps_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_cap_loader_default() {
        let loader = CapLoader::default();
        assert!(!loader.user_caps_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_cap_loader_clone() {
        let loader = CapLoader::new();
        let cloned = loader.clone();
        assert_eq!(cloned.user_caps_dir, loader.user_caps_dir);
    }

    #[test]
    fn test_cap_loader_with_paths() {
        let project_dir = PathBuf::from("/project/caps");
        let user_dir = PathBuf::from("/user/caps");

        let loader = CapLoader::with_paths(Some(project_dir.clone()), user_dir.clone());
        assert_eq!(loader.project_caps_dir, Some(project_dir));
        assert_eq!(loader.user_caps_dir, user_dir);
    }

    #[test]
    fn test_cap_loader_with_paths_no_project() {
        let user_dir = PathBuf::from("/user/caps");
        let loader = CapLoader::with_paths(None, user_dir.clone());
        assert!(loader.project_caps_dir.is_none());
        assert_eq!(loader.user_caps_dir, user_dir);
    }

    #[test]
    fn test_load_builtin() {
        let loader = CapLoader::new();
        let cap = loader.load("base").unwrap();
        assert_eq!(cap.name, "base");
        assert!(cap.is_builtin);
    }

    #[test]
    fn test_load_builtin_rust_expert() {
        let loader = CapLoader::new();
        let cap = loader.load("rust-expert").unwrap();
        assert_eq!(cap.name, "rust-expert");
        assert!(cap.is_builtin);
    }

    #[test]
    fn test_load_nonexistent_cap() {
        let loader = CapLoader::new();
        let result = loader.load("nonexistent-cap-12345");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Cap not found"));
    }

    #[test]
    fn test_list_available() {
        let loader = CapLoader::new();
        let caps = loader.list_available().unwrap();
        // "base" is excluded from list (always applied silently)
        assert!(!caps.iter().any(|(name, _)| name == "base"));
        // But other builtins should be present
        assert!(caps.iter().any(|(name, _)| name == "rust-expert"));
    }

    #[test]
    fn test_list_available_contains_builtins() {
        let loader = CapLoader::new();
        let caps = loader.list_available().unwrap();

        // Should contain known builtins (except "base" which is hidden)
        let builtin_names: Vec<_> = caps
            .iter()
            .filter(|(_, is_builtin)| *is_builtin)
            .map(|(name, _)| name.as_str())
            .collect();

        // "base" is excluded from list (always applied silently)
        assert!(!builtin_names.contains(&"base"));
        assert!(builtin_names.contains(&"rust-expert"));
    }

    #[test]
    fn test_list_available_sorted() {
        let loader = CapLoader::new();
        let caps = loader.list_available().unwrap();

        // Check that caps are sorted alphabetically
        let names: Vec<_> = caps.iter().map(|(name, _)| name.clone()).collect();
        let mut sorted_names = names.clone();
        sorted_names.sort();
        assert_eq!(names, sorted_names);
    }

    #[test]
    fn test_load_from_file() {
        let dir = tempdir().unwrap();
        let cap_content = r#"
name = "test-cap"
description = "A test cap"
system_prompt = "You are helpful."
"#;

        std::fs::write(dir.path().join("test-cap.toml"), cap_content).unwrap();

        let loader = CapLoader::with_paths(Some(dir.path().to_path_buf()), PathBuf::new());

        let cap = loader.load("test-cap").unwrap();
        assert_eq!(cap.name, "test-cap");
        assert!(!cap.is_builtin);
    }

    #[test]
    fn test_load_from_file_sets_source_path() {
        let dir = tempdir().unwrap();
        let cap_content = r#"
name = "test-cap"
description = "A test cap"
"#;

        let cap_path = dir.path().join("test-cap.toml");
        std::fs::write(&cap_path, cap_content).unwrap();

        let loader = CapLoader::with_paths(Some(dir.path().to_path_buf()), PathBuf::new());
        let cap = loader.load("test-cap").unwrap();

        assert_eq!(cap.source_path, Some(cap_path));
    }

    #[test]
    fn test_load_project_caps_override_user_caps() {
        let project_dir = tempdir().unwrap();
        let user_dir = tempdir().unwrap();

        // Create same cap in both dirs with different content
        std::fs::write(
            project_dir.path().join("override-cap.toml"),
            "name = \"override-cap\"\ndescription = \"project version\"",
        )
        .unwrap();

        std::fs::write(
            user_dir.path().join("override-cap.toml"),
            "name = \"override-cap\"\ndescription = \"user version\"",
        )
        .unwrap();

        let loader = CapLoader::with_paths(
            Some(project_dir.path().to_path_buf()),
            user_dir.path().to_path_buf(),
        );

        let cap = loader.load("override-cap").unwrap();
        // Project cap should take precedence
        assert!(cap.source_path.unwrap().starts_with(project_dir.path()));
    }

    #[test]
    fn test_load_from_user_dir_when_no_project() {
        let user_dir = tempdir().unwrap();
        let cap_content = r#"
name = "user-cap"
description = "User cap"
"#;

        std::fs::write(user_dir.path().join("user-cap.toml"), cap_content).unwrap();

        let loader = CapLoader::with_paths(None, user_dir.path().to_path_buf());
        let cap = loader.load("user-cap").unwrap();

        assert_eq!(cap.name, "user-cap");
        assert!(cap.source_path.unwrap().starts_with(user_dir.path()));
    }

    #[test]
    fn test_load_invalid_toml() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("invalid-cap.toml"),
            "this is not valid toml {{{{",
        )
        .unwrap();

        let loader = CapLoader::with_paths(Some(dir.path().to_path_buf()), PathBuf::new());
        let result = loader.load("invalid-cap");

        assert!(result.is_err());
    }

    #[test]
    fn test_exists_builtin() {
        let loader = CapLoader::new();
        assert!(loader.exists("base"));
        assert!(loader.exists("rust-expert"));
    }

    #[test]
    fn test_exists_nonexistent() {
        let loader = CapLoader::new();
        assert!(!loader.exists("nonexistent-cap-12345"));
    }

    #[test]
    fn test_exists_user_cap() {
        let user_dir = tempdir().unwrap();
        std::fs::write(user_dir.path().join("my-cap.toml"), r#"name = "my-cap""#).unwrap();

        let loader = CapLoader::with_paths(None, user_dir.path().to_path_buf());
        assert!(loader.exists("my-cap"));
    }

    #[test]
    fn test_exists_project_cap() {
        let project_dir = tempdir().unwrap();
        std::fs::write(
            project_dir.path().join("project-cap.toml"),
            r#"name = "project-cap""#,
        )
        .unwrap();

        let loader = CapLoader::with_paths(Some(project_dir.path().to_path_buf()), PathBuf::new());
        assert!(loader.exists("project-cap"));
    }

    #[test]
    fn test_list_caps_in_empty_dir() {
        let dir = tempdir().unwrap();
        let loader = CapLoader::with_paths(Some(dir.path().to_path_buf()), PathBuf::new());

        let caps = loader.list_caps_in_dir(&dir.path().to_path_buf()).unwrap();
        assert!(caps.is_empty());
    }

    #[test]
    fn test_list_caps_in_nonexistent_dir() {
        let loader = CapLoader::new();
        let caps = loader
            .list_caps_in_dir(&PathBuf::from("/nonexistent/path"))
            .unwrap();
        assert!(caps.is_empty());
    }

    #[test]
    fn test_list_caps_ignores_non_toml() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("cap.toml"), r#"name = "cap""#).unwrap();
        std::fs::write(dir.path().join("readme.md"), "# Readme").unwrap();
        std::fs::write(dir.path().join("config.json"), "{}").unwrap();

        let loader = CapLoader::new();
        let caps = loader.list_caps_in_dir(&dir.path().to_path_buf()).unwrap();

        assert_eq!(caps.len(), 1);
        assert!(caps.contains(&"cap".to_string()));
    }

    #[test]
    fn test_list_available_includes_user_caps() {
        let user_dir = tempdir().unwrap();
        std::fs::write(
            user_dir.path().join("custom-user-cap.toml"),
            r#"name = "custom-user-cap""#,
        )
        .unwrap();

        let loader = CapLoader::with_paths(None, user_dir.path().to_path_buf());
        let caps = loader.list_available().unwrap();

        // "base" should NOT be in the list (always applied silently)
        assert!(!caps.iter().any(|(name, _)| name == "base"));
        // But other builtins should be there
        assert!(caps
            .iter()
            .any(|(name, is_builtin)| name == "rust-expert" && *is_builtin));
        // Should also include the user cap (marked as not builtin)
        assert!(caps
            .iter()
            .any(|(name, is_builtin)| name == "custom-user-cap" && !*is_builtin));
    }
}
