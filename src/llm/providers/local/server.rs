// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! llama-server subprocess manager
//!
//! Manages the lifecycle of a llama-server process for local LLM inference.
//! The server exposes an OpenAI-compatible API on a local port.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use crate::error::{Result, TedError};

const DEFAULT_PORT: u16 = 8847;
const HEALTH_POLL_INTERVAL_MS: u64 = 500;
const HEALTH_TIMEOUT_SECS: u64 = 120;

/// Manages a llama-server subprocess
pub struct LlamaServer {
    process: Mutex<Option<Child>>,
    port: u16,
    model_path: PathBuf,
    binary_path: PathBuf,
    gpu_layers: Option<i32>,
    ctx_size: Option<u32>,
}

impl LlamaServer {
    pub fn new(
        binary_path: PathBuf,
        model_path: PathBuf,
        port: Option<u16>,
        gpu_layers: Option<i32>,
        ctx_size: Option<u32>,
    ) -> Self {
        Self {
            process: Mutex::new(None),
            port: port.unwrap_or(DEFAULT_PORT),
            model_path,
            binary_path,
            gpu_layers,
            ctx_size,
        }
    }

    /// Start the llama-server subprocess
    pub async fn start(&self) -> Result<()> {
        // Check if already running
        if self.is_running() {
            return Ok(());
        }

        let mut cmd = Command::new(&self.binary_path);

        cmd.arg("--model")
            .arg(&self.model_path)
            .arg("--port")
            .arg(self.port.to_string())
            .arg("--host")
            .arg("127.0.0.1");

        if let Some(ngl) = self.gpu_layers {
            cmd.arg("--n-gpu-layers").arg(ngl.to_string());
        }

        if let Some(ctx) = self.ctx_size {
            cmd.arg("--ctx-size").arg(ctx.to_string());
        }

        // Suppress output from the server process
        cmd.stdout(Stdio::null()).stderr(Stdio::null());

        let child = cmd.spawn().map_err(|e| {
            TedError::Config(format!(
                "Failed to start llama-server at {}: {}",
                self.binary_path.display(),
                e
            ))
        })?;

        *self.process.lock().unwrap() = Some(child);

        // Wait for server to be ready
        self.wait_for_ready().await?;

        tracing::info!(
            "llama-server started on port {} with model {}",
            self.port,
            self.model_path.display()
        );

        Ok(())
    }

    /// Wait for the server's /health endpoint to respond
    async fn wait_for_ready(&self) -> Result<()> {
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/health", self.port);
        let max_attempts = (HEALTH_TIMEOUT_SECS * 1000 / HEALTH_POLL_INTERVAL_MS) as usize;

        for _ in 0..max_attempts {
            // Check if process is still alive
            if !self.is_running() {
                return Err(TedError::Config(
                    "llama-server process exited unexpectedly during startup".to_string(),
                ));
            }

            if let Ok(resp) = client.get(&url).send().await {
                if resp.status().is_success() {
                    return Ok(());
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(HEALTH_POLL_INTERVAL_MS)).await;
        }

        // Kill the process if it didn't become ready
        self.shutdown();
        Err(TedError::Config(format!(
            "llama-server failed to start within {} seconds. \
             The model may be too large for your system's memory.",
            HEALTH_TIMEOUT_SECS
        )))
    }

    /// Get the base URL for API calls
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Check if the server process is still running
    pub fn is_running(&self) -> bool {
        if let Ok(mut guard) = self.process.lock() {
            if let Some(ref mut child) = *guard {
                return matches!(child.try_wait(), Ok(None));
            }
        }
        false
    }

    /// Gracefully shutdown the server
    pub fn shutdown(&self) {
        if let Ok(mut guard) = self.process.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llama_server_new() {
        let server = LlamaServer::new(
            PathBuf::from("/usr/bin/llama-server"),
            PathBuf::from("/models/test.gguf"),
            None,
            None,
            None,
        );
        assert_eq!(server.port, DEFAULT_PORT);
        assert_eq!(
            server.base_url(),
            format!("http://127.0.0.1:{}", DEFAULT_PORT)
        );
    }

    #[test]
    fn test_llama_server_custom_port() {
        let server = LlamaServer::new(
            PathBuf::from("/usr/bin/llama-server"),
            PathBuf::from("/models/test.gguf"),
            Some(9999),
            None,
            None,
        );
        assert_eq!(server.port, 9999);
        assert_eq!(server.base_url(), "http://127.0.0.1:9999");
    }

    #[test]
    fn test_llama_server_not_running_initially() {
        let server = LlamaServer::new(
            PathBuf::from("/nonexistent"),
            PathBuf::from("/nonexistent"),
            None,
            None,
            None,
        );
        assert!(!server.is_running());
    }
}
