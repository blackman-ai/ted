// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Ted - AI coding assistant for your terminal
//!
//! Entry point for the Ted CLI application.

#![allow(unused_assignments)]

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::Parser;
use crossterm::{
    style::{Color, ResetColor, SetForegroundColor},
    ExecutableCommand,
};

use ted::caps::CapLoader;
#[cfg(test)]
use ted::caps::CapResolver;
use ted::chat;
use ted::cli::{ChatArgs, Cli, Commands};
use ted::commands;
use ted::config::Settings;
#[cfg(test)]
use ted::context::ContextManager;
use ted::context::SessionId;
use ted::error::Result;
#[cfg(test)]
use ted::error::TedError;
#[cfg(test)]
use ted::history::HistoryStore;
use ted::history::SessionInfo;
use ted::llm::factory::ProviderFactory;
#[cfg(test)]
use ted::llm::message::Conversation;
use ted::llm::message::Message;
#[cfg(test)]
use ted::llm::message::{ContentBlock, MessageContent};
use ted::plans::PlanStore;
#[cfg(test)]
use ted::tools::ToolResult;
use ted::tools::{ToolContext, ToolExecutor};
use ted::tui::chat::{run_chat_tui_loop, ChatTuiConfig};
use ted::utils;

#[path = "main/agent_loop.rs"]
mod agent_loop;
#[path = "main/chat_runtime.rs"]
mod chat_runtime;
#[path = "main/chat_ui.rs"]
mod chat_ui;
#[path = "main/cli_commands.rs"]
mod cli_commands;

use agent_loop::run_agent_loop;
#[cfg(test)]
use agent_loop::{get_response_with_retry, run_agent_loop_inner, stream_response};
#[cfg(test)]
use chat_runtime::check_provider_configuration;
use chat_runtime::{apply_thermal_guardrails, initialize_chat_runtime, ChatRuntimeSetup};
#[cfg(test)]
use chat_ui::{print_shell_output, SHELL_OUTPUT_MAX_LINES};
use chat_ui::{print_tool_invocation, print_tool_result};
#[cfg(test)]
use chat_ui::{prompt_session_choice, resume_session};
use cli_commands::{
    print_cap_badge, print_help, print_response_prefix, print_welcome, read_user_input, run_ask,
    run_caps_command, run_clear, run_context_command, run_custom_command, run_history_command,
    run_init, run_settings_command, run_settings_tui, run_update_command,
};

/// Maximum number of retries for rate-limited requests
#[cfg(test)]
const MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff (in seconds)
#[cfg(test)]
const BASE_RETRY_DELAY: u64 = 2;

// ==================== Pure Helper Functions ====================
// Most pure helper functions have been moved to the ted::chat module
// for better testability. See:
// - ted::chat::input_parser - Input parsing functions
// - ted::chat::commands - Command handling
// - ted::chat::display - Display formatting
// - ted::chat::agent - Agent loop logic
// - ted::chat::streaming - Streaming response handling
// - ted::chat::provider_config - Provider configuration

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Initialize tracing
    let mut env_filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive(tracing::Level::WARN.into());

    // Practical debug toggle: `-v` enables chat runtime diagnostics without requiring
    // users to know target names up front. `RUST_LOG` still takes precedence.
    if cli.verbose > 0 {
        for directive in [
            "ted.chat.engine=debug",
            "ted.tui.runner=debug",
            "ted.embedded=debug",
        ] {
            if let Ok(parsed) = directive.parse() {
                env_filter = env_filter.add_directive(parsed);
            }
        }
    }

    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    // Load settings
    let settings = Settings::load()?;

    // Ensure directories exist
    Settings::ensure_directories()?;

    // Dispatch to appropriate command
    let verbose = cli.verbose;
    match cli.command {
        None => {
            run_chat(ChatArgs::default(), settings, verbose).await?;
        }
        Some(Commands::Chat(args)) => {
            run_chat(args, settings, verbose).await?;
        }
        Some(Commands::Ask(args)) => {
            run_ask(args, settings, verbose).await?;
        }
        Some(Commands::Clear) => {
            run_clear()?;
        }
        Some(Commands::Settings(args)) => {
            if args.command.is_none() {
                run_settings_tui()?;
            } else {
                run_settings_command(args, settings)?;
            }
        }
        Some(Commands::Caps(args)) => {
            run_caps_command(args)?;
        }
        Some(Commands::History(args)) => {
            run_history_command(args)?;
        }
        Some(Commands::Context(args)) => {
            run_context_command(args, &settings).await?;
        }
        Some(Commands::Init) => {
            run_init()?;
        }
        Some(Commands::Update(args)) => {
            run_update_command(args).await?;
        }
        Some(Commands::System(args)) => {
            commands::system::execute(&args, &cli.format)?;
        }
        Some(Commands::Mcp(args)) => {
            commands::mcp::execute(&args).await?;
        }
        Some(Commands::Lsp) => {
            ted::lsp::start_server().await?;
        }
        Some(Commands::Run(args)) => {
            run_custom_command(args)?;
        }
    }

    Ok(())
}

