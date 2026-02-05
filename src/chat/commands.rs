// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Command handling for chat interface
//!
//! This module provides structures and functions for processing slash commands
//! in the chat interface, with testable command routing logic.

use super::input_parser;

// === Slash Command Argument Structs ===

/// Arguments for /commit command
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitArgs {
    /// Optional manual commit message (-m flag)
    pub message: Option<String>,
    /// Whether to amend the last commit (--amend flag)
    pub amend: bool,
    /// Specific files to commit
    pub files: Vec<String>,
}

/// Arguments for /test command
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestArgs {
    /// Watch mode (--watch flag)
    pub watch: bool,
    /// Specific test pattern or file
    pub pattern: Option<String>,
    /// Coverage mode (--coverage flag)
    pub coverage: bool,
}

/// Arguments for /review command
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReviewArgs {
    /// PR number, URL, or path to review
    pub target: Option<String>,
    /// Focus area (security, performance, etc.)
    pub focus: Option<String>,
}

/// Arguments for /fix command
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FixArgs {
    /// Type of fixes (lint, types, all)
    pub fix_type: Option<String>,
    /// Specific file or pattern
    pub pattern: Option<String>,
}

/// Arguments for /explain command
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExplainArgs {
    /// Target to explain (file path, code selection, or inline code)
    pub target: Option<String>,
    /// Verbosity level (brief, detailed)
    pub verbosity: Option<String>,
}

/// Arguments for /skills command
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SkillsArgs {
    /// Subcommand: None (list), "list", "show", "create"
    pub subcommand: Option<String>,
    /// Skill name for show/create
    pub name: Option<String>,
}

/// Arguments for /beads command
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BeadsArgs {
    /// Subcommand: None (list), "list", "add", "show", "status", "stats"
    pub subcommand: Option<String>,
    /// Bead ID for show/status commands
    pub id: Option<String>,
    /// Title for add command or status value for status command
    pub value: Option<String>,
}

/// Represents the different types of commands that can be issued in chat
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatCommand {
    /// Exit the chat session
    Exit,
    /// Clear the conversation context
    Clear,
    /// Show help information
    Help,
    /// Show context/stats information
    Stats,
    /// Open settings interface
    Settings,
    /// List recent sessions
    Sessions,
    /// Start a new session
    New,
    /// Open plans browser
    Plans,
    /// List plans
    PlansList,
    /// Show current model or available models
    Model,
    /// Switch to a different model
    ModelSwitch(String),
    /// Show active caps
    Caps,
    /// Add a cap
    CapAdd(String),
    /// Remove a cap
    CapRemove(String),
    /// Set caps (replace all)
    CapSet(Vec<String>),
    /// Clear all caps
    CapClear,
    /// Create a new cap
    CapCreate(String),
    /// List available caps
    CapList,
    /// Switch to a different session
    Switch(String),
    /// Execute a shell command
    Shell(String),
    /// Regular user message (not a command)
    Message(String),
    /// Empty input
    Empty,
    /// Unknown slash command
    Unknown(String),

    // === Development Slash Commands ===
    /// Stage and commit changes with AI-generated message
    Commit(CommitArgs),
    /// Run project tests
    Test(TestArgs),
    /// Review code changes or PR
    Review(ReviewArgs),
    /// Fix linting/type errors
    Fix(FixArgs),
    /// Explain code or file
    Explain(ExplainArgs),

    // === Skills & Beads Commands ===
    /// Manage skills (list, show, create)
    Skills(SkillsArgs),
    /// Manage beads for task tracking
    Beads(BeadsArgs),
}

