// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Input parsing for chat commands
//!
//! This module provides pure functions for parsing and classifying user input
//! in the chat interface. All functions are designed to be easily testable
//! with no side effects.

use crate::llm::provider::ContentBlockResponse;

use crate::beads::BeadStatus;

use super::commands::{
    BeadsArgs, CommitArgs, ExplainArgs, FixArgs, ModelArgs, ReviewArgs, SkillsArgs, TestArgs,
};

/// Parse a shell command from user input.
/// Returns Some(command) if input starts with '>', None otherwise.
/// Returns Some("") if input is just '>'.
pub fn parse_shell_command(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    if trimmed.starts_with('>') {
        Some(trimmed.strip_prefix('>').unwrap().trim())
    } else {
        None
    }
}

/// Truncate a command string for display purposes.
/// Commands longer than max_len are truncated with "...".
pub fn truncate_command_display(command: &str, max_len: usize) -> String {
    if command.len() > max_len {
        format!("{}...", &command[..max_len.saturating_sub(3)])
    } else {
        command.to_string()
    }
}

/// Check if user input is an exit command.
pub fn is_exit_command(input: &str) -> bool {
    let trimmed = input.trim().to_lowercase();
    matches!(trimmed.as_str(), "exit" | "quit" | "/exit" | "/quit")
}

/// Check if user input is a clear command.
pub fn is_clear_command(input: &str) -> bool {
    input.trim().to_lowercase() == "/clear"
}

/// Check if user input is a help command.
pub fn is_help_command(input: &str) -> bool {
    input.trim().to_lowercase() == "/help"
}

/// Check if user input is a stats/context command.
pub fn is_stats_command(input: &str) -> bool {
    let trimmed = input.trim().to_lowercase();
    trimmed == "/stats" || trimmed == "/context"
}

/// Check if user input is a settings command.
pub fn is_settings_command(input: &str) -> bool {
    let trimmed = input.trim().to_lowercase();
    trimmed == "/settings" || trimmed == "/config"
}

/// Check if user input is a sessions command.
pub fn is_sessions_command(input: &str) -> bool {
    let trimmed = input.trim().to_lowercase();
    trimmed == "/sessions" || trimmed == "/session"
}

/// Check if user input is a new session command.
pub fn is_new_command(input: &str) -> bool {
    input.trim().to_lowercase() == "/new"
}

/// Check if user input is a plans command.
pub fn is_plans_command(input: &str) -> bool {
    let trimmed = input.trim().to_lowercase();
    trimmed == "/plans" || trimmed == "/plan"
}

/// Check if user input is a model command.
pub fn is_model_command(input: &str) -> bool {
    let trimmed = input.trim().to_lowercase();
    trimmed == "/model" || trimmed == "/models"
}

/// Check if user input is a caps command.
pub fn is_caps_command(input: &str) -> bool {
    input.trim().to_lowercase() == "/caps"
}

/// Parse a switch command argument.
/// Returns the session identifier if valid.
pub fn parse_switch_command(input: &str) -> Option<&str> {
    let trimmed = input.trim().to_lowercase();
    if trimmed.starts_with("/switch ") {
        Some(input.trim().strip_prefix("/switch ").unwrap_or("").trim())
    } else {
        None
    }
}

/// Parse a model switch command argument.
/// Returns the model name if valid.
pub fn parse_model_switch_command(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    if trimmed.to_lowercase().starts_with("/model ") {
        Some(trimmed.strip_prefix("/model ").unwrap_or("").trim())
    } else {
        None
    }
}

/// Parse a cap command argument.
/// Returns (action, argument) if valid.
pub fn parse_cap_command(input: &str) -> Option<(&str, Option<&str>)> {
    let trimmed = input.trim();
    if !trimmed.to_lowercase().starts_with("/cap ") {
        return None;
    }

    let without_prefix = trimmed.strip_prefix("/cap ").unwrap_or("").trim();
    let parts: Vec<&str> = without_prefix.splitn(2, ' ').collect();

    if parts.is_empty() || parts[0].is_empty() {
        return None;
    }

    let action = parts[0];
    let arg = parts.get(1).copied();
    Some((action, arg))
}

/// Provider choice during configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderChoice {
    Anthropic,
    Local,
    OpenRouter,
    Settings,
    Invalid,
}

/// Parse provider choice from user input during configuration.
/// Returns the choice as an enum variant.
pub fn parse_provider_choice(input: &str) -> ProviderChoice {
    match input.trim().to_lowercase().as_str() {
        "1" | "anthropic" => ProviderChoice::Anthropic,
        "2" | "local" => ProviderChoice::Local,
        "3" | "openrouter" => ProviderChoice::OpenRouter,
        "s" | "settings" => ProviderChoice::Settings,
        _ => ProviderChoice::Invalid,
    }
}

/// Format shell output for display.
/// Returns (formatted_output, line_count, was_truncated).
pub fn format_shell_output_lines(
    stdout: &str,
    stderr: &str,
    max_lines: usize,
) -> (Vec<String>, usize, bool) {
    let output_lines: Vec<String> = stdout
        .lines()
        .chain(stderr.lines())
        .map(|s| s.to_string())
        .collect();

    let total_lines = output_lines.len();
    let was_truncated = total_lines > max_lines;

    let display_lines = if was_truncated {
        output_lines.into_iter().take(max_lines).collect()
    } else {
        output_lines
    };

    (display_lines, total_lines, was_truncated)
}

/// Extract tool uses from completion response content blocks.
pub fn extract_tool_uses(
    response_content: &[ContentBlockResponse],
) -> Vec<(String, String, serde_json::Value)> {
    response_content
        .iter()
        .filter_map(|block| {
            if let ContentBlockResponse::ToolUse { id, name, input } = block {
                Some((id.clone(), name.clone(), input.clone()))
            } else {
                None
            }
        })
        .collect()
}

