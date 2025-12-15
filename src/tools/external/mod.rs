// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! External tools system
//!
//! This module allows external tools written in any language to be registered
//! as first-class tools in Ted. External tools communicate via stdio using
//! a simplified JSON-RPC protocol.
//!
//! # Creating an External Tool
//!
//! 1. Create a manifest file at `~/.ted/tools/my_tool.json`:
//!
//! ```json
//! {
//!   "name": "my_tool",
//!   "description": "Description shown to the LLM",
//!   "command": ["node", "~/.ted/tools/my-tool/index.js"],
//!   "input_schema": {
//!     "type": "object",
//!     "properties": {
//!       "path": { "type": "string", "description": "File path" }
//!     },
//!     "required": ["path"]
//!   }
//! }
//! ```
//!
//! 2. Implement the tool to read JSON-RPC requests from stdin and write
//!    responses to stdout:
//!
//! ```javascript
//! // Read request from stdin
//! const readline = require('readline');
//! const rl = readline.createInterface({ input: process.stdin });
//!
//! rl.on('line', (line) => {
//!   const request = JSON.parse(line);
//!   const { params, id } = request;
//!
//!   // Do work...
//!   const result = {
//!     output: "Tool output here",
//!     recall: {
//!       files_read: [params.path]
//!     }
//!   };
//!
//!   // Write response to stdout
//!   console.log(JSON.stringify({ jsonrpc: "2.0", result, id }));
//!   process.exit(0);
//! });
//! ```
//!
//! # Protocol
//!
//! See the [`protocol`] module for the JSON-RPC format.
//!
//! # Memory Integration
//!
//! External tools can report file accesses through the `recall` field in their
//! response. This allows Ted's memory system to track which files the tool
//! accessed and boost their retention scores.

pub mod loader;
pub mod manifest;
pub mod protocol;

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{Result, TedError};
use crate::llm::provider::ToolDefinition;
use crate::tools::{PermissionRequest, Tool, ToolContext, ToolResult};

pub use loader::ToolLoader;
pub use manifest::ToolManifest;
pub use protocol::{RecallPayload, Request, Response};

/// Global request ID counter for JSON-RPC
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// An external tool loaded from a manifest.
pub struct ExternalTool {
    /// The tool manifest
    manifest: ToolManifest,
}

impl ExternalTool {
    /// Create a new external tool from a manifest.
    pub fn new(manifest: ToolManifest) -> Self {
        Self { manifest }
    }

    /// Get the manifest.
    pub fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    /// Execute the tool process and communicate via JSON-RPC.
    fn execute_process(&self, input: Value, working_dir: &std::path::Path) -> Result<Response> {
        let command = self.manifest.expand_command();
        if command.is_empty() {
            return Err(TedError::ToolExecution("Empty command".to_string()));
        }

        let program = &command[0];
        let args = &command[1..];

        // Build the request
        let request_id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);
        let request = Request::execute(input, request_id);
        let request_json = request.to_json();

        // Set up working directory
        let work_dir = self.manifest.expand_working_directory(working_dir);

        // Spawn the process
        let mut child = Command::new(program)
            .args(args)
            .current_dir(&work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .envs(&self.manifest.env)
            .spawn()
            .map_err(|e| {
                TedError::ToolExecution(format!(
                    "Failed to spawn external tool '{}': {}",
                    self.manifest.name, e
                ))
            })?;

        // Write request to stdin
        if let Some(mut stdin) = child.stdin.take() {
            writeln!(stdin, "{}", request_json).map_err(|e| {
                TedError::ToolExecution(format!("Failed to write to tool stdin: {}", e))
            })?;
        }

        // Wait for completion with timeout
        let timeout = Duration::from_millis(self.manifest.timeout_ms);
        let output = wait_with_timeout(&mut child, timeout)?;

        // Check exit status
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TedError::ToolExecution(format!(
                "External tool '{}' failed with exit code {:?}: {}",
                self.manifest.name,
                output.status.code(),
                stderr.trim()
            )));
        }

        // Parse response from stdout
        let stdout = String::from_utf8_lossy(&output.stdout);
        let response_line = stdout.lines().last().unwrap_or("");

        if response_line.is_empty() {
            return Err(TedError::ToolExecution(format!(
                "External tool '{}' produced no output",
                self.manifest.name
            )));
        }

        Response::parse(response_line).map_err(|e| {
            TedError::ToolExecution(format!(
                "Failed to parse tool response: {} (raw: {})",
                e, response_line
            ))
        })
    }

    /// Process recall data from the response.
    fn process_recall(&self, recall: &RecallPayload, context: &ToolContext) {
        // Emit file read events
        for path in &recall.files_read {
            context.emit_file_read(path);
        }

        // Emit file write events
        for path in &recall.files_written {
            context.emit_file_write(path);
        }

        // Emit file edit events
        for path in &recall.files_edited {
            context.emit_file_edit(path);
        }

        // Emit search match events
        if !recall.search_matches.is_empty() {
            context.emit_search_match(recall.search_matches.clone());
        }
    }
}

