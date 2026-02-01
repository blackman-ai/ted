// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Tool system for Ted
//!
//! Provides the framework for tools that the LLM can use to interact with
//! the filesystem, execute commands, and more.
//!
//! # External Tools
//!
//! In addition to built-in tools, Ted supports external tools written in any
//! language. External tools are discovered from `~/.ted/tools/` and communicate
//! via a JSON-RPC protocol over stdio.
//!
//! See the [`external`] module for details on creating external tools.

pub mod builtin;
pub mod definition;
pub mod executor;
pub mod external;
pub mod permission;

pub use definition::*;
pub use executor::*;
pub use permission::*;

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::indexer::RecallSender;
use crate::llm::provider::{LlmProvider, ToolDefinition};
use crate::skills::SkillRegistry;
use tokio::sync::mpsc;

/// Shell output event for streaming command output
#[derive(Debug, Clone)]
pub struct ShellOutputEvent {
    pub stream: String, // "stdout" or "stderr"
    pub text: String,
    pub done: bool,
    pub exit_code: Option<i32>,
}

/// Sender for shell output events
pub type ShellOutputSender = mpsc::UnboundedSender<ShellOutputEvent>;

/// Mode for file change sets
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSetMode {
    /// All changes applied together atomically
    Atomic,
    /// Changes applied one at a time incrementally
    Incremental,
}

/// A single file operation within a change set
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileOperation {
    /// Read a file
    Read { path: String },
    /// Edit a file (find and replace)
    Edit {
        path: String,
        old_string: String,
        new_string: String,
    },
    /// Write/create a new file
    Write { path: String, content: String },
    /// Delete a file
    Delete { path: String },
}

/// A set of related file changes
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileChangeSet {
    /// Unique identifier for this change set
    pub id: String,
    /// List of file operations
    pub files: Vec<FileOperation>,
    /// Description of what this change set accomplishes
    pub description: String,
    /// Related files that may be affected (for dependency tracking)
    pub related_files: Vec<String>,
    /// Mode: atomic or incremental
    pub mode: ChangeSetMode,
}

/// Context provided to tools during execution
#[derive(Clone)]
pub struct ToolContext {
    /// Current working directory
    pub working_directory: std::path::PathBuf,
    /// Detected project root (if any)
    pub project_root: Option<std::path::PathBuf>,
    /// Current session ID
    pub session_id: uuid::Uuid,
    /// Whether trust mode is enabled (auto-approve all)
    pub trust_mode: bool,
    /// Optional recall sender for memory integration
    recall_sender: Option<RecallSender>,
    /// Optional sender for shell output streaming
    shell_output_sender: Option<ShellOutputSender>,
    /// Files already provided in the context (to avoid re-reading)
    pub files_in_context: Vec<String>,
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("working_directory", &self.working_directory)
            .field("project_root", &self.project_root)
            .field("session_id", &self.session_id)
            .field("trust_mode", &self.trust_mode)
            .field("has_recall_sender", &self.recall_sender.is_some())
            .field(
                "has_shell_output_sender",
                &self.shell_output_sender.is_some(),
            )
            .field("files_in_context", &self.files_in_context.len())
            .finish()
    }
}

impl ToolContext {
    /// Create a new tool context.
    pub fn new(
        working_directory: PathBuf,
        project_root: Option<PathBuf>,
        session_id: uuid::Uuid,
        trust_mode: bool,
    ) -> Self {
        Self {
            working_directory,
            project_root,
            session_id,
            trust_mode,
            recall_sender: None,
            shell_output_sender: None,
            files_in_context: Vec::new(),
        }
    }

    /// Set the list of files already provided in context
    pub fn with_files_in_context(mut self, files: Vec<String>) -> Self {
        self.files_in_context = files;
        self
    }

    /// Check if a file is already in context
    pub fn is_file_in_context(&self, path: &str) -> bool {
        let normalized_path = normalize_context_path(path);
        self.files_in_context.iter().any(|f| {
            let normalized_file = normalize_context_path(f);
            // Match by filename or full path (normalized for separators and ./ prefixes)
            normalized_file == normalized_path
                || normalized_file.ends_with(&format!("/{}", normalized_path))
                || normalized_path.ends_with(&format!("/{}", normalized_file))
        })
    }

    /// Set the recall sender for memory integration.
    pub fn with_recall_sender(mut self, sender: RecallSender) -> Self {
        self.recall_sender = Some(sender);
        self
    }

    /// Set the shell output sender for streaming command output.
    pub fn with_shell_output_sender(mut self, sender: ShellOutputSender) -> Self {
        self.shell_output_sender = Some(sender);
        self
    }

