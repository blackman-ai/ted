// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Skill schema definitions
//!
//! Skills are domain-specific expertise packages that can be loaded by subagents.
//! Each skill is defined by a SKILL.md file with YAML frontmatter.
//!
//! ## SKILL.md Format
//!
//! ```markdown
//! ---
//! name: rust-async
//! description: Expert guidance for async Rust with tokio
//! ---
//!
//! # Async Rust with Tokio
//!
//! ## Patterns
//! ...
//!
//! ## Common Pitfalls
//! ...
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Result, TedError};

/// A skill provides domain-specific expertise for subagents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Skill name (from YAML frontmatter)
    pub name: String,
    /// Short description (from YAML frontmatter)
    pub description: String,
    /// The main content of the SKILL.md (markdown body)
    pub content: String,
    /// Additional resource files in the skill directory
    pub resources: Vec<SkillResource>,
    /// Executable scripts in the skill directory
    pub scripts: Vec<SkillScript>,
    /// Path to the skill directory
    pub source_path: PathBuf,
    /// Optional tool permissions additions
    #[serde(default)]
    pub tool_permissions: Option<SkillToolPermissions>,
    /// Optional metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// YAML frontmatter for a SKILL.md file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    /// Skill name
    pub name: String,
    /// Short description
    pub description: String,
    /// Optional tool permissions
    #[serde(default)]
    pub tools: Option<SkillToolPermissions>,
    /// Optional additional metadata
    #[serde(default, flatten)]
    pub extra: HashMap<String, String>,
}

/// Tool permission modifications from a skill
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillToolPermissions {
    /// Additional tools to allow
    #[serde(default)]
    pub allow: Vec<String>,
    /// Tools to deny
    #[serde(default)]
    pub deny: Vec<String>,
}

/// An additional resource file in the skill directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillResource {
    /// File name
    pub name: String,
    /// Relative path from skill directory
    pub path: PathBuf,
    /// File content (loaded on demand)
    #[serde(skip)]
    pub content: Option<String>,
}

/// An executable script in the skill directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillScript {
    /// Script name
    pub name: String,
    /// Relative path from skill directory
    pub path: PathBuf,
    /// Script description
    pub description: Option<String>,
}

/// Metadata-only view of a skill (for fast loading)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Skill name
    pub name: String,
    /// Short description
    pub description: String,
    /// Path to the skill directory
    pub source_path: PathBuf,
}

impl Skill {
    /// Parse a SKILL.md file and create a Skill
    pub fn parse(content: &str, source_path: PathBuf) -> Result<Self> {
        let (frontmatter, body) = parse_frontmatter(content)?;

        Ok(Self {
            name: frontmatter.name,
            description: frontmatter.description,
            content: body,
            resources: Vec::new(),
            scripts: Vec::new(),
            source_path,
            tool_permissions: frontmatter.tools,
            metadata: frontmatter.extra,
        })
    }

    /// Get metadata-only view (for listing)
    pub fn metadata(&self) -> SkillMetadata {
        SkillMetadata {
            name: self.name.clone(),
            description: self.description.clone(),
            source_path: self.source_path.clone(),
        }
    }

    /// Get the full content suitable for system prompt injection
    pub fn to_prompt_content(&self) -> String {
        let mut content = self.content.clone();

        // Add resource references if any
        if !self.resources.is_empty() {
            content.push_str("\n\n## Available Resources\n");
            for resource in &self.resources {
                content.push_str(&format!("- {}\n", resource.name));
            }
        }

        // Add script references if any
        if !self.scripts.is_empty() {
            content.push_str("\n\n## Available Scripts\n");
            for script in &self.scripts {
                if let Some(desc) = &script.description {
                    content.push_str(&format!("- {} - {}\n", script.name, desc));
                } else {
                    content.push_str(&format!("- {}\n", script.name));
                }
            }
        }

        content
    }

    /// Add a resource file
    pub fn add_resource(&mut self, name: String, path: PathBuf) {
        self.resources.push(SkillResource {
            name,
            path,
            content: None,
        });
    }

    /// Add a script
    pub fn add_script(&mut self, name: String, path: PathBuf, description: Option<String>) {
        self.scripts.push(SkillScript {
            name,
            path,
            description,
        });
    }
}

impl SkillMetadata {
    /// Parse just the frontmatter from a SKILL.md file
    pub fn parse(content: &str, source_path: PathBuf) -> Result<Self> {
        let (frontmatter, _) = parse_frontmatter(content)?;

        Ok(Self {
            name: frontmatter.name,
            description: frontmatter.description,
            source_path,
        })
    }
}

