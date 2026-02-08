// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Slash command execution
//!
//! Implements execution logic for development slash commands like /commit, /test,
//! /review, /fix, and /explain. Commands can execute shell commands, send messages
//! to the LLM, or spawn specialized agents.

use std::path::Path;

use crate::beads::{init_beads, Bead, BeadId, BeadStatus, BeadStore};
use crate::skills::SkillRegistry;

use super::commands::{
    BeadsArgs, CommitArgs, ExplainArgs, FixArgs, ModelArgs, ReviewArgs, SkillsArgs, TestArgs,
};
use super::input_parser::parse_bead_status;
use crate::models::{scan_for_models, DownloadRegistry, ModelCategory};

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

// === Skills Command Execution ===

/// Execute /skills command
pub fn execute_skills(args: &SkillsArgs, registry: &SkillRegistry) -> SlashCommandResult {
    match args.subcommand.as_deref() {
        None | Some("list") => execute_skills_list(registry),
        Some("show") => match &args.name {
            Some(name) => execute_skills_show(name, registry),
            None => SlashCommandResult::Error("Usage: /skills show <name>".to_string()),
        },
        Some("create") => match &args.name {
            Some(name) => execute_skills_create(name),
            None => SlashCommandResult::Error("Usage: /skills create <name>".to_string()),
        },
        Some(cmd) => SlashCommandResult::Error(format!(
            "Unknown skills subcommand: {}. Use: list, show <name>, create <name>",
            cmd
        )),
    }
}

/// List all available skills
fn execute_skills_list(registry: &SkillRegistry) -> SlashCommandResult {
    let metadata = registry.all_metadata();

    if metadata.is_empty() {
        return SlashCommandResult::Message(
            "No skills found.\n\n\
             Skills can be added to:\n\
             - .ted/skills/<name>/SKILL.md (project-local)\n\
             - ~/.ted/skills/<name>/SKILL.md (global)"
                .to_string(),
        );
    }

    let mut output = String::from("Available Skills\n");
    output.push_str("───────────────────────────────────────\n");

    for meta in &metadata {
        let source = if meta.source_path.to_string_lossy().contains(".ted/skills") {
            "[local]"
        } else {
            "[global]"
        };
        output.push_str(&format!(
            "  {} {} - {}\n",
            meta.name, source, meta.description
        ));
    }

    output.push_str("───────────────────────────────────────\n");
    output.push_str(&format!("{} skill(s) available\n", metadata.len()));
    output.push_str("\nUse /skills show <name> to view a skill's content");

    SlashCommandResult::Message(output)
}

/// Show a specific skill's content
fn execute_skills_show(name: &str, registry: &SkillRegistry) -> SlashCommandResult {
    match registry.load(name) {
        Ok(skill) => {
            let mut output = String::new();
            output.push_str(&format!("Skill: {}\n", skill.name));
            output.push_str(&format!("Description: {}\n", skill.description));
            output.push_str("───────────────────────────────────────\n");
            output.push_str(&skill.to_prompt_content());

            if let Some(perms) = &skill.tool_permissions {
                output.push_str("\n\n## Tool Permissions\n");
                if !perms.allow.is_empty() {
                    output.push_str(&format!("Allow: {}\n", perms.allow.join(", ")));
                }
                if !perms.deny.is_empty() {
                    output.push_str(&format!("Deny: {}\n", perms.deny.join(", ")));
                }
            }

            SlashCommandResult::Message(output)
        }
        Err(e) => SlashCommandResult::Error(format!("Failed to load skill '{}': {}", name, e)),
    }
}

/// Create a new skill (interactive - sends to LLM)
fn execute_skills_create(name: &str) -> SlashCommandResult {
    SlashCommandResult::SendToLlm(format!(
        "I want to create a new skill called '{}'. Please help me by:\n\n\
         1. First, ask me what domain or expertise this skill should cover\n\
         2. Then generate a SKILL.md file with the appropriate:\n\
            - YAML frontmatter (name, description, optional tool permissions)\n\
            - Markdown content with guidance and patterns\n\n\
         The skill file should be created at .ted/skills/{}/SKILL.md\n\n\
         What should this skill be about?",
        name, name
    ))
}

