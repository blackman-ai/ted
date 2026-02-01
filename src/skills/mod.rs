// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Skills system for domain-specific expertise
//!
//! Skills are packages of domain expertise that can be loaded by subagents
//! to provide specialized knowledge and guidance.
//!
//! ## Skill Structure
//!
//! Each skill is a directory containing:
//! - `SKILL.md` - Main skill file with YAML frontmatter
//! - Optional `.md` or `.txt` resource files
//! - Optional `.sh`, `.py`, `.rb` script files
//!
//! ## SKILL.md Format
//!
//! ```markdown
//! ---
//! name: rust-async
//! description: Expert guidance for async Rust with tokio
//! tools:
//!   allow:
//!     - database_query
//!   deny:
//!     - file_delete
//! ---
//!
//! # Async Rust with Tokio
//!
//! ## Key Patterns
//! - Use `tokio::spawn` for concurrent tasks
//! - Prefer channels over shared state
//! ...
//! ```
//!
//! ## Skill Locations
//!
//! Skills are searched in order:
//! 1. `./.ted/skills/{name}/` (project-local, highest priority)
//! 2. `~/.ted/skills/{name}/` (user-global)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use ted::skills::SkillRegistry;
//!
//! // Create and scan for skills
//! let mut registry = SkillRegistry::new();
//! registry.scan()?;
//!
//! // List available skills
//! for meta in registry.all_metadata() {
//!     println!("{}: {}", meta.name, meta.description);
//! }
//!
//! // Load a skill fully
//! let skill = registry.load("rust-async")?;
//! println!("{}", skill.to_prompt_content());
//! ```

pub mod loader;
pub mod schema;

// Re-export commonly used types
pub use loader::{load_skill_from_path, load_skill_metadata_from_path, SkillRegistry};
pub use schema::{
    Skill, SkillFrontmatter, SkillMetadata, SkillResource, SkillScript, SkillToolPermissions,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_module_exports() {
        // Verify types are exported correctly
        let _ = SkillRegistry::new();
    }

    #[test]
    fn test_skill_parse_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = r#"---
name: integration-test
description: Integration test skill
---

# Test Skill

This is test content.
"#;

        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        let skill = load_skill_from_path(&skill_dir).unwrap();
        assert_eq!(skill.name, "integration-test");
        assert!(skill.content.contains("test content"));
    }

    #[test]
    fn test_skill_with_tools() {
        let content = r#"---
name: secure-skill
description: Skill with tool restrictions
tools:
  allow:
    - file_read
    - grep
  deny:
    - shell
---

Content.
"#;

        let skill = Skill::parse(content, PathBuf::from("/test")).unwrap();

        let perms = skill.tool_permissions.unwrap();
        assert!(perms.allow.contains(&"file_read".to_string()));
        assert!(perms.deny.contains(&"shell".to_string()));
    }
}