/// Parse user input into a ChatCommand
pub fn parse_command(input: &str) -> ChatCommand {
    let trimmed = input.trim();

    // Check for empty input
    if trimmed.is_empty() {
        return ChatCommand::Empty;
    }

    // Check for shell command
    if let Some(cmd) = input_parser::parse_shell_command(trimmed) {
        if cmd.is_empty() {
            return ChatCommand::Shell(String::new());
        }
        return ChatCommand::Shell(cmd.to_string());
    }

    // Check for exit commands
    if input_parser::is_exit_command(trimmed) {
        return ChatCommand::Exit;
    }

    // Check for clear command
    if input_parser::is_clear_command(trimmed) {
        return ChatCommand::Clear;
    }

    // Check for help command
    if input_parser::is_help_command(trimmed) {
        return ChatCommand::Help;
    }

    // Check for stats command
    if input_parser::is_stats_command(trimmed) {
        return ChatCommand::Stats;
    }

    // Check for settings command
    if input_parser::is_settings_command(trimmed) {
        return ChatCommand::Settings;
    }

    // Check for sessions command
    if input_parser::is_sessions_command(trimmed) {
        return ChatCommand::Sessions;
    }

    // Check for new command
    if input_parser::is_new_command(trimmed) {
        return ChatCommand::New;
    }

    // Check for plans commands
    let lower = trimmed.to_lowercase();
    if lower == "/plans list" || lower == "/plan list" {
        return ChatCommand::PlansList;
    }
    if input_parser::is_plans_command(trimmed) {
        return ChatCommand::Plans;
    }

    // Check for model commands
    if let Some(model_name) = input_parser::parse_model_switch_command(trimmed) {
        return ChatCommand::ModelSwitch(model_name.to_string());
    }
    if input_parser::is_model_command(trimmed) {
        return ChatCommand::Model;
    }

    // Check for caps command
    if input_parser::is_caps_command(trimmed) {
        return ChatCommand::Caps;
    }

    // Check for cap subcommands
    if let Some((action, arg)) = input_parser::parse_cap_command(trimmed) {
        return match action.to_lowercase().as_str() {
            "add" => match arg {
                Some(name) => ChatCommand::CapAdd(name.to_string()),
                None => ChatCommand::Unknown("/cap add".to_string()),
            },
            "remove" => match arg {
                Some(name) => ChatCommand::CapRemove(name.to_string()),
                None => ChatCommand::Unknown("/cap remove".to_string()),
            },
            "set" => match arg {
                Some(names) => ChatCommand::CapSet(input_parser::parse_cap_names(names)),
                None => ChatCommand::Unknown("/cap set".to_string()),
            },
            "clear" => ChatCommand::CapClear,
            "create" => match arg {
                Some(name) => ChatCommand::CapCreate(name.to_string()),
                None => ChatCommand::Unknown("/cap create".to_string()),
            },
            "list" => ChatCommand::CapList,
            _ => ChatCommand::Unknown(format!("/cap {}", action)),
        };
    }

    // Check for switch command
    if let Some(session_id) = input_parser::parse_switch_command(trimmed) {
        if session_id.is_empty() {
            return ChatCommand::Unknown("/switch".to_string());
        }
        return ChatCommand::Switch(session_id.to_string());
    }

    // Check for development slash commands
    if let Some(args) = input_parser::parse_commit_command(trimmed) {
        return ChatCommand::Commit(args);
    }
    if let Some(args) = input_parser::parse_test_command(trimmed) {
        return ChatCommand::Test(args);
    }
    if let Some(args) = input_parser::parse_review_command(trimmed) {
        return ChatCommand::Review(args);
    }
    if let Some(args) = input_parser::parse_fix_command(trimmed) {
        return ChatCommand::Fix(args);
    }
    if let Some(args) = input_parser::parse_explain_command(trimmed) {
        return ChatCommand::Explain(args);
    }
    if let Some(args) = input_parser::parse_skills_command(trimmed) {
        return ChatCommand::Skills(args);
    }
    if let Some(args) = input_parser::parse_beads_command(trimmed) {
        return ChatCommand::Beads(args);
    }

    // Check for unknown slash command
    if input_parser::is_slash_command(trimmed) {
        return ChatCommand::Unknown(trimmed.to_string());
    }

    // Regular message
    ChatCommand::Message(trimmed.to_string())
}

/// Represents the result of executing a command
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResult {
    /// Command completed successfully
    Success,
    /// Command completed with a message to display
    SuccessWithMessage(String),
    /// Continue to next iteration of chat loop
    Continue,
    /// Exit the chat loop
    Exit,
    /// Error occurred
    Error(String),
    /// Command needs to process message with LLM
    ProcessMessage(String),
    /// Command requires interactive handling (can't be unit tested easily)
    RequiresInteractive,
}

