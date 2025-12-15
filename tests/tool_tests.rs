// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use ted::tools::{ToolContext, ToolOutput, ToolRegistry, ToolResult};

#[test]
fn test_tool_result_success() {
    let result = ToolResult::success("test-id", "Operation completed");

    assert!(!result.is_error());
    assert_eq!(result.tool_use_id, "test-id");
    assert_eq!(result.output_text(), "Operation completed");
}

#[test]
fn test_tool_result_error() {
    let result = ToolResult::error("test-id", "Something went wrong");

    assert!(result.is_error());
    assert_eq!(result.tool_use_id, "test-id");
    assert_eq!(result.output_text(), "Something went wrong");
}

#[test]
fn test_tool_output_success_variant() {
    let output = ToolOutput::Success("Success message".to_string());
    match output {
        ToolOutput::Success(msg) => assert_eq!(msg, "Success message"),
        ToolOutput::Error(_) => panic!("Expected Success variant"),
    }
}

#[test]
fn test_tool_output_error_variant() {
    let output = ToolOutput::Error("Error message".to_string());
    match output {
        ToolOutput::Error(msg) => assert_eq!(msg, "Error message"),
        ToolOutput::Success(_) => panic!("Expected Error variant"),
    }
}

#[test]
fn test_tool_registry_with_builtins() {
    let registry = ToolRegistry::with_builtins();

    // Check that expected tools are registered
    assert!(registry.get("file_read").is_some());
    assert!(registry.get("file_write").is_some());
    assert!(registry.get("file_edit").is_some());
    assert!(registry.get("shell").is_some());
    assert!(registry.get("glob").is_some());
    assert!(registry.get("grep").is_some());
}

#[test]
fn test_tool_registry_names() {
    let registry = ToolRegistry::with_builtins();
    let names = registry.names();

    assert!(names.contains(&"file_read"));
    assert!(names.contains(&"file_write"));
    assert!(names.contains(&"file_edit"));
    assert!(names.contains(&"shell"));
    assert!(names.contains(&"glob"));
    assert!(names.contains(&"grep"));
}

#[test]
fn test_tool_registry_definitions() {
    let registry = ToolRegistry::with_builtins();
    let definitions = registry.definitions();

    // Should have 7 built-in tools
    assert_eq!(definitions.len(), 7);

    // Each definition should have a name
    for def in &definitions {
        assert!(!def.name.is_empty());
    }
}

#[test]
fn test_tool_context_creation() {
    let context = ToolContext::new(
        std::path::PathBuf::from("/tmp"),
        Some(std::path::PathBuf::from("/project")),
        uuid::Uuid::new_v4(),
        false,
    );

    assert_eq!(context.working_directory, std::path::PathBuf::from("/tmp"));
    assert!(context.project_root.is_some());
    assert!(!context.trust_mode);
}

#[test]
fn test_tool_context_trust_mode() {
    let context = ToolContext::new(
        std::path::PathBuf::from("/tmp"),
        None,
        uuid::Uuid::new_v4(),
        true,
    );

    assert!(context.trust_mode);
}