#[async_trait]
impl Tool for ExternalTool {
    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn definition(&self) -> ToolDefinition {
        self.manifest.to_tool_definition()
    }

    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        // Execute the tool process (blocking, but we're in async context)
        let response = match self.execute_process(input, &context.working_directory) {
            Ok(r) => r,
            Err(e) => return Ok(ToolResult::error(tool_use_id, e.to_string())),
        };

        // Process recall data for memory integration
        if let Some(recall) = response.recall() {
            self.process_recall(recall, context);
        }

        // Return the result
        if response.is_error() {
            Ok(ToolResult::error(tool_use_id, response.output()))
        } else {
            Ok(ToolResult::success(tool_use_id, response.output()))
        }
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        if !self.manifest.requires_permission {
            return None;
        }

        // Try to extract paths from common input fields
        let mut affected_paths = Vec::new();
        if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
            affected_paths.push(path.to_string());
        }
        if let Some(paths) = input.get("paths").and_then(|v| v.as_array()) {
            for p in paths {
                if let Some(s) = p.as_str() {
                    affected_paths.push(s.to_string());
                }
            }
        }

        Some(PermissionRequest {
            tool_name: self.manifest.name.clone(),
            action_description: format!("Run external tool: {}", self.manifest.name),
            affected_paths,
            is_destructive: false, // Could be extended in manifest
        })
    }

    fn requires_permission(&self) -> bool {
        self.manifest.requires_permission
    }
}

/// Wait for a child process with timeout.
fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<std::process::Output> {
    use std::thread;
    use std::time::Instant;

    let start = Instant::now();
    let poll_interval = Duration::from_millis(50);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process exited, collect output
                let stdout = child
                    .stdout
                    .take()
                    .map(|s| {
                        let mut reader = BufReader::new(s);
                        let mut output = String::new();
                        let _ = reader.read_line(&mut output);
                        output.into_bytes()
                    })
                    .unwrap_or_default();

                let stderr = child
                    .stderr
                    .take()
                    .map(|s| {
                        let mut output = Vec::new();
                        let _ = std::io::Read::read_to_end(&mut BufReader::new(s), &mut output);
                        output
                    })
                    .unwrap_or_default();

                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                // Still running
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Err(TedError::ToolExecution(format!(
                        "External tool timed out after {:?}",
                        timeout
                    )));
                }
                thread::sleep(poll_interval);
            }
            Err(e) => {
                return Err(TedError::ToolExecution(format!(
                    "Failed to wait for tool: {}",
                    e
                )));
            }
        }
    }
}

/// Load all external tools and return them as boxed Tool trait objects.
pub fn load_external_tools() -> Vec<Box<dyn Tool>> {
    let loader = ToolLoader::new();
    loader
        .load_all()
        .into_iter()
        .map(|m| Box::new(ExternalTool::new(m)) as Box<dyn Tool>)
        .collect()
}

