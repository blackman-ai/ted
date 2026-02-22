// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::io::{self, Write};
use std::sync::Arc;

use crossterm::{
    style::{Color, ResetColor, SetForegroundColor},
    ExecutableCommand,
};

use ted::caps::resolver::MergedCap;
use ted::caps::{CapLoader, CapResolver};
use ted::chat::{ChatSession, SessionState};
use ted::cli::ChatArgs;
use ted::config::Settings;
use ted::context::{ContextManager, SessionId};
use ted::error::{Result, TedError};
use ted::hardware::{sample_thermal_status, SystemProfile, ThermalLevel};
use ted::history::{HistoryStore, SessionInfo};
use ted::llm::factory::ProviderFactory;
use ted::llm::message::Conversation;
use ted::llm::provider::LlmProvider;
use ted::llm::providers::AnthropicProvider;
use ted::skills::SkillRegistry;
use ted::tools::ToolExecutor;

use super::chat_ui::{prompt_session_choice, resume_session};
use super::run_settings_tui;

pub(super) struct ChatRuntimeSetup {
    pub(super) settings: Settings,
    pub(super) provider: Arc<dyn LlmProvider>,
    pub(super) provider_name: String,
    pub(super) model: String,
    pub(super) cap_names: Vec<String>,
    pub(super) merged_cap: MergedCap,
    pub(super) loader: CapLoader,
    pub(super) resolver: CapResolver,
    pub(super) conversation: Conversation,
    pub(super) context_manager: ContextManager,
    pub(super) tool_executor: ToolExecutor,
    pub(super) history_store: HistoryStore,
    pub(super) skill_registry: Arc<SkillRegistry>,
    pub(super) session_id: SessionId,
    pub(super) session_info: SessionInfo,
    pub(super) message_count: usize,
    pub(super) working_directory: std::path::PathBuf,
    pub(super) project_root: Option<std::path::PathBuf>,
    pub(super) rate_coordinator: Option<Arc<ted::llm::TokenRateCoordinator>>,
    pub(super) _compaction_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Check if provider is configured and prompt user to set up if not.
pub(super) fn check_provider_configuration(
    settings: &Settings,
    provider_name: &str,
) -> Result<Settings> {
    // If the requested provider is already configured, proceed
    if settings.is_provider_configured(provider_name) {
        return Ok(settings.clone());
    }

    // API-based providers should fail fast with actionable setup guidance
    if provider_name == "openrouter" {
        return Err(TedError::Config(
            "No OpenRouter API key found. Set OPENROUTER_API_KEY or run 'ted settings'."
                .to_string(),
        ));
    }

    if provider_name == "blackman" {
        return Err(TedError::Config(
            "No Blackman API key found. Set BLACKMAN_API_KEY or run 'ted settings'.".to_string(),
        ));
    }

    // No provider configured - prompt the user
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
    println!(" Use local models (runs on your machine)");
    println!("     Place a GGUF at: ~/.ted/models/local/model.gguf");
    println!();
    print!("Choose an option [1/2], or 's' for settings: ");
    stdout.flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice = input.trim().to_lowercase();

    let mut updated_settings = settings.clone();

    match choice.as_str() {
        "2" | "local" => {
            // Switch to Local
            updated_settings.defaults.provider = "local".to_string();
            updated_settings.save()?;
            println!();
            stdout.execute(SetForegroundColor(Color::Green))?;
            print!("Provider set to Local.");
            stdout.execute(ResetColor)?;
            println!();
            println!("Place a GGUF model at: ~/.ted/models/local/model.gguf");
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
            updated_settings.defaults.provider = "anthropic".to_string();
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
            if reloaded.is_provider_configured(&reloaded.defaults.provider) {
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

/// Apply runtime thermal guardrails on constrained hardware.
pub(super) fn apply_thermal_guardrails(settings: &mut Settings, verbose: u8) {
    let Ok(profile) = SystemProfile::detect() else {
        return;
    };

    if !profile.tier.monitor_thermal() && !profile.thermal_throttle_risk() {
        return;
    }

    let Some(status) = sample_thermal_status(&profile) else {
        if verbose > 0 {
            eprintln!(
                "[verbose] Thermal monitoring enabled for {:?}, but no telemetry source was found",
                profile.tier
            );
        }
        return;
    };

    if !status.needs_throttle() {
        if verbose > 0 {
            eprintln!(
                "[verbose] Thermal status: {:?} via {}",
                status.level, status.source
            );
        }
        return;
    }

    let previous_tokens = settings.defaults.max_tokens;
    let previous_warm_chunks = settings.context.max_warm_chunks;

    settings.defaults.max_tokens = settings.defaults.max_tokens.min(2048);
    settings.context.max_warm_chunks = settings.context.max_warm_chunks.min(8);
    settings.context.auto_compact = false;

    let temp_text = status
        .temperature_c
        .map(|temp| format!("{temp:.1}C"))
        .or_else(|| {
            status
                .cpu_speed_limit_percent
                .map(|pct| format!("CPU limit {pct}%"))
        })
        .unwrap_or_else(|| "unknown".to_string());

    let level_text = match status.level {
        ThermalLevel::Critical => "critical",
        ThermalLevel::Hot => "hot",
        ThermalLevel::Warm => "warm",
        ThermalLevel::Cool => "cool",
    };

    eprintln!(
        "Thermal guardrails active ({level_text}, {temp_text}, source: {}). Reduced max tokens {}->{} and warm chunks {}->{}; background compaction disabled.",
        status.source,
        previous_tokens,
        settings.defaults.max_tokens,
        previous_warm_chunks,
        settings.context.max_warm_chunks
    );
}

pub(super) async fn initialize_chat_runtime(
    args: &ChatArgs,
    mut settings: Settings,
    verbose: u8,
) -> Result<ChatRuntimeSetup> {
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

    // Override local model path if specified via CLI
    if let Some(ref model_path) = args.model_path {
        settings.providers.local.model_path = model_path.clone();
    }

    // Create the appropriate provider (mutable so it can be changed via /settings)
    let mut provider: Arc<dyn LlmProvider> = if provider_name == "local" {
        match ProviderFactory::create_local(&settings).await {
            Ok(p) => p,
            Err(e) => {
                // Local provider failed — try to fall back or re-run setup
                let mut stdout = io::stdout();
                stdout.execute(SetForegroundColor(Color::Yellow))?;
                print!("Warning:");
                stdout.execute(ResetColor)?;
                println!(" {}", e);
                println!();

                if let Some(api_key) = settings.get_anthropic_api_key() {
                    // Have an Anthropic key — auto-switch
                    println!("Falling back to Anthropic provider.");
                    println!();
                    provider_name = "anthropic".to_string();
                    settings.defaults.provider = "anthropic".to_string();
                    settings.save()?;
                    Arc::new(AnthropicProvider::new(api_key))
                } else {
                    // No other provider available — run setup flow
                    settings = check_provider_configuration(&Settings::default(), "none")?;
                    provider_name = settings.defaults.provider.clone();
                    ProviderFactory::create(&provider_name, &settings, false).await?
                }
            }
        }
    } else {
        ProviderFactory::create(&provider_name, &settings, false).await?
    };

    // Resolve requested caps (defaults unless overridden via CLI)
    let mut cap_names: Vec<String> = if args.cap.is_empty() {
        settings.defaults.caps.clone()
    } else {
        args.cap.clone()
    };

    // Resolve working directory and preselect session state.
    let working_directory = std::env::current_dir()?;
    let history_store_for_selection = HistoryStore::open()?;
    let (selected_session_id, selected_session_info, selected_message_count, selected_is_resumed) =
        if let Some(ref resume_id) = args.resume {
            resume_session(&history_store_for_selection, resume_id, &working_directory)?
        } else {
            let recent_sessions =
                history_store_for_selection.sessions_for_directory(&working_directory);
            let mut recent: Vec<_> = recent_sessions
                .into_iter()
                .filter(|s| {
                    let age = chrono::Utc::now() - s.last_active;
                    age.num_hours() < 24
                })
                .collect();
            recent.sort_by(|a, b| b.last_active.cmp(&a.last_active));

            if !recent.is_empty() {
                if let Some(session_to_resume) = prompt_session_choice(&recent)? {
                    let sid = SessionId(session_to_resume.id);
                    let info = session_to_resume.clone();
                    let count = info.message_count;
                    (sid, info, count, true)
                } else {
                    let sid = SessionId::new();
                    let info = SessionInfo::new(sid.0, working_directory.clone());
                    (sid, info, 0, false)
                }
            } else {
                let sid = SessionId::new();
                let info = SessionInfo::new(sid.0, working_directory.clone());
                (sid, info, 0, false)
            }
        };

    // Build a shared chat session runtime (conversation/context/tools/history).
    let mut session_builder = ChatSession::builder(settings.clone())
        .with_provider(provider.clone(), &provider_name)
        .with_caps(cap_names.clone())
        .with_trust(args.trust)
        .with_working_directory(working_directory.clone())
        .with_session_state(SessionState {
            session_id: selected_session_id,
            session_info: selected_session_info,
            message_count: selected_message_count,
            is_resumed: selected_is_resumed,
        })
        .with_files_in_context(args.files_in_context.clone())
        .with_verbose(verbose);

    if let Some(ref model_override) = args.model {
        session_builder = session_builder.with_model(model_override.clone());
    }

    let chat_session = session_builder.build().await?;

    provider = chat_session.provider;
    provider_name = chat_session.provider_name;
    let model = chat_session.model;
    cap_names = chat_session.cap_names;
    let mut merged_cap = chat_session.merged_cap;
    let loader = chat_session.cap_loader;
    let resolver = chat_session.cap_resolver;
    let mut conversation = chat_session.conversation;
    let context_manager = chat_session.context_manager;
    let mut tool_executor = chat_session.tool_executor;
    let history_store = chat_session.history_store;
    let skill_registry = chat_session.skill_registry;
    let session_id = chat_session.session_id;
    let mut session_info = chat_session.session_info;
    let message_count = chat_session.message_count;
    let is_resumed = chat_session.is_resumed;
    let project_root = session_info.project_root.clone();

    if verbose > 0 {
        eprintln!("[verbose] Provider: {}", provider_name);
        eprintln!("[verbose] Model: {}", model);
        eprintln!("[verbose] Caps loaded: {:?}", cap_names);
    }

    // Start background compaction (every 5 minutes), if enabled
    let compaction_handle = if settings.context.auto_compact {
        Some(context_manager.start_background_compaction(300))
    } else {
        if verbose > 0 {
            eprintln!("[verbose] Background compaction disabled by settings");
        }
        None
    };

    // Create rate coordinator if rate limits are enabled
    let rate_coordinator = if settings.rate_limits.enabled {
        let limit = settings.rate_limits.get_for_model(&model);
        if verbose > 0 {
            eprintln!(
                "[verbose] Rate limiting enabled: {} tokens/min for model {}",
                limit.tokens_per_minute, model
            );
        }
        Some(ted::llm::TokenRateCoordinator::new(limit.tokens_per_minute))
    } else {
        None
    };

    // Register spawn_agent with rate coordination when configured.
    // Base registration already happens in ChatSessionBuilder.
    if let Some(ref coordinator) = rate_coordinator {
        tool_executor
            .registry_mut()
            .register_spawn_agent_with_coordinator(
                provider.clone(),
                skill_registry.clone(),
                Arc::clone(coordinator),
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

    Ok(ChatRuntimeSetup {
        settings,
        provider,
        provider_name,
        model,
        cap_names,
        merged_cap,
        loader,
        resolver,
        conversation,
        context_manager,
        tool_executor,
        history_store,
        skill_registry,
        session_id,
        session_info,
        message_count,
        working_directory,
        project_root,
        rate_coordinator,
        _compaction_handle: compaction_handle,
    })
}
