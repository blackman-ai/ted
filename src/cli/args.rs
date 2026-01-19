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

    /// LLM provider to use (anthropic, ollama)
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

    /// Path to conversation history JSON file (for multi-turn embedded conversations)
    #[arg(long, hide = true)]
    pub history: Option<PathBuf>,

    /// Review mode - emit file events but don't execute file modifications (for GUI review before applying)
    #[arg(long, hide = true)]
    pub review_mode: bool,

    /// Project has existing files - used by enforcement logic to determine if editing is expected
    #[arg(long, hide = true)]
    pub project_has_files: bool,
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

    /// LLM provider to use (anthropic, ollama)
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
#[derive(ValueEnum, Clone, Debug, Default)]
pub enum OutputFormat {
    /// Plain text output
    #[default]
    Text,

    /// JSON output
    Json,

    /// Markdown output
    Markdown,
}