/// Load external tools from a specific directory.
pub fn load_external_tools_from(dir: PathBuf) -> Vec<Box<dyn Tool>> {
    let loader = ToolLoader::with_dir(dir);
    loader
        .load_all()
        .into_iter()
        .map(|m| Box::new(ExternalTool::new(m)) as Box<dyn Tool>)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn create_test_context(temp_dir: &TempDir) -> ToolContext {
        ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            Uuid::new_v4(),
            true,
        )
    }

    #[test]
    fn test_external_tool_new() {
        let manifest = ToolManifest::parse(
            r#"{
                "name": "test",
                "description": "Test tool",
                "command": ["echo"],
                "input_schema": {"type": "object", "properties": {}}
            }"#,
        )
        .unwrap();

        let tool = ExternalTool::new(manifest);
        assert_eq!(tool.name(), "test");
    }

    #[test]
    fn test_external_tool_definition() {
        let manifest = ToolManifest::parse(
            r#"{
                "name": "my_tool",
                "description": "My test tool",
                "command": ["node", "index.js"],
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    },
                    "required": ["path"]
                }
            }"#,
        )
        .unwrap();

        let tool = ExternalTool::new(manifest);
        let def = tool.definition();

        assert_eq!(def.name, "my_tool");
        assert_eq!(def.description, "My test tool");
        assert_eq!(def.input_schema.required, vec!["path"]);
    }

    #[test]
    fn test_external_tool_requires_permission() {
        let manifest_with = ToolManifest::parse(
            r#"{
                "name": "test",
                "description": "Test",
                "command": ["echo"],
                "input_schema": {"type": "object", "properties": {}},
                "requires_permission": true
            }"#,
        )
        .unwrap();

        let manifest_without = ToolManifest::parse(
            r#"{
                "name": "test",
                "description": "Test",
                "command": ["echo"],
                "input_schema": {"type": "object", "properties": {}},
                "requires_permission": false
            }"#,
        )
        .unwrap();

        let tool_with = ExternalTool::new(manifest_with);
        let tool_without = ExternalTool::new(manifest_without);

        assert!(tool_with.requires_permission());
        assert!(!tool_without.requires_permission());
    }

    #[test]
    fn test_external_tool_permission_request() {
        let manifest = ToolManifest::parse(
            r#"{
                "name": "test",
                "description": "Test",
                "command": ["echo"],
                "input_schema": {"type": "object", "properties": {}},
                "requires_permission": true
            }"#,
        )
        .unwrap();

        let tool = ExternalTool::new(manifest);
        let input = serde_json::json!({"path": "/test/file.txt"});

        let request = tool.permission_request(&input).unwrap();
        assert_eq!(request.tool_name, "test");
        assert!(request
            .affected_paths
            .contains(&"/test/file.txt".to_string()));
    }

    #[test]
    fn test_external_tool_permission_request_no_permission() {
        let manifest = ToolManifest::parse(
            r#"{
                "name": "test",
                "description": "Test",
                "command": ["echo"],
                "input_schema": {"type": "object", "properties": {}},
                "requires_permission": false
            }"#,
        )
        .unwrap();

        let tool = ExternalTool::new(manifest);
        let input = serde_json::json!({});

        assert!(tool.permission_request(&input).is_none());
    }

    #[tokio::test]
    async fn test_external_tool_execute_echo() {
        let temp_dir = TempDir::new().unwrap();
        let context = create_test_context(&temp_dir);

        // Create a simple shell script that acts as a JSON-RPC tool
        let script_path = temp_dir.path().join("tool.sh");
        std::fs::write(
            &script_path,
            r#"#!/bin/bash
read input
echo '{"jsonrpc": "2.0", "result": {"output": "Hello from tool!"}, "id": 1}'
"#,
        )
        .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let manifest = ToolManifest::parse(&format!(
            r#"{{
                "name": "echo_tool",
                "description": "Echo tool",
                "command": ["bash", "{}"],
                "input_schema": {{"type": "object", "properties": {{}}}}
            }}"#,
            script_path.display()
        ))
        .unwrap();

        let tool = ExternalTool::new(manifest);
        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("Hello from tool!"));
    }

    #[tokio::test]
    async fn test_external_tool_execute_with_recall() {
        let temp_dir = TempDir::new().unwrap();

        // Set up recall channel to verify events
        use crate::indexer::recall_channel;
        let (sender, receiver) = recall_channel();
        let context = ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            Uuid::new_v4(),
            true,
        )
        .with_recall_sender(sender);

        // Create a tool that reports file access
        let script_path = temp_dir.path().join("tool.sh");
        std::fs::write(
            &script_path,
            r#"#!/bin/bash
read input
echo '{"jsonrpc": "2.0", "result": {"output": "Done", "recall": {"files_read": ["src/main.rs"]}}, "id": 1}'
"#,
        )
        .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let manifest = ToolManifest::parse(&format!(
            r#"{{
                "name": "recall_tool",
                "description": "Tool with recall",
                "command": ["bash", "{}"],
                "input_schema": {{"type": "object", "properties": {{}}}}
            }}"#,
            script_path.display()
        ))
        .unwrap();

        let tool = ExternalTool::new(manifest);
        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await
            .unwrap();

        assert!(!result.is_error());

        // Verify recall event was emitted
        let events = receiver.drain();
        assert!(!events.is_empty());
    }

    #[tokio::test]
    async fn test_external_tool_execute_error_response() {
        let temp_dir = TempDir::new().unwrap();
        let context = create_test_context(&temp_dir);

        let script_path = temp_dir.path().join("tool.sh");
        std::fs::write(
            &script_path,
            r#"#!/bin/bash
read input
echo '{"jsonrpc": "2.0", "error": {"code": -32000, "message": "Something went wrong"}, "id": 1}'
"#,
        )
        .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let manifest = ToolManifest::parse(&format!(
            r#"{{
                "name": "error_tool",
                "description": "Error tool",
                "command": ["bash", "{}"],
                "input_schema": {{"type": "object", "properties": {{}}}}
            }}"#,
            script_path.display()
        ))
        .unwrap();

        let tool = ExternalTool::new(manifest);
        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("Something went wrong"));
    }

    #[test]
    fn test_load_external_tools_from_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let tools = load_external_tools_from(temp_dir.path().to_path_buf());
        assert!(tools.is_empty());
    }

    #[test]
    fn test_load_external_tools_from_with_manifests() {
        let temp_dir = TempDir::new().unwrap();

        // Create test manifests
        std::fs::write(
            temp_dir.path().join("tool_a.json"),
            r#"{
                "name": "tool_a",
                "description": "Tool A",
                "command": ["echo"],
                "input_schema": {"type": "object", "properties": {}}
            }"#,
        )
        .unwrap();

        std::fs::write(
            temp_dir.path().join("tool_b.json"),
            r#"{
                "name": "tool_b",
                "description": "Tool B",
                "command": ["echo"],
                "input_schema": {"type": "object", "properties": {}}
            }"#,
        )
        .unwrap();

        let tools = load_external_tools_from(temp_dir.path().to_path_buf());
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn test_request_id_increments() {
        let id1 = REQUEST_ID.load(Ordering::SeqCst);
        let _ = REQUEST_ID.fetch_add(1, Ordering::SeqCst);
        let id2 = REQUEST_ID.load(Ordering::SeqCst);
        assert_eq!(id2, id1 + 1);
    }
}
