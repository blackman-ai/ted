// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! MCP server implementation

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::Result;
use crate::tools::{Tool as TedTool, ToolContext, ToolExecutor, ToolOutput};
use super::protocol::*;
use super::transport::StdioTransport;

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
    /// Tool executor
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
            _ => Self::error_response(
                request.id,
                JsonRpcError::method_not_found(),
            ),
        }
    }

    /// Handle initialize request
    async fn handle_initialize(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        let params: InitializeParams = match request.params {
            Some(ref p) => match serde_json::from_value(p.clone()) {
                Ok(params) => params,
                Err(_) => {
                    return Self::error_response(
                        request.id,
                        JsonRpcError::invalid_params(),
                    );
                }
            },
            None => {
                return Self::error_response(
                    request.id,
                    JsonRpcError::invalid_params(),
                );
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
                    return Self::error_response(
                        request.id,
                        JsonRpcError::invalid_params(),
                    );
                }
            },
            None => {
                return Self::error_response(
                    request.id,
                    JsonRpcError::invalid_params(),
                );
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
        let args = params.arguments.unwrap_or(Value::Object(serde_json::Map::new()));
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

    #[test]
    fn test_success_response() {
        let response = McpServer::success_response(
            Some(Value::Number(1.into())),
            Value::String("test".to_string()),
        );

        assert_eq!(response.jsonrpc, "2.0");
        assert_eq!(response.id, Some(Value::Number(1.into())));
        assert!(response.error.is_none());
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
}
