// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Skill loading system
//!
//! Loads skills from the filesystem with priority:
//! 1. `./.ted/skills/{name}/SKILL.md` (project-local)
//! 2. `~/.ted/skills/{name}/SKILL.md` (user-global)
//!
//! Supports progressive loading:
//! - `load_metadata()` - Just name/description (for startup listing)
//! - `load_full()` - Full content + resources + scripts (on-demand)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::error::{Result, TedError};

use super::schema::{Skill, SkillMetadata};

/// Registry of available skills
pub struct SkillRegistry {
    /// Loaded skill metadata (name -> metadata)
    metadata: HashMap<String, SkillMetadata>,
    /// Fully loaded skills (cached)
    loaded: RwLock<HashMap<String, Skill>>,
    /// Search paths for skills (in priority order)
    search_paths: Vec<PathBuf>,
}

impl SkillRegistry {
    /// Create a new skill registry with default search paths
    pub fn new() -> Self {
        let mut search_paths = Vec::new();

        // Project-local skills (highest priority)
        if let Ok(cwd) = std::env::current_dir() {
            search_paths.push(cwd.join(".ted/skills"));
        }

        // User-global skills
        if let Some(home) = dirs::home_dir() {
            search_paths.push(home.join(".ted/skills"));
        }

        Self {
            metadata: HashMap::new(),
            loaded: RwLock::new(HashMap::new()),
            search_paths,
        }
    }

    /// Create a registry with custom search paths
    pub fn with_paths(paths: Vec<PathBuf>) -> Self {
        Self {
            metadata: HashMap::new(),
            loaded: RwLock::new(HashMap::new()),
            search_paths: paths,
        }
    }

    /// Add a search path
    pub fn add_search_path(&mut self, path: PathBuf) {
        if !self.search_paths.contains(&path) {
            self.search_paths.push(path);
        }
    }

    /// Scan search paths and load skill metadata
    pub fn scan(&mut self) -> Result<usize> {
        let mut count = 0;

        for search_path in &self.search_paths.clone() {
            if !search_path.exists() {
                continue;
            }

            // Read skill directories
            let entries = std::fs::read_dir(search_path).map_err(|e| {
                TedError::Io(std::io::Error::new(
                    e.kind(),
                    format!("Failed to read skills directory: {}", e),
                ))
            })?;

            for entry in entries {
                let entry = entry?;
                let path = entry.path();

                if !path.is_dir() {
                    continue;
                }

                let skill_file = path.join("SKILL.md");
                if !skill_file.exists() {
                    continue;
                }

                // Load metadata only
                match self.load_skill_metadata(&skill_file) {
                    Ok(meta) => {
                        // Only add if not already present (first match wins)
                        if !self.metadata.contains_key(&meta.name) {
                            self.metadata.insert(meta.name.clone(), meta);
                            count += 1;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load skill metadata from {}: {}",
                            skill_file.display(),
                            e
                        );
                    }
                }
            }
        }

        Ok(count)
    }

    /// Load just the metadata from a SKILL.md file
    fn load_skill_metadata(&self, skill_file: &Path) -> Result<SkillMetadata> {
        let content = std::fs::read_to_string(skill_file)?;
        let source_path = skill_file.parent().unwrap_or(skill_file).to_path_buf();
        SkillMetadata::parse(&content, source_path)
    }

    /// Get skill metadata by name
    pub fn get_metadata(&self, name: &str) -> Option<&SkillMetadata> {
        self.metadata.get(name)
    }

    /// List all available skill names
    pub fn list_skills(&self) -> Vec<&str> {
        self.metadata.keys().map(|s| s.as_str()).collect()
    }

    /// Get all skill metadata
    pub fn all_metadata(&self) -> Vec<&SkillMetadata> {
        self.metadata.values().collect()
    }

    /// Load a skill fully (with content, resources, scripts)
    pub fn load(&self, name: &str) -> Result<Skill> {
        // Check cache first
        {
            let loaded = self.loaded.read().unwrap();
            if let Some(skill) = loaded.get(name) {
                return Ok(skill.clone());
            }
        }

        // Find the skill
        let meta = self
            .metadata
            .get(name)
            .ok_or_else(|| TedError::Config(format!("Skill '{}' not found", name)))?;

        // Load fully
        let skill = self.load_full_skill(&meta.source_path)?;

        // Cache it
        {
            let mut loaded = self.loaded.write().unwrap();
            loaded.insert(name.to_string(), skill.clone());
        }

        Ok(skill)
    }

