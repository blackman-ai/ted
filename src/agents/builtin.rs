// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Built-in agent types
//!
//! This module defines the standard agent types that come with Ted:
//! - explore: Codebase discovery and search
//! - plan: Architecture design and planning
//! - implement: Code writing and modification
//! - bash: Command execution
//! - review: Code review and analysis

use std::collections::HashMap;
use std::sync::OnceLock;

use super::types::{MemoryStrategy, ToolPermissions};

/// Definition of an agent type
#[derive(Debug, Clone)]
pub struct AgentTypeDefinition {
    /// Name of the agent type
    pub name: &'static str,
    /// Short description
    pub description: &'static str,
    /// Default caps to load
    pub default_caps: Vec<&'static str>,
    /// Tool permissions (which tools are allowed)
    pub tool_permissions: ToolPermissions,
    /// Maximum iterations
    pub max_iterations: u32,
    /// Default memory strategy
    pub memory_strategy: MemoryStrategy,
    /// Whether this agent can write files
    pub can_write: bool,
    /// Whether this agent can execute shell commands
    pub can_execute: bool,
    /// System prompt additions for this agent type
    pub system_prompt_additions: &'static str,
}

/// Registry of built-in agent types
static BUILTIN_TYPES: OnceLock<HashMap<&'static str, AgentTypeDefinition>> = OnceLock::new();

/// Get the built-in agent type definitions
pub fn get_builtin_types() -> &'static HashMap<&'static str, AgentTypeDefinition> {
    BUILTIN_TYPES.get_or_init(|| {
        let mut types = HashMap::new();

        // Explore agent - read-only codebase discovery
        types.insert(
            "explore",
            AgentTypeDefinition {
                name: "explore",
                description: "Codebase discovery and search agent",
                default_caps: vec!["coding"],
                tool_permissions: ToolPermissions::allow(&["file_read", "glob", "grep"]),
                max_iterations: 30,
                memory_strategy: MemoryStrategy::Full,
                can_write: false,
                can_execute: false,
                system_prompt_additions: r#"
You are an EXPLORE agent. Your job is to search and discover code in the codebase.

CONSTRAINTS:
- You can ONLY read files, search with glob, and search content with grep
- You CANNOT modify any files or run shell commands
- Focus on finding relevant files and understanding code structure

GOALS:
- Find files matching the user's query
- Understand code organization and patterns
- Report your findings clearly and concisely
"#,
            },
        );

        // Plan agent - architecture and design
        types.insert(
            "plan",
            AgentTypeDefinition {
                name: "plan",
                description: "Architecture design and planning agent",
                default_caps: vec!["coding", "planning"],
                tool_permissions: ToolPermissions::allow(&["file_read", "glob", "grep"]),
                max_iterations: 50,
                memory_strategy: MemoryStrategy::summarizing(),
                can_write: false,
                can_execute: false,
                system_prompt_additions: r#"
You are a PLAN agent. Your job is to design implementation strategies.

CONSTRAINTS:
- You can ONLY read files and search the codebase
- You CANNOT modify files or execute commands
- Focus on understanding existing patterns before proposing changes

GOALS:
- Analyze the current architecture
- Identify files that need to be modified
- Create a step-by-step implementation plan
- Consider edge cases and potential issues
- Propose a testing strategy
"#,
            },
        );

        // Implement agent - code writing
        types.insert(
            "implement",
            AgentTypeDefinition {
                name: "implement",
                description: "Code writing and modification agent",
                default_caps: vec!["coding", "testing"],
                tool_permissions: ToolPermissions::allow(&[
                    "file_read",
                    "file_write",
                    "file_edit",
                    "glob",
                    "grep",
                    "shell",
                ]),
                max_iterations: 40,
                memory_strategy: MemoryStrategy::summarizing(),
                can_write: true,
                can_execute: true,
                system_prompt_additions: r#"
You are an IMPLEMENT agent. Your job is to write and modify code.

CAPABILITIES:
- Read, write, and edit files
- Run shell commands (builds, tests, etc.)
- Search the codebase

GOALS:
- Implement the requested feature or fix
- Follow existing code patterns and conventions
- Run tests to verify your changes
- Keep changes minimal and focused
"#,
            },
        );

        // Bash agent - command execution
        types.insert(
            "bash",
            AgentTypeDefinition {
                name: "bash",
                description: "Command execution agent",
                default_caps: vec!["shell"],
                tool_permissions: ToolPermissions::allow(&["shell", "file_read", "glob"]),
                max_iterations: 25,
                memory_strategy: MemoryStrategy::windowed(20),
                can_write: false,
                can_execute: true,
                system_prompt_additions: r#"
You are a BASH agent. Your job is to execute shell commands.

CAPABILITIES:
- Run shell commands
- Read files for context
- Search for files with glob

CONSTRAINTS:
- You CANNOT directly modify files (use shell commands if needed)
- Be careful with destructive commands
- Always verify command success

GOALS:
- Execute the requested commands
- Handle errors gracefully
- Report results clearly
"#,
            },
        );

        // Review agent - code review
        types.insert(
            "review",
            AgentTypeDefinition {
                name: "review",
                description: "Code review and analysis agent",
                default_caps: vec!["coding", "review"],
                tool_permissions: ToolPermissions::allow(&["file_read", "glob", "grep"]),
                max_iterations: 30,
                memory_strategy: MemoryStrategy::Full,
                can_write: false,
                can_execute: false,
                system_prompt_additions: r#"
You are a REVIEW agent. Your job is to analyze and review code.

CONSTRAINTS:
- You can ONLY read files and search the codebase
- You CANNOT modify any files or run commands

GOALS:
- Analyze code quality and patterns
- Identify potential bugs or issues
- Suggest improvements
- Check for security vulnerabilities
- Verify adherence to best practices
"#,
            },
        );

        types
    })
}