// === Beads Command Execution ===

/// Execute /beads command
pub fn execute_beads(args: &BeadsArgs, working_dir: &Path) -> SlashCommandResult {
    // Initialize bead store for the project
    let store = match init_beads(working_dir) {
        Ok(store) => store,
        Err(e) => {
            return SlashCommandResult::Error(format!("Failed to initialize beads storage: {}", e))
        }
    };

    execute_beads_with_store(args, &store)
}

/// Execute /beads command with an existing BeadStore
pub fn execute_beads_with_store(args: &BeadsArgs, store: &BeadStore) -> SlashCommandResult {
    match args.subcommand.as_deref() {
        None | Some("list") => execute_beads_list(store),
        Some("add") => match &args.value {
            Some(title) => execute_beads_add(title, store),
            None => SlashCommandResult::Error("Usage: /beads add <title>".to_string()),
        },
        Some("show") => match &args.id {
            Some(id) => execute_beads_show(id, store),
            None => SlashCommandResult::Error("Usage: /beads show <id>".to_string()),
        },
        Some("status") => match (&args.id, &args.value) {
            (Some(id), Some(status)) => execute_beads_status(id, status, store),
            (Some(_), None) => SlashCommandResult::Error(
                "Usage: /beads status <id> <status>\n\
                 Valid statuses: pending, ready, in-progress, done, blocked:<reason>, cancelled:<reason>"
                    .to_string(),
            ),
            (None, _) => SlashCommandResult::Error("Usage: /beads status <id> <status>".to_string()),
        },
        Some("stats") => execute_beads_stats(store),
        Some(cmd) => SlashCommandResult::Error(format!(
            "Unknown beads subcommand: {}. Use: list, add <title>, show <id>, status <id> <status>, stats",
            cmd
        )),
    }
}

/// List all beads
fn execute_beads_list(store: &BeadStore) -> SlashCommandResult {
    let beads = store.all();

    if beads.is_empty() {
        return SlashCommandResult::Message(
            "No beads found.\n\n\
             Create a bead with: /beads add <title>\n\
             Beads are stored in .beads/beads.jsonl"
                .to_string(),
        );
    }

    let mut output = String::from("Beads\n");
    output.push_str("───────────────────────────────────────\n");

    // Group by status
    let in_progress: Vec<_> = beads
        .iter()
        .filter(|b| matches!(b.status, BeadStatus::InProgress))
        .collect();
    let ready: Vec<_> = beads
        .iter()
        .filter(|b| matches!(b.status, BeadStatus::Ready))
        .collect();
    let pending: Vec<_> = beads
        .iter()
        .filter(|b| matches!(b.status, BeadStatus::Pending))
        .collect();
    let blocked: Vec<_> = beads
        .iter()
        .filter(|b| matches!(b.status, BeadStatus::Blocked { .. }))
        .collect();
    let done: Vec<_> = beads
        .iter()
        .filter(|b| matches!(b.status, BeadStatus::Done))
        .collect();

    if !in_progress.is_empty() {
        output.push_str("\n[In Progress]\n");
        for bead in in_progress {
            output.push_str(&format!("  {} - {}\n", bead.id, bead.title));
        }
    }

    if !ready.is_empty() {
        output.push_str("\n[Ready]\n");
        for bead in ready {
            output.push_str(&format!("  {} - {}\n", bead.id, bead.title));
        }
    }

    if !pending.is_empty() {
        output.push_str("\n[Pending]\n");
        for bead in pending {
            output.push_str(&format!("  {} - {}\n", bead.id, bead.title));
        }
    }

    if !blocked.is_empty() {
        output.push_str("\n[Blocked]\n");
        for bead in blocked {
            let reason = if let BeadStatus::Blocked { reason } = &bead.status {
                format!(" ({})", reason)
            } else {
                String::new()
            };
            output.push_str(&format!("  {} - {}{}\n", bead.id, bead.title, reason));
        }
    }

    if !done.is_empty() {
        output.push_str(&format!("\n[Done] ({} total)\n", done.len()));
        // Only show most recent 5 done items
        for bead in done.iter().take(5) {
            output.push_str(&format!("  {} - {}\n", bead.id, bead.title));
        }
        if done.len() > 5 {
            output.push_str(&format!("  ... and {} more\n", done.len() - 5));
        }
    }

    output.push_str("───────────────────────────────────────\n");
    let stats = store.stats();
    output.push_str(&format!(
        "{} total | {} done ({:.0}%)",
        stats.total,
        stats.done,
        stats.completion_percentage()
    ));

    SlashCommandResult::Message(output)
}

