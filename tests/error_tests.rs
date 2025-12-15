// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::io;
use ted::error::{ApiError, TedError};

#[test]
fn test_io_error_conversion() {
    let io_error = io::Error::new(io::ErrorKind::NotFound, "File not found");
    let ted_error: TedError = io_error.into();

    match ted_error {
        TedError::Io(_) => {} // Expected
        _ => panic!("Expected Io error, got different error type"),
    }
}

#[test]
fn test_config_error_display() {
    let error = TedError::Config("Missing API key".to_string());
    assert_eq!(error.to_string(), "Configuration error: Missing API key");
}

#[test]
fn test_tool_execution_error() {
    let error = TedError::ToolExecution("Command failed".to_string());
    assert_eq!(error.to_string(), "Tool execution failed: Command failed");
}

#[test]
fn test_permission_denied_error() {
    let error = TedError::PermissionDenied("Cannot write to file".to_string());
    assert_eq!(error.to_string(), "Permission denied: Cannot write to file");
}

#[test]
fn test_api_rate_limited_error() {
    let error = ApiError::RateLimited(30);
    assert_eq!(error.to_string(), "Rate limited: retry after 30 seconds");
}

#[test]
fn test_api_authentication_error() {
    let error = ApiError::AuthenticationFailed;
    assert_eq!(error.to_string(), "Authentication failed: invalid API key");
}

#[test]
fn test_api_context_too_long_error() {
    let error = ApiError::ContextTooLong {
        current: 150000,
        limit: 100000,
    };
    assert_eq!(
        error.to_string(),
        "Context too long: 150000 tokens exceeds limit of 100000"
    );
}

#[test]
fn test_api_server_error() {
    let error = ApiError::ServerError {
        status: 500,
        message: "Internal server error".to_string(),
    };
    assert_eq!(error.to_string(), "API error (500): Internal server error");
}

#[test]
fn test_api_error_to_ted_error_conversion() {
    let api_error = ApiError::Timeout;
    let ted_error: TedError = api_error.into();

    match ted_error {
        TedError::Api(ApiError::Timeout) => {} // Expected
        _ => panic!("Expected Api(Timeout) error"),
    }
}

#[test]
fn test_cap_error() {
    let error = TedError::Cap("Cap not found: unknown-cap".to_string());
    assert_eq!(error.to_string(), "Cap error: Cap not found: unknown-cap");
}

#[test]
fn test_invalid_input_error() {
    let error = TedError::InvalidInput("Empty prompt".to_string());
    assert_eq!(error.to_string(), "Invalid input: Empty prompt");
}

#[test]
fn test_session_error() {
    let error = TedError::Session("Session expired".to_string());
    assert_eq!(error.to_string(), "Session error: Session expired");
}
