// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! CLI argument definitions using Clap
//!
//! Defines all command-line arguments and subcommands for Ted.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Ted - AI coding assistant for your terminal
#[derive(Parser, Debug)]
#[command(name = "ted")]
#[command(version, about = "AI coding assistant for your terminal")]
#[command(propagate_version = true)]
pub struct Cli {
    /// Working directory (defaults to current)
    #[arg(short = 'C', long, global = true)]
    pub directory: Option<PathBuf>,

    /// Config file path
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Verbosity level (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Output format
    #[arg(long, global = true, default_value = "text")]
    pub format: OutputFormat,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available subcommands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start interactive chat session (default when no command given)
    Chat(ChatArgs),

    /// Ask a single question (non-interactive)
    Ask(AskArgs),

    /// Clear context and start fresh
    Clear,

    /// Caps management
    Caps(CapsArgs),

    /// History management
    History(HistoryArgs),

    /// Context/storage management
    Context(ContextArgs),

    /// Open settings TUI or manage configuration
    #[command(alias = "config")]
    Settings(SettingsArgs),

    /// Initialize ted in current project
    Init,

    /// Update ted to the latest version
    Update(UpdateArgs),

    /// Show system hardware information and recommendations
    #[command(alias = "hw")]
    System(SystemArgs),

    /// Start MCP server for external tool integrations
    Mcp(McpArgs),

    /// Start Language Server Protocol (LSP) server
    Lsp,

    /// Run a custom command from .ted/commands/
    #[command(external_subcommand)]
    Run(Vec<String>),
}

/// Arguments for the update subcommand
#[derive(clap::Args, Debug)]
pub struct UpdateArgs {
    /// Check for updates without installing
    #[arg(long)]
    pub check: bool,

    /// Update to a specific version (e.g., "v0.2.0")
    #[arg(long = "target")]
    pub target_version: Option<String>,

    /// Force update even if already on latest version
    #[arg(short, long)]
    pub force: bool,
}

/// Arguments for the system subcommand
#[derive(clap::Args, Debug)]
pub struct SystemArgs {
    /// Show upgrade suggestions
    #[arg(short, long)]
    pub upgrades: bool,

    /// Show detailed hardware information
    #[arg(short, long)]
    pub detailed: bool,
}

/// Arguments for the MCP subcommand
#[derive(clap::Args, Debug)]
pub struct McpArgs {
    /// Project directory to expose tools for
    #[arg(short, long)]
    pub project: Option<String>,
}

/// Arguments for the chat subcommand
#[derive(clap::Args, Debug, Default)]
pub struct ChatArgs {
    /// Initial prompt (optional)
    pub prompt: Option<String>,

    /// Caps to activate
    #[arg(short, long, num_args = 1..)]
    pub cap: Vec<String>,

    /// Model to use
    #[arg(short, long)]
    pub model: Option<String>,

    /// LLM provider to use (anthropic, local, openrouter, blackman)
    #[arg(short, long)]
    pub provider: Option<String>,

    /// Resume a previous session
    #[arg(long)]
    pub resume: Option<String>,

    /// Trust mode (auto-approve all tool uses)
    #[arg(long)]
    pub trust: bool,

    /// Disable streaming output
    #[arg(long)]
    pub no_stream: bool,

    /// Embedded mode (output JSONL events for GUI integration)
    #[arg(long, hide = true)]
    pub embedded: bool,

    /// Path to a GGUF model file for local provider
    #[arg(long, value_name = "PATH")]
    pub model_path: Option<PathBuf>,

    /// Disable TUI mode (use simple terminal output, useful for piping/scripting)
    #[arg(long)]
    pub no_tui: bool,

    /// Path to conversation history JSON file (for multi-turn embedded conversations)
    #[arg(long, hide = true)]
    pub history: Option<PathBuf>,

    /// Review mode - emit file events but don't execute file modifications (for GUI review before applying)
    #[arg(long, hide = true)]
    pub review_mode: bool,

    /// Project has existing files - used by enforcement logic to determine if editing is expected
    #[arg(long, hide = true)]
    pub project_has_files: bool,