/// Add a new bead
fn execute_beads_add(title: &str, store: &BeadStore) -> SlashCommandResult {
    let bead = Bead::new(title, "");

    match store.create(bead) {
        Ok(id) => SlashCommandResult::Message(format!(
            "Created bead: {}\n\nUse /beads show {} for details",
            id, id
        )),
        Err(e) => SlashCommandResult::Error(format!("Failed to create bead: {}", e)),
    }
}

/// Show bead details
fn execute_beads_show(id: &str, store: &BeadStore) -> SlashCommandResult {
    let bead_id = BeadId::from(id);

    match store.get(&bead_id) {
        Some(bead) => {
            let mut output = String::new();
            output.push_str(&format!("Bead: {}\n", bead.id));
            output.push_str("───────────────────────────────────────\n");
            output.push_str(&format!("Title:       {}\n", bead.title));
            output.push_str(&format!("Status:      {:?}\n", bead.status));
            output.push_str(&format!("Priority:    {:?}\n", bead.priority));
            output.push_str(&format!(
                "Created:     {}\n",
                bead.created_at.format("%Y-%m-%d %H:%M")
            ));
            output.push_str(&format!(
                "Updated:     {}\n",
                bead.updated_at.format("%Y-%m-%d %H:%M")
            ));

            if !bead.description.is_empty() {
                output.push_str(&format!("\nDescription:\n{}\n", bead.description));
            }

            if !bead.depends_on.is_empty() {
                output.push_str("\nDependencies:\n");
                for dep in &bead.depends_on {
                    output.push_str(&format!("  - {}\n", dep));
                }
            }

            if !bead.tags.is_empty() {
                output.push_str(&format!("\nTags: {}\n", bead.tags.join(", ")));
            }

            if !bead.files_affected.is_empty() {
                output.push_str("\nAffected files:\n");
                for file in &bead.files_affected {
                    output.push_str(&format!("  - {}\n", file.display()));
                }
            }

            if !bead.notes.is_empty() {
                output.push_str("\nNotes:\n");
                for note in &bead.notes {
                    output.push_str(&format!(
                        "  [{} by {}]: {}\n",
                        note.timestamp.format("%Y-%m-%d %H:%M"),
                        note.author,
                        note.content
                    ));
                }
            }

            if let Some(summary) = &bead.compacted_summary {
                output.push_str(&format!("\nSummary:\n{}\n", summary));
            }

            SlashCommandResult::Message(output)
        }
        None => {
            // Try partial match
            let all_beads = store.all();
            let matches: Vec<_> = all_beads
                .iter()
                .filter(|b| b.id.as_str().contains(id))
                .collect();

            if matches.is_empty() {
                SlashCommandResult::Error(format!("Bead '{}' not found", id))
            } else if matches.len() == 1 {
                // Recursive call with full ID
                execute_beads_show(matches[0].id.as_str(), store)
            } else {
                let mut output = format!("Multiple beads match '{}'. Did you mean:\n", id);
                for bead in matches {
                    output.push_str(&format!("  {} - {}\n", bead.id, bead.title));
                }
                SlashCommandResult::Error(output)
            }
        }
    }
}

