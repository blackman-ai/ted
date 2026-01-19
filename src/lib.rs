// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Ted - AI coding assistant for your terminal
//!
//! A fast, portable AI coding assistant written in Rust.

pub mod caps;
pub mod cli;
pub mod commands;
pub mod config;
pub mod context;
pub mod embedded;
pub mod embedded_runner;
pub mod error;
pub mod hardware;
pub mod history;
pub mod indexer;
pub mod llm;
pub mod mcp;
pub mod plans;
pub mod tools;
pub mod tui;
pub mod update;
pub mod utils;

pub use error::{Result, TedError};
