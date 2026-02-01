// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Error types for Ted
//!
//! This module defines all error types used throughout the application.

use thiserror::Error;

/// Main error type for Ted operations
#[derive(Error, Debug)]
pub enum TedError {
    /// API-related errors
    #[error("API error: {0}")]
    Api(#[from] ApiError),

    /// Tool execution errors
    #[error("Tool execution failed: {0}")]
    ToolExecution(String),

    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Context management errors
    #[error("Context error: {0}")]
    Context(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// IO errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// TOML parsing errors
    #[error("TOML error: {0}")]
    Toml(String),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// HTTP request errors
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Cap-related errors
    #[error("Cap error: {0}")]
    Cap(String),

    /// Session errors
    #[error("Session error: {0}")]
    Session(String),

    /// Plan errors
    #[error("Plan error: {0}")]
    Plan(String),

    /// LSP server errors
    #[error("LSP error: {0}")]
    Lsp(String),

    /// Agent errors
    #[error("Agent error: {0}")]
    Agent(String),

    /// Skill errors
    #[error("Skill error: {0}")]
    Skill(String),

    /// Bead errors
    #[error("Bead error: {0}")]
    Bead(String),
}

/// API-specific error types
#[derive(Error, Debug)]
pub enum ApiError {
    /// Authentication failed (invalid API key)
    #[error("Authentication failed: invalid API key")]
    AuthenticationFailed,

    /// Rate limited by the API
    #[error("Rate limited: retry after {0} seconds")]
    RateLimited(u32),

    /// Requested model not found
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    /// Context window exceeded
    #[error("Context too long: {current} tokens exceeds limit of {limit}")]
    ContextTooLong { current: u32, limit: u32 },

    /// Network connectivity error
    #[error("Network error: {0}")]
    Network(String),

    /// Invalid response from API
    #[error("Invalid API response: {0}")]
    InvalidResponse(String),

    /// API returned an error
    #[error("API error ({status}): {message}")]
    ServerError { status: u16, message: String },

    /// Timeout waiting for response
    #[error("Request timed out")]
    Timeout,

    /// Streaming error
    #[error("Streaming error: {0}")]
    StreamError(String),
}

/// Result type alias for Ted operations
pub type Result<T> = std::result::Result<T, TedError>;

impl From<toml::de::Error> for TedError {
    fn from(err: toml::de::Error) -> Self {
        TedError::Toml(err.to_string())
    }
}

impl From<toml::ser::Error> for TedError {
    fn from(err: toml::ser::Error) -> Self {
        TedError::Toml(err.to_string())
    }
}

impl From<anyhow::Error> for TedError {
    fn from(err: anyhow::Error) -> Self {
        TedError::Lsp(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ted_error_tool_execution() {
        let err = TedError::ToolExecution("tool failed".to_string());
        assert!(err.to_string().contains("tool failed"));
    }

    #[test]
    fn test_ted_error_permission_denied() {
        let err = TedError::PermissionDenied("access denied".to_string());
        assert!(err.to_string().contains("Permission denied"));
        assert!(err.to_string().contains("access denied"));
    }

    #[test]
    fn test_ted_error_context() {
        let err = TedError::Context("context error".to_string());
        assert!(err.to_string().contains("Context error"));
    }

    #[test]
    fn test_ted_error_config() {
        let err = TedError::Config("bad config".to_string());
        assert!(err.to_string().contains("Configuration error"));
    }

    #[test]
    fn test_ted_error_toml() {
        let err = TedError::Toml("parse error".to_string());
        assert!(err.to_string().contains("TOML error"));
    }

    #[test]
    fn test_ted_error_invalid_input() {
        let err = TedError::InvalidInput("bad input".to_string());
        assert!(err.to_string().contains("Invalid input"));
    }

    #[test]
    fn test_ted_error_cap() {
        let err = TedError::Cap("cap not found".to_string());
        assert!(err.to_string().contains("Cap error"));
    }

    #[test]
    fn test_ted_error_session() {
        let err = TedError::Session("session expired".to_string());
        assert!(err.to_string().contains("Session error"));
    }

    #[test]
    fn test_ted_error_plan() {
        let err = TedError::Plan("plan not found".to_string());
        assert!(err.to_string().contains("Plan error"));
    }

    #[test]
    fn test_ted_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let ted_err: TedError = io_err.into();
        assert!(ted_err.to_string().contains("IO error"));
    }

    #[test]
    fn test_ted_error_debug() {
        let err = TedError::ToolExecution("test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("ToolExecution"));
    }

    #[test]
    fn test_api_error_authentication_failed() {
        let err = ApiError::AuthenticationFailed;
        assert!(err.to_string().contains("Authentication failed"));
    }

    #[test]
    fn test_api_error_rate_limited() {
        let err = ApiError::RateLimited(30);
        assert!(err.to_string().contains("Rate limited"));
        assert!(err.to_string().contains("30"));
    }

    #[test]
    fn test_api_error_model_not_found() {
        let err = ApiError::ModelNotFound("gpt-5".to_string());
        assert!(err.to_string().contains("Model not found"));
        assert!(err.to_string().contains("gpt-5"));
    }

    #[test]
    fn test_api_error_context_too_long() {
        let err = ApiError::ContextTooLong {
            current: 10000,
            limit: 8192,
        };
        assert!(err.to_string().contains("10000"));
        assert!(err.to_string().contains("8192"));
    }

    #[test]
    fn test_api_error_network() {
        let err = ApiError::Network("connection refused".to_string());
        assert!(err.to_string().contains("Network error"));
    }

    #[test]
    fn test_api_error_invalid_response() {
        let err = ApiError::InvalidResponse("malformed json".to_string());
        assert!(err.to_string().contains("Invalid API response"));
    }

    #[test]
    fn test_api_error_server_error() {
        let err = ApiError::ServerError {
            status: 500,
            message: "internal server error".to_string(),
        };
        assert!(err.to_string().contains("500"));
        assert!(err.to_string().contains("internal server error"));
    }

    #[test]
    fn test_api_error_timeout() {
        let err = ApiError::Timeout;
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn test_api_error_stream_error() {
        let err = ApiError::StreamError("stream closed".to_string());
        assert!(err.to_string().contains("Streaming error"));
    }

    #[test]
    fn test_api_error_debug() {
        let err = ApiError::Timeout;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Timeout"));
    }

    #[test]
    fn test_ted_error_from_api_error() {
        let api_err = ApiError::AuthenticationFailed;
        let ted_err: TedError = api_err.into();
        assert!(ted_err.to_string().contains("API error"));
    }

    #[test]
    fn test_result_type_alias() {
        fn test_fn() -> Result<i32> {
            Ok(42)
        }

        assert_eq!(test_fn().unwrap(), 42);
    }

    #[test]
    fn test_result_error() {
        fn test_fn() -> Result<i32> {
            Err(TedError::InvalidInput("test".to_string()))
        }

        assert!(test_fn().is_err());
    }
}
