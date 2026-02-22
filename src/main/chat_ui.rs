// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::io::{self, Write};

use crossterm::{
    style::{Color, ResetColor, SetForegroundColor},
    ExecutableCommand,
};

use ted::context::SessionId;
use ted::error::{Result, TedError};
use ted::history::{HistoryStore, SessionInfo};
use ted::tools::ToolResult;

/// Resume a session by its ID (supports short or full ID)
pub(super) fn resume_session(
    history_store: &HistoryStore,
    resume_id: &str,
    _working_directory: &std::path::PathBuf,
) -> Result<(SessionId, SessionInfo, usize, bool)> {
    // Parse session ID (support both full and short forms)
    let id = if resume_id.len() <= 8 {
        // Short form - find matching session
        let sessions = history_store.list_recent(1000);
        sessions
            .iter()
            .find(|s| s.id.to_string().starts_with(resume_id))
            .map(|s| s.id)
            .ok_or_else(|| {
                TedError::InvalidInput(format!("No session found matching '{}'", resume_id))
            })?
    } else {
        uuid::Uuid::parse_str(resume_id)
            .map_err(|_| TedError::InvalidInput("Invalid session ID format".to_string()))?
    };

    let session = history_store
        .get(id)
        .ok_or_else(|| TedError::InvalidInput(format!("Session '{}' not found", resume_id)))?;

    let session_info = session.clone();
    let message_count = session_info.message_count;
    let session_id = SessionId(id);

    println!();
    let mut stdout = io::stdout();
    stdout.execute(SetForegroundColor(Color::Cyan))?;
    println!("Resuming session: {}", &id.to_string()[..8]);
    stdout.execute(ResetColor)?;
    if let Some(ref summary) = session_info.summary {
        println!("  {}", summary);
    }
    println!(
        "  {} messages from {}",
        message_count,
        session_info.last_active.format("%Y-%m-%d %H:%M")
    );
    println!();

    Ok((session_id, session_info, message_count, true))
}

/// Prompt user to choose from recent sessions or start fresh
pub(super) fn prompt_session_choice(sessions: &[&SessionInfo]) -> Result<Option<SessionInfo>> {
    if sessions.is_empty() {
        return Ok(None);
    }

    let mut stdout = io::stdout();
    stdout.execute(SetForegroundColor(Color::Cyan))?;
    println!("\nRecent session(s) found in this directory:\n");
    stdout.execute(ResetColor)?;

    // Show up to 3 recent sessions
    let show_sessions: Vec<_> = sessions.iter().take(3).collect();

    for (i, session) in show_sessions.iter().enumerate() {
        let id_short = &session.id.to_string()[..8];
        let age = chrono::Utc::now() - session.last_active;
        let age_str = if age.num_minutes() < 60 {
            format!("{}m ago", age.num_minutes())
        } else {
            format!("{}h ago", age.num_hours())
        };

        let summary = session.summary.as_deref().unwrap_or("(no summary)");
        let truncated_summary = if summary.len() > 50 {
            format!("{}...", &summary[..47])
        } else {
            summary.to_string()
        };

        println!(
            "  [{}] {} | {} | {} msgs | {}",
            i + 1,
            id_short,
            age_str,
            session.message_count,
            truncated_summary
        );
    }

    println!("\n  [n] Start new session");
    print!("\nChoice [1/n]: ");
    stdout.flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    if input.is_empty() || input == "1" {
        // Default to resuming the most recent session
        Ok(Some((*show_sessions[0]).clone()))
    } else if input == "n" || input == "new" {
        Ok(None)
    } else if let Ok(num) = input.parse::<usize>() {
        if num >= 1 && num <= show_sessions.len() {
            Ok(Some((*show_sessions[num - 1]).clone()))
        } else {
            // Invalid choice, start new
            println!("Invalid choice, starting new session.");
            Ok(None)
        }
    } else {
        // Invalid input, start new
        println!("Invalid choice, starting new session.");
        Ok(None)
    }
}

