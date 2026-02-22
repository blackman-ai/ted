// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Ted - AI coding assistant for local-first development workflows.
//!
//! This crate exposes the shared runtime used by:
//! - the `ted` CLI (`src/main.rs`)
//! - the interactive TUI chat runtime
//! - embedded JSONL mode consumed by Teddy (Electron)
//!
//! Architecture highlights:
//! - `chat`: shared conversation engine, tool loop orchestration, session flow
//! - `llm`: provider abstraction and implementations (Anthropic/OpenRouter/Blackman/local)
//! - `context`: WAL-backed context storage and compaction
//! - `tools`: built-in tool implementations and execution/permission flow
//! - `agents`, `plans`, `beads`: multi-agent and planning/task primitives
//! - `tui`, `embedded`: runtime-specific presentation layers
//!
//! See `docs/ARCHITECTURE.md` for a cross-module system overview.

pub mod agents;
pub mod beads;
pub mod caps;
pub mod chat;
pub mod cli;
pub mod commands;
pub mod config;
pub mod context;
pub mod embedded;
pub mod embedded_runner;
pub mod embeddings;
pub mod error;
pub mod hardware;
pub mod history;
pub mod indexer;
pub mod llm;
pub mod lsp;
pub mod mcp;
pub mod models;
pub mod plans;
pub mod skills;
pub mod tools;
pub mod tui;
pub mod update;
pub mod utils;

pub use error::{Result, TedError};
