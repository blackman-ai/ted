// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! llama-server subprocess manager
//!
//! Manages the lifecycle of a llama-server process for local LLM inference.
//! The server exposes an OpenAI-compatible API on a local port.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use crate::error::{Result, TedError};

const DEFAULT_PORT: u16 = 8847;
const HEALTH_POLL_INTERVAL_MS: u64 = 500;
const HEALTH_TIMEOUT_SECS: u64 = 120;
const MAX_ERROR_DETAIL_CHARS: usize = 600;

#[cfg(target_os = "macos")]
fn ensure_macos_loader_symlinks(binary_path: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    let Some(dir) = binary_path.parent() else {
        return Ok(());
    };

    let required = [
        "libmtmd.0.dylib",
        "libllama.0.dylib",
        "libggml.0.dylib",
        "libggml-cpu.0.dylib",
        "libggml-blas.0.dylib",
        "libggml-metal.0.dylib",
        "libggml-rpc.0.dylib",
        "libggml-base.0.dylib",
    ];

    let entries = std::fs::read_dir(dir)
        .map_err(|e| TedError::Config(format!("Failed reading {}: {}", dir.display(), e)))?;
    let names: Vec<String> = entries
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| entry.file_name().to_str().map(|s| s.to_string()))
        .collect();

    for required_name in required {
        let required_path = dir.join(required_name);
        if std::fs::symlink_metadata(&required_path).is_ok() {
            continue;
        }

        let prefix = format!("{}.", required_name.trim_end_matches(".dylib"));
        let candidate = names
            .iter()
            .find(|name| name.starts_with(&prefix) && name.ends_with(".dylib"));

        if let Some(candidate) = candidate {
            symlink(candidate, &required_path).map_err(|e| {
                TedError::Config(format!(
                    "Failed creating symlink {} -> {}: {}",
                    required_name, candidate, e
                ))
            })?;
            tracing::info!(
                "Created macOS dylib link {} -> {}",
                required_name,
                candidate
            );
        }
    }

    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn ensure_macos_loader_symlinks(_binary_path: &Path) -> Result<()> {
    Ok(())
}

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

        ensure_macos_loader_symlinks(&self.binary_path)?;

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

        // Keep stderr piped so we can surface startup failures meaningfully.
        cmd.stdout(Stdio::null()).stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| {
            TedError::Config(format!(
                "Failed to start llama-server at {}: {}",
                self.binary_path.display(),
                e
            ))
        })?;

        match self.process.lock() {
            Ok(mut process) => *process = Some(child),
            Err(poisoned) => {
                tracing::warn!("Process lock was poisoned, recovering");
                *poisoned.into_inner() = Some(child);
            }
        }

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
                let detail = self
                    .collect_exit_detail()
                    .unwrap_or_else(|| "no stderr".to_string());

                if is_bind_port_failure(&detail) {
                    return Err(TedError::Config(format!(
                        "llama-server could not start because port {} is already in use. \
                         Stop the process using that port or change the Local Server Port in settings. \
                         Startup detail: {}",
                        self.port, detail
                    )));
                }

                return Err(TedError::Config(format!(
                    "llama-server process exited unexpectedly during startup: {}",
                    detail
                )));
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

    /// Collect an explanatory exit detail from a process that has already exited.
    fn collect_exit_detail(&self) -> Option<String> {
        let mut guard = self.process.lock().ok()?;
        let child = guard.take()?;

        match child.wait_with_output() {
            Ok(output) => {
                let code = output
                    .status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".to_string());
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stderr = stderr.trim();

                if stderr.is_empty() {
                    Some(format!("exit code {}", code))
                } else {
                    let detail = truncate_detail(stderr, MAX_ERROR_DETAIL_CHARS);
                    Some(format!("exit code {} - {}", code, detail))
                }
            }
            Err(err) => Some(format!("failed to read process output: {}", err)),
        }
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn truncate_detail(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    input.chars().take(max_chars).collect::<String>() + "..."
}

fn is_bind_port_failure(detail: &str) -> bool {
    let lower = detail.to_lowercase();
    lower.contains("couldn't bind http server socket")
        || lower.contains("address already in use")
        || lower.contains("eaddrinuse")
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

    #[test]
    fn test_truncate_detail_short_string_unchanged() {
        let value = "hello";
        assert_eq!(truncate_detail(value, 10), "hello");
    }

    #[test]
    fn test_truncate_detail_long_string_truncated() {
        let value = "abcdefghijklmnopqrstuvwxyz";
        assert_eq!(truncate_detail(value, 5), "abcde...");
    }

    #[test]
    fn test_is_bind_port_failure_detects_known_messages() {
        assert!(is_bind_port_failure(
            "start: couldn't bind HTTP server socket, hostname: 127.0.0.1, port: 8847"
        ));
        assert!(is_bind_port_failure("Address already in use"));
        assert!(is_bind_port_failure("bind failed: EADDRINUSE"));
    }

    #[test]
    fn test_is_bind_port_failure_false_for_other_errors() {
        assert!(!is_bind_port_failure("failed to load model"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_ensure_macos_loader_symlinks_creates_missing_links() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("llama-server");
        std::fs::write(&binary, b"bin").unwrap();
        std::fs::write(dir.path().join("libmtmd.0.0.7951.dylib"), b"x").unwrap();
        std::fs::write(dir.path().join("libllama.0.0.7951.dylib"), b"x").unwrap();

        ensure_macos_loader_symlinks(&binary).unwrap();

        assert!(
            std::fs::symlink_metadata(dir.path().join("libmtmd.0.dylib"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            std::fs::symlink_metadata(dir.path().join("libllama.0.dylib"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}