/// Print tool invocation with visual formatting
pub(super) fn print_tool_invocation(tool_name: &str, input: &serde_json::Value) -> Result<()> {
    let mut stdout = io::stdout();

    // Tool icon and name
    stdout.execute(SetForegroundColor(Color::DarkGrey))?;
    print!("  ╭─ ");
    stdout.execute(SetForegroundColor(Color::Magenta))?;
    print!("{}", tool_name);
    stdout.execute(ResetColor)?;

    // Print relevant parameters based on tool type
    match tool_name {
        "file_read" => {
            if let Some(path) = input["path"].as_str() {
                stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                print!(" → ");
                stdout.execute(SetForegroundColor(Color::Blue))?;
                println!("{}", path);
            } else {
                println!();
            }
        }
        "file_write" => {
            if let Some(path) = input["path"].as_str() {
                stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                print!(" → ");
                stdout.execute(SetForegroundColor(Color::Green))?;
                print!("{}", path);
                stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                println!(" (new file)");
            } else {
                println!();
            }
        }
        "file_edit" => {
            if let Some(path) = input["path"].as_str() {
                stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                print!(" → ");
                stdout.execute(SetForegroundColor(Color::Yellow))?;
                println!("{}", path);
            } else {
                println!();
            }
        }
        "shell" => {
            if let Some(command) = input["command"].as_str() {
                stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                print!(" → ");
                stdout.execute(SetForegroundColor(Color::Cyan))?;
                // Truncate long commands
                let display_cmd = if command.len() > 60 {
                    format!("{}...", &command[..57])
                } else {
                    command.to_string()
                };
                println!("{}", display_cmd);
            } else {
                println!();
            }
        }
        "glob" => {
            if let Some(pattern) = input["pattern"].as_str() {
                stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                print!(" → ");
                stdout.execute(SetForegroundColor(Color::Blue))?;
                println!("{}", pattern);
            } else {
                println!();
            }
        }
        "grep" => {
            if let Some(pattern) = input["pattern"].as_str() {
                stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                print!(" → ");
                stdout.execute(SetForegroundColor(Color::Blue))?;
                print!("/{}/", pattern);
                if let Some(path) = input["path"].as_str() {
                    stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                    print!(" in ");
                    stdout.execute(SetForegroundColor(Color::Blue))?;
                    print!("{}", path);
                }
                println!();
            } else {
                println!();
            }
        }
        _ => {
            println!();
        }
    }

    stdout.execute(ResetColor)?;
    stdout.flush()?;
    Ok(())
}

/// Maximum lines to show for shell output before collapsing
pub(super) const SHELL_OUTPUT_MAX_LINES: usize = 15;

/// Print tool result with visual formatting
pub(super) fn print_tool_result(tool_name: &str, result: &ToolResult) -> Result<()> {
    let mut stdout = io::stdout();

    stdout.execute(SetForegroundColor(Color::DarkGrey))?;
    print!("  ╰─ ");

    if result.is_error() {
        stdout.execute(SetForegroundColor(Color::Red))?;
        print!("✗ ");
        // Show first few lines of error for context
        let error_lines: Vec<_> = result.output_text().lines().take(5).collect();
        if error_lines.len() == 1 {
            let line = error_lines[0];
            let display = if line.len() > 80 {
                format!("{}...", &line[..77])
            } else {
                line.to_string()
            };
            println!("{}", display);
        } else {
            println!();
            for line in error_lines {
                stdout.execute(SetForegroundColor(Color::Red))?;
                println!("     {}", line);
            }
        }
    } else {
        stdout.execute(SetForegroundColor(Color::Green))?;
        print!("✓ ");
        stdout.execute(ResetColor)?;

        // Show result summary based on tool type
        match tool_name {
            "file_read" => {
                // Count lines in the result
                let lines = result.output_text().lines().count();
                println!("Read {} lines", lines);
            }
            "file_write" | "file_edit" => {
                // Just show success message from the tool
                let msg = result.output_text().lines().next().unwrap_or("Done");
                let display = if msg.len() > 80 {
                    format!("{}...", &msg[..77])
                } else {
                    msg.to_string()
                };
                println!("{}", display);
            }
            "shell" => {
                // Show more comprehensive shell output
                print_shell_output(result.output_text())?;
            }
            "glob" => {
                // Show matched files with preview
                let output = result.output_text();
                let lines: Vec<_> = output.lines().collect();
                let count = lines.len();

                if count == 0 {
                    println!("No files found");
                } else if count <= 5 {
                    println!("Found {} files:", count);
                    for line in &lines {
                        stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                        println!("     {}", line);
                    }
                } else {
                    println!("Found {} files:", count);
                    for line in lines.iter().take(3) {
                        stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                        println!("     {}", line);
                    }
                    stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                    println!("     ... and {} more", count - 3);
                }
            }
            "grep" => {
                // Show matches with preview
                let output = result.output_text();
                let lines: Vec<_> = output.lines().collect();
                let count = lines.len();

                if count == 0 {
                    println!("No matches found");
                } else if count <= 5 {
                    println!("Found {} matches:", count);
                    for line in &lines {
                        let display = if line.len() > 100 {
                            format!("{}...", &line[..97])
                        } else {
                            line.to_string()
                        };
                        stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                        println!("     {}", display);
                    }
                } else {
                    println!("Found {} matches:", count);
                    for line in lines.iter().take(3) {
                        let display = if line.len() > 100 {
                            format!("{}...", &line[..97])
                        } else {
                            (*line).to_string()
                        };
                        stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                        println!("     {}", display);
                    }
                    stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                    println!("     ... and {} more matches", count - 3);
                }
            }
            _ => {
                println!("Done");
            }
        }
    }

    stdout.execute(ResetColor)?;
    stdout.flush()?;
    Ok(())
}