/// Run interactive chat mode
async fn run_chat(args: ChatArgs, mut settings: Settings, verbose: u8) -> Result<()> {
    // Verbose output for debugging
    if verbose > 0 {
        eprintln!("[verbose] Ted starting in chat mode");
        eprintln!(
            "[verbose] Working directory: {:?}",
            std::env::current_dir().ok()
        );
    }
    if verbose > 1 {
        eprintln!("[verbose:2] Settings: {:?}", settings.defaults);
        eprintln!("[verbose:2] Trust mode: {}", args.trust);
    }

    apply_thermal_guardrails(&mut settings, verbose);

    // Check for embedded mode (JSONL output for GUI integration)
    if args.embedded {
        return ted::embedded_runner::run_embedded_chat(args, settings).await;
    }

    let ChatRuntimeSetup {
        mut settings,
        mut provider,
        mut provider_name,
        mut model,
        mut cap_names,
        mut merged_cap,
        mut loader,
        resolver,
        mut conversation,
        context_manager,
        mut tool_executor,
        mut history_store,
        skill_registry,
        mut session_id,
        mut session_info,
        mut message_count,
        working_directory,
        project_root,
        rate_coordinator,
        _compaction_handle,
    } = initialize_chat_runtime(&args, settings, verbose).await?;

    // Use TUI or simple mode
    if !args.no_tui {
        // TUI mode requires trust mode for tool permissions since interactive
        // prompts don't work in raw terminal mode. We'll add a TUI permission
        // dialog in the future, but for now auto-approve is necessary.
        let tui_trust_mode = true;

        // Set environment variable to suppress agent output in TUI mode
        std::env::set_var("TED_TUI_MODE", "1");

        // Recreate tool executor with trust mode for TUI
        let tui_tool_context = ToolContext::new(
            working_directory.clone(),
            project_root.clone(),
            session_id.0,
            tui_trust_mode,
        )
        .with_files_in_context(args.files_in_context.clone());
        let mut tui_tool_executor = ToolExecutor::new(tui_tool_context, tui_trust_mode);

        // Re-register spawn_agent tool for TUI executor with progress tracking
        let agent_progress_tracker = tui_tool_executor
            .registry_mut()
            .register_spawn_agent_with_progress(
                provider.clone(),
                skill_registry.clone(),
                model.to_string(),
            );

        // Run TUI mode
        let tui_config = ChatTuiConfig {
            session_id: session_id.0,
            provider_name: provider_name.clone(),
            model: model.clone(),
            caps: cap_names.clone(),
            trust_mode: tui_trust_mode,
            stream_enabled: !args.no_stream,
        };

        return run_chat_tui_loop(
            tui_config,
            provider,
            tui_tool_executor,
            context_manager,
            settings,
            conversation,
            history_store,
            session_info,
            agent_progress_tracker,
        )
        .await;
    }

    // Print welcome message with cap info (only for simple mode)
    print_welcome(
        &provider_name,
        &model,
        args.trust,
        &session_id,
        &merged_cap.source_caps,
    )?;

    // Main chat loop (simple mode - used when --no-tui is set)
    loop {
        // Get user input
        let input = read_user_input()?;

        // Check for shell command (starts with >)
        if input.trim().starts_with('>') {
            // Safe: we already verified the string starts with '>'
            let command = input
                .trim()
                .strip_prefix('>')
                .expect("input starts with '>'")
                .trim();
            if command.is_empty() {
                println!("\nUsage: >command [args...]");
                println!("Example: >ls -la");
                println!("Example: >git status\n");
                continue;
            }

            // Execute the shell command directly
            let mut stdout = io::stdout();
            stdout.execute(SetForegroundColor(Color::DarkGrey))?;
            print!("  ╭─ ");
            stdout.execute(SetForegroundColor(Color::Magenta))?;
            print!("shell");
            stdout.execute(SetForegroundColor(Color::DarkGrey))?;
            print!(" → ");
            stdout.execute(SetForegroundColor(Color::Cyan))?;
            let display_cmd = if command.len() > 60 {
                format!("{}...", &command[..57])
            } else {
                command.to_string()
            };
            println!("{}", display_cmd);
            stdout.execute(ResetColor)?;

            // Execute the command using std::process::Command
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .output();

            match output {
                Ok(result) => {
                    let stdout_text = String::from_utf8_lossy(&result.stdout);
                    let stderr_text = String::from_utf8_lossy(&result.stderr);
                    let exit_code = result.status.code().unwrap_or(-1);

                    // Print result indicator
                    let mut stdout = io::stdout();
                    stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                    print!("  ╰─ ");

                    if exit_code == 0 {
                        stdout.execute(SetForegroundColor(Color::Green))?;
                        print!("✓ ");
                    } else {
                        stdout.execute(SetForegroundColor(Color::Red))?;
                        print!("✗ ");
                    }
                    stdout.execute(ResetColor)?;

                    // Print output
                    if exit_code == 0 {
                        if stdout_text.trim().is_empty() && stderr_text.trim().is_empty() {
                            println!("Command completed (no output)");
                        } else {
                            let output_lines: Vec<_> =
                                stdout_text.lines().chain(stderr_text.lines()).collect();
                            let total_lines = output_lines.len();

                            if total_lines <= 15 {
                                println!("Command completed ({} lines):", total_lines);
                                for line in &output_lines {
                                    stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                                    let display = if line.len() > 120 {
                                        format!("{}...", &line[..117])
                                    } else {
                                        line.to_string()
                                    };
                                    println!("     {}", display);
                                    stdout.execute(ResetColor)?;
                                }
                            } else {
                                println!("Command completed ({} lines):", total_lines);
                                // Show first 5 lines
                                for line in output_lines.iter().take(5) {
                                    stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                                    let display = if line.len() > 120 {
                                        format!("{}...", &line[..117])
                                    } else {
                                        line.to_string()
                                    };
                                    println!("     {}", display);
                                    stdout.execute(ResetColor)?;
                                }
                                stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                                println!("     ┄┄┄ {} more lines ┄┄┄", total_lines - 10);
                                // Show last 5 lines
                                for line in output_lines.iter().skip(total_lines - 5) {
                                    let display = if line.len() > 120 {
                                        format!("{}...", &line[..117])
                                    } else {
                                        line.to_string()
                                    };
                                    println!("     {}", display);
                                }
                                stdout.execute(ResetColor)?;
                            }
                        }
                    } else {
                        println!("Command failed (exit code {})", exit_code);
                        let error_output = if !stderr_text.trim().is_empty() {
                            stderr_text.trim()
                        } else {
                            stdout_text.trim()
                        };

                        for line in error_output.lines().take(10) {
                            stdout.execute(SetForegroundColor(Color::DarkGrey))?;
                            let display = if line.len() > 120 {
                                format!("{}...", &line[..117])
                            } else {
                                line.to_string()
                            };
                            println!("     {}", display);
                            stdout.execute(ResetColor)?;
                        }
                    }
                }
                Err(e) => {
                    stdout.execute(SetForegroundColor(Color::Red))?;
                    println!("✗ Failed to execute command: {}", e);
                    stdout.execute(ResetColor)?;
                }
            }

            println!(); // Add spacing after command output
            continue;
        }

        // Check for exit commands
        let trimmed = input.trim().to_lowercase();
        if trimmed == "exit" || trimmed == "quit" || trimmed == "/exit" || trimmed == "/quit" {
            println!("\nGoodbye!");
            break;
        }

        // Check for clear command
        if trimmed == "/clear" {
            conversation.clear();
            context_manager.clear().await?;
            println!("\nContext cleared.\n");
            continue;
        }

        // Check for help command
        if trimmed == "/help" {
            print_help()?;
            continue;
        }

        // Check for stats/context command
        if trimmed == "/stats" || trimmed == "/context" {
            let stats = context_manager.stats().await;
            println!("\nContext Statistics");
            println!("─────────────────────────────────────");
            println!(
                "  Session ID:      {}",
                &stats.session_id[..8.min(stats.session_id.len())]
            );
            println!("  Model:           {}", model);
            println!("  Messages:        {}", message_count);
            println!();
            println!("  Storage:");
            println!("    Total chunks:  {}", stats.total_chunks);
            println!("    Hot (cache):   {}", stats.hot_chunks);
            println!("    Warm (disk):   {}", stats.warm_chunks);
            println!("    Cold (archive):{}", stats.cold_chunks);
            println!();
            println!("  Tokens:          ~{}", stats.total_tokens);
            if stats.storage_bytes > 0 {
                let kb = stats.storage_bytes as f64 / 1024.0;
                println!("  Storage:         {:.1} KB", kb);
            }
            println!();
            print!("  Active caps:     ");
            if cap_names.is_empty() {
                println!("(none)");
            } else {
                for (i, cap) in cap_names.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    print!("{}", cap);
                }
                println!();
            }
            println!();
            println!(
                "  System prompt:   {} chars",
                conversation
                    .system_prompt
                    .as_ref()
                    .map(|s| s.len())
                    .unwrap_or(0)
            );
            if context_manager.has_file_tree().await {
                println!("  File tree:       ✓ loaded");
            } else {
                println!("  File tree:       ✗ not loaded");
            }
            println!("─────────────────────────────────────\n");
            continue;
        }

        // Check for /settings command (launch TUI settings)
        if trimmed == "/settings" || trimmed == "/config" {
            // Launch TUI settings interface
            let current_settings = Settings::load()?;
            match ted::tui::run_tui_interactive(current_settings) {
                Ok((new_settings, was_modified)) => {
                    if was_modified {
                        // Apply changes to the current session
                        let mut stdout = io::stdout();
                        stdout.execute(SetForegroundColor(Color::Green))?;
                        println!("\n✓ Settings updated");
                        stdout.execute(ResetColor)?;
                        let mut spawn_agent_needs_refresh = false;

                        // Check if model changed (use correct provider's default model)
                        let new_model = ProviderFactory::default_model(
                            &new_settings.defaults.provider,
                            &new_settings,
                        );
                        if new_model != model {
                            model = new_model;
                            println!("  Model: {}", model);
                            spawn_agent_needs_refresh = true;
                        }

                        // Check if provider changed - need to recreate the provider object
                        if new_settings.defaults.provider != settings.defaults.provider {
                            provider_name = new_settings.defaults.provider.clone();
                            settings.defaults.provider = provider_name.clone();

                            // Recreate the provider for the new backend
                            match ProviderFactory::create(&provider_name, &new_settings, false)
                                .await
                            {
                                Ok(new_provider) => {
                                    provider = new_provider;
                                    spawn_agent_needs_refresh = true;
                                }
                                Err(e) => {
                                    eprintln!(
                                        "Warning: Failed to create provider '{}': {}",
                                        provider_name, e
                                    );
                                }
                            }
                            println!("  Provider: {}", settings.defaults.provider);
                        }

                        // Keep spawn_agent aligned with current provider/model.
                        if spawn_agent_needs_refresh {
                            if let Some(ref coordinator) = rate_coordinator {
                                tool_executor
                                    .registry_mut()
                                    .register_spawn_agent_with_coordinator(
                                        provider.clone(),
                                        skill_registry.clone(),
                                        Arc::clone(coordinator),
                                        model.to_string(),
                                    );
                            } else {
                                tool_executor.registry_mut().register_spawn_agent(
                                    provider.clone(),
                                    skill_registry.clone(),
                                    model.to_string(),
                                );
                            }
                        }

                        // Check if default caps changed
                        let new_caps = new_settings.defaults.caps.clone();
                        if new_caps != cap_names {
                            cap_names = new_caps;
                            // Re-resolve caps and update system prompt
                            loader = CapLoader::new();
                            merged_cap = resolver.resolve_and_merge(&cap_names)?;
                            if merged_cap.system_prompt.is_empty() {
                                conversation.system_prompt = None;
                            } else {
                                conversation.set_system(&merged_cap.system_prompt);
                            }
                            session_info.caps = cap_names.clone();

                            print!("  Caps: ");
                            if cap_names.is_empty() {
                                println!("(none)");
                            } else {
                                for (i, cap) in cap_names.iter().enumerate() {
                                    if i > 0 {
                                        print!(" ");
                                    }
                                    print_cap_badge(cap)?;
                                }
                                println!();
                            }
                        }

                        println!();
                    } else {
                        println!("\nNo changes made.\n");
                    }
                }
                Err(e) => {
                    eprintln!("\nError launching settings: {}\n", e);
                }
            }
            continue;
        }

        // Check for session command (list/show sessions)
        if trimmed == "/sessions" || trimmed == "/session" {
            let recent = history_store.list_recent(10);
            println!("\nRecent sessions:\n");
            for (i, session) in recent.iter().enumerate() {
                let id_short = &session.id.to_string()[..8];
                let date = session.last_active.format("%Y-%m-%d %H:%M");
                let current_marker = if session.id == session_id.0 {
                    " ← current"
                } else {
                    ""
                };
                let summary = session.summary.as_deref().unwrap_or("(no summary)");
                let truncated = if summary.len() > 40 {
                    format!("{}...", &summary[..37])
                } else {
                    summary.to_string()
                };
                println!(
                    "  [{}] {} | {} | {} msgs | {}{}",
                    i + 1,
                    id_short,
                    date,
                    session.message_count,
                    truncated,
                    current_marker
                );
            }
            println!("\nTo switch: /switch <number> or /switch <session-id>");
            println!("To start fresh: /new\n");
            continue;
        }

        // Check for /switch command (switch to a different session)
        if trimmed.starts_with("/switch ") {
            // Safe: we already verified the string starts with "/switch "
            let arg = trimmed
                .strip_prefix("/switch ")
                .expect("input starts with '/switch '")
                .trim();

            // Try to parse as a number first (for quick selection)
            let target_session: Option<SessionInfo> = if let Ok(num) = arg.parse::<usize>() {
                let recent = history_store.list_recent(10);
                if num >= 1 && num <= recent.len() {
                    Some(recent[num - 1].clone())
                } else {
                    println!(
                        "\nInvalid session number. Run /sessions to see available sessions.\n"
                    );
                    continue;
                }
            } else {
                // Try to find by ID prefix
                let sessions = history_store.list_recent(100);
                sessions
                    .into_iter()
                    .find(|s| s.id.to_string().starts_with(arg))
                    .cloned()
            };

            if let Some(new_session) = target_session {
                if new_session.id == session_id.0 {
                    println!("\nAlready in that session.\n");
                    continue;
                }

                // Switch to the new session
                let mut stdout = io::stdout();
                stdout.execute(SetForegroundColor(Color::Cyan))?;
                println!(
                    "\nSwitching to session: {}",
                    &new_session.id.to_string()[..8]
                );
                stdout.execute(ResetColor)?;

                if let Some(ref summary) = new_session.summary {
                    println!("  {}", summary);
                }
                println!(
                    "  {} messages from {}",
                    new_session.message_count,
                    new_session.last_active.format("%Y-%m-%d %H:%M")
                );

                // Update session state
                session_id = SessionId(new_session.id);
                session_info = new_session.clone();
                message_count = session_info.message_count;

                // Clear and reset conversation
                conversation.clear();
                if !merged_cap.system_prompt.is_empty() {
                    conversation.set_system(&merged_cap.system_prompt);
                }

                // Restore caps from the session if available
                if !session_info.caps.is_empty() {
                    cap_names = session_info.caps.clone();
                    merged_cap = resolver.resolve_and_merge(&cap_names)?;
                    if !merged_cap.system_prompt.is_empty() {
                        conversation.set_system(&merged_cap.system_prompt);
                    }
                }

                // Show active caps
                if !cap_names.is_empty() {
                    print!("  Caps: ");
                    for (i, cap) in cap_names.iter().enumerate() {
                        if i > 0 {
                            print!(" ");
                        }
                        print_cap_badge(cap)?;
                    }
                    println!();
                }

                println!(
                    "\nSession switched. Note: Previous messages are not loaded into context."
                );
                println!("Use /stats to see session info.\n");
            } else {
                println!("\nSession not found. Run /sessions to see available sessions.\n");
            }
            continue;
        }

        // Check for /new command (start a new session)
        if trimmed == "/new" {
            // Create a new session
            let new_session_id = SessionId::new();
            let new_session_info = SessionInfo::new(new_session_id.0, working_directory.clone());

            let mut stdout = io::stdout();
            stdout.execute(SetForegroundColor(Color::Green))?;
            println!(
                "\n✓ Started new session: {}",
                &new_session_id.0.to_string()[..8]
            );
            stdout.execute(ResetColor)?;

            // Update state
            session_id = new_session_id;
            session_info = new_session_info;
            session_info.caps = cap_names.clone();
            message_count = 0;

            // Clear conversation but keep caps
            conversation.clear();
            if !merged_cap.system_prompt.is_empty() {
                conversation.set_system(&merged_cap.system_prompt);
            }

            println!("Context cleared. Ready for a fresh conversation.\n");
            continue;
        }

        // Check for /plans command (open plans TUI or list plans)
        if trimmed == "/plans" || trimmed == "/plan" {
            // Launch TUI plans browser
            let current_settings = Settings::load()?;
            match ted::tui::run_tui_plans(current_settings) {
                Ok(()) => {
                    println!();
                }
                Err(e) => {
                    eprintln!("\nError launching plans browser: {}\n", e);
                }
            }
            continue;
        }

        // Check for /plans list command
        if trimmed == "/plans list" || trimmed == "/plan list" {
            match PlanStore::open() {
                Ok(store) => {
                    let plans = store.list();
                    if plans.is_empty() {
                        println!("\nNo plans found. Ted creates plans automatically when working on complex tasks.\n");
                    } else {
                        println!("\nPlans:\n");
                        for (i, plan) in plans.iter().enumerate() {
                            let id_short = &plan.id.to_string()[..8];
                            let status_badge = plan.status.label();
                            let progress = if plan.task_count > 0 {
                                format!("{}/{}", plan.completed_count, plan.task_count)
                            } else {
                                "0/0".to_string()
                            };
                            println!(
                                "  [{}] {} | [{}] {} ({})",
                                i + 1,
                                id_short,
                                status_badge,
                                plan.title,
                                progress
                            );
                        }
                        println!("\nRun /plans to browse in TUI.\n");
                    }
                }
                Err(e) => {
                    eprintln!("\nError loading plans: {}\n", e);
                }
            }
            continue;
        }

        // Check for /model command (show or switch model)
        if trimmed == "/model" || trimmed == "/models" {
            println!("\nCurrent model: {}", model);
            println!("\nAvailable models:");
            println!("  claude-sonnet-4-20250514    - Best quality, moderate rate limits");
            println!("  claude-3-5-sonnet-20241022  - Previous Sonnet, good balance");
            println!("  claude-3-5-haiku-20241022   - Fastest, highest rate limits, cheapest");
            println!("\nTo switch: /model <name> or use -m flag when starting ted");
            println!("Example: /model claude-3-5-haiku-20241022\n");
            continue;
        }

        if trimmed.starts_with("/model ") {
            // Safe: we already verified the string starts with "/model "
            let new_model = trimmed
                .strip_prefix("/model ")
                .expect("input starts with '/model '")
                .trim();
            let valid_models = [
                "claude-sonnet-4-20250514",
                "claude-3-5-sonnet-20241022",
                "claude-3-5-haiku-20241022",
            ];
            if valid_models.contains(&new_model) {
                model = new_model.to_string();
                if let Some(ref coordinator) = rate_coordinator {
                    tool_executor
                        .registry_mut()
                        .register_spawn_agent_with_coordinator(
                            provider.clone(),
                            skill_registry.clone(),
                            Arc::clone(coordinator),
                            model.to_string(),
                        );
                } else {
                    tool_executor.registry_mut().register_spawn_agent(
                        provider.clone(),
                        skill_registry.clone(),
                        model.to_string(),
                    );
                }
                let mut stdout = io::stdout();
                stdout.execute(SetForegroundColor(Color::Green))?;
                println!("\n✓ Switched to model: {}", model);
                stdout.execute(ResetColor)?;

                // Show a tip about rate limits
                if new_model.contains("haiku") {
                    println!("  Tip: Haiku has higher rate limits and is faster.");
                }
                println!();
            } else {
                println!(
                    "\nUnknown model '{}'. Run /model to see available models.\n",
                    new_model
                );
            }
            continue;
        }

        // Check for caps command (show active caps)
        if trimmed == "/caps" {
            println!();
            print!("Active caps: ");
            if cap_names.is_empty() {
                println!("(none)");
            } else {
                for (i, cap) in cap_names.iter().enumerate() {
                    if i > 0 {
                        print!(" ");
                    }
                    print_cap_badge(cap)?;
                }
                println!();
            }
            println!("\nCommands:");
            println!("  /cap add <name>     - Add a cap");
            println!("  /cap remove <name>  - Remove a cap");
            println!("  /cap set <names>    - Replace all caps (comma-separated)");
            println!("  /cap clear          - Remove all caps");
            println!("\nRun 'ted caps list' to see available caps.\n");
            continue;
        }

        // Check for cap modification commands
        if trimmed.starts_with("/cap ") {
            let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
            if parts.len() < 2 {
                println!("\nUsage: /cap <add|remove|set|clear> [name]\n");
                continue;
            }

            let action = parts[1];
            let arg = parts.get(2).copied();

            match action {
                "add" => {
                    if let Some(name) = arg {
                        // Verify cap exists
                        if loader.load(name).is_err() {
                            println!("\nCap '{}' not found. Run 'ted caps list' to see available caps.\n", name);
                            continue;
                        }
                        if cap_names.contains(&name.to_string()) {
                            println!("\nCap '{}' is already active.\n", name);
                            continue;
                        }
                        cap_names.push(name.to_string());

                        // Re-resolve and update system prompt
                        merged_cap = resolver.resolve_and_merge(&cap_names)?;
                        conversation.set_system(&merged_cap.system_prompt);
                        session_info.caps = cap_names.clone();

                        print!("\nAdded cap: ");
                        print_cap_badge(name)?;
                        println!("\n");
                    } else {
                        println!("\nUsage: /cap add <name>\n");
                    }
                }
                "remove" => {
                    if let Some(name) = arg {
                        if let Some(pos) = cap_names.iter().position(|c| c == name) {
                            cap_names.remove(pos);

                            // Re-resolve and update system prompt
                            merged_cap = resolver.resolve_and_merge(&cap_names)?;
                            if merged_cap.system_prompt.is_empty() {
                                conversation.system_prompt = None;
                            } else {
                                conversation.set_system(&merged_cap.system_prompt);
                            }
                            session_info.caps = cap_names.clone();

                            println!("\nRemoved cap: {}\n", name);
                        } else {
                            println!("\nCap '{}' is not active.\n", name);
                        }
                    } else {
                        println!("\nUsage: /cap remove <name>\n");
                    }
                }
                "set" => {
                    if let Some(names) = arg {
                        let new_caps: Vec<String> = names
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();

                        // Verify all caps exist
                        for name in &new_caps {
                            if loader.load(name).is_err() {
                                println!("\nCap '{}' not found. Run 'ted caps list' to see available caps.\n", name);
                                continue;
                            }
                        }

                        cap_names = new_caps;
                        merged_cap = resolver.resolve_and_merge(&cap_names)?;
                        if merged_cap.system_prompt.is_empty() {
                            conversation.system_prompt = None;
                        } else {
                            conversation.set_system(&merged_cap.system_prompt);
                        }
                        session_info.caps = cap_names.clone();

                        print!("\nActive caps: ");
                        if cap_names.is_empty() {
                            println!("(none)");
                        } else {
                            for (i, cap) in cap_names.iter().enumerate() {
                                if i > 0 {
                                    print!(" ");
                                }
                                print_cap_badge(cap)?;
                            }
                            println!();
                        }
                        println!();
                    } else {
                        println!("\nUsage: /cap set <name1,name2,...>\n");
                    }
                }
                "clear" => {
                    cap_names.clear();
                    merged_cap = resolver.resolve_and_merge(&cap_names)?;
                    conversation.system_prompt = None;
                    session_info.caps = cap_names.clone();
                    println!("\nAll caps removed.\n");
                }
                "create" => {
                    if let Some(name) = arg {
                        let caps_dir = Settings::caps_dir();
                        std::fs::create_dir_all(&caps_dir)?;
                        let cap_path = caps_dir.join(format!("{}.toml", name));

                        if cap_path.exists() {
                            println!(
                                "\nCap '{}' already exists at {}\n",
                                name,
                                cap_path.display()
                            );
                            continue;
                        }

                        // Interactive prompts
                        println!("\nCreating cap: {}\n", name);

                        // Get description
                        print!("Description (press Enter for default): ");
                        io::stdout().flush()?;
                        let mut description = String::new();
                        io::stdin().read_line(&mut description)?;
                        let description = description.trim();
                        let description = if description.is_empty() {
                            format!("Custom {} persona", name)
                        } else {
                            description.to_string()
                        };

                        // Get system prompt
                        println!("\nSystem prompt (enter a blank line to finish, or press Enter for default):");
                        println!("---");
                        let mut system_prompt_lines: Vec<String> = Vec::new();
                        loop {
                            let mut line = String::new();
                            io::stdin().read_line(&mut line)?;
                            let line = line.trim_end_matches('\n').trim_end_matches('\r');
                            if line.is_empty() && system_prompt_lines.is_empty() {
                                break;
                            }
                            if line.is_empty() && !system_prompt_lines.is_empty() {
                                break;
                            }
                            system_prompt_lines.push(line.to_string());
                        }
                        println!("---");

                        let system_prompt = if system_prompt_lines.is_empty() {
                            format!("You are an AI assistant with the {} persona.\n\nFollow best practices and be helpful.", name)
                        } else {
                            system_prompt_lines.join("\n")
                        };

                        // Create the cap file
                        let template = format!(
                            r#"# Cap definition for {name}
name = "{name}"
description = "{description}"
version = "1.0.0"
priority = 10

# Inherit from other caps (optional)
extends = ["base"]

# System prompt - must be defined BEFORE any [tables]
system_prompt = """
{system_prompt}
"""

# Tool permissions (optional)
[tool_permissions]
enable = []
disable = []
require_edit_confirmation = true
require_shell_confirmation = true
auto_approve_paths = []
blocked_commands = []
"#
                        );

                        std::fs::write(&cap_path, template)?;

                        let mut stdout = io::stdout();
                        stdout.execute(SetForegroundColor(Color::Green))?;
                        println!("\n✓ Created cap: {}", name);
                        stdout.execute(ResetColor)?;
                        println!("  Path: {}", cap_path.display());
                        println!("\nTo activate: /cap add {}", name);
                        println!("To edit:     ted caps edit {}\n", name);

                        // Reload the loader so the new cap is available
                        loader = CapLoader::new();
                    } else {
                        println!("\nUsage: /cap create <name>\n");
                    }
                }
                "list" => {
                    println!("\nAvailable caps:");
                    let all_caps = loader.list_available()?;
                    for (name, is_builtin) in &all_caps {
                        let active = if cap_names.contains(name) {
                            " (active)"
                        } else {
                            ""
                        };
                        let builtin_tag = if *is_builtin { " [builtin]" } else { "" };
                        // Try to load the cap to get description
                        let desc = loader
                            .load(name)
                            .map(|c| c.description.clone())
                            .unwrap_or_default();
                        println!("  {} - {}{}{}", name, desc, builtin_tag, active);
                    }
                    println!();
                }
                _ => {
                    println!(
                        "\nUnknown action '{}'. Use: add, remove, set, clear, create, list\n",
                        action
                    );
                }
            }
            continue;
        }

        // Skip empty input
        if input.trim().is_empty() {
            continue;
        }

        // Store user message in context
        context_manager.store_message("user", &input, None).await?;

        // Add user message
        conversation.push(Message::user(&input));

        // Track message and update history
        chat::record_message_and_persist(
            &mut history_store,
            &mut session_info,
            &mut message_count,
            Some(&input),
        )?;

        // Set up interrupt flag for Ctrl+C handling
        let interrupted = Arc::new(AtomicBool::new(false));
        let interrupted_clone = interrupted.clone();

        // Track conversation length before agent loop - needed for Ctrl+C cleanup
        // since select! cancels the future and run_agent_loop's cleanup won't run
        let conversation_len_before_agent = conversation.messages.len();

        // Run the agent loop with Ctrl+C handling
        let agent_future = run_agent_loop(
            provider.as_ref(),
            &model,
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            !args.no_stream && settings.defaults.stream,
            &cap_names,
            interrupted.clone(),
        );

        // Use tokio::select! to handle Ctrl+C during agent execution
        let result = tokio::select! {
            result = agent_future => result,
            _ = tokio::signal::ctrl_c() => {
                // Set the interrupted flag so any ongoing operations can check it
                interrupted_clone.store(true, Ordering::SeqCst);

                // Restore conversation to pre-agent-loop state
                // This is critical because the future was cancelled and run_agent_loop's
                // cleanup code won't run. Without this, the conversation could be left
                // with incomplete tool_use/tool_result pairs.
                conversation.messages.truncate(conversation_len_before_agent);

                // Print interruption message
                let mut stdout = io::stdout();
                println!();
                stdout.execute(SetForegroundColor(Color::Yellow))?;
                println!("\n⚡ Interrupted");
                stdout.execute(ResetColor)?;
                println!("Type your next message or use /help for commands.\n");

                // Return a special result indicating interruption
                Ok(false)
            }
        };

        match result {
            Ok(true) => {
                // Completed normally - update message count for assistant response
                chat::record_message_and_persist(
                    &mut history_store,
                    &mut session_info,
                    &mut message_count,
                    None,
                )?;
                chat::trim_conversation_if_needed(provider.as_ref(), &model, &mut conversation);
            }
            Ok(false) => {
                // Interrupted - remove the pending user message since we didn't complete
                // But keep the context, user can continue
            }
            Err(e) => {
                eprintln!("\n{}", utils::format_error(&e));
                // Note: conversation is already restored to pre-call state by run_agent_loop
                // which handles the complex case of multi-turn tool use where multiple
                // messages (assistant with tool_use, user with tool_result) may have been added
            }
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "main/tests.rs"]
mod tests;