    /// Path to a file containing additional system prompt text to append
    /// This allows frontends to inject custom guidance without modifying Ted's core
    #[arg(long, hide = true)]
    pub system_prompt_file: Option<PathBuf>,

    /// Comma-separated list of files already provided in the context
    /// When file_read is called for one of these, returns a short reminder instead
    #[arg(long, hide = true, value_delimiter = ',')]
    pub files_in_context: Vec<String>,
}

/// Arguments for the ask subcommand
#[derive(clap::Args, Debug)]
pub struct AskArgs {
    /// The question to ask
    pub prompt: String,

    /// Caps to use
    #[arg(short, long, num_args = 1..)]
    pub cap: Vec<String>,

    /// Model to use
    #[arg(short, long)]
    pub model: Option<String>,

    /// LLM provider to use (anthropic, local, openrouter, blackman)
    #[arg(short, long)]
    pub provider: Option<String>,

    /// Include file contents in context
    #[arg(short, long, num_args = 1..)]
    pub file: Vec<PathBuf>,

    /// Read prompt from stdin
    #[arg(long)]
    pub stdin: bool,
}

/// Arguments for caps management
#[derive(clap::Args, Debug)]
pub struct CapsArgs {
    #[command(subcommand)]
    pub command: CapsCommands,
}

/// Caps subcommands
#[derive(Subcommand, Debug)]
pub enum CapsCommands {
    /// List available caps
    List {
        /// Show detailed information
        #[arg(short = 'd', long)]
        detailed: bool,
    },

    /// Show cap details
    Show {
        /// Name of the cap to show
        name: String,
    },

    /// Create a new cap
    Create {
        /// Name for the new cap
        name: String,
    },

    /// Edit an existing cap
    Edit {
        /// Name of the cap to edit
        name: String,
    },

    /// Import cap from file or URL
    Import {
        /// Source file path or URL
        source: String,
    },

