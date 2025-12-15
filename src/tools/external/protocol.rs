// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! JSON-RPC protocol for external tools
//!
//! External tools communicate via stdio using a simplified JSON-RPC 2.0 protocol.
//!
//! # Request Format
//!
//! ```json
//! {
//!   "jsonrpc": "2.0",
//!   "method": "execute",
//!   "params": { /* tool input */ },
//!   "id": 1
//! }
//! ```
//!
//! # Response Format
//!
//! ```json
//! {
//!   "jsonrpc": "2.0",
//!   "result": {
//!     "output": "Tool output text",
//!     "is_error": false,
//!     "recall": {
//!       "files_read": ["src/main.rs"],
//!       "files_written": ["output.txt"],
//!       "files_edited": []
//!     }
//!   },
//!   "id": 1
//! }
//! ```
//!
//! # Error Response Format
//!
//! ```json
//! {
//!   "jsonrpc": "2.0",
//!   "error": {
//!     "code": -32000,
//!     "message": "Error description"
//!   },
//!   "id": 1
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// JSON-RPC request sent to external tool.
#[derive(Debug, Clone, Serialize)]
pub struct Request {
    /// JSON-RPC version (always "2.0")
    pub jsonrpc: &'static str,
    /// Method name (always "execute" for tool execution)
    pub method: &'static str,
    /// Tool input parameters
    pub params: serde_json::Value,
    /// Request ID
    pub id: u64,
}

impl Request {
    /// Create a new execute request.
    pub fn execute(params: serde_json::Value, id: u64) -> Self {
        Self {
            jsonrpc: "2.0",
            method: "execute",
            params,
            id,
        }
    }

    /// Serialize to JSON string with newline.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// JSON-RPC response from external tool.
#[derive(Debug, Clone, Deserialize)]
pub struct Response {
    /// JSON-RPC version
    pub jsonrpc: String,
    /// Successful result
    pub result: Option<ToolResultPayload>,
    /// Error response
    pub error: Option<ErrorPayload>,
    /// Request ID
    pub id: u64,
}

impl Response {
    /// Parse a response from JSON string.
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Check if this is an error response.
    pub fn is_error(&self) -> bool {
        self.error.is_some() || self.result.as_ref().is_some_and(|r| r.is_error)
    }

    /// Get the output text.
    pub fn output(&self) -> String {
        if let Some(error) = &self.error {
            error.message.clone()
        } else if let Some(result) = &self.result {
            result.output.clone()
        } else {
            "No output".to_string()
        }
    }

    /// Get recall data from the response.
    pub fn recall(&self) -> Option<&RecallPayload> {
        self.result.as_ref().and_then(|r| r.recall.as_ref())
    }
}

/// Successful result payload.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolResultPayload {
    /// Output text to return to the LLM
    pub output: String,

    /// Whether this is an error result
    #[serde(default)]
    pub is_error: bool,

    /// Optional recall data for memory integration
    #[serde(default)]
    pub recall: Option<RecallPayload>,
}

/// Recall data for memory integration.
///
/// External tools can report which files they accessed so that
/// the memory system can boost those files' retention scores.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RecallPayload {
    /// Files that were read by the tool
    #[serde(default)]
    pub files_read: Vec<PathBuf>,

    /// Files that were written/created by the tool
    #[serde(default)]
    pub files_written: Vec<PathBuf>,

    /// Files that were edited (modified in place) by the tool
    #[serde(default)]
    pub files_edited: Vec<PathBuf>,

    /// Additional files that were searched/matched
    #[serde(default)]
    pub search_matches: Vec<PathBuf>,
}

impl RecallPayload {
    /// Check if there's any recall data.
    pub fn is_empty(&self) -> bool {
        self.files_read.is_empty()
            && self.files_written.is_empty()
            && self.files_edited.is_empty()
            && self.search_matches.is_empty()
    }

    /// Get all affected file paths.
    pub fn all_paths(&self) -> Vec<&PathBuf> {
        let mut paths = Vec::new();
        paths.extend(&self.files_read);
        paths.extend(&self.files_written);
        paths.extend(&self.files_edited);
        paths.extend(&self.search_matches);
        paths
    }
}

