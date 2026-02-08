// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Project context file loading
//!
//! Discovers and loads project-specific context files from industry-standard conventions:
//! - CLAUDE.md / AGENTS.md (Claude Code / OpenAI Codex conventions)
//! - .cursorrules (legacy Cursor format)
//! - .cursor/rules/*.mdc (Cursor rule files with frontmatter)
//!
//! Files are loaded in priority order and concatenated into the system prompt.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::Result;

/// Configuration for project context discovery and loading
#[derive(Debug, Clone)]
pub struct ProjectContextConfig {
    /// Whether to search for Claude/Codex files (CLAUDE.md, AGENTS.md)
    pub enable_claude_files: bool,
    /// Whether to search for Cursor rules (.cursorrules, .cursor/rules/*.mdc)
    pub enable_cursor_rules: bool,
    /// Maximum total size for all context files (in bytes)
    pub max_total_size: usize,
    /// Maximum depth to search for subdirectory CLAUDE.md/AGENTS.md files
    pub subdirectory_depth: usize,
}

impl Default for ProjectContextConfig {
    fn default() -> Self {
        Self {
            enable_claude_files: true,
            enable_cursor_rules: true,
            max_total_size: 100_000, // 100KB
            subdirectory_depth: 3,
        }
    }
}

/// Source/type of a context file
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextFileSource {
    /// Global user config (~/.claude/ or ~/.codex/)
    GlobalUser,
    /// Project root level (CLAUDE.md or AGENTS.md)
    ProjectRoot,
    /// Local override (CLAUDE.local.md or AGENTS.local.md, typically gitignored)
    ProjectLocal,
    /// Subdirectory-scoped context
    Subdirectory(PathBuf),
    /// Legacy .cursorrules file
    CursorRules,
    /// Cursor .mdc rule file with frontmatter
    CursorMdc {
        description: Option<String>,
        globs: Vec<String>,
        always_apply: bool,
    },
}

/// A loaded context file
#[derive(Debug, Clone)]
pub struct ContextFile {
    /// Path to the source file
    pub path: PathBuf,
    /// Content of the file (body only, frontmatter stripped for .mdc)
    pub content: String,
    /// Source type
    pub source: ContextFileSource,
    /// Priority (lower = applied first in concatenation)
    pub priority: u32,
}

/// Frontmatter from a .cursor/rules/*.mdc file
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MdcFrontmatter {
    /// Human-readable description
    #[serde(default)]
    pub description: Option<String>,
    /// Glob patterns for when to apply this rule
    #[serde(default)]
    pub globs: Vec<String>,
    /// Whether to always apply regardless of file context
    #[serde(default)]
    pub always_apply: bool,
}

/// Collected project context from all discovered files
#[derive(Debug, Clone)]
pub struct ProjectContext {
    /// All discovered context files (in priority order)
    files: Vec<ContextFile>,
    /// Pre-rendered combined context string
    combined_context: String,
    /// Total size in bytes
    total_size: usize,
    /// Whether context was truncated due to size limits
    truncated: bool,
}

impl ProjectContext {
    /// Discover and load project context from a directory
    pub fn discover(project_root: &Path, config: &ProjectContextConfig) -> Result<Self> {
        let mut files = Vec::new();

        if config.enable_claude_files {
            // 1. Global user context (~/.claude/CLAUDE.md or ~/.codex/AGENTS.md)
            Self::discover_global_user(&mut files);

            // 2. Project root CLAUDE.md or AGENTS.md
            Self::discover_project_root(project_root, &mut files);

            // 3. Project local override (CLAUDE.local.md or AGENTS.local.md)
            Self::discover_project_local(project_root, &mut files);

            // 4. Subdirectory CLAUDE.md/AGENTS.md files
            if config.subdirectory_depth > 0 {
                Self::discover_subdirectories(project_root, config.subdirectory_depth, &mut files);
            }
        }

        if config.enable_cursor_rules {
            // 5. Legacy .cursorrules
            Self::discover_cursor_rules(project_root, &mut files);

            // 6. .cursor/rules/*.mdc files
            Self::discover_mdc_rules(project_root, &mut files);
        }

        // Sort by priority
        files.sort_by_key(|f| f.priority);

        // Build combined context
        let (combined_context, total_size, truncated) =
            Self::build_combined_context(&files, config.max_total_size);

        Ok(Self {
            files,
            combined_context,
            total_size,
            truncated,
        })
    }

