// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Slash command execution
//!
//! Implements execution logic for development slash commands like /commit, /test,
//! /review, /fix, and /explain. Commands can execute shell commands, send messages
//! to the LLM, or spawn specialized agents.

use std::path::Path;

use super::commands::{CommitArgs, ExplainArgs, FixArgs, ReviewArgs, TestArgs};

/// Result of executing a slash command
#[derive(Debug, Clone)]
pub enum SlashCommandResult {
    /// Command completed, show this message to user
    Message(String),
    /// Command needs to send a prefixed message to LLM
    SendToLlm(String),
    /// Command spawns an agent task
    SpawnAgent {
        agent_type: String,
        task: String,
        skill: Option<String>,
    },
    /// Command failed with error
    Error(String),
}

/// Execute /commit command
pub fn execute_commit(args: &CommitArgs, _working_dir: &Path) -> SlashCommandResult {
    // If message provided, use it directly
    if let Some(ref msg) = args.message {
        let files_arg = if args.files.is_empty() {
            ".".to_string()
        } else {
            args.files.join(" ")
        };

        let amend = if args.amend { " --amend" } else { "" };

        return SlashCommandResult::SendToLlm(format!(
            "Please execute these git commands to commit with the provided message:\n\
             1. `git add {}`\n\
             2. `git commit{} -m \"{}\"`\n\n\
             Show me the result of each command.",
            files_arg, amend, msg
        ));
    }

    // No message - ask LLM to generate one
    let amend_note = if args.amend {
        " This should amend the previous commit."
    } else {
        ""
    };

    let files_note = if args.files.is_empty() {
        String::new()
    } else {
        format!(" Focus on these files: {}", args.files.join(", "))
    };

    SlashCommandResult::SendToLlm(format!(
        "Analyze the current git diff and staged changes.{}{}\n\n\
         1. First, run `git status` and `git diff --cached` to see what will be committed\n\
         2. Generate an appropriate commit message following conventional commits format\n\
         3. Stage all relevant changes with `git add`\n\
         4. Create the commit\n\n\
         Show me what you committed.",
        files_note, amend_note
    ))
}

/// Execute /test command
pub fn execute_test(args: &TestArgs, working_dir: &Path) -> SlashCommandResult {
    // Detect project type and build test command
    let test_cmd = detect_test_command(working_dir);

    let mut cmd_parts = vec![test_cmd];

    if args.watch {
        cmd_parts.push("--watch".to_string());
    }
    if args.coverage {
        cmd_parts.push("--coverage".to_string());
    }
    if let Some(ref pattern) = args.pattern {
        cmd_parts.push(pattern.clone());
    }

    let full_cmd = cmd_parts.join(" ");

    SlashCommandResult::SendToLlm(format!(
        "Run the project tests using this command: `{}`\n\n\
         If tests fail:\n\
         1. Analyze the failure output\n\
         2. Identify the root cause\n\
         3. Suggest specific fixes\n\n\
         Show me the test results.",
        full_cmd
    ))
}

/// Execute /review command
pub fn execute_review(args: &ReviewArgs, _working_dir: &Path) -> SlashCommandResult {
    let task = match &args.target {
        Some(target) if target.contains("github.com") || target.contains("/pull/") => {
            format!(
                "Review the pull request at {}. \
                 Focus on code quality, potential bugs, security issues, and best practices.",
                target
            )
        }
        Some(target) if target.parse::<u32>().is_ok() => {
            format!(
                "Review pull request #{} in this repository. \
                 Use `gh pr view {}` to get the PR details and `gh pr diff {}` to see the changes. \
                 Focus on code quality, potential bugs, security issues, and best practices.",
                target, target, target
            )
        }
        Some(path) => {
            format!(
                "Review the code in `{}`. \
                 Focus on code quality, potential bugs, security issues, and best practices.",
                path
            )
        }
        None => "Review the current uncommitted changes using `git diff`. \
             Focus on code quality, potential bugs, security issues, and best practices. \
             Provide actionable feedback for each issue found."
            .to_string(),
    };

    let focus_addendum = args
        .focus
        .as_ref()
        .map(|f| format!(" Pay special attention to {} concerns.", f))
        .unwrap_or_default();

    SlashCommandResult::SpawnAgent {
        agent_type: "review".to_string(),
        task: format!(
            "{}{}\n\n\
             For each issue found, provide:\n\
             - Location (file and line if applicable)\n\
             - Severity (Critical/High/Medium/Low)\n\
             - Description of the issue\n\
             - Suggested fix",
            task, focus_addendum
        ),
        skill: Some("code-review".to_string()),
    }
}