/// Update bead status
fn execute_beads_status(id: &str, status_str: &str, store: &BeadStore) -> SlashCommandResult {
    let bead_id = BeadId::from(id);

    // Check if bead exists, try partial match if not
    if store.get(&bead_id).is_none() {
        let all_beads = store.all();
        let matches: Vec<_> = all_beads
            .iter()
            .filter(|b| b.id.as_str().contains(id))
            .collect();

        if matches.is_empty() {
            return SlashCommandResult::Error(format!("Bead '{}' not found", id));
        } else if matches.len() > 1 {
            let mut output = format!("Multiple beads match '{}'. Please be more specific:\n", id);
            for bead in matches {
                output.push_str(&format!("  {} - {}\n", bead.id, bead.title));
            }
            return SlashCommandResult::Error(output);
        }

        // Use full ID from single match
        return execute_beads_status(matches[0].id.as_str(), status_str, store);
    }

    let status = match parse_bead_status(status_str) {
        Some(s) => s,
        None => {
            return SlashCommandResult::Error(format!(
                "Invalid status '{}'. Valid statuses:\n\
                 - pending\n\
                 - ready\n\
                 - in-progress (or: wip, inprogress)\n\
                 - done (or: complete, completed)\n\
                 - blocked:<reason>\n\
                 - cancelled:<reason> (or: canceled)",
                status_str
            ))
        }
    };

    match store.set_status(&bead_id, status.clone()) {
        Ok(()) => {
            SlashCommandResult::Message(format!("Updated {} status to {:?}", bead_id, status))
        }
        Err(e) => SlashCommandResult::Error(format!("Failed to update status: {}", e)),
    }
}

/// Show bead statistics
fn execute_beads_stats(store: &BeadStore) -> SlashCommandResult {
    let stats = store.stats();

    let mut output = String::from("Bead Statistics\n");
    output.push_str("───────────────────────────────────────\n");
    output.push_str(&format!("  Total:       {}\n", stats.total));
    output.push_str(&format!("  Pending:     {}\n", stats.pending));
    output.push_str(&format!("  Ready:       {}\n", stats.ready));
    output.push_str(&format!("  In Progress: {}\n", stats.in_progress));
    output.push_str(&format!("  Blocked:     {}\n", stats.blocked));
    output.push_str(&format!("  Done:        {}\n", stats.done));
    output.push_str(&format!("  Cancelled:   {}\n", stats.cancelled));
    output.push_str("───────────────────────────────────────\n");
    output.push_str(&format!(
        "  Progress:    {:.1}% complete\n",
        stats.completion_percentage()
    ));

    if stats.ready > 0 {
        output.push_str(&format!("\n{} task(s) ready to work on!", stats.ready));
    }

    SlashCommandResult::Message(output)
}

// === Model Command Execution ===

/// Execute /model command
pub fn execute_model(args: &ModelArgs) -> SlashCommandResult {
    match args.subcommand.as_deref() {
        None | Some("list") => execute_model_list(),
        Some("download") => match &args.name {
            Some(name) => execute_model_download(name, args.quantization.as_deref()),
            None => SlashCommandResult::Error("Usage: /model download <name> [-q QUANT]".to_string()),
        },
        Some("load") => match &args.name {
            Some(name) => execute_model_load(name),
            None => SlashCommandResult::Error("Usage: /model load <name>".to_string()),
        },
        Some("info") => match &args.name {
            Some(name) => execute_model_info(name),
            None => SlashCommandResult::Error("Usage: /model info <name>".to_string()),
        },
        Some("switch") => match &args.name {
            Some(name) => execute_model_switch(name),
            None => SlashCommandResult::Error("Usage: /model <model-name>".to_string()),
        },
        Some(cmd) => SlashCommandResult::Error(format!(
            "Unknown model subcommand: {}. Use: list, download <name>, load <name>, info <name>, or <model-name> to switch",
            cmd
        )),
    }
}