/// Validate a model name against the known valid models
pub fn validate_model(model: &str) -> Result<(), String> {
    if input_parser::is_valid_model(model) {
        Ok(())
    } else {
        Err(format!(
            "Unknown model '{}'. Run /model to see available models.",
            model
        ))
    }
}

/// Validate a session identifier format
pub fn validate_session_id(session_id: &str) -> Result<(), String> {
    if session_id.is_empty() {
        return Err("Session ID cannot be empty".to_string());
    }

    // Allow short IDs (partial UUIDs) or full UUIDs
    if session_id.len() > 36 {
        return Err("Session ID is too long".to_string());
    }

    // Check for valid UUID characters
    let valid_chars = session_id
        .chars()
        .all(|c| c.is_ascii_hexdigit() || c == '-');
    if !valid_chars {
        return Err("Session ID contains invalid characters".to_string());
    }

    Ok(())
}

/// Format help text for display
pub fn format_help_text() -> String {
    r#"Ted Commands:

Session & Navigation:
  /help       - Show this help message
  /clear      - Clear conversation context
  /stats      - Show context statistics
  /settings   - Open settings interface
  /sessions   - List recent sessions
  /new        - Start a new session
  /switch <id> - Switch to a different session
  exit, quit  - Exit Ted

Development Commands:
  /commit [-m "msg"] [--amend]  - Commit changes with AI message
  /test [--watch] [pattern]    - Run project tests
  /review [target] [--focus X] - Review changes or PR
  /fix [lint|types|all]        - Fix linting/type errors
  /explain [file] [--brief]    - Explain code

Skills & Beads:
  /skills              - List available skills
  /skills show <name>  - Display skill content
  /skills create <name> - Create a new skill (interactive)
  /beads               - List all beads (task tracking)
  /beads add <title>   - Create a new bead
  /beads show <id>     - Show bead details
  /beads status <id> <status> - Update bead status
  /beads stats         - Show bead statistics

Model & Capabilities:
  /model      - Show current and available models
  /model <name> - Switch to a different model
  /caps       - Show active caps
  /cap add <name>    - Add a cap
  /cap remove <name> - Remove a cap
  /cap set <names>   - Set caps (comma-separated)
  /cap clear  - Remove all caps
  /cap create <name> - Create a new cap
  /cap list   - List available caps

Plans:
  /plans      - Open plans browser
  /plans list - List all plans

Shell Commands:
  >command    - Execute a shell command directly
  Example: >git status

Tips:
  - Use Ctrl+C to interrupt a running command
  - Context is automatically trimmed when needed"#
        .to_string()
}

/// Format statistics display
pub struct ContextStats {
    pub session_id: String,
    pub model: String,
    pub message_count: usize,
    pub total_chunks: usize,
    pub hot_chunks: usize,
    pub warm_chunks: usize,
    pub cold_chunks: usize,
    pub total_tokens: usize,
    pub storage_bytes: usize,
    pub caps: Vec<String>,
    pub system_prompt_len: usize,
    pub has_file_tree: bool,
}

/// Format context statistics for display
pub fn format_stats(stats: &ContextStats) -> String {
    let mut output = String::new();
    output.push_str("Context Statistics\n");
    output.push_str("─────────────────────────────────────\n");
    output.push_str(&format!(
        "  Session ID:      {}\n",
        &stats.session_id[..8.min(stats.session_id.len())]
    ));
    output.push_str(&format!("  Model:           {}\n", stats.model));
    output.push_str(&format!("  Messages:        {}\n", stats.message_count));
    output.push('\n');
    output.push_str("  Storage:\n");
    output.push_str(&format!("    Total chunks:  {}\n", stats.total_chunks));
    output.push_str(&format!("    Hot (cache):   {}\n", stats.hot_chunks));
    output.push_str(&format!("    Warm (disk):   {}\n", stats.warm_chunks));
    output.push_str(&format!("    Cold (archive):{}\n", stats.cold_chunks));
    output.push('\n');
    output.push_str(&format!("  Tokens:          ~{}\n", stats.total_tokens));
    if stats.storage_bytes > 0 {
        let kb = stats.storage_bytes as f64 / 1024.0;
        output.push_str(&format!("  Storage:         {:.1} KB\n", kb));
    }
    output.push('\n');
    output.push_str("  Active caps:     ");
    if stats.caps.is_empty() {
        output.push_str("(none)\n");
    } else {
        output.push_str(&stats.caps.join(", "));
        output.push('\n');
    }
    output.push('\n');
    output.push_str(&format!(
        "  System prompt:   {} chars\n",
        stats.system_prompt_len
    ));
    if stats.has_file_tree {
        output.push_str("  File tree:       loaded\n");
    } else {
        output.push_str("  File tree:       not loaded\n");
    }
    output.push_str("─────────────────────────────────────");
    output
}