    /// Export cap to file
    Export {
        /// Name of the cap to export
        name: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

/// Arguments for history management
#[derive(clap::Args, Debug)]
pub struct HistoryArgs {
    #[command(subcommand)]
    pub command: HistoryCommands,
}

/// History subcommands
#[derive(Subcommand, Debug)]
pub enum HistoryCommands {
    /// List recent sessions
    List {
        /// Maximum number of sessions to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Search history
    Search {
        /// Search query
        query: String,
    },

    /// Show a specific session
    Show {
        /// Session ID
        session_id: String,
    },

    /// Delete a session
    Delete {
        /// Session ID
        session_id: String,
    },

    /// Clear all history
    Clear {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}

/// Arguments for context/storage management
#[derive(clap::Args, Debug)]
pub struct ContextArgs {
    #[command(subcommand)]
    pub command: ContextCommands,
}

/// Context subcommands
#[derive(Subcommand, Debug)]
pub enum ContextCommands {
    /// Show storage statistics
    Stats,

    /// Prune old sessions to free up space
    Prune {
        /// Days of history to keep (default: from settings, usually 30)
        #[arg(short, long)]
        days: Option<u32>,

        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,

        /// Dry run - show what would be deleted without deleting
        #[arg(long)]
        dry_run: bool,
    },

    /// Show disk usage breakdown
    Usage,

    /// Clear ALL context data (dangerous!)
    Clear {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
}

/// Arguments for settings/config
#[derive(clap::Args, Debug)]
pub struct SettingsArgs {
    #[command(subcommand)]
    pub command: Option<SettingsCommands>,
}

/// Settings subcommands
#[derive(Subcommand, Debug)]
pub enum SettingsCommands {
    /// Show current configuration
    Show,

    /// Set a configuration value
    Set {
        /// Configuration key (e.g., "model", "api_key")
        key: String,

        /// Value to set
        value: String,
    },

    /// Get a configuration value
    Get {
        /// Configuration key
        key: String,
    },

    /// Reset configuration to defaults
    Reset,
}

/// Output format for responses
#[derive(ValueEnum, Clone, Debug, Default, PartialEq)]
pub enum OutputFormat {
    /// Plain text output
    #[default]
    Text,

    /// JSON output
    Json,

    /// Markdown output
    Markdown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    // ==================== CLI Global Arguments ====================

    #[test]
    fn test_cli_default_no_command() {
        let cli = Cli::parse_from(["ted"]);
        assert!(cli.command.is_none());
        assert_eq!(cli.verbose, 0);
        assert!(matches!(cli.format, OutputFormat::Text));
    }

    #[test]
    fn test_cli_verbose_single() {
        let cli = Cli::parse_from(["ted", "-v"]);
        assert_eq!(cli.verbose, 1);
    }

    #[test]
    fn test_cli_verbose_multiple() {
        let cli = Cli::parse_from(["ted", "-vvv"]);
        assert_eq!(cli.verbose, 3);
    }

    #[test]
    fn test_cli_directory_short() {
        let cli = Cli::parse_from(["ted", "-C", "/some/path"]);
        assert_eq!(cli.directory, Some(PathBuf::from("/some/path")));
    }

    #[test]
    fn test_cli_directory_long() {
        let cli = Cli::parse_from(["ted", "--directory", "/other/path"]);
        assert_eq!(cli.directory, Some(PathBuf::from("/other/path")));
    }

    #[test]
    fn test_cli_config_path() {
        let cli = Cli::parse_from(["ted", "--config", "/path/to/config.toml"]);
        assert_eq!(cli.config, Some(PathBuf::from("/path/to/config.toml")));
    }

    #[test]
    fn test_cli_format_json() {
        let cli = Cli::parse_from(["ted", "--format", "json"]);
        assert_eq!(cli.format, OutputFormat::Json);
    }

    #[test]
    fn test_cli_format_markdown() {
        let cli = Cli::parse_from(["ted", "--format", "markdown"]);
        assert_eq!(cli.format, OutputFormat::Markdown);
    }

    // ==================== Chat Command ====================

    #[test]
    fn test_chat_command_basic() {
        let cli = Cli::parse_from(["ted", "chat"]);
        assert!(matches!(cli.command, Some(Commands::Chat(_))));
    }

    #[test]
    fn test_chat_with_prompt() {
        let cli = Cli::parse_from(["ted", "chat", "Hello, Ted!"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert_eq!(args.prompt, Some("Hello, Ted!".to_string()));
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_with_single_cap() {
        let cli = Cli::parse_from(["ted", "chat", "-c", "rust"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert_eq!(args.cap, vec!["rust"]);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_with_multiple_caps() {
        let cli = Cli::parse_from(["ted", "chat", "-c", "rust", "-c", "security"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert_eq!(args.cap, vec!["rust", "security"]);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_with_model() {
        let cli = Cli::parse_from(["ted", "chat", "-m", "claude-3-opus"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert_eq!(args.model, Some("claude-3-opus".to_string()));
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_with_provider() {
        let cli = Cli::parse_from(["ted", "chat", "-p", "local"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert_eq!(args.provider, Some("local".to_string()));
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_with_trust() {
        let cli = Cli::parse_from(["ted", "chat", "--trust"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert!(args.trust);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_with_no_stream() {
        let cli = Cli::parse_from(["ted", "chat", "--no-stream"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert!(args.no_stream);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_with_resume() {
        let cli = Cli::parse_from(["ted", "chat", "--resume", "session-123"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert_eq!(args.resume, Some("session-123".to_string()));
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_hidden_embedded() {
        let cli = Cli::parse_from(["ted", "chat", "--embedded"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert!(args.embedded);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_hidden_review_mode() {
        let cli = Cli::parse_from(["ted", "chat", "--review-mode"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert!(args.review_mode);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_hidden_history() {
        let cli = Cli::parse_from(["ted", "chat", "--history", "/path/to/history.json"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert_eq!(args.history, Some(PathBuf::from("/path/to/history.json")));
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_hidden_system_prompt_file() {
        let cli = Cli::parse_from(["ted", "chat", "--system-prompt-file", "/path/prompt.txt"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert_eq!(
                args.system_prompt_file,
                Some(PathBuf::from("/path/prompt.txt"))
            );
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_hidden_files_in_context() {
        let cli = Cli::parse_from(["ted", "chat", "--files-in-context", "a.rs,b.rs,c.rs"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert_eq!(args.files_in_context, vec!["a.rs", "b.rs", "c.rs"]);
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_hidden_project_has_files() {
        let cli = Cli::parse_from(["ted", "chat", "--project-has-files"]);
        if let Some(Commands::Chat(args)) = cli.command {
            assert!(args.project_has_files);
        } else {
            panic!("Expected Chat command");
        }
    }

    // ==================== Ask Command ====================

    #[test]
    fn test_ask_command_basic() {
        let cli = Cli::parse_from(["ted", "ask", "What is Rust?"]);
        if let Some(Commands::Ask(args)) = cli.command {
            assert_eq!(args.prompt, "What is Rust?");
        } else {
            panic!("Expected Ask command");
        }
    }

    #[test]
    fn test_ask_with_stdin() {
        let cli = Cli::parse_from(["ted", "ask", "--stdin", "placeholder"]);
        if let Some(Commands::Ask(args)) = cli.command {
            assert!(args.stdin);
        } else {
            panic!("Expected Ask command");
        }
    }

    #[test]
    fn test_ask_with_files() {
        let cli = Cli::parse_from(["ted", "ask", "Question?", "-f", "a.rs", "-f", "b.rs"]);
        if let Some(Commands::Ask(args)) = cli.command {
            assert_eq!(
                args.file,
                vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")]
            );
        } else {
            panic!("Expected Ask command");
        }
    }

    #[test]
    fn test_ask_with_caps() {
        let cli = Cli::parse_from(["ted", "ask", "Question?", "-c", "rust"]);
        if let Some(Commands::Ask(args)) = cli.command {
            assert_eq!(args.cap, vec!["rust"]);
        } else {
            panic!("Expected Ask command");
        }
    }

    // ==================== Clear Command ====================

    #[test]
    fn test_clear_command() {
        let cli = Cli::parse_from(["ted", "clear"]);
        assert!(matches!(cli.command, Some(Commands::Clear)));
    }

    // ==================== Caps Commands ====================

    #[test]
    fn test_caps_list() {
        let cli = Cli::parse_from(["ted", "caps", "list"]);
        if let Some(Commands::Caps(args)) = cli.command {
            assert!(matches!(
                args.command,
                CapsCommands::List { detailed: false }
            ));
        } else {
            panic!("Expected Caps command");
        }
    }

    #[test]
    fn test_caps_list_detailed() {
        let cli = Cli::parse_from(["ted", "caps", "list", "-d"]);
        if let Some(Commands::Caps(args)) = cli.command {
            assert!(matches!(
                args.command,
                CapsCommands::List { detailed: true }
            ));
        } else {
            panic!("Expected Caps command");
        }
    }

    #[test]
    fn test_caps_show() {
        let cli = Cli::parse_from(["ted", "caps", "show", "rust"]);
        if let Some(Commands::Caps(args)) = cli.command {
            if let CapsCommands::Show { name } = args.command {
                assert_eq!(name, "rust");
            } else {
                panic!("Expected Show subcommand");
            }
        } else {
            panic!("Expected Caps command");
        }
    }

    #[test]
    fn test_caps_create() {
        let cli = Cli::parse_from(["ted", "caps", "create", "my-cap"]);
        if let Some(Commands::Caps(args)) = cli.command {
            if let CapsCommands::Create { name } = args.command {
                assert_eq!(name, "my-cap");
            } else {
                panic!("Expected Create subcommand");
            }
        } else {
            panic!("Expected Caps command");
        }
    }

    #[test]
    fn test_caps_edit() {
        let cli = Cli::parse_from(["ted", "caps", "edit", "my-cap"]);
        if let Some(Commands::Caps(args)) = cli.command {
            if let CapsCommands::Edit { name } = args.command {
                assert_eq!(name, "my-cap");
            } else {
                panic!("Expected Edit subcommand");
            }
        } else {
            panic!("Expected Caps command");
        }
    }

    #[test]
    fn test_caps_import() {
        let cli = Cli::parse_from(["ted", "caps", "import", "/path/to/cap.json"]);
        if let Some(Commands::Caps(args)) = cli.command {
            if let CapsCommands::Import { source } = args.command {
                assert_eq!(source, "/path/to/cap.json");
            } else {
                panic!("Expected Import subcommand");
            }
        } else {
            panic!("Expected Caps command");
        }
    }

    #[test]
    fn test_caps_export() {
        let cli = Cli::parse_from(["ted", "caps", "export", "rust", "-o", "/path/out.json"]);
        if let Some(Commands::Caps(args)) = cli.command {
            if let CapsCommands::Export { name, output } = args.command {
                assert_eq!(name, "rust");
                assert_eq!(output, Some(PathBuf::from("/path/out.json")));
            } else {
                panic!("Expected Export subcommand");
            }
        } else {
            panic!("Expected Caps command");
        }
    }

    // ==================== History Commands ====================

    #[test]
    fn test_history_list_default() {
        let cli = Cli::parse_from(["ted", "history", "list"]);
        if let Some(Commands::History(args)) = cli.command {
            if let HistoryCommands::List { limit } = args.command {
                assert_eq!(limit, 10); // default
            } else {
                panic!("Expected List subcommand");
            }
        } else {
            panic!("Expected History command");
        }
    }

    #[test]
    fn test_history_list_with_limit() {
        let cli = Cli::parse_from(["ted", "history", "list", "-l", "20"]);
        if let Some(Commands::History(args)) = cli.command {
            if let HistoryCommands::List { limit } = args.command {
                assert_eq!(limit, 20);
            } else {
                panic!("Expected List subcommand");
            }
        } else {
            panic!("Expected History command");
        }
    }

    #[test]
    fn test_history_search() {
        let cli = Cli::parse_from(["ted", "history", "search", "authentication"]);
        if let Some(Commands::History(args)) = cli.command {
            if let HistoryCommands::Search { query } = args.command {
                assert_eq!(query, "authentication");
            } else {
                panic!("Expected Search subcommand");
            }
        } else {
            panic!("Expected History command");
        }
    }

    #[test]
    fn test_history_show() {
        let cli = Cli::parse_from(["ted", "history", "show", "abc-123"]);
        if let Some(Commands::History(args)) = cli.command {
            if let HistoryCommands::Show { session_id } = args.command {
                assert_eq!(session_id, "abc-123");
            } else {
                panic!("Expected Show subcommand");
            }
        } else {
            panic!("Expected History command");
        }
    }

    #[test]
    fn test_history_delete() {
        let cli = Cli::parse_from(["ted", "history", "delete", "abc-123"]);
        if let Some(Commands::History(args)) = cli.command {
            if let HistoryCommands::Delete { session_id } = args.command {
                assert_eq!(session_id, "abc-123");
            } else {
                panic!("Expected Delete subcommand");
            }
        } else {
            panic!("Expected History command");
        }
    }

    #[test]
    fn test_history_clear() {
        let cli = Cli::parse_from(["ted", "history", "clear"]);
        if let Some(Commands::History(args)) = cli.command {
            assert!(matches!(
                args.command,
                HistoryCommands::Clear { force: false }
            ));
        } else {
            panic!("Expected History command");
        }
    }

    #[test]
    fn test_history_clear_force() {
        let cli = Cli::parse_from(["ted", "history", "clear", "-f"]);
        if let Some(Commands::History(args)) = cli.command {
            assert!(matches!(
                args.command,
                HistoryCommands::Clear { force: true }
            ));
        } else {
            panic!("Expected History command");
        }
    }

    // ==================== Context Commands ====================

    #[test]
    fn test_context_stats() {
        let cli = Cli::parse_from(["ted", "context", "stats"]);
        if let Some(Commands::Context(args)) = cli.command {
            assert!(matches!(args.command, ContextCommands::Stats));
        } else {
            panic!("Expected Context command");
        }
    }

    #[test]
    fn test_context_usage() {
        let cli = Cli::parse_from(["ted", "context", "usage"]);
        if let Some(Commands::Context(args)) = cli.command {
            assert!(matches!(args.command, ContextCommands::Usage));
        } else {
            panic!("Expected Context command");
        }
    }

    #[test]
    fn test_context_prune_default() {
        let cli = Cli::parse_from(["ted", "context", "prune"]);
        if let Some(Commands::Context(args)) = cli.command {
            if let ContextCommands::Prune {
                days,
                force,
                dry_run,
            } = args.command
            {
                assert!(days.is_none());
                assert!(!force);
                assert!(!dry_run);
            } else {
                panic!("Expected Prune subcommand");
            }
        } else {
            panic!("Expected Context command");
        }
    }

    #[test]
    fn test_context_prune_with_options() {
        let cli = Cli::parse_from(["ted", "context", "prune", "-d", "30", "-f", "--dry-run"]);
        if let Some(Commands::Context(args)) = cli.command {
            if let ContextCommands::Prune {
                days,
                force,
                dry_run,
            } = args.command
            {
                assert_eq!(days, Some(30));
                assert!(force);
                assert!(dry_run);
            } else {
                panic!("Expected Prune subcommand");
            }
        } else {
            panic!("Expected Context command");
        }
    }

    #[test]
    fn test_context_clear() {
        let cli = Cli::parse_from(["ted", "context", "clear"]);
        if let Some(Commands::Context(args)) = cli.command {
            assert!(matches!(
                args.command,
                ContextCommands::Clear { force: false }
            ));
        } else {
            panic!("Expected Context command");
        }
    }

    #[test]
    fn test_context_clear_force() {
        let cli = Cli::parse_from(["ted", "context", "clear", "-f"]);
        if let Some(Commands::Context(args)) = cli.command {
            assert!(matches!(
                args.command,
                ContextCommands::Clear { force: true }
            ));
        } else {
            panic!("Expected Context command");
        }
    }

    // ==================== Settings Commands ====================

    #[test]
    fn test_settings_no_subcommand() {
        let cli = Cli::parse_from(["ted", "settings"]);
        if let Some(Commands::Settings(args)) = cli.command {
            assert!(args.command.is_none());
        } else {
            panic!("Expected Settings command");
        }
    }

    #[test]
    fn test_settings_show() {
        let cli = Cli::parse_from(["ted", "settings", "show"]);
        if let Some(Commands::Settings(args)) = cli.command {
            assert!(matches!(args.command, Some(SettingsCommands::Show)));
        } else {
            panic!("Expected Settings command");
        }
    }

    #[test]
    fn test_settings_get() {
        let cli = Cli::parse_from(["ted", "settings", "get", "provider"]);
        if let Some(Commands::Settings(args)) = cli.command {
            if let Some(SettingsCommands::Get { key }) = args.command {
                assert_eq!(key, "provider");
            } else {
                panic!("Expected Get subcommand");
            }
        } else {
            panic!("Expected Settings command");
        }
    }

    #[test]
    fn test_settings_set() {
        let cli = Cli::parse_from(["ted", "settings", "set", "provider", "local"]);
        if let Some(Commands::Settings(args)) = cli.command {
            if let Some(SettingsCommands::Set { key, value }) = args.command {
                assert_eq!(key, "provider");
                assert_eq!(value, "local");
            } else {
                panic!("Expected Set subcommand");
            }
        } else {
            panic!("Expected Settings command");
        }
    }

    #[test]
    fn test_settings_reset() {
        let cli = Cli::parse_from(["ted", "settings", "reset"]);
        if let Some(Commands::Settings(args)) = cli.command {
            assert!(matches!(args.command, Some(SettingsCommands::Reset)));
        } else {
            panic!("Expected Settings command");
        }
    }

    #[test]
    fn test_settings_config_alias() {
        let cli = Cli::parse_from(["ted", "config"]);
        assert!(matches!(cli.command, Some(Commands::Settings(_))));
    }

    // ==================== Init Command ====================

    #[test]
    fn test_init_command() {
        let cli = Cli::parse_from(["ted", "init"]);
        assert!(matches!(cli.command, Some(Commands::Init)));
    }

    // ==================== Update Command ====================

    #[test]
    fn test_update_command_basic() {
        let cli = Cli::parse_from(["ted", "update"]);
        if let Some(Commands::Update(args)) = cli.command {
            assert!(!args.check);
            assert!(!args.force);
            assert!(args.target_version.is_none());
        } else {
            panic!("Expected Update command");
        }
    }

    #[test]
    fn test_update_check() {
        let cli = Cli::parse_from(["ted", "update", "--check"]);
        if let Some(Commands::Update(args)) = cli.command {
            assert!(args.check);
        } else {
            panic!("Expected Update command");
        }
    }

    #[test]
    fn test_update_force() {
        let cli = Cli::parse_from(["ted", "update", "-f"]);
        if let Some(Commands::Update(args)) = cli.command {
            assert!(args.force);
        } else {
            panic!("Expected Update command");
        }
    }

    #[test]
    fn test_update_target_version() {
        let cli = Cli::parse_from(["ted", "update", "--target", "v0.2.0"]);
        if let Some(Commands::Update(args)) = cli.command {
            assert_eq!(args.target_version, Some("v0.2.0".to_string()));
        } else {
            panic!("Expected Update command");
        }
    }

    // ==================== System Command ====================

    #[test]
    fn test_system_command_basic() {
        let cli = Cli::parse_from(["ted", "system"]);
        if let Some(Commands::System(args)) = cli.command {
            assert!(!args.upgrades);
            assert!(!args.detailed);
        } else {
            panic!("Expected System command");
        }
    }

    #[test]
    fn test_system_upgrades() {
        let cli = Cli::parse_from(["ted", "system", "-u"]);
        if let Some(Commands::System(args)) = cli.command {
            assert!(args.upgrades);
        } else {
            panic!("Expected System command");
        }
    }

    #[test]
    fn test_system_detailed() {
        let cli = Cli::parse_from(["ted", "system", "-d"]);
        if let Some(Commands::System(args)) = cli.command {
            assert!(args.detailed);
        } else {
            panic!("Expected System command");
        }
    }

    #[test]
    fn test_system_hw_alias() {
        let cli = Cli::parse_from(["ted", "hw"]);
        assert!(matches!(cli.command, Some(Commands::System(_))));
    }

    // ==================== MCP Command ====================

    #[test]
    fn test_mcp_command_basic() {
        let cli = Cli::parse_from(["ted", "mcp"]);
        if let Some(Commands::Mcp(args)) = cli.command {
            assert!(args.project.is_none());
        } else {
            panic!("Expected Mcp command");
        }
    }

    #[test]
    fn test_mcp_with_project() {
        let cli = Cli::parse_from(["ted", "mcp", "-p", "/path/to/project"]);
        if let Some(Commands::Mcp(args)) = cli.command {
            assert_eq!(args.project, Some("/path/to/project".to_string()));
        } else {
            panic!("Expected Mcp command");
        }
    }

    // ==================== LSP Command ====================

    #[test]
    fn test_lsp_command() {
        let cli = Cli::parse_from(["ted", "lsp"]);
        assert!(matches!(cli.command, Some(Commands::Lsp)));
    }

    // ==================== OutputFormat Tests ====================

    #[test]
    fn test_output_format_default() {
        let format = OutputFormat::default();
        assert_eq!(format, OutputFormat::Text);
    }

    #[test]
    fn test_output_format_clone() {
        let format = OutputFormat::Json;
        let cloned = format.clone();
        assert_eq!(cloned, OutputFormat::Json);
    }

    #[test]
    fn test_output_format_debug() {
        let format = OutputFormat::Markdown;
        let debug_str = format!("{:?}", format);
        assert!(debug_str.contains("Markdown"));
    }

    // ==================== ChatArgs Default ====================

    #[test]
    fn test_chat_args_default() {
        let args = ChatArgs::default();
        assert!(args.prompt.is_none());
        assert!(args.cap.is_empty());
        assert!(args.model.is_none());
        assert!(args.provider.is_none());
        assert!(args.resume.is_none());
        assert!(!args.trust);
        assert!(!args.no_stream);
        assert!(!args.embedded);
        assert!(args.history.is_none());
        assert!(!args.review_mode);
        assert!(!args.project_has_files);
        assert!(args.system_prompt_file.is_none());
        assert!(args.files_in_context.is_empty());
    }
}