/// Execute /fix command
pub fn execute_fix(args: &FixArgs, working_dir: &Path) -> SlashCommandResult {
    let fix_type = args.fix_type.as_deref().unwrap_or("all");
    let pattern = args.pattern.as_deref().unwrap_or(".");

    // Detect linting/type checking commands based on project type
    let (lint_cmd, type_cmd) = detect_check_commands(working_dir);

    let task = match fix_type {
        "lint" => format!(
            "Run the linter on `{}` using `{}`.\n\
             1. Show me all linting errors found\n\
             2. Fix each error\n\
             3. Explain what you fixed and why\n\
             4. Re-run the linter to verify all issues are resolved",
            pattern, lint_cmd
        ),
        "types" => format!(
            "Run the type checker on `{}` using `{}`.\n\
             1. Show me all type errors found\n\
             2. Fix each error with proper type annotations\n\
             3. Explain what you fixed and why\n\
             4. Re-run the type checker to verify all issues are resolved",
            pattern, type_cmd
        ),
        _ => format!(
            "Run both the linter (`{}`) and type checker (`{}`) on `{}`.\n\
             1. Show me all errors found\n\
             2. Fix each error\n\
             3. Explain what you fixed and why\n\
             4. Re-run both tools to verify all issues are resolved",
            lint_cmd, type_cmd, pattern
        ),
    };

    SlashCommandResult::SpawnAgent {
        agent_type: "implement".to_string(),
        task,
        skill: None,
    }
}

/// Execute /explain command
pub fn execute_explain(args: &ExplainArgs) -> SlashCommandResult {
    let verbosity_instruction = match args.verbosity.as_deref() {
        Some("brief") => "Provide a brief, concise explanation (2-3 paragraphs max).",
        Some("detailed") => {
            "Provide a detailed, comprehensive explanation with examples and edge cases."
        }
        _ => "Provide a clear, helpful explanation.",
    };

    let message = match &args.target {
        Some(target) => format!(
            "Please explain the code in `{}`.\n\n\
             {}\n\n\
             Cover:\n\
             - What the code does (purpose)\n\
             - How it works (logic flow)\n\
             - Key patterns or techniques used\n\
             - Any notable aspects or potential gotchas",
            target, verbosity_instruction
        ),
        None => format!(
            "Please explain the most recently discussed code or the current file in context.\n\n\
             {}\n\n\
             Cover:\n\
             - What the code does (purpose)\n\
             - How it works (logic flow)\n\
             - Key patterns or techniques used\n\
             - Any notable aspects or potential gotchas",
            verbosity_instruction
        ),
    };

    SlashCommandResult::SendToLlm(message)
}

// === Helper Functions ===

/// Detect the appropriate test command for the project
fn detect_test_command(working_dir: &Path) -> String {
    // Check for various project files to determine the appropriate test command
    if working_dir.join("Cargo.toml").exists() {
        return "cargo test".to_string();
    }
    if working_dir.join("package.json").exists() {
        // Could check for specific test runners in package.json
        return "npm test".to_string();
    }
    if working_dir.join("pyproject.toml").exists() || working_dir.join("setup.py").exists() {
        return "pytest".to_string();
    }
    if working_dir.join("go.mod").exists() {
        return "go test ./...".to_string();
    }
    if working_dir.join("Gemfile").exists() {
        return "bundle exec rspec".to_string();
    }
    if working_dir.join("Makefile").exists() {
        return "make test".to_string();
    }

    // Default fallback
    "echo 'No test command detected - please specify your test command'".to_string()
}

