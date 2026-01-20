// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Language Server Protocol (LSP) implementation for Ted
//!
//! This module provides LSP support for IDE integration, including:
//! - Autocomplete suggestions based on project context
//! - Go-to-definition using file indexing
//! - Hover information
//! - Diagnostics (via linting)

mod server;
mod capabilities;
mod completion;
mod definition;
mod hover;

pub use server::TedLanguageServer;

use tower_lsp::{LspService, Server};
use tokio::io::{stdin, stdout};

/// Start the LSP server on stdio
pub async fn start_server() -> anyhow::Result<()> {
    tracing::info!("Starting Ted LSP server...");

    let (service, socket) = LspService::new(|client| TedLanguageServer::new(client));

    let stdin = stdin();
    let stdout = stdout();

    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