    /// Get the combined context string for injection into system prompt
    pub fn to_context_string(&self) -> &str {
        &self.combined_context
    }

    /// Get count of loaded context files
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Get total size in bytes
    pub fn total_size(&self) -> usize {
        self.total_size
    }

    /// Check if context was truncated
    pub fn is_truncated(&self) -> bool {
        self.truncated
    }

    /// Check if any context was found
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Get all loaded files (for debugging/display)
    pub fn files(&self) -> &[ContextFile] {
        &self.files
    }

    /// Filter MDC rules to only those that match current file context
    pub fn filter_for_context(&self, current_files: &[PathBuf]) -> Self {
        let filtered_files: Vec<ContextFile> = self
            .files
            .iter()
            .filter(|f| {
                match &f.source {
                    // Always include non-MDC files
                    ContextFileSource::CursorMdc {
                        globs,
                        always_apply,
                        ..
                    } => {
                        if *always_apply || globs.is_empty() {
                            return true;
                        }
                        // Check if any current file matches any glob
                        for file in current_files {
                            let file_str = file.to_string_lossy();
                            for glob_pattern in globs {
                                if let Ok(pattern) = glob::Pattern::new(glob_pattern) {
                                    if pattern.matches(&file_str) || pattern.matches_path(file) {
                                        return true;
                                    }
                                }
                            }
                        }
                        false
                    }
                    _ => true,
                }
            })
            .cloned()
            .collect();

        let (combined_context, total_size, truncated) =
            Self::build_combined_context(&filtered_files, usize::MAX);

        Self {
            files: filtered_files,
            combined_context,
            total_size,
            truncated,
        }
    }

    // === Discovery Functions ===

    fn discover_global_user(files: &mut Vec<ContextFile>) {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => {
                tracing::debug!("Could not determine home directory for global context");
                return;
            }
        };

        // Try ~/.claude/CLAUDE.md first
        let claude_path = home.join(".claude").join("CLAUDE.md");
        if claude_path.exists() {
            match std::fs::read_to_string(&claude_path) {
                Ok(content) => {
                    tracing::debug!("Loaded global context: {}", claude_path.display());
                    files.push(ContextFile {
                        path: claude_path,
                        content,
                        source: ContextFileSource::GlobalUser,
                        priority: 0,
                    });
                    return;
                }
                Err(e) => {
                    tracing::warn!("Failed to read global context file: {}", e);
                }
            }
        }