    /// Emit shell output (for streaming command output).
    pub fn emit_shell_output(
        &self,
        stream: &str,
        text: String,
        done: bool,
        exit_code: Option<i32>,
    ) {
        eprintln!(
            "[EMIT DEBUG] emit_shell_output called, has_sender={}",
            self.shell_output_sender.is_some()
        );
        if let Some(sender) = &self.shell_output_sender {
            let result = sender.send(ShellOutputEvent {
                stream: stream.to_string(),
                text: text.clone(),
                done,
                exit_code,
            });
            eprintln!(
                "[EMIT DEBUG] send result: {:?}, text_len={}",
                result.is_ok(),
                text.len()
            );
        }
    }

    /// Emit a file read recall event.
    pub fn emit_file_read(&self, path: &std::path::Path) {
        if let Some(sender) = &self.recall_sender {
            let relative = self.make_relative(path);
            sender.file_read(relative);
        }
    }

    /// Emit a file edit recall event.
    pub fn emit_file_edit(&self, path: &std::path::Path) {
        if let Some(sender) = &self.recall_sender {
            let relative = self.make_relative(path);
            sender.file_edit(relative);
        }
    }

    /// Emit a file write recall event.
    pub fn emit_file_write(&self, path: &std::path::Path) {
        if let Some(sender) = &self.recall_sender {
            let relative = self.make_relative(path);
            sender.file_write(relative);
        }
    }

    /// Emit a search match recall event.
    pub fn emit_search_match(&self, paths: Vec<PathBuf>) {
        if let Some(sender) = &self.recall_sender {
            let relative_paths: Vec<PathBuf> =
                paths.iter().map(|p| self.make_relative(p)).collect();
            sender.search_match(relative_paths);
        }
    }

    /// Make a path relative to the project root (if set).
    fn make_relative(&self, path: &std::path::Path) -> PathBuf {
        if let Some(root) = &self.project_root {
            path.strip_prefix(root).unwrap_or(path).to_path_buf()
        } else {
            path.strip_prefix(&self.working_directory)
                .unwrap_or(path)
                .to_path_buf()
        }
    }
}

fn normalize_context_path(path: &str) -> String {
    let mut normalized = path.replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized.trim_start_matches("./").to_string();
    }
    normalized
}

/// Result of tool execution
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// The tool_use_id this result corresponds to
    pub tool_use_id: String,
    /// The output of the tool
    pub output: ToolOutput,
}

/// Output from a tool
#[derive(Debug, Clone)]
pub enum ToolOutput {
    /// Successful output
    Success(String),
    /// Error output
    Error(String),
}

impl ToolResult {
    /// Create a successful result
    pub fn success(tool_use_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            output: ToolOutput::Success(output.into()),
        }
    }

    /// Create an error result
    pub fn error(tool_use_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            output: ToolOutput::Error(error.into()),
        }
    }

    /// Check if this is an error
    pub fn is_error(&self) -> bool {
        matches!(self.output, ToolOutput::Error(_))
    }

    /// Get the output text
    pub fn output_text(&self) -> &str {
        match &self.output {
            ToolOutput::Success(s) => s,
            ToolOutput::Error(s) => s,
        }
    }
}

/// Trait for implementing tools
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool definition for the LLM
    fn definition(&self) -> ToolDefinition;

    /// Execute the tool with given input
    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult>;

    /// Generate permission request for this action (if needed)
    fn permission_request(&self, input: &Value) -> Option<PermissionRequest>;

    /// Whether this tool requires permission by default
    fn requires_permission(&self) -> bool {
        true
    }

    /// Get the tool name
    fn name(&self) -> &str;
}

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    /// Aliases mapping alternate names to canonical tool names
    aliases: HashMap<String, String>,
}

