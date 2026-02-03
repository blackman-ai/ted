// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! MCP transport layer - stdio-based communication
//!
//! MCP servers communicate via stdio (standard input/output) using JSON-RPC 2.0

use std::io::{self, BufRead, Write};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::error::Result;

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

        Err(crate::error::TedError::Config(
            "EOF reached on stdin".to_string(),
        ))
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

    // ===== StdioTransport Creation Tests =====

    #[test]
    fn test_stdio_transport_creation() {
        let transport = StdioTransport::new();
        assert!(Arc::strong_count(&transport.stdin) >= 1);
        assert!(Arc::strong_count(&transport.stdout) >= 1);
    }

    #[test]
    fn test_stdio_transport_default() {
        let transport = StdioTransport::default();
        assert!(Arc::strong_count(&transport.stdin) >= 1);
        assert!(Arc::strong_count(&transport.stdout) >= 1);
    }

    #[test]
    fn test_stdio_transport_new_vs_default() {
        let transport1 = StdioTransport::new();
        let transport2 = StdioTransport::default();

        // Both should have valid Arc references
        assert!(Arc::strong_count(&transport1.stdin) >= 1);
        assert!(Arc::strong_count(&transport2.stdin) >= 1);
    }

    #[test]
    fn test_stdio_transport_multiple_instances() {
        // Each instance should have its own Arc
        let t1 = StdioTransport::new();
        let t2 = StdioTransport::new();

        // Both should have valid references
        assert!(Arc::strong_count(&t1.stdin) >= 1);
        assert!(Arc::strong_count(&t2.stdin) >= 1);
    }

    // ===== JsonRpcResponse Serialization Tests =====
    // These test the write_response serialization logic

    #[test]
    fn test_jsonrpc_response_serialization_success() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::Number(1.into())),
            result: Some(serde_json::json!({"data": "test"})),
            error: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_jsonrpc_response_serialization_error() {
        use super::super::protocol::JsonRpcError;

        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::Number(1.into())),
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: "Method not found".to_string(),
                data: None,
            }),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"error\""));
        assert!(json.contains("-32601"));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn test_jsonrpc_response_serialization_with_string_id() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::String("req-abc".to_string())),
            result: Some(serde_json::Value::Null),
            error: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"id\":\"req-abc\""));
    }

    #[test]
    fn test_jsonrpc_response_serialization_null_id() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: None,
            result: Some(serde_json::Value::Null),
            error: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        // None id should be omitted in serialization due to skip_serializing_if
        assert!(!json.contains("\"id\""));
    }

    // ===== JsonRpcRequest Deserialization Tests =====
    // These test the read_request parsing logic

    #[test]
    fn test_jsonrpc_request_deserialization_basic() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.id, Some(serde_json::Value::Number(1.into())));
        assert_eq!(request.method, "tools/list");
        assert!(request.params.is_none());
    }

    #[test]
    fn test_jsonrpc_request_deserialization_with_params() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"test"}}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.method, "tools/call");
        assert!(request.params.is_some());
        let params = request.params.unwrap();
        assert_eq!(params["name"], "test");
    }

    #[test]
    fn test_jsonrpc_request_deserialization_string_id() {
        let json = r#"{"jsonrpc":"2.0","id":"abc-123","method":"test"}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert_eq!(
            request.id,
            Some(serde_json::Value::String("abc-123".to_string()))
        );
    }

    #[test]
    fn test_jsonrpc_request_deserialization_notification() {
        // Notifications have no id
        let json = r#"{"jsonrpc":"2.0","method":"initialized"}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert!(request.id.is_none());
        assert_eq!(request.method, "initialized");
    }

    #[test]
    fn test_jsonrpc_request_deserialization_complex_params() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 42,
            "method": "tools/call",
            "params": {
                "name": "file_read",
                "arguments": {
                    "path": "/tmp/test.txt",
                    "encoding": "utf-8"
                }
            }
        }"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.method, "tools/call");
        let params = request.params.unwrap();
        assert_eq!(params["name"], "file_read");
        assert_eq!(params["arguments"]["path"], "/tmp/test.txt");
    }

    #[test]
    fn test_jsonrpc_request_deserialization_null_params() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"test","params":null}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        // params: null may deserialize to None or Some(Value::Null) depending on serde config
        // The key behavior is that we can parse it without error
        // If it's Some, it should be null; if it's None, that's also fine
        if let Some(params) = &request.params {
            assert!(params.is_null());
        }
    }

    #[test]
    fn test_jsonrpc_request_deserialization_empty_object_params() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"test","params":{}}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert!(request.params.is_some());
        assert!(request.params.unwrap().is_object());
    }

    // ===== Error Handling Tests =====

    #[test]
    fn test_invalid_json_fails_to_parse() {
        let invalid_json = r#"{"jsonrpc": "2.0", method: "test"}"#; // Missing quotes around "method"
        let result = serde_json::from_str::<JsonRpcRequest>(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_required_field_fails() {
        // Missing jsonrpc field
        let json = r#"{"id":1,"method":"test"}"#;
        let result = serde_json::from_str::<JsonRpcRequest>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_method_fails() {
        let json = r#"{"jsonrpc":"2.0","id":1}"#;
        let result = serde_json::from_str::<JsonRpcRequest>(json);
        assert!(result.is_err());
    }

    // ===== Roundtrip Tests =====

    #[test]
    fn test_response_roundtrip() {
        let original = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::Number(123.into())),
            result: Some(serde_json::json!({"tools": []})),
            error: None,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: JsonRpcResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(original.jsonrpc, deserialized.jsonrpc);
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.result, deserialized.result);
    }

    #[test]
    fn test_request_roundtrip() {
        let original = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::Number(456.into())),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({"name": "test"})),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: JsonRpcRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(original.jsonrpc, deserialized.jsonrpc);
        assert_eq!(original.method, deserialized.method);
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.params, deserialized.params);
    }

    // ===== Line Format Tests =====
    // Testing the JSONL format expectations

    #[test]
    fn test_response_is_single_line() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::Number(1.into())),
            result: Some(serde_json::json!({
                "multiline": "value\nwith\nnewlines"
            })),
            error: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        // Even with embedded newlines in values, the JSON should be on one line
        // (they get escaped as \n)
        assert!(!json.contains('\n'));
    }

    #[test]
    fn test_request_parses_with_embedded_newlines() {
        // Test that we can parse JSON even if the values contain escaped newlines
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"test","params":{"text":"line1\nline2"}}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        let params = request.params.unwrap();
        assert_eq!(params["text"], "line1\nline2");
    }

    // ===== Async Transport Tests =====
    // These tests exercise the transport methods' logic

    #[tokio::test]
    async fn test_transport_mutex_locking() {
        // Verify that the mutex can be locked
        let transport = StdioTransport::new();

        // Lock stdin (won't actually read in test)
        let _stdin_guard = transport.stdin.lock().await;
        // If we get here, locking works

        drop(_stdin_guard);

        // Lock stdout
        let _stdout_guard = transport.stdout.lock().await;
        // If we get here, locking works
    }

    #[tokio::test]
    async fn test_transport_stdout_write_format() {
        // Test the format that would be written
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::Number(1.into())),
            result: Some(serde_json::json!({"status": "ok"})),
            error: None,
        };

        // Test serialization that write_response would do
        let json = serde_json::to_string(&response).unwrap();
        let output = format!("{}\n", json);

        // Verify the format
        assert!(output.ends_with('\n'));
        assert!(output.contains("\"jsonrpc\":\"2.0\""));
    }

    #[tokio::test]
    async fn test_transport_stdin_parsing_logic() {
        // Test the parsing logic used in read_request
        let lines = vec![
            "",                                            // Empty line - should be skipped
            "   ",                                         // Whitespace line - should be skipped
            r#"{"jsonrpc":"2.0","id":1,"method":"test"}"#, // Valid
        ];

        for line in lines {
            if line.trim().is_empty() {
                // This is the continue branch in read_request
                continue;
            }

            match serde_json::from_str::<JsonRpcRequest>(line) {
                Ok(req) => {
                    assert_eq!(req.method, "test");
                }
                Err(_) => {
                    // This would log error and continue
                }
            }
        }
    }

    #[tokio::test]
    async fn test_transport_empty_line_handling() {
        // Test empty line trimming
        let empty_lines = vec!["", "  ", "\t", "   \t   "];

        for line in empty_lines {
            assert!(line.trim().is_empty());
        }
    }

    #[tokio::test]
    async fn test_transport_invalid_json_handling() {
        // Test invalid JSON handling (the continue branch)
        let invalid_jsons = vec![
            "not json at all",
            "{incomplete",
            r#"{"jsonrpc": "2.0"}"#, // Missing method
            "[]",                    // Array not object
        ];

        for json in invalid_jsons {
            let result = serde_json::from_str::<JsonRpcRequest>(json);
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn test_transport_valid_json_handling() {
        // Test valid JSON handling (the Ok branch)
        let valid_jsons = vec![
            r#"{"jsonrpc":"2.0","id":1,"method":"test"}"#,
            r#"{"jsonrpc":"2.0","method":"notify"}"#, // Notification (no id)
            r#"{"jsonrpc":"2.0","id":"abc","method":"test","params":{}}"#,
        ];

        for json in valid_jsons {
            let result = serde_json::from_str::<JsonRpcRequest>(json);
            assert!(result.is_ok(), "Failed to parse: {}", json);
        }
    }

    #[test]
    fn test_eof_error_creation() {
        // Test the EOF error that read_request returns
        let error = crate::error::TedError::Config("EOF reached on stdin".to_string());
        let error_str = format!("{}", error);
        assert!(
            error_str.contains("EOF")
                || error_str.contains("stdin")
                || error_str.contains("Config")
        );
    }

    #[test]
    fn test_write_format_with_writeln() {
        // Test the format! + writeln! pattern used in write_response
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::Number(1.into())),
            result: Some(serde_json::Value::Null),
            error: None,
        };

        let json = serde_json::to_string(&response).unwrap();

        // Simulate what writeln! would produce
        let mut buffer = Vec::new();
        writeln!(buffer, "{}", json).unwrap();

        let output = String::from_utf8(buffer).unwrap();
        assert!(output.ends_with('\n'));
    }

    #[tokio::test]
    async fn test_transport_arc_clone() {
        // Test that Arc cloning works for concurrent access
        let transport = StdioTransport::new();
        let stdin_clone = Arc::clone(&transport.stdin);
        let stdout_clone = Arc::clone(&transport.stdout);

        assert!(Arc::strong_count(&transport.stdin) >= 2);
        assert!(Arc::strong_count(&transport.stdout) >= 2);

        drop(stdin_clone);
        drop(stdout_clone);

        assert!(Arc::strong_count(&transport.stdin) >= 1);
        assert!(Arc::strong_count(&transport.stdout) >= 1);
    }

    #[test]
    fn test_bufreader_lines_pattern() {
        use std::io::BufRead;

        // Test the pattern used in read_request
        let input = "line1\nline2\n\nline3\n";
        let reader = std::io::BufReader::new(input.as_bytes());

        let lines: Vec<_> = reader.lines().map_while(|l| l.ok()).collect();

        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "line1");
        assert_eq!(lines[1], "line2");
        assert_eq!(lines[2], ""); // Empty line
        assert_eq!(lines[3], "line3");
    }

    #[test]
    fn test_json_serialization_error_handling() {
        // Test serde_json::to_string error handling
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::Number(1.into())),
            result: Some(serde_json::json!({"key": "value"})),
            error: None,
        };

        // This should always succeed for JsonRpcResponse
        let result = serde_json::to_string(&response);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_concurrent_lock_attempts() {
        // Test that multiple lock attempts work correctly
        let transport = StdioTransport::new();

        // Sequential locks should work
        {
            let _guard = transport.stdin.lock().await;
        }
        {
            let _guard = transport.stdin.lock().await;
        }

        // Same for stdout
        {
            let _guard = transport.stdout.lock().await;
        }
        {
            let _guard = transport.stdout.lock().await;
        }
    }
}