/// Get a specific agent type definition by name
pub fn get_agent_type(name: &str) -> Option<&'static AgentTypeDefinition> {
    get_builtin_types().get(name)
}

/// Get all available agent type names
pub fn get_agent_type_names() -> Vec<&'static str> {
    get_builtin_types().keys().copied().collect()
}

/// Check if an agent type exists
pub fn is_valid_agent_type(name: &str) -> bool {
    get_builtin_types().contains_key(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_builtin_types() {
        let types = get_builtin_types();

        assert!(types.contains_key("explore"));
        assert!(types.contains_key("plan"));
        assert!(types.contains_key("implement"));
        assert!(types.contains_key("bash"));
        assert!(types.contains_key("review"));
    }

    #[test]
    fn test_get_agent_type() {
        let explore = get_agent_type("explore").unwrap();

        assert_eq!(explore.name, "explore");
        assert!(!explore.can_write);
        assert!(!explore.can_execute);
    }

    #[test]
    fn test_implement_agent_permissions() {
        let implement = get_agent_type("implement").unwrap();

        assert!(implement.can_write);
        assert!(implement.can_execute);
        assert!(implement.tool_permissions.is_allowed("file_write"));
        assert!(implement.tool_permissions.is_allowed("shell"));
    }

    #[test]
    fn test_explore_agent_read_only() {
        let explore = get_agent_type("explore").unwrap();

        assert!(explore.tool_permissions.is_allowed("file_read"));
        assert!(explore.tool_permissions.is_allowed("glob"));
        assert!(!explore.tool_permissions.is_allowed("file_write"));
        assert!(!explore.tool_permissions.is_allowed("shell"));
    }

    #[test]
    fn test_is_valid_agent_type() {
        assert!(is_valid_agent_type("explore"));
        assert!(is_valid_agent_type("implement"));
        assert!(!is_valid_agent_type("nonexistent"));
    }

    #[test]
    fn test_get_agent_type_names() {
        let names = get_agent_type_names();

        assert!(names.contains(&"explore"));
        assert!(names.contains(&"plan"));
        assert!(names.contains(&"implement"));
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"review"));
    }
}