/// List available models from registry
fn execute_model_list() -> SlashCommandResult {
    let mut output = String::from("Local Models\n");
    output.push_str("───────────────────────────────────────\n\n");

    // Show discovered models on the system first
    let discovered = scan_for_models();
    if !discovered.is_empty() {
        output.push_str("[Installed on your system]\n");
        for model in &discovered {
            output.push_str(&format!(
                "  {} - {} ({})\n",
                model.filename,
                model.source,
                model.size_display()
            ));
        }
        output.push('\n');
    }

    // Use embedded registry for synchronous operation
    let registry = match DownloadRegistry::embedded() {
        Ok(r) => r,
        Err(e) => {
            return SlashCommandResult::Error(format!("Failed to load model registry: {}", e))
        }
    };

    // Group by category
    let code_models: Vec<_> = registry
        .models
        .iter()
        .filter(|m| m.category == ModelCategory::Code)
        .collect();
    let chat_models: Vec<_> = registry
        .models
        .iter()
        .filter(|m| m.category == ModelCategory::Chat)
        .collect();

    if !code_models.is_empty() {
        output.push_str("[Available for Download - Code]\n");
        for model in code_models {
            let recommended = if model.tags.contains(&"recommended".to_string()) {
                " ★"
            } else {
                ""
            };
            output.push_str(&format!("  {} - {}{}\n", model.id, model.name, recommended));
            output.push_str(&format!(
                "    Parameters: {} | Context: {}K\n",
                model.parameters,
                model.context_size / 1024
            ));
        }
        output.push('\n');
    }

    if !chat_models.is_empty() {
        output.push_str("[Available for Download - Chat]\n");
        for model in chat_models {
            let recommended = if model.tags.contains(&"recommended".to_string()) {
                " ★"
            } else {
                ""
            };
            output.push_str(&format!("  {} - {}{}\n", model.id, model.name, recommended));
            output.push_str(&format!(
                "    Parameters: {} | Context: {}K\n",
                model.parameters,
                model.context_size / 1024
            ));
        }
        output.push('\n');
    }

    output.push_str("───────────────────────────────────────\n");
    output.push_str(&format!(
        "{} installed, {} available for download\n",
        discovered.len(),
        registry.models.len()
    ));
    output.push_str("\nCommands:\n");
    output.push_str("  /model info <name>     - Show model details and variants\n");
    output.push_str("  /model download <name> - Download a model\n");
    output.push_str("  /model load <name>     - Load a downloaded model\n");
    output.push_str("  /model <name>          - Switch to cloud model\n");

    SlashCommandResult::Message(output)
}

/// Download a model
fn execute_model_download(name: &str, quantization: Option<&str>) -> SlashCommandResult {
    let quant = quantization.unwrap_or("q4_k_m");

    let registry = match DownloadRegistry::embedded() {
        Ok(r) => r,
        Err(e) => {
            return SlashCommandResult::Error(format!("Failed to load model registry: {}", e))
        }
    };

    let name_lower = name.to_lowercase();
    let model = registry
        .models
        .iter()
        .find(|m| m.id.to_lowercase() == name_lower || m.id.to_lowercase().contains(&name_lower));

    match model {
        Some(m) => {
            let quant_lower = quant.to_lowercase();
            let variant = m
                .variants
                .iter()
                .find(|v| format!("{:?}", v.quantization).to_lowercase() == quant_lower);

            match variant {
                Some(v) => {
                    let dest = dirs::home_dir()
                        .map(|h| h.join(".ted/models/local"))
                        .unwrap_or_else(|| std::path::PathBuf::from("~/.ted/models/local"));
                    let filename = format!("{}-{}.gguf", m.id, quant_lower);
                    let dest_path = dest.join(&filename);

                    SlashCommandResult::SendToLlm(format!(
                        "Download this model file using curl or wget:\n\n\
                         Model: {} ({})\n\
                         Size: {}\n\
                         URL: {}\n\
                         Destination: {}\n\n\
                         First create the directory if needed:\n\
                         mkdir -p \"{}\"\n\n\
                         Then download:\n\
                         curl -L --progress-bar -o \"{}\" \"{}\"\n\n\
                         Run these commands now to download the model.",
                        m.name,
                        quant,
                        v.size_display(),
                        v.url,
                        dest_path.display(),
                        dest.display(),
                        dest_path.display(),
                        v.url
                    ))
                }
                None => {
                    let available: Vec<String> = m
                        .variants
                        .iter()
                        .map(|v| format!("{:?}", v.quantization).to_lowercase())
                        .collect();
                    SlashCommandResult::Error(format!(
                        "Quantization '{}' not available for {}.\nAvailable: {}",
                        quant,
                        m.name,
                        available.join(", ")
                    ))
                }
            }
        }
        None => {
            let similar: Vec<_> = registry
                .models
                .iter()
                .filter(|m| {
                    m.id.to_lowercase().contains(&name_lower)
                        || m.name.to_lowercase().contains(&name_lower)
                })
                .take(5)
                .collect();

            if similar.is_empty() {
                SlashCommandResult::Error(format!(
                    "Model '{}' not found. Use /model list to see available models.",
                    name
                ))
            } else {
                let mut output = format!("Model '{}' not found. Did you mean:\n", name);
                for m in similar {
                    output.push_str(&format!("  {} - {}\n", m.id, m.name));
                }
                SlashCommandResult::Error(output)
            }
        }
    }
}

