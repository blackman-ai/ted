// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Tool manifest parsing
//!
//! Defines the manifest format for external tools.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::{Result, TedError};
use crate::llm::provider::{ToolDefinition, ToolInputSchema};

/// Manifest for an external tool.
///
/// External tools are defined by JSON manifest files in `~/.ted/tools/`.
///
/// # Example manifest (`~/.ted/tools/my-tool.json`)
///
/// ```json
/// {
///   "name": "my_tool",
///   "description": "Description shown to the LLM",
///   "command": ["node", "~/.ted/tools/my-tool/index.js"],
///   "input_schema": {
///     "type": "object",
///     "properties": {
///       "path": { "type": "string", "description": "File path" }
///     },
///     "required": ["path"]
///   },
///   "requires_permission": true,
///   "timeout_ms": 30000
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolManifest {
    /// Tool name (must be unique, used by LLM to invoke)
    pub name: String,

    /// Human-readable description for the LLM
    pub description: String,

    /// Command to execute the tool (first element is program, rest are args)
    pub command: Vec<String>,

    /// JSON Schema for tool input
    pub input_schema: ManifestInputSchema,

    /// Whether this tool requires permission before execution
    #[serde(default = "default_requires_permission")]
    pub requires_permission: bool,

    /// Timeout in milliseconds (default: 30000 = 30 seconds)
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,

    /// Working directory for the tool (default: project root)
    #[serde(default)]
    pub working_directory: Option<String>,

    /// Environment variables to pass to the tool
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

fn default_requires_permission() -> bool {
    true
}

fn default_timeout() -> u64 {
    30000 // 30 seconds
}

/// Input schema from manifest (mirrors JSON Schema structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestInputSchema {
    /// Schema type (always "object" for tool inputs)
    #[serde(rename = "type")]
    pub schema_type: String,

    /// Property definitions
    pub properties: serde_json::Value,

    /// Required properties
    #[serde(default)]
    pub required: Vec<String>,
}

impl ToolManifest {
    /// Load a manifest from a file.
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            TedError::Config(format!("Failed to read manifest {}: {}", path.display(), e))
        })?;

        Self::parse(&content)
    }

    /// Parse a manifest from JSON string.
    pub fn parse(json: &str) -> Result<Self> {
        let manifest: Self = serde_json::from_str(json)
            .map_err(|e| TedError::Config(format!("Failed to parse tool manifest: {}", e)))?;

        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate the manifest.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(TedError::Config(
                "Tool manifest: name cannot be empty".to_string(),
            ));
        }

        // Validate name format (alphanumeric + underscore)
        if !self.name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(TedError::Config(format!(
                "Tool manifest: name '{}' must contain only alphanumeric characters and underscores",
                self.name
            )));
        }

        if self.description.is_empty() {
            return Err(TedError::Config(
                "Tool manifest: description cannot be empty".to_string(),
            ));
        }

        if self.command.is_empty() {
            return Err(TedError::Config(
                "Tool manifest: command cannot be empty".to_string(),
            ));
        }

        if self.input_schema.schema_type != "object" {
            return Err(TedError::Config(
                "Tool manifest: input_schema type must be 'object'".to_string(),
            ));
        }

        Ok(())
    }

    /// Convert to a ToolDefinition for the LLM.
    pub fn to_tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: ToolInputSchema {
                schema_type: self.input_schema.schema_type.clone(),
                properties: self.input_schema.properties.clone(),
                required: self.input_schema.required.clone(),
            },
        }
    }

    /// Expand ~ in command paths to actual home directory.
    pub fn expand_command(&self) -> Vec<String> {
        self.command.iter().map(|arg| expand_tilde(arg)).collect()
    }

    /// Get the expanded working directory.
    pub fn expand_working_directory(&self, default: &std::path::Path) -> PathBuf {
        match &self.working_directory {
            Some(dir) => PathBuf::from(expand_tilde(dir)),
            None => default.to_path_buf(),
        }
    }
}