/// Extract text content from completion response content blocks.
pub fn extract_text_content(response_content: &[ContentBlockResponse]) -> String {
    response_content
        .iter()
        .filter_map(|block| {
            if let ContentBlockResponse::Text { text } = block {
                Some(text.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Calculate target tokens for context trimming.
/// Returns the target token count (70% of context window).
pub fn calculate_trim_target(context_window: u32) -> u32 {
    (context_window as f64 * 0.7) as u32
}

/// Validate that a model name is in the list of known valid models.
pub fn is_valid_model(model: &str) -> bool {
    const VALID_MODELS: &[&str] = &[
        "claude-sonnet-4-20250514",
        "claude-3-5-sonnet-20241022",
        "claude-3-5-haiku-20241022",
        "claude-opus-4-20250514",
    ];
    VALID_MODELS.contains(&model)
}

/// Parse comma-separated cap names from input.
pub fn parse_cap_names(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Truncate a line for display, adding ellipsis if too long.
pub fn truncate_line(line: &str, max_len: usize) -> String {
    if line.len() > max_len {
        format!("{}...", &line[..max_len.saturating_sub(3)])
    } else {
        line.to_string()
    }
}

/// Check if input looks like a slash command (starts with /).
pub fn is_slash_command(input: &str) -> bool {
    input.trim().starts_with('/')
}

/// Parse the base command name from a slash command.
/// Returns the command without the leading slash and without arguments.
pub fn parse_slash_command_name(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let without_slash = &trimmed[1..];
    // Get the first word (before any space)
    without_slash.split_whitespace().next()
}

// === Development Slash Command Parsers ===

/// Parse /commit command and arguments.
/// Supports: /commit, /commit -m "message", /commit --amend, /commit file1 file2
pub fn parse_commit_command(input: &str) -> Option<CommitArgs> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    if !lower.starts_with("/commit") {
        return None;
    }

    // Check if it's exactly "/commit" or starts with "/commit "
    if lower != "/commit" && !lower.starts_with("/commit ") {
        return None;
    }

    let args_str = trimmed.get(7..).unwrap_or("").trim();
    let mut args = CommitArgs::default();

    if args_str.is_empty() {
        return Some(args);
    }

    // Parse arguments
    let mut i = 0;
    let chars: Vec<char> = args_str.chars().collect();

    while i < chars.len() {
        // Skip whitespace
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }

        if i >= chars.len() {
            break;
        }

        // Check for flags
        if chars[i] == '-' {
            // Find the end of this argument
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            let flag: String = chars[start..i].iter().collect();

            match flag.as_str() {
                "-m" | "--message" => {
                    // Skip whitespace
                    while i < chars.len() && chars[i].is_whitespace() {
                        i += 1;
                    }
                    // Extract message (quoted or until next flag)
                    if i < chars.len() {
                        let msg = extract_quoted_string_or_word(&chars, &mut i);
                        args.message = Some(msg);
                    }
                }
                "--amend" => {
                    args.amend = true;
                }
                _ => {
                    // Unknown flag, skip
                }
            }
        } else {
            // It's a file path
            let word = extract_word(&chars, &mut i);
            if !word.is_empty() {
                args.files.push(word);
            }
        }
    }

    Some(args)
}

/// Parse /test command and arguments.
/// Supports: /test, /test --watch, /test --coverage, /test pattern
pub fn parse_test_command(input: &str) -> Option<TestArgs> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    if !lower.starts_with("/test") {
        return None;
    }

    if lower != "/test" && !lower.starts_with("/test ") {
        return None;
    }

    let args_str = trimmed.get(5..).unwrap_or("").trim();
    let mut args = TestArgs::default();

    for part in args_str.split_whitespace() {
        match part.to_lowercase().as_str() {
            "--watch" | "-w" => args.watch = true,
            "--coverage" | "-c" => args.coverage = true,
            other if !other.starts_with('-') => {
                args.pattern = Some(other.to_string());
            }
            _ => {}
        }
    }

    Some(args)
}

/// Parse /review command and arguments.
/// Supports: /review, /review PR#, /review URL, /review --focus security
pub fn parse_review_command(input: &str) -> Option<ReviewArgs> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    if !lower.starts_with("/review") {
        return None;
    }

    if lower != "/review" && !lower.starts_with("/review ") {
        return None;
    }

    let args_str = trimmed.get(7..).unwrap_or("").trim();
    let mut args = ReviewArgs::default();

    let parts: Vec<&str> = args_str.split_whitespace().collect();
    let mut i = 0;

    while i < parts.len() {
        match parts[i].to_lowercase().as_str() {
            "--focus" | "-f" => {
                if i + 1 < parts.len() {
                    args.focus = Some(parts[i + 1].to_string());
                    i += 2;
                    continue;
                }
            }
            other if !other.starts_with('-') => {
                // Could be PR number, URL, or path
                args.target = Some(other.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    Some(args)
}

/// Parse /fix command and arguments.
/// Supports: /fix, /fix lint, /fix types, /fix all, /fix lint src/
pub fn parse_fix_command(input: &str) -> Option<FixArgs> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    if !lower.starts_with("/fix") {
        return None;
    }

    if lower != "/fix" && !lower.starts_with("/fix ") {
        return None;
    }

    let args_str = trimmed.get(4..).unwrap_or("").trim();
    let mut args = FixArgs::default();

    for part in args_str.split_whitespace() {
        match part.to_lowercase().as_str() {
            "lint" | "types" | "all" => {
                args.fix_type = Some(part.to_lowercase());
            }
            other if !other.starts_with('-') => {
                args.pattern = Some(other.to_string());
            }
            _ => {}
        }
    }

    Some(args)
}

/// Parse /explain command and arguments.
/// Supports: /explain, /explain file.rs, /explain --brief, /explain --detailed
pub fn parse_explain_command(input: &str) -> Option<ExplainArgs> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    if !lower.starts_with("/explain") {
        return None;
    }

    if lower != "/explain" && !lower.starts_with("/explain ") {
        return None;
    }

    let args_str = trimmed.get(8..).unwrap_or("").trim();
    let mut args = ExplainArgs::default();

    for part in args_str.split_whitespace() {
        match part.to_lowercase().as_str() {
            "--brief" | "-b" => args.verbosity = Some("brief".to_string()),
            "--detailed" | "-d" => args.verbosity = Some("detailed".to_string()),
            other if !other.starts_with('-') => {
                args.target = Some(other.to_string());
            }
            _ => {}
        }
    }

    Some(args)
}

// === Skills & Beads Command Parsers ===

/// Parse /skills command and arguments.
/// Supports: /skills, /skills list, /skills show <name>, /skills create <name>
pub fn parse_skills_command(input: &str) -> Option<SkillsArgs> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    // Handle both /skill and /skills
    if !lower.starts_with("/skills") && !lower.starts_with("/skill") {
        return None;
    }

    let prefix_len = if lower.starts_with("/skills") { 7 } else { 6 };

    // Check if it's exactly "/skill(s)" or starts with "/skill(s) "
    if trimmed.len() > prefix_len && !trimmed[prefix_len..].starts_with(' ') {
        return None;
    }

    let args_str = trimmed.get(prefix_len..).unwrap_or("").trim();
    let mut args = SkillsArgs::default();

    if args_str.is_empty() {
        // /skills with no args = list
        return Some(args);
    }

    let parts: Vec<&str> = args_str.split_whitespace().collect();

    if parts.is_empty() {
        return Some(args);
    }

    match parts[0].to_lowercase().as_str() {
        "list" => {
            args.subcommand = Some("list".to_string());
        }
        "show" => {
            args.subcommand = Some("show".to_string());
            if parts.len() > 1 {
                args.name = Some(parts[1..].join(" "));
            }
        }
        "create" => {
            args.subcommand = Some("create".to_string());
            if parts.len() > 1 {
                args.name = Some(parts[1..].join(" "));
            }
        }
        _ => {
            // Unknown subcommand - treat as skill name for show
            args.subcommand = Some("show".to_string());
            args.name = Some(parts.join(" "));
        }
    }

    Some(args)
}

/// Parse /beads command and arguments.
/// Supports: /beads, /beads list, /beads add <title>, /beads show <id>,
///           /beads status <id> <status>, /beads stats
pub fn parse_beads_command(input: &str) -> Option<BeadsArgs> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    // Handle both /bead and /beads
    if !lower.starts_with("/beads") && !lower.starts_with("/bead") {
        return None;
    }

    let prefix_len = if lower.starts_with("/beads") { 6 } else { 5 };

    // Check if it's exactly "/bead(s)" or starts with "/bead(s) "
    if trimmed.len() > prefix_len && !trimmed[prefix_len..].starts_with(' ') {
        return None;
    }

    let args_str = trimmed.get(prefix_len..).unwrap_or("").trim();
    let mut args = BeadsArgs::default();

    if args_str.is_empty() {
        // /beads with no args = list
        return Some(args);
    }

    let parts: Vec<&str> = args_str.split_whitespace().collect();

    if parts.is_empty() {
        return Some(args);
    }

    match parts[0].to_lowercase().as_str() {
        "list" => {
            args.subcommand = Some("list".to_string());
        }
        "add" => {
            args.subcommand = Some("add".to_string());
            if parts.len() > 1 {
                args.value = Some(parts[1..].join(" "));
            }
        }
        "show" => {
            args.subcommand = Some("show".to_string());
            if parts.len() > 1 {
                args.id = Some(parts[1].to_string());
            }
        }
        "status" => {
            args.subcommand = Some("status".to_string());
            if parts.len() > 1 {
                args.id = Some(parts[1].to_string());
            }
            if parts.len() > 2 {
                args.value = Some(parts[2..].join(" "));
            }
        }
        "stats" => {
            args.subcommand = Some("stats".to_string());
        }
        _ => {
            // Unknown subcommand - treat as bead ID for show
            args.subcommand = Some("show".to_string());
            args.id = Some(parts[0].to_string());
        }
    }

    Some(args)
}

/// Parse /model command and arguments.
/// Supports: /model, /models, /model list, /model download <name> [-q QUANT],
///           /model load <name>, /model info <name>, /model <name> (switch)
pub fn parse_model_command(input: &str) -> Option<ModelArgs> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    // Must start with /model or /models
    if !lower.starts_with("/model") {
        return None;
    }

    // Determine which command it is and validate
    let prefix_len = if let Some(rest) = lower.strip_prefix("/models") {
        // Must be exactly /models or /models followed by space
        if !rest.is_empty() && !rest.starts_with(' ') {
            return None; // Invalid: /modelssomething
        }
        7
    } else if let Some(rest) = lower.strip_prefix("/model") {
        // Must be exactly /model or /model followed by space
        if !rest.is_empty() && !rest.starts_with(' ') {
            return None; // Invalid: /modelsomething (when not /models)
        }
        6
    } else {
        return None;
    };

    let args_str = trimmed.get(prefix_len..).unwrap_or("").trim();
    let mut args = ModelArgs::default();

    if args_str.is_empty() {
        // /model or /models with no args = list
        args.subcommand = Some("list".to_string());
        return Some(args);
    }

    let parts: Vec<&str> = args_str.split_whitespace().collect();

    if parts.is_empty() {
        args.subcommand = Some("list".to_string());
        return Some(args);
    }

    match parts[0].to_lowercase().as_str() {
        "list" => {
            args.subcommand = Some("list".to_string());
        }
        "download" => {
            args.subcommand = Some("download".to_string());
            // Parse remaining args
            let mut i = 1;
            while i < parts.len() {
                match parts[i].to_lowercase().as_str() {
                    "-q" | "--quantization" => {
                        if i + 1 < parts.len() {
                            args.quantization = Some(parts[i + 1].to_string());
                            i += 2;
                            continue;
                        }
                    }
                    other if !other.starts_with('-') && args.name.is_none() => {
                        args.name = Some(other.to_string());
                    }
                    _ => {}
                }
                i += 1;
            }
        }
        "load" => {
            args.subcommand = Some("load".to_string());
            if parts.len() > 1 {
                args.name = Some(parts[1..].join(" "));
            }
        }
        "info" => {
            args.subcommand = Some("info".to_string());
            if parts.len() > 1 {
                args.name = Some(parts[1..].join(" "));
            }
        }
        _ => {
            // Unknown subcommand - treat as model name for switch
            args.subcommand = Some("switch".to_string());
            args.name = Some(parts.join(" "));
        }
    }

    Some(args)
}

/// Parse a status string into BeadStatus
pub fn parse_bead_status(status: &str) -> Option<BeadStatus> {
    match status.to_lowercase().as_str() {
        "pending" => Some(BeadStatus::Pending),
        "ready" => Some(BeadStatus::Ready),
        "in-progress" | "inprogress" | "in_progress" | "wip" => Some(BeadStatus::InProgress),
        "done" | "complete" | "completed" => Some(BeadStatus::Done),
        s if s.starts_with("blocked:") || s.starts_with("blocked ") => {
            let reason = s
                .strip_prefix("blocked:")
                .or_else(|| s.strip_prefix("blocked "))
                .unwrap_or("No reason provided")
                .trim()
                .to_string();
            Some(BeadStatus::Blocked { reason })
        }
        "blocked" => Some(BeadStatus::Blocked {
            reason: "No reason provided".to_string(),
        }),
        s if s.starts_with("cancelled:")
            || s.starts_with("cancelled ")
            || s.starts_with("canceled:")
            || s.starts_with("canceled ") =>
        {
            let reason = s
                .strip_prefix("cancelled:")
                .or_else(|| s.strip_prefix("cancelled "))
                .or_else(|| s.strip_prefix("canceled:"))
                .or_else(|| s.strip_prefix("canceled "))
                .unwrap_or("No reason provided")
                .trim()
                .to_string();
            Some(BeadStatus::Cancelled { reason })
        }
        "cancelled" | "canceled" => Some(BeadStatus::Cancelled {
            reason: "No reason provided".to_string(),
        }),
        _ => None,
    }
}

// === Helper Functions for Argument Parsing ===

/// Extract a quoted string or single word from character array.
fn extract_quoted_string_or_word(chars: &[char], i: &mut usize) -> String {
    let mut result = String::new();

    if *i < chars.len() && (chars[*i] == '"' || chars[*i] == '\'') {
        let quote = chars[*i];
        *i += 1;
        while *i < chars.len() && chars[*i] != quote {
            result.push(chars[*i]);
            *i += 1;
        }
        if *i < chars.len() {
            *i += 1; // Skip closing quote
        }
    } else {
        result = extract_word(chars, i);
    }

    result
}

/// Extract a word (non-whitespace sequence) from character array.
fn extract_word(chars: &[char], i: &mut usize) -> String {
    let mut result = String::new();
    while *i < chars.len() && !chars[*i].is_whitespace() {
        result.push(chars[*i]);
        *i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== parse_shell_command tests ====================

    #[test]
    fn test_parse_shell_command_valid() {
        assert_eq!(parse_shell_command(">ls -la"), Some("ls -la"));
        assert_eq!(parse_shell_command("> git status"), Some("git status"));
        assert_eq!(parse_shell_command("  >  echo hello  "), Some("echo hello"));
    }

    #[test]
    fn test_parse_shell_command_empty() {
        assert_eq!(parse_shell_command(">"), Some(""));
        assert_eq!(parse_shell_command(">  "), Some(""));
    }

    #[test]
    fn test_parse_shell_command_not_shell() {
        assert_eq!(parse_shell_command("hello"), None);
        assert_eq!(parse_shell_command("ls -la"), None);
        assert_eq!(parse_shell_command(""), None);
    }

    #[test]
    fn test_parse_shell_command_complex() {
        assert_eq!(
            parse_shell_command(">git commit -m 'test message'"),
            Some("git commit -m 'test message'")
        );
        assert_eq!(
            parse_shell_command(">echo 'hello > world'"),
            Some("echo 'hello > world'")
        );
    }

    // ==================== truncate_command_display tests ====================

    #[test]
    fn test_truncate_command_display_short() {
        assert_eq!(truncate_command_display("ls -la", 60), "ls -la");
        assert_eq!(truncate_command_display("short", 10), "short");
    }

    #[test]
    fn test_truncate_command_display_long() {
        let long_cmd = "a".repeat(100);
        let result = truncate_command_display(&long_cmd, 60);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 60);
    }

    #[test]
    fn test_truncate_command_display_exact() {
        let cmd = "a".repeat(60);
        let result = truncate_command_display(&cmd, 60);
        assert_eq!(result, cmd);
    }

    #[test]
    fn test_truncate_command_display_small_max() {
        let cmd = "hello world";
        let result = truncate_command_display(cmd, 5);
        assert_eq!(result, "he...");
    }

    #[test]
    fn test_truncate_command_display_empty() {
        assert_eq!(truncate_command_display("", 10), "");
    }

    // ==================== is_exit_command tests ====================

    #[test]
    fn test_is_exit_command_lowercase() {
        assert!(is_exit_command("exit"));
        assert!(is_exit_command("quit"));
        assert!(is_exit_command("/exit"));
        assert!(is_exit_command("/quit"));
    }

    #[test]
    fn test_is_exit_command_uppercase() {
        assert!(is_exit_command("EXIT"));
        assert!(is_exit_command("QUIT"));
        assert!(is_exit_command("/EXIT"));
        assert!(is_exit_command("/QUIT"));
    }

    #[test]
    fn test_is_exit_command_whitespace() {
        assert!(is_exit_command("  exit  "));
        assert!(is_exit_command("\texit\n"));
    }

    #[test]
    fn test_is_exit_command_invalid() {
        assert!(!is_exit_command("hello"));
        assert!(!is_exit_command("exiting"));
        assert!(!is_exit_command("quitting"));
        assert!(!is_exit_command(""));
    }

    // ==================== is_clear_command tests ====================

    #[test]
    fn test_is_clear_command_valid() {
        assert!(is_clear_command("/clear"));
        assert!(is_clear_command("/CLEAR"));
        assert!(is_clear_command("  /clear  "));
    }

    #[test]
    fn test_is_clear_command_invalid() {
        assert!(!is_clear_command("clear"));
        assert!(!is_clear_command("/clearall"));
        assert!(!is_clear_command(""));
    }

    // ==================== is_help_command tests ====================

    #[test]
    fn test_is_help_command_valid() {
        assert!(is_help_command("/help"));
        assert!(is_help_command("/HELP"));
        assert!(is_help_command("  /help  "));
    }

    #[test]
    fn test_is_help_command_invalid() {
        assert!(!is_help_command("help"));
        assert!(!is_help_command("/helper"));
        assert!(!is_help_command(""));
    }

    // ==================== is_stats_command tests ====================

    #[test]
    fn test_is_stats_command_valid() {
        assert!(is_stats_command("/stats"));
        assert!(is_stats_command("/context"));
        assert!(is_stats_command("/STATS"));
        assert!(is_stats_command("  /context  "));
    }

    #[test]
    fn test_is_stats_command_invalid() {
        assert!(!is_stats_command("stats"));
        assert!(!is_stats_command("/statistics"));
        assert!(!is_stats_command(""));
    }

    // ==================== is_settings_command tests ====================

    #[test]
    fn test_is_settings_command_valid() {
        assert!(is_settings_command("/settings"));
        assert!(is_settings_command("/config"));
        assert!(is_settings_command("/SETTINGS"));
        assert!(is_settings_command("  /config  "));
    }

    #[test]
    fn test_is_settings_command_invalid() {
        assert!(!is_settings_command("settings"));
        assert!(!is_settings_command("/configure"));
        assert!(!is_settings_command(""));
    }

    // ==================== is_sessions_command tests ====================

    #[test]
    fn test_is_sessions_command_valid() {
        assert!(is_sessions_command("/sessions"));
        assert!(is_sessions_command("/session"));
        assert!(is_sessions_command("/SESSIONS"));
    }

    #[test]
    fn test_is_sessions_command_invalid() {
        assert!(!is_sessions_command("sessions"));
        assert!(!is_sessions_command("/sess"));
    }

    // ==================== is_new_command tests ====================

    #[test]
    fn test_is_new_command_valid() {
        assert!(is_new_command("/new"));
        assert!(is_new_command("/NEW"));
        assert!(is_new_command("  /new  "));
    }

    #[test]
    fn test_is_new_command_invalid() {
        assert!(!is_new_command("new"));
        assert!(!is_new_command("/newone"));
    }

    // ==================== is_plans_command tests ====================

    #[test]
    fn test_is_plans_command_valid() {
        assert!(is_plans_command("/plans"));
        assert!(is_plans_command("/plan"));
        assert!(is_plans_command("/PLANS"));
    }

    #[test]
    fn test_is_plans_command_invalid() {
        assert!(!is_plans_command("plans"));
        assert!(!is_plans_command("/planning"));
    }

    // ==================== is_model_command tests ====================

    #[test]
    fn test_is_model_command_valid() {
        assert!(is_model_command("/model"));
        assert!(is_model_command("/models"));
        assert!(is_model_command("/MODEL"));
    }

    #[test]
    fn test_is_model_command_invalid() {
        assert!(!is_model_command("model"));
        assert!(!is_model_command("/model gpt-4")); // This has an argument
    }

    // ==================== is_caps_command tests ====================

    #[test]
    fn test_is_caps_command_valid() {
        assert!(is_caps_command("/caps"));
        assert!(is_caps_command("/CAPS"));
        assert!(is_caps_command("  /caps  "));
    }

    #[test]
    fn test_is_caps_command_invalid() {
        assert!(!is_caps_command("caps"));
        assert!(!is_caps_command("/cap"));
    }

    // ==================== parse_switch_command tests ====================

    #[test]
    fn test_parse_switch_command_valid() {
        assert_eq!(parse_switch_command("/switch abc123"), Some("abc123"));
        assert_eq!(parse_switch_command("/switch 1"), Some("1"));
    }

    #[test]
    fn test_parse_switch_command_invalid() {
        assert_eq!(parse_switch_command("/switch"), None);
        assert_eq!(parse_switch_command("switch abc"), None);
    }

    // ==================== parse_model_switch_command tests ====================

    #[test]
    fn test_parse_model_switch_command_valid() {
        assert_eq!(
            parse_model_switch_command("/model claude-3-5-sonnet"),
            Some("claude-3-5-sonnet")
        );
        assert_eq!(parse_model_switch_command("/model gpt-4"), Some("gpt-4"));
    }

    #[test]
    fn test_parse_model_switch_command_invalid() {
        assert_eq!(parse_model_switch_command("/model"), None);
        assert_eq!(parse_model_switch_command("model gpt-4"), None);
    }

    // ==================== parse_cap_command tests ====================

    #[test]
    fn test_parse_cap_command_with_action_and_arg() {
        let result = parse_cap_command("/cap add mycode");
        assert_eq!(result, Some(("add", Some("mycode"))));
    }

    #[test]
    fn test_parse_cap_command_with_action_only() {
        let result = parse_cap_command("/cap clear");
        assert_eq!(result, Some(("clear", None)));
    }

    #[test]
    fn test_parse_cap_command_with_arg_containing_spaces() {
        let result = parse_cap_command("/cap set base,code");
        assert_eq!(result, Some(("set", Some("base,code"))));
    }

    #[test]
    fn test_parse_cap_command_invalid() {
        assert_eq!(parse_cap_command("cap add"), None);
        assert_eq!(parse_cap_command("/cap"), None);
        assert_eq!(parse_cap_command("/cap "), None);
    }

    // ==================== parse_provider_choice tests ====================

    #[test]
    fn test_parse_provider_choice_anthropic() {
        assert_eq!(parse_provider_choice("1"), ProviderChoice::Anthropic);
        assert_eq!(
            parse_provider_choice("anthropic"),
            ProviderChoice::Anthropic
        );
        assert_eq!(
            parse_provider_choice("ANTHROPIC"),
            ProviderChoice::Anthropic
        );
    }

    #[test]
    fn test_parse_provider_choice_local() {
        assert_eq!(parse_provider_choice("2"), ProviderChoice::Local);
        assert_eq!(parse_provider_choice("local"), ProviderChoice::Local);
        assert_eq!(parse_provider_choice("LOCAL"), ProviderChoice::Local);
    }

    #[test]
    fn test_parse_provider_choice_openrouter() {
        assert_eq!(parse_provider_choice("3"), ProviderChoice::OpenRouter);
        assert_eq!(
            parse_provider_choice("openrouter"),
            ProviderChoice::OpenRouter
        );
    }

    #[test]
    fn test_parse_provider_choice_settings() {
        assert_eq!(parse_provider_choice("s"), ProviderChoice::Settings);
        assert_eq!(parse_provider_choice("settings"), ProviderChoice::Settings);
        assert_eq!(parse_provider_choice("S"), ProviderChoice::Settings);
    }

    #[test]
    fn test_parse_provider_choice_invalid() {
        assert_eq!(parse_provider_choice(""), ProviderChoice::Invalid);
        assert_eq!(parse_provider_choice("4"), ProviderChoice::Invalid);
        assert_eq!(parse_provider_choice("invalid"), ProviderChoice::Invalid);
    }

    // ==================== format_shell_output_lines tests ====================

    #[test]
    fn test_format_shell_output_lines_small() {
        let (lines, total, truncated) = format_shell_output_lines("line1\nline2\nline3", "", 10);
        assert_eq!(lines.len(), 3);
        assert_eq!(total, 3);
        assert!(!truncated);
    }

    #[test]
    fn test_format_shell_output_lines_truncated() {
        let stdout = (0..20)
            .map(|i| format!("line{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let (lines, total, truncated) = format_shell_output_lines(&stdout, "", 10);
        assert_eq!(lines.len(), 10);
        assert_eq!(total, 20);
        assert!(truncated);
    }

    #[test]
    fn test_format_shell_output_lines_combined() {
        let (lines, total, truncated) =
            format_shell_output_lines("stdout1\nstdout2", "stderr1", 10);
        assert_eq!(lines.len(), 3);
        assert_eq!(total, 3);
        assert!(!truncated);
        assert!(lines.contains(&"stderr1".to_string()));
    }

    #[test]
    fn test_format_shell_output_lines_empty() {
        let (lines, total, truncated) = format_shell_output_lines("", "", 10);
        assert!(lines.is_empty());
        assert_eq!(total, 0);
        assert!(!truncated);
    }

    #[test]
    fn test_format_shell_output_lines_stdout_only() {
        let (lines, total, truncated) = format_shell_output_lines("line1\nline2", "", 5);
        assert_eq!(lines.len(), 2);
        assert_eq!(total, 2);
        assert!(!truncated);
    }

    #[test]
    fn test_format_shell_output_lines_stderr_only() {
        let (lines, total, truncated) = format_shell_output_lines("", "error1\nerror2", 5);
        assert_eq!(lines.len(), 2);
        assert_eq!(total, 2);
        assert!(!truncated);
    }

    // ==================== extract_tool_uses tests ====================

    #[test]
    fn test_extract_tool_uses_empty() {
        let content: Vec<ContentBlockResponse> = vec![];
        let tool_uses = extract_tool_uses(&content);
        assert!(tool_uses.is_empty());
    }

    #[test]
    fn test_extract_tool_uses_text_only() {
        let content = vec![ContentBlockResponse::Text {
            text: "Hello".to_string(),
        }];
        let tool_uses = extract_tool_uses(&content);
        assert!(tool_uses.is_empty());
    }

    #[test]
    fn test_extract_tool_uses_with_tools() {
        let content = vec![
            ContentBlockResponse::Text {
                text: "I will read the file".to_string(),
            },
            ContentBlockResponse::ToolUse {
                id: "tool_1".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({"path": "/tmp/test.txt"}),
            },
        ];
        let tool_uses = extract_tool_uses(&content);
        assert_eq!(tool_uses.len(), 1);
        assert_eq!(tool_uses[0].0, "tool_1");
        assert_eq!(tool_uses[0].1, "file_read");
    }

    #[test]
    fn test_extract_tool_uses_multiple() {
        let content = vec![
            ContentBlockResponse::ToolUse {
                id: "tool_1".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({}),
            },
            ContentBlockResponse::ToolUse {
                id: "tool_2".to_string(),
                name: "shell".to_string(),
                input: serde_json::json!({"command": "ls"}),
            },
        ];
        let tool_uses = extract_tool_uses(&content);
        assert_eq!(tool_uses.len(), 2);
    }

    // ==================== extract_text_content tests ====================

    #[test]
    fn test_extract_text_content_empty() {
        let content: Vec<ContentBlockResponse> = vec![];
        let text = extract_text_content(&content);
        assert!(text.is_empty());
    }

    #[test]
    fn test_extract_text_content_single() {
        let content = vec![ContentBlockResponse::Text {
            text: "Hello world".to_string(),
        }];
        let text = extract_text_content(&content);
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn test_extract_text_content_multiple() {
        let content = vec![
            ContentBlockResponse::Text {
                text: "First".to_string(),
            },
            ContentBlockResponse::Text {
                text: "Second".to_string(),
            },
        ];
        let text = extract_text_content(&content);
        assert_eq!(text, "First\nSecond");
    }

    #[test]
    fn test_extract_text_content_mixed() {
        let content = vec![
            ContentBlockResponse::Text {
                text: "Text before".to_string(),
            },
            ContentBlockResponse::ToolUse {
                id: "tool_1".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({}),
            },
            ContentBlockResponse::Text {
                text: "Text after".to_string(),
            },
        ];
        let text = extract_text_content(&content);
        assert_eq!(text, "Text before\nText after");
    }

    // ==================== calculate_trim_target tests ====================

    #[test]
    fn test_calculate_trim_target() {
        assert_eq!(calculate_trim_target(100000), 70000);
        assert_eq!(calculate_trim_target(200000), 140000);
        assert_eq!(calculate_trim_target(0), 0);
    }

    #[test]
    fn test_calculate_trim_target_small() {
        assert_eq!(calculate_trim_target(100), 70);
        assert_eq!(calculate_trim_target(10), 7);
    }

    #[test]
    fn test_calculate_trim_target_rounding() {
        // Test that we get reasonable rounding behavior
        assert_eq!(calculate_trim_target(1), 0);
        assert_eq!(calculate_trim_target(2), 1);
        assert_eq!(calculate_trim_target(3), 2);
    }

    // ==================== is_valid_model tests ====================

    #[test]
    fn test_is_valid_model_valid() {
        assert!(is_valid_model("claude-sonnet-4-20250514"));
        assert!(is_valid_model("claude-3-5-sonnet-20241022"));
        assert!(is_valid_model("claude-3-5-haiku-20241022"));
    }

    #[test]
    fn test_is_valid_model_invalid() {
        assert!(!is_valid_model("gpt-4"));
        assert!(!is_valid_model("invalid-model"));
        assert!(!is_valid_model(""));
    }

    // ==================== parse_cap_names tests ====================

    #[test]
    fn test_parse_cap_names_single() {
        let caps = parse_cap_names("base");
        assert_eq!(caps, vec!["base"]);
    }

    #[test]
    fn test_parse_cap_names_multiple() {
        let caps = parse_cap_names("base, code, debug");
        assert_eq!(caps, vec!["base", "code", "debug"]);
    }

    #[test]
    fn test_parse_cap_names_with_extra_whitespace() {
        let caps = parse_cap_names("  base  ,  code  ");
        assert_eq!(caps, vec!["base", "code"]);
    }

    #[test]
    fn test_parse_cap_names_empty() {
        let caps = parse_cap_names("");
        assert!(caps.is_empty());
    }

    #[test]
    fn test_parse_cap_names_with_empty_items() {
        let caps = parse_cap_names("base,,code,");
        assert_eq!(caps, vec!["base", "code"]);
    }

    // ==================== truncate_line tests ====================

    #[test]
    fn test_truncate_line_short() {
        assert_eq!(truncate_line("short", 10), "short");
    }

    #[test]
    fn test_truncate_line_long() {
        let long = "a".repeat(100);
        let result = truncate_line(&long, 20);
        assert!(result.len() <= 20);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_line_exact() {
        assert_eq!(truncate_line("exactly10!", 10), "exactly10!");
    }

    // ==================== is_slash_command tests ====================

    #[test]
    fn test_is_slash_command_valid() {
        assert!(is_slash_command("/help"));
        assert!(is_slash_command("  /clear"));
        assert!(is_slash_command("/model gpt-4"));
    }

    #[test]
    fn test_is_slash_command_invalid() {
        assert!(!is_slash_command("help"));
        assert!(!is_slash_command(""));
        assert!(!is_slash_command(">shell"));
    }

    // ==================== parse_slash_command_name tests ====================

    #[test]
    fn test_parse_slash_command_name_simple() {
        assert_eq!(parse_slash_command_name("/help"), Some("help"));
        assert_eq!(parse_slash_command_name("/clear"), Some("clear"));
    }

    #[test]
    fn test_parse_slash_command_name_with_args() {
        assert_eq!(parse_slash_command_name("/model gpt-4"), Some("model"));
        assert_eq!(parse_slash_command_name("/switch abc123"), Some("switch"));
    }

    #[test]
    fn test_parse_slash_command_name_invalid() {
        assert_eq!(parse_slash_command_name("help"), None);
        assert_eq!(parse_slash_command_name(""), None);
    }

    #[test]
    fn test_parse_slash_command_name_empty_command() {
        assert_eq!(parse_slash_command_name("/"), None);
        assert_eq!(parse_slash_command_name("/ "), None);
    }

    // ==================== ProviderChoice tests ====================

    #[test]
    fn test_provider_choice_debug() {
        let choice = ProviderChoice::Anthropic;
        let debug_str = format!("{:?}", choice);
        assert!(debug_str.contains("Anthropic"));
    }

    #[test]
    fn test_provider_choice_clone() {
        let choice = ProviderChoice::Local;
        let cloned = choice.clone();
        assert_eq!(choice, cloned);
    }

    #[test]
    fn test_provider_choice_eq() {
        assert_eq!(ProviderChoice::Anthropic, ProviderChoice::Anthropic);
        assert_ne!(ProviderChoice::Anthropic, ProviderChoice::Local);
    }

    // ==================== Edge cases and regression tests ====================

    #[test]
    fn test_unicode_input_handling() {
        assert!(!is_exit_command("退出"));
        assert!(is_slash_command("/退出"));
        assert_eq!(parse_slash_command_name("/退出"), Some("退出"));
    }

    #[test]
    fn test_whitespace_only_input() {
        assert!(!is_exit_command("   "));
        assert!(!is_slash_command("   "));
        assert_eq!(parse_shell_command("   "), None);
    }

    #[test]
    fn test_newline_handling() {
        assert!(is_exit_command("exit\n"));
        assert!(is_slash_command("/help\n"));
    }

    #[test]
    fn test_mixed_case_commands() {
        assert!(is_exit_command("ExIt"));
        assert!(is_clear_command("/CleAr"));
        assert!(is_help_command("/HelP"));
    }

    // ==================== Skills Command Parser Tests ====================

    #[test]
    fn test_parse_skills_command_list() {
        let args = parse_skills_command("/skills").unwrap();
        assert!(args.subcommand.is_none());
        assert!(args.name.is_none());

        let args = parse_skills_command("/skills list").unwrap();
        assert_eq!(args.subcommand, Some("list".to_string()));
    }

    #[test]
    fn test_parse_skills_command_show() {
        let args = parse_skills_command("/skills show rust-async").unwrap();
        assert_eq!(args.subcommand, Some("show".to_string()));
        assert_eq!(args.name, Some("rust-async".to_string()));
    }

    #[test]
    fn test_parse_skills_command_show_multi_word() {
        let args = parse_skills_command("/skills show my skill name").unwrap();
        assert_eq!(args.subcommand, Some("show".to_string()));
        assert_eq!(args.name, Some("my skill name".to_string()));
    }

    #[test]
    fn test_parse_skills_command_create() {
        let args = parse_skills_command("/skills create my-new-skill").unwrap();
        assert_eq!(args.subcommand, Some("create".to_string()));
        assert_eq!(args.name, Some("my-new-skill".to_string()));
    }

    #[test]
    fn test_parse_skills_command_alias() {
        // Both /skill and /skills should work
        assert!(parse_skills_command("/skill").is_some());
        assert!(parse_skills_command("/skills").is_some());
        assert!(parse_skills_command("/skill show test").is_some());
    }

    #[test]
    fn test_parse_skills_command_invalid() {
        assert!(parse_skills_command("/skillset").is_none());
        assert!(parse_skills_command("/sk").is_none());
        assert!(parse_skills_command("skills").is_none());
    }

    #[test]
    fn test_parse_skills_command_unknown_treated_as_show() {
        let args = parse_skills_command("/skills rust-async").unwrap();
        assert_eq!(args.subcommand, Some("show".to_string()));
        assert_eq!(args.name, Some("rust-async".to_string()));
    }

    // ==================== Beads Command Parser Tests ====================

    #[test]
    fn test_parse_beads_command_list() {
        let args = parse_beads_command("/beads").unwrap();
        assert!(args.subcommand.is_none());

        let args = parse_beads_command("/beads list").unwrap();
        assert_eq!(args.subcommand, Some("list".to_string()));
    }

    #[test]
    fn test_parse_beads_command_add() {
        let args = parse_beads_command("/beads add Implement feature X").unwrap();
        assert_eq!(args.subcommand, Some("add".to_string()));
        assert_eq!(args.value, Some("Implement feature X".to_string()));
    }

    #[test]
    fn test_parse_beads_command_show() {
        let args = parse_beads_command("/beads show bd-12345678").unwrap();
        assert_eq!(args.subcommand, Some("show".to_string()));
        assert_eq!(args.id, Some("bd-12345678".to_string()));
    }

    #[test]
    fn test_parse_beads_command_status() {
        let args = parse_beads_command("/beads status bd-12345678 done").unwrap();
        assert_eq!(args.subcommand, Some("status".to_string()));
        assert_eq!(args.id, Some("bd-12345678".to_string()));
        assert_eq!(args.value, Some("done".to_string()));
    }

    #[test]
    fn test_parse_beads_command_status_with_reason() {
        let args =
            parse_beads_command("/beads status bd-12345678 blocked:waiting for API").unwrap();
        assert_eq!(args.value, Some("blocked:waiting for API".to_string()));
    }

    #[test]
    fn test_parse_beads_command_stats() {
        let args = parse_beads_command("/beads stats").unwrap();
        assert_eq!(args.subcommand, Some("stats".to_string()));
    }

    #[test]
    fn test_parse_beads_command_alias() {
        // Both /bead and /beads should work
        assert!(parse_beads_command("/bead").is_some());
        assert!(parse_beads_command("/beads").is_some());
    }

    #[test]
    fn test_parse_beads_command_invalid() {
        assert!(parse_beads_command("/beadwork").is_none());
        assert!(parse_beads_command("beads").is_none());
    }

    #[test]
    fn test_parse_beads_command_unknown_treated_as_show() {
        let args = parse_beads_command("/beads bd-12345678").unwrap();
        assert_eq!(args.subcommand, Some("show".to_string()));
        assert_eq!(args.id, Some("bd-12345678".to_string()));
    }

    // ==================== Bead Status Parser Tests ====================

    #[test]
    fn test_parse_bead_status_simple() {
        assert!(matches!(
            parse_bead_status("pending"),
            Some(BeadStatus::Pending)
        ));
        assert!(matches!(
            parse_bead_status("ready"),
            Some(BeadStatus::Ready)
        ));
        assert!(matches!(
            parse_bead_status("in-progress"),
            Some(BeadStatus::InProgress)
        ));
        assert!(matches!(
            parse_bead_status("wip"),
            Some(BeadStatus::InProgress)
        ));
        assert!(matches!(parse_bead_status("done"), Some(BeadStatus::Done)));
        assert!(matches!(
            parse_bead_status("complete"),
            Some(BeadStatus::Done)
        ));
    }

    #[test]
    fn test_parse_bead_status_blocked() {
        let status = parse_bead_status("blocked:waiting for api").unwrap();
        if let BeadStatus::Blocked { reason } = status {
            assert_eq!(reason, "waiting for api");
        } else {
            panic!("Expected Blocked status");
        }

        // Simple blocked without reason
        let status = parse_bead_status("blocked").unwrap();
        if let BeadStatus::Blocked { reason } = status {
            assert_eq!(reason, "No reason provided");
        } else {
            panic!("Expected Blocked status");
        }
    }

    #[test]
    fn test_parse_bead_status_cancelled() {
        let status = parse_bead_status("cancelled:no longer needed").unwrap();
        if let BeadStatus::Cancelled { reason } = status {
            assert_eq!(reason, "no longer needed");
        } else {
            panic!("Expected Cancelled status");
        }

        // American spelling
        let status = parse_bead_status("canceled").unwrap();
        assert!(matches!(status, BeadStatus::Cancelled { .. }));
    }

    #[test]
    fn test_parse_bead_status_invalid() {
        assert!(parse_bead_status("unknown").is_none());
        assert!(parse_bead_status("").is_none());
        assert!(parse_bead_status("xyz").is_none());
    }

    #[test]
    fn test_parse_bead_status_case_insensitive() {
        assert!(matches!(
            parse_bead_status("PENDING"),
            Some(BeadStatus::Pending)
        ));
        assert!(matches!(parse_bead_status("Done"), Some(BeadStatus::Done)));
        assert!(matches!(
            parse_bead_status("IN-PROGRESS"),
            Some(BeadStatus::InProgress)
        ));
    }

    // ==================== Model Command Parser Tests ====================

    #[test]
    fn test_parse_model_command_list() {
        let args = parse_model_command("/model").unwrap();
        assert_eq!(args.subcommand, Some("list".to_string()));

        let args = parse_model_command("/models").unwrap();
        assert_eq!(args.subcommand, Some("list".to_string()));

        let args = parse_model_command("/model list").unwrap();
        assert_eq!(args.subcommand, Some("list".to_string()));
    }

    #[test]
    fn test_parse_model_command_download() {
        let args = parse_model_command("/model download qwen3-coder-30b").unwrap();
        assert_eq!(args.subcommand, Some("download".to_string()));
        assert_eq!(args.name, Some("qwen3-coder-30b".to_string()));
    }

    #[test]
    fn test_parse_model_command_download_with_quantization() {
        let args = parse_model_command("/model download qwen3-coder-30b -q q4_k_m").unwrap();
        assert_eq!(args.subcommand, Some("download".to_string()));
        assert_eq!(args.name, Some("qwen3-coder-30b".to_string()));
        assert_eq!(args.quantization, Some("q4_k_m".to_string()));

        let args = parse_model_command("/model download llama3 --quantization Q5_K_M").unwrap();
        assert_eq!(args.quantization, Some("Q5_K_M".to_string()));
    }

    #[test]
    fn test_parse_model_command_load() {
        let args = parse_model_command("/model load qwen3-coder-30b").unwrap();
        assert_eq!(args.subcommand, Some("load".to_string()));
        assert_eq!(args.name, Some("qwen3-coder-30b".to_string()));
    }

    #[test]
    fn test_parse_model_command_info() {
        let args = parse_model_command("/model info llama3").unwrap();
        assert_eq!(args.subcommand, Some("info".to_string()));
        assert_eq!(args.name, Some("llama3".to_string()));
    }

    #[test]
    fn test_parse_model_command_switch() {
        // Model name without subcommand = switch
        let args = parse_model_command("/model claude-3-5-sonnet-20241022").unwrap();
        assert_eq!(args.subcommand, Some("switch".to_string()));
        assert_eq!(args.name, Some("claude-3-5-sonnet-20241022".to_string()));
    }

    #[test]
    fn test_parse_model_command_invalid() {
        assert!(parse_model_command("/modelsomething").is_none());
        assert!(parse_model_command("model").is_none());
        assert!(parse_model_command("/mod").is_none());
    }
}
