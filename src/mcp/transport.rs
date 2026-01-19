// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! MCP transport layer - stdio-based communication
//!
//! MCP servers communicate via stdio (standard input/output) using JSON-RPC 2.0

use std::io::{self, BufRead, Write};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::Result;
use super::protocol::{JsonRpcRequest, JsonRpcResponse};

/// Stdio transport for MCP
pub struct StdioTransport {
    stdin: Arc<Mutex<io::Stdin>>,
    stdout: Arc<Mutex<io::Stdout>>,
}

impl StdioTransport {
    /// Create a new stdio transport
    pub fn new() -> Self {
        Self {
            stdin: Arc::new(Mutex::new(io::stdin())),
            stdout: Arc::new(Mutex::new(io::stdout())),
        }
    }

    /// Read a JSON-RPC request from stdin
    pub async fn read_request(&self) -> Result<JsonRpcRequest> {
        let stdin = self.stdin.lock().await;
        let reader = io::BufReader::new(&*stdin);

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<JsonRpcRequest>(&line) {
                Ok(req) => return Ok(req),
                Err(e) => {
                    tracing::error!("Failed to parse JSON-RPC request: {}", e);
                    continue;
                }
            }
        }

        Err(crate::error::TedError::Config("EOF reached on stdin".to_string()))
    }

    /// Write a JSON-RPC response to stdout
    pub async fn write_response(&self, response: &JsonRpcResponse) -> Result<()> {
        let json = serde_json::to_string(response)?;

        let mut stdout = self.stdout.lock().await;
        writeln!(stdout, "{}", json)?;
        stdout.flush()?;

        Ok(())
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdio_transport_creation() {
        let transport = StdioTransport::new();
        assert!(Arc::strong_count(&transport.stdin) >= 1);
        assert!(Arc::strong_count(&transport.stdout) >= 1);
    }
}
