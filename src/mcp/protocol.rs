// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! MCP protocol types and definitions
//!
//! Based on the Model Context Protocol specification:
//! https://spec.modelcontextprotocol.io/

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP protocol version
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// JSON-RPC 2.0 request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn parse_error() -> Self {
        Self {
            code: -32700,
            message: "Parse error".to_string(),
            data: None,
        }
    }

    pub fn invalid_request() -> Self {
        Self {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: None,
        }
    }

    pub fn method_not_found() -> Self {
        Self {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        }
    }

    pub fn invalid_params() -> Self {
        Self {
            code: -32602,
            message: "Invalid params".to_string(),
            data: None,
        }
    }

    pub fn internal_error() -> Self {
        Self {
            code: -32603,
            message: "Internal error".to_string(),
            data: None,
        }
    }
}

/// MCP server capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptsCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// MCP client capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootsCapability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Server information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// Client information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// Initialize request params
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: ClientInfo,
}

/// Initialize result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
}

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Tools list result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsListResult {
    pub tools: Vec<Tool>,
}

/// Tool call request params
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolParams {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

/// Tool call result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolResult {
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Tool content (text or image)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    #[serde(rename = "resource")]
    Resource { resource: ResourceContent },
}

/// Resource content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Protocol Version Tests =====

    #[test]
    fn test_protocol_version_format() {
        // Verify protocol version is set and follows date format (e.g., "2024-11-05")
        assert!(PROTOCOL_VERSION.contains('-'));
        // Should have at least year-month-day format
        assert!(PROTOCOL_VERSION.len() >= 10);
    }

    // ===== JsonRpcRequest Tests =====

    #[test]
    fn test_jsonrpc_request_serialization() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(1.into())),
            method: "test".to_string(),
            params: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"method\":\"test\""));
    }

    #[test]
    fn test_jsonrpc_request_deserialization() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "tools/list");
        assert!(req.params.is_none());
    }

    #[test]
    fn test_jsonrpc_request_with_params() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"test"}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert!(req.params.is_some());
        let params = req.params.unwrap();
        assert_eq!(params["name"], "test");
    }

    #[test]
    fn test_jsonrpc_request_with_string_id() {
        let json = r#"{"jsonrpc":"2.0","id":"abc-123","method":"test"}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert_eq!(req.id, Some(Value::String("abc-123".to_string())));
    }

    #[test]
    fn test_jsonrpc_request_clone() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(42.into())),
            method: "test".to_string(),
            params: Some(serde_json::json!({"key": "value"})),
        };

        let cloned = req.clone();
        assert_eq!(req.method, cloned.method);
        assert_eq!(req.id, cloned.id);
    }

    #[test]
    fn test_jsonrpc_request_debug() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "test".to_string(),
            params: None,
        };

        let debug_str = format!("{:?}", req);
        assert!(debug_str.contains("JsonRpcRequest"));
    }

    // ===== JsonRpcResponse Tests =====

    #[test]
    fn test_jsonrpc_response_success() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(1.into())),
            result: Some(serde_json::json!({"status": "ok"})),
            error: None,
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(1.into())),
            result: None,
            error: Some(JsonRpcError::method_not_found()),
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn test_jsonrpc_response_deserialization() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"data":"test"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();

        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    // ===== JsonRpcError Tests =====

    #[test]
    fn test_jsonrpc_error_codes() {
        assert_eq!(JsonRpcError::parse_error().code, -32700);
        assert_eq!(JsonRpcError::invalid_request().code, -32600);
        assert_eq!(JsonRpcError::method_not_found().code, -32601);
        assert_eq!(JsonRpcError::invalid_params().code, -32602);
        assert_eq!(JsonRpcError::internal_error().code, -32603);
    }

    #[test]
    fn test_jsonrpc_error_messages() {
        assert_eq!(JsonRpcError::parse_error().message, "Parse error");
        assert_eq!(JsonRpcError::invalid_request().message, "Invalid Request");
        assert_eq!(JsonRpcError::method_not_found().message, "Method not found");
        assert_eq!(JsonRpcError::invalid_params().message, "Invalid params");
        assert_eq!(JsonRpcError::internal_error().message, "Internal error");
    }

    #[test]
    fn test_jsonrpc_error_no_data() {
        let errors = vec![
            JsonRpcError::parse_error(),
            JsonRpcError::invalid_request(),
            JsonRpcError::method_not_found(),
            JsonRpcError::invalid_params(),
            JsonRpcError::internal_error(),
        ];

        for error in errors {
            assert!(error.data.is_none());
        }
    }

    #[test]
    fn test_jsonrpc_error_serialization() {
        let error = JsonRpcError {
            code: -32000,
            message: "Custom error".to_string(),
            data: Some(serde_json::json!({"details": "more info"})),
        };

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"code\":-32000"));
        assert!(json.contains("\"message\":\"Custom error\""));
        assert!(json.contains("\"data\""));
    }

    #[test]
    fn test_jsonrpc_error_clone() {
        let error = JsonRpcError::method_not_found();
        let cloned = error.clone();

        assert_eq!(error.code, cloned.code);
        assert_eq!(error.message, cloned.message);
    }

    // ===== ServerCapabilities Tests =====

    #[test]
    fn test_server_capabilities_empty() {
        let caps = ServerCapabilities {
            tools: None,
            prompts: None,
            resources: None,
        };

        let json = serde_json::to_string(&caps).unwrap();
        // All None fields should be skipped
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_server_capabilities_with_tools() {
        let caps = ServerCapabilities {
            tools: Some(ToolsCapability {
                list_changed: Some(true),
            }),
            prompts: None,
            resources: None,
        };

        let json = serde_json::to_value(&caps).unwrap();
        // list_changed is serialized as "list_changed" (snake_case by default)
        assert!(json["tools"]["list_changed"].as_bool().unwrap());
    }

    #[test]
    fn test_server_capabilities_full() {
        let caps = ServerCapabilities {
            tools: Some(ToolsCapability {
                list_changed: Some(false),
            }),
            prompts: Some(PromptsCapability {
                list_changed: Some(true),
            }),
            resources: Some(ResourcesCapability {
                subscribe: Some(true),
                list_changed: Some(false),
            }),
        };

        let json = serde_json::to_value(&caps).unwrap();
        assert!(json["tools"].is_object());
        assert!(json["prompts"].is_object());
        assert!(json["resources"].is_object());
    }

    // ===== ClientCapabilities Tests =====

    #[test]
    fn test_client_capabilities_empty() {
        let caps = ClientCapabilities {
            sampling: None,
            roots: None,
        };

        let json = serde_json::to_string(&caps).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_client_capabilities_with_roots() {
        let caps = ClientCapabilities {
            sampling: None,
            roots: Some(RootsCapability {
                list_changed: Some(true),
            }),
        };

        let json = serde_json::to_value(&caps).unwrap();
        assert!(json["roots"]["list_changed"].as_bool().unwrap());
    }

    // ===== ServerInfo and ClientInfo Tests =====

    #[test]
    fn test_server_info() {
        let info = ServerInfo {
            name: "ted".to_string(),
            version: "1.0.0".to_string(),
        };

        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["name"], "ted");
        assert_eq!(json["version"], "1.0.0");
    }

    #[test]
    fn test_client_info() {
        let info = ClientInfo {
            name: "test-client".to_string(),
            version: "2.0.0".to_string(),
        };

        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["name"], "test-client");
        assert_eq!(json["version"], "2.0.0");
    }

    // ===== InitializeParams Tests =====

    #[test]
    fn test_initialize_params_serialization() {
        let params = InitializeParams {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ClientCapabilities {
                sampling: None,
                roots: None,
            },
            client_info: ClientInfo {
                name: "test".to_string(),
                version: "1.0".to_string(),
            },
        };

        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["protocolVersion"], PROTOCOL_VERSION);
        assert!(json["clientInfo"].is_object());
    }

    #[test]
    fn test_initialize_params_deserialization() {
        let json = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "TestClient",
                "version": "1.0"
            }
        });

        let params: InitializeParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.protocol_version, "2024-11-05");
        assert_eq!(params.client_info.name, "TestClient");
    }

    // ===== InitializeResult Tests =====

    #[test]
    fn test_initialize_result_serialization() {
        let result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                prompts: None,
                resources: None,
            },
            server_info: ServerInfo {
                name: "ted".to_string(),
                version: "1.0.0".to_string(),
            },
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(json["serverInfo"]["name"], "ted");
    }

    // ===== Tool Tests =====

    #[test]
    fn test_tool_serialization() {
        let tool = Tool {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        };

        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["name"], "test_tool");
        assert_eq!(json["inputSchema"]["type"], "object");
    }

    #[test]
    fn test_tool_deserialization() {
        let json = serde_json::json!({
            "name": "file_read",
            "description": "Read a file",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"]
            }
        });

        let tool: Tool = serde_json::from_value(json).unwrap();
        assert_eq!(tool.name, "file_read");
        assert_eq!(tool.description, "Read a file");
    }

    // ===== ToolsListResult Tests =====

    #[test]
    fn test_tools_list_result_empty() {
        let result = ToolsListResult { tools: vec![] };

        let json = serde_json::to_value(&result).unwrap();
        assert!(json["tools"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_tools_list_result_with_tools() {
        let result = ToolsListResult {
            tools: vec![
                Tool {
                    name: "tool1".to_string(),
                    description: "First tool".to_string(),
                    input_schema: serde_json::json!({}),
                },
                Tool {
                    name: "tool2".to_string(),
                    description: "Second tool".to_string(),
                    input_schema: serde_json::json!({}),
                },
            ],
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["tools"].as_array().unwrap().len(), 2);
    }

    // ===== CallToolParams Tests =====

    #[test]
    fn test_call_tool_params_minimal() {
        let params = CallToolParams {
            name: "test".to_string(),
            arguments: None,
        };

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("\"name\":\"test\""));
        assert!(!json.contains("\"arguments\""));
    }

    #[test]
    fn test_call_tool_params_with_arguments() {
        let params = CallToolParams {
            name: "file_read".to_string(),
            arguments: Some(serde_json::json!({"path": "/tmp/test.txt"})),
        };

        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["name"], "file_read");
        assert_eq!(json["arguments"]["path"], "/tmp/test.txt");
    }

    #[test]
    fn test_call_tool_params_deserialization() {
        let json = r#"{"name":"shell","arguments":{"command":"ls -la"}}"#;
        let params: CallToolParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.name, "shell");
        assert!(params.arguments.is_some());
    }

    // ===== CallToolResult Tests =====

    #[test]
    fn test_call_tool_result_success() {
        let result = CallToolResult {
            content: vec![ToolContent::Text {
                text: "Success".to_string(),
            }],
            is_error: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"content\""));
        assert!(!json.contains("\"isError\""));
    }

    #[test]
    fn test_call_tool_result_error() {
        let result = CallToolResult {
            content: vec![ToolContent::Text {
                text: "Error occurred".to_string(),
            }],
            is_error: Some(true),
        };

        let json = serde_json::to_value(&result).unwrap();
        assert!(json["is_error"].as_bool().unwrap());
    }

    // ===== ToolContent Tests =====

    #[test]
    fn test_tool_content_text() {
        let content = ToolContent::Text {
            text: "Hello world".to_string(),
        };

        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "Hello world");
    }

    #[test]
    fn test_tool_content_image() {
        let content = ToolContent::Image {
            data: "base64data".to_string(),
            mime_type: "image/png".to_string(),
        };

        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json["type"], "image");
        assert_eq!(json["mime_type"], "image/png");
    }

    #[test]
    fn test_tool_content_resource() {
        let content = ToolContent::Resource {
            resource: ResourceContent {
                uri: "file:///tmp/test.txt".to_string(),
                mime_type: Some("text/plain".to_string()),
                text: Some("File content".to_string()),
                blob: None,
            },
        };

        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json["type"], "resource");
        assert_eq!(json["resource"]["uri"], "file:///tmp/test.txt");
    }

    // ===== ResourceContent Tests =====

    #[test]
    fn test_resource_content_minimal() {
        let resource = ResourceContent {
            uri: "file:///test".to_string(),
            mime_type: None,
            text: None,
            blob: None,
        };

        let json = serde_json::to_string(&resource).unwrap();
        assert!(json.contains("\"uri\":\"file:///test\""));
        assert!(!json.contains("\"mimeType\""));
        assert!(!json.contains("\"text\""));
        assert!(!json.contains("\"blob\""));
    }

    #[test]
    fn test_resource_content_with_text() {
        let resource = ResourceContent {
            uri: "file:///test.txt".to_string(),
            mime_type: Some("text/plain".to_string()),
            text: Some("Hello".to_string()),
            blob: None,
        };

        let json = serde_json::to_value(&resource).unwrap();
        assert_eq!(json["text"], "Hello");
        assert_eq!(json["mime_type"], "text/plain");
    }

    #[test]
    fn test_resource_content_with_blob() {
        let resource = ResourceContent {
            uri: "file:///test.bin".to_string(),
            mime_type: Some("application/octet-stream".to_string()),
            text: None,
            blob: Some("base64encodeddata".to_string()),
        };

        let json = serde_json::to_value(&resource).unwrap();
        assert_eq!(json["blob"], "base64encodeddata");
    }

    // ===== Capability Structs Tests =====

    #[test]
    fn test_tools_capability() {
        let cap = ToolsCapability {
            list_changed: Some(true),
        };

        let json = serde_json::to_value(&cap).unwrap();
        assert!(json["list_changed"].as_bool().unwrap());
    }

    #[test]
    fn test_prompts_capability() {
        let cap = PromptsCapability {
            list_changed: Some(false),
        };

        let json = serde_json::to_value(&cap).unwrap();
        assert!(!json["list_changed"].as_bool().unwrap());
    }

    #[test]
    fn test_resources_capability() {
        let cap = ResourcesCapability {
            subscribe: Some(true),
            list_changed: Some(false),
        };

        let json = serde_json::to_value(&cap).unwrap();
        assert!(json["subscribe"].as_bool().unwrap());
        assert!(!json["list_changed"].as_bool().unwrap());
    }

    #[test]
    fn test_roots_capability() {
        let cap = RootsCapability {
            list_changed: Some(true),
        };

        let json = serde_json::to_value(&cap).unwrap();
        assert!(json["list_changed"].as_bool().unwrap());
    }

    // ===== Clone and Debug Tests =====

    #[test]
    fn test_all_types_are_cloneable() {
        // Just verify these compile - they should all be Clone
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "test".to_string(),
            params: None,
        };
        let _ = req.clone();

        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: None,
            result: None,
            error: None,
        };
        let _ = resp.clone();

        let tool = Tool {
            name: "test".to_string(),
            description: "test".to_string(),
            input_schema: Value::Null,
        };
        let _ = tool.clone();
    }

    #[test]
    fn test_all_types_are_debuggable() {
        // Just verify these compile - they should all be Debug
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "test".to_string(),
            params: None,
        };
        let _ = format!("{:?}", req);

        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: None,
            result: None,
            error: None,
        };
        let _ = format!("{:?}", resp);

        let error = JsonRpcError::internal_error();
        let _ = format!("{:?}", error);
    }

    // ===== Roundtrip Tests =====

    #[test]
    fn test_jsonrpc_request_roundtrip() {
        let original = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(123.into())),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({"name": "test", "arguments": {}})),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: JsonRpcRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(original.jsonrpc, deserialized.jsonrpc);
        assert_eq!(original.method, deserialized.method);
        assert_eq!(original.id, deserialized.id);
    }

    #[test]
    fn test_tool_roundtrip() {
        let original = Tool {
            name: "test_tool".to_string(),
            description: "A description".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                }
            }),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Tool = serde_json::from_str(&json).unwrap();

        assert_eq!(original.name, deserialized.name);
        assert_eq!(original.description, deserialized.description);
    }
}