        // Fall back to ~/.codex/AGENTS.md
        let codex_path = home.join(".codex").join("AGENTS.md");
        if codex_path.exists() {
            match std::fs::read_to_string(&codex_path) {
                Ok(content) => {
                    tracing::debug!("Loaded global context: {}", codex_path.display());
                    files.push(ContextFile {
                        path: codex_path,
                        content,
                        source: ContextFileSource::GlobalUser,
                        priority: 0,
                    });
                }
                Err(e) => {
                    tracing::warn!("Failed to read global context file: {}", e);
                }
            }
        }
    }

    fn discover_project_root(root: &Path, files: &mut Vec<ContextFile>) {
        // Try CLAUDE.md first, then AGENTS.md
        for filename in &["CLAUDE.md", "AGENTS.md"] {
            let path = root.join(filename);
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        tracing::debug!("Loaded project context: {}", path.display());
                        files.push(ContextFile {
                            path,
                            content,
                            source: ContextFileSource::ProjectRoot,
                            priority: 10,
                        });
                        return; // Only load one
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read project context file: {}", e);
                    }
                }
            }
        }
    }

    fn discover_project_local(root: &Path, files: &mut Vec<ContextFile>) {
        // Try CLAUDE.local.md first, then AGENTS.local.md
        for filename in &["CLAUDE.local.md", "AGENTS.local.md"] {
            let path = root.join(filename);
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        tracing::debug!("Loaded local context: {}", path.display());
                        files.push(ContextFile {
                            path,
                            content,
                            source: ContextFileSource::ProjectLocal,
                            priority: 20,
                        });
                        return; // Only load one
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read local context file: {}", e);
                    }
                }
            }
        }
    }

    fn discover_subdirectories(root: &Path, max_depth: usize, files: &mut Vec<ContextFile>) {
        let walker = walkdir::WalkDir::new(root)
            .max_depth(max_depth)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file());

        for entry in walker {
            let name = entry.file_name().to_string_lossy();
            if name == "CLAUDE.md" || name == "AGENTS.md" {
                let path = entry.path();

                // Skip root level (already handled)
                if path.parent() == Some(root) {
                    continue;
                }

                let relative_dir = path
                    .parent()
                    .and_then(|p| p.strip_prefix(root).ok())
                    .map(|p| p.to_path_buf())
                    .unwrap_or_default();

                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        tracing::debug!("Loaded subdirectory context: {}", path.display());
                        files.push(ContextFile {
                            path: path.to_path_buf(),
                            content,
                            source: ContextFileSource::Subdirectory(relative_dir),
                            priority: 30,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read subdirectory context file: {}", e);
                    }
                }
            }
        }
    }

    fn discover_cursor_rules(root: &Path, files: &mut Vec<ContextFile>) {
        let cursorrules_path = root.join(".cursorrules");
        if cursorrules_path.exists() {
            match std::fs::read_to_string(&cursorrules_path) {
                Ok(content) => {
                    tracing::debug!("Loaded .cursorrules: {}", cursorrules_path.display());
                    files.push(ContextFile {
                        path: cursorrules_path,
                        content,
                        source: ContextFileSource::CursorRules,
                        priority: 40,
                    });
                }
                Err(e) => {
                    tracing::warn!("Failed to read .cursorrules: {}", e);
                }
            }
        }
    }

    fn discover_mdc_rules(root: &Path, files: &mut Vec<ContextFile>) {
        let rules_dir = root.join(".cursor").join("rules");
        if !rules_dir.exists() {
            return;
        }

        let entries = match std::fs::read_dir(&rules_dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Failed to read .cursor/rules directory: {}", e);
                return;
            }
        };

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();

            if path.extension().is_some_and(|e| e == "mdc") {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        let (frontmatter, body) = parse_mdc_frontmatter(&content);

                        tracing::debug!(
                            "Loaded .mdc rule: {} (globs: {:?}, always: {})",
                            path.display(),
                            frontmatter.globs,
                            frontmatter.always_apply
                        );

                        files.push(ContextFile {
                            path: path.clone(),
                            content: body,
                            source: ContextFileSource::CursorMdc {
                                description: frontmatter.description,
                                globs: frontmatter.globs,
                                always_apply: frontmatter.always_apply,
                            },
                            priority: 50,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read .mdc rule {}: {}", path.display(), e);
                    }
                }
            }
        }
    }

    // === Context Building ===

    fn build_combined_context(files: &[ContextFile], max_size: usize) -> (String, usize, bool) {
        if files.is_empty() {
            return (String::new(), 0, false);
        }

        let mut result = String::from("# Project Context\n\n");
        let mut total_size = result.len();
        let mut truncated = false;

        for file in files {
            let section_header = match &file.source {
                ContextFileSource::GlobalUser => "## Global User Context\n\n".to_string(),
                ContextFileSource::ProjectRoot => "## Project Context\n\n".to_string(),
                ContextFileSource::ProjectLocal => "## Project Local Context\n\n".to_string(),
                ContextFileSource::Subdirectory(dir) => {
                    format!("## Context for {}/\n\n", dir.display())
                }
                ContextFileSource::CursorRules => "## Cursor Rules\n\n".to_string(),
                ContextFileSource::CursorMdc { description, .. } => {
                    if let Some(desc) = description {
                        format!("## Cursor Rule: {}\n\n", desc)
                    } else {
                        format!(
                            "## Cursor Rule ({})\n\n",
                            file.path
                                .file_name()
                                .map_or("unknown".to_string(), |n| n.to_string_lossy().to_string())
                        )
                    }
                }
            };

            let section_content = format!("{}{}\n\n", section_header, file.content);
            let section_size = section_content.len();

            if total_size + section_size > max_size {
                truncated = true;
                // Add partial content up to limit
                let remaining = max_size.saturating_sub(total_size);
                if remaining > 100 {
                    // Only add if there's meaningful space
                    result.push_str(&section_content[..remaining.min(section_content.len())]);
                    result.push_str("\n... (truncated)\n");
                    total_size = max_size;
                }
                break;
            }

            result.push_str(&section_content);
            total_size += section_size;
        }

        (result, total_size, truncated)
    }
}