/// Detect linting and type checking commands for the project
fn detect_check_commands(working_dir: &Path) -> (String, String) {
    if working_dir.join("Cargo.toml").exists() {
        return ("cargo clippy".to_string(), "cargo check".to_string());
    }
    if working_dir.join("package.json").exists() {
        return ("npm run lint".to_string(), "npm run typecheck".to_string());
    }
    if working_dir.join("pyproject.toml").exists() || working_dir.join("setup.py").exists() {
        return ("ruff check .".to_string(), "mypy .".to_string());
    }
    if working_dir.join("go.mod").exists() {
        return ("golangci-lint run".to_string(), "go vet ./...".to_string());
    }

    // Default fallback
    (
        "echo 'No linter detected'".to_string(),
        "echo 'No type checker detected'".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_execute_commit_no_args() {
        let args = CommitArgs::default();
        let temp = TempDir::new().unwrap();
        let result = execute_commit(&args, temp.path());

        match result {
            SlashCommandResult::SendToLlm(msg) => {
                assert!(msg.contains("git diff"));
                assert!(msg.contains("conventional commits"));
            }
            _ => panic!("Expected SendToLlm"),
        }
    }

    #[test]
    fn test_execute_commit_with_message() {
        let args = CommitArgs {
            message: Some("fix: resolve bug".to_string()),
            amend: false,
            files: vec![],
        };
        let temp = TempDir::new().unwrap();
        let result = execute_commit(&args, temp.path());

        match result {
            SlashCommandResult::SendToLlm(msg) => {
                assert!(msg.contains("fix: resolve bug"));
                assert!(msg.contains("git add"));
                assert!(msg.contains("git commit"));
            }
            _ => panic!("Expected SendToLlm"),
        }
    }

    #[test]
    fn test_execute_commit_amend() {
        let args = CommitArgs {
            message: None,
            amend: true,
            files: vec![],
        };
        let temp = TempDir::new().unwrap();
        let result = execute_commit(&args, temp.path());

        match result {
            SlashCommandResult::SendToLlm(msg) => {
                assert!(msg.contains("amend"));
            }
            _ => panic!("Expected SendToLlm"),
        }
    }

    #[test]
    fn test_execute_test_basic() {
        let args = TestArgs::default();
        let temp = TempDir::new().unwrap();
        let result = execute_test(&args, temp.path());

        match result {
            SlashCommandResult::SendToLlm(msg) => {
                assert!(msg.contains("Run the project tests"));
            }
            _ => panic!("Expected SendToLlm"),
        }
    }

    #[test]
    fn test_execute_test_with_options() {
        let args = TestArgs {
            watch: true,
            coverage: true,
            pattern: Some("auth".to_string()),
        };
        let temp = TempDir::new().unwrap();
        let result = execute_test(&args, temp.path());

        match result {
            SlashCommandResult::SendToLlm(msg) => {
                assert!(msg.contains("--watch"));
                assert!(msg.contains("--coverage"));
                assert!(msg.contains("auth"));
            }
            _ => panic!("Expected SendToLlm"),
        }
    }

    #[test]
    fn test_execute_review_no_target() {
        let args = ReviewArgs::default();
        let temp = TempDir::new().unwrap();
        let result = execute_review(&args, temp.path());

        match result {
            SlashCommandResult::SpawnAgent { task, skill, .. } => {
                assert!(task.contains("git diff"));
                assert_eq!(skill, Some("code-review".to_string()));
            }
            _ => panic!("Expected SpawnAgent"),
        }
    }

    #[test]
    fn test_execute_review_pr_number() {
        let args = ReviewArgs {
            target: Some("123".to_string()),
            focus: None,
        };
        let temp = TempDir::new().unwrap();
        let result = execute_review(&args, temp.path());

        match result {
            SlashCommandResult::SpawnAgent { task, .. } => {
                assert!(task.contains("123"));
                assert!(task.contains("gh pr"));
            }
            _ => panic!("Expected SpawnAgent"),
        }
    }

    #[test]
    fn test_execute_fix_lint() {
        let args = FixArgs {
            fix_type: Some("lint".to_string()),
            pattern: Some("src/".to_string()),
        };
        let temp = TempDir::new().unwrap();
        let result = execute_fix(&args, temp.path());

        match result {
            SlashCommandResult::SpawnAgent { task, .. } => {
                assert!(task.contains("linter"));
                assert!(task.contains("src/"));
            }
            _ => panic!("Expected SpawnAgent"),
        }
    }

    #[test]
    fn test_execute_explain_basic() {
        let args = ExplainArgs::default();
        let result = execute_explain(&args);

        match result {
            SlashCommandResult::SendToLlm(msg) => {
                assert!(msg.contains("explain"));
            }
            _ => panic!("Expected SendToLlm"),
        }
    }

    #[test]
    fn test_execute_explain_with_target() {
        let args = ExplainArgs {
            target: Some("src/main.rs".to_string()),
            verbosity: Some("detailed".to_string()),
        };
        let result = execute_explain(&args);

        match result {
            SlashCommandResult::SendToLlm(msg) => {
                assert!(msg.contains("src/main.rs"));
                assert!(msg.contains("detailed"));
            }
            _ => panic!("Expected SendToLlm"),
        }
    }

    #[test]
    fn test_detect_test_command_rust() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("Cargo.toml"), "[package]").unwrap();

        let cmd = detect_test_command(temp.path());
        assert_eq!(cmd, "cargo test");
    }

    #[test]
    fn test_detect_test_command_node() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("package.json"), "{}").unwrap();

        let cmd = detect_test_command(temp.path());
        assert_eq!(cmd, "npm test");
    }

    #[test]
    fn test_detect_test_command_python() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("pyproject.toml"), "").unwrap();

        let cmd = detect_test_command(temp.path());
        assert_eq!(cmd, "pytest");
    }

    #[test]
    fn test_detect_check_commands_rust() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("Cargo.toml"), "[package]").unwrap();

        let (lint, types) = detect_check_commands(temp.path());
        assert_eq!(lint, "cargo clippy");
        assert_eq!(types, "cargo check");
    }
}
