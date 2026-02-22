// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use crate::chat::input_parser::{
    parse_beads_command, parse_commit_command, parse_explain_command, parse_fix_command,
    parse_model_command, parse_review_command, parse_skills_command, parse_test_command,
};
use crate::chat::slash_commands::{
    execute_beads, execute_commit, execute_explain, execute_fix, execute_model, execute_review,
    execute_skills, execute_test, SlashCommandResult,
};
use crate::error::Result;
use crate::llm::message::{Conversation, Message};
use crate::skills::SkillRegistry;
use crate::tui::chat::state::DisplayMessage;

use super::super::app::ChatMode;
use super::{SettingsSection, TuiState};

pub(super) fn handle_command(
    input: &str,
    state: &mut TuiState,
    conversation: Option<&mut Conversation>,
) -> Result<()> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    tracing::debug!(
        target: "ted.tui.runner",
        command = %trimmed,
        "processing slash command"
    );

    // Check for /model commands (with subcommands or model switch)
    if lower.starts_with("/model") {
        if let Some(args) = parse_model_command(trimmed) {
            // Check if it's a simple model switch (subcommand = "switch")
            if args.subcommand.as_deref() == Some("switch") {
                if let Some(ref model_name) = args.name {
                    state.current_model = model_name.clone();
                    state.set_status(&format!("Model set to: {}", model_name));
                    return Ok(());
                }
            }

            // Handle other subcommands via execute_model
            match execute_model(&args) {
                SlashCommandResult::SendToLlm(msg) => {
                    state.pending_messages.push(msg);
                    state.set_status("Processing /model...");
                }
                SlashCommandResult::Message(msg) => {
                    state.messages.push(DisplayMessage::system(msg));
                    state.auto_scroll();
                }
                SlashCommandResult::Error(e) => {
                    state.set_error(&e);
                }
                SlashCommandResult::SpawnAgent { task, .. } => {
                    state.pending_messages.push(task);
                    state.set_status("Processing...");
                }
            }
            return Ok(());
        }
    }

    if lower.starts_with("/cap ") {
        // Toggle a specific cap: /cap <name>
        let cap_name = trimmed[5..].trim();
        if cap_name.is_empty() {
            state.set_error("Usage: /cap <name> to toggle a capability");
            return Ok(());
        }

        // Check if cap exists
        let cap_exists = state
            .available_caps
            .iter()
            .any(|(name, _)| name == cap_name);
        if !cap_exists {
            state.set_error(&format!(
                "Unknown cap: {}. Use /caps to see available.",
                cap_name
            ));
            return Ok(());
        }

        // Toggle the cap
        if let Some(pos) = state.enabled_caps.iter().position(|c| c == cap_name) {
            state.enabled_caps.remove(pos);
            state.set_status(&format!("Disabled cap: {}", cap_name));
        } else {
            state.enabled_caps.push(cap_name.to_string());
            state.set_status(&format!("Enabled cap: {}", cap_name));
        }

        // Update config.caps so new messages use updated caps
        state.config.caps = state.enabled_caps.clone();
        return Ok(());
    }

    match lower.as_str() {
        "/help" => {
            state.mode = ChatMode::Help;
        }
        "/clear" => {
            state.messages.clear();
            state.set_status("Chat cleared");
        }
        "/agents" => {
            state.agent_pane_visible = !state.agent_pane_visible;
        }
        // "/model" is handled above with parse_model_command
        "/settings" => {
            // Open settings editor
            state.mode = ChatMode::Settings;
        }
        "/caps" => {
            // Open settings on Capabilities tab
            if let Some(ref mut settings) = state.settings_state {
                settings.current_section = SettingsSection::Capabilities;
            }
            state.mode = ChatMode::Settings;
        }
        // Development slash commands
        cmd if cmd.starts_with("/commit") => {
            if let Some(args) = parse_commit_command(trimmed) {
                let working_dir = std::env::current_dir().unwrap_or_default();
                match execute_commit(&args, &working_dir) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /commit...");
                    }
                    SlashCommandResult::Message(msg) => {
                        state.messages.push(DisplayMessage::system(msg));
                        state.auto_scroll();
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        // Convert to LLM message for now
                        state.pending_messages.push(task);
                        state.set_status("Processing...");
                    }
                }
            } else {
                state.set_error("Failed to parse /commit command");
            }
        }
        cmd if cmd.starts_with("/test") => {
            if let Some(args) = parse_test_command(trimmed) {
                let working_dir = std::env::current_dir().unwrap_or_default();
                match execute_test(&args, &working_dir) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /test...");
                    }
                    SlashCommandResult::Message(msg) => {
                        state.messages.push(DisplayMessage::system(msg));
                        state.auto_scroll();
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        state.pending_messages.push(task);
                        state.set_status("Processing...");
                    }
                }
            } else {
                state.set_error("Failed to parse /test command");
            }
        }
        cmd if cmd.starts_with("/review") => {
            if let Some(args) = parse_review_command(trimmed) {
                let working_dir = std::env::current_dir().unwrap_or_default();
                match execute_review(&args, &working_dir) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /review...");
                    }
                    SlashCommandResult::Message(msg) => {
                        state.messages.push(DisplayMessage::system(msg));
                        state.auto_scroll();
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        // Convert agent task to LLM message
                        state.pending_messages.push(task);
                        state.set_status("Starting code review...");
                    }
                }
            } else {
                state.set_error("Failed to parse /review command");
            }
        }
        cmd if cmd.starts_with("/fix") => {
            if let Some(args) = parse_fix_command(trimmed) {
                let working_dir = std::env::current_dir().unwrap_or_default();
                match execute_fix(&args, &working_dir) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /fix...");
                    }
                    SlashCommandResult::Message(msg) => {
                        state.messages.push(DisplayMessage::system(msg));
                        state.auto_scroll();
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        // Convert agent task to LLM message
                        state.pending_messages.push(task);
                        state.set_status("Fixing issues...");
                    }
                }
            } else {
                state.set_error("Failed to parse /fix command");
            }
        }
        cmd if cmd.starts_with("/explain") => {
            if let Some(args) = parse_explain_command(trimmed) {
                match execute_explain(&args) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /explain...");
                    }
                    SlashCommandResult::Message(msg) => {
                        state.messages.push(DisplayMessage::system(msg));
                        state.auto_scroll();
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        state.pending_messages.push(task);
                        state.set_status("Processing...");
                    }
                }
            } else {
                state.set_error("Failed to parse /explain command");
            }
        }
        cmd if cmd.starts_with("/skill") => {
            if let Some(args) = parse_skills_command(trimmed) {
                // Create a skill registry and scan for skills
                let mut registry = SkillRegistry::new();
                if let Err(e) = registry.scan() {
                    state.set_error(&format!("Failed to scan skills: {}", e));
                    return Ok(());
                }
                match execute_skills(&args, &registry) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /skills...");
                    }
                    SlashCommandResult::Message(msg) => {
                        state.messages.push(DisplayMessage::system(msg));
                        state.auto_scroll();
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        state.pending_messages.push(task);
                        state.set_status("Processing...");
                    }
                }
            } else {
                state.set_error("Failed to parse /skills command");
            }
        }
        cmd if cmd.starts_with("/bead") => {
            if let Some(args) = parse_beads_command(trimmed) {
                let working_dir = std::env::current_dir().unwrap_or_default();
                match execute_beads(&args, &working_dir) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /beads...");
                    }
                    SlashCommandResult::Message(msg) => {
                        // Display in UI as system message
                        state.messages.push(DisplayMessage::system(msg.clone()));
                        state.auto_scroll();

                        // Also add to conversation so LLM can see and act on pending beads
                        // This allows the assistant to proactively work on tasks
                        if let Some(conv) = conversation {
                            let context_msg = format!(
                                "[System: User ran /beads command. Current task status:]\n{}",
                                msg
                            );
                            conv.push(Message::user(&context_msg));
                        }
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        state.pending_messages.push(task);
                        state.set_status("Processing...");
                    }
                }
            } else {
                state.set_error("Failed to parse /beads command");
            }
        }
        _ => {
            tracing::warn!(
                target: "ted.tui.runner",
                command = %trimmed,
                "unknown slash command"
            );
            state.set_error(&format!("Unknown command: {}. Try /help", trimmed));
        }
    }

    Ok(())
}