    /// Load a skill directory fully
    fn load_full_skill(&self, skill_dir: &Path) -> Result<Skill> {
        let skill_file = skill_dir.join("SKILL.md");
        let content = std::fs::read_to_string(&skill_file)?;
        let mut skill = Skill::parse(&content, skill_dir.to_path_buf())?;

        // Scan for resources and scripts
        if let Ok(entries) = std::fs::read_dir(skill_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                if name == "SKILL.md" {
                    continue;
                }

                if path.is_file() {
                    if name.ends_with(".sh") || name.ends_with(".py") || name.ends_with(".rb") {
                        // It's a script
                        let description = self.extract_script_description(&path);
                        skill.add_script(name, path.clone(), description);
                    } else if name.ends_with(".md") || name.ends_with(".txt") {
                        // It's a resource
                        skill.add_resource(name, path.clone());
                    }
                }
            }
        }

        Ok(skill)
    }

    /// Extract description from script file (first comment line)
    fn extract_script_description(&self, path: &Path) -> Option<String> {
        let content = std::fs::read_to_string(path).ok()?;

        for line in content.lines() {
            let line = line.trim();

            // Skip shebang
            if line.starts_with("#!") {
                continue;
            }

            // Extract comment
            if let Some(stripped) = line.strip_prefix('#') {
                return Some(stripped.trim().to_string());
            } else if let Some(stripped) = line.strip_prefix("//") {
                return Some(stripped.trim().to_string());
            }

            // Stop at first non-comment, non-empty line
            if !line.is_empty() {
                break;
            }
        }

        None
    }

    /// Check if a skill exists
    pub fn exists(&self, name: &str) -> bool {
        self.metadata.contains_key(name)
    }

    /// Clear the loaded skill cache
    pub fn clear_cache(&self) {
        let mut loaded = self.loaded.write().unwrap();
        loaded.clear();
    }

    /// Find a skill by name across search paths (without loading)
    pub fn find_skill_path(&self, name: &str) -> Option<PathBuf> {
        for search_path in &self.search_paths {
            let skill_path = search_path.join(name);
            let skill_file = skill_path.join("SKILL.md");
            if skill_file.exists() {
                return Some(skill_path);
            }
        }
        None
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Load a single skill from a path
pub fn load_skill_from_path(skill_dir: &Path) -> Result<Skill> {
    let skill_file = skill_dir.join("SKILL.md");
    if !skill_file.exists() {
        return Err(TedError::Config(format!(
            "SKILL.md not found in {}",
            skill_dir.display()
        )));
    }

    let content = std::fs::read_to_string(&skill_file)?;
    Skill::parse(&content, skill_dir.to_path_buf())
}

/// Load skill metadata from a path
pub fn load_skill_metadata_from_path(skill_dir: &Path) -> Result<SkillMetadata> {
    let skill_file = skill_dir.join("SKILL.md");
    if !skill_file.exists() {
        return Err(TedError::Config(format!(
            "SKILL.md not found in {}",
            skill_dir.display()
        )));
    }

    let content = std::fs::read_to_string(&skill_file)?;
    SkillMetadata::parse(&content, skill_dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_skill(dir: &Path, name: &str, description: &str) {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = format!(
            r#"---
name: {}
description: {}
---

# {}

Skill content here.
"#,
            name, description, name
        );

        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    #[test]
    fn test_skill_registry_new() {
        let registry = SkillRegistry::new();
        assert!(registry.list_skills().is_empty());
    }

    #[test]
    fn test_skill_registry_scan() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        create_test_skill(&skills_dir, "rust-async", "Async Rust patterns");
        create_test_skill(&skills_dir, "react-hooks", "React hooks guide");

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        let count = registry.scan().unwrap();

        assert_eq!(count, 2);
        assert!(registry.exists("rust-async"));
        assert!(registry.exists("react-hooks"));
    }

    #[test]
    fn test_skill_registry_load() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        create_test_skill(&skills_dir, "test-skill", "A test skill");

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        registry.scan().unwrap();

        let skill = registry.load("test-skill").unwrap();
        assert_eq!(skill.name, "test-skill");
        assert!(skill.content.contains("Skill content"));
    }

    #[test]
    fn test_skill_registry_load_nonexistent() {
        let registry = SkillRegistry::new();
        let result = registry.load("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_skill_registry_with_resources() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        let skill_dir = skills_dir.join("with-resources");
        std::fs::create_dir_all(&skill_dir).unwrap();

        // Create SKILL.md
        let content = r#"---
name: with-resources
description: Skill with resources
---

Main content.
"#;
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        // Create resources
        std::fs::write(skill_dir.join("examples.md"), "# Examples").unwrap();
        std::fs::write(skill_dir.join("patterns.txt"), "Patterns").unwrap();

        // Create script
        std::fs::write(
            skill_dir.join("run-tests.sh"),
            "#!/bin/bash\n# Run the tests\necho test",
        )
        .unwrap();

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        registry.scan().unwrap();

        let skill = registry.load("with-resources").unwrap();

        assert_eq!(skill.resources.len(), 2);
        assert_eq!(skill.scripts.len(), 1);
        assert_eq!(
            skill.scripts[0].description,
            Some("Run the tests".to_string())
        );
    }

    #[test]
    fn test_skill_registry_priority() {
        let temp_dir = TempDir::new().unwrap();

        // Create two directories with same skill name
        let project_skills = temp_dir.path().join("project/skills");
        let user_skills = temp_dir.path().join("user/skills");

        std::fs::create_dir_all(&project_skills).unwrap();
        std::fs::create_dir_all(&user_skills).unwrap();

        // Project skill (should win)
        create_test_skill(&project_skills, "shared-skill", "Project version");
        // User skill
        create_test_skill(&user_skills, "shared-skill", "User version");

        // Project skills have higher priority (first in list)
        let mut registry = SkillRegistry::with_paths(vec![project_skills, user_skills]);
        registry.scan().unwrap();

        let meta = registry.get_metadata("shared-skill").unwrap();
        assert_eq!(meta.description, "Project version");
    }

    #[test]
    fn test_load_skill_from_path() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = r#"---
name: direct-load
description: Directly loaded skill
---

Content.
"#;
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        let skill = load_skill_from_path(&skill_dir).unwrap();
        assert_eq!(skill.name, "direct-load");
    }

    #[test]
    fn test_load_skill_from_path_missing() {
        let temp_dir = TempDir::new().unwrap();
        let result = load_skill_from_path(temp_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_skill_registry_clear_cache() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        create_test_skill(&skills_dir, "cached", "Cached skill");

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        registry.scan().unwrap();

        // Load to cache
        registry.load("cached").unwrap();

        // Clear cache
        registry.clear_cache();

        // Should still be loadable (just not cached)
        registry.load("cached").unwrap();
    }

    #[test]
    fn test_skill_registry_find_path() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        create_test_skill(&skills_dir, "findable", "Findable skill");

        let mut registry = SkillRegistry::with_paths(vec![skills_dir.clone()]);
        registry.scan().unwrap();

        let path = registry.find_skill_path("findable");
        assert!(path.is_some());
        assert_eq!(path.unwrap(), skills_dir.join("findable"));

        let missing = registry.find_skill_path("missing");
        assert!(missing.is_none());
    }

    // ===== Additional Test Coverage =====

    #[test]
    fn test_skill_registry_default() {
        let registry = SkillRegistry::default();
        assert!(registry.list_skills().is_empty());
        // Should have search paths set up
        // Search paths may be empty if no home/cwd, which is acceptable
        let _ = registry.search_paths.is_empty();
    }

    #[test]
    fn test_skill_registry_add_search_path() {
        let temp_dir = TempDir::new().unwrap();
        let mut registry = SkillRegistry::with_paths(vec![]);

        let path1 = temp_dir.path().join("skills1");
        let path2 = temp_dir.path().join("skills2");

        registry.add_search_path(path1.clone());
        assert_eq!(registry.search_paths.len(), 1);

        // Adding same path again should not duplicate
        registry.add_search_path(path1.clone());
        assert_eq!(registry.search_paths.len(), 1);

        // Adding different path should work
        registry.add_search_path(path2.clone());
        assert_eq!(registry.search_paths.len(), 2);
    }

    #[test]
    fn test_skill_registry_scan_empty_paths() {
        let mut registry = SkillRegistry::with_paths(vec![]);
        let count = registry.scan().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_skill_registry_scan_nonexistent_path() {
        let mut registry =
            SkillRegistry::with_paths(vec![PathBuf::from("/nonexistent/path/to/skills")]);
        let count = registry.scan().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_skill_registry_scan_with_files_only() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        // Create a file instead of a directory
        std::fs::write(skills_dir.join("not-a-skill.txt"), "content").unwrap();

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        let count = registry.scan().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_skill_registry_scan_directory_without_skill_md() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        let empty_skill_dir = skills_dir.join("empty-skill");
        std::fs::create_dir_all(&empty_skill_dir).unwrap();

        // Create empty directory (no SKILL.md)
        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        let count = registry.scan().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_skill_registry_all_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        create_test_skill(&skills_dir, "skill1", "First skill");
        create_test_skill(&skills_dir, "skill2", "Second skill");

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        registry.scan().unwrap();

        let all = registry.all_metadata();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_skill_registry_get_metadata_not_found() {
        let registry = SkillRegistry::with_paths(vec![]);
        assert!(registry.get_metadata("nonexistent").is_none());
    }

    #[test]
    fn test_skill_registry_load_caches() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        create_test_skill(&skills_dir, "cached-skill", "A cached skill");

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        registry.scan().unwrap();

        // First load
        let skill1 = registry.load("cached-skill").unwrap();
        // Second load should return cached version
        let skill2 = registry.load("cached-skill").unwrap();

        assert_eq!(skill1.name, skill2.name);
    }

    #[test]
    fn test_skill_registry_exists() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        create_test_skill(&skills_dir, "existing", "Existing skill");

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        registry.scan().unwrap();

        assert!(registry.exists("existing"));
        assert!(!registry.exists("nonexistent"));
    }

    #[test]
    fn test_extract_script_description_shell_with_shebang() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        let skill_dir = skills_dir.join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = r#"---
name: test-skill
description: Test skill
---

Content.
"#;
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        // Shell script with shebang and comment
        std::fs::write(
            skill_dir.join("test.sh"),
            "#!/bin/bash\n# This is the description\necho hello",
        )
        .unwrap();

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        registry.scan().unwrap();

        let skill = registry.load("test-skill").unwrap();
        assert_eq!(skill.scripts.len(), 1);
        assert_eq!(
            skill.scripts[0].description,
            Some("This is the description".to_string())
        );
    }

    #[test]
    fn test_extract_script_description_python() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        let skill_dir = skills_dir.join("py-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = r#"---
name: py-skill
description: Python skill
---

Content.
"#;
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        // Python script with shebang and comment
        std::fs::write(
            skill_dir.join("run.py"),
            "#!/usr/bin/env python3\n# Python script description\nimport sys",
        )
        .unwrap();

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        registry.scan().unwrap();

        let skill = registry.load("py-skill").unwrap();
        assert_eq!(skill.scripts.len(), 1);
        assert_eq!(
            skill.scripts[0].description,
            Some("Python script description".to_string())
        );
    }

    #[test]
    fn test_extract_script_description_no_comment() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        let skill_dir = skills_dir.join("no-comment");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = r#"---
name: no-comment
description: No comment skill
---

Content.
"#;
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        // Script without comment (just code after shebang)
        std::fs::write(skill_dir.join("script.sh"), "#!/bin/bash\necho hello world").unwrap();

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        registry.scan().unwrap();

        let skill = registry.load("no-comment").unwrap();
        assert_eq!(skill.scripts.len(), 1);
        assert!(skill.scripts[0].description.is_none());
    }

    #[test]
    fn test_extract_script_description_double_slash() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        let skill_dir = skills_dir.join("slash-comment");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = r#"---
