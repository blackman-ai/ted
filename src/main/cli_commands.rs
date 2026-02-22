// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::io::{self, Write};
use std::sync::Arc;

use crossterm::{
    style::{Color, ResetColor, SetForegroundColor},
    ExecutableCommand,
};

use ted::caps::CapLoader;
use ted::chat;
use ted::cli::UpdateArgs;
use ted::config::Settings;
use ted::context::SessionId;
use ted::error::{Result, TedError};
use ted::history::HistoryStore;
use ted::llm::factory::ProviderFactory;
use ted::llm::message::Message;
use ted::llm::provider::{CompletionRequest, LlmProvider};
use ted::update;
use ted::utils;

pub(super) fn run_clear() -> Result<()> {
    let settings = Settings::load()?;
    let context_path = settings.context.storage_path;
    if context_path.exists() {
        if let Err(e) = std::fs::remove_dir_all(&context_path) {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                eprintln!(
                    "Warning: Could not clear context at '{}' (permission denied).",
                    context_path.display()
                );
                return Ok(());
            }
            return Err(e.into());
        }
    }

    if let Err(e) = std::fs::create_dir_all(&context_path) {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            eprintln!(
                "Warning: Could not initialize context directory '{}' (permission denied).",
                context_path.display()
            );
            return Ok(());
        }
        return Err(e.into());
    }

    println!("Context cleared.");
    Ok(())
}

/// Run settings TUI
pub(super) fn run_settings_tui() -> Result<()> {
    let settings = Settings::load()?;
    ted::tui::run_tui(settings)
}

/// Run settings subcommands
pub(super) fn run_settings_command(
    args: ted::cli::SettingsArgs,
    mut settings: Settings,
) -> Result<()> {
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
                        "local" => settings.providers.local.default_model = value,
                        "openrouter" => settings.providers.openrouter.default_model = value,
                        "blackman" => settings.providers.blackman.default_model = value,
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
                    let valid_providers = ["anthropic", "local", "openrouter", "blackman"];
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
                "local.port" => {
                    settings.providers.local.port = value
                        .parse()
                        .map_err(|_| TedError::InvalidInput("Invalid port value".to_string()))?;
                }
                "local.model" => {
                    settings.providers.local.default_model = value;
                }
                "local.model_path" => {
                    settings.providers.local.model_path = std::path::PathBuf::from(value);
                }
                "openrouter.model" => {
                    settings.providers.openrouter.default_model = value;
                }
                "blackman.model" => {
                    settings.providers.blackman.default_model = value;
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
                    "local" => settings.providers.local.default_model.clone(),
                    "openrouter" => settings.providers.openrouter.default_model.clone(),
                    "blackman" => settings.providers.blackman.default_model.clone(),
                    _ => settings.providers.anthropic.default_model.clone(),
                },
                "temperature" => settings.defaults.temperature.to_string(),
                "stream" => settings.defaults.stream.to_string(),
                "provider" => settings.defaults.provider.clone(),
                "local.port" => settings.providers.local.port.to_string(),
                "local.model" => settings.providers.local.default_model.clone(),
                "local.model_path" => settings.providers.local.model_path.display().to_string(),
                "openrouter.model" => settings.providers.openrouter.default_model.clone(),
                "blackman.model" => settings.providers.blackman.default_model.clone(),
                _ => {
                    return Err(TedError::InvalidInput(format!("Unknown setting: {}", key)));
                }
            };
            println!("{}", value);
        }
        Some(ted::cli::SettingsCommands::Reset) => {
            let default_settings = Settings::default();
            default_settings.save_clean()?;
            println!("Settings reset to defaults.");
        }
        None => {
            // This case is handled by run_settings_tui
        }
    }
    Ok(())
}

/// Run the update command
pub(super) async fn run_update_command(args: UpdateArgs) -> Result<()> {
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

/// Run single question mode
pub(super) async fn run_ask(
    args: ted::cli::AskArgs,
    settings: Settings,
    verbose: u8,
) -> Result<()> {
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

    // Create the provider using the same factory path as chat mode
    let provider: Arc<dyn LlmProvider> =
        ProviderFactory::create(&provider_name, &settings, false).await?;

    let model = args
        .model
        .unwrap_or_else(|| ProviderFactory::default_model(&provider_name, &settings));

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

    // Ask mode uses the shared streaming/retry path, but renders plain text only.
    struct AskStreamObserver;

    impl chat::AgentLoopObserver for AskStreamObserver {
        fn on_text_delta(&mut self, text: &str) -> Result<()> {
            print!("{}", text);
            io::stdout().flush()?;
            Ok(())
        }
    }

    let mut observer = AskStreamObserver;
    let _ =
        chat::engine::get_response_with_retry(provider.as_ref(), request, true, &[], &mut observer)
            .await?;

    // Keep ask-mode output behavior consistent: always terminate with a newline.
    println!();

    Ok(())
}

/// Initialize ted in current project
pub(super) fn run_init() -> Result<()> {
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
pub(super) fn print_welcome(
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
pub(super) fn print_cap_badge(cap_name: &str) -> Result<()> {
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
pub(super) fn print_help() -> Result<()> {
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
pub(super) fn read_user_input() -> Result<String> {
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
pub(super) fn print_response_prefix(active_caps: &[String]) -> Result<()> {
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
pub(super) fn run_caps_command(args: ted::cli::CapsArgs) -> Result<()> {
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
pub(super) fn run_custom_command(args: Vec<String>) -> Result<()> {
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
pub(super) fn run_history_command(args: ted::cli::HistoryArgs) -> Result<()> {
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
pub(super) async fn run_context_command(
    args: ted::cli::ContextArgs,
    settings: &Settings,
) -> Result<()> {
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
