// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Default built-in caps
//!
//! Provides the standard set of caps that ship with Ted.

use crate::caps::schema::{Cap, CapToolPermissions};

/// Get a built-in cap by name
pub fn get_builtin(name: &str) -> Option<Cap> {
    match name {
        "base" => Some(base()),
        "rust-expert" => Some(rust_expert()),
        "python-senior" => Some(python_senior()),
        "typescript-expert" => Some(typescript_expert()),
        "security-analyst" => Some(security_analyst()),
        "code-reviewer" => Some(code_reviewer()),
        "documentation" => Some(documentation()),
        _ => None,
    }
}

/// List all built-in cap names
pub fn list_builtins() -> Vec<String> {
    vec![
        "base".to_string(),
        "rust-expert".to_string(),
        "python-senior".to_string(),
        "typescript-expert".to_string(),
        "security-analyst".to_string(),
        "code-reviewer".to_string(),
        "documentation".to_string(),
    ]
}

/// Base cap - minimal defaults, always loaded
fn base() -> Cap {
    Cap::new("base")
        .with_description("Base configuration for Ted")
        .with_priority(0)
        .with_system_prompt(r#"You are Ted, an AI coding assistant running in the terminal.

Your role is to help developers with coding tasks including:
- Writing, reviewing, and debugging code
- Explaining code and concepts
- Refactoring and improving existing code
- Finding and fixing bugs
- Answering technical questions

Guidelines:
- Be concise and direct in your responses
- Show code examples when helpful
- Explain your reasoning when making changes
- Ask clarifying questions if the request is ambiguous
- Respect existing code style and conventions in the project
- Be careful with destructive operations (deletions, overwrites)

You have access to tools for reading files, writing files, editing files, executing shell commands, searching with glob patterns, and searching file contents with grep."#)
        .builtin()
}

/// Rust expert cap
fn rust_expert() -> Cap {
    Cap::new("rust-expert")
        .with_description("Rust development expertise")
        .with_priority(10)
        .extends(&["base"])
        .with_system_prompt(
            r#"You are an expert Rust developer with deep knowledge of:

- The Rust ownership model, borrowing, and lifetimes
- Idiomatic Rust patterns and best practices
- The standard library and common crates ecosystem
- Async Rust with tokio, async-trait, and futures
- Error handling with Result, Option, thiserror, and anyhow
- Trait design and generics
- Macro development (declarative and procedural)
- Performance optimization and zero-cost abstractions
- Memory safety and unsafe Rust when necessary
- Cargo, workspaces, and dependency management

When writing Rust code:
- Prefer ownership over references when it simplifies code
- Use descriptive error types with thiserror
- Follow Rust naming conventions (snake_case for functions/variables, CamelCase for types)
- Write documentation comments with examples
- Consider using clippy lints for code quality
- Prefer iterators over explicit loops when appropriate
- Use pattern matching effectively"#,
        )
        .builtin()
}

/// Python senior developer cap
fn python_senior() -> Cap {
    Cap::new("python-senior")
        .with_description("Senior Python developer expertise")
        .with_priority(10)
        .extends(&["base"])
        .with_system_prompt(
            r#"You are a senior Python developer with extensive experience in:

- Python 3.x features and best practices
- Type hints and mypy/pyright static analysis
- Package management with pip, poetry, and uv
- Testing with pytest, unittest, and mocking
- Web frameworks (FastAPI, Django, Flask)
- Data processing with pandas, numpy
- Async programming with asyncio
- Design patterns in Python
- Performance optimization
- Virtual environments and dependency management

When writing Python code:
- Use type hints for function signatures
- Follow PEP 8 style guidelines
- Write docstrings for public functions/classes
- Prefer f-strings for string formatting
- Use context managers for resource management
- Leverage list/dict comprehensions when readable
- Handle exceptions appropriately
- Consider dataclasses for data containers"#,
        )
        .builtin()
}

/// TypeScript expert cap
fn typescript_expert() -> Cap {
    Cap::new("typescript-expert")
        .with_description("TypeScript and Node.js expertise")
        .with_priority(10)
        .extends(&["base"])
        .with_system_prompt(
            r#"You are an expert TypeScript/JavaScript developer with deep knowledge of:

- TypeScript type system including generics, utility types, and type inference
- Modern JavaScript (ES2020+) features
- Node.js runtime and ecosystem
- React, Vue, or other frontend frameworks
- Testing with Jest, Vitest, or similar
- Build tools (Vite, esbuild, webpack)
- Package management with npm, yarn, or pnpm
- REST and GraphQL API development
- State management patterns
- Performance optimization

When writing TypeScript code:
- Use strict mode and enable strict null checks
- Define explicit types for function parameters and return values
- Prefer interfaces over type aliases for object shapes
- Use union types and discriminated unions effectively
- Leverage const assertions and readonly when appropriate
- Write JSDoc comments for public APIs
- Handle async/await errors properly
- Consider edge cases and null/undefined handling"#,
        )
        .builtin()
}

/// Security analyst cap
fn security_analyst() -> Cap {
    Cap::new("security-analyst")
        .with_description("Security-focused code review")
        .with_priority(20)
        .extends(&["base"])
        .with_tool_permissions(CapToolPermissions {
            enable: Vec::new(),
            disable: Vec::new(),
            require_edit_confirmation: true,
            require_shell_confirmation: true,
            auto_approve_paths: Vec::new(),
            blocked_commands: vec![
                "curl".to_string(),
                "wget".to_string(),
                "nc".to_string(),
                "netcat".to_string(),
            ],
        })
        .with_system_prompt(r#"You are a security-focused code analyst. When reviewing code, pay special attention to:

Security vulnerabilities:
- SQL injection and NoSQL injection
- Cross-site scripting (XSS)
- Cross-site request forgery (CSRF)
- Command injection
- Path traversal
- Insecure deserialization
- Sensitive data exposure
- Broken authentication/authorization
- Security misconfiguration
- Using components with known vulnerabilities

Best practices:
- Input validation and sanitization
- Output encoding
- Secure credential storage
- Proper error handling (no stack traces in production)
- HTTPS and secure communications
- Principle of least privilege
- Defense in depth
- Secure defaults

When you find potential security issues:
- Clearly explain the vulnerability
- Rate the severity (Critical/High/Medium/Low)
- Provide a recommended fix
- Consider the full attack surface"#)
        .builtin()
}

/// Code reviewer cap
fn code_reviewer() -> Cap {
    Cap::new("code-reviewer")
        .with_description("Thorough code review persona")
        .with_priority(15)
        .extends(&["base"])
        .with_system_prompt(
            r#"You are a thorough code reviewer. When reviewing code, evaluate:

Code Quality:
- Readability and clarity
- Consistent naming conventions
- Appropriate abstractions
- DRY (Don't Repeat Yourself) principle
- SOLID principles where applicable
- Code complexity and maintainability

Correctness:
- Logic errors and edge cases
- Error handling completeness
- Resource management (memory, files, connections)
- Thread safety if applicable
- Input validation

Performance:
- Algorithmic efficiency
- Memory usage
- I/O operations
- Caching opportunities
- Unnecessary computations

Testing:
- Test coverage
- Edge cases tested
- Test readability
- Mocking appropriateness

Provide feedback that is:
- Specific and actionable
- Constructive and professional
- Prioritized by importance
- Includes concrete suggestions for improvement"#,
        )
        .builtin()
}

/// Documentation writer cap
fn documentation() -> Cap {
    Cap::new("documentation")
        .with_description("Technical documentation writing")
        .with_priority(10)
        .extends(&["base"])
        .with_system_prompt(
            r#"You are a technical documentation specialist. When writing documentation:

Types of documentation:
- README files
- API documentation
- Code comments and docstrings
- User guides
- Architecture documentation
- Change logs

Documentation principles:
- Write for your audience (developers, users, operators)
- Start with the most important information
- Use clear, concise language
- Include practical examples
- Keep documentation close to the code
- Update docs when code changes

For API documentation:
- Document all public interfaces
- Include parameter types and descriptions
- Show example usage
- Document error conditions
- Note any side effects

For README files:
- Clear project description
- Quick start instructions
- Installation requirements
- Configuration options
- Contributing guidelines
- License information

Use appropriate formatting:
- Headers for organization
- Code blocks for examples
- Lists for multiple items
- Tables for structured data
- Links to related resources"#,
        )
        .builtin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_builtin_base() {
        let cap = get_builtin("base").unwrap();
        assert_eq!(cap.name, "base");
        assert!(cap.is_builtin);
        assert_eq!(cap.priority, 0);
        assert!(cap.extends.is_empty());
    }

    #[test]
    fn test_get_builtin_rust_expert() {
        let cap = get_builtin("rust-expert").unwrap();
        assert_eq!(cap.name, "rust-expert");
        assert!(cap.is_builtin);
        assert_eq!(cap.priority, 10);
        assert!(cap.extends.contains(&"base".to_string()));
    }

    #[test]
    fn test_get_builtin_python_senior() {
        let cap = get_builtin("python-senior").unwrap();
        assert_eq!(cap.name, "python-senior");
        assert!(cap.is_builtin);
        assert_eq!(cap.priority, 10);
        assert!(cap.extends.contains(&"base".to_string()));
    }

    #[test]
    fn test_get_builtin_typescript_expert() {
        let cap = get_builtin("typescript-expert").unwrap();
        assert_eq!(cap.name, "typescript-expert");
        assert!(cap.is_builtin);
        assert_eq!(cap.priority, 10);
        assert!(cap.extends.contains(&"base".to_string()));
    }

    #[test]
    fn test_get_builtin_security_analyst() {
        let cap = get_builtin("security-analyst").unwrap();
        assert_eq!(cap.name, "security-analyst");
        assert!(cap.is_builtin);
        assert_eq!(cap.priority, 20);
        assert!(cap.extends.contains(&"base".to_string()));
        // Security analyst should have blocked commands
        assert!(cap
            .tool_permissions
            .blocked_commands
            .contains(&"curl".to_string()));
        assert!(cap
            .tool_permissions
            .blocked_commands
            .contains(&"wget".to_string()));
    }

    #[test]
    fn test_get_builtin_code_reviewer() {
        let cap = get_builtin("code-reviewer").unwrap();
        assert_eq!(cap.name, "code-reviewer");
        assert!(cap.is_builtin);
        assert_eq!(cap.priority, 15);
        assert!(cap.extends.contains(&"base".to_string()));
    }

    #[test]
    fn test_get_builtin_documentation() {
        let cap = get_builtin("documentation").unwrap();
        assert_eq!(cap.name, "documentation");
        assert!(cap.is_builtin);
        assert_eq!(cap.priority, 10);
        assert!(cap.extends.contains(&"base".to_string()));
    }

    #[test]
    fn test_get_builtin_nonexistent() {
        assert!(get_builtin("nonexistent").is_none());
        assert!(get_builtin("").is_none());
        assert!(get_builtin("Base").is_none()); // Case sensitive
    }

    #[test]
    fn test_list_builtins() {
        let builtins = list_builtins();
        assert_eq!(builtins.len(), 7);
        assert!(builtins.contains(&"base".to_string()));
        assert!(builtins.contains(&"rust-expert".to_string()));
        assert!(builtins.contains(&"python-senior".to_string()));
        assert!(builtins.contains(&"typescript-expert".to_string()));
        assert!(builtins.contains(&"security-analyst".to_string()));
        assert!(builtins.contains(&"code-reviewer".to_string()));
        assert!(builtins.contains(&"documentation".to_string()));
    }

    #[test]
    fn test_all_builtins_have_system_prompts() {
        for name in list_builtins() {
            let cap = get_builtin(&name).unwrap();
            assert!(
                !cap.system_prompt.is_empty(),
                "Cap {} should have a system prompt",
                name
            );
        }
    }

    #[test]
    fn test_all_builtins_have_descriptions() {
        for name in list_builtins() {
            let cap = get_builtin(&name).unwrap();
            assert!(
                !cap.description.is_empty(),
                "Cap {} should have a description",
                name
            );
        }
    }

    #[test]
    fn test_all_builtins_are_marked_builtin() {
        for name in list_builtins() {
            let cap = get_builtin(&name).unwrap();
            assert!(cap.is_builtin, "Cap {} should be marked as builtin", name);
        }
    }

    #[test]
    fn test_base_has_no_extends() {
        let cap = get_builtin("base").unwrap();
        assert!(
            cap.extends.is_empty(),
            "Base cap should not extend anything"
        );
    }

    #[test]
    fn test_non_base_caps_extend_base() {
        for name in list_builtins() {
            if name != "base" {
                let cap = get_builtin(&name).unwrap();
                assert!(
                    cap.extends.contains(&"base".to_string()),
                    "Cap {} should extend base",
                    name
                );
            }
        }
    }

    #[test]
    fn test_base_system_prompt_content() {
        let cap = get_builtin("base").unwrap();
        assert!(cap.system_prompt.contains("Ted"));
        assert!(cap.system_prompt.contains("coding"));
    }

    #[test]
    fn test_rust_expert_system_prompt_content() {
        let cap = get_builtin("rust-expert").unwrap();
        assert!(cap.system_prompt.contains("Rust"));
        assert!(cap.system_prompt.contains("ownership"));
    }

    #[test]
    fn test_python_senior_system_prompt_content() {
        let cap = get_builtin("python-senior").unwrap();
        assert!(cap.system_prompt.contains("Python"));
        assert!(cap.system_prompt.contains("type hints"));
    }

    #[test]
    fn test_typescript_expert_system_prompt_content() {
        let cap = get_builtin("typescript-expert").unwrap();
        assert!(cap.system_prompt.contains("TypeScript"));
        assert!(cap.system_prompt.contains("Node.js"));
    }

    #[test]
    fn test_security_analyst_system_prompt_content() {
        let cap = get_builtin("security-analyst").unwrap();
        assert!(cap.system_prompt.contains("security"));
        assert!(cap.system_prompt.contains("XSS"));
        assert!(cap.system_prompt.contains("injection"));
    }

    #[test]
    fn test_code_reviewer_system_prompt_content() {
        let cap = get_builtin("code-reviewer").unwrap();
        assert!(cap.system_prompt.contains("review"));
        assert!(cap.system_prompt.contains("Quality"));
    }

    #[test]
    fn test_documentation_system_prompt_content() {
        let cap = get_builtin("documentation").unwrap();
        assert!(cap.system_prompt.contains("documentation"));
        assert!(cap.system_prompt.contains("README"));
    }

    #[test]
    fn test_security_analyst_tool_permissions() {
        let cap = get_builtin("security-analyst").unwrap();
        assert!(cap.tool_permissions.require_edit_confirmation);
        assert!(cap.tool_permissions.require_shell_confirmation);
        assert!(cap.tool_permissions.blocked_commands.len() >= 4);
    }

    #[test]
    fn test_priority_ordering() {
        let base = get_builtin("base").unwrap();
        let rust = get_builtin("rust-expert").unwrap();
        let security = get_builtin("security-analyst").unwrap();

        // Base should have lowest priority
        assert!(base.priority < rust.priority);
        assert!(base.priority < security.priority);
        // Security analyst has highest priority of the experts
        assert!(security.priority > rust.priority);
    }
}
