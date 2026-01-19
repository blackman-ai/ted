// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! MCP server command

use crate::cli::args::McpArgs;
use crate::error::Result;
use crate::mcp::McpServer;
use crate::tools::ToolExecutor;

/// Execute the MCP server command
pub async fn execute(args: &McpArgs) -> Result<()> {
    eprintln!("[TED MCP] Starting Model Context Protocol server");
    eprintln!("[TED MCP] Protocol version: {}", crate::mcp::PROTOCOL_VERSION);

    let project_dir = if let Some(ref project) = args.project {
        eprintln!("[TED MCP] Project directory: {}", project);
        std::path::PathBuf::from(project)
    } else {
        std::env::current_dir()?
    };

    // Create tool context for executor
    let context = crate::tools::ToolContext::new(
        project_dir.clone(),
        Some(project_dir),
        uuid::Uuid::new_v4(),
        false, // Not in trust mode by default
    );

    // Create tool executor
    let executor = ToolExecutor::new(context, false);

    // Create MCP server
    let server = McpServer::new(executor);

    // Register all built-in tools
    let registry = crate::tools::ToolRegistry::with_builtins();
    for tool_name in registry.names() {
        if let Some(tool) = registry.get(tool_name) {
            eprintln!("[TED MCP] Registering tool: {}", tool_name);
            server.register_tool(tool.clone()).await;
        }
    }

    eprintln!("[TED MCP] Registered {} tools", registry.len());
    eprintln!("[TED MCP] Server ready - listening on stdio");
    eprintln!("[TED MCP] Compatible with Claude Desktop and other MCP clients");

    // Run the server
    server.run().await
}
