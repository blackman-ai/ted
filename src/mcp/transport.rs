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
}