/// Format available models for display
pub fn format_available_models(current_model: &str) -> String {
    let mut output = String::new();
    output.push_str(&format!("Current model: {}\n\n", current_model));
    output.push_str("Available models:\n");
    output.push_str("  claude-sonnet-4-20250514    - Best quality, moderate rate limits\n");
    output.push_str("  claude-3-5-sonnet-20241022  - Previous Sonnet, good balance\n");
    output.push_str("  claude-3-5-haiku-20241022   - Fastest, highest rate limits, cheapest\n");
    output.push_str("\nTo switch: /model <name> or use -m flag when starting ted\n");
    output.push_str("Example: /model claude-3-5-haiku-20241022");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== parse_command tests ====================

    #[test]
    fn test_parse_command_empty() {
        assert_eq!(parse_command(""), ChatCommand::Empty);
        assert_eq!(parse_command("   "), ChatCommand::Empty);
        assert_eq!(parse_command("\n"), ChatCommand::Empty);
    }

    #[test]
    fn test_parse_command_exit() {
        assert_eq!(parse_command("exit"), ChatCommand::Exit);
        assert_eq!(parse_command("quit"), ChatCommand::Exit);
        assert_eq!(parse_command("/exit"), ChatCommand::Exit);
        assert_eq!(parse_command("/quit"), ChatCommand::Exit);
        assert_eq!(parse_command("EXIT"), ChatCommand::Exit);
    }

    #[test]
    fn test_parse_command_clear() {
        assert_eq!(parse_command("/clear"), ChatCommand::Clear);
        assert_eq!(parse_command("/CLEAR"), ChatCommand::Clear);
    }

    #[test]
    fn test_parse_command_help() {
        assert_eq!(parse_command("/help"), ChatCommand::Help);
        assert_eq!(parse_command("/HELP"), ChatCommand::Help);
    }

    #[test]
    fn test_parse_command_stats() {
        assert_eq!(parse_command("/stats"), ChatCommand::Stats);
        assert_eq!(parse_command("/context"), ChatCommand::Stats);
    }

    #[test]
    fn test_parse_command_settings() {
        assert_eq!(parse_command("/settings"), ChatCommand::Settings);
        assert_eq!(parse_command("/config"), ChatCommand::Settings);
    }

    #[test]
    fn test_parse_command_sessions() {
        assert_eq!(parse_command("/sessions"), ChatCommand::Sessions);
        assert_eq!(parse_command("/session"), ChatCommand::Sessions);
    }

    #[test]
    fn test_parse_command_new() {
        assert_eq!(parse_command("/new"), ChatCommand::New);
    }

    #[test]
    fn test_parse_command_plans() {
        assert_eq!(parse_command("/plans"), ChatCommand::Plans);
        assert_eq!(parse_command("/plan"), ChatCommand::Plans);
        assert_eq!(parse_command("/plans list"), ChatCommand::PlansList);
        assert_eq!(parse_command("/plan list"), ChatCommand::PlansList);
    }

    #[test]
    fn test_parse_command_model() {
        assert_eq!(parse_command("/model"), ChatCommand::Model);
        assert_eq!(parse_command("/models"), ChatCommand::Model);
        assert_eq!(
            parse_command("/model claude-3-5-haiku-20241022"),
            ChatCommand::ModelSwitch("claude-3-5-haiku-20241022".to_string())
        );
    }

    #[test]
    fn test_parse_command_caps() {
        assert_eq!(parse_command("/caps"), ChatCommand::Caps);
    }

    #[test]
    fn test_parse_command_cap_add() {
        assert_eq!(
            parse_command("/cap add mycode"),
            ChatCommand::CapAdd("mycode".to_string())
        );
    }

    #[test]
    fn test_parse_command_cap_remove() {
        assert_eq!(
            parse_command("/cap remove mycode"),
            ChatCommand::CapRemove("mycode".to_string())
        );
    }

    #[test]
    fn test_parse_command_cap_set() {
        assert_eq!(
            parse_command("/cap set base,code"),
            ChatCommand::CapSet(vec!["base".to_string(), "code".to_string()])
        );
    }

    #[test]
    fn test_parse_command_cap_clear() {
        assert_eq!(parse_command("/cap clear"), ChatCommand::CapClear);
    }

    #[test]
    fn test_parse_command_cap_create() {
        assert_eq!(
            parse_command("/cap create newcap"),
            ChatCommand::CapCreate("newcap".to_string())
        );
    }

    #[test]
    fn test_parse_command_cap_list() {
        assert_eq!(parse_command("/cap list"), ChatCommand::CapList);
    }

    #[test]
    fn test_parse_command_cap_missing_arg() {
        assert_eq!(
            parse_command("/cap add"),
            ChatCommand::Unknown("/cap add".to_string())
        );
        assert_eq!(
            parse_command("/cap remove"),
            ChatCommand::Unknown("/cap remove".to_string())
        );
        assert_eq!(
            parse_command("/cap set"),
            ChatCommand::Unknown("/cap set".to_string())
        );
    }

    #[test]
    fn test_parse_command_switch() {
        assert_eq!(
            parse_command("/switch abc123"),
            ChatCommand::Switch("abc123".to_string())
        );
        assert_eq!(
            parse_command("/switch 1"),
            ChatCommand::Switch("1".to_string())
        );
    }

    #[test]
    fn test_parse_command_switch_missing_arg() {
        // /switch without argument should be handled
        match parse_command("/switch ") {
            ChatCommand::Unknown(_) | ChatCommand::Switch(_) => {}
            other => panic!("Expected Unknown or Switch, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_command_shell() {
        assert_eq!(
            parse_command(">ls -la"),
            ChatCommand::Shell("ls -la".to_string())
        );
        assert_eq!(
            parse_command("> git status"),
            ChatCommand::Shell("git status".to_string())
        );
        assert_eq!(parse_command(">"), ChatCommand::Shell(String::new()));
    }

    #[test]
    fn test_parse_command_message() {
        assert_eq!(
            parse_command("Hello, how are you?"),
            ChatCommand::Message("Hello, how are you?".to_string())
        );
        assert_eq!(
            parse_command("Write some code"),
            ChatCommand::Message("Write some code".to_string())
        );
    }

    #[test]
    fn test_parse_command_unknown_slash() {
        assert_eq!(
            parse_command("/unknown"),
            ChatCommand::Unknown("/unknown".to_string())
        );
        assert_eq!(
            parse_command("/foobar arg"),
            ChatCommand::Unknown("/foobar arg".to_string())
        );
    }

    // ==================== validate_model tests ====================

    #[test]
    fn test_validate_model_valid() {
        assert!(validate_model("claude-sonnet-4-20250514").is_ok());
        assert!(validate_model("claude-3-5-sonnet-20241022").is_ok());
        assert!(validate_model("claude-3-5-haiku-20241022").is_ok());
    }

    #[test]
    fn test_validate_model_invalid() {
        let result = validate_model("gpt-4");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown model"));
    }

    // ==================== validate_session_id tests ====================

    #[test]
    fn test_validate_session_id_valid() {
        assert!(validate_session_id("abc123").is_ok());
        assert!(validate_session_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn test_validate_session_id_empty() {
        let result = validate_session_id("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_validate_session_id_too_long() {
        let long_id = "a".repeat(50);
        let result = validate_session_id(&long_id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too long"));
    }

    #[test]
    fn test_validate_session_id_invalid_chars() {
        let result = validate_session_id("abc!@#$");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));
    }

    // ==================== format_help_text tests ====================

    #[test]
    fn test_format_help_text_contains_commands() {
        let help = format_help_text();
        assert!(help.contains("/help"));
        assert!(help.contains("/clear"));
        assert!(help.contains("/stats"));
        assert!(help.contains("/settings"));
        assert!(help.contains("/sessions"));
        assert!(help.contains("/new"));
        assert!(help.contains("/switch"));
        assert!(help.contains("/model"));
        assert!(help.contains("/caps"));
        assert!(help.contains("/plans"));
        assert!(help.contains("exit"));
        assert!(help.contains(">command"));
    }

    // ==================== format_stats tests ====================

    #[test]
    fn test_format_stats_basic() {
        let stats = ContextStats {
            session_id: "12345678-1234-1234-1234-123456789abc".to_string(),
            model: "claude-3-5-sonnet".to_string(),
            message_count: 10,
            total_chunks: 5,
            hot_chunks: 3,
            warm_chunks: 1,
            cold_chunks: 1,
            total_tokens: 5000,
            storage_bytes: 10240,
            caps: vec!["base".to_string(), "code".to_string()],
            system_prompt_len: 500,
            has_file_tree: true,
        };

        let output = format_stats(&stats);
        assert!(output.contains("12345678")); // Short ID
        assert!(output.contains("claude-3-5-sonnet"));
        assert!(output.contains("10")); // message count
        assert!(output.contains("5000")); // tokens
        assert!(output.contains("base, code"));
        assert!(output.contains("loaded"));
    }

    #[test]
    fn test_format_stats_no_caps() {
        let stats = ContextStats {
            session_id: "abcdef12".to_string(),
            model: "test-model".to_string(),
            message_count: 0,
            total_chunks: 0,
            hot_chunks: 0,
            warm_chunks: 0,
            cold_chunks: 0,
            total_tokens: 0,
            storage_bytes: 0,
            caps: vec![],
            system_prompt_len: 0,
            has_file_tree: false,
        };

        let output = format_stats(&stats);
        assert!(output.contains("(none)"));
        assert!(output.contains("not loaded"));
    }

    // ==================== format_available_models tests ====================

    #[test]
    fn test_format_available_models() {
        let output = format_available_models("claude-3-5-sonnet-20241022");
        assert!(output.contains("Current model: claude-3-5-sonnet-20241022"));
        assert!(output.contains("claude-sonnet-4-20250514"));
        assert!(output.contains("claude-3-5-haiku-20241022"));
    }

    // ==================== ChatCommand enum tests ====================

    #[test]
    fn test_chat_command_debug() {
        let cmd = ChatCommand::Exit;
        let debug = format!("{:?}", cmd);
        assert!(debug.contains("Exit"));
    }

    #[test]
    fn test_chat_command_clone() {
        let cmd = ChatCommand::Model;
        let cloned = cmd.clone();
        assert_eq!(cmd, cloned);
    }

    #[test]
    fn test_chat_command_eq() {
        assert_eq!(ChatCommand::Exit, ChatCommand::Exit);
        assert_ne!(ChatCommand::Exit, ChatCommand::Clear);
    }

    #[test]
    fn test_chat_command_with_data_eq() {
        assert_eq!(
            ChatCommand::Shell("ls".to_string()),
            ChatCommand::Shell("ls".to_string())
        );
        assert_ne!(
            ChatCommand::Shell("ls".to_string()),
            ChatCommand::Shell("pwd".to_string())
        );
    }

    // ==================== CommandResult tests ====================

    #[test]
    fn test_command_result_debug() {
        let result = CommandResult::Success;
        let debug = format!("{:?}", result);
        assert!(debug.contains("Success"));
    }

    #[test]
    fn test_command_result_with_message() {
        let result = CommandResult::SuccessWithMessage("Done!".to_string());
        assert_eq!(
            result,
            CommandResult::SuccessWithMessage("Done!".to_string())
        );
    }

    #[test]
    fn test_command_result_error() {
        let result = CommandResult::Error("Something went wrong".to_string());
        if let CommandResult::Error(msg) = result {
            assert!(msg.contains("wrong"));
        } else {
            panic!("Expected Error variant");
        }
    }

    // ==================== Edge cases ====================

    #[test]
    fn test_parse_command_whitespace_handling() {
        assert_eq!(parse_command("  /help  "), ChatCommand::Help);
        assert_eq!(parse_command("\t/clear\n"), ChatCommand::Clear);
    }

    #[test]
    fn test_parse_command_case_insensitive() {
        assert_eq!(parse_command("/HELP"), ChatCommand::Help);
        assert_eq!(parse_command("/Help"), ChatCommand::Help);
        assert_eq!(parse_command("/hElP"), ChatCommand::Help);
    }

    #[test]
    fn test_parse_command_model_preserves_case() {
        // Model names should preserve their case
        assert_eq!(
            parse_command("/model Claude-3-5-Sonnet"),
            ChatCommand::ModelSwitch("Claude-3-5-Sonnet".to_string())
        );
    }

    #[test]
    fn test_parse_command_shell_preserves_command() {
        // Shell commands should preserve their exact form
        assert_eq!(
            parse_command(">echo 'Hello World'"),
            ChatCommand::Shell("echo 'Hello World'".to_string())
        );
    }

    #[test]
    fn test_parse_command_message_preserves_content() {
        // Messages should preserve their exact content
        let content = "Write a function that\ndoes something complex";
        assert_eq!(
            parse_command(content),
            ChatCommand::Message(content.to_string())
        );
    }

    // ==================== Development slash commands tests ====================

    #[test]
    fn test_parse_command_commit() {
        // Basic commit
        let result = parse_command("/commit");
        assert!(matches!(result, ChatCommand::Commit(_)));

        // Commit with message
        let result = parse_command("/commit -m \"fix typo\"");
        if let ChatCommand::Commit(args) = result {
            assert!(args.message.is_some() || !args.files.is_empty());
        } else {
            panic!("Expected Commit command");
        }
    }

    #[test]
    fn test_parse_command_test() {
        // Basic test
        let result = parse_command("/test");
        assert!(matches!(result, ChatCommand::Test(_)));

        // Test with pattern
        let result = parse_command("/test src/main.rs");
        if let ChatCommand::Test(args) = result {
            assert!(args.pattern.is_some() || args.watch || args.coverage);
        } else {
            panic!("Expected Test command");
        }
    }

    #[test]
    fn test_parse_command_review() {
        // Basic review
        let result = parse_command("/review");
        assert!(matches!(result, ChatCommand::Review(_)));

        // Review with target
        let result = parse_command("/review changes.diff");
        if let ChatCommand::Review(args) = result {
            assert!(args.target.is_some() || args.focus.is_some());
        } else {
            panic!("Expected Review command");
        }
    }

    #[test]
    fn test_parse_command_fix() {
        // Basic fix
        let result = parse_command("/fix");
        assert!(matches!(result, ChatCommand::Fix(_)));

        // Fix with pattern
        let result = parse_command("/fix lint");
        if let ChatCommand::Fix(args) = result {
            assert!(args.fix_type.is_some() || args.pattern.is_some());
        } else {
            panic!("Expected Fix command");
        }
    }

    #[test]
    fn test_parse_command_explain() {
        // Basic explain
        let result = parse_command("/explain");
        assert!(matches!(result, ChatCommand::Explain(_)));

        // Explain with target
        let result = parse_command("/explain this function");
        if let ChatCommand::Explain(args) = result {
            assert!(args.target.is_some() || args.verbosity.is_some());
        } else {
            panic!("Expected Explain command");
        }
    }

    #[test]
    fn test_parse_command_cap_create_without_name() {
        assert_eq!(
            parse_command("/cap create"),
            ChatCommand::Unknown("/cap create".to_string())
        );
    }

    #[test]
    fn test_parse_command_cap_unknown_action() {
        assert_eq!(
            parse_command("/cap unknown_action"),
            ChatCommand::Unknown("/cap unknown_action".to_string())
        );
    }

    #[test]
    fn test_parse_command_switch_empty() {
        assert_eq!(
            parse_command("/switch"),
            ChatCommand::Unknown("/switch".to_string())
        );
    }
}
