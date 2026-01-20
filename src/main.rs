// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Ted - AI coding assistant for your terminal
//!
//! Entry point for the Ted CLI application.

#![allow(unused_assignments)]

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use crossterm::{
    style::{Color, ResetColor, SetForegroundColor},
    ExecutableCommand,
};
use futures::StreamExt;

use ted::caps::{CapLoader, CapResolver};
use ted::cli::{ChatArgs, Cli, Commands, UpdateArgs};
use ted::commands;
use ted::config::Settings;
use ted::context::{ContextManager, SessionId};
use ted::error::{ApiError, Result, TedError};
use ted::history::{HistoryStore, SessionInfo};
use ted::llm::message::{ContentBlock, Conversation, Message, MessageContent};
use ted::llm::provider::{
    CompletionRequest, ContentBlockDelta, ContentBlockResponse, LlmProvider, StopReason,
    StreamEvent,
};
use ted::llm::providers::{AnthropicProvider, OllamaProvider, OpenRouterProvider};
use ted::plans::PlanStore;
use ted::tools::{ToolContext, ToolExecutor, ToolResult};
use ted::update;
use ted::utils;

/// Maximum number of retries for rate-limited requests
const MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff (in seconds)
const BASE_RETRY_DELAY: u64 = 2;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Load settings
    let settings = Settings::load()?;

    // Ensure directories exist
    Settings::ensure_directories()?;

    // Dispatch to appropriate command
    match cli.command {
        None => {
            run_chat(ChatArgs::default(), settings).await?;
        }
        Some(Commands::Chat(args)) => {
            run_chat(args, settings).await?;
        }
        Some(Commands::Ask(args)) => {
            run_ask(args, settings).await?;
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

/// Check if provider is configured and prompt user to set up if not
fn check_provider_configuration(settings: &Settings, provider_name: &str) -> Result<Settings> {
    match provider_name {
        "ollama" => {
            // Ollama doesn't require an API key, just needs to be running
            // We'll check connectivity later
            Ok(settings.clone())
        }
        _ => {
            // Anthropic (default) requires an API key
            if settings.get_anthropic_api_key().is_some() {
                return Ok(settings.clone());
            }

            // No API key configured - prompt the user
            let mut stdout = io::stdout();
            println!();
            stdout.execute(SetForegroundColor(Color::Cyan))?;
            print!("Welcome to Ted!");
            stdout.execute(ResetColor)?;
            println!();
            println!();
            println!("No LLM provider is configured. You have two options:");
            println!();
            stdout.execute(SetForegroundColor(Color::Yellow))?;
            print!("  1.");
            stdout.execute(ResetColor)?;
            println!(" Use Anthropic Claude (requires API key)");
            println!("     Get your API key at: https://console.anthropic.com/");
            println!();
            stdout.execute(SetForegroundColor(Color::Yellow))?;
            print!("  2.");
            stdout.execute(ResetColor)?;
            println!(" Use Ollama for local models (free, runs on your machine)");
            println!("     Install from: https://ollama.ai");
            println!();
            print!("Choose an option [1/2], or 's' for settings: ");
            stdout.flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let choice = input.trim().to_lowercase();

            let mut updated_settings = settings.clone();

            match choice.as_str() {
                "2" | "ollama" => {
                    // Switch to Ollama
                    updated_settings.defaults.provider = "ollama".to_string();
                    updated_settings.save()?;
                    println!();
                    stdout.execute(SetForegroundColor(Color::Green))?;
                    print!("Provider set to Ollama.");
                    stdout.execute(ResetColor)?;
                    println!();
                    println!("Make sure Ollama is running: ollama serve");
                    println!();
                    Ok(updated_settings)
                }
                "1" | "anthropic" | "" => {
                    // Prompt for API key
                    println!();
                    print!("Enter your Anthropic API key: ");
                    stdout.flush()?;

                    let mut api_key = String::new();
                    io::stdin().read_line(&mut api_key)?;
                    let api_key = api_key.trim().to_string();

                    if api_key.is_empty() {
                        return Err(TedError::Config(
                            "No API key provided. Run 'ted settings' to configure.".to_string(),
                        ));
                    }

                    // Validate key format (basic check)
                    if !api_key.starts_with("sk-") {
                        stdout.execute(SetForegroundColor(Color::Yellow))?;
                        print!("Warning:");
                        stdout.execute(ResetColor)?;
                        println!(" API key doesn't start with 'sk-'. Saving anyway.");
                    }

                    updated_settings.providers.anthropic.api_key = Some(api_key);
                    updated_settings.save()?;
                    println!();
                    stdout.execute(SetForegroundColor(Color::Green))?;
                    print!("API key saved.");
                    stdout.execute(ResetColor)?;
                    println!(" Starting Ted...");
                    println!();
                    Ok(updated_settings)
                }
                "s" | "settings" => {
                    // Launch settings TUI
                    drop(input); // Release stdin
                    run_settings_tui()?;
                    // Reload settings after TUI
                    let reloaded = Settings::load()?;
                    // Check again if provider is now configured
                    if reloaded.defaults.provider == "ollama"
                        || reloaded.get_anthropic_api_key().is_some()
                    {
                        Ok(reloaded)
                    } else {
                        Err(TedError::Config(
                            "No provider configured. Run 'ted' again after setting up.".to_string(),
                        ))
                    }
                }
                _ => Err(TedError::Config(
                    "Invalid choice. Run 'ted' again to configure.".to_string(),
                )),
            }
        }
    }
}

/// Run interactive chat mode
async fn run_chat(args: ChatArgs, mut settings: Settings) -> Result<()> {
    // Check for embedded mode (JSONL output for GUI integration)
    if args.embedded {
        return ted::embedded_runner::run_embedded_chat(args, settings).await;
    }

    // Determine which provider to use
    let provider_name = args
        .provider
        .clone()
        .unwrap_or_else(|| settings.defaults.provider.clone());

    // Check provider configuration and prompt if needed
    settings = check_provider_configuration(&settings, &provider_name)?;

    // Re-determine provider in case it changed during setup
    let provider_name = args
        .provider
        .clone()
        .unwrap_or_else(|| settings.defaults.provider.clone());

    // Create the appropriate provider
    let provider: Box<dyn LlmProvider> = match provider_name.as_str() {
        "ollama" => {
            let ollama_provider =
                OllamaProvider::with_base_url(&settings.providers.ollama.base_url);
            // Perform health check
            if let Err(e) = ollama_provider.health_check().await {
                let mut stdout = io::stdout();
                println!();
                stdout.execute(SetForegroundColor(Color::Yellow))?;
                print!("Ollama is not running.");
                stdout.execute(ResetColor)?;
                println!();
                println!("Start Ollama with: ollama serve");
                println!("Or switch to Anthropic: ted settings");
                return Err(e);
            }
            Box::new(ollama_provider)
        }
        "openrouter" => {
            let api_key = settings.get_openrouter_api_key().ok_or_else(|| {
                TedError::Config(
                    "No OpenRouter API key found. Set OPENROUTER_API_KEY env var or run 'ted settings'.".to_string(),
                )
            })?;
            let provider = if let Some(ref base_url) = settings.providers.openrouter.base_url {
                OpenRouterProvider::with_base_url(api_key, base_url)
            } else {
                OpenRouterProvider::new(api_key)
            };
            Box::new(provider)
        }
        _ => {
            // Get API key (anthropic is the default)
            let api_key = settings.get_anthropic_api_key().ok_or_else(|| {
                TedError::Config("No Anthropic API key found. Run 'ted' to configure.".to_string())
            })?;
            Box::new(AnthropicProvider::new(api_key))
        }
    };

    // Load and resolve caps
    let mut cap_names: Vec<String> = if args.cap.is_empty() {
        settings.defaults.caps.clone()
    } else {
        args.cap.clone()
    };

    let mut loader = CapLoader::new();
    let resolver = CapResolver::new(loader.clone());
    let mut merged_cap = resolver.resolve_and_merge(&cap_names)?;

    // Determine model (cap preferences can override) - mutable for /model command
    let mut model = args
        .model
        .clone()
        .or_else(|| merged_cap.preferred_model().map(|s| s.to_string()))
        .unwrap_or_else(|| match provider_name.as_str() {
            "ollama" => settings.providers.ollama.default_model.clone(),
            "openrouter" => settings.providers.openrouter.default_model.clone(),
            _ => settings.providers.anthropic.default_model.clone(),
        });

    // Create conversation with system prompt from caps
    let mut conversation = Conversation::new();
    if !merged_cap.system_prompt.is_empty() {
        conversation.set_system(&merged_cap.system_prompt);
    }

    // Get working directory and project root
    let working_directory = std::env::current_dir()?;
    let project_root = utils::find_project_root();

    // Initialize history tracking
    let mut history_store = HistoryStore::open()?;

    // Check for recent sessions in this directory (unless --resume was specified)
    let (mut session_id, mut session_info, mut message_count, is_resumed) =
        if let Some(ref resume_id) = args.resume {
            // Resume a specific session by ID
            resume_session(&history_store, resume_id, &working_directory)?
        } else {
            // Check for recent sessions in current directory
            let recent_sessions = history_store.sessions_for_directory(&working_directory);
            let mut recent: Vec<_> = recent_sessions
                .into_iter()
                .filter(|s| {
                    // Only show sessions from the last 24 hours
                    let age = chrono::Utc::now() - s.last_active;
                    age.num_hours() < 24
                })
                .collect();
            // Sort by most recent first
            recent.sort_by(|a, b| b.last_active.cmp(&a.last_active));

            if !recent.is_empty() {
                // Prompt user to resume or start new
                if let Some(session_to_resume) = prompt_session_choice(&recent)? {
                    // User chose to resume
                    let sid = SessionId(session_to_resume.id);
                    let info = session_to_resume.clone();
                    let count = info.message_count;
                    (sid, info, count, true)
                } else {
                    // User chose to start new
                    let sid = SessionId::new();
                    let info = SessionInfo::new(sid.0, working_directory.clone());
                    (sid, info, 0, false)
                }
            } else {
                // No recent sessions, start fresh
                let sid = SessionId::new();
                let info = SessionInfo::new(sid.0, working_directory.clone());
                (sid, info, 0, false)
            }
        };

    // Initialize context manager
    let context_storage_path = Settings::context_path();
    let mut context_manager = ContextManager::new(context_storage_path, session_id.clone()).await?;

    // Set project root and generate file tree for awareness
    if let Some(ref root) = project_root {
        context_manager.set_project_root(root.clone(), true).await?;
    }

    // Append file tree to system prompt for LLM awareness
    if let Some(file_tree_context) = context_manager.file_tree_context().await {
        let current_system = conversation.system_prompt.clone().unwrap_or_default();
        let enhanced_system = if current_system.is_empty() {
            file_tree_context
        } else {
            format!("{}\n\n{}", current_system, file_tree_context)
        };
        conversation.set_system(&enhanced_system);
    }

    // Start background compaction (every 5 minutes)
    let _compaction_handle = context_manager.start_background_compaction(300);

    // Create tool executor
    let tool_context = ToolContext::new(
        working_directory.clone(),
        project_root.clone(),
        session_id.0,
        args.trust,
    );
    let mut tool_executor = ToolExecutor::new(tool_context, args.trust);

    // Update session info
    session_info.project_root = project_root;
    if !is_resumed {
        session_info.caps = cap_names.clone();
    } else {
        // Use caps from resumed session if available
        if !session_info.caps.is_empty() && args.cap.is_empty() {
            cap_names = session_info.caps.clone();
            merged_cap = resolver.resolve_and_merge(&cap_names)?;
            if !merged_cap.system_prompt.is_empty() {
                conversation.set_system(&merged_cap.system_prompt);
            }
        }
    }

    // Print welcome message with cap info
    print_welcome(
        &provider_name,
        &model,
        args.trust,
        &session_id,
        &merged_cap.source_caps,
    )?;

    // Main chat loop
    loop {
        // Get user input
        let input = read_user_input()?;

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

                        // Check if model changed (use correct provider)
                        let new_model = if new_settings.defaults.provider == "ollama" {
                            new_settings.providers.ollama.default_model.clone()
                        } else {
                            new_settings.providers.anthropic.default_model.clone()
                        };
                        if new_model != model {
                            model = new_model;
                            println!("  Model: {}", model);
                        }

                        // Check if provider changed
                        if new_settings.defaults.provider != settings.defaults.provider {
                            settings.defaults.provider = new_settings.defaults.provider.clone();
                            println!("  Provider: {}", settings.defaults.provider);
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
            let arg = trimmed.strip_prefix("/switch ").unwrap().trim();

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
            let new_model = trimmed.strip_prefix("/model ").unwrap().trim();
            let valid_models = [
                "claude-sonnet-4-20250514",
                "claude-3-5-sonnet-20241022",
                "claude-3-5-haiku-20241022",
            ];
            if valid_models.contains(&new_model) {
                model = new_model.to_string();
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
        message_count += 1;
        if message_count == 1 {
            // Set summary from first user message
            session_info.set_summary(&input);
        }
        session_info.message_count = message_count;
        session_info.touch();
        history_store.upsert(session_info.clone())?;

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
                message_count += 1;
                session_info.message_count = message_count;
                session_info.touch();
                history_store.upsert(session_info.clone())?;

                // Proactively trim conversation to prevent future delays
                // Get model context window
                let context_window = provider
                    .get_model_info(&model)
                    .map(|m| m.context_window)
                    .unwrap_or(200_000);

                // Check if we're approaching the limit (80% threshold)
                if conversation.needs_trimming(context_window) {
                    let removed = conversation.trim_to_fit(context_window);
                    if removed > 0 {
                        // Silent trimming - user doesn't need to know unless debugging
                        tracing::debug!(
                            "Proactively trimmed {} old messages from conversation",
                            removed
                        );
                    }
                }
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

/// Run the agent loop - handles streaming, tool use, and multi-turn interactions
/// Returns Ok(true) if completed normally, Ok(false) if interrupted by Ctrl+C
/// On error or interruption, automatically restores conversation to its initial state.
#[allow(clippy::too_many_arguments)]
async fn run_agent_loop(
    provider: &dyn LlmProvider,
    model: &str,
    conversation: &mut Conversation,
    tool_executor: &mut ToolExecutor,
    settings: &Settings,
    context_manager: &ContextManager,
    stream: bool,
    active_caps: &[String],
    interrupted: Arc<AtomicBool>,
) -> Result<bool> {
    // Track the conversation length at the start so we can restore on error/interrupt
    let initial_message_count = conversation.messages.len();

    let result = run_agent_loop_inner(
        provider,
        model,
        conversation,
        tool_executor,
        settings,
        context_manager,
        stream,
        active_caps,
        interrupted,
    )
    .await;

    // On error OR interruption, restore conversation to its initial state
    // This is critical for multi-turn tool use where multiple messages
    // (assistant with tool_use, user with tool_result) may have been added
    // before the failure/interruption occurred
    match &result {
        Err(_) | Ok(false) => {
            conversation.messages.truncate(initial_message_count);
        }
        Ok(true) => {}
    }

    result
}

/// Inner implementation of the agent loop
#[allow(clippy::too_many_arguments)]
async fn run_agent_loop_inner(
    provider: &dyn LlmProvider,
    model: &str,
    conversation: &mut Conversation,
    tool_executor: &mut ToolExecutor,
    settings: &Settings,
    context_manager: &ContextManager,
    stream: bool,
    active_caps: &[String],
    interrupted: Arc<AtomicBool>,
) -> Result<bool> {
    // Track recent tool calls for loop detection
    // Format: (tool_name, serialized_input)
    let mut recent_tool_calls: Vec<(String, String)> = Vec::new();
    const MAX_CONSECUTIVE_IDENTICAL_CALLS: usize = 2;

    loop {
        // Check if we were interrupted before starting a new iteration
        if interrupted.load(Ordering::SeqCst) {
            return Ok(false);
        }

        // Conversation should already be trimmed proactively after each exchange
        // Just send all messages directly - proactive trimming handles the size
        let mut request = CompletionRequest::new(model, conversation.messages.clone())
            .with_max_tokens(settings.defaults.max_tokens)
            .with_temperature(settings.defaults.temperature)
            .with_tools(tool_executor.tool_definitions());

        // Add system prompt if present
        if let Some(ref system_prompt) = conversation.system_prompt {
            request = request.with_system(system_prompt);
        }

        // Get response with retry logic for rate limits
        let (response_content, stop_reason) =
            get_response_with_retry(provider, request, stream, active_caps).await?;

        // Extract text content for context storage
        let text_content: String = response_content
            .iter()
            .filter_map(|block| {
                if let ContentBlockResponse::Text { text } = block {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Store assistant message in context
        if !text_content.is_empty() {
            context_manager
                .store_message("assistant", &text_content, None)
                .await?;
        }

        // Check if there are any tool uses
        let tool_uses: Vec<_> = response_content
            .iter()
            .filter_map(|block| {
                if let ContentBlockResponse::ToolUse { id, name, input } = block {
                    Some((id.clone(), name.clone(), input.clone()))
                } else {
                    None
                }
            })
            .collect();

        // Add assistant message to conversation
        let assistant_blocks: Vec<ContentBlock> = response_content
            .iter()
            .map(|block| match block {
                ContentBlockResponse::Text { text } => ContentBlock::Text { text: text.clone() },
                ContentBlockResponse::ToolUse { id, name, input } => ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                },
            })
            .collect();

        conversation.push(Message {
            id: uuid::Uuid::new_v4(),
            role: ted::llm::message::Role::Assistant,
            content: MessageContent::Blocks(assistant_blocks),
            timestamp: chrono::Utc::now(),
            tool_use_id: None,
            token_count: None,
        });

        // If there are tool uses, execute them and continue the loop
        if !tool_uses.is_empty() {
            println!(); // Newline before tool output
            let mut tool_results: Vec<ToolResult> = Vec::new();
            let mut loop_detected = false;

            for (id, name, input) in &tool_uses {
                // Serialize input for comparison
                let input_str = serde_json::to_string(input).unwrap_or_default();
                let current_call = (name.clone(), input_str.clone());

                // Check for loop: count how many recent calls match this one
                let consecutive_matches = recent_tool_calls
                    .iter()
                    .rev()
                    .take_while(|call| *call == &current_call)
                    .count();

                if consecutive_matches >= MAX_CONSECUTIVE_IDENTICAL_CALLS {
                    // Loop detected! Inject an error message instead of executing
                    loop_detected = true;
                    println!(
                        "  ⚠️  Loop detected: '{}' called {} times with same arguments. Breaking loop.",
                        name,
                        consecutive_matches + 1
                    );

                    let error_result = ToolResult::error(
                        id.clone(),
                        format!(
                            "LOOP DETECTED: You have called '{}' {} times in a row with the same arguments. \
                            This appears to be a loop. Please try a DIFFERENT approach or tool. \
                            If you were searching, try reading a specific file instead. \
                            If you need more information, try asking the user for clarification.",
                            name,
                            consecutive_matches + 1
                        ),
                    );
                    tool_results.push(error_result);
                    recent_tool_calls.clear(); // Reset to give model a fresh start
                    continue;
                }

                // Track this call
                recent_tool_calls.push(current_call);
                // Keep only the last 10 calls
                if recent_tool_calls.len() > 10 {
                    recent_tool_calls.remove(0);
                }

                // Display tool invocation
                print_tool_invocation(name, input)?;

                let result = tool_executor
                    .execute_tool_use(id, name, input.clone())
                    .await?;

                // Display tool result
                print_tool_result(name, &result)?;

                // Small delay between tool executions to avoid rate limiting
                // when the model makes many consecutive tool calls
                tokio::time::sleep(Duration::from_millis(100)).await;

                // Store tool call in context
                context_manager
                    .store_tool_call(name, input, result.output_text(), result.is_error(), None)
                    .await?;

                tool_results.push(result);
            }

            // Add tool results to conversation
            let result_blocks: Vec<ContentBlock> = tool_results
                .into_iter()
                .map(|r| {
                    let is_error = r.is_error();
                    let output = r.output_text().to_string();
                    ContentBlock::ToolResult {
                        tool_use_id: r.tool_use_id,
                        content: ted::llm::message::ToolResultContent::Text(output),
                        is_error: if is_error { Some(true) } else { None },
                    }
                })
                .collect();

            conversation.push(Message {
                id: uuid::Uuid::new_v4(),
                role: ted::llm::message::Role::User,
                content: MessageContent::Blocks(result_blocks),
                timestamp: chrono::Utc::now(),
                tool_use_id: None,
                token_count: None,
            });

            // If we detected a loop, give the model one more chance to recover
            // If it loops again, the next iteration will catch it
            if loop_detected {
                println!("\n  Giving model a chance to try a different approach...\n");
            }

            // Add a small delay before the next API call to help with rate limiting
            // This is especially important for multi-tool responses
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Continue the loop to get the next response
            continue;
        }

        // No tool uses, we're done
        if stop_reason != Some(StopReason::ToolUse) {
            println!(); // Final newline
            break;
        }
    }

    Ok(true)
}

/// Get response from LLM with retry logic for rate limits
async fn get_response_with_retry(
    provider: &dyn LlmProvider,
    request: CompletionRequest,
    stream: bool,
    active_caps: &[String],
) -> Result<(Vec<ContentBlockResponse>, Option<StopReason>)> {
    let mut attempt = 0;
    let mut _last_error: Option<TedError> = None;

    loop {
        attempt += 1;

        let result = if stream {
            stream_response(provider, request.clone(), active_caps).await
        } else {
            // For non-streaming, print the prefix with caps
            print_response_prefix(active_caps)?;
            provider
                .complete(request.clone())
                .await
                .map(|r| (r.content, r.stop_reason))
        };

        match result {
            Ok(response) => return Ok(response),
            Err(TedError::Api(ApiError::RateLimited(retry_after))) => {
                if attempt > MAX_RETRIES {
                    return Err(TedError::Api(ApiError::RateLimited(retry_after)));
                }

                // Calculate delay: use server's retry_after or exponential backoff
                let delay_secs = if retry_after > 0 {
                    retry_after as u64
                } else {
                    BASE_RETRY_DELAY.pow(attempt)
                };

                let mut stdout = io::stdout();
                stdout.execute(SetForegroundColor(Color::Yellow))?;
                println!(
                    "\n⏳ Rate limited. Retrying in {} seconds... (attempt {}/{})",
                    delay_secs, attempt, MAX_RETRIES
                );
                stdout.execute(ResetColor)?;

                tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                _last_error = Some(TedError::Api(ApiError::RateLimited(retry_after)));
            }
            Err(e) => {
                // For non-rate-limit errors, don't retry
                return Err(e);
            }
        }
    }
}

/// Stream the response from the LLM and return content blocks
async fn stream_response(
    provider: &dyn LlmProvider,
    request: CompletionRequest,
    active_caps: &[String],
) -> Result<(Vec<ContentBlockResponse>, Option<StopReason>)> {
    let mut stdout = io::stdout();

    let mut stream = provider.complete_stream(request).await?;
    let mut content_blocks: Vec<ContentBlockResponse> = Vec::new();
    let mut current_text = String::new();
    let mut current_tool_input = String::new();
    let mut stop_reason = None;
    let mut prefix_printed = false;

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::ContentBlockStart { content_block, .. } => {
                match &content_block {
                    ContentBlockResponse::Text { .. } => {
                        current_text.clear();
                    }
                    ContentBlockResponse::ToolUse { .. } => {
                        current_tool_input.clear();
                    }
                }
                content_blocks.push(content_block);
            }
            StreamEvent::ContentBlockDelta { index, delta } => {
                match delta {
                    ContentBlockDelta::TextDelta { text } => {
                        // Only print the prefix when we have actual text content
                        if !prefix_printed {
                            print_response_prefix(active_caps)?;
                            prefix_printed = true;
                        }
                        print!("{}", text);
                        stdout.flush()?;
                        current_text.push_str(&text);

                        // Update the content block
                        if let Some(ContentBlockResponse::Text { text: block_text }) =
                            content_blocks.get_mut(index)
                        {
                            block_text.push_str(&current_text);
                            current_text.clear();
                        }
                    }
                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                        current_tool_input.push_str(&partial_json);
                    }
                }
            }
            StreamEvent::ContentBlockStop { index } => {
                // Finalize the content block
                if let Some(block) = content_blocks.get_mut(index) {
                    match block {
                        ContentBlockResponse::Text { text } => {
                            if !current_text.is_empty() {
                                text.push_str(&current_text);
                                current_text.clear();
                            }
                        }
                        ContentBlockResponse::ToolUse { input, .. } => {
                            if !current_tool_input.is_empty() {
                                if let Ok(parsed) = serde_json::from_str(&current_tool_input) {
                                    *input = parsed;
                                }
                                current_tool_input.clear();
                            }
                        }
                    }
                }
            }
            StreamEvent::MessageDelta {
                stop_reason: sr, ..
            } => {
                stop_reason = sr;
            }
            StreamEvent::MessageStop => {}
            StreamEvent::Error {
                error_type,
                message,
            } => {
                return Err(TedError::Api(ted::error::ApiError::ServerError {
                    status: 0,
                    message: format!("{}: {}", error_type, message),
                }));
            }
            _ => {}
        }
    }

    Ok((content_blocks, stop_reason))
}

/// Run single question mode
async fn run_ask(args: ted::cli::AskArgs, settings: Settings) -> Result<()> {
    // Determine which provider to use
    let provider_name = args
        .provider
        .clone()
        .unwrap_or_else(|| settings.defaults.provider.clone());

    // Create the appropriate provider
    let provider: Box<dyn LlmProvider> = match provider_name.as_str() {
        "ollama" => {
            let ollama_provider =
                OllamaProvider::with_base_url(&settings.providers.ollama.base_url);
            // Perform health check
            ollama_provider.health_check().await?;
            Box::new(ollama_provider)
        }
        "openrouter" => {
            let api_key = settings
                .get_openrouter_api_key()
                .ok_or_else(|| TedError::Config("No OpenRouter API key found.".to_string()))?;
            let provider = if let Some(ref base_url) = settings.providers.openrouter.base_url {
                OpenRouterProvider::with_base_url(api_key, base_url)
            } else {
                OpenRouterProvider::new(api_key)
            };
            Box::new(provider)
        }
        _ => {
            let api_key = settings
                .get_anthropic_api_key()
                .ok_or_else(|| TedError::Config("No Anthropic API key found.".to_string()))?;
            Box::new(AnthropicProvider::new(api_key))
        }
    };

    let model = args.model.unwrap_or_else(|| match provider_name.as_str() {
        "ollama" => settings.providers.ollama.default_model.clone(),
        "openrouter" => settings.providers.openrouter.default_model.clone(),
        _ => settings.providers.anthropic.default_model.clone(),
    });

    // Build prompt with any file contents
    let mut prompt = args.prompt;
    for file_path in &args.file {
        if let Ok(content) = std::fs::read_to_string(file_path) {
            prompt = format!(
                "{}\n\n<file path=\"{}\">\n{}\n</file>",
                prompt,
                file_path.display(),
                content
            );
        }
    }

    let messages = vec![Message::user(&prompt)];
    let request = CompletionRequest::new(&model, messages)
        .with_max_tokens(settings.defaults.max_tokens)
        .with_temperature(settings.defaults.temperature);

    // Stream the response
    let mut stdout = io::stdout();
    let mut stream = provider.complete_stream(request).await?;

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::ContentBlockDelta {
                delta: ContentBlockDelta::TextDelta { text },
                ..
            } => {
                print!("{}", text);
                stdout.flush()?;
            }
            StreamEvent::MessageStop => {
                println!();
            }
            _ => {}
        }
    }

    Ok(())
}

/// Clear context
fn run_clear() -> Result<()> {
    // For now, just print a message. Context clearing will be implemented with the WAL system.
    println!("Context cleared.");
    Ok(())
}

/// Run settings TUI
fn run_settings_tui() -> Result<()> {
    let settings = Settings::load()?;
    ted::tui::run_tui(settings)
}

/// Run settings subcommands
fn run_settings_command(args: ted::cli::SettingsArgs, mut settings: Settings) -> Result<()> {
    match args.command {
        Some(ted::cli::SettingsCommands::Show) => {
            let json = serde_json::to_string_pretty(&settings)?;
            println!("{}", json);
        }
        Some(ted::cli::SettingsCommands::Set { key, value }) => {
            match key.as_str() {
                "model" => {
                    // Set model for the current default provider
                    match settings.defaults.provider.as_str() {
                        "ollama" => settings.providers.ollama.default_model = value,
                        _ => settings.providers.anthropic.default_model = value,
                    }
                }
                "temperature" => {
                    settings.defaults.temperature = value.parse().map_err(|_| {
                        TedError::InvalidInput("Invalid temperature value".to_string())
                    })?;
                }
                "stream" => {
                    settings.defaults.stream = value
                        .parse()
                        .map_err(|_| TedError::InvalidInput("Invalid boolean value".to_string()))?;
                }
                "provider" => {
                    let valid_providers = ["anthropic", "ollama"];
                    if valid_providers.contains(&value.as_str()) {
                        settings.defaults.provider = value;
                    } else {
                        return Err(TedError::InvalidInput(format!(
                            "Invalid provider '{}'. Valid providers: {}",
                            value,
                            valid_providers.join(", ")
                        )));
                    }
                }
                "ollama.base_url" => {
                    settings.providers.ollama.base_url = value;
                }
                "ollama.model" => {
                    settings.providers.ollama.default_model = value;
                }
                _ => {
                    return Err(TedError::InvalidInput(format!("Unknown setting: {}", key)));
                }
            }
            settings.save()?;
            println!("Setting '{}' updated.", key);
        }
        Some(ted::cli::SettingsCommands::Get { key }) => {
            let value = match key.as_str() {
                "model" => match settings.defaults.provider.as_str() {
                    "ollama" => settings.providers.ollama.default_model.clone(),
                    _ => settings.providers.anthropic.default_model.clone(),
                },
                "temperature" => settings.defaults.temperature.to_string(),
                "stream" => settings.defaults.stream.to_string(),
                "provider" => settings.defaults.provider.clone(),
                "ollama.base_url" => settings.providers.ollama.base_url.clone(),
                "ollama.model" => settings.providers.ollama.default_model.clone(),
                _ => {
                    return Err(TedError::InvalidInput(format!("Unknown setting: {}", key)));
                }
            };
            println!("{}", value);
        }
        Some(ted::cli::SettingsCommands::Reset) => {
            let default_settings = Settings::default();
            default_settings.save()?;
            println!("Settings reset to defaults.");
        }
        None => {
            // This case is handled by run_settings_tui
        }
    }
    Ok(())
}

/// Run the update command
async fn run_update_command(args: UpdateArgs) -> Result<()> {
    let mut stdout = io::stdout();

    println!("ted v{}", update::VERSION);

    if args.check {
        // Just check for updates without installing
        println!("Checking for updates...\n");

        match update::check_for_updates().await {
            Ok(Some(release)) => {
                stdout.execute(SetForegroundColor(Color::Green))?;
                println!("New version available: v{}", release.version);
                stdout.execute(ResetColor)?;
                println!("  Released: {}", release.published_at);
                if !release.body.is_empty() {
                    println!("\nRelease notes:");
                    // Show first few lines of release notes
                    for line in release.body.lines().take(10) {
                        println!("  {}", line);
                    }
                }
                println!("\nRun 'ted update' to install the update.");
            }
            Ok(None) => {
                stdout.execute(SetForegroundColor(Color::Green))?;
                println!("You're on the latest version!");
                stdout.execute(ResetColor)?;
            }
            Err(e) => {
                stdout.execute(SetForegroundColor(Color::Red))?;
                eprintln!("Failed to check for updates: {}", e);
                stdout.execute(ResetColor)?;
            }
        }
        return Ok(());
    }

    // Check for specific version or latest
    let release = if let Some(ref version) = args.target_version {
        println!("Checking for version {}...\n", version);
        match update::check_for_version(version).await {
            Ok(Some(r)) => r,
            Ok(None) => {
                eprintln!("Version {} not found", version);
                return Ok(());
            }
            Err(e) => {
                eprintln!("Failed to find version: {}", e);
                return Ok(());
            }
        }
    } else {
        println!("Checking for updates...\n");
        match update::check_for_updates().await {
            Ok(Some(r)) => r,
            Ok(None) => {
                if args.force {
                    // Force reinstall of current version
                    match update::check_for_version(update::VERSION).await {
                        Ok(Some(r)) => r,
                        _ => {
                            eprintln!("No release found for current version");
                            return Ok(());
                        }
                    }
                } else {
                    stdout.execute(SetForegroundColor(Color::Green))?;
                    println!(
                        "You're already on the latest version (v{})!",
                        update::VERSION
                    );
                    stdout.execute(ResetColor)?;
                    println!("\nUse --force to reinstall the current version.");
                    return Ok(());
                }
            }
            Err(e) => {
                eprintln!("Failed to check for updates: {}", e);
                return Ok(());
            }
        }
    };

    stdout.execute(SetForegroundColor(Color::Green))?;
    println!("Installing v{}...", release.version);
    stdout.execute(ResetColor)?;

    match update::install_update(&release).await {
        Ok(()) => {
            stdout.execute(SetForegroundColor(Color::Green))?;
            println!("\n✓ Successfully updated to v{}!", release.version);
            stdout.execute(ResetColor)?;
            println!("\nRestart ted to use the new version.");
        }
        Err(e) => {
            stdout.execute(SetForegroundColor(Color::Red))?;
            eprintln!("\n✗ Update failed: {}", e);
            stdout.execute(ResetColor)?;
        }
    }

    Ok(())
}

/// Initialize ted in current project
fn run_init() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let ted_dir = cwd.join(".ted");

    if ted_dir.exists() {
        println!("Ted is already initialized in this directory.");
        return Ok(());
    }

    // Create .ted directory structure
    std::fs::create_dir_all(ted_dir.join("caps"))?;
    std::fs::create_dir_all(ted_dir.join("commands"))?;

    // Create a default project config
    let config = serde_json::json!({
        "project_name": cwd.file_name().and_then(|n| n.to_str()).unwrap_or("project"),
        "default_caps": ["base"]
    });

    std::fs::write(
        ted_dir.join("config.json"),
        serde_json::to_string_pretty(&config)?,
    )?;

    println!("Initialized ted in {}", ted_dir.display());
    println!("\nCreated:");
    println!("  .ted/");
    println!("  .ted/caps/       - Project-specific caps");
    println!("  .ted/commands/   - Custom commands");
    println!("  .ted/config.json - Project configuration");

    Ok(())
}

/// Print welcome message
fn print_welcome(
    provider: &str,
    model: &str,
    trust_mode: bool,
    session_id: &SessionId,
    caps: &[String],
) -> Result<()> {
    let mut stdout = io::stdout();
    stdout.execute(SetForegroundColor(Color::Cyan))?;
    println!("ted v{}", env!("CARGO_PKG_VERSION"));
    stdout.execute(ResetColor)?;
    println!("AI coding assistant for your terminal");
    println!("Provider: {}", provider);
    println!("Model: {}", model);
    println!("Session: {}", &session_id.as_str()[..8]); // Show first 8 chars of session ID

    // Display caps with color coding
    if !caps.is_empty() {
        print!("Caps: ");
        for (i, cap) in caps.iter().enumerate() {
            if i > 0 {
                print!(" ");
            }
            print_cap_badge(cap)?;
        }
        println!();
    }

    if trust_mode {
        stdout.execute(SetForegroundColor(Color::Yellow))?;
        println!("⚠ Trust mode enabled - all tool actions auto-approved");
        stdout.execute(ResetColor)?;
    }
    println!("Type /help for commands, exit to quit\n");
    Ok(())
}

/// Print a colored cap badge
fn print_cap_badge(cap_name: &str) -> Result<()> {
    let mut stdout = io::stdout();

    // Assign colors based on cap type/name
    let (bg_color, fg_color) = utils::get_cap_colors(cap_name);

    stdout.execute(crossterm::style::SetBackgroundColor(bg_color))?;
    stdout.execute(SetForegroundColor(fg_color))?;
    print!(" {} ", cap_name);
    stdout.execute(ResetColor)?;
    stdout.execute(crossterm::style::SetBackgroundColor(
        crossterm::style::Color::Reset,
    ))?;

    Ok(())
}

/// Print help message
fn print_help() -> Result<()> {
    println!("\nCommands:");
    println!("  /settings  - Open settings TUI (model, caps, context)");
    println!("  /caps      - Show active caps");
    println!("  /cap       - Manage caps (add/remove/set/clear/create/list)");
    println!("  /model     - Show/switch model (use haiku for high rate limits)");
    println!("  /sessions  - List recent sessions");
    println!("  /switch    - Switch to a different session");
    println!("  /new       - Start a new session");
    println!("  /plans     - Browse and manage work plans");
    println!("  /clear     - Clear conversation context");
    println!("  /stats     - Show context/session statistics");
    println!("  /help      - Show this help message");
    println!("  exit       - Exit ted");
    println!("\nTools available:");
    println!("  file_read    - Read file contents");
    println!("  file_write   - Create new files");
    println!("  file_edit    - Edit existing files");
    println!("  shell        - Execute shell commands");
    println!("  glob         - Find files by pattern");
    println!("  grep         - Search file contents");
    println!("  plan_update  - Create/update work plans");
    println!("\nTip: Press Ctrl+C to interrupt a running command without exiting.");
    println!();
    Ok(())
}

/// Read user input
fn read_user_input() -> Result<String> {
    let mut stdout = io::stdout();
    stdout.execute(SetForegroundColor(Color::Green))?;
    print!("you: ");
    stdout.execute(ResetColor)?;
    stdout.flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

/// Print the response prefix with ted's name and active cap badges (excluding "base")
fn print_response_prefix(active_caps: &[String]) -> Result<()> {
    let mut stdout = io::stdout();

    // Print newline and ted's name
    stdout.execute(SetForegroundColor(Color::Cyan))?;
    print!("\nted");
    stdout.execute(ResetColor)?;

    // Print cap badges (excluding "base")
    let display_caps: Vec<_> = active_caps.iter().filter(|c| *c != "base").collect();
    if !display_caps.is_empty() {
        print!(" ");
        for (i, cap) in display_caps.iter().enumerate() {
            if i > 0 {
                print!(" ");
            }
            print_cap_badge(cap)?;
        }
    }

    // Print the colon separator
    stdout.execute(SetForegroundColor(Color::Cyan))?;
    print!(": ");
    stdout.execute(ResetColor)?;
    stdout.flush()?;

    Ok(())
}

/// Run caps subcommands
fn run_caps_command(args: ted::cli::CapsArgs) -> Result<()> {
    let loader = CapLoader::new();

    match args.command {
        ted::cli::CapsCommands::List { detailed } => {
            let available = loader.list_available()?;

            println!("\nAvailable caps:\n");

            // Separate built-in and custom caps
            let mut builtins: Vec<_> = available.iter().filter(|(_, builtin)| *builtin).collect();
            let mut custom: Vec<_> = available.iter().filter(|(_, builtin)| !*builtin).collect();

            builtins.sort_by(|a, b| a.0.cmp(&b.0));
            custom.sort_by(|a, b| a.0.cmp(&b.0));

            if !builtins.is_empty() {
                println!("  Built-in:");
                for (name, _) in &builtins {
                    if detailed {
                        if let Ok(cap) = loader.load(name) {
                            println!("    {} - {}", name, cap.description);
                        } else {
                            println!("    {}", name);
                        }
                    } else {
                        println!("    {}", name);
                    }
                }
            }

            if !custom.is_empty() {
                println!("\n  Custom:");
                for (name, _) in &custom {
                    if detailed {
                        if let Ok(cap) = loader.load(name) {
                            println!("    {} - {}", name, cap.description);
                        } else {
                            println!("    {}", name);
                        }
                    } else {
                        println!("    {}", name);
                    }
                }
            }

            println!();
        }

        ted::cli::CapsCommands::Show { name } => {
            let cap = loader.load(&name)?;

            println!("\n{}", cap.name);
            println!("{}", "=".repeat(cap.name.len()));
            println!();
            println!("Description: {}", cap.description);
            println!("Version: {}", cap.version);
            println!("Priority: {}", cap.priority);

            if cap.is_builtin {
                println!("Type: Built-in");
            } else if let Some(path) = &cap.source_path {
                println!("Type: Custom");
                println!("Source: {}", path.display());
            }

            if !cap.extends.is_empty() {
                println!("Extends: {}", cap.extends.join(", "));
            }

            if cap.model.is_some() {
                let model = cap.model.as_ref().unwrap();
                if let Some(preferred) = &model.preferred_model {
                    println!("Preferred model: {}", preferred);
                }
            }

            println!("\nSystem Prompt:");
            println!("---");
            // Show first 500 chars of system prompt
            let prompt_preview = if cap.system_prompt.len() > 500 {
                format!(
                    "{}...\n(truncated, {} total chars)",
                    &cap.system_prompt[..500],
                    cap.system_prompt.len()
                )
            } else {
                cap.system_prompt.clone()
            };
            println!("{}", prompt_preview);
            println!("---");
            println!();
        }

        ted::cli::CapsCommands::Create { name } => {
            let caps_dir = Settings::caps_dir();
            std::fs::create_dir_all(&caps_dir)?;
            let cap_path = caps_dir.join(format!("{}.toml", name));

            if cap_path.exists() {
                return Err(TedError::InvalidInput(format!(
                    "Cap '{}' already exists at {}",
                    name,
                    cap_path.display()
                )));
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
                    // Use default
                    break;
                }
                if line.is_empty() && !system_prompt_lines.is_empty() {
                    // Done entering
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
            println!("\nTo use it: ted -c {} or /cap add {}", name, name);
            println!("To edit:   ted caps edit {}", name);
        }

        ted::cli::CapsCommands::Edit { name } => {
            // Try to find the cap file
            let caps_dir = Settings::caps_dir();
            let cap_path = caps_dir.join(format!("{}.toml", name));

            if !cap_path.exists() {
                // Check if it's a built-in
                if ted::caps::builtin::get_builtin(&name).is_some() {
                    return Err(TedError::InvalidInput(format!(
                        "Cannot edit built-in cap '{}'. Create a custom cap with the same name to override it.",
                        name
                    )));
                }
                return Err(TedError::InvalidInput(format!(
                    "Cap '{}' not found. Create it first with 'ted caps create {}'",
                    name, name
                )));
            }

            // Open in default editor
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
            let status = std::process::Command::new(&editor)
                .arg(&cap_path)
                .status()?;

            if status.success() {
                println!("Cap '{}' updated.", name);
            } else {
                println!("Editor exited with non-zero status.");
            }
        }

        ted::cli::CapsCommands::Import { source } => {
            // Read the source file
            let content = if source.starts_with("http://") || source.starts_with("https://") {
                return Err(TedError::InvalidInput(
                    "URL imports not yet implemented. Download the file and import locally."
                        .to_string(),
                ));
            } else {
                std::fs::read_to_string(&source)?
            };

            // Parse to validate
            let cap: ted::caps::Cap = toml::from_str(&content)
                .map_err(|e| TedError::Config(format!("Invalid cap file: {}", e)))?;

            // Save to caps directory
            let caps_dir = Settings::caps_dir();
            let dest_path = caps_dir.join(format!("{}.toml", cap.name));

            if dest_path.exists() {
                return Err(TedError::InvalidInput(format!(
                    "Cap '{}' already exists. Remove it first or choose a different name.",
                    cap.name
                )));
            }

            std::fs::write(&dest_path, content)?;
            println!("Imported cap '{}' to {}", cap.name, dest_path.display());
        }

        ted::cli::CapsCommands::Export { name, output } => {
            let cap = loader.load(&name)?;

            // Serialize to TOML
            let content = toml::to_string_pretty(&cap)
                .map_err(|e| TedError::Config(format!("Failed to serialize cap: {}", e)))?;

            match output {
                Some(path) => {
                    std::fs::write(&path, &content)?;
                    println!("Exported cap '{}' to {}", name, path.display());
                }
                None => {
                    println!("{}", content);
                }
            }
        }
    }

    Ok(())
}

/// Run a custom command from .ted/commands/
fn run_custom_command(args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        // List available custom commands
        let commands = ted::commands::discover_commands()?;

        if commands.is_empty() {
            println!("\nNo custom commands found.");
            println!("Add executable scripts to ~/.ted/commands/ or ./.ted/commands/");
            return Ok(());
        }

        println!("\nAvailable custom commands:\n");
        let mut sorted: Vec<_> = commands.values().collect();
        sorted.sort_by_key(|c| &c.name);

        for cmd in sorted {
            let scope = if cmd.is_local { "local" } else { "global" };
            println!("  {} ({}) - {}", cmd.name, scope, cmd.path.display());
        }
        println!();
        return Ok(());
    }

    let command_name = &args[0];
    let command_args = &args[1..];

    // Look up the command
    match ted::commands::get_command(command_name)? {
        Some(cmd) => {
            let exit_code = ted::commands::execute_command(&cmd, command_args)?;
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        None => {
            return Err(TedError::InvalidInput(format!(
                "Unknown command '{}'. Run 'ted' with no arguments to see available commands.",
                command_name
            )));
        }
    }

    Ok(())
}

/// Run history subcommands
fn run_history_command(args: ted::cli::HistoryArgs) -> Result<()> {
    let store = HistoryStore::open()?;

    match args.command {
        ted::cli::HistoryCommands::List { limit } => {
            let sessions = store.list_recent(limit);

            if sessions.is_empty() {
                println!("\nNo sessions in history.\n");
                return Ok(());
            }

            println!("\nRecent sessions:\n");
            for session in sessions {
                let id_short = &session.id.to_string()[..8];
                let date = session.last_active.format("%Y-%m-%d %H:%M");
                let dir = session
                    .working_directory
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?");

                let summary = session.summary.as_deref().unwrap_or("(no summary)");

                println!("  {} | {} | {} | {}", id_short, date, dir, summary);
            }
            println!();
        }

        ted::cli::HistoryCommands::Search { query } => {
            let results = store.search(&query);

            if results.is_empty() {
                println!("\nNo sessions matching '{}'.\n", query);
                return Ok(());
            }

            println!("\nSessions matching '{}':\n", query);
            for session in results {
                let id_short = &session.id.to_string()[..8];
                let date = session.last_active.format("%Y-%m-%d %H:%M");
                let summary = session.summary.as_deref().unwrap_or("(no summary)");

                println!("  {} | {} | {}", id_short, date, summary);
            }
            println!();
        }

        ted::cli::HistoryCommands::Show { session_id } => {
            // Parse session ID (support both full and short forms)
            let id = if session_id.len() == 8 {
                // Short form - find matching session
                let sessions = store.list_recent(1000);
                sessions
                    .iter()
                    .find(|s| s.id.to_string().starts_with(&session_id))
                    .map(|s| s.id)
                    .ok_or_else(|| {
                        TedError::InvalidInput(format!(
                            "No session found matching '{}'",
                            session_id
                        ))
                    })?
            } else {
                uuid::Uuid::parse_str(&session_id)
                    .map_err(|_| TedError::InvalidInput("Invalid session ID format".to_string()))?
            };

            let session = store.get(id).ok_or_else(|| {
                TedError::InvalidInput(format!("Session '{}' not found", session_id))
            })?;

            println!("\nSession: {}", session.id);
            println!(
                "Started: {}",
                session.started_at.format("%Y-%m-%d %H:%M:%S")
            );
            println!(
                "Last active: {}",
                session.last_active.format("%Y-%m-%d %H:%M:%S")
            );
            println!("Directory: {}", session.working_directory.display());
            if let Some(ref root) = session.project_root {
                println!("Project root: {}", root.display());
            }
            println!("Messages: {}", session.message_count);
            if !session.caps.is_empty() {
                println!("Caps: {}", session.caps.join(", "));
            }
            if let Some(ref summary) = session.summary {
                println!("\nSummary: {}", summary);
            }
            println!();

            println!("To resume this session:");
            println!("  ted chat --resume {}\n", &session.id.to_string()[..8]);
        }

        ted::cli::HistoryCommands::Delete { session_id } => {
            let mut store = HistoryStore::open()?;

            let id = uuid::Uuid::parse_str(&session_id)
                .map_err(|_| TedError::InvalidInput("Invalid session ID format".to_string()))?;

            if store.delete(id)? {
                println!("Session deleted.");
            } else {
                println!("Session not found.");
            }
        }

        ted::cli::HistoryCommands::Clear { force } => {
            if !force {
                println!("This will delete ALL session history.");
                println!("Run with --force to confirm.");
                return Ok(());
            }

            let mut store = HistoryStore::open()?;
            let removed = store.cleanup(0)?; // Remove all
            println!("Cleared {} sessions from history.", removed);
        }
    }

    Ok(())
}

/// Run context subcommands
async fn run_context_command(args: ted::cli::ContextArgs, settings: &Settings) -> Result<()> {
    match args.command {
        ted::cli::ContextCommands::Stats => {
            // Show overall storage stats
            let context_path = &settings.context.storage_path;
            let history_path = Settings::history_dir();

            println!("\nContext Storage Statistics");
            println!("─────────────────────────────────────");

            // Calculate context directory size
            let context_size = utils::calculate_dir_size(context_path);
            let history_size = utils::calculate_dir_size(&history_path);

            // Count sessions
            let store = HistoryStore::open()?;
            let sessions = store.list_recent(10000);
            let session_count = sessions.len();

            println!("  Sessions:        {}", session_count);
            println!("  Context path:    {}", context_path.display());
            println!("  Context size:    {}", utils::format_size(context_size));
            println!("  History path:    {}", history_path.display());
            println!("  History size:    {}", utils::format_size(history_size));
            println!(
                "  Total size:      {}",
                utils::format_size(context_size + history_size)
            );
            println!();
            println!(
                "  Retention:       {} days",
                settings.context.cold_retention_days
            );
            println!(
                "  Auto-compact:    {}",
                if settings.context.auto_compact {
                    "enabled"
                } else {
                    "disabled"
                }
            );
            println!("─────────────────────────────────────\n");
        }

        ted::cli::ContextCommands::Usage => {
            // Show per-session usage
            let store = HistoryStore::open()?;
            let sessions = store.list_recent(20);
            let context_path = &settings.context.storage_path;

            println!("\nDisk Usage by Session");
            println!("─────────────────────────────────────");

            let mut total_size = 0u64;
            for session in &sessions {
                let session_path = context_path.join(session.id.to_string());
                let size = utils::calculate_dir_size(&session_path);
                total_size += size;

                let id_short = &session.id.to_string()[..8];
                let date = session.last_active.format("%Y-%m-%d");
                let summary = session
                    .summary
                    .as_deref()
                    .map(|s| {
                        if s.len() > 30 {
                            format!("{}...", &s[..27])
                        } else {
                            s.to_string()
                        }
                    })
                    .unwrap_or_else(|| "(no summary)".to_string());

                println!(
                    "  {} | {} | {:>8} | {}",
                    id_short,
                    date,
                    utils::format_size(size),
                    summary
                );
            }
            println!("─────────────────────────────────────");
            println!("  Total (shown):   {}", utils::format_size(total_size));
            println!();
        }

        ted::cli::ContextCommands::Prune {
            days,
            force,
            dry_run,
        } => {
            let retention_days = days.unwrap_or(settings.context.cold_retention_days);
            let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);

            let store = HistoryStore::open()?;
            let sessions = store.list_recent(10000);
            let context_path = &settings.context.storage_path;

            // Find sessions to prune
            let to_prune: Vec<_> = sessions.iter().filter(|s| s.last_active < cutoff).collect();

            if to_prune.is_empty() {
                println!("\nNo sessions older than {} days found.\n", retention_days);
                return Ok(());
            }

            // Calculate space to be freed
            let mut total_size = 0u64;
            for session in &to_prune {
                let session_path = context_path.join(session.id.to_string());
                total_size += utils::calculate_dir_size(&session_path);
            }

            println!("\nSessions to prune ({} days old):", retention_days);
            println!("─────────────────────────────────────");
            for session in &to_prune {
                let id_short = &session.id.to_string()[..8];
                let date = session.last_active.format("%Y-%m-%d");
                let session_path = context_path.join(session.id.to_string());
                let size = utils::calculate_dir_size(&session_path);
                println!("  {} | {} | {}", id_short, date, utils::format_size(size));
            }
            println!("─────────────────────────────────────");
            println!(
                "  {} sessions, {} to free\n",
                to_prune.len(),
                utils::format_size(total_size)
            );

            if dry_run {
                println!("Dry run - no changes made.");
                return Ok(());
            }

            if !force {
                println!("Run with --force to delete these sessions.");
                println!("Run with --dry-run to preview without deleting.\n");
                return Ok(());
            }

            // Actually delete
            let mut deleted_count = 0;
            for session in &to_prune {
                // Delete context data
                let session_path = context_path.join(session.id.to_string());
                if session_path.exists() {
                    let _ = std::fs::remove_dir_all(&session_path);
                }

                // Delete from history
                let mut store = HistoryStore::open()?;
                let _ = store.delete(session.id);
                deleted_count += 1;
            }

            println!(
                "Deleted {} sessions, freed {}.",
                deleted_count,
                utils::format_size(total_size)
            );
        }

        ted::cli::ContextCommands::Clear { force } => {
            if !force {
                println!("\n⚠ This will delete ALL context data for ALL sessions!");
                println!("Run with --force to confirm.\n");
                return Ok(());
            }

            let context_path = &settings.context.storage_path;

            // Calculate size before deletion
            let size = utils::calculate_dir_size(context_path);

            // Delete all contents
            if context_path.exists() {
                std::fs::remove_dir_all(context_path)?;
                std::fs::create_dir_all(context_path)?;
            }

            println!(
                "\nCleared all context data ({} freed).\n",
                utils::format_size(size)
            );
        }
    }

    Ok(())
}

/// Resume a session by its ID (supports short or full ID)
fn resume_session(
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
fn prompt_session_choice(sessions: &[&SessionInfo]) -> Result<Option<SessionInfo>> {
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
fn print_tool_invocation(tool_name: &str, input: &serde_json::Value) -> Result<()> {
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
const SHELL_OUTPUT_MAX_LINES: usize = 15;

/// Print tool result with visual formatting
fn print_tool_result(tool_name: &str, result: &ToolResult) -> Result<()> {
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
fn print_shell_output(output: &str) -> Result<()> {
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