/// Print shell command output with smart formatting
pub(super) fn print_shell_output(output: &str) -> Result<()> {
    let mut stdout = io::stdout();

    // Parse the output to extract exit code and content
    let lines: Vec<_> = output.lines().collect();

    // Find exit code line
    let exit_code = lines
        .iter()
        .find(|l| l.starts_with("Exit code:"))
        .map(|l| l.strip_prefix("Exit code: ").unwrap_or("0"))
        .unwrap_or("0");

    // Get content lines (skip metadata)
    let content_lines: Vec<_> = lines
        .iter()
        .filter(|l| !l.starts_with("Exit code:") && !l.starts_with("---") && !l.is_empty())
        .collect();

    let total_lines = content_lines.len();

    if exit_code == "0" {
        if total_lines == 0 {
            println!("Command completed (no output)");
        } else if total_lines <= SHELL_OUTPUT_MAX_LINES {
            // Show all output
            println!("Command completed ({} lines):", total_lines);
            for line in &content_lines {
                stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                // Truncate very long lines
                let display = if line.len() > 120 {
                    format!("{}...", &line[..117])
                } else {
                    (*line).to_string()
                };
                println!("     {}", display);
            }
        } else {
            // Show first and last lines with summary
            println!("Command completed ({} lines):", total_lines);

            // Show first few lines
            let show_start = 5;
            let show_end = 5;

            for line in content_lines.iter().take(show_start) {
                stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                let display = if line.len() > 120 {
                    format!("{}...", &line[..117])
                } else {
                    (*line).to_string()
                };
                println!("     {}", display);
            }

            // Summary line
            let hidden = total_lines - show_start - show_end;
            if hidden > 0 {
                stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                println!("     ┄┄┄ {} more lines ┄┄┄", hidden);
            }

            // Show last few lines
            for line in content_lines.iter().skip(total_lines - show_end) {
                stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                let display = if line.len() > 120 {
                    format!("{}...", &line[..117])
                } else {
                    (*line).to_string()
                };
                println!("     {}", display);
            }
        }
    } else {
        // Non-zero exit - show more context for debugging
        stdout.execute(SetForegroundColor(Color::Red))?;
        println!("Command failed (exit code {})", exit_code);

        // For failures, show more output to help debug
        let show_lines = std::cmp::min(total_lines, 20);
        for line in content_lines.iter().take(show_lines) {
            stdout.execute(SetForegroundColor(Color::DarkGrey))?;
            let display = if line.len() > 120 {
                format!("{}...", &line[..117])
            } else {
                (*line).to_string()
            };
            println!("     {}", display);
        }

        if total_lines > show_lines {
            stdout.execute(SetForegroundColor(Color::DarkGrey))?;
            println!("     ... and {} more lines", total_lines - show_lines);
        }
    }

    stdout.execute(ResetColor)?;
    Ok(())
}
