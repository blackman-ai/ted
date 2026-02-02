// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! MCP server implementation

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::protocol::*;
use super::transport::StdioTransport;
use crate::error::Result;
use crate::tools::{Tool as TedTool, ToolContext, ToolExecutor, ToolOutput};

/// Adapter to expose Ted tools through MCP protocol
struct TedToolAdapter {
    tool: Arc<dyn TedTool>,
    name: String,
    description: String,
    parameters: Value,
}

impl TedToolAdapter {
    fn new(tool: Arc<dyn TedTool>) -> Self {
        let def = tool.definition();
        Self {
            tool,
            name: def.name.clone(),
            description: def.description.clone(),
            parameters: serde_json::to_value(&def.input_schema)
                .unwrap_or(Value::Object(serde_json::Map::new())),
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        self.parameters.clone()
    }
}

/// MCP server state
pub struct McpServer {
    /// Available tools (wrapped in adapters)
    tools: Arc<RwLock<HashMap<String, TedToolAdapter>>>,
    /// Server initialized
    initialized: Arc<RwLock<bool>>,
    /// Transport layer
    transport: Arc<StdioTransport>,
    /// Tool executor (reserved for future use)
    #[allow(dead_code)]
    executor: Arc<ToolExecutor>,
}

impl McpServer {
    /// Create a new MCP server
    pub fn new(executor: ToolExecutor) -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            initialized: Arc::new(RwLock::new(false)),
            transport: Arc::new(StdioTransport::new()),
            executor: Arc::new(executor),
        }
    }

    /// Register a tool with the MCP server
    pub async fn register_tool(&self, tool: Arc<dyn TedTool>) {
        let adapter = TedToolAdapter::new(tool);
        let name = adapter.name().to_string();
        let mut tools = self.tools.write().await;
        tools.insert(name, adapter);
    }

    /// Run the MCP server (main loop)
    pub async fn run(&self) -> Result<()> {
        tracing::info!("[MCP] Starting Model Context Protocol server");

        loop {
            // Read request from stdin
            let request = match self.transport.read_request().await {
                Ok(req) => req,
                Err(e) => {
                    tracing::error!("[MCP] Failed to read request: {}", e);
                    break;
                }
            };

            tracing::debug!("[MCP] Received request: {}", request.method);

            // Handle request
            let response = self.handle_request(request).await;

            // Write response to stdout
            if let Err(e) = self.transport.write_response(&response).await {
                tracing::error!("[MCP] Failed to write response: {}", e);
                break;
            }
        }

        Ok(())
    }

    /// Handle a JSON-RPC request
    async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let method = &request.method;

        match method.as_str() {
            "initialize" => self.handle_initialize(request).await,
            "initialized" => self.handle_initialized(request).await,
            "tools/list" => self.handle_tools_list(request).await,
            "tools/call" => self.handle_tools_call(request).await,
            _ => Self::error_response(request.id, JsonRpcError::method_not_found()),
        }
    }

    /// Handle initialize request
    async fn handle_initialize(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let params: InitializeParams = match request.params {
            Some(ref p) => match serde_json::from_value(p.clone()) {
                Ok(params) => params,
                Err(_) => {
                    return Self::error_response(request.id, JsonRpcError::invalid_params());
                }
            },
            None => {
                return Self::error_response(request.id, JsonRpcError::invalid_params());
            }
        };

        tracing::info!(
            "[MCP] Initialize from client: {} v{}",
            params.client_info.name,
            params.client_info.version
        );

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
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        Self::success_response(request.id, serde_json::to_value(result).unwrap())
    }

    /// Handle initialized notification
    async fn handle_initialized(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let mut initialized = self.initialized.write().await;
        *initialized = true;

        tracing::info!("[MCP] Server initialized");

        // Notification - no response needed
        Self::success_response(request.id, Value::Null)
    }

    /// Handle tools/list request
    async fn handle_tools_list(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let tools = self.tools.read().await;

        let tool_list: Vec<Tool> = tools
            .values()
            .map(|adapter| Tool {
                name: adapter.name().to_string(),
                description: adapter.description().to_string(),
                input_schema: adapter.parameters(),
            })
            .collect();

        let result = ToolsListResult { tools: tool_list };

        Self::success_response(request.id, serde_json::to_value(result).unwrap())
    }

    /// Handle tools/call request
    async fn handle_tools_call(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let params: CallToolParams = match request.params {
            Some(ref p) => match serde_json::from_value(p.clone()) {
                Ok(params) => params,
                Err(_) => {
                    return Self::error_response(request.id, JsonRpcError::invalid_params());
                }
            },
            None => {
                return Self::error_response(request.id, JsonRpcError::invalid_params());
            }
        };

        tracing::info!("[MCP] Calling tool: {}", params.name);

        let tools = self.tools.read().await;

        let adapter = match tools.get(&params.name) {
            Some(t) => t,
            None => {
                return Self::error_response(
                    request.id,
                    JsonRpcError {
                        code: -32000,
                        message: format!("Tool not found: {}", params.name),
                        data: None,
                    },
                );
            }
        };

        let tool = adapter.tool.clone();
        drop(tools);

        // Execute the tool with a basic context
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let context = ToolContext::new(
            cwd.clone(),
            Some(cwd),
            uuid::Uuid::new_v4(),
            false, // Not in trust mode
        );
        let args = params
            .arguments
            .unwrap_or(Value::Object(serde_json::Map::new()));
        let tool_use_id = uuid::Uuid::new_v4().to_string();

        let result = match tool.execute(tool_use_id, args, &context).await {
            Ok(result) => result,
            Err(e) => {
                return Self::error_response(
                    request.id,
                    JsonRpcError {
                        code: -32001,
                        message: format!("Tool execution failed: {}", e),
                        data: None,
                    },
                );
            }
        };

        let (text, is_error) = match result.output {
            ToolOutput::Success(s) => (s, None),
            ToolOutput::Error(s) => (s, Some(true)),
        };

        let call_result = CallToolResult {
            content: vec![ToolContent::Text { text }],
            is_error,
        };

        Self::success_response(request.id, serde_json::to_value(call_result).unwrap())
    }

    /// Create a success response
    fn success_response(id: Option<Value>, result: Value) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response
    fn error_response(id: Option<Value>, error: JsonRpcError) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== McpServer Creation Tests =====

    #[tokio::test]
    async fn test_mcp_server_creation() {
        use std::env;
        let context = crate::tools::ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );
        let executor = ToolExecutor::new(context, false);
        let server = McpServer::new(executor);

        let initialized = server.initialized.read().await;
        assert!(!*initialized);
    }

    #[tokio::test]
    async fn test_mcp_server_initial_tools_empty() {
        use std::env;
        let context = crate::tools::ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );
        let executor = ToolExecutor::new(context, false);
        let server = McpServer::new(executor);

        let tools = server.tools.read().await;
        assert!(tools.is_empty());
    }

    // ===== Response Helper Tests =====

    #[test]
    fn test_success_response() {
        let response = McpServer::success_response(
            Some(Value::Number(1.into())),
            Value::String("test".to_string()),
        );

        assert_eq!(response.jsonrpc, "2.0");
        assert_eq!(response.id, Some(Value::Number(1.into())));
        assert!(response.error.is_none());
        assert!(response.result.is_some());
    }

    #[test]
    fn test_success_response_with_null_id() {
        let response = McpServer::success_response(None, Value::String("test".to_string()));

        assert_eq!(response.jsonrpc, "2.0");
        assert!(response.id.is_none());
        assert!(response.result.is_some());
    }

    #[test]
    fn test_success_response_with_object_result() {
        let result = serde_json::json!({
            "tools": [
                {"name": "test", "description": "A test tool"}
            ]
        });
        let response = McpServer::success_response(Some(Value::Number(1.into())), result.clone());

        assert_eq!(response.result, Some(result));
    }

    #[test]
    fn test_error_response() {
        let response = McpServer::error_response(
            Some(Value::Number(1.into())),
            JsonRpcError::method_not_found(),
        );

        assert_eq!(response.jsonrpc, "2.0");
        assert!(response.result.is_none());
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[test]
    fn test_error_response_with_null_id() {
        let response = McpServer::error_response(None, JsonRpcError::invalid_params());

        assert_eq!(response.jsonrpc, "2.0");
        assert!(response.id.is_none());
        assert!(response.error.is_some());
    }

    #[test]
    fn test_error_response_parse_error() {
        let response =
            McpServer::error_response(Some(Value::Number(1.into())), JsonRpcError::parse_error());

        let error = response.error.unwrap();
        assert_eq!(error.code, -32700);
        assert_eq!(error.message, "Parse error");
    }

    #[test]
    fn test_error_response_invalid_request() {
        let response = McpServer::error_response(
            Some(Value::Number(1.into())),
            JsonRpcError::invalid_request(),
        );

        let error = response.error.unwrap();
        assert_eq!(error.code, -32600);
    }

    #[test]
    fn test_error_response_internal_error() {
        let response = McpServer::error_response(
            Some(Value::Number(1.into())),
            JsonRpcError::internal_error(),
        );

        let error = response.error.unwrap();
        assert_eq!(error.code, -32603);
    }

    #[test]
    fn test_error_response_custom_error() {
        let custom_error = JsonRpcError {
            code: -32000,
            message: "Tool not found: unknown_tool".to_string(),
            data: Some(serde_json::json!({"tool": "unknown_tool"})),
        };
        let response = McpServer::error_response(Some(Value::Number(1.into())), custom_error);

        let error = response.error.unwrap();
        assert_eq!(error.code, -32000);
        assert!(error.message.contains("Tool not found"));
        assert!(error.data.is_some());
    }

    // ===== TedToolAdapter Tests =====

    // Note: We can't easily test TedToolAdapter directly since it requires
    // creating concrete Tool implementations. However, we can test through
    // the server's register_tool method using test utilities.

    // ===== Handle Request Tests =====

    #[tokio::test]
    async fn test_handle_request_method_not_found() {
        use std::env;
        let context = crate::tools::ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );
        let executor = ToolExecutor::new(context, false);
        let server = McpServer::new(executor);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(1.into())),
            method: "unknown/method".to_string(),
            params: None,
        };

        let response = server.handle_request(request).await;

        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn test_handle_initialized_sets_flag() {
        use std::env;
        let context = crate::tools::ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );
        let executor = ToolExecutor::new(context, false);
        let server = McpServer::new(executor);

        // Initially not initialized
        assert!(!*server.initialized.read().await);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(1.into())),
            method: "initialized".to_string(),
            params: None,
        };

        let _ = server.handle_request(request).await;

        // Now should be initialized
        assert!(*server.initialized.read().await);
    }

    #[tokio::test]
    async fn test_handle_tools_list_empty() {
        use std::env;
        let context = crate::tools::ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );
        let executor = ToolExecutor::new(context, false);
        let server = McpServer::new(executor);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(1.into())),
            method: "tools/list".to_string(),
            params: None,
        };

        let response = server.handle_request(request).await;

        assert!(response.error.is_none());
        assert!(response.result.is_some());
        let result = response.result.unwrap();
        assert!(result["tools"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_handle_initialize_valid_params() {
        use std::env;
        let context = crate::tools::ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );
        let executor = ToolExecutor::new(context, false);
        let server = McpServer::new(executor);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(1.into())),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "TestClient",
                    "version": "1.0"
                }
            })),
        };

        let response = server.handle_request(request).await;

        assert!(response.error.is_none());
        assert!(response.result.is_some());
        let result = response.result.unwrap();
        assert_eq!(result["serverInfo"]["name"], "ted");
    }

    #[tokio::test]
    async fn test_handle_initialize_missing_params() {
        use std::env;
        let context = crate::tools::ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );
        let executor = ToolExecutor::new(context, false);
        let server = McpServer::new(executor);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(1.into())),
            method: "initialize".to_string(),
            params: None,
        };

        let response = server.handle_request(request).await;

        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602); // Invalid params
    }

    #[tokio::test]
    async fn test_handle_initialize_invalid_params() {
        use std::env;
        let context = crate::tools::ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );
        let executor = ToolExecutor::new(context, false);
        let server = McpServer::new(executor);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(1.into())),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "invalid": "params"
            })),
        };

        let response = server.handle_request(request).await;

        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602);
    }

    #[tokio::test]
    async fn test_handle_tools_call_missing_params() {
        use std::env;
        let context = crate::tools::ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );
        let executor = ToolExecutor::new(context, false);
        let server = McpServer::new(executor);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(1.into())),
            method: "tools/call".to_string(),
            params: None,
        };

        let response = server.handle_request(request).await;

        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602);
    }

    #[tokio::test]
    async fn test_handle_tools_call_invalid_params() {
        use std::env;
        let context = crate::tools::ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );
        let executor = ToolExecutor::new(context, false);
        let server = McpServer::new(executor);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(1.into())),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "not_name": "test"
            })),
        };

        let response = server.handle_request(request).await;

        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602);
    }

    #[tokio::test]
    async fn test_handle_tools_call_tool_not_found() {
        use std::env;
        let context = crate::tools::ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );
        let executor = ToolExecutor::new(context, false);
        let server = McpServer::new(executor);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(1.into())),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "nonexistent_tool"
            })),
        };

        let response = server.handle_request(request).await;

        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32000);
    }

    // ===== Response Format Tests =====

    #[test]
    fn test_response_always_has_jsonrpc_version() {
        let success = McpServer::success_response(None, Value::Null);
        assert_eq!(success.jsonrpc, "2.0");

        let error = McpServer::error_response(None, JsonRpcError::internal_error());
        assert_eq!(error.jsonrpc, "2.0");
    }

    #[test]
    fn test_success_response_has_result_no_error() {
        let response =
            McpServer::success_response(Some(Value::Number(1.into())), serde_json::json!({}));

        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_error_response_has_error_no_result() {
        let response = McpServer::error_response(
            Some(Value::Number(1.into())),
            JsonRpcError::method_not_found(),
        );

        assert!(response.result.is_none());
        assert!(response.error.is_some());
    }

    // ===== String ID Tests =====

    #[test]
    fn test_success_response_with_string_id() {
        let response =
            McpServer::success_response(Some(Value::String("req-123".to_string())), Value::Null);

        assert_eq!(response.id, Some(Value::String("req-123".to_string())));
    }

    #[test]
    fn test_error_response_with_string_id() {
        let response = McpServer::error_response(
            Some(Value::String("req-456".to_string())),
            JsonRpcError::internal_error(),
        );

        assert_eq!(response.id, Some(Value::String("req-456".to_string())));
    }

    // ===== Integration-style Tests =====

    #[tokio::test]
    async fn test_multiple_requests_sequence() {
        use std::env;
        let context = crate::tools::ToolContext::new(
            env::current_dir().unwrap(),
            None,
            uuid::Uuid::new_v4(),
            false,
        );
        let executor = ToolExecutor::new(context, false);
        let server = McpServer::new(executor);

        // First: initialize
        let init_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(1.into())),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "Test", "version": "1.0"}
            })),
        };
        let init_response = server.handle_request(init_request).await;
        assert!(init_response.error.is_none());

        // Second: initialized notification
        let initialized_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(2.into())),
            method: "initialized".to_string(),
            params: None,
        };
        let _ = server.handle_request(initialized_request).await;
        assert!(*server.initialized.read().await);

        // Third: list tools
        let list_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::Number(3.into())),
            method: "tools/list".to_string(),
            params: None,
        };
        let list_response = server.handle_request(list_request).await;
        assert!(list_response.error.is_none());
    }
}