/// Load a model for inference
fn execute_model_load(name: &str) -> SlashCommandResult {
    let discovered = scan_for_models();
    let name_lower = name.to_lowercase();

    let matching: Vec<_> = discovered
        .iter()
        .filter(|m| m.filename.to_lowercase().contains(&name_lower))
        .collect();

    match matching.len() {
        0 => {
            let mut output = format!("No model matching '{}' found on your system.\n", name);
            output.push_str(&format!("Download it with: /model download {}\n", name));
            if !discovered.is_empty() {
                output.push_str("\nAvailable models:\n");
                for m in &discovered {
                    output.push_str(&format!(
                        "  {} ({}, {})\n",
                        m.filename,
                        m.source,
                        m.size_display()
                    ));
                }
            }
            SlashCommandResult::Error(output)
        }
        1 => {
            let model = &matching[0];
            SlashCommandResult::Message(format!(
                "Found: {} ({})\n\
                 Path: {}\n\n\
                 To use this model, update your settings:\n\
                 Set providers.local.model_path = \"{}\"",
                model.display_name(),
                model.size_display(),
                model.path.display(),
                model.path.display()
            ))
        }
        _ => {
            let mut output = format!("Multiple models matching '{}':\n", name);
            for (i, m) in matching.iter().enumerate() {
                output.push_str(&format!(
                    "  {}. {} ({})\n",
                    i + 1,
                    m.display_name(),
                    m.size_display()
                ));
            }
            output.push_str("\nSpecify a more precise name.");
            SlashCommandResult::Message(output)
        }
    }
}

/// Show model info
fn execute_model_info(name: &str) -> SlashCommandResult {
    // Use embedded registry for synchronous operation
    let registry = match DownloadRegistry::embedded() {
        Ok(r) => r,
        Err(e) => {
            return SlashCommandResult::Error(format!("Failed to load model registry: {}", e))
        }
    };

    // Find model by ID (case-insensitive, partial match)
    let name_lower = name.to_lowercase();
    let model = registry
        .models
        .iter()
        .find(|m| m.id.to_lowercase() == name_lower || m.id.to_lowercase().contains(&name_lower));

    match model {
        Some(m) => {
            let mut output = String::new();
            output.push_str(&format!("Model: {}\n", m.name));
            output.push_str("───────────────────────────────────────\n");
            output.push_str(&format!("  ID:         {}\n", m.id));
            output.push_str(&format!("  Category:   {}\n", m.category.display_name()));
            output.push_str(&format!("  Parameters: {}\n", m.parameters));
            output.push_str(&format!("  Context:    {} tokens\n", m.context_size));
            output.push_str(&format!("  Base:       {}\n", m.base_model));
            output.push_str(&format!("  Creator:    {}\n", m.creator));
            output.push_str(&format!("  License:    {}\n", m.license));

            if !m.tags.is_empty() {
                output.push_str(&format!("  Tags:       {}\n", m.tags.join(", ")));
            }

            output.push_str("\nAvailable Quantizations:\n");
            for variant in &m.variants {
                let size_gb = variant.size_bytes as f64 / 1_073_741_824.0;
                output.push_str(&format!(
                    "  {} - {:.1}GB (min VRAM: {:.0}GB)\n",
                    variant.quantization.display_name(),
                    size_gb,
                    variant.min_vram_gb
                ));
            }

            output.push_str(&format!(
                "\nDownload: /model download {} [-q QUANT]\n",
                m.id
            ));

            SlashCommandResult::Message(output)
        }
        None => {
            // Try to find similar models
            let similar: Vec<_> = registry
                .models
                .iter()
                .filter(|m| {
                    m.id.to_lowercase().contains(&name_lower)
                        || m.name.to_lowercase().contains(&name_lower)
                })
                .take(5)
                .collect();

            if similar.is_empty() {
                SlashCommandResult::Error(format!(
                    "Model '{}' not found. Use /model list to see available models.",
                    name
                ))
            } else {
                let mut output = format!("Model '{}' not found. Did you mean:\n", name);
                for m in similar {
                    output.push_str(&format!("  {} - {}\n", m.id, m.name));
                }
                SlashCommandResult::Error(output)
            }
        }
    }
}

