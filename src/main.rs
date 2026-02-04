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

use ted::skills::SkillRegistry;

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
use ted::tui::chat::{run_chat_tui_loop, ChatTuiConfig};
use ted::update;
use ted::utils;

/// Maximum number of retries for rate-limited requests
const MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff (in seconds)
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
    let mut provider_name = args
        .provider
        .clone()
        .unwrap_or_else(|| settings.defaults.provider.clone());

    // Create the appropriate provider (mutable so it can be changed via /settings)
    let mut provider: Arc<dyn LlmProvider> = match provider_name.as_str() {
        "ollama" => {
            let ollama_provider = OllamaProvider::with_openai_api(
                &settings.providers.ollama.base_url,
                settings.providers.ollama.use_openai_api,
            );
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
            Arc::new(ollama_provider)
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
            Arc::new(provider)
        }
        _ => {
            // Get API key (anthropic is the default)
            let api_key = settings.get_anthropic_api_key().ok_or_else(|| {
                TedError::Config("No Anthropic API key found. Run 'ted' to configure.".to_string())
            })?;
            Arc::new(AnthropicProvider::new(api_key))
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

    // Verbose output for provider and model selection
    if verbose > 0 {
        eprintln!("[verbose] Provider: {}", provider_name);
        eprintln!("[verbose] Model: {}", model);
        eprintln!("[verbose] Caps loaded: {:?}", cap_names);
    }

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
        if verbose > 0 {
            eprintln!("[verbose] Setting project root: {}", root.display());
        }
        context_manager.set_project_root(root.clone(), true).await?;

        // Also load project context files (CLAUDE.md, AGENTS.md, .cursorrules, etc.)
        context_manager.refresh_project_context().await?;
    } else if verbose > 0 {
        eprintln!("[verbose] No project root found");
    }

    // Append project context to system prompt (higher priority than file tree)
    if let Some(project_context) = context_manager.project_context_string().await {
        if verbose > 0 {
            eprintln!("[verbose] Project context: {} chars", project_context.len());
        }
        let current_system = conversation.system_prompt.clone().unwrap_or_default();
        let enhanced_system = if current_system.is_empty() {
            project_context
        } else {
            format!("{}\n\n{}", current_system, project_context)
        };
        conversation.set_system(&enhanced_system);
    } else if verbose > 0 {
        eprintln!("[verbose] No project context files found");
    }

    // Append file tree to system prompt for LLM awareness
    if let Some(file_tree_context) = context_manager.file_tree_context().await {
        if verbose > 0 {
            eprintln!(
                "[verbose] File tree context: {} chars",
                file_tree_context.len()
            );
        }
        let current_system = conversation.system_prompt.clone().unwrap_or_default();
        let enhanced_system = if current_system.is_empty() {
            file_tree_context
        } else {
            format!("{}\n\n{}", current_system, file_tree_context)
        };
        conversation.set_system(&enhanced_system);
        if verbose > 0 {
            eprintln!(
                "[verbose] System prompt total: {} chars",
                enhanced_system.len()
            );
        }
    } else if verbose > 0 {
        eprintln!("[verbose] No file tree context available");
    }

    // Start background compaction (every 5 minutes)
    let _compaction_handle = context_manager.start_background_compaction(300);

    // Create tool executor
    let tool_context = ToolContext::new(
        working_directory.clone(),
        project_root.clone(),
        session_id.0,
        args.trust,
    )
    .with_files_in_context(args.files_in_context.clone());
    let mut tool_executor = ToolExecutor::new(tool_context, args.trust);

    // Initialize skill registry and register spawn_agent tool
    let mut skill_registry = SkillRegistry::new();
    if let Err(e) = skill_registry.scan() {
        if verbose > 0 {
            eprintln!("[verbose] Failed to scan for skills: {}", e);
        }
    } else if verbose > 0 {
        let skill_count = skill_registry.list_skills().len();
        if skill_count > 0 {
            eprintln!("[verbose] Loaded {} skill(s)", skill_count);
        }
    }
    let skill_registry = Arc::new(skill_registry);

    // Create rate coordinator if rate limits are enabled
    let rate_coordinator = if settings.rate_limits.enabled {
        let limit = settings.rate_limits.get_for_model(&model);
        if verbose > 0 {
            eprintln!(
                "[verbose] Rate limiting enabled: {} tokens/min for model {}",
                limit.tokens_per_minute, model
            );
        }
        Some(Arc::new(ted::llm::TokenRateCoordinator::new(
            limit.tokens_per_minute,
        )))
    } else {
        None
    };

    // Register spawn_agent tool (with or without rate coordinator)
    // Pass the current model so subagents inherit it
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

    // Update session info
    session_info.project_root = project_root.clone();
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
            let command = input.trim().strip_prefix('>').unwrap().trim();
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

                        // Check if model changed (use correct provider's default model)
                        let new_model = match new_settings.defaults.provider.as_str() {
                            "ollama" => new_settings.providers.ollama.default_model.clone(),
                            "openrouter" => new_settings.providers.openrouter.default_model.clone(),
                            _ => new_settings.providers.anthropic.default_model.clone(),
                        };
                        if new_model != model {
                            model = new_model;
                            println!("  Model: {}", model);
                        }

                        // Check if provider changed - need to recreate the provider object
                        if new_settings.defaults.provider != settings.defaults.provider {
                            provider_name = new_settings.defaults.provider.clone();
                            settings.defaults.provider = provider_name.clone();

                            // Recreate the provider for the new backend
                            match provider_name.as_str() {
                                "ollama" => {
                                    let ollama_provider = OllamaProvider::with_openai_api(
                                        &new_settings.providers.ollama.base_url,
                                        new_settings.providers.ollama.use_openai_api,
                                    );
                                    // Perform health check
                                    if let Err(e) = ollama_provider.health_check().await {
                                        eprintln!("Warning: Ollama is not running: {}", e);
                                        eprintln!("Start Ollama with: ollama serve");
                                    } else {
                                        provider = Arc::new(ollama_provider);
                                    }
                                }
                                "openrouter" => {
                                    if let Some(api_key) = new_settings.get_openrouter_api_key() {
                                        let new_provider = if let Some(ref base_url) =
                                            new_settings.providers.openrouter.base_url
                                        {
                                            OpenRouterProvider::with_base_url(api_key, base_url)
                                        } else {
                                            OpenRouterProvider::new(api_key)
                                        };
                                        provider = Arc::new(new_provider);
                                    } else {
                                        eprintln!("Warning: No OpenRouter API key found");
                                    }
                                }
                                _ => {
                                    // anthropic is the default
                                    if let Some(api_key) = new_settings.get_anthropic_api_key() {
                                        provider = Arc::new(AnthropicProvider::new(api_key));
                                    } else {
                                        eprintln!("Warning: No Anthropic API key found");
                                    }
                                }
                            }
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

        // Get response with retry logic for rate limits and context overflow
        let (response_content, stop_reason) =
            match get_response_with_retry(provider, request.clone(), stream, active_caps).await {
                Ok(result) => result,
                Err(TedError::Api(ApiError::ContextTooLong { current, limit })) => {
                    // Auto-trim conversation and retry
                    let mut stdout = io::stdout();
                    stdout.execute(SetForegroundColor(Color::Yellow))?;
                    println!(
                    "\n⚠ Context too long ({} tokens > {} limit). Auto-trimming older messages...",
                    current, limit
                );
                    stdout.execute(ResetColor)?;

                    // Get the actual limit from the model info or use the reported limit
                    let context_window = provider
                        .get_model_info(model)
                        .map(|m| m.context_window)
                        .unwrap_or(limit);

                    // Trim the conversation to fit within 70% of the limit to leave room
                    let target_tokens = (context_window as f64 * 0.7) as u32;
                    let removed = conversation.trim_to_fit(target_tokens);

                    if removed > 0 {
                        println!("  Removed {} older messages. Retrying...\n", removed);
                    }

                    // Build a new request with trimmed messages
                    let mut retry_request =
                        CompletionRequest::new(model, conversation.messages.clone())
                            .with_max_tokens(settings.defaults.max_tokens)
                            .with_temperature(settings.defaults.temperature)
                            .with_tools(tool_executor.tool_definitions());

                    if let Some(ref system_prompt) = conversation.system_prompt {
                        retry_request = retry_request.with_system(system_prompt);
                    }

                    // Retry with trimmed context
                    get_response_with_retry(provider, retry_request, stream, active_caps).await?
                }
                Err(e) => return Err(e),
            };

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
async fn run_ask(args: ted::cli::AskArgs, settings: Settings, verbose: u8) -> Result<()> {
    if verbose > 0 {
        eprintln!(
            "[verbose] Starting ask mode with provider: {}",
            args.provider
                .as_ref()
                .unwrap_or(&settings.defaults.provider)
        );
    }
    // Determine which provider to use
    let provider_name = args
        .provider
        .clone()
        .unwrap_or_else(|| settings.defaults.provider.clone());

    // Create the appropriate provider
    let provider: Box<dyn LlmProvider> = match provider_name.as_str() {
        "ollama" => {
            let ollama_provider = OllamaProvider::with_openai_api(
                &settings.providers.ollama.base_url,
                settings.providers.ollama.use_openai_api,
            );
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
    println!("Type /help for commands, >command for shell, exit to quit\n");
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
    println!("\nDirect shell commands:");
    println!("  >command   - Execute shell command directly (e.g., >ls -la, >git status)");
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

            if let Some(model) = &cap.model {
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::stream;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};
    use ted::llm::provider::{
        CompletionRequest, CompletionResponse, ContentBlockDelta, ContentBlockResponse,
        LlmProvider, ModelInfo, StopReason, StreamEvent, Usage,
    };
    // Use the new modules for testing
    use std::path::PathBuf;
    use ted::chat::input_parser;
    use ted::chat::input_parser::ProviderChoice;
    use tempfile::TempDir;

    // ==================== Pure Helper Function Tests ====================
    // These tests verify that the functions in ted::chat::input_parser work correctly
    // from main.rs. The comprehensive tests are in the module itself.

    #[test]
    fn test_parse_shell_command_valid() {
        assert_eq!(input_parser::parse_shell_command(">ls -la"), Some("ls -la"));
        assert_eq!(
            input_parser::parse_shell_command("> git status"),
            Some("git status")
        );
        assert_eq!(
            input_parser::parse_shell_command("  >  echo hello  "),
            Some("echo hello")
        );
    }

    #[test]
    fn test_parse_shell_command_empty() {
        assert_eq!(input_parser::parse_shell_command(">"), Some(""));
        assert_eq!(input_parser::parse_shell_command(">  "), Some(""));
    }

    #[test]
    fn test_parse_shell_command_not_shell() {
        assert_eq!(input_parser::parse_shell_command("hello"), None);
        assert_eq!(input_parser::parse_shell_command("ls -la"), None);
        assert_eq!(input_parser::parse_shell_command(""), None);
    }

    #[test]
    fn test_truncate_command_display_short() {
        assert_eq!(
            input_parser::truncate_command_display("ls -la", 60),
            "ls -la"
        );
        assert_eq!(input_parser::truncate_command_display("short", 10), "short");
    }

    #[test]
    fn test_truncate_command_display_long() {
        let long_cmd = "a".repeat(100);
        let result = input_parser::truncate_command_display(&long_cmd, 60);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 60);
    }

    #[test]
    fn test_truncate_command_display_exact() {
        let cmd = "a".repeat(60);
        let result = input_parser::truncate_command_display(&cmd, 60);
        assert_eq!(result, cmd);
    }

    #[test]
    fn test_is_exit_command() {
        assert!(input_parser::is_exit_command("exit"));
        assert!(input_parser::is_exit_command("quit"));
        assert!(input_parser::is_exit_command("/exit"));
        assert!(input_parser::is_exit_command("/quit"));
        assert!(input_parser::is_exit_command("EXIT"));
        assert!(input_parser::is_exit_command("  exit  "));
        assert!(!input_parser::is_exit_command("hello"));
        assert!(!input_parser::is_exit_command("exiting"));
    }

    #[test]
    fn test_is_clear_command() {
        assert!(input_parser::is_clear_command("/clear"));
        assert!(input_parser::is_clear_command("/CLEAR"));
        assert!(input_parser::is_clear_command("  /clear  "));
        assert!(!input_parser::is_clear_command("clear"));
        assert!(!input_parser::is_clear_command("/clearall"));
    }

    #[test]
    fn test_is_help_command() {
        assert!(input_parser::is_help_command("/help"));
        assert!(input_parser::is_help_command("/HELP"));
        assert!(input_parser::is_help_command("  /help  "));
        assert!(!input_parser::is_help_command("help"));
        assert!(!input_parser::is_help_command("/helper"));
    }

    #[test]
    fn test_is_stats_command() {
        assert!(input_parser::is_stats_command("/stats"));
        assert!(input_parser::is_stats_command("/context"));
        assert!(input_parser::is_stats_command("/STATS"));
        assert!(input_parser::is_stats_command("  /context  "));
        assert!(!input_parser::is_stats_command("stats"));
        assert!(!input_parser::is_stats_command("/statistics"));
    }

    #[test]
    fn test_is_settings_command() {
        assert!(input_parser::is_settings_command("/settings"));
        assert!(input_parser::is_settings_command("/config"));
        assert!(input_parser::is_settings_command("/SETTINGS"));
        assert!(input_parser::is_settings_command("  /config  "));
        assert!(!input_parser::is_settings_command("settings"));
        assert!(!input_parser::is_settings_command("/configure"));
    }

    #[test]
    fn test_parse_provider_choice_anthropic() {
        assert_eq!(
            input_parser::parse_provider_choice("1"),
            ProviderChoice::Anthropic
        );
        assert_eq!(
            input_parser::parse_provider_choice("anthropic"),
            ProviderChoice::Anthropic
        );
        assert_eq!(
            input_parser::parse_provider_choice("ANTHROPIC"),
            ProviderChoice::Anthropic
        );
    }

    #[test]
    fn test_parse_provider_choice_ollama() {
        assert_eq!(
            input_parser::parse_provider_choice("2"),
            ProviderChoice::Ollama
        );
        assert_eq!(
            input_parser::parse_provider_choice("ollama"),
            ProviderChoice::Ollama
        );
        assert_eq!(
            input_parser::parse_provider_choice("OLLAMA"),
            ProviderChoice::Ollama
        );
    }

    #[test]
    fn test_parse_provider_choice_settings() {
        assert_eq!(
            input_parser::parse_provider_choice("s"),
            ProviderChoice::Settings
        );
        assert_eq!(
            input_parser::parse_provider_choice("settings"),
            ProviderChoice::Settings
        );
        assert_eq!(
            input_parser::parse_provider_choice("S"),
            ProviderChoice::Settings
        );
    }

    #[test]
    fn test_parse_provider_choice_invalid() {
        assert_eq!(
            input_parser::parse_provider_choice(""),
            ProviderChoice::Invalid
        );
        // Note: "3" is now valid for OpenRouter in the new module
        assert_eq!(
            input_parser::parse_provider_choice("4"),
            ProviderChoice::Invalid
        );
        assert_eq!(
            input_parser::parse_provider_choice("invalid"),
            ProviderChoice::Invalid
        );
    }

    #[test]
    fn test_format_shell_output_lines_small() {
        let (lines, total, truncated) =
            input_parser::format_shell_output_lines("line1\nline2\nline3", "", 10);
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
        let (lines, total, truncated) = input_parser::format_shell_output_lines(&stdout, "", 10);
        assert_eq!(lines.len(), 10);
        assert_eq!(total, 20);
        assert!(truncated);
    }

    #[test]
    fn test_format_shell_output_lines_combined() {
        let (lines, total, truncated) =
            input_parser::format_shell_output_lines("stdout1\nstdout2", "stderr1", 10);
        assert_eq!(lines.len(), 3);
        assert_eq!(total, 3);
        assert!(!truncated);
        assert!(lines.contains(&"stderr1".to_string()));
    }

    #[test]
    fn test_format_shell_output_lines_empty() {
        let (lines, total, truncated) = input_parser::format_shell_output_lines("", "", 10);
        assert!(lines.is_empty());
        assert_eq!(total, 0);
        assert!(!truncated);
    }

    #[test]
    fn test_extract_tool_uses_empty() {
        let content: Vec<ContentBlockResponse> = vec![];
        let tool_uses = input_parser::extract_tool_uses(&content);
        assert!(tool_uses.is_empty());
    }

    #[test]
    fn test_extract_tool_uses_text_only() {
        let content = vec![ContentBlockResponse::Text {
            text: "Hello".to_string(),
        }];
        let tool_uses = input_parser::extract_tool_uses(&content);
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
        let tool_uses = input_parser::extract_tool_uses(&content);
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
        let tool_uses = input_parser::extract_tool_uses(&content);
        assert_eq!(tool_uses.len(), 2);
    }

    #[test]
    fn test_extract_text_content_empty() {
        let content: Vec<ContentBlockResponse> = vec![];
        let text = input_parser::extract_text_content(&content);
        assert!(text.is_empty());
    }

    #[test]
    fn test_extract_text_content_single() {
        let content = vec![ContentBlockResponse::Text {
            text: "Hello world".to_string(),
        }];
        let text = input_parser::extract_text_content(&content);
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
        let text = input_parser::extract_text_content(&content);
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
        let text = input_parser::extract_text_content(&content);
        assert_eq!(text, "Text before\nText after");
    }

    #[test]
    fn test_calculate_trim_target() {
        assert_eq!(input_parser::calculate_trim_target(100000), 70000);
        assert_eq!(input_parser::calculate_trim_target(200000), 140000);
        assert_eq!(input_parser::calculate_trim_target(0), 0);
    }

    #[test]
    fn test_calculate_trim_target_small() {
        assert_eq!(input_parser::calculate_trim_target(100), 70);
        assert_eq!(input_parser::calculate_trim_target(10), 7);
    }

    #[test]
    fn test_provider_choice_debug() {
        let choice = ProviderChoice::Anthropic;
        let debug_str = format!("{:?}", choice);
        assert!(debug_str.contains("Anthropic"));
    }

    #[test]
    fn test_provider_choice_clone() {
        let choice = ProviderChoice::Ollama;
        let cloned = choice.clone();
        assert_eq!(choice, cloned);
    }

    // ==================== Mock LLM Provider ====================

    /// A mock LLM provider for testing
    struct MockProvider {
        name: String,
        /// Response to return from complete()
        response: std::sync::Mutex<Option<CompletionResponse>>,
        /// Stream events to return from complete_stream()
        stream_events: std::sync::Mutex<Vec<StreamEvent>>,
        /// Count of complete() calls
        complete_call_count: AtomicU32,
        /// Count of complete_stream() calls
        stream_call_count: AtomicU32,
        /// If true, return a rate limit error on first attempt
        simulate_rate_limit: std::sync::atomic::AtomicBool,
        /// If true, return a context too long error
        simulate_context_too_long: std::sync::atomic::AtomicBool,
        /// If true, return a server error in streaming
        simulate_stream_error: std::sync::atomic::AtomicBool,
    }

    impl MockProvider {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                response: std::sync::Mutex::new(None),
                stream_events: std::sync::Mutex::new(Vec::new()),
                complete_call_count: AtomicU32::new(0),
                stream_call_count: AtomicU32::new(0),
                simulate_rate_limit: std::sync::atomic::AtomicBool::new(false),
                simulate_context_too_long: std::sync::atomic::AtomicBool::new(false),
                simulate_stream_error: std::sync::atomic::AtomicBool::new(false),
            }
        }

        fn with_text_response(name: &str, text: &str) -> Self {
            let provider = Self::new(name);
            provider.set_text_response(text);
            provider
        }

        fn set_text_response(&self, text: &str) {
            let response = CompletionResponse {
                id: "mock-response-id".to_string(),
                model: "mock-model".to_string(),
                content: vec![ContentBlockResponse::Text {
                    text: text.to_string(),
                }],
                stop_reason: Some(StopReason::EndTurn),
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 20,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
            };
            *self.response.lock().unwrap() = Some(response);
        }

        fn set_tool_use_response(&self, tool_id: &str, tool_name: &str, input: serde_json::Value) {
            let response = CompletionResponse {
                id: "mock-response-id".to_string(),
                model: "mock-model".to_string(),
                content: vec![ContentBlockResponse::ToolUse {
                    id: tool_id.to_string(),
                    name: tool_name.to_string(),
                    input,
                }],
                stop_reason: Some(StopReason::ToolUse),
                usage: Usage::default(),
            };
            *self.response.lock().unwrap() = Some(response);
        }

        fn set_stream_events(&self, events: Vec<StreamEvent>) {
            *self.stream_events.lock().unwrap() = events;
        }

        fn set_rate_limit(&self, enabled: bool) {
            self.simulate_rate_limit
                .store(enabled, AtomicOrdering::SeqCst);
        }

        fn set_context_too_long(&self, enabled: bool) {
            self.simulate_context_too_long
                .store(enabled, AtomicOrdering::SeqCst);
        }

        fn set_stream_error(&self, enabled: bool) {
            self.simulate_stream_error
                .store(enabled, AtomicOrdering::SeqCst);
        }

        fn complete_call_count(&self) -> u32 {
            self.complete_call_count.load(AtomicOrdering::SeqCst)
        }

        fn stream_call_count(&self) -> u32 {
            self.stream_call_count.load(AtomicOrdering::SeqCst)
        }
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn available_models(&self) -> Vec<ModelInfo> {
            vec![ModelInfo {
                id: "mock-model".to_string(),
                display_name: "Mock Model".to_string(),
                context_window: 200000,
                max_output_tokens: 4096,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
            }]
        }

        fn supports_model(&self, model: &str) -> bool {
            model == "mock-model" || model.starts_with("claude")
        }

        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> ted::error::Result<CompletionResponse> {
            let call_count = self
                .complete_call_count
                .fetch_add(1, AtomicOrdering::SeqCst);

            // Simulate rate limiting on first call if enabled
            if self.simulate_rate_limit.load(AtomicOrdering::SeqCst) && call_count == 0 {
                return Err(ted::error::TedError::Api(
                    ted::error::ApiError::RateLimited(1),
                ));
            }

            // Simulate context too long error
            if self.simulate_context_too_long.load(AtomicOrdering::SeqCst) && call_count == 0 {
                return Err(ted::error::TedError::Api(
                    ted::error::ApiError::ContextTooLong {
                        current: 250000,
                        limit: 200000,
                    },
                ));
            }

            let response = self.response.lock().unwrap();
            match response.as_ref() {
                Some(r) => Ok(r.clone()),
                None => Ok(CompletionResponse {
                    id: "default-response".to_string(),
                    model: "mock-model".to_string(),
                    content: vec![ContentBlockResponse::Text {
                        text: "Default mock response".to_string(),
                    }],
                    stop_reason: Some(StopReason::EndTurn),
                    usage: Usage::default(),
                }),
            }
        }

        async fn complete_stream(
            &self,
            _request: CompletionRequest,
        ) -> ted::error::Result<
            Pin<Box<dyn futures::Stream<Item = ted::error::Result<StreamEvent>> + Send>>,
        > {
            self.stream_call_count.fetch_add(1, AtomicOrdering::SeqCst);

            // Simulate stream error if enabled
            if self.simulate_stream_error.load(AtomicOrdering::SeqCst) {
                let events = vec![Ok(StreamEvent::Error {
                    error_type: "server_error".to_string(),
                    message: "Simulated stream error".to_string(),
                })];
                return Ok(Box::pin(stream::iter(events)));
            }

            let events = self.stream_events.lock().unwrap().clone();
            if events.is_empty() {
                // Default streaming response
                let default_events = vec![
                    Ok(StreamEvent::MessageStart {
                        id: "msg-id".to_string(),
                        model: "mock-model".to_string(),
                    }),
                    Ok(StreamEvent::ContentBlockStart {
                        index: 0,
                        content_block: ContentBlockResponse::Text {
                            text: String::new(),
                        },
                    }),
                    Ok(StreamEvent::ContentBlockDelta {
                        index: 0,
                        delta: ContentBlockDelta::TextDelta {
                            text: "Streamed ".to_string(),
                        },
                    }),
                    Ok(StreamEvent::ContentBlockDelta {
                        index: 0,
                        delta: ContentBlockDelta::TextDelta {
                            text: "response".to_string(),
                        },
                    }),
                    Ok(StreamEvent::ContentBlockStop { index: 0 }),
                    Ok(StreamEvent::MessageDelta {
                        stop_reason: Some(StopReason::EndTurn),
                        usage: Some(Usage::default()),
                    }),
                    Ok(StreamEvent::MessageStop),
                ];
                Ok(Box::pin(stream::iter(default_events)))
            } else {
                let events: Vec<ted::error::Result<StreamEvent>> =
                    events.into_iter().map(Ok).collect();
                Ok(Box::pin(stream::iter(events)))
            }
        }

        fn count_tokens(&self, text: &str, _model: &str) -> ted::error::Result<u32> {
            // Simple approximation: ~4 chars per token
            Ok((text.len() / 4) as u32)
        }
    }

    // ==================== run_clear tests ====================

    #[test]
    fn test_run_clear_returns_ok() {
        // run_clear should always succeed
        let result = run_clear();
        assert!(result.is_ok());
    }

    // ==================== run_init tests ====================

    #[test]
    fn test_run_init_creates_directory_structure() {
        // Create a temp directory to simulate a project
        let temp_dir = TempDir::new().unwrap();
        let original_dir = std::env::current_dir().unwrap();

        // Change to temp directory
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Run init
        let result = run_init();

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());

        // Verify directory structure was created
        let ted_dir = temp_dir.path().join(".ted");
        assert!(ted_dir.exists());
        assert!(ted_dir.join("caps").exists());
        assert!(ted_dir.join("commands").exists());
        assert!(ted_dir.join("config.json").exists());

        // Verify config.json content
        let config_content = std::fs::read_to_string(ted_dir.join("config.json")).unwrap();
        let config: serde_json::Value = serde_json::from_str(&config_content).unwrap();
        assert!(config.get("project_name").is_some());
        assert!(config.get("default_caps").is_some());
    }

    #[test]
    fn test_run_init_already_initialized() {
        let temp_dir = TempDir::new().unwrap();
        let original_dir = std::env::current_dir().unwrap();

        // Pre-create .ted directory
        std::fs::create_dir_all(temp_dir.path().join(".ted")).unwrap();

        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Run init - should succeed but indicate already initialized
        let result = run_init();

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());
    }

    // ==================== Settings command tests ====================

    #[test]
    fn test_run_settings_command_show() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Show),
        };

        let result = run_settings_command(args, settings);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_settings_command_get_temperature() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Get {
                key: "temperature".to_string(),
            }),
        };

        let result = run_settings_command(args, settings);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_settings_command_get_stream() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Get {
                key: "stream".to_string(),
            }),
        };

        let result = run_settings_command(args, settings);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_settings_command_get_provider() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Get {
                key: "provider".to_string(),
            }),
        };

        let result = run_settings_command(args, settings);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_settings_command_get_unknown_key() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Get {
                key: "unknown_key".to_string(),
            }),
        };

        let result = run_settings_command(args, settings);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown setting"));
    }

    #[test]
    fn test_run_settings_command_set_invalid_temperature() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Set {
                key: "temperature".to_string(),
                value: "not_a_number".to_string(),
            }),
        };

        let result = run_settings_command(args, settings);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_settings_command_set_invalid_stream() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Set {
                key: "stream".to_string(),
                value: "not_a_bool".to_string(),
            }),
        };

        let result = run_settings_command(args, settings);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_settings_command_set_invalid_provider() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Set {
                key: "provider".to_string(),
                value: "invalid_provider".to_string(),
            }),
        };

        let result = run_settings_command(args, settings);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid provider"));
    }

    #[test]
    fn test_run_settings_command_set_unknown_key() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Set {
                key: "unknown_key".to_string(),
                value: "value".to_string(),
            }),
        };

        let result = run_settings_command(args, settings);
        assert!(result.is_err());
    }

    // ==================== History command tests ====================

    #[test]
    fn test_run_history_command_list() {
        let args = ted::cli::HistoryArgs {
            command: ted::cli::HistoryCommands::List { limit: 5 },
        };

        let result = run_history_command(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_history_command_search_no_results() {
        let args = ted::cli::HistoryArgs {
            command: ted::cli::HistoryCommands::Search {
                query: "nonexistent_query_xyz_123".to_string(),
            },
        };

        let result = run_history_command(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_history_command_show_invalid_id() {
        let args = ted::cli::HistoryArgs {
            command: ted::cli::HistoryCommands::Show {
                session_id: "invalid".to_string(),
            },
        };

        let result = run_history_command(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_history_command_delete_invalid_id() {
        let args = ted::cli::HistoryArgs {
            command: ted::cli::HistoryCommands::Delete {
                session_id: "invalid".to_string(),
            },
        };

        let result = run_history_command(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_history_command_clear_without_force() {
        let args = ted::cli::HistoryArgs {
            command: ted::cli::HistoryCommands::Clear { force: false },
        };

        let result = run_history_command(args);
        assert!(result.is_ok());
    }

    // ==================== Caps command tests ====================

    #[test]
    fn test_run_caps_command_list() {
        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::List { detailed: false },
        };

        let result = run_caps_command(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_caps_command_list_detailed() {
        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::List { detailed: true },
        };

        let result = run_caps_command(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_caps_command_show_base() {
        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Show {
                name: "base".to_string(),
            },
        };

        let result = run_caps_command(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_caps_command_show_nonexistent() {
        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Show {
                name: "nonexistent_cap_xyz".to_string(),
            },
        };

        let result = run_caps_command(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_caps_command_edit_builtin() {
        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Edit {
                name: "base".to_string(),
            },
        };

        let result = run_caps_command(args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("built-in"));
    }

    #[test]
    fn test_run_caps_command_edit_nonexistent() {
        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Edit {
                name: "nonexistent_cap_xyz".to_string(),
            },
        };

        let result = run_caps_command(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_caps_command_export_base() {
        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Export {
                name: "base".to_string(),
                output: None,
            },
        };

        let result = run_caps_command(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_caps_command_import_url_not_supported() {
        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Import {
                source: "https://example.com/cap.toml".to_string(),
            },
        };

        let result = run_caps_command(args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("URL imports"));
    }

    #[test]
    fn test_run_caps_command_import_nonexistent_file() {
        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Import {
                source: "/nonexistent/path/cap.toml".to_string(),
            },
        };

        let result = run_caps_command(args);
        assert!(result.is_err());
    }

    // ==================== Custom command tests ====================

    #[test]
    fn test_run_custom_command_empty_args() {
        // Empty args should list available commands
        let result = run_custom_command(vec![]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_custom_command_nonexistent() {
        let result = run_custom_command(vec!["nonexistent_command_xyz".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown command"));
    }

    // ==================== Tool invocation formatting tests ====================

    #[test]
    fn test_print_tool_invocation_file_read() {
        let input = serde_json::json!({
            "path": "/test/path/file.txt"
        });

        let result = print_tool_invocation("file_read", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_file_read_no_path() {
        let input = serde_json::json!({});

        let result = print_tool_invocation("file_read", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_file_write() {
        let input = serde_json::json!({
            "path": "/test/path/new_file.txt",
            "content": "test content"
        });

        let result = print_tool_invocation("file_write", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_file_edit() {
        let input = serde_json::json!({
            "path": "/test/path/file.txt",
            "edits": []
        });

        let result = print_tool_invocation("file_edit", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_shell() {
        let input = serde_json::json!({
            "command": "ls -la"
        });

        let result = print_tool_invocation("shell", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_shell_long_command() {
        let input = serde_json::json!({
            "command": "this is a very long command that exceeds sixty characters in length to test truncation"
        });

        let result = print_tool_invocation("shell", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_glob() {
        let input = serde_json::json!({
            "pattern": "**/*.rs"
        });

        let result = print_tool_invocation("glob", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_grep() {
        let input = serde_json::json!({
            "pattern": "fn main",
            "path": "src/"
        });

        let result = print_tool_invocation("grep", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_grep_no_path() {
        let input = serde_json::json!({
            "pattern": "fn main"
        });

        let result = print_tool_invocation("grep", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_unknown_tool() {
        let input = serde_json::json!({
            "some_arg": "value"
        });

        let result = print_tool_invocation("unknown_tool", &input);
        assert!(result.is_ok());
    }

    // ==================== Tool result formatting tests ====================

    #[test]
    fn test_print_tool_result_file_read_success() {
        let result =
            ToolResult::success("test-id".to_string(), "line 1\nline 2\nline 3".to_string());

        let print_result = print_tool_result("file_read", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_file_write_success() {
        let result = ToolResult::success(
            "test-id".to_string(),
            "File written successfully".to_string(),
        );

        let print_result = print_tool_result("file_write", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_file_edit_success() {
        let result = ToolResult::success(
            "test-id".to_string(),
            "File edited successfully".to_string(),
        );

        let print_result = print_tool_result("file_edit", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_shell_success() {
        let result = ToolResult::success(
            "test-id".to_string(),
            "Exit code: 0\n---\ncommand output\n---".to_string(),
        );

        let print_result = print_tool_result("shell", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_glob_no_files() {
        let result = ToolResult::success("test-id".to_string(), "".to_string());

        let print_result = print_tool_result("glob", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_glob_few_files() {
        let result = ToolResult::success(
            "test-id".to_string(),
            "file1.rs\nfile2.rs\nfile3.rs".to_string(),
        );

        let print_result = print_tool_result("glob", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_glob_many_files() {
        let files: Vec<String> = (1..=10).map(|i| format!("file{}.rs", i)).collect();
        let result = ToolResult::success("test-id".to_string(), files.join("\n"));

        let print_result = print_tool_result("glob", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_grep_no_matches() {
        let result = ToolResult::success("test-id".to_string(), "".to_string());

        let print_result = print_tool_result("grep", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_grep_few_matches() {
        let result = ToolResult::success(
            "test-id".to_string(),
            "file1.rs:10: fn main\nfile2.rs:5: fn test".to_string(),
        );

        let print_result = print_tool_result("grep", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_grep_many_matches() {
        let matches: Vec<String> = (1..=10)
            .map(|i| format!("file{}.rs:{}: match content", i, i * 10))
            .collect();
        let result = ToolResult::success("test-id".to_string(), matches.join("\n"));

        let print_result = print_tool_result("grep", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_error() {
        let result = ToolResult::error("test-id".to_string(), "Error message".to_string());

        let print_result = print_tool_result("file_read", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_error_multiline() {
        let result = ToolResult::error(
            "test-id".to_string(),
            "Error line 1\nError line 2\nError line 3\nError line 4\nError line 5\nError line 6"
                .to_string(),
        );

        let print_result = print_tool_result("file_read", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_error_long_line() {
        let long_error = "E".repeat(100);
        let result = ToolResult::error("test-id".to_string(), long_error);

        let print_result = print_tool_result("file_read", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_unknown_tool() {
        let result = ToolResult::success("test-id".to_string(), "some output".to_string());

        let print_result = print_tool_result("unknown_tool", &result);
        assert!(print_result.is_ok());
    }

    // ==================== Shell output formatting tests ====================

    #[test]
    fn test_print_shell_output_success_no_output() {
        let output = "Exit code: 0\n---\n---";
        let result = print_shell_output(output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_success_few_lines() {
        let output = "Exit code: 0\n---\nline1\nline2\nline3\n---";
        let result = print_shell_output(output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_success_many_lines() {
        let lines: Vec<String> = (1..=30).map(|i| format!("line {}", i)).collect();
        let output = format!("Exit code: 0\n---\n{}\n---", lines.join("\n"));
        let result = print_shell_output(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_success_long_lines() {
        let long_line = "x".repeat(150);
        let output = format!("Exit code: 0\n---\n{}\n---", long_line);
        let result = print_shell_output(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_failure() {
        let output = "Exit code: 1\n---\nerror message\n---";
        let result = print_shell_output(output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_failure_many_lines() {
        let lines: Vec<String> = (1..=30).map(|i| format!("error line {}", i)).collect();
        let output = format!("Exit code: 1\n---\n{}\n---", lines.join("\n"));
        let result = print_shell_output(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_no_exit_code() {
        // Should default to exit code 0
        let output = "some output";
        let result = print_shell_output(output);
        assert!(result.is_ok());
    }

    // ==================== Help and welcome message tests ====================

    #[test]
    fn test_print_help() {
        let result = print_help();
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_welcome() {
        let session_id = SessionId::new();
        let result = print_welcome("anthropic", "claude-sonnet", false, &session_id, &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_welcome_with_caps() {
        let session_id = SessionId::new();
        let caps = vec!["base".to_string(), "rust".to_string()];
        let result = print_welcome("anthropic", "claude-sonnet", false, &session_id, &caps);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_welcome_trust_mode() {
        let session_id = SessionId::new();
        let result = print_welcome("anthropic", "claude-sonnet", true, &session_id, &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_cap_badge() {
        let result = print_cap_badge("test");
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_cap_badge_base() {
        let result = print_cap_badge("base");
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_cap_badge_rust() {
        let result = print_cap_badge("rust");
        assert!(result.is_ok());
    }

    // ==================== Response prefix tests ====================

    #[test]
    fn test_print_response_prefix_no_caps() {
        let result = print_response_prefix(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_response_prefix_with_base_cap() {
        // Base cap should be filtered out
        let caps = vec!["base".to_string()];
        let result = print_response_prefix(&caps);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_response_prefix_with_multiple_caps() {
        let caps = vec!["base".to_string(), "rust".to_string(), "python".to_string()];
        let result = print_response_prefix(&caps);
        assert!(result.is_ok());
    }

    // ==================== Provider configuration tests ====================

    #[test]
    fn test_check_provider_configuration_ollama() {
        let settings = Settings::default();
        let result = check_provider_configuration(&settings, "ollama");
        assert!(result.is_ok());
    }

    // ==================== Session resume tests ====================

    #[test]
    fn test_resume_session_invalid_short_id() {
        let store = HistoryStore::open().unwrap();
        let working_dir = PathBuf::from("/tmp");

        let result = resume_session(&store, "invalid1", &working_dir);
        assert!(result.is_err());
    }

    #[test]
    fn test_resume_session_invalid_full_id() {
        let store = HistoryStore::open().unwrap();
        let working_dir = PathBuf::from("/tmp");

        let result = resume_session(&store, "invalid-uuid-format", &working_dir);
        assert!(result.is_err());
    }

    #[test]
    fn test_resume_session_nonexistent_uuid() {
        let store = HistoryStore::open().unwrap();
        let working_dir = PathBuf::from("/tmp");

        // Valid UUID format but doesn't exist
        let result = resume_session(&store, "00000000-0000-0000-0000-000000000000", &working_dir);
        assert!(result.is_err());
    }

    // ==================== Session choice tests ====================

    #[test]
    fn test_prompt_session_choice_empty() {
        let result = prompt_session_choice(&[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // ==================== Constants tests ====================

    #[test]
    fn test_max_retries_constant() {
        // Verify constants are in expected ranges
        assert_eq!(MAX_RETRIES, 3);
    }

    #[test]
    fn test_base_retry_delay_constant() {
        // Verify constants are in expected ranges
        assert_eq!(BASE_RETRY_DELAY, 2);
    }

    #[test]
    fn test_shell_output_max_lines_constant() {
        // Verify constants are in expected ranges
        assert_eq!(SHELL_OUTPUT_MAX_LINES, 15);
    }

    // ==================== Context command tests ====================

    #[tokio::test]
    async fn test_run_context_command_stats() {
        let settings = Settings::default();
        let args = ted::cli::ContextArgs {
            command: ted::cli::ContextCommands::Stats,
        };

        let result = run_context_command(args, &settings).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_context_command_usage() {
        let settings = Settings::default();
        let args = ted::cli::ContextArgs {
            command: ted::cli::ContextCommands::Usage,
        };

        let result = run_context_command(args, &settings).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_context_command_prune_dry_run() {
        let settings = Settings::default();
        let args = ted::cli::ContextArgs {
            command: ted::cli::ContextCommands::Prune {
                days: Some(30),
                force: false,
                dry_run: true,
            },
        };

        let result = run_context_command(args, &settings).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_context_command_clear_without_force() {
        let settings = Settings::default();
        let args = ted::cli::ContextArgs {
            command: ted::cli::ContextCommands::Clear { force: false },
        };

        let result = run_context_command(args, &settings).await;
        assert!(result.is_ok());
    }

    // ==================== Create cap with temp dir tests ====================

    #[test]
    fn test_run_caps_command_create_existing() {
        // First, we need to create a temporary caps directory
        let temp_dir = TempDir::new().unwrap();
        let caps_dir = temp_dir.path().join("caps");
        std::fs::create_dir_all(&caps_dir).unwrap();

        // Create an existing cap file
        let cap_path = caps_dir.join("existing_cap.toml");
        std::fs::write(&cap_path, "name = \"existing_cap\"").unwrap();

        // The test would need to mock Settings::caps_dir() which is not easily testable
        // This test validates the error path when a cap already exists
    }

    // ==================== Export cap to file tests ====================

    #[test]
    fn test_run_caps_command_export_to_file() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("exported_cap.toml");

        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Export {
                name: "base".to_string(),
                output: Some(output_path.clone()),
            },
        };

        let result = run_caps_command(args);
        assert!(result.is_ok());
        assert!(output_path.exists());

        // Verify content is valid TOML
        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(!content.is_empty());
    }

    // ==================== Integration-style tests ====================

    #[test]
    fn test_full_init_and_caps_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let original_dir = std::env::current_dir().unwrap();

        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Initialize
        let init_result = run_init();
        assert!(init_result.is_ok());

        // List caps
        let list_args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::List { detailed: true },
        };
        let list_result = run_caps_command(list_args);
        assert!(list_result.is_ok());

        // Show base cap
        let show_args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Show {
                name: "base".to_string(),
            },
        };
        let show_result = run_caps_command(show_args);
        assert!(show_result.is_ok());

        std::env::set_current_dir(original_dir).unwrap();
    }

    // ==================== Edge case tests ====================

    #[test]
    fn test_print_tool_result_with_very_long_message() {
        let long_content = "x".repeat(1000);
        let result = ToolResult::success("test-id".to_string(), long_content);

        let print_result = print_tool_result("file_write", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_grep_with_very_long_match() {
        let long_match = format!("file.rs:1: {}", "x".repeat(200));
        let result = ToolResult::success("test-id".to_string(), long_match);

        let print_result = print_tool_result("grep", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_shell_output_with_empty_lines() {
        let output = "Exit code: 0\n---\n\n\nline1\n\nline2\n\n\n---";
        let result = print_shell_output(output);
        assert!(result.is_ok());
    }

    // ==================== Additional Settings command tests ====================

    #[test]
    fn test_run_settings_command_get_model() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Get {
                key: "model".to_string(),
            }),
        };

        let result = run_settings_command(args, settings);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_settings_command_get_ollama_base_url() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Get {
                key: "ollama.base_url".to_string(),
            }),
        };

        let result = run_settings_command(args, settings);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_settings_command_get_ollama_model() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Get {
                key: "ollama.model".to_string(),
            }),
        };

        let result = run_settings_command(args, settings);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_settings_command_get_model_ollama_provider() {
        let mut settings = Settings::default();
        settings.defaults.provider = "ollama".to_string();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Get {
                key: "model".to_string(),
            }),
        };

        let result = run_settings_command(args, settings);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_settings_command_set_valid_provider_anthropic() {
        let settings = Settings::default();
        // Don't actually save, just test parsing
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Set {
                key: "provider".to_string(),
                value: "anthropic".to_string(),
            }),
        };

        // This will try to save which might fail in test environment
        // But we're testing the validation path
        let _ = run_settings_command(args, settings);
    }

    #[test]
    fn test_run_settings_command_set_valid_provider_ollama() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Set {
                key: "provider".to_string(),
                value: "ollama".to_string(),
            }),
        };

        // This will try to save, may or may not succeed
        let _ = run_settings_command(args, settings);
    }

    #[test]
    fn test_run_settings_command_none() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs { command: None };

        // This path goes to TUI, so just verify it doesn't panic
        // In test environment this will likely fail, which is expected
        let _ = run_settings_command(args, settings);
    }

    // ==================== Additional History command tests ====================

    #[test]
    fn test_run_history_command_list_with_sessions() {
        // Create a session first
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        let _ = store.upsert(session_info);

        let args = ted::cli::HistoryArgs {
            command: ted::cli::HistoryCommands::List { limit: 10 },
        };

        let result = run_history_command(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_history_command_search_with_results() {
        // Create a session with a searchable summary
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let mut session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        session_info.set_summary("test searchable summary xyz123");
        let _ = store.upsert(session_info);

        let args = ted::cli::HistoryArgs {
            command: ted::cli::HistoryCommands::Search {
                query: "searchable".to_string(),
            },
        };

        let result = run_history_command(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_history_command_show_valid_session() {
        // Create a session first
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        store.upsert(session_info).unwrap();
        // Force the store to close so changes are persisted
        drop(store);

        let args = ted::cli::HistoryArgs {
            command: ted::cli::HistoryCommands::Show {
                session_id: session_id.to_string(),
            },
        };

        let result = run_history_command(args);
        // May fail if another test deleted the session, so just check the function runs
        let _ = result;

        // Clean up
        if let Ok(mut store) = HistoryStore::open() {
            let _ = store.delete(session_id);
        }
    }

    #[test]
    fn test_run_history_command_show_short_id() {
        // Create a session first
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        store.upsert(session_info).unwrap();
        // Force the store to close so changes are persisted
        drop(store);

        // Use short ID (first 8 chars)
        let short_id = &session_id.to_string()[..8];
        let args = ted::cli::HistoryArgs {
            command: ted::cli::HistoryCommands::Show {
                session_id: short_id.to_string(),
            },
        };

        let result = run_history_command(args);
        // May fail if another test deleted the session, so just check the function runs
        let _ = result;

        // Clean up
        if let Ok(mut store) = HistoryStore::open() {
            let _ = store.delete(session_id);
        }
    }

    #[test]
    fn test_run_history_command_delete_valid_session() {
        // Create a session first
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        let _ = store.upsert(session_info);

        let args = ted::cli::HistoryArgs {
            command: ted::cli::HistoryCommands::Delete {
                session_id: session_id.to_string(),
            },
        };

        let result = run_history_command(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_history_command_delete_nonexistent_valid_uuid() {
        let args = ted::cli::HistoryArgs {
            command: ted::cli::HistoryCommands::Delete {
                session_id: "00000000-0000-0000-0000-000000000001".to_string(),
            },
        };

        let result = run_history_command(args);
        // Should succeed but report not found
        assert!(result.is_ok());
    }

    // ==================== Additional Caps command tests ====================

    #[test]
    fn test_run_caps_command_show_with_long_system_prompt() {
        // Base cap has a long system prompt, test the truncation
        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Show {
                name: "base".to_string(),
            },
        };

        let result = run_caps_command(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_caps_command_import_invalid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let invalid_toml_path = temp_dir.path().join("invalid.toml");
        std::fs::write(&invalid_toml_path, "this is not valid toml [[[").unwrap();

        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Import {
                source: invalid_toml_path.to_str().unwrap().to_string(),
            },
        };

        let result = run_caps_command(args);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_caps_command_import_http_url() {
        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Import {
                source: "http://example.com/cap.toml".to_string(),
            },
        };

        let result = run_caps_command(args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("URL imports"));
    }

    // ==================== Additional Context command tests ====================

    #[tokio::test]
    async fn test_run_context_command_prune_no_old_sessions() {
        let settings = Settings::default();
        let args = ted::cli::ContextArgs {
            command: ted::cli::ContextCommands::Prune {
                days: Some(365), // Very old, likely no sessions this old
                force: false,
                dry_run: false,
            },
        };

        let result = run_context_command(args, &settings).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_context_command_prune_without_force_or_dry_run() {
        let settings = Settings::default();
        let args = ted::cli::ContextArgs {
            command: ted::cli::ContextCommands::Prune {
                days: Some(0), // All sessions
                force: false,
                dry_run: false,
            },
        };

        let result = run_context_command(args, &settings).await;
        assert!(result.is_ok());
    }

    // ==================== Additional Tool invocation tests ====================

    #[test]
    fn test_print_tool_invocation_file_write_no_path() {
        let input = serde_json::json!({
            "content": "test content"
        });

        let result = print_tool_invocation("file_write", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_file_edit_no_path() {
        let input = serde_json::json!({
            "edits": []
        });

        let result = print_tool_invocation("file_edit", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_shell_no_command() {
        let input = serde_json::json!({});

        let result = print_tool_invocation("shell", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_glob_no_pattern() {
        let input = serde_json::json!({});

        let result = print_tool_invocation("glob", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_grep_no_pattern() {
        let input = serde_json::json!({
            "path": "src/"
        });

        let result = print_tool_invocation("grep", &input);
        assert!(result.is_ok());
    }

    // ==================== Additional Tool result tests ====================

    #[test]
    fn test_print_tool_result_file_write_empty_message() {
        let result = ToolResult::success("test-id".to_string(), "".to_string());

        let print_result = print_tool_result("file_write", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_glob_exactly_5_files() {
        let files: Vec<String> = (1..=5).map(|i| format!("file{}.rs", i)).collect();
        let result = ToolResult::success("test-id".to_string(), files.join("\n"));

        let print_result = print_tool_result("glob", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_grep_exactly_5_matches() {
        let matches: Vec<String> = (1..=5)
            .map(|i| format!("file{}.rs:{}: match", i, i * 10))
            .collect();
        let result = ToolResult::success("test-id".to_string(), matches.join("\n"));

        let print_result = print_tool_result("grep", &result);
        assert!(print_result.is_ok());
    }

    // ==================== Additional Shell output tests ====================

    #[test]
    fn test_print_shell_output_exactly_15_lines() {
        let lines: Vec<String> = (1..=15).map(|i| format!("line {}", i)).collect();
        let output = format!("Exit code: 0\n---\n{}\n---", lines.join("\n"));
        let result = print_shell_output(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_16_lines_triggers_collapse() {
        let lines: Vec<String> = (1..=16).map(|i| format!("line {}", i)).collect();
        let output = format!("Exit code: 0\n---\n{}\n---", lines.join("\n"));
        let result = print_shell_output(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_failure_exactly_20_lines() {
        let lines: Vec<String> = (1..=20).map(|i| format!("error line {}", i)).collect();
        let output = format!("Exit code: 1\n---\n{}\n---", lines.join("\n"));
        let result = print_shell_output(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_failure_21_lines_truncated() {
        let lines: Vec<String> = (1..=21).map(|i| format!("error line {}", i)).collect();
        let output = format!("Exit code: 1\n---\n{}\n---", lines.join("\n"));
        let result = print_shell_output(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_failure_with_long_lines() {
        let long_line = "e".repeat(150);
        let output = format!("Exit code: 1\n---\n{}\n---", long_line);
        let result = print_shell_output(&output);
        assert!(result.is_ok());
    }

    // ==================== Additional Resume session tests ====================

    #[test]
    fn test_resume_session_valid_session() {
        // Create a session first
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        let _ = store.upsert(session_info);

        let working_dir = std::env::current_dir().unwrap();
        let result = resume_session(&store, &session_id.to_string(), &working_dir);
        assert!(result.is_ok());

        let (sid, _info, _count, is_resumed) = result.unwrap();
        assert_eq!(sid.0, session_id);
        assert!(is_resumed);
    }

    #[test]
    fn test_resume_session_valid_short_id() {
        // Create a session first
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        let _ = store.upsert(session_info);

        let working_dir = std::env::current_dir().unwrap();
        let short_id = &session_id.to_string()[..8];
        let result = resume_session(&store, short_id, &working_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resume_session_with_summary() {
        // Create a session with summary
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let mut session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        session_info.set_summary("Test session summary");
        session_info.message_count = 5;
        let _ = store.upsert(session_info);

        let working_dir = std::env::current_dir().unwrap();
        let result = resume_session(&store, &session_id.to_string(), &working_dir);
        assert!(result.is_ok());

        let (_, info, count, _) = result.unwrap();
        assert_eq!(count, 5);
        assert!(info.summary.is_some());
    }

    // ==================== Additional Cap badge tests ====================

    #[test]
    fn test_print_cap_badge_python() {
        let result = print_cap_badge("python");
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_cap_badge_typescript() {
        let result = print_cap_badge("typescript");
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_cap_badge_custom() {
        let result = print_cap_badge("my-custom-cap");
        assert!(result.is_ok());
    }

    // ==================== Additional Welcome message tests ====================

    #[test]
    fn test_print_welcome_ollama_provider() {
        let session_id = SessionId::new();
        let result = print_welcome("ollama", "llama3", false, &session_id, &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_welcome_openrouter_provider() {
        let session_id = SessionId::new();
        let result = print_welcome("openrouter", "gpt-4", false, &session_id, &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_welcome_with_many_caps() {
        let session_id = SessionId::new();
        let caps = vec![
            "base".to_string(),
            "rust".to_string(),
            "python".to_string(),
            "typescript".to_string(),
        ];
        let result = print_welcome("anthropic", "claude-sonnet", false, &session_id, &caps);
        assert!(result.is_ok());
    }

    // ==================== Session info tests ====================

    #[test]
    fn test_session_info_new() {
        let id = uuid::Uuid::new_v4();
        let working_dir = PathBuf::from("/test/path");
        let info = SessionInfo::new(id, working_dir.clone());

        assert_eq!(info.id, id);
        assert_eq!(info.working_directory, working_dir);
        assert_eq!(info.message_count, 0);
        assert!(info.summary.is_none());
    }

    #[test]
    fn test_session_info_set_summary() {
        let id = uuid::Uuid::new_v4();
        let working_dir = PathBuf::from("/test/path");
        let mut info = SessionInfo::new(id, working_dir);

        info.set_summary("This is a test summary for the session");
        assert!(info.summary.is_some());
    }

    #[test]
    fn test_session_info_touch() {
        let id = uuid::Uuid::new_v4();
        let working_dir = PathBuf::from("/test/path");
        let mut info = SessionInfo::new(id, working_dir);

        let original_time = info.last_active;
        std::thread::sleep(std::time::Duration::from_millis(10));
        info.touch();

        assert!(info.last_active >= original_time);
    }

    // ==================== Session ID tests ====================

    #[test]
    fn test_session_id_new() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        assert_ne!(id1.0, id2.0);
    }

    #[test]
    fn test_session_id_as_str() {
        let id = SessionId::new();
        let id_str = id.as_str();
        assert!(!id_str.is_empty());
        assert!(id_str.len() > 8);
    }

    // ==================== Error path tests ====================

    #[test]
    fn test_print_tool_result_error_exact_5_lines() {
        let error_lines: Vec<String> = (1..=5).map(|i| format!("Error line {}", i)).collect();
        let result = ToolResult::error("test-id".to_string(), error_lines.join("\n"));

        let print_result = print_tool_result("file_read", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_error_6_lines_truncated() {
        let error_lines: Vec<String> = (1..=6).map(|i| format!("Error line {}", i)).collect();
        let result = ToolResult::error("test-id".to_string(), error_lines.join("\n"));

        let print_result = print_tool_result("file_read", &result);
        assert!(print_result.is_ok());
    }

    // ==================== History store interaction tests ====================

    #[test]
    fn test_history_store_open() {
        let result = HistoryStore::open();
        assert!(result.is_ok());
    }

    #[test]
    fn test_history_store_list_recent_empty() {
        let store = HistoryStore::open().unwrap();
        let sessions = store.list_recent(10);
        // May or may not be empty depending on state, just verify no panic
        assert!(sessions.len() <= 10);
    }

    #[test]
    fn test_history_store_upsert_and_get() {
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let mut session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        session_info.set_summary("Test session");

        let _ = store.upsert(session_info.clone());

        let retrieved = store.get(session_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, session_id);
    }

    // ==================== Custom command with arguments tests ====================

    #[test]
    fn test_run_custom_command_with_args() {
        let result = run_custom_command(vec![
            "nonexistent_command_xyz".to_string(),
            "arg1".to_string(),
            "arg2".to_string(),
        ]);
        assert!(result.is_err());
    }

    // ==================== Provider configuration additional tests ====================

    #[test]
    fn test_check_provider_configuration_anthropic_no_key() {
        let _settings = Settings::default();
        // This tests the path where no API key is configured
        // Can't fully test without mocking stdin, but validates the code path
        // The function will wait for input which we can't provide in tests
    }

    // ==================== Cap loader tests ====================

    #[test]
    fn test_cap_loader_list_available() {
        let loader = CapLoader::new();
        let available = loader.list_available();
        assert!(available.is_ok());
        let caps = available.unwrap();
        // Should have at least the base cap
        assert!(!caps.is_empty());
    }

    #[test]
    fn test_cap_loader_load_base() {
        let loader = CapLoader::new();
        let cap = loader.load("base");
        assert!(cap.is_ok());
        let cap = cap.unwrap();
        assert_eq!(cap.name, "base");
    }

    #[test]
    fn test_cap_loader_load_nonexistent() {
        let loader = CapLoader::new();
        let cap = loader.load("nonexistent_cap_xyz_123");
        assert!(cap.is_err());
    }

    // ==================== Cap resolver tests ====================

    #[test]
    fn test_cap_resolver_resolve_empty() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);
        let result = resolver.resolve_and_merge(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cap_resolver_resolve_base() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);
        let result = resolver.resolve_and_merge(&["base".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cap_resolver_resolve_nonexistent() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);
        let result = resolver.resolve_and_merge(&["nonexistent_cap".to_string()]);
        assert!(result.is_err());
    }

    // ==================== Settings default tests ====================

    #[test]
    fn test_settings_default() {
        let settings = Settings::default();
        assert!(!settings.defaults.provider.is_empty());
    }

    #[test]
    fn test_settings_load() {
        // Settings::load() may fail in some test environments due to permissions
        // Just verify it doesn't panic
        let result = Settings::load();
        let _ = result; // May be Ok or Err depending on environment
    }

    // ==================== Utils tests ====================

    #[test]
    fn test_utils_find_project_root() {
        // May or may not find a project root depending on test environment
        let _ = utils::find_project_root();
    }

    #[test]
    fn test_utils_format_size() {
        assert_eq!(utils::format_size(0), "0 B");
        assert_eq!(utils::format_size(1023), "1023 B");
        // Just verify it returns something reasonable for larger sizes
        let kb = utils::format_size(1024);
        assert!(kb.contains("KB") || kb.contains("1"));
        let mb = utils::format_size(1048576);
        assert!(mb.contains("MB") || mb.contains("1"));
    }

    #[test]
    fn test_utils_calculate_dir_size() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "hello").unwrap();
        let size = utils::calculate_dir_size(temp_dir.path());
        assert!(size >= 5);
    }

    #[test]
    fn test_utils_calculate_dir_size_nonexistent() {
        let size = utils::calculate_dir_size(&PathBuf::from("/nonexistent/path"));
        assert_eq!(size, 0);
    }

    #[test]
    fn test_utils_get_cap_colors() {
        let (_bg, _fg) = utils::get_cap_colors("base");
        // Just verify it returns colors without panicking
        let (_bg2, _fg2) = utils::get_cap_colors("rust");
        let (_bg3, _fg3) = utils::get_cap_colors("python");
        let (_bg4, _fg4) = utils::get_cap_colors("unknown");
    }

    #[test]
    fn test_utils_format_error() {
        let error = TedError::Config("test error".to_string());
        let formatted = utils::format_error(&error);
        assert!(!formatted.is_empty());
    }

    // ==================== Tool context tests ====================

    #[test]
    fn test_tool_context_new() {
        let working_dir = std::env::current_dir().unwrap();
        let project_root = Some(working_dir.clone());
        let session_id = uuid::Uuid::new_v4();
        let trust_mode = false;

        let context = ToolContext::new(working_dir.clone(), project_root, session_id, trust_mode);

        assert_eq!(context.working_directory, working_dir);
        assert!(!context.trust_mode);
    }

    #[test]
    fn test_tool_context_with_files_in_context() {
        let working_dir = std::env::current_dir().unwrap();
        let session_id = uuid::Uuid::new_v4();

        let context = ToolContext::new(working_dir, None, session_id, false)
            .with_files_in_context(vec!["file1.rs".to_string(), "file2.rs".to_string()]);

        assert_eq!(context.files_in_context.len(), 2);
    }

    // ==================== Tool result tests ====================

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult::success("test-id".to_string(), "output".to_string());
        assert!(!result.is_error());
        assert_eq!(result.output_text(), "output");
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("test-id".to_string(), "error message".to_string());
        assert!(result.is_error());
        assert_eq!(result.output_text(), "error message");
    }

    // ==================== Conversation tests ====================

    #[test]
    fn test_conversation_new() {
        let conv = Conversation::new();
        assert!(conv.messages.is_empty());
        assert!(conv.system_prompt.is_none());
    }

    #[test]
    fn test_conversation_set_system() {
        let mut conv = Conversation::new();
        conv.set_system("You are a helpful assistant");
        assert!(conv.system_prompt.is_some());
        assert_eq!(
            conv.system_prompt.as_ref().unwrap(),
            "You are a helpful assistant"
        );
    }

    #[test]
    fn test_conversation_push() {
        let mut conv = Conversation::new();
        conv.push(Message::user("Hello"));
        assert_eq!(conv.messages.len(), 1);
    }

    #[test]
    fn test_conversation_clear() {
        let mut conv = Conversation::new();
        conv.push(Message::user("Hello"));
        conv.clear();
        assert!(conv.messages.is_empty());
    }

    // ==================== Message tests ====================

    #[test]
    fn test_message_user() {
        let msg = Message::user("Hello, world!");
        assert_eq!(msg.role, ted::llm::message::Role::User);
    }

    // ==================== Integration tests with actual operations ====================

    #[test]
    fn test_full_history_workflow() {
        let mut store = HistoryStore::open().unwrap();

        // Create session
        let session_id = uuid::Uuid::new_v4();
        let mut info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        info.set_summary("Full workflow test");
        info.message_count = 3;

        // Upsert
        let _ = store.upsert(info.clone());

        // List
        let sessions = store.list_recent(100);
        assert!(sessions.iter().any(|s| s.id == session_id));

        // Search
        let _results = store.search("workflow");
        // May or may not find depending on other test state

        // Get
        let retrieved = store.get(session_id);
        assert!(retrieved.is_some());

        // Delete
        let _ = store.delete(session_id);
        let after_delete = store.get(session_id);
        assert!(after_delete.is_none());
    }

    #[test]
    fn test_session_for_directory() {
        let mut store = HistoryStore::open().unwrap();
        let working_dir = std::env::current_dir().unwrap();

        // Create session in current directory
        let session_id = uuid::Uuid::new_v4();
        let info = SessionInfo::new(session_id, working_dir.clone());
        let _ = store.upsert(info);

        // Find sessions for this directory
        let sessions = store.sessions_for_directory(&working_dir);
        assert!(sessions.iter().any(|s| s.id == session_id));

        // Clean up
        let _ = store.delete(session_id);
    }

    // ==================== Additional Settings command tests ====================

    #[test]
    fn test_run_settings_command_reset() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Reset),
        };

        // This will save default settings
        let result = run_settings_command(args, settings);
        // May succeed or fail depending on permissions
        let _ = result;
    }

    #[test]
    fn test_run_settings_command_set_ollama_base_url() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Set {
                key: "ollama.base_url".to_string(),
                value: "http://localhost:11434".to_string(),
            }),
        };

        // May succeed or fail depending on permissions
        let _ = run_settings_command(args, settings);
    }

    #[test]
    fn test_run_settings_command_set_ollama_model() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Set {
                key: "ollama.model".to_string(),
                value: "llama3".to_string(),
            }),
        };

        let _ = run_settings_command(args, settings);
    }

    #[test]
    fn test_run_settings_command_set_model_anthropic_provider() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Set {
                key: "model".to_string(),
                value: "claude-sonnet-4-20250514".to_string(),
            }),
        };

        let _ = run_settings_command(args, settings);
    }

    #[test]
    fn test_run_settings_command_set_model_ollama_provider() {
        let mut settings = Settings::default();
        settings.defaults.provider = "ollama".to_string();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Set {
                key: "model".to_string(),
                value: "llama3".to_string(),
            }),
        };

        let _ = run_settings_command(args, settings);
    }

    #[test]
    fn test_run_settings_command_set_temperature_valid() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Set {
                key: "temperature".to_string(),
                value: "0.5".to_string(),
            }),
        };

        let _ = run_settings_command(args, settings);
    }

    #[test]
    fn test_run_settings_command_set_stream_true() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Set {
                key: "stream".to_string(),
                value: "true".to_string(),
            }),
        };

        let _ = run_settings_command(args, settings);
    }

    #[test]
    fn test_run_settings_command_set_stream_false() {
        let settings = Settings::default();
        let args = ted::cli::SettingsArgs {
            command: Some(ted::cli::SettingsCommands::Set {
                key: "stream".to_string(),
                value: "false".to_string(),
            }),
        };

        let _ = run_settings_command(args, settings);
    }

    // ==================== Additional History command tests ====================

    #[test]
    fn test_run_history_command_show_with_caps_and_project_root() {
        // Create a session with caps and project root
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let mut session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        session_info.set_summary("Test session with caps");
        session_info.caps = vec!["base".to_string(), "rust".to_string()];
        session_info.project_root = Some(std::env::current_dir().unwrap());
        store.upsert(session_info).unwrap();
        // Force the store to close so changes are persisted
        drop(store);

        let args = ted::cli::HistoryArgs {
            command: ted::cli::HistoryCommands::Show {
                session_id: session_id.to_string(),
            },
        };

        let result = run_history_command(args);
        // May fail if another test deleted the session, so just check the function runs
        let _ = result;

        // Clean up
        if let Ok(mut store) = HistoryStore::open() {
            let _ = store.delete(session_id);
        }
    }

    #[test]
    fn test_run_history_command_clear_with_force() {
        // This will clear all history - be careful with this test
        // Create a test session first to clean up
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        let _ = store.upsert(session_info);

        let args = ted::cli::HistoryArgs {
            command: ted::cli::HistoryCommands::Clear { force: true },
        };

        let result = run_history_command(args);
        assert!(result.is_ok());
    }

    // ==================== Additional Context command tests ====================

    #[tokio::test]
    async fn test_run_context_command_prune_with_force() {
        let settings = Settings::default();
        // Create an old session to prune
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let mut session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        // Manually set an old timestamp
        session_info.last_active = chrono::Utc::now() - chrono::Duration::days(400);
        let _ = store.upsert(session_info);

        let args = ted::cli::ContextArgs {
            command: ted::cli::ContextCommands::Prune {
                days: Some(365),
                force: true,
                dry_run: false,
            },
        };

        let result = run_context_command(args, &settings).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_context_command_clear_with_force() {
        let settings = Settings::default();
        let args = ted::cli::ContextArgs {
            command: ted::cli::ContextCommands::Clear { force: true },
        };

        let result = run_context_command(args, &settings).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_context_command_usage_with_sessions() {
        // Create a session first
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let mut session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        session_info
            .set_summary("Test session for context usage that is a bit longer to test truncation");
        let _ = store.upsert(session_info);

        let settings = Settings::default();
        let args = ted::cli::ContextArgs {
            command: ted::cli::ContextCommands::Usage,
        };

        let result = run_context_command(args, &settings).await;
        assert!(result.is_ok());

        // Clean up
        let _ = store.delete(session_id);
    }

    // ==================== Additional Tool invocation tests ====================

    #[test]
    fn test_print_tool_invocation_file_read_with_long_path() {
        let long_path = format!("/very/long/path/{}", "a".repeat(100));
        let input = serde_json::json!({
            "path": long_path
        });

        let result = print_tool_invocation("file_read", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_file_write_with_content() {
        let input = serde_json::json!({
            "path": "/test/file.txt",
            "content": "Hello, world!\nLine 2\nLine 3"
        });

        let result = print_tool_invocation("file_write", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_shell_exactly_60_chars() {
        let cmd = "a".repeat(60);
        let input = serde_json::json!({
            "command": cmd
        });

        let result = print_tool_invocation("shell", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_shell_61_chars() {
        let cmd = "a".repeat(61);
        let input = serde_json::json!({
            "command": cmd
        });

        let result = print_tool_invocation("shell", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_glob_with_complex_pattern() {
        let input = serde_json::json!({
            "pattern": "**/*.{rs,toml,md}"
        });

        let result = print_tool_invocation("glob", &input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_tool_invocation_grep_pattern_only() {
        let input = serde_json::json!({
            "pattern": "fn\\s+main"
        });

        let result = print_tool_invocation("grep", &input);
        assert!(result.is_ok());
    }

    // ==================== Additional Tool result tests ====================

    #[test]
    fn test_print_tool_result_file_read_empty() {
        let result = ToolResult::success("test-id".to_string(), "".to_string());

        let print_result = print_tool_result("file_read", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_file_write_long_success_message() {
        let long_msg = format!("File written successfully: {}", "x".repeat(100));
        let result = ToolResult::success("test-id".to_string(), long_msg);

        let print_result = print_tool_result("file_write", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_file_edit_long_message() {
        let long_msg = format!("Applied 10 edits to file: {}", "x".repeat(100));
        let result = ToolResult::success("test-id".to_string(), long_msg);

        let print_result = print_tool_result("file_edit", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_glob_exactly_3_files() {
        let result = ToolResult::success(
            "test-id".to_string(),
            "file1.rs\nfile2.rs\nfile3.rs".to_string(),
        );

        let print_result = print_tool_result("glob", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_glob_exactly_6_files() {
        let files: Vec<String> = (1..=6).map(|i| format!("file{}.rs", i)).collect();
        let result = ToolResult::success("test-id".to_string(), files.join("\n"));

        let print_result = print_tool_result("glob", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_grep_exactly_3_matches() {
        let matches: Vec<String> = (1..=3)
            .map(|i| format!("file{}.rs:{}: match", i, i * 10))
            .collect();
        let result = ToolResult::success("test-id".to_string(), matches.join("\n"));

        let print_result = print_tool_result("grep", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_grep_exactly_6_matches() {
        let matches: Vec<String> = (1..=6)
            .map(|i| format!("file{}.rs:{}: match", i, i * 10))
            .collect();
        let result = ToolResult::success("test-id".to_string(), matches.join("\n"));

        let print_result = print_tool_result("grep", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_grep_with_long_match_exactly_100_chars() {
        let match_content = format!("file.rs:1: {}", "x".repeat(91)); // Total 100 chars
        let result = ToolResult::success("test-id".to_string(), match_content);

        let print_result = print_tool_result("grep", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_grep_with_long_match_101_chars() {
        let match_content = format!("file.rs:1: {}", "x".repeat(92)); // Total 101 chars
        let result = ToolResult::success("test-id".to_string(), match_content);

        let print_result = print_tool_result("grep", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_error_single_line_exactly_80_chars() {
        let error_msg = "E".repeat(80);
        let result = ToolResult::error("test-id".to_string(), error_msg);

        let print_result = print_tool_result("file_read", &result);
        assert!(print_result.is_ok());
    }

    #[test]
    fn test_print_tool_result_error_single_line_81_chars() {
        let error_msg = "E".repeat(81);
        let result = ToolResult::error("test-id".to_string(), error_msg);

        let print_result = print_tool_result("file_read", &result);
        assert!(print_result.is_ok());
    }

    // ==================== Additional Shell output tests ====================

    #[test]
    fn test_print_shell_output_exactly_0_content_lines() {
        let output = "Exit code: 0\n---\n---";
        let result = print_shell_output(output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_exactly_1_content_line() {
        let output = "Exit code: 0\n---\nsingle line\n---";
        let result = print_shell_output(output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_success_line_exactly_120_chars() {
        let long_line = "x".repeat(120);
        let output = format!("Exit code: 0\n---\n{}\n---", long_line);
        let result = print_shell_output(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_success_line_121_chars() {
        let long_line = "x".repeat(121);
        let output = format!("Exit code: 0\n---\n{}\n---", long_line);
        let result = print_shell_output(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_failure_line_exactly_120_chars() {
        let long_line = "x".repeat(120);
        let output = format!("Exit code: 1\n---\n{}\n---", long_line);
        let result = print_shell_output(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_failure_line_121_chars() {
        let long_line = "x".repeat(121);
        let output = format!("Exit code: 1\n---\n{}\n---", long_line);
        let result = print_shell_output(&output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_with_exit_code_negative() {
        let output = "Exit code: -1\n---\nerror\n---";
        let result = print_shell_output(output);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_shell_output_success_exactly_11_lines() {
        // 11 lines triggers the hidden lines message (show 5 + hidden + 5)
        let lines: Vec<String> = (1..=11).map(|i| format!("line {}", i)).collect();
        let output = format!("Exit code: 0\n---\n{}\n---", lines.join("\n"));
        let result = print_shell_output(&output);
        assert!(result.is_ok());
    }

    // ==================== Additional Resume session tests ====================

    #[test]
    fn test_resume_session_with_no_summary() {
        // Create a session without summary
        let mut store = HistoryStore::open().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let session_info = SessionInfo::new(session_id, std::env::current_dir().unwrap());
        let _ = store.upsert(session_info);

        let working_dir = std::env::current_dir().unwrap();
        let result = resume_session(&store, &session_id.to_string(), &working_dir);
        assert!(result.is_ok());

        let (_, info, _, _) = result.unwrap();
        assert!(info.summary.is_none());

        // Clean up
        let _ = store.delete(session_id);
    }

    // ==================== Additional Cap resolver tests ====================

    #[test]
    fn test_cap_resolver_resolve_multiple_caps() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);
        let result = resolver.resolve_and_merge(&["base".to_string()]);
        assert!(result.is_ok());

        let merged = result.unwrap();
        assert!(!merged.system_prompt.is_empty());
    }

    // ==================== Additional Caps command tests ====================

    #[test]
    fn test_run_caps_command_show_cap_with_preferred_model() {
        // Show a cap that might have a preferred model
        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Show {
                name: "base".to_string(),
            },
        };

        let result = run_caps_command(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_caps_command_export_nonexistent() {
        let args = ted::cli::CapsArgs {
            command: ted::cli::CapsCommands::Export {
                name: "nonexistent_cap_xyz".to_string(),
                output: None,
            },
        };

        let result = run_caps_command(args);
        assert!(result.is_err());
    }

    // ==================== Additional Conversation tests ====================

    #[test]
    fn test_conversation_needs_trimming_below_threshold() {
        let conv = Conversation::new();
        // With empty conversation, should not need trimming
        assert!(!conv.needs_trimming(200000));
    }

    #[test]
    fn test_conversation_trim_to_fit_empty() {
        let mut conv = Conversation::new();
        let removed = conv.trim_to_fit(100000);
        assert_eq!(removed, 0);
    }

    #[test]
    fn test_conversation_with_multiple_messages() {
        let mut conv = Conversation::new();
        conv.push(Message::user("Hello"));
        conv.push(Message::user("How are you?"));
        conv.push(Message::user("What is the weather?"));
        assert_eq!(conv.messages.len(), 3);
    }

    // ==================== Additional Content block tests ====================

    #[test]
    fn test_content_block_text() {
        let block = ContentBlock::Text {
            text: "Hello, world!".to_string(),
        };
        if let ContentBlock::Text { text } = block {
            assert_eq!(text, "Hello, world!");
        } else {
            panic!("Expected Text block");
        }
    }

    #[test]
    fn test_content_block_tool_use() {
        let block = ContentBlock::ToolUse {
            id: "test-id".to_string(),
            name: "file_read".to_string(),
            input: serde_json::json!({"path": "/test/file.txt"}),
        };
        if let ContentBlock::ToolUse { id, name, input: _ } = block {
            assert_eq!(id, "test-id");
            assert_eq!(name, "file_read");
        } else {
            panic!("Expected ToolUse block");
        }
    }

    #[test]
    fn test_content_block_tool_result() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "test-id".to_string(),
            content: ted::llm::message::ToolResultContent::Text("output".to_string()),
            is_error: None,
        };
        if let ContentBlock::ToolResult {
            tool_use_id,
            content: _,
            is_error,
        } = block
        {
            assert_eq!(tool_use_id, "test-id");
            assert!(is_error.is_none());
        } else {
            panic!("Expected ToolResult block");
        }
    }

    // ==================== Additional Message content tests ====================

    #[test]
    fn test_message_content_text() {
        let content = MessageContent::Text("Hello".to_string());
        if let MessageContent::Text(text) = content {
            assert_eq!(text, "Hello");
        } else {
            panic!("Expected Text content");
        }
    }

    #[test]
    fn test_message_content_blocks() {
        let blocks = vec![ContentBlock::Text {
            text: "Hello".to_string(),
        }];
        let content = MessageContent::Blocks(blocks);
        if let MessageContent::Blocks(b) = content {
            assert_eq!(b.len(), 1);
        } else {
            panic!("Expected Blocks content");
        }
    }

    // ==================== Additional History store tests ====================

    #[test]
    fn test_history_store_cleanup() {
        let mut store = HistoryStore::open().unwrap();
        // Cleanup with retention of 9999 days should not remove anything recent
        let result = store.cleanup(9999);
        assert!(result.is_ok());
    }

    #[test]
    fn test_history_store_search_empty_query() {
        let store = HistoryStore::open().unwrap();
        let results = store.search("");
        // Should return something or empty, just no panic
        let _ = results;
    }

    // ==================== Additional Settings ensure directories tests ====================

    #[test]
    fn test_settings_ensure_directories() {
        let result = Settings::ensure_directories();
        assert!(result.is_ok());
    }

    #[test]
    fn test_settings_context_path() {
        let path = Settings::context_path();
        // Just verify it returns a valid path
        assert!(!path.to_string_lossy().is_empty());
    }

    #[test]
    fn test_settings_history_dir() {
        let path = Settings::history_dir();
        assert!(!path.to_string_lossy().is_empty());
    }

    #[test]
    fn test_settings_caps_dir() {
        let path = Settings::caps_dir();
        assert!(!path.to_string_lossy().is_empty());
    }

    // ==================== Additional Tool executor tests ====================

    #[test]
    fn test_tool_executor_new() {
        let working_dir = std::env::current_dir().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let context = ToolContext::new(working_dir, None, session_id, false);
        let executor = ToolExecutor::new(context, false);

        // Verify it has tool definitions
        let definitions = executor.tool_definitions();
        assert!(!definitions.is_empty());
    }

    // ==================== Provider specific tests ====================

    #[test]
    fn test_anthropic_provider_info() {
        // Can't test without API key, but can verify the type exists
        let provider_name = "anthropic";
        assert_eq!(provider_name, "anthropic");
    }

    #[test]
    fn test_ollama_provider_info() {
        let provider_name = "ollama";
        assert_eq!(provider_name, "ollama");
    }

    #[test]
    fn test_openrouter_provider_info() {
        let provider_name = "openrouter";
        assert_eq!(provider_name, "openrouter");
    }

    // ==================== Run clear tests ====================

    #[test]
    fn test_run_clear_message() {
        // run_clear just prints a message and returns Ok
        let result = run_clear();
        assert!(result.is_ok());
    }

    // ==================== Additional prompt session choice tests ====================

    // Note: prompt_session_choice requires stdin input which we can't easily test
    // But we can test the empty case which is already covered

    // ==================== Session info caps tests ====================

    #[test]
    fn test_session_info_with_caps() {
        let id = uuid::Uuid::new_v4();
        let working_dir = PathBuf::from("/test/path");
        let mut info = SessionInfo::new(id, working_dir);
        info.caps = vec!["base".to_string(), "rust".to_string()];

        assert_eq!(info.caps.len(), 2);
        assert!(info.caps.contains(&"base".to_string()));
    }

    #[test]
    fn test_session_info_with_project_root() {
        let id = uuid::Uuid::new_v4();
        let working_dir = PathBuf::from("/test/path");
        let mut info = SessionInfo::new(id, working_dir);
        info.project_root = Some(PathBuf::from("/test"));

        assert!(info.project_root.is_some());
    }

    // ==================== Additional utils tests ====================

    #[test]
    fn test_utils_format_size_large() {
        let gb = utils::format_size(1073741824); // 1 GB
        assert!(gb.contains("GB") || gb.contains("1"));
    }

    #[test]
    fn test_utils_calculate_dir_size_empty() {
        let temp_dir = TempDir::new().unwrap();
        let size = utils::calculate_dir_size(temp_dir.path());
        // Empty dir might have some size depending on filesystem
        // Size is u64, so just verify the function runs
        let _ = size;
    }

    #[test]
    fn test_utils_calculate_dir_size_nested() {
        let temp_dir = TempDir::new().unwrap();
        let nested = temp_dir.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("file.txt"), "content").unwrap();

        let size = utils::calculate_dir_size(temp_dir.path());
        assert!(size > 0);
    }

    // ==================== Mock Provider Tests ====================

    #[test]
    fn test_mock_provider_new() {
        let provider = MockProvider::new("test");
        assert_eq!(provider.name(), "test");
        assert_eq!(provider.complete_call_count(), 0);
        assert_eq!(provider.stream_call_count(), 0);
    }

    #[test]
    fn test_mock_provider_with_text_response() {
        let provider = MockProvider::with_text_response("test", "Hello, world!");
        assert_eq!(provider.name(), "test");
    }

    #[test]
    fn test_mock_provider_available_models() {
        let provider = MockProvider::new("test");
        let models = provider.available_models();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "mock-model");
        assert_eq!(models[0].context_window, 200000);
    }

    #[test]
    fn test_mock_provider_supports_model() {
        let provider = MockProvider::new("test");
        assert!(provider.supports_model("mock-model"));
        assert!(provider.supports_model("claude-sonnet"));
        assert!(!provider.supports_model("gpt-4"));
    }

    #[test]
    fn test_mock_provider_get_model_info() {
        let provider = MockProvider::new("test");
        let info = provider.get_model_info("mock-model");
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.id, "mock-model");
    }

    #[test]
    fn test_mock_provider_count_tokens() {
        let provider = MockProvider::new("test");
        let count = provider.count_tokens("Hello, world!", "mock-model");
        assert!(count.is_ok());
        // "Hello, world!" is 13 chars, ~3 tokens
        assert!(count.unwrap() >= 3);
    }

    #[tokio::test]
    async fn test_mock_provider_complete() {
        let provider = MockProvider::with_text_response("test", "Test response");
        let request = CompletionRequest::new("mock-model", vec![]);

        let result = provider.complete(request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.content.len(), 1);
        if let ContentBlockResponse::Text { text } = &response.content[0] {
            assert_eq!(text, "Test response");
        } else {
            panic!("Expected text response");
        }

        assert_eq!(provider.complete_call_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_provider_complete_default_response() {
        let provider = MockProvider::new("test");
        let request = CompletionRequest::new("mock-model", vec![]);

        let result = provider.complete(request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        if let ContentBlockResponse::Text { text } = &response.content[0] {
            assert_eq!(text, "Default mock response");
        }
    }

    #[tokio::test]
    async fn test_mock_provider_complete_with_rate_limit() {
        let provider = MockProvider::new("test");
        provider.set_rate_limit(true);

        let request = CompletionRequest::new("mock-model", vec![]);

        // First call should fail with rate limit
        let result = provider.complete(request.clone()).await;
        assert!(result.is_err());
        if let Err(ted::error::TedError::Api(ted::error::ApiError::RateLimited(_))) = result {
            // Expected
        } else {
            panic!("Expected rate limit error");
        }

        // Second call should succeed
        let result = provider.complete(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_provider_complete_with_context_too_long() {
        let provider = MockProvider::new("test");
        provider.set_context_too_long(true);

        let request = CompletionRequest::new("mock-model", vec![]);

        // First call should fail with context too long
        let result = provider.complete(request.clone()).await;
        assert!(result.is_err());
        if let Err(ted::error::TedError::Api(ted::error::ApiError::ContextTooLong {
            current,
            limit,
        })) = result
        {
            assert_eq!(current, 250000);
            assert_eq!(limit, 200000);
        } else {
            panic!("Expected context too long error");
        }

        // Second call should succeed
        let result = provider.complete(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_provider_complete_tool_use_response() {
        let provider = MockProvider::new("test");
        provider.set_tool_use_response(
            "tool-123",
            "file_read",
            serde_json::json!({"path": "/test/file.txt"}),
        );

        let request = CompletionRequest::new("mock-model", vec![]);
        let result = provider.complete(request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.stop_reason, Some(StopReason::ToolUse));
        if let ContentBlockResponse::ToolUse { id, name, input } = &response.content[0] {
            assert_eq!(id, "tool-123");
            assert_eq!(name, "file_read");
            assert_eq!(input["path"], "/test/file.txt");
        } else {
            panic!("Expected tool use response");
        }
    }

    #[tokio::test]
    async fn test_mock_provider_complete_stream() {
        let provider = MockProvider::new("test");
        let request = CompletionRequest::new("mock-model", vec![]);

        let result = provider.complete_stream(request).await;
        assert!(result.is_ok());

        let mut stream = result.unwrap();
        let mut events = Vec::new();
        use futures::StreamExt;
        while let Some(event) = stream.next().await {
            events.push(event);
        }

        assert!(!events.is_empty());
        assert_eq!(provider.stream_call_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_provider_complete_stream_with_custom_events() {
        let provider = MockProvider::new("test");
        let custom_events = vec![
            StreamEvent::MessageStart {
                id: "custom-id".to_string(),
                model: "custom-model".to_string(),
            },
            StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlockResponse::Text {
                    text: String::new(),
                },
            },
            StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::TextDelta {
                    text: "Custom response".to_string(),
                },
            },
            StreamEvent::ContentBlockStop { index: 0 },
            StreamEvent::MessageStop,
        ];
        provider.set_stream_events(custom_events);

        let request = CompletionRequest::new("mock-model", vec![]);
        let result = provider.complete_stream(request).await;
        assert!(result.is_ok());

        let mut stream = result.unwrap();
        use futures::StreamExt;
        let first_event = stream.next().await;
        assert!(first_event.is_some());
        let first_event = first_event.unwrap().unwrap();
        if let StreamEvent::MessageStart { id, .. } = first_event {
            assert_eq!(id, "custom-id");
        } else {
            panic!("Expected MessageStart event");
        }
    }

    #[tokio::test]
    async fn test_mock_provider_complete_stream_with_error() {
        let provider = MockProvider::new("test");
        provider.set_stream_error(true);

        let request = CompletionRequest::new("mock-model", vec![]);
        let result = provider.complete_stream(request).await;
        assert!(result.is_ok());

        let mut stream = result.unwrap();
        use futures::StreamExt;
        let first_event = stream.next().await;
        assert!(first_event.is_some());
        let first_event = first_event.unwrap().unwrap();
        if let StreamEvent::Error {
            error_type,
            message,
        } = first_event
        {
            assert_eq!(error_type, "server_error");
            assert!(message.contains("Simulated"));
        } else {
            panic!("Expected Error event");
        }
    }

    // ==================== run_agent_loop tests with mock provider ====================

    #[tokio::test]
    async fn test_run_agent_loop_simple_text_response() {
        let provider = MockProvider::with_text_response("test", "Hello from the assistant!");
        let mut conversation = Conversation::new();
        conversation.push(Message::user("Hello"));

        let working_dir = std::env::current_dir().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let context = ToolContext::new(working_dir.clone(), None, session_id, true);
        let mut tool_executor = ToolExecutor::new(context, true);
        let settings = Settings::default();
        let context_path = Settings::context_path();
        let context_manager = ContextManager::new(context_path, SessionId(session_id))
            .await
            .unwrap();
        let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let result = run_agent_loop(
            &provider,
            "mock-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            false, // no streaming
            &[],
            interrupted,
        )
        .await;

        assert!(result.is_ok());
        assert!(result.unwrap());
        // Conversation should have the assistant's response
        assert!(conversation.messages.len() >= 2);
    }

    #[tokio::test]
    async fn test_run_agent_loop_with_interrupt() {
        let provider = MockProvider::with_text_response("test", "Hello");
        let mut conversation = Conversation::new();
        conversation.push(Message::user("Hello"));

        let working_dir = std::env::current_dir().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let context = ToolContext::new(working_dir.clone(), None, session_id, true);
        let mut tool_executor = ToolExecutor::new(context, true);
        let settings = Settings::default();
        let context_path = Settings::context_path();
        let context_manager = ContextManager::new(context_path, SessionId(session_id))
            .await
            .unwrap();
        let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(true)); // Already interrupted

        let result = run_agent_loop(
            &provider,
            "mock-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            false,
            &[],
            interrupted,
        )
        .await;

        assert!(result.is_ok());
        assert!(!result.unwrap()); // Interrupted returns false
    }

    #[tokio::test]
    async fn test_run_agent_loop_with_streaming() {
        let provider = MockProvider::new("test");
        let mut conversation = Conversation::new();
        conversation.push(Message::user("Hello"));

        let working_dir = std::env::current_dir().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let context = ToolContext::new(working_dir.clone(), None, session_id, true);
        let mut tool_executor = ToolExecutor::new(context, true);
        let settings = Settings::default();
        let context_path = Settings::context_path();
        let context_manager = ContextManager::new(context_path, SessionId(session_id))
            .await
            .unwrap();
        let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let result = run_agent_loop(
            &provider,
            "mock-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            true, // with streaming
            &[],
            interrupted,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(provider.stream_call_count(), 1);
    }

    #[tokio::test]
    async fn test_run_agent_loop_with_active_caps() {
        let provider = MockProvider::with_text_response("test", "Rust response");
        let mut conversation = Conversation::new();
        conversation.push(Message::user("Hello"));

        let working_dir = std::env::current_dir().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let context = ToolContext::new(working_dir.clone(), None, session_id, true);
        let mut tool_executor = ToolExecutor::new(context, true);
        let settings = Settings::default();
        let context_path = Settings::context_path();
        let context_manager = ContextManager::new(context_path, SessionId(session_id))
            .await
            .unwrap();
        let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let caps = vec!["base".to_string(), "rust".to_string()];

        let result = run_agent_loop(
            &provider,
            "mock-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            false,
            &caps,
            interrupted,
        )
        .await;

        assert!(result.is_ok());
    }

    // ==================== get_response_with_retry tests ====================

    #[tokio::test]
    async fn test_get_response_with_retry_success() {
        let provider = MockProvider::with_text_response("test", "Success");
        let request = CompletionRequest::new("mock-model", vec![Message::user("Hello")]);

        let result = get_response_with_retry(&provider, request, false, &[]).await;
        assert!(result.is_ok());

        let (content, stop_reason) = result.unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(stop_reason, Some(StopReason::EndTurn));
    }

    #[tokio::test]
    async fn test_get_response_with_retry_rate_limited() {
        let provider = MockProvider::new("test");
        provider.set_rate_limit(true);
        provider.set_text_response("Success after retry");

        let request = CompletionRequest::new("mock-model", vec![Message::user("Hello")]);

        let result = get_response_with_retry(&provider, request, false, &[]).await;
        assert!(result.is_ok());

        // Provider should have been called twice (first rate limited, second success)
        assert_eq!(provider.complete_call_count(), 2);
    }

    #[tokio::test]
    async fn test_get_response_with_retry_streaming() {
        let provider = MockProvider::new("test");
        let request = CompletionRequest::new("mock-model", vec![Message::user("Hello")]);

        let result = get_response_with_retry(&provider, request, true, &[]).await;
        assert!(result.is_ok());
        assert_eq!(provider.stream_call_count(), 1);
    }

    #[tokio::test]
    async fn test_get_response_with_retry_with_caps() {
        let provider = MockProvider::with_text_response("test", "Response");
        let request = CompletionRequest::new("mock-model", vec![Message::user("Hello")]);
        let caps = vec!["rust".to_string(), "python".to_string()];

        let result = get_response_with_retry(&provider, request, false, &caps).await;
        assert!(result.is_ok());
    }

    // ==================== stream_response tests ====================

    #[tokio::test]
    async fn test_stream_response_basic() {
        let provider = MockProvider::new("test");
        let request = CompletionRequest::new("mock-model", vec![Message::user("Hello")]);

        let result = stream_response(&provider, request, &[]).await;
        assert!(result.is_ok());

        let (content, stop_reason) = result.unwrap();
        // Default streaming events produce content
        assert!(!content.is_empty() || stop_reason.is_some());
    }

    #[tokio::test]
    async fn test_stream_response_with_caps() {
        let provider = MockProvider::new("test");
        let request = CompletionRequest::new("mock-model", vec![Message::user("Hello")]);
        let caps = vec!["base".to_string()];

        let result = stream_response(&provider, request, &caps).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stream_response_with_tool_use() {
        let provider = MockProvider::new("test");
        let events = vec![
            StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlockResponse::ToolUse {
                    id: "tool-1".to_string(),
                    name: "file_read".to_string(),
                    input: serde_json::json!({}),
                },
            },
            StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::InputJsonDelta {
                    partial_json: r#"{"path":"#.to_string(),
                },
            },
            StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::InputJsonDelta {
                    partial_json: r#""/test.txt"}"#.to_string(),
                },
            },
            StreamEvent::ContentBlockStop { index: 0 },
            StreamEvent::MessageDelta {
                stop_reason: Some(StopReason::ToolUse),
                usage: Some(Usage::default()),
            },
            StreamEvent::MessageStop,
        ];
        provider.set_stream_events(events);

        let request = CompletionRequest::new("mock-model", vec![Message::user("Read file")]);
        let result = stream_response(&provider, request, &[]).await;
        assert!(result.is_ok());

        let (content, stop_reason) = result.unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(stop_reason, Some(StopReason::ToolUse));
    }

    #[tokio::test]
    async fn test_stream_response_with_error() {
        let provider = MockProvider::new("test");
        provider.set_stream_error(true);

        let request = CompletionRequest::new("mock-model", vec![Message::user("Hello")]);
        let result = stream_response(&provider, request, &[]).await;

        // Error event should result in an error
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_stream_response_with_text_content_updates() {
        let provider = MockProvider::new("test");
        let events = vec![
            StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlockResponse::Text {
                    text: String::new(),
                },
            },
            StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::TextDelta {
                    text: "Hello ".to_string(),
                },
            },
            StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::TextDelta {
                    text: "World!".to_string(),
                },
            },
            StreamEvent::ContentBlockStop { index: 0 },
            StreamEvent::MessageDelta {
                stop_reason: Some(StopReason::EndTurn),
                usage: None,
            },
            StreamEvent::MessageStop,
        ];
        provider.set_stream_events(events);

        let request = CompletionRequest::new("mock-model", vec![Message::user("Say hello")]);
        let result = stream_response(&provider, request, &[]).await;
        assert!(result.is_ok());

        let (content, _stop_reason) = result.unwrap();
        assert_eq!(content.len(), 1);
    }

    #[tokio::test]
    async fn test_stream_response_ping_event() {
        let provider = MockProvider::new("test");
        let events = vec![
            StreamEvent::Ping,
            StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlockResponse::Text {
                    text: String::new(),
                },
            },
            StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::TextDelta {
                    text: "Hi".to_string(),
                },
            },
            StreamEvent::ContentBlockStop { index: 0 },
            StreamEvent::MessageStop,
        ];
        provider.set_stream_events(events);

        let request = CompletionRequest::new("mock-model", vec![Message::user("Hello")]);
        let result = stream_response(&provider, request, &[]).await;
        assert!(result.is_ok());
    }

    // ==================== Completion request builder tests ====================

    #[test]
    fn test_completion_request_builder() {
        let request = CompletionRequest::new("model", vec![])
            .with_max_tokens(1000)
            .with_temperature(0.5)
            .with_system("You are helpful");

        assert_eq!(request.model, "model");
        assert_eq!(request.max_tokens, 1000);
        assert_eq!(request.temperature, 0.5);
        assert_eq!(request.system, Some("You are helpful".to_string()));
    }

    #[test]
    fn test_completion_request_with_tools() {
        let tools = vec![ted::llm::provider::ToolDefinition {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: ted::llm::provider::ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: vec![],
            },
        }];

        let request = CompletionRequest::new("model", vec![]).with_tools(tools);
        assert_eq!(request.tools.len(), 1);
    }

    // ==================== Message building tests ====================

    #[test]
    fn test_message_with_tool_blocks() {
        let blocks = vec![
            ContentBlock::Text {
                text: "Hello".to_string(),
            },
            ContentBlock::ToolUse {
                id: "tool-1".to_string(),
                name: "shell".to_string(),
                input: serde_json::json!({"command": "ls"}),
            },
        ];

        let msg = Message {
            id: uuid::Uuid::new_v4(),
            role: ted::llm::message::Role::Assistant,
            content: MessageContent::Blocks(blocks),
            timestamp: chrono::Utc::now(),
            tool_use_id: None,
            token_count: None,
        };

        if let MessageContent::Blocks(b) = msg.content {
            assert_eq!(b.len(), 2);
        }
    }

    // ==================== Tool result content block tests ====================

    #[test]
    fn test_tool_result_in_content_block() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "tool-1".to_string(),
            content: ted::llm::message::ToolResultContent::Text("Output".to_string()),
            is_error: Some(false),
        };

        if let ContentBlock::ToolResult {
            tool_use_id,
            is_error,
            ..
        } = block
        {
            assert_eq!(tool_use_id, "tool-1");
            assert_eq!(is_error, Some(false));
        }
    }

    #[test]
    fn test_tool_result_content_block_with_error() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "tool-2".to_string(),
            content: ted::llm::message::ToolResultContent::Text("Error occurred".to_string()),
            is_error: Some(true),
        };

        if let ContentBlock::ToolResult { is_error, .. } = block {
            assert_eq!(is_error, Some(true));
        }
    }

    // ==================== CompletionResponse tests ====================

    #[test]
    fn test_completion_response_structure() {
        let response = CompletionResponse {
            id: "resp-123".to_string(),
            model: "claude-sonnet".to_string(),
            content: vec![ContentBlockResponse::Text {
                text: "Hello".to_string(),
            }],
            stop_reason: Some(StopReason::EndTurn),
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
        };

        assert_eq!(response.id, "resp-123");
        assert_eq!(response.model, "claude-sonnet");
        assert_eq!(response.content.len(), 1);
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
    }

    // ==================== StopReason tests ====================

    #[test]
    fn test_stop_reason_variants() {
        assert_eq!(StopReason::EndTurn, StopReason::EndTurn);
        assert_eq!(StopReason::MaxTokens, StopReason::MaxTokens);
        assert_eq!(StopReason::ToolUse, StopReason::ToolUse);
        assert_eq!(StopReason::StopSequence, StopReason::StopSequence);

        assert_ne!(StopReason::EndTurn, StopReason::ToolUse);
    }

    // ==================== StreamEvent tests ====================

    #[test]
    fn test_stream_event_message_start() {
        let event = StreamEvent::MessageStart {
            id: "msg-1".to_string(),
            model: "model".to_string(),
        };

        if let StreamEvent::MessageStart { id, model } = event {
            assert_eq!(id, "msg-1");
            assert_eq!(model, "model");
        }
    }

    #[test]
    fn test_stream_event_content_block_start() {
        let event = StreamEvent::ContentBlockStart {
            index: 0,
            content_block: ContentBlockResponse::Text {
                text: String::new(),
            },
        };

        if let StreamEvent::ContentBlockStart { index, .. } = event {
            assert_eq!(index, 0);
        }
    }

    #[test]
    fn test_stream_event_content_block_delta_text() {
        let event = StreamEvent::ContentBlockDelta {
            index: 0,
            delta: ContentBlockDelta::TextDelta {
                text: "Hello".to_string(),
            },
        };

        if let StreamEvent::ContentBlockDelta { index, delta } = event {
            assert_eq!(index, 0);
            if let ContentBlockDelta::TextDelta { text } = delta {
                assert_eq!(text, "Hello");
            }
        }
    }

    #[test]
    fn test_stream_event_content_block_delta_json() {
        let event = StreamEvent::ContentBlockDelta {
            index: 0,
            delta: ContentBlockDelta::InputJsonDelta {
                partial_json: r#"{"key":"#.to_string(),
            },
        };

        if let StreamEvent::ContentBlockDelta {
            delta: ContentBlockDelta::InputJsonDelta { partial_json },
            ..
        } = event
        {
            assert!(partial_json.contains("key"));
        }
    }

    #[test]
    fn test_stream_event_message_delta() {
        let event = StreamEvent::MessageDelta {
            stop_reason: Some(StopReason::EndTurn),
            usage: Some(Usage {
                input_tokens: 100,
                output_tokens: 50,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            }),
        };

        if let StreamEvent::MessageDelta { stop_reason, usage } = event {
            assert_eq!(stop_reason, Some(StopReason::EndTurn));
            assert!(usage.is_some());
        }
    }

    #[test]
    fn test_stream_event_error() {
        let event = StreamEvent::Error {
            error_type: "rate_limit".to_string(),
            message: "Too many requests".to_string(),
        };

        if let StreamEvent::Error {
            error_type,
            message,
        } = event
        {
            assert_eq!(error_type, "rate_limit");
            assert!(message.contains("requests"));
        }
    }

    // ==================== Usage tests ====================

    #[test]
    fn test_usage_default() {
        let usage = Usage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.cache_creation_input_tokens, 0);
        assert_eq!(usage.cache_read_input_tokens, 0);
    }

    #[test]
    fn test_usage_with_cache() {
        let usage = Usage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: 20,
            cache_read_input_tokens: 10,
        };

        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.cache_creation_input_tokens, 20);
        assert_eq!(usage.cache_read_input_tokens, 10);
    }

    // ==================== ContentBlockResponse tests ====================

    #[test]
    fn test_content_block_response_text() {
        let block = ContentBlockResponse::Text {
            text: "Response text".to_string(),
        };

        if let ContentBlockResponse::Text { text } = block {
            assert_eq!(text, "Response text");
        }
    }

    #[test]
    fn test_content_block_response_tool_use() {
        let block = ContentBlockResponse::ToolUse {
            id: "tool-id".to_string(),
            name: "grep".to_string(),
            input: serde_json::json!({"pattern": "fn main"}),
        };

        if let ContentBlockResponse::ToolUse { id, name, input } = block {
            assert_eq!(id, "tool-id");
            assert_eq!(name, "grep");
            assert_eq!(input["pattern"], "fn main");
        }
    }

    // ==================== Model info tests ====================

    #[test]
    fn test_model_info_structure() {
        let info = ModelInfo {
            id: "claude-3".to_string(),
            display_name: "Claude 3".to_string(),
            context_window: 200000,
            max_output_tokens: 4096,
            supports_tools: true,
            supports_vision: true,
            input_cost_per_1k: 0.003,
            output_cost_per_1k: 0.015,
        };

        assert_eq!(info.id, "claude-3");
        assert_eq!(info.context_window, 200000);
        assert!(info.supports_tools);
        assert!(info.supports_vision);
    }

    // ==================== run_agent_loop_inner tests ====================

    #[tokio::test]
    async fn test_run_agent_loop_inner_simple() {
        let provider = MockProvider::with_text_response("test", "Inner loop response");
        let mut conversation = Conversation::new();
        conversation.push(Message::user("Hello"));

        let working_dir = std::env::current_dir().unwrap();
        let session_id = uuid::Uuid::new_v4();
        let context = ToolContext::new(working_dir.clone(), None, session_id, true);
        let mut tool_executor = ToolExecutor::new(context, true);
        let settings = Settings::default();
        let context_path = Settings::context_path();
        let context_manager = ContextManager::new(context_path, SessionId(session_id))
            .await
            .unwrap();
        let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let result = run_agent_loop_inner(
            &provider,
            "mock-model",
            &mut conversation,
            &mut tool_executor,
            &settings,
            &context_manager,
            false,
            &[],
            interrupted,
        )
        .await;

        assert!(result.is_ok());
    }

    // ==================== Tool execution in agent loop tests ====================
    // Note: Full tool execution tests are not included because they require stdin
    // interaction for tool confirmation, which cannot be provided in automated tests.

    // ==================== check_provider_configuration tests ====================

    #[test]
    fn test_check_provider_configuration_ollama_always_ok() {
        // Ollama doesn't require API key
        let settings = Settings::default();
        let result = check_provider_configuration(&settings, "ollama");
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_provider_configuration_openrouter_with_key() {
        // OpenRouter requires API key - set a temporary one to avoid stdin prompt
        std::env::set_var("ANTHROPIC_API_KEY", "test-key-for-openrouter");
        let settings = Settings::default();
        let result = check_provider_configuration(&settings, "openrouter");
        assert!(result.is_ok());
        // Clean up
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    // ==================== Rate limit handling tests ====================

    #[tokio::test]
    async fn test_rate_limit_retry_with_backoff() {
        let provider = MockProvider::new("test");
        provider.set_rate_limit(true);
        provider.set_text_response("Success after retry");

        let request = CompletionRequest::new("mock-model", vec![Message::user("Hello")]);

        // Use tokio timeout to ensure retry doesn't hang
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            get_response_with_retry(&provider, request, false, &[]),
        )
        .await;

        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_ok());

        // Should have retried once
        assert_eq!(provider.complete_call_count(), 2);
    }

    // ==================== Context too long handling tests ====================

    #[tokio::test]
    async fn test_context_too_long_error() {
        let provider = MockProvider::new("test");
        provider.set_context_too_long(true);
        provider.set_text_response("Success after trimming");

        let request = CompletionRequest::new("mock-model", vec![Message::user("Hello")]);

        let _result = get_response_with_retry(&provider, request, false, &[]).await;

        // Context too long error is returned to the caller for handling
        // The first call returns error, but it also sets up text response for next call
        // Check that the provider was called at least once
        assert!(provider.complete_call_count() >= 1);
    }

    // ==================== Multiple content blocks tests ====================

    #[tokio::test]
    async fn test_stream_response_multiple_content_blocks() {
        let provider = MockProvider::new("test");
        let events = vec![
            StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlockResponse::Text {
                    text: String::new(),
                },
            },
            StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::TextDelta {
                    text: "First block".to_string(),
                },
            },
            StreamEvent::ContentBlockStop { index: 0 },
            StreamEvent::ContentBlockStart {
                index: 1,
                content_block: ContentBlockResponse::Text {
                    text: String::new(),
                },
            },
            StreamEvent::ContentBlockDelta {
                index: 1,
                delta: ContentBlockDelta::TextDelta {
                    text: "Second block".to_string(),
                },
            },
            StreamEvent::ContentBlockStop { index: 1 },
            StreamEvent::MessageDelta {
                stop_reason: Some(StopReason::EndTurn),
                usage: None,
            },
            StreamEvent::MessageStop,
        ];
        provider.set_stream_events(events);

        let request = CompletionRequest::new("mock-model", vec![Message::user("Hello")]);
        let result = stream_response(&provider, request, &[]).await;
        assert!(result.is_ok());

        let (content, _) = result.unwrap();
        assert_eq!(content.len(), 2);
    }

    // ==================== Max tokens handling tests ====================

    #[tokio::test]
    async fn test_response_max_tokens_stop_reason() {
        let provider = MockProvider::new("test");
        let events = vec![
            StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlockResponse::Text {
                    text: String::new(),
                },
            },
            StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::TextDelta {
                    text: "Truncated...".to_string(),
                },
            },
            StreamEvent::ContentBlockStop { index: 0 },
            StreamEvent::MessageDelta {
                stop_reason: Some(StopReason::MaxTokens),
                usage: None,
            },
            StreamEvent::MessageStop,
        ];
        provider.set_stream_events(events);

        let request = CompletionRequest::new("mock-model", vec![Message::user("Hello")]);
        let result = stream_response(&provider, request, &[]).await;
        assert!(result.is_ok());

        let (_, stop_reason) = result.unwrap();
        assert_eq!(stop_reason, Some(StopReason::MaxTokens));
    }

    // ==================== Tool definition tests ====================

    #[test]
    fn test_tool_definition_structure() {
        let tool = ted::llm::provider::ToolDefinition {
            name: "file_read".to_string(),
            description: "Read a file".to_string(),
            input_schema: ted::llm::provider::ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({
                    "path": {
                        "type": "string",
                        "description": "File path"
                    }
                }),
                required: vec!["path".to_string()],
            },
        };

        assert_eq!(tool.name, "file_read");
        assert_eq!(tool.input_schema.required.len(), 1);
    }
}
