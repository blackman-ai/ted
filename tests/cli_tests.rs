// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use clap::Parser;
use ted::cli::{Cli, Commands};

#[test]
fn test_parse_chat_command() {
    let args = vec!["ted", "chat"];
    let cli = Cli::try_parse_from(args).expect("Valid command parsing");
    assert!(matches!(cli.command, Some(Commands::Chat(_))));
}

#[test]
fn test_parse_chat_with_cap() {
    let args = vec!["ted", "chat", "-c", "rust-expert"];
    let cli = Cli::try_parse_from(args).expect("Valid command parsing");
    if let Some(Commands::Chat(chat_args)) = cli.command {
        assert_eq!(chat_args.cap, vec!["rust-expert"]);
    } else {
        panic!("Expected Chat command");
    }
}

#[test]
fn test_parse_chat_with_model() {
    let args = vec!["ted", "chat", "-m", "claude-3-5-haiku-20241022"];
    let cli = Cli::try_parse_from(args).expect("Valid command parsing");
    if let Some(Commands::Chat(chat_args)) = cli.command {
        assert_eq!(
            chat_args.model,
            Some("claude-3-5-haiku-20241022".to_string())
        );
    } else {
        panic!("Expected Chat command");
    }
}

#[test]
fn test_parse_ask_command() {
    let args = vec!["ted", "ask", "What is Rust?"];
    let cli = Cli::try_parse_from(args).expect("Valid command parsing");
    if let Some(Commands::Ask(ask_args)) = cli.command {
        assert_eq!(ask_args.prompt, "What is Rust?");
    } else {
        panic!("Expected Ask command");
    }
}

#[test]
fn test_parse_caps_list() {
    let args = vec!["ted", "caps", "list"];
    let cli = Cli::try_parse_from(args).expect("Valid command parsing");
    assert!(matches!(cli.command, Some(Commands::Caps(_))));
}

#[test]
fn test_parse_settings_command() {
    let args = vec!["ted", "settings"];
    let cli = Cli::try_parse_from(args).expect("Valid command parsing");
    assert!(matches!(cli.command, Some(Commands::Settings(_))));
}

#[test]
fn test_parse_history_list() {
    let args = vec!["ted", "history", "list"];
    let cli = Cli::try_parse_from(args).expect("Valid command parsing");
    assert!(matches!(cli.command, Some(Commands::History(_))));
}

#[test]
fn test_parse_context_stats() {
    let args = vec!["ted", "context", "stats"];
    let cli = Cli::try_parse_from(args).expect("Valid command parsing");
    assert!(matches!(cli.command, Some(Commands::Context(_))));
}

#[test]
fn test_parse_clear_command() {
    let args = vec!["ted", "clear"];
    let cli = Cli::try_parse_from(args).expect("Valid command parsing");
    assert!(matches!(cli.command, Some(Commands::Clear)));
}

#[test]
fn test_parse_init_command() {
    let args = vec!["ted", "init"];
    let cli = Cli::try_parse_from(args).expect("Valid command parsing");
    assert!(matches!(cli.command, Some(Commands::Init)));
}

#[test]
fn test_parse_no_command_defaults_to_none() {
    let args = vec!["ted"];
    let cli = Cli::try_parse_from(args).expect("Valid command parsing");
    assert!(cli.command.is_none());
}

#[test]
fn test_global_verbose_flag() {
    let args = vec!["ted", "-vvv", "chat"];
    let cli = Cli::try_parse_from(args).expect("Valid command parsing");
    assert_eq!(cli.verbose, 3);
}

#[test]
fn test_external_subcommand() {
    // External subcommands are caught by Run(Vec<String>), not rejected
    let args = vec!["ted", "my-custom-command", "arg1"];
    let cli = Cli::try_parse_from(args).expect("External subcommand should parse");
    if let Some(Commands::Run(args)) = cli.command {
        assert_eq!(args, vec!["my-custom-command", "arg1"]);
    } else {
        panic!("Expected Run command for external subcommand");
    }
}