/// Switch to a different (cloud) model
fn execute_model_switch(name: &str) -> SlashCommandResult {
    // For cloud models, just return a message indicating the switch
    // The actual model switching happens in the chat runner
    SlashCommandResult::Message(format!(
        "Switching to model: {}\n\n\
         Note: If this is a cloud model (claude-*, gpt-*, etc.), the switch will take effect immediately.\n\
         For local models, use /model load <name> instead.",
        name
    ))
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

    // === Skills Command Tests ===

    #[test]
    fn test_execute_skills_list_empty() {
        let temp = TempDir::new().unwrap();
        let mut registry = SkillRegistry::with_paths(vec![temp.path().to_path_buf()]);
        registry.scan().unwrap();

        let args = SkillsArgs::default();
        let result = execute_skills(&args, &registry);

        match result {
            SlashCommandResult::Message(msg) => {
                assert!(msg.contains("No skills found"));
                assert!(msg.contains(".ted/skills"));
            }
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_execute_skills_show_not_found() {
        let temp = TempDir::new().unwrap();
        let mut registry = SkillRegistry::with_paths(vec![temp.path().to_path_buf()]);
        registry.scan().unwrap();

        let args = SkillsArgs {
            subcommand: Some("show".to_string()),
            name: Some("nonexistent".to_string()),
        };
        let result = execute_skills(&args, &registry);

        match result {
            SlashCommandResult::Error(msg) => {
                assert!(msg.contains("nonexistent"));
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_execute_skills_create() {
        let temp = TempDir::new().unwrap();
        let mut registry = SkillRegistry::with_paths(vec![temp.path().to_path_buf()]);
        registry.scan().unwrap();

        let args = SkillsArgs {
            subcommand: Some("create".to_string()),
            name: Some("my-skill".to_string()),
        };
        let result = execute_skills(&args, &registry);

        match result {
            SlashCommandResult::SendToLlm(msg) => {
                assert!(msg.contains("my-skill"));
                assert!(msg.contains("SKILL.md"));
            }
            _ => panic!("Expected SendToLlm"),
        }
    }

    // === Beads Command Tests ===

    #[test]
    fn test_execute_beads_list_empty() {
        let temp = TempDir::new().unwrap();
        let store = init_beads(temp.path()).unwrap();

        let args = BeadsArgs::default();
        let result = execute_beads_with_store(&args, &store);

        match result {
            SlashCommandResult::Message(msg) => {
                assert!(msg.contains("No beads found"));
                assert!(msg.contains("/beads add"));
            }
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_execute_beads_add() {
        let temp = TempDir::new().unwrap();
        let store = init_beads(temp.path()).unwrap();

        let args = BeadsArgs {
            subcommand: Some("add".to_string()),
            id: None,
            value: Some("Test task".to_string()),
        };
        let result = execute_beads_with_store(&args, &store);

        match result {
            SlashCommandResult::Message(msg) => {
                assert!(msg.contains("Created bead"));
                assert!(msg.contains("bd-"));
            }
            _ => panic!("Expected Message"),
        }

        // Verify bead was created
        let beads = store.all();
        assert_eq!(beads.len(), 1);
        assert_eq!(beads[0].title, "Test task");
    }

    #[test]
    fn test_execute_beads_show() {
        let temp = TempDir::new().unwrap();
        let store = init_beads(temp.path()).unwrap();

        // Create a bead first
        let bead = Bead::new("Show me", "Description here");
        let id = store.create(bead).unwrap();

        let args = BeadsArgs {
            subcommand: Some("show".to_string()),
            id: Some(id.to_string()),
            value: None,
        };
        let result = execute_beads_with_store(&args, &store);

        match result {
            SlashCommandResult::Message(msg) => {
                assert!(msg.contains("Show me"));
                assert!(msg.contains("Title:"));
                assert!(msg.contains("Status:"));
            }
            _ => panic!("Expected Message"),
        }
    }

    #[test]
    fn test_execute_beads_show_not_found() {
        let temp = TempDir::new().unwrap();
        let store = init_beads(temp.path()).unwrap();

        let args = BeadsArgs {
            subcommand: Some("show".to_string()),
            id: Some("bd-nonexistent".to_string()),
            value: None,
        };
        let result = execute_beads_with_store(&args, &store);

        match result {
            SlashCommandResult::Error(msg) => {
                assert!(msg.contains("not found"));
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_execute_beads_status_update() {
        let temp = TempDir::new().unwrap();
        let store = init_beads(temp.path()).unwrap();

        // Create a bead first
        let bead = Bead::new("Status test", "");
        let id = store.create(bead).unwrap();

        let args = BeadsArgs {
            subcommand: Some("status".to_string()),
            id: Some(id.to_string()),
            value: Some("done".to_string()),
        };
        let result = execute_beads_with_store(&args, &store);

        match result {
            SlashCommandResult::Message(msg) => {
                assert!(msg.contains("Updated"));
                assert!(msg.contains("Done"));
            }
            _ => panic!("Expected Message, got {:?}", result),
        }

        // Verify status was updated
        let updated = store.get(&id).unwrap();
        assert!(matches!(updated.status, BeadStatus::Done));
    }

    #[test]
    fn test_execute_beads_status_invalid() {
        let temp = TempDir::new().unwrap();
        let store = init_beads(temp.path()).unwrap();

        // Create a bead first
        let bead = Bead::new("Status test", "");
        let id = store.create(bead).unwrap();

        let args = BeadsArgs {
            subcommand: Some("status".to_string()),
            id: Some(id.to_string()),
            value: Some("invalid-status".to_string()),
        };
        let result = execute_beads_with_store(&args, &store);

        match result {
            SlashCommandResult::Error(msg) => {
                assert!(msg.contains("Invalid status"));
                assert!(msg.contains("pending"));
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_execute_beads_stats() {
        let temp = TempDir::new().unwrap();
        let store = init_beads(temp.path()).unwrap();

        // Create some beads with different statuses
        let bead1 = Bead::new("Task 1", "");
        let bead2 = Bead::new("Task 2", "");
        let bead3 = Bead::new("Task 3", "");

        let id1 = store.create(bead1).unwrap();
        store.create(bead2).unwrap();
        store.create(bead3).unwrap();

        // Mark one as done
        store.set_status(&id1, BeadStatus::Done).unwrap();

        let args = BeadsArgs {
            subcommand: Some("stats".to_string()),
            id: None,
            value: None,
        };
        let result = execute_beads_with_store(&args, &store);

        match result {
            SlashCommandResult::Message(msg) => {
                assert!(msg.contains("Bead Statistics"));
                assert!(msg.contains("Total:       3"));
                assert!(msg.contains("Done:        1"));
                assert!(msg.contains("33.3% complete"));
            }
            _ => panic!("Expected Message"),
        }
    }
}