impl ToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            aliases: HashMap::new(),
        }
    }

    /// Build the default tool aliases
    fn default_aliases() -> HashMap<String, String> {
        let mut aliases = HashMap::new();
        // Common alternate names for file operations
        aliases.insert("file".to_string(), "file_read".to_string());
        aliases.insert("read".to_string(), "file_read".to_string());
        aliases.insert("read_file".to_string(), "file_read".to_string());
        aliases.insert("cat".to_string(), "file_read".to_string());
        aliases.insert("write".to_string(), "file_write".to_string());
        aliases.insert("write_file".to_string(), "file_write".to_string());
        aliases.insert("edit".to_string(), "file_edit".to_string());
        aliases.insert("edit_file".to_string(), "file_edit".to_string());
        // Common alternate names for shell
        aliases.insert("bash".to_string(), "shell".to_string());
        aliases.insert("exec".to_string(), "shell".to_string());
        aliases.insert("run".to_string(), "shell".to_string());
        aliases.insert("command".to_string(), "shell".to_string());
        aliases.insert("terminal".to_string(), "shell".to_string());
        // Common alternate names for search tools
        aliases.insert("search".to_string(), "grep".to_string());
        aliases.insert("find".to_string(), "glob".to_string());
        aliases.insert("ls".to_string(), "glob".to_string());
        aliases
    }

    /// Create a registry with all built-in tools
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();

        // Register all built-in tools
        registry.register(Arc::new(builtin::FileReadTool));
        registry.register(Arc::new(builtin::FileWriteTool));
        registry.register(Arc::new(builtin::FileEditTool));
        registry.register(Arc::new(builtin::FileChangeSetTool));
        registry.register(Arc::new(builtin::ShellTool::new()));
        registry.register(Arc::new(builtin::GlobTool));
        registry.register(Arc::new(builtin::GrepTool));
        registry.register(Arc::new(builtin::PlanUpdateTool));

        // Database tools
        registry.register(Arc::new(builtin::DatabaseInitTool));
        registry.register(Arc::new(builtin::DatabaseMigrateTool));
        registry.register(Arc::new(builtin::DatabaseQueryTool));
        registry.register(Arc::new(builtin::DatabaseSeedTool));

        // Add default aliases for common alternate tool names
        registry.aliases = Self::default_aliases();

        registry
    }

    /// Create a registry with built-in tools and external tools from ~/.ted/tools/
    pub fn with_all() -> Self {
        let mut registry = Self::with_builtins();
        registry.load_external_tools();
        registry
    }

    /// Register the spawn_agent tool with its required dependencies
    ///
    /// This must be called separately from `with_builtins()` because the spawn_agent
    /// tool requires an LLM provider and skill registry to function.
    pub fn register_spawn_agent(
        &mut self,
        provider: Arc<dyn LlmProvider>,
        skill_registry: Arc<SkillRegistry>,
    ) {
        self.register(Arc::new(builtin::SpawnAgentTool::new(
            provider,
            skill_registry,
        )));
    }

    /// Load external tools from the default directory (~/.ted/tools/)
    pub fn load_external_tools(&mut self) {
        let tools = external::load_external_tools();
        for tool in tools {
            self.register_boxed(tool);
        }
    }

    /// Load external tools from a specific directory
    pub fn load_external_tools_from(&mut self, dir: PathBuf) {
        let tools = external::load_external_tools_from(dir);
        for tool in tools {
            self.register_boxed(tool);
        }
    }

    /// Register a tool
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Register a boxed tool (converts to Arc internally)
    pub fn register_boxed(&mut self, tool: Box<dyn Tool>) {
        let arc: Arc<dyn Tool> = Arc::from(tool);
        self.tools.insert(arc.name().to_string(), arc);
    }

    /// Get a tool by name, resolving aliases if needed
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        // First try direct lookup
        if let Some(tool) = self.tools.get(name) {
            return Some(tool);
        }
        // Then try alias resolution
        if let Some(canonical_name) = self.aliases.get(name) {
            return self.tools.get(canonical_name);
        }
        None
    }

    /// Resolve an alias to the canonical tool name
    pub fn resolve_alias<'a>(&'a self, name: &'a str) -> &'a str {
        self.aliases.get(name).map(|s| s.as_str()).unwrap_or(name)
    }

    /// Get all tool definitions
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// List all tool names
    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Get the number of registered tools
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Check if an external tool with the given name exists
    pub fn has_external(&self, name: &str) -> bool {
        // External tools are currently not distinguishable by type,
        // but we can check the loader directly
        let loader = external::ToolLoader::new();
        loader.load_by_name(name).is_ok()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_tool_context_creation() {
        let context = ToolContext::new(
            PathBuf::from("/tmp"),
            Some(PathBuf::from("/home/user/project")),
            uuid::Uuid::new_v4(),
            false,
        );

        assert_eq!(context.working_directory, PathBuf::from("/tmp"));
        assert!(context.project_root.is_some());
        assert!(!context.trust_mode);
    }

    #[test]
    fn test_tool_context_trust_mode() {
        let context = ToolContext::new(PathBuf::from("/tmp"), None, uuid::Uuid::new_v4(), true);

        assert!(context.trust_mode);
        assert!(context.project_root.is_none());
    }

    #[test]
    fn test_tool_context_emit_recall() {
        use crate::indexer::recall_channel;

        let (sender, receiver) = recall_channel();
        let context = ToolContext::new(
            PathBuf::from("/project"),
            Some(PathBuf::from("/project")),
            uuid::Uuid::new_v4(),
            false,
        )
        .with_recall_sender(sender);

        context.emit_file_read(std::path::Path::new("/project/src/main.rs"));
        context.emit_file_edit(std::path::Path::new("/project/src/lib.rs"));
        context.emit_file_write(std::path::Path::new("/project/src/new.rs"));

        let events = receiver.drain();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_tool_context_make_relative() {
        let context = ToolContext::new(
            PathBuf::from("/work"),
            Some(PathBuf::from("/project")),
            uuid::Uuid::new_v4(),
            false,
        );

        // Should strip project root
        let relative = context.make_relative(std::path::Path::new("/project/src/main.rs"));
        assert_eq!(relative, PathBuf::from("src/main.rs"));

        // Non-matching paths stay as-is
        let other = context.make_relative(std::path::Path::new("/other/file.rs"));
        assert_eq!(other, PathBuf::from("/other/file.rs"));
    }

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult::success("id123", "Success output");

        assert!(!result.is_error());
        assert_eq!(result.tool_use_id, "id123");
        assert_eq!(result.output_text(), "Success output");
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("id456", "Error message");

        assert!(result.is_error());
        assert_eq!(result.tool_use_id, "id456");
        assert_eq!(result.output_text(), "Error message");
    }

    #[test]
    fn test_tool_output_success_variant() {
        let output = ToolOutput::Success("Success".to_string());
        match output {
            ToolOutput::Success(s) => assert_eq!(s, "Success"),
            ToolOutput::Error(_) => panic!("Expected Success variant"),
        }
    }

    #[test]
    fn test_tool_output_error_variant() {
        let output = ToolOutput::Error("Error".to_string());
        match output {
            ToolOutput::Success(_) => panic!("Expected Error variant"),
            ToolOutput::Error(s) => assert_eq!(s, "Error"),
        }
    }

    #[test]
    fn test_tool_registry_new() {
        let registry = ToolRegistry::new();
        assert!(registry.tools.is_empty());
    }

    #[test]
    fn test_tool_registry_with_builtins() {
        let registry = ToolRegistry::with_builtins();

        assert!(!registry.tools.is_empty());
        assert!(registry.get("file_read").is_some());
        assert!(registry.get("file_write").is_some());
        assert!(registry.get("file_edit").is_some());
        assert!(registry.get("shell").is_some());
        assert!(registry.get("glob").is_some());
        assert!(registry.get("grep").is_some());
    }

    #[test]
    fn test_tool_registry_get_nonexistent() {
        let registry = ToolRegistry::with_builtins();
        assert!(registry.get("nonexistent_tool").is_none());
    }

    #[test]
    fn test_tool_registry_definitions() {
        let registry = ToolRegistry::with_builtins();
        let definitions = registry.definitions();

        assert!(!definitions.is_empty());
        let names: Vec<&str> = definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"file_read"));
    }

    #[test]
    fn test_tool_registry_names() {
        let registry = ToolRegistry::with_builtins();
        let names = registry.names();

        assert!(names.contains(&"file_read"));
        assert!(names.contains(&"file_write"));
        assert!(names.contains(&"shell"));
    }

    #[test]
    fn test_tool_registry_default() {
        let registry = ToolRegistry::default();
        assert!(!registry.tools.is_empty());
    }

    #[test]
    fn test_tool_result_debug() {
        let result = ToolResult::success("id", "output");
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("ToolResult"));
    }

    #[test]
    fn test_tool_output_debug() {
        let output = ToolOutput::Success("test".to_string());
        let debug_str = format!("{:?}", output);
        assert!(debug_str.contains("Success"));
    }

    #[test]
    fn test_tool_context_debug() {
        let context = ToolContext::new(PathBuf::from("/tmp"), None, uuid::Uuid::new_v4(), false);
        let debug_str = format!("{:?}", context);
        assert!(debug_str.contains("ToolContext"));
    }

    #[test]
    fn test_tool_context_clone() {
        let context = ToolContext::new(
            PathBuf::from("/tmp"),
            Some(PathBuf::from("/project")),
            uuid::Uuid::new_v4(),
            true,
        );

        let cloned = context.clone();
        assert_eq!(cloned.working_directory, context.working_directory);
        assert_eq!(cloned.project_root, context.project_root);
        assert_eq!(cloned.session_id, context.session_id);
        assert_eq!(cloned.trust_mode, context.trust_mode);
    }

    #[test]
    fn test_tool_result_clone() {
        let result = ToolResult::success("id", "output");
        let cloned = result.clone();

        assert_eq!(cloned.tool_use_id, result.tool_use_id);
        assert_eq!(cloned.output_text(), result.output_text());
    }

    #[test]
    fn test_tool_output_clone() {
        let output = ToolOutput::Error("error".to_string());
        let cloned = output.clone();

        match (output, cloned) {
            (ToolOutput::Error(a), ToolOutput::Error(b)) => assert_eq!(a, b),
            _ => panic!("Clone should produce same variant"),
        }
    }

    #[test]
    fn test_tool_registry_len() {
        let registry = ToolRegistry::with_builtins();
        assert_eq!(registry.len(), 12); // 12 built-in tools (8 core + 4 database)
    }

    #[test]
    fn test_tool_registry_is_empty() {
        let empty = ToolRegistry::new();
        assert!(empty.is_empty());

        let with_builtins = ToolRegistry::with_builtins();
        assert!(!with_builtins.is_empty());
    }

    #[test]
    fn test_tool_registry_load_external_from_empty_dir() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let mut registry = ToolRegistry::with_builtins();
        let initial_count = registry.len();

        registry.load_external_tools_from(temp_dir.path().to_path_buf());

        // No external tools to load, count should be the same
        assert_eq!(registry.len(), initial_count);
    }

    #[test]
    fn test_tool_registry_load_external_from_dir() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        // Create a test manifest
        std::fs::write(
            temp_dir.path().join("test_tool.json"),
            r#"{
                "name": "test_tool",
                "description": "A test external tool",
                "command": ["echo", "test"],
                "input_schema": {"type": "object", "properties": {}}
            }"#,
        )
        .unwrap();

        let mut registry = ToolRegistry::with_builtins();
        let initial_count = registry.len();

        registry.load_external_tools_from(temp_dir.path().to_path_buf());

        // Should have one more tool
        assert_eq!(registry.len(), initial_count + 1);
        assert!(registry.get("test_tool").is_some());
    }

    #[test]
    fn test_tool_registry_with_all() {
        // Note: This test depends on whether ~/.ted/tools/ exists
        // It should at least have the built-in tools
        let registry = ToolRegistry::with_all();
        assert!(registry.len() >= 11);
    }

    #[test]
    fn test_tool_registry_register_boxed() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        // Create a manifest
        std::fs::write(
            temp_dir.path().join("boxed_tool.json"),
            r#"{
                "name": "boxed_tool",
                "description": "Boxed tool test",
                "command": ["echo"],
                "input_schema": {"type": "object", "properties": {}}
            }"#,
        )
        .unwrap();

        let mut registry = ToolRegistry::new();
        let tools = external::load_external_tools_from(temp_dir.path().to_path_buf());

        for tool in tools {
            registry.register_boxed(tool);
        }

        assert!(registry.get("boxed_tool").is_some());
    }

    #[test]
    fn test_tool_registry_alias_file_to_file_read() {
        let registry = ToolRegistry::with_builtins();

        // "file" should resolve to "file_read"
        let tool = registry.get("file");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name(), "file_read");
    }

    #[test]
    fn test_tool_registry_alias_bash_to_shell() {
        let registry = ToolRegistry::with_builtins();

        // "bash" should resolve to "shell"
        let tool = registry.get("bash");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name(), "shell");
    }

    #[test]
    fn test_tool_registry_alias_search_to_grep() {
        let registry = ToolRegistry::with_builtins();

        // "search" should resolve to "grep"
        let tool = registry.get("search");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name(), "grep");
    }

    #[test]
    fn test_tool_registry_resolve_alias() {
        let registry = ToolRegistry::with_builtins();

        assert_eq!(registry.resolve_alias("file"), "file_read");
        assert_eq!(registry.resolve_alias("bash"), "shell");
        assert_eq!(registry.resolve_alias("search"), "grep");
        // Non-aliases should return themselves
        assert_eq!(registry.resolve_alias("file_read"), "file_read");
        assert_eq!(registry.resolve_alias("nonexistent"), "nonexistent");
    }

    #[test]
    fn test_tool_registry_direct_name_takes_precedence() {
        let registry = ToolRegistry::with_builtins();

        // Direct tool names should work
        let tool = registry.get("file_read");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name(), "file_read");

        let tool = registry.get("shell");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name(), "shell");
    }
}
