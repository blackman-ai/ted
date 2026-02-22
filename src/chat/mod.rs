// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Chat session management
//!
//! This module provides structures for managing interactive chat sessions,
//! including session state, provider management, and conversation handling.
//!
//! ## Submodules
//!
//! - `agent` - Agent loop logic for processing LLM responses and tool executions
//! - `commands` - Command parsing and handling for slash commands
//! - `display` - Display formatting functions for the chat interface
//! - `input_parser` - Pure input parsing functions
//! - `provider_config` - Provider configuration and validation
//! - `session` - Chat session state management
//! - `slash_commands` - Development slash command execution
//! - `streaming` - Streaming response handling

pub mod agent;
pub mod commands;
pub mod display;
pub mod engine;
pub mod input_parser;
pub mod provider_config;
mod session;
pub mod slash_commands;
pub mod streaming;

pub use session::{
    record_message_and_persist, trim_conversation_if_needed, ChatSession, ChatSessionBuilder,
    SessionState,
};

// Re-export commonly used types
pub use agent::{AgentLoopConfig, AgentLoopState, ToolCallResult, ToolCallTracker};
pub use commands::{ChatCommand, CommandResult};
pub use display::{ShellOutputDisplay, ShellStatus, ToolInvocationDisplay, ToolResultDisplay};
pub use engine::{AgentLoopObserver, NoopAgentLoopObserver};
pub use input_parser::ProviderChoice;
pub use provider_config::{ApiKeyValidation, ProviderConfig, ProviderValidation};
pub use streaming::{StreamAccumulator, StreamEventResult, StreamStats};