/// Parse YAML frontmatter from a markdown file
fn parse_frontmatter(content: &str) -> Result<(SkillFrontmatter, String)> {
    let content = content.trim();

    // Check for YAML frontmatter delimiters
    if !content.starts_with("---") {
        return Err(TedError::Config(
            "SKILL.md must start with YAML frontmatter (---)".to_string(),
        ));
    }

    // Find the closing ---
    let after_first = &content[3..];
    let end_pos = after_first
        .find("\n---")
        .ok_or_else(|| TedError::Config("Missing closing --- for frontmatter".to_string()))?;

    let yaml_content = after_first[..end_pos].trim();
    let body_start = 3 + end_pos + 4; // Skip first ---, yaml, \n---
    let body = if body_start < content.len() {
        content[body_start..].trim().to_string()
    } else {
        String::new()
    };

    // Parse YAML
    let frontmatter: SkillFrontmatter = serde_yaml::from_str(yaml_content)
        .map_err(|e| TedError::Config(format!("Failed to parse skill frontmatter: {}", e)))?;

    Ok((frontmatter, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter() {
        let content = r#"---
name: rust-async
description: Async Rust patterns with tokio
---

# Async Rust

Use async/await patterns.
"#;

        let (frontmatter, body) = parse_frontmatter(content).unwrap();

        assert_eq!(frontmatter.name, "rust-async");
        assert_eq!(frontmatter.description, "Async Rust patterns with tokio");
        assert!(body.contains("Async Rust"));
    }

    #[test]
    fn test_parse_frontmatter_with_tools() {
        let content = r#"---
name: database
description: Database operations
tools:
  allow:
    - database_query
  deny:
    - file_write
---

Database skill content.
"#;

        let (frontmatter, _) = parse_frontmatter(content).unwrap();

        assert_eq!(frontmatter.name, "database");
        let tools = frontmatter.tools.unwrap();
        assert!(tools.allow.contains(&"database_query".to_string()));
        assert!(tools.deny.contains(&"file_write".to_string()));
    }

    #[test]
    fn test_skill_parse() {
        let content = r#"---
name: react-typescript
description: React with TypeScript best practices
---

# React + TypeScript

Always use functional components.
"#;

        let skill = Skill::parse(content, PathBuf::from("/skills/react")).unwrap();

        assert_eq!(skill.name, "react-typescript");
        assert_eq!(skill.description, "React with TypeScript best practices");
        assert!(skill.content.contains("functional components"));
    }

    #[test]
    fn test_skill_metadata() {
        let skill = Skill {
            name: "test".to_string(),
            description: "Test skill".to_string(),
            content: "Content".to_string(),
            resources: Vec::new(),
            scripts: Vec::new(),
            source_path: PathBuf::from("/skills/test"),
            tool_permissions: None,
            metadata: HashMap::new(),
        };

        let meta = skill.metadata();
        assert_eq!(meta.name, "test");
        assert_eq!(meta.description, "Test skill");
    }

    #[test]
    fn test_skill_to_prompt_content() {
        let mut skill = Skill {
            name: "test".to_string(),
            description: "Test".to_string(),
            content: "Main content".to_string(),
            resources: Vec::new(),
            scripts: Vec::new(),
            source_path: PathBuf::from("/skills/test"),
            tool_permissions: None,
            metadata: HashMap::new(),
        };

        skill.add_resource("examples.md".to_string(), PathBuf::from("examples.md"));
        skill.add_script(
            "run-tests.sh".to_string(),
            PathBuf::from("run-tests.sh"),
            Some("Run the test suite".to_string()),
        );

        let prompt = skill.to_prompt_content();

        assert!(prompt.contains("Main content"));
        assert!(prompt.contains("Available Resources"));
        assert!(prompt.contains("examples.md"));
        assert!(prompt.contains("Available Scripts"));
        assert!(prompt.contains("run-tests.sh"));
        assert!(prompt.contains("Run the test suite"));
    }

    #[test]
    fn test_parse_frontmatter_no_opening() {
        let content = "Just some content without frontmatter";
        let result = parse_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_frontmatter_no_closing() {
        let content = "---\nname: test\n\nNo closing delimiter";
        let result = parse_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_skill_metadata_parse() {
        let content = r#"---
name: quick-skill
description: A quick skill
---

Long content that we don't want to load.
"#;

        let meta = SkillMetadata::parse(content, PathBuf::from("/skills/quick")).unwrap();

        assert_eq!(meta.name, "quick-skill");
        assert_eq!(meta.description, "A quick skill");
    }
}