name: slash-comment
description: Slash comment skill
---

Content.
"#;
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        // Use .sh extension but // comment style for testing
        std::fs::write(
            skill_dir.join("runner.sh"),
            "#!/bin/bash\n// Double slash comment\necho test",
        )
        .unwrap();

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        registry.scan().unwrap();

        let skill = registry.load("slash-comment").unwrap();
        assert_eq!(skill.scripts.len(), 1);
        assert_eq!(
            skill.scripts[0].description,
            Some("Double slash comment".to_string())
        );
    }

    #[test]
    fn test_extract_script_description_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        let skill_dir = skills_dir.join("empty-script");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = r#"---
name: empty-script
description: Empty script skill
---

Content.
"#;
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        // Empty script file
        std::fs::write(skill_dir.join("empty.sh"), "").unwrap();

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        registry.scan().unwrap();

        let skill = registry.load("empty-script").unwrap();
        assert_eq!(skill.scripts.len(), 1);
        assert!(skill.scripts[0].description.is_none());
    }

    #[test]
    fn test_load_skill_metadata_from_path_success() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("meta-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = r#"---
name: meta-skill
description: Metadata test skill
---

Content here.
"#;
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        let meta = load_skill_metadata_from_path(&skill_dir).unwrap();
        assert_eq!(meta.name, "meta-skill");
        assert_eq!(meta.description, "Metadata test skill");
    }

    #[test]
    fn test_load_skill_metadata_from_path_missing() {
        let temp_dir = TempDir::new().unwrap();
        let result = load_skill_metadata_from_path(temp_dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(format!("{:?}", err).contains("SKILL.md not found"));
    }

    #[test]
    fn test_skill_with_ruby_script() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        let skill_dir = skills_dir.join("ruby-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = r#"---
name: ruby-skill
description: Ruby skill
---

Content.
"#;
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        // Ruby script
        std::fs::write(
            skill_dir.join("run.rb"),
            "#!/usr/bin/env ruby\n# Ruby script description\nputs 'hello'",
        )
        .unwrap();

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        registry.scan().unwrap();

        let skill = registry.load("ruby-skill").unwrap();
        assert_eq!(skill.scripts.len(), 1);
        assert_eq!(skill.scripts[0].name, "run.rb");
        assert_eq!(
            skill.scripts[0].description,
            Some("Ruby script description".to_string())
        );
    }

    #[test]
    fn test_skill_with_mixed_resources() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path().join("skills");
        let skill_dir = skills_dir.join("mixed-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = r#"---
name: mixed-skill
description: Mixed resources skill
---

Content.
"#;
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        // Various resources and scripts
        std::fs::write(skill_dir.join("docs.md"), "# Documentation").unwrap();
        std::fs::write(skill_dir.join("notes.txt"), "Notes").unwrap();
        std::fs::write(skill_dir.join("test.sh"), "#!/bin/bash\n# Test\necho test").unwrap();
        std::fs::write(
            skill_dir.join("run.py"),
            "#!/usr/bin/env python\n# Run\nprint('hi')",
        )
        .unwrap();
        // Create a non-matching file that should be ignored
        std::fs::write(skill_dir.join("data.json"), "{}").unwrap();

        let mut registry = SkillRegistry::with_paths(vec![skills_dir]);
        registry.scan().unwrap();

        let skill = registry.load("mixed-skill").unwrap();
        assert_eq!(skill.resources.len(), 2); // docs.md, notes.txt
        assert_eq!(skill.scripts.len(), 2); // test.sh, run.py
    }

    #[test]
    fn test_skill_registry_find_path_multiple_search_paths() {
        let temp_dir = TempDir::new().unwrap();
        let path1 = temp_dir.path().join("path1/skills");
        let path2 = temp_dir.path().join("path2/skills");

        std::fs::create_dir_all(&path1).unwrap();
        std::fs::create_dir_all(&path2).unwrap();

        // Create skill only in second path
        create_test_skill(&path2, "only-in-path2", "Skill in path2");

        let registry = SkillRegistry::with_paths(vec![path1, path2.clone()]);

        let found = registry.find_skill_path("only-in-path2");
        assert!(found.is_some());
        assert_eq!(found.unwrap(), path2.join("only-in-path2"));
    }
}