/// Expand ~ to home directory in a path string.
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), &path[1..]);
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_parse_minimal() {
        let json = r#"{
            "name": "my_tool",
            "description": "A test tool",
            "command": ["echo", "hello"],
            "input_schema": {
                "type": "object",
                "properties": {}
            }
        }"#;

        let manifest = ToolManifest::parse(json).unwrap();
        assert_eq!(manifest.name, "my_tool");
        assert_eq!(manifest.description, "A test tool");
        assert_eq!(manifest.command, vec!["echo", "hello"]);
        assert!(manifest.requires_permission); // default
        assert_eq!(manifest.timeout_ms, 30000); // default
    }

    #[test]
    fn test_manifest_parse_full() {
        let json = r#"{
            "name": "full_tool",
            "description": "A full test tool",
            "command": ["node", "index.js"],
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path"}
                },
                "required": ["path"]
            },
            "requires_permission": false,
            "timeout_ms": 60000,
            "working_directory": "/tmp",
            "env": {"FOO": "bar"}
        }"#;

        let manifest = ToolManifest::parse(json).unwrap();
        assert_eq!(manifest.name, "full_tool");
        assert!(!manifest.requires_permission);
        assert_eq!(manifest.timeout_ms, 60000);
        assert_eq!(manifest.working_directory, Some("/tmp".to_string()));
        assert_eq!(manifest.env.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(manifest.input_schema.required, vec!["path"]);
    }

    #[test]
    fn test_manifest_validate_empty_name() {
        let json = r#"{
            "name": "",
            "description": "Test",
            "command": ["echo"],
            "input_schema": {"type": "object", "properties": {}}
        }"#;

        let result = ToolManifest::parse(json);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("name cannot be empty"));
    }

    #[test]
    fn test_manifest_validate_invalid_name() {
        let json = r#"{
            "name": "my-tool",
            "description": "Test",
            "command": ["echo"],
            "input_schema": {"type": "object", "properties": {}}
        }"#;

        let result = ToolManifest::parse(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("alphanumeric"));
    }

    #[test]
    fn test_manifest_validate_empty_command() {
        let json = r#"{
            "name": "test",
            "description": "Test",
            "command": [],
            "input_schema": {"type": "object", "properties": {}}
        }"#;

        let result = ToolManifest::parse(json);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("command cannot be empty"));
    }

    #[test]
    fn test_manifest_validate_wrong_schema_type() {
        let json = r#"{
            "name": "test",
            "description": "Test",
            "command": ["echo"],
            "input_schema": {"type": "array", "properties": {}}
        }"#;

        let result = ToolManifest::parse(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be 'object'"));
    }

    #[test]
    fn test_manifest_to_tool_definition() {
        let json = r#"{
            "name": "my_tool",
            "description": "A test tool",
            "command": ["echo"],
            "input_schema": {
                "type": "object",
                "properties": {
                    "msg": {"type": "string"}
                },
                "required": ["msg"]
            }
        }"#;

        let manifest = ToolManifest::parse(json).unwrap();
        let def = manifest.to_tool_definition();

        assert_eq!(def.name, "my_tool");
        assert_eq!(def.description, "A test tool");
        assert_eq!(def.input_schema.schema_type, "object");
        assert_eq!(def.input_schema.required, vec!["msg"]);
    }

    #[test]
    fn test_expand_tilde() {
        // Test non-tilde path
        assert_eq!(expand_tilde("/usr/bin/node"), "/usr/bin/node");
        assert_eq!(expand_tilde("relative/path"), "relative/path");

        // Test tilde expansion (will vary by system)
        let expanded = expand_tilde("~/test");
        if let Some(home) = dirs::home_dir() {
            assert!(expanded.starts_with(&home.to_string_lossy().to_string()));
            assert!(expanded.ends_with("/test"));
        }
    }

    #[test]
    fn test_expand_command() {
        let json = r#"{
            "name": "test",
            "description": "Test",
            "command": ["node", "~/tools/index.js"],
            "input_schema": {"type": "object", "properties": {}}
        }"#;

        let manifest = ToolManifest::parse(json).unwrap();
        let expanded = manifest.expand_command();

        assert_eq!(expanded[0], "node");
        if let Some(home) = dirs::home_dir() {
            assert!(expanded[1].starts_with(&home.to_string_lossy().to_string()));
        }
    }

    #[test]
    fn test_expand_working_directory() {
        let json = r#"{
            "name": "test",
            "description": "Test",
            "command": ["echo"],
            "input_schema": {"type": "object", "properties": {}},
            "working_directory": "~/projects"
        }"#;

        let manifest = ToolManifest::parse(json).unwrap();
        let default = std::path::Path::new("/default");
        let expanded = manifest.expand_working_directory(default);

        if let Some(home) = dirs::home_dir() {
            assert!(expanded
                .to_string_lossy()
                .starts_with(&home.to_string_lossy().to_string()));
        }
    }

    #[test]
    fn test_expand_working_directory_default() {
        let json = r#"{
            "name": "test",
            "description": "Test",
            "command": ["echo"],
            "input_schema": {"type": "object", "properties": {}}
        }"#;

        let manifest = ToolManifest::parse(json).unwrap();
        let default = std::path::Path::new("/default/path");
        let expanded = manifest.expand_working_directory(default);

        assert_eq!(expanded, PathBuf::from("/default/path"));
    }

    #[test]
    fn test_manifest_defaults() {
        let json = r#"{
            "name": "test",
            "description": "Test",
            "command": ["echo"],
            "input_schema": {"type": "object", "properties": {}}
        }"#;

        let manifest = ToolManifest::parse(json).unwrap();

        // Check defaults
        assert!(manifest.requires_permission);
        assert_eq!(manifest.timeout_ms, 30000);
        assert!(manifest.working_directory.is_none());
        assert!(manifest.env.is_empty());
        assert!(manifest.input_schema.required.is_empty());
    }

    #[test]
    fn test_manifest_clone() {
        let json = r#"{
            "name": "test",
            "description": "Test",
            "command": ["echo"],
            "input_schema": {"type": "object", "properties": {}}
        }"#;

        let manifest = ToolManifest::parse(json).unwrap();
        let cloned = manifest.clone();

        assert_eq!(cloned.name, manifest.name);
        assert_eq!(cloned.command, manifest.command);
    }

    #[test]
    fn test_manifest_serialize() {
        let json = r#"{
            "name": "test",
            "description": "Test",
            "command": ["echo"],
            "input_schema": {"type": "object", "properties": {}}
        }"#;

        let manifest = ToolManifest::parse(json).unwrap();
        let serialized = serde_json::to_string(&manifest).unwrap();

        assert!(serialized.contains("\"name\":\"test\""));
        assert!(serialized.contains("\"description\":\"Test\""));
    }
}