/// Parse YAML frontmatter from an .mdc file
fn parse_mdc_frontmatter(content: &str) -> (MdcFrontmatter, String) {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return (MdcFrontmatter::default(), content.to_string());
    }

    let rest = &trimmed[3..];
    if let Some(end_idx) = rest.find("\n---") {
        let frontmatter_str = rest[..end_idx].trim();
        let body = rest[end_idx + 4..].trim_start().to_string();

        match serde_yaml::from_str::<MdcFrontmatter>(frontmatter_str) {
            Ok(frontmatter) => (frontmatter, body),
            Err(e) => {
                tracing::warn!("Failed to parse MDC frontmatter: {}", e);
                (MdcFrontmatter::default(), content.to_string())
            }
        }
    } else {
        (MdcFrontmatter::default(), content.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_project_context_config_default() {
        let config = ProjectContextConfig::default();
        assert!(config.enable_claude_files);
        assert!(config.enable_cursor_rules);
        assert_eq!(config.max_total_size, 100_000);
        assert_eq!(config.subdirectory_depth, 3);
    }

    #[test]
    fn test_discover_empty_project() {
        let temp_dir = TempDir::new().unwrap();
        let config = ProjectContextConfig::default();

        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        assert!(context.is_empty());
        assert_eq!(context.file_count(), 0);
    }

    #[test]
    fn test_discover_claude_md() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.md"), "# Project\nTest content").unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        assert_eq!(context.file_count(), 1);
        assert!(context.to_context_string().contains("Test content"));
        assert!(matches!(
            context.files()[0].source,
            ContextFileSource::ProjectRoot
        ));
    }

    #[test]
    fn test_discover_agents_md() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("AGENTS.md"), "# Agents\nAgent content").unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        assert_eq!(context.file_count(), 1);
        assert!(context.to_context_string().contains("Agent content"));
    }

    #[test]
    fn test_claude_md_takes_priority_over_agents_md() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.md"), "Claude content").unwrap();
        std::fs::write(temp_dir.path().join("AGENTS.md"), "Agents content").unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        // Should only load CLAUDE.md (first in priority)
        assert_eq!(context.file_count(), 1);
        assert!(context.to_context_string().contains("Claude content"));
        assert!(!context.to_context_string().contains("Agents content"));
    }

    #[test]
    fn test_discover_local_override() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.md"), "Project content").unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.local.md"), "Local override").unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        assert_eq!(context.file_count(), 2);
        assert!(context.to_context_string().contains("Project content"));
        assert!(context.to_context_string().contains("Local override"));
    }

    #[test]
    fn test_discover_subdirectory_context() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        std::fs::write(temp_dir.path().join("src/CLAUDE.md"), "Source context").unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        assert_eq!(context.file_count(), 1);
        assert!(context.to_context_string().contains("Source context"));
        assert!(matches!(
            &context.files()[0].source,
            ContextFileSource::Subdirectory(dir) if dir.to_string_lossy() == "src"
        ));
    }

    #[test]
    fn test_discover_cursorrules() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join(".cursorrules"), "Cursor rules content").unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        assert_eq!(context.file_count(), 1);
        assert!(context.to_context_string().contains("Cursor rules content"));
        assert!(matches!(
            context.files()[0].source,
            ContextFileSource::CursorRules
        ));
    }

    #[test]
    fn test_discover_mdc_rules() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join(".cursor/rules")).unwrap();
        std::fs::write(
            temp_dir.path().join(".cursor/rules/typescript.mdc"),
            r#"---
description: TypeScript rules
globs:
  - "**/*.ts"
  - "**/*.tsx"
alwaysApply: false
---

Use strict TypeScript.
"#,
        )
        .unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        assert_eq!(context.file_count(), 1);
        assert!(context
            .to_context_string()
            .contains("Use strict TypeScript"));

        if let ContextFileSource::CursorMdc {
            description,
            globs,
            always_apply,
        } = &context.files()[0].source
        {
            assert_eq!(description.as_deref(), Some("TypeScript rules"));
            assert_eq!(globs.len(), 2);
            assert!(!always_apply);
        } else {
            panic!("Expected CursorMdc source");
        }
    }

    #[test]
    fn test_mdc_frontmatter_parsing() {
        let content = r#"---
description: Test rule
globs:
  - "*.rs"
alwaysApply: true
---

Rule content here.
"#;

        let (frontmatter, body) = parse_mdc_frontmatter(content);

        assert_eq!(frontmatter.description, Some("Test rule".to_string()));
        assert_eq!(frontmatter.globs, vec!["*.rs"]);
        assert!(frontmatter.always_apply);
        assert!(body.contains("Rule content here"));
    }

    #[test]
    fn test_mdc_frontmatter_parsing_no_frontmatter() {
        let content = "Just plain content";

        let (frontmatter, body) = parse_mdc_frontmatter(content);

        assert!(frontmatter.description.is_none());
        assert!(frontmatter.globs.is_empty());
        assert!(!frontmatter.always_apply);
        assert_eq!(body, content);
    }

    #[test]
    fn test_priority_ordering() {
        let temp_dir = TempDir::new().unwrap();

        // Create multiple context files
        std::fs::write(temp_dir.path().join("CLAUDE.md"), "Project").unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.local.md"), "Local").unwrap();
        std::fs::write(temp_dir.path().join(".cursorrules"), "Cursor").unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        // Check priority order (lower priority = earlier in output)
        assert_eq!(context.file_count(), 3);
        assert!(matches!(
            context.files()[0].source,
            ContextFileSource::ProjectRoot
        )); // priority 10
        assert!(matches!(
            context.files()[1].source,
            ContextFileSource::ProjectLocal
        )); // priority 20
        assert!(matches!(
            context.files()[2].source,
            ContextFileSource::CursorRules
        )); // priority 40
    }

    #[test]
    fn test_truncation() {
        let temp_dir = TempDir::new().unwrap();

        // Create a large context file
        let large_content = "x".repeat(50_000);
        std::fs::write(temp_dir.path().join("CLAUDE.md"), &large_content).unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.local.md"), &large_content).unwrap();

        let config = ProjectContextConfig {
            max_total_size: 60_000,
            ..Default::default()
        };
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        assert!(context.is_truncated());
        assert!(context.total_size() <= 60_000);
    }

    #[test]
    fn test_filter_for_context_mdc() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join(".cursor/rules")).unwrap();

        // TypeScript-specific rule
        std::fs::write(
            temp_dir.path().join(".cursor/rules/typescript.mdc"),
            r#"---
globs: ["**/*.ts"]
---
TS rule"#,
        )
        .unwrap();

        // Always-apply rule
        std::fs::write(
            temp_dir.path().join(".cursor/rules/general.mdc"),
            r#"---
alwaysApply: true
---
General rule"#,
        )
        .unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        // Filter for a .ts file
        let filtered = context.filter_for_context(&[PathBuf::from("src/main.ts")]);
        assert_eq!(filtered.file_count(), 2); // Both rules

        // Filter for a .rs file (should only get always-apply)
        let filtered = context.filter_for_context(&[PathBuf::from("src/main.rs")]);
        assert_eq!(filtered.file_count(), 1);
        assert!(filtered.to_context_string().contains("General rule"));
    }

    #[test]
    fn test_disable_claude_files() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.md"), "Claude content").unwrap();
        std::fs::write(temp_dir.path().join(".cursorrules"), "Cursor content").unwrap();

        let config = ProjectContextConfig {
            enable_claude_files: false,
            ..Default::default()
        };
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        assert_eq!(context.file_count(), 1);
        assert!(!context.to_context_string().contains("Claude content"));
        assert!(context.to_context_string().contains("Cursor content"));
    }

    #[test]
    fn test_disable_cursor_rules() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.md"), "Claude content").unwrap();
        std::fs::write(temp_dir.path().join(".cursorrules"), "Cursor content").unwrap();

        let config = ProjectContextConfig {
            enable_cursor_rules: false,
            ..Default::default()
        };
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        assert_eq!(context.file_count(), 1);
        assert!(context.to_context_string().contains("Claude content"));
        assert!(!context.to_context_string().contains("Cursor content"));
    }

    #[test]
    fn test_mdc_frontmatter_incomplete() {
        // Frontmatter starts but never closes
        let content = "---\ndescription: Test\nSome content";

        let (frontmatter, body) = parse_mdc_frontmatter(content);

        // Should return default frontmatter and original content
        assert!(frontmatter.description.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_mdc_frontmatter_invalid_yaml() {
        let content = r#"---
description: [invalid yaml
globs: not a list
---

Body content"#;

        let (frontmatter, body) = parse_mdc_frontmatter(content);

        // Should return default frontmatter and original content on parse error
        assert!(frontmatter.description.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_mdc_frontmatter_empty() {
        let content = r#"---
---

Body only"#;

        let (frontmatter, body) = parse_mdc_frontmatter(content);

        // Empty frontmatter should parse as defaults
        assert!(frontmatter.description.is_none());
        assert!(frontmatter.globs.is_empty());
        assert!(!frontmatter.always_apply);
        assert!(body.contains("Body only"));
    }

    #[test]
    fn test_mdc_frontmatter_with_leading_whitespace() {
        let content = r#"
---
description: With whitespace
---

Content"#;

        let (frontmatter, body) = parse_mdc_frontmatter(content);

        assert_eq!(frontmatter.description, Some("With whitespace".to_string()));
        assert!(body.contains("Content"));
    }

    #[test]
    fn test_filter_for_context_empty_files() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join(".cursor/rules")).unwrap();

        std::fs::write(
            temp_dir.path().join(".cursor/rules/typescript.mdc"),
            r#"---
globs: ["**/*.ts"]
---
TS rule"#,
        )
        .unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        // Filter with empty file list - should not match glob-only rules
        let filtered = context.filter_for_context(&[]);
        assert_eq!(filtered.file_count(), 0);
    }

    #[test]
    fn test_filter_for_context_mdc_empty_globs() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join(".cursor/rules")).unwrap();

        // MDC with empty globs should always apply
        std::fs::write(
            temp_dir.path().join(".cursor/rules/empty-globs.mdc"),
            r#"---
description: Empty globs rule
globs: []
---
Should always apply"#,
        )
        .unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        let filtered = context.filter_for_context(&[PathBuf::from("any/file.xyz")]);
        assert_eq!(filtered.file_count(), 1);
        assert!(filtered.to_context_string().contains("Should always apply"));
    }

    #[test]
    fn test_subdirectory_depth_limit() {
        let temp_dir = TempDir::new().unwrap();

        // Create nested directories beyond the depth limit
        // walkdir max_depth counts from root: depth 0=root, 1=a/, 2=a/CLAUDE.md or a/b/, etc.
        std::fs::create_dir_all(temp_dir.path().join("a/b/c/d")).unwrap();
        std::fs::write(temp_dir.path().join("a/CLAUDE.md"), "Level 1").unwrap();
        std::fs::write(temp_dir.path().join("a/b/CLAUDE.md"), "Level 2").unwrap();
        std::fs::write(temp_dir.path().join("a/b/c/CLAUDE.md"), "Level 3").unwrap();
        std::fs::write(temp_dir.path().join("a/b/c/d/CLAUDE.md"), "Level 4").unwrap();

        // Depth of 3 finds: a/CLAUDE.md (depth 2), a/b/CLAUDE.md (depth 3)
        // but not a/b/c/CLAUDE.md (depth 4) or a/b/c/d/CLAUDE.md (depth 5)
        let config = ProjectContextConfig {
            subdirectory_depth: 3,
            ..Default::default()
        };
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        assert_eq!(context.file_count(), 2);
        assert!(context.to_context_string().contains("Level 1"));
        assert!(context.to_context_string().contains("Level 2"));
        assert!(!context.to_context_string().contains("Level 3"));
        assert!(!context.to_context_string().contains("Level 4"));
    }

    #[test]
    fn test_subdirectory_depth_zero() {
        let temp_dir = TempDir::new().unwrap();

        std::fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.md"), "Root").unwrap();
        std::fs::write(temp_dir.path().join("src/CLAUDE.md"), "Subdirectory").unwrap();

        let config = ProjectContextConfig {
            subdirectory_depth: 0,
            ..Default::default()
        };
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        // Should only find root level
        assert_eq!(context.file_count(), 1);
        assert!(context.to_context_string().contains("Root"));
        assert!(!context.to_context_string().contains("Subdirectory"));
    }

    #[test]
    fn test_agents_local_md() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("AGENTS.md"), "Agent project").unwrap();
        std::fs::write(temp_dir.path().join("AGENTS.local.md"), "Agent local").unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        assert_eq!(context.file_count(), 2);
        assert!(context.to_context_string().contains("Agent project"));
        assert!(context.to_context_string().contains("Agent local"));
    }

    #[test]
    fn test_claude_local_takes_priority_over_agents_local() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.local.md"), "Claude local").unwrap();
        std::fs::write(temp_dir.path().join("AGENTS.local.md"), "Agent local").unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        // Should only load CLAUDE.local.md
        assert_eq!(context.file_count(), 1);
        assert!(context.to_context_string().contains("Claude local"));
        assert!(!context.to_context_string().contains("Agent local"));
    }

    #[test]
    fn test_truncation_small_remaining_space() {
        let temp_dir = TempDir::new().unwrap();

        // Create content that will leave less than 100 bytes remaining
        let content1 = "x".repeat(900);
        let content2 = "y".repeat(500);
        std::fs::write(temp_dir.path().join("CLAUDE.md"), &content1).unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.local.md"), &content2).unwrap();

        let config = ProjectContextConfig {
            max_total_size: 1000, // Very small limit
            ..Default::default()
        };
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        // Should be truncated and not include second file content due to < 100 bytes
        assert!(context.is_truncated());
    }

    #[test]
    fn test_mdc_no_frontmatter_markers() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join(".cursor/rules")).unwrap();

        // MDC file without any frontmatter
        std::fs::write(
            temp_dir.path().join(".cursor/rules/plain.mdc"),
            "Just plain content without frontmatter",
        )
        .unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        assert_eq!(context.file_count(), 1);
        // Should have empty globs which means always apply
        if let ContextFileSource::CursorMdc {
            globs,
            always_apply,
            ..
        } = &context.files()[0].source
        {
            assert!(globs.is_empty());
            assert!(!always_apply);
        } else {
            panic!("Expected CursorMdc source");
        }
    }

    #[test]
    fn test_context_file_source_equality() {
        assert_eq!(ContextFileSource::GlobalUser, ContextFileSource::GlobalUser);
        assert_eq!(
            ContextFileSource::ProjectRoot,
            ContextFileSource::ProjectRoot
        );
        assert_eq!(
            ContextFileSource::CursorRules,
            ContextFileSource::CursorRules
        );
        assert_ne!(
            ContextFileSource::GlobalUser,
            ContextFileSource::ProjectRoot
        );

        let sub1 = ContextFileSource::Subdirectory(PathBuf::from("src"));
        let sub2 = ContextFileSource::Subdirectory(PathBuf::from("src"));
        let sub3 = ContextFileSource::Subdirectory(PathBuf::from("lib"));
        assert_eq!(sub1, sub2);
        assert_ne!(sub1, sub3);
    }

    #[test]
    fn test_context_combined_string_headers() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join(".cursor/rules")).unwrap();
        std::fs::create_dir_all(temp_dir.path().join("src")).unwrap();

        std::fs::write(temp_dir.path().join("CLAUDE.md"), "project").unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.local.md"), "local").unwrap();
        std::fs::write(temp_dir.path().join("src/CLAUDE.md"), "subdir").unwrap();
        std::fs::write(temp_dir.path().join(".cursorrules"), "cursor").unwrap();
        std::fs::write(
            temp_dir.path().join(".cursor/rules/test.mdc"),
            r#"---
description: Test MDC
---
mdc content"#,
        )
        .unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();
        let combined = context.to_context_string();

        // Check all section headers are present
        assert!(combined.contains("# Project Context"));
        assert!(combined.contains("## Project Context"));
        assert!(combined.contains("## Project Local Context"));
        assert!(combined.contains("## Context for src/"));
        assert!(combined.contains("## Cursor Rules"));
        assert!(combined.contains("## Cursor Rule: Test MDC"));
    }

    #[test]
    fn test_mdc_rule_without_description_uses_filename() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join(".cursor/rules")).unwrap();

        std::fs::write(
            temp_dir.path().join(".cursor/rules/my-rule.mdc"),
            r#"---
globs: ["*.rs"]
---
content"#,
        )
        .unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();
        let combined = context.to_context_string();

        // Should use filename when no description
        assert!(combined.contains("## Cursor Rule (my-rule.mdc)"));
    }

    #[test]
    fn test_filter_for_context_preserves_non_mdc() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(temp_dir.path().join(".cursor/rules")).unwrap();

        std::fs::write(temp_dir.path().join("CLAUDE.md"), "Project content").unwrap();
        std::fs::write(temp_dir.path().join(".cursorrules"), "Cursor content").unwrap();
        std::fs::write(
            temp_dir.path().join(".cursor/rules/ts.mdc"),
            r#"---
globs: ["*.ts"]
---
TS only"#,
        )
        .unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        // Filter for a .rs file - non-MDC files should always be included
        let filtered = context.filter_for_context(&[PathBuf::from("main.rs")]);

        assert_eq!(filtered.file_count(), 2); // CLAUDE.md and .cursorrules
        assert!(filtered.to_context_string().contains("Project content"));
        assert!(filtered.to_context_string().contains("Cursor content"));
        assert!(!filtered.to_context_string().contains("TS only"));
    }

    #[test]
    fn test_total_size_tracking() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("CLAUDE.md"), "Hello World").unwrap();

        let config = ProjectContextConfig::default();
        let context = ProjectContext::discover(temp_dir.path(), &config).unwrap();

        // Total size should include headers and content
        assert!(context.total_size() > "Hello World".len());
        assert!(context.total_size() > 0);
    }
}