/// Error payload in response.
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorPayload {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Optional additional data
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

/// Standard JSON-RPC error codes.
pub mod error_codes {
    /// Parse error
    pub const PARSE_ERROR: i32 = -32700;
    /// Invalid request
    pub const INVALID_REQUEST: i32 = -32600;
    /// Method not found
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid params
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal error
    pub const INTERNAL_ERROR: i32 = -32603;
    /// Server error (application-specific)
    pub const SERVER_ERROR: i32 = -32000;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_execute() {
        let request = Request::execute(serde_json::json!({"path": "test.txt"}), 1);

        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.method, "execute");
        assert_eq!(request.id, 1);
        assert_eq!(request.params["path"], "test.txt");
    }

    #[test]
    fn test_request_to_json() {
        let request = Request::execute(serde_json::json!({"key": "value"}), 42);
        let json = request.to_json();

        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"execute\""));
        assert!(json.contains("\"id\":42"));
        assert!(json.contains("\"key\":\"value\""));
    }

    #[test]
    fn test_response_parse_success() {
        let json = r#"{
            "jsonrpc": "2.0",
            "result": {
                "output": "Success!",
                "is_error": false
            },
            "id": 1
        }"#;

        let response = Response::parse(json).unwrap();

        assert_eq!(response.jsonrpc, "2.0");
        assert_eq!(response.id, 1);
        assert!(!response.is_error());
        assert_eq!(response.output(), "Success!");
        assert!(response.recall().is_none());
    }

    #[test]
    fn test_response_parse_with_recall() {
        let json = r#"{
            "jsonrpc": "2.0",
            "result": {
                "output": "Done",
                "recall": {
                    "files_read": ["src/main.rs"],
                    "files_written": ["output.txt"]
                }
            },
            "id": 2
        }"#;

        let response = Response::parse(json).unwrap();

        let recall = response.recall().unwrap();
        assert_eq!(recall.files_read.len(), 1);
        assert_eq!(recall.files_written.len(), 1);
        assert!(recall.files_edited.is_empty());
    }

    #[test]
    fn test_response_parse_error() {
        let json = r#"{
            "jsonrpc": "2.0",
            "error": {
                "code": -32000,
                "message": "Something went wrong"
            },
            "id": 3
        }"#;

        let response = Response::parse(json).unwrap();

        assert!(response.is_error());
        assert_eq!(response.output(), "Something went wrong");
        assert!(response.result.is_none());
    }

    #[test]
    fn test_response_result_is_error() {
        let json = r#"{
            "jsonrpc": "2.0",
            "result": {
                "output": "Failed to process",
                "is_error": true
            },
            "id": 4
        }"#;

        let response = Response::parse(json).unwrap();

        assert!(response.is_error());
        assert_eq!(response.output(), "Failed to process");
    }

    #[test]
    fn test_recall_payload_is_empty() {
        let empty = RecallPayload::default();
        assert!(empty.is_empty());

        let with_read = RecallPayload {
            files_read: vec![PathBuf::from("test.rs")],
            ..Default::default()
        };
        assert!(!with_read.is_empty());
    }

    #[test]
    fn test_recall_payload_all_paths() {
        let recall = RecallPayload {
            files_read: vec![PathBuf::from("a.rs")],
            files_written: vec![PathBuf::from("b.rs")],
            files_edited: vec![PathBuf::from("c.rs")],
            search_matches: vec![PathBuf::from("d.rs")],
        };

        let paths = recall.all_paths();
        assert_eq!(paths.len(), 4);
    }

    #[test]
    fn test_response_no_output() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 5
        }"#;

        let response = Response::parse(json).unwrap();
        assert_eq!(response.output(), "No output");
    }

    #[test]
    fn test_error_payload_with_data() {
        let json = r#"{
            "jsonrpc": "2.0",
            "error": {
                "code": -32602,
                "message": "Invalid params",
                "data": {"param": "path", "reason": "missing"}
            },
            "id": 6
        }"#;

        let response = Response::parse(json).unwrap();
        let error = response.error.unwrap();

        assert_eq!(error.code, error_codes::INVALID_PARAMS);
        assert!(error.data.is_some());
    }

    #[test]
    fn test_recall_payload_clone() {
        let recall = RecallPayload {
            files_read: vec![PathBuf::from("test.rs")],
            ..Default::default()
        };

        let cloned = recall.clone();
        assert_eq!(cloned.files_read, recall.files_read);
    }

    #[test]
    fn test_request_clone() {
        let request = Request::execute(serde_json::json!({"x": 1}), 10);
        let cloned = request.clone();

        assert_eq!(cloned.id, request.id);
        assert_eq!(cloned.params, request.params);
    }

    #[test]
    fn test_response_clone() {
        let json = r#"{
            "jsonrpc": "2.0",
            "result": {"output": "test"},
            "id": 1
        }"#;

        let response = Response::parse(json).unwrap();
        let cloned = response.clone();

        assert_eq!(cloned.output(), response.output());
    }
}
