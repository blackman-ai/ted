// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Core types for the subagent system
//!
//! This module defines the fundamental data structures for spawning and
//! managing specialized subagents with isolated contexts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use uuid::Uuid;

/// Memory management strategy for subagent context
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum MemoryStrategy {
    /// Keep all messages, only trim if over token budget
    #[default]
    Full,
    /// LLM-summarize older messages when threshold exceeded
    Summarizing {
        /// Token count that triggers summarization
        threshold: u32,
        /// Target token count after summarization
        target: u32,
    },
    /// Fixed sliding window of recent messages
    Windowed {
        /// Maximum number of messages to keep
        window_size: usize,
    },
}

impl MemoryStrategy {
    /// Create a summarizing strategy with default thresholds
    pub fn summarizing() -> Self {
        Self::Summarizing {
            threshold: 50_000,
            target: 20_000,
        }
    }

    /// Create a windowed strategy with a specific window size
    pub fn windowed(size: usize) -> Self {
        Self::Windowed { window_size: size }
    }
}

/// Configuration for spawning a subagent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Unique identifier for this agent instance
    pub id: Uuid,
    /// Human-readable name (e.g., "explore-a3f8")
    pub name: String,
    /// Type of agent (explore, plan, implement, bash, review)
    pub agent_type: String,
    /// Additional caps to load beyond type defaults
    pub caps: Vec<String>,
    /// Optional skill to load (e.g., "rust-async")
    pub skill: Option<String>,
    /// Memory management strategy
    pub memory_strategy: MemoryStrategy,
    /// Maximum iterations before stopping
    pub max_iterations: u32,
    /// Token budget for the entire agent run
    pub token_budget: u32,
    /// Whether to run in background (async)
    pub background: bool,
    /// Parent agent ID (None for root agent)
    pub parent_id: Option<Uuid>,
    /// Optional bead ID for task tracking
    pub bead_id: Option<String>,
    /// The task/prompt for the agent to accomplish
    pub task: String,
    /// Working directory for the agent
    pub working_dir: PathBuf,
    /// Model to use (inherits from parent if None)
    pub model: Option<String>,
}

impl AgentConfig {
    /// Create a new agent configuration with defaults
    pub fn new(agent_type: &str, task: &str, working_dir: PathBuf) -> Self {
        let id = Uuid::new_v4();
        let short_id = &id.to_string()[..4];

        Self {
            id,
            name: format!("{}-{}", agent_type, short_id),
            agent_type: agent_type.to_string(),
            caps: Vec::new(),
            skill: None,
            memory_strategy: MemoryStrategy::default(),
            max_iterations: 30,
            token_budget: 100_000,
            background: false,
            parent_id: None,
            bead_id: None,
            task: task.to_string(),
            working_dir,
            model: None,
        }
    }

    /// Set additional caps
    pub fn with_caps(mut self, caps: Vec<String>) -> Self {
        self.caps = caps;
        self
    }

    /// Set the skill to load
    pub fn with_skill(mut self, skill: String) -> Self {
        self.skill = Some(skill);
        self
    }

    /// Set memory strategy
    pub fn with_memory_strategy(mut self, strategy: MemoryStrategy) -> Self {
        self.memory_strategy = strategy;
        self
    }

    /// Set max iterations
    pub fn with_max_iterations(mut self, max: u32) -> Self {
        self.max_iterations = max;
        self
    }

    /// Set token budget
    pub fn with_token_budget(mut self, budget: u32) -> Self {
        self.token_budget = budget;
        self
    }

    /// Set background execution
    pub fn with_background(mut self, background: bool) -> Self {
        self.background = background;
        self
    }

    /// Set parent agent ID
    pub fn with_parent(mut self, parent_id: Uuid) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Set bead ID for task tracking
    pub fn with_bead(mut self, bead_id: String) -> Self {
        self.bead_id = Some(bead_id);
        self
    }

    /// Set model override
    pub fn with_model(mut self, model: String) -> Self {
        self.model = Some(model);
        self
    }
}

/// Result returned when a subagent completes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    /// The agent's unique ID
    pub agent_id: Uuid,
    /// The agent's display name
    pub agent_name: String,
    /// Whether the agent completed successfully
    pub success: bool,
    /// The full output/response from the agent
    pub output: String,
    /// A concise summary of what was accomplished
    pub summary: String,
    /// Files that were modified by the agent
    pub files_changed: Vec<PathBuf>,
    /// Files that were read by the agent
    pub files_read: Vec<PathBuf>,
    /// Number of iterations/turns the agent took
    pub iterations: u32,
    /// Total tokens used by the agent
    pub tokens_used: u32,
    /// Any errors encountered (empty if success)
    pub errors: Vec<String>,
    /// When the agent started
    pub started_at: DateTime<Utc>,
    /// When the agent completed
    pub completed_at: DateTime<Utc>,
    /// Optional bead ID if task tracking was enabled
    pub bead_id: Option<String>,
}

impl AgentResult {
    /// Create a new successful result
    pub fn success(
        agent_id: Uuid,
        agent_name: String,
        output: String,
        summary: String,
        started_at: DateTime<Utc>,
    ) -> Self {
        Self {
            agent_id,
            agent_name,
            success: true,
            output,
            summary,
            files_changed: Vec::new(),
            files_read: Vec::new(),
            iterations: 0,
            tokens_used: 0,
            errors: Vec::new(),
            started_at,
            completed_at: Utc::now(),
            bead_id: None,
        }
    }

    /// Create a failed result
    pub fn failure(
        agent_id: Uuid,
        agent_name: String,
        errors: Vec<String>,
        started_at: DateTime<Utc>,
    ) -> Self {
        Self {
            agent_id,
            agent_name,
            success: false,
            output: String::new(),
            summary: format!("Agent failed: {}", errors.join(", ")),
            files_changed: Vec::new(),
            files_read: Vec::new(),
            iterations: 0,
            tokens_used: 0,
            errors,
            started_at,
            completed_at: Utc::now(),
            bead_id: None,
        }
    }

    /// Set files changed
    pub fn with_files_changed(mut self, files: Vec<PathBuf>) -> Self {
        self.files_changed = files;
        self
    }

    /// Set files read
    pub fn with_files_read(mut self, files: Vec<PathBuf>) -> Self {
        self.files_read = files;
        self
    }

    /// Set iteration count
    pub fn with_iterations(mut self, iterations: u32) -> Self {
        self.iterations = iterations;
        self
    }

    /// Set token usage
    pub fn with_tokens_used(mut self, tokens: u32) -> Self {
        self.tokens_used = tokens;
        self
    }

    /// Set bead ID
    pub fn with_bead_id(mut self, bead_id: String) -> Self {
        self.bead_id = Some(bead_id);
        self
    }

    /// Get duration of agent execution
    pub fn duration(&self) -> chrono::Duration {
        self.completed_at - self.started_at
    }

    /// Format as a display string for the parent agent
    pub fn format_for_parent(&self) -> String {
        let status = if self.success { "Success" } else { "Failed" };
        let duration = self.duration();
        let duration_str = if duration.num_seconds() < 60 {
            format!("{}s", duration.num_seconds())
        } else {
            format!(
                "{}m {}s",
                duration.num_minutes(),
                duration.num_seconds() % 60
            )
        };

        let mut result = format!(
            "=== Agent '{}' {} ===\n\n\
             Status: {}\n\
             Iterations: {}\n\
             Tokens used: {}\n\
             Duration: {}\n",
            self.agent_name,
            if self.success { "Completed" } else { "Failed" },
            status,
            self.iterations,
            self.tokens_used,
            duration_str,
        );

        if !self.files_changed.is_empty() {
            result.push_str("\n## Files Changed\n");
            for file in &self.files_changed {
                result.push_str(&format!("- {}\n", file.display()));
            }
        }

        if !self.files_read.is_empty() {
            result.push_str("\n## Files Read\n");
            for file in &self.files_read {
                result.push_str(&format!("- {}\n", file.display()));
            }
        }

        result.push_str(&format!("\n## Summary\n{}\n", self.summary));

        if !self.errors.is_empty() {
            result.push_str("\n## Errors\n");
            for error in &self.errors {
                result.push_str(&format!("- {}\n", error));
            }
        }

        result
    }
}

/// Status of a running or completed agent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentStatus {
    /// Agent is queued but not yet started
    Pending,
    /// Agent is currently running
    Running,
    /// Agent completed successfully
    Completed,
    /// Agent failed with errors
    Failed,
    /// Agent was cancelled
    Cancelled,
}

/// A handle to a background agent
#[derive(Debug, Clone)]
pub struct AgentHandle {
    /// The agent's unique ID
    pub id: Uuid,
    /// The agent's display name
    pub name: String,
    /// Current status
    pub status: AgentStatus,
    /// When the agent started (None if pending)
    pub started_at: Option<DateTime<Utc>>,
}

impl AgentHandle {
    /// Create a new pending agent handle
    pub fn new(id: Uuid, name: String) -> Self {
        Self {
            id,
            name,
            status: AgentStatus::Pending,
            started_at: None,
        }
    }

    /// Mark as running
    pub fn start(&mut self) {
        self.status = AgentStatus::Running;
        self.started_at = Some(Utc::now());
    }

    /// Check if the agent is still running
    pub fn is_running(&self) -> bool {
        self.status == AgentStatus::Running
    }
}

/// Tool permissions for an agent type
#[derive(Debug, Clone, Default)]
pub struct ToolPermissions {
    /// Tools that are allowed
    pub allowed: HashSet<String>,
    /// Tools that are explicitly denied
    pub denied: HashSet<String>,
}

impl ToolPermissions {
    /// Create permissions allowing specific tools
    pub fn allow(tools: &[&str]) -> Self {
        Self {
            allowed: tools.iter().map(|s| s.to_string()).collect(),
            denied: HashSet::new(),
        }
    }

    /// Create permissions denying specific tools
    pub fn deny(tools: &[&str]) -> Self {
        Self {
            allowed: HashSet::new(),
            denied: tools.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Check if a tool is permitted
    pub fn is_allowed(&self, tool: &str) -> bool {
        if !self.denied.is_empty() && self.denied.contains(tool) {
            return false;
        }
        if !self.allowed.is_empty() {
            return self.allowed.contains(tool);
        }
        true
    }

    /// Merge with another set of permissions (for caps + skill)
    pub fn merge(&mut self, other: &ToolPermissions) {
        self.allowed.extend(other.allowed.iter().cloned());
        self.denied.extend(other.denied.iter().cloned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_new() {
        let config = AgentConfig::new("explore", "Find auth files", PathBuf::from("/project"));

        assert_eq!(config.agent_type, "explore");
        assert_eq!(config.task, "Find auth files");
        assert!(config.name.starts_with("explore-"));
        assert_eq!(config.max_iterations, 30);
    }

    #[test]
    fn test_agent_config_builder() {
        let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/project"))
            .with_caps(vec!["testing".to_string()])
            .with_skill("rust-async".to_string())
            .with_max_iterations(50)
            .with_background(true);

        assert_eq!(config.caps, vec!["testing"]);
        assert_eq!(config.skill, Some("rust-async".to_string()));
        assert_eq!(config.max_iterations, 50);
        assert!(config.background);
    }

    #[test]
    fn test_memory_strategy_default() {
        let strategy = MemoryStrategy::default();
        assert!(matches!(strategy, MemoryStrategy::Full));
    }

    #[test]
    fn test_memory_strategy_summarizing() {
        let strategy = MemoryStrategy::summarizing();
        if let MemoryStrategy::Summarizing { threshold, target } = strategy {
            assert_eq!(threshold, 50_000);
            assert_eq!(target, 20_000);
        } else {
            panic!("Expected Summarizing strategy");
        }
    }

    #[test]
    fn test_agent_result_success() {
        let id = Uuid::new_v4();
        let started = Utc::now();
        let result = AgentResult::success(
            id,
            "explore-a1b2".to_string(),
            "Full output".to_string(),
            "Found 5 files".to_string(),
            started,
        );

        assert!(result.success);
        assert!(result.errors.is_empty());
        assert_eq!(result.summary, "Found 5 files");
    }

    #[test]
    fn test_agent_result_failure() {
        let id = Uuid::new_v4();
        let started = Utc::now();
        let result = AgentResult::failure(
            id,
            "explore-a1b2".to_string(),
            vec!["Timeout".to_string()],
            started,
        );

        assert!(!result.success);
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_tool_permissions_allow() {
        let perms = ToolPermissions::allow(&["file_read", "glob", "grep"]);

        assert!(perms.is_allowed("file_read"));
        assert!(perms.is_allowed("glob"));
        assert!(!perms.is_allowed("file_write"));
    }

    #[test]
    fn test_tool_permissions_deny() {
        let perms = ToolPermissions::deny(&["shell", "file_write"]);

        assert!(!perms.is_allowed("shell"));
        assert!(!perms.is_allowed("file_write"));
        assert!(perms.is_allowed("file_read"));
    }

    #[test]
    fn test_tool_permissions_merge() {
        let mut perms1 = ToolPermissions::allow(&["file_read"]);
        let perms2 = ToolPermissions::allow(&["glob"]);

        perms1.merge(&perms2);

        assert!(perms1.is_allowed("file_read"));
        assert!(perms1.is_allowed("glob"));
    }

    #[test]
    fn test_agent_handle() {
        let id = Uuid::new_v4();
        let mut handle = AgentHandle::new(id, "test-agent".to_string());

        assert_eq!(handle.status, AgentStatus::Pending);
        assert!(!handle.is_running());

        handle.start();
        assert!(handle.is_running());
        assert!(handle.started_at.is_some());
    }

    // ==================== Additional MemoryStrategy Tests ====================

    #[test]
    fn test_memory_strategy_windowed() {
        let strategy = MemoryStrategy::windowed(25);
        if let MemoryStrategy::Windowed { window_size } = strategy {
            assert_eq!(window_size, 25);
        } else {
            panic!("Expected Windowed strategy");
        }
    }

    #[test]
    fn test_memory_strategy_windowed_zero() {
        let strategy = MemoryStrategy::windowed(0);
        if let MemoryStrategy::Windowed { window_size } = strategy {
            assert_eq!(window_size, 0);
        } else {
            panic!("Expected Windowed strategy");
        }
    }

    #[test]
    fn test_memory_strategy_clone() {
        let strategy = MemoryStrategy::summarizing();
        let cloned = strategy.clone();
        if let MemoryStrategy::Summarizing { threshold, target } = cloned {
            assert_eq!(threshold, 50_000);
            assert_eq!(target, 20_000);
        } else {
            panic!("Expected Summarizing strategy");
        }
    }

    #[test]
    fn test_memory_strategy_debug() {
        let strategy = MemoryStrategy::Full;
        let debug_str = format!("{:?}", strategy);
        assert!(debug_str.contains("Full"));
    }

    #[test]
    fn test_memory_strategy_serialize() {
        let strategy = MemoryStrategy::windowed(10);
        let json = serde_json::to_string(&strategy).unwrap();
        assert!(json.contains("Windowed"));
        assert!(json.contains("10"));
    }

    #[test]
    fn test_memory_strategy_deserialize() {
        let json = r#"{"Windowed":{"window_size":15}}"#;
        let strategy: MemoryStrategy = serde_json::from_str(json).unwrap();
        if let MemoryStrategy::Windowed { window_size } = strategy {
            assert_eq!(window_size, 15);
        } else {
            panic!("Expected Windowed strategy");
        }
    }

    // ==================== Additional AgentConfig Tests ====================

    #[test]
    fn test_agent_config_with_token_budget() {
        let config = AgentConfig::new("explore", "task", PathBuf::from("/"))
            .with_token_budget(200_000);
        assert_eq!(config.token_budget, 200_000);
    }

    #[test]
    fn test_agent_config_with_parent() {
        let parent_id = Uuid::new_v4();
        let config = AgentConfig::new("explore", "task", PathBuf::from("/"))
            .with_parent(parent_id);
        assert_eq!(config.parent_id, Some(parent_id));
    }

    #[test]
    fn test_agent_config_with_model() {
        let config = AgentConfig::new("explore", "task", PathBuf::from("/"))
            .with_model("claude-opus".to_string());
        assert_eq!(config.model, Some("claude-opus".to_string()));
    }

    #[test]
    fn test_agent_config_default_values() {
        let config = AgentConfig::new("explore", "task", PathBuf::from("/project"));

        assert_eq!(config.max_iterations, 30);
        assert_eq!(config.token_budget, 100_000);
        assert!(!config.background);
        assert!(config.parent_id.is_none());
        assert!(config.bead_id.is_none());
        assert!(config.model.is_none());
        assert!(config.caps.is_empty());
        assert!(config.skill.is_none());
        assert!(matches!(config.memory_strategy, MemoryStrategy::Full));
    }

    #[test]
    fn test_agent_config_name_format() {
        let config = AgentConfig::new("implement", "task", PathBuf::from("/"));
        // Name should be "implement-XXXX" where XXXX is first 4 chars of UUID
        assert!(config.name.starts_with("implement-"));
        assert_eq!(config.name.len(), "implement-".len() + 4);
    }

    #[test]
    fn test_agent_config_full_chain() {
        let parent_id = Uuid::new_v4();
        let config = AgentConfig::new("implement", "Write tests", PathBuf::from("/project"))
            .with_caps(vec!["testing".to_string(), "code".to_string()])
            .with_skill("rust".to_string())
            .with_memory_strategy(MemoryStrategy::windowed(30))
            .with_max_iterations(100)
            .with_token_budget(150_000)
            .with_background(true)
            .with_parent(parent_id)
            .with_bead("bead-123".to_string())
            .with_model("claude-sonnet".to_string());

        assert_eq!(config.caps.len(), 2);
        assert_eq!(config.skill, Some("rust".to_string()));
        assert!(matches!(config.memory_strategy, MemoryStrategy::Windowed { .. }));
        assert_eq!(config.max_iterations, 100);
        assert_eq!(config.token_budget, 150_000);
        assert!(config.background);
        assert_eq!(config.parent_id, Some(parent_id));
        assert_eq!(config.bead_id, Some("bead-123".to_string()));
        assert_eq!(config.model, Some("claude-sonnet".to_string()));
    }

    #[test]
    fn test_agent_config_serialize() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("explore"));
        assert!(json.contains("Find files"));
    }

    #[test]
    fn test_agent_config_debug() {
        let config = AgentConfig::new("explore", "task", PathBuf::from("/"));
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("AgentConfig"));
        assert!(debug_str.contains("explore"));
    }

    // ==================== Additional AgentResult Tests ====================

    #[test]
    fn test_agent_result_with_files_changed() {
        let id = Uuid::new_v4();
        let started = Utc::now();
        let result = AgentResult::success(id, "agent".to_string(), "output".to_string(), "summary".to_string(), started)
            .with_files_changed(vec![PathBuf::from("/src/main.rs"), PathBuf::from("/src/lib.rs")]);

        assert_eq!(result.files_changed.len(), 2);
        assert_eq!(result.files_changed[0], PathBuf::from("/src/main.rs"));
    }

    #[test]
    fn test_agent_result_with_files_read() {
        let id = Uuid::new_v4();
        let started = Utc::now();
        let result = AgentResult::success(id, "agent".to_string(), "output".to_string(), "summary".to_string(), started)
            .with_files_read(vec![PathBuf::from("/Cargo.toml")]);

        assert_eq!(result.files_read.len(), 1);
    }

    #[test]
    fn test_agent_result_with_iterations() {
        let id = Uuid::new_v4();
        let started = Utc::now();
        let result = AgentResult::success(id, "agent".to_string(), "output".to_string(), "summary".to_string(), started)
            .with_iterations(15);

        assert_eq!(result.iterations, 15);
    }

    #[test]
    fn test_agent_result_with_tokens_used() {
        let id = Uuid::new_v4();
        let started = Utc::now();
        let result = AgentResult::success(id, "agent".to_string(), "output".to_string(), "summary".to_string(), started)
            .with_tokens_used(50_000);

        assert_eq!(result.tokens_used, 50_000);
    }

    #[test]
    fn test_agent_result_with_bead_id() {
        let id = Uuid::new_v4();
        let started = Utc::now();
        let result = AgentResult::success(id, "agent".to_string(), "output".to_string(), "summary".to_string(), started)
            .with_bead_id("task-456".to_string());

        assert_eq!(result.bead_id, Some("task-456".to_string()));
    }

    #[test]
    fn test_agent_result_duration() {
        let id = Uuid::new_v4();
        let started = Utc::now() - chrono::Duration::seconds(60);
        let result = AgentResult::success(id, "agent".to_string(), "output".to_string(), "summary".to_string(), started);

        let duration = result.duration();
        // Duration should be approximately 60 seconds (allow some tolerance)
        assert!(duration.num_seconds() >= 59);
        assert!(duration.num_seconds() <= 61);
    }

    #[test]
    fn test_agent_result_format_for_parent_success() {
        let id = Uuid::new_v4();
        let started = Utc::now() - chrono::Duration::seconds(30);
        let result = AgentResult::success(
            id,
            "explore-a1b2".to_string(),
            "Found the files".to_string(),
            "Located 3 matching files".to_string(),
            started,
        )
        .with_iterations(5)
        .with_tokens_used(1500)
        .with_files_read(vec![PathBuf::from("/src/main.rs")]);

        let formatted = result.format_for_parent();

        assert!(formatted.contains("explore-a1b2"));
        assert!(formatted.contains("Completed"));
        assert!(formatted.contains("Success"));
        assert!(formatted.contains("Iterations: 5"));
        assert!(formatted.contains("Tokens used: 1500"));
        assert!(formatted.contains("Located 3 matching files"));
        assert!(formatted.contains("Files Read"));
        assert!(formatted.contains("main.rs"));
    }

    #[test]
    fn test_agent_result_format_for_parent_failure() {
        let id = Uuid::new_v4();
        let started = Utc::now();
        let result = AgentResult::failure(
            id,
            "implement-c3d4".to_string(),
            vec!["Timeout".to_string(), "Max iterations reached".to_string()],
            started,
        );

        let formatted = result.format_for_parent();

        assert!(formatted.contains("implement-c3d4"));
        assert!(formatted.contains("Failed"));
        assert!(formatted.contains("Errors"));
        assert!(formatted.contains("Timeout"));
        assert!(formatted.contains("Max iterations reached"));
    }

    #[test]
    fn test_agent_result_format_for_parent_with_files_changed() {
        let id = Uuid::new_v4();
        let started = Utc::now();
        let result = AgentResult::success(id, "agent".to_string(), "output".to_string(), "summary".to_string(), started)
            .with_files_changed(vec![PathBuf::from("/src/lib.rs"), PathBuf::from("/src/main.rs")]);

        let formatted = result.format_for_parent();

        assert!(formatted.contains("Files Changed"));
        assert!(formatted.contains("lib.rs"));
        assert!(formatted.contains("main.rs"));
    }

    #[test]
    fn test_agent_result_format_for_parent_duration_minutes() {
        let id = Uuid::new_v4();
        let started = Utc::now() - chrono::Duration::minutes(2) - chrono::Duration::seconds(30);
        let result = AgentResult::success(id, "agent".to_string(), "output".to_string(), "summary".to_string(), started);

        let formatted = result.format_for_parent();

        // Should show "2m 30s" format
        assert!(formatted.contains("2m"));
    }

    #[test]
    fn test_agent_result_format_for_parent_duration_seconds_only() {
        let id = Uuid::new_v4();
        let started = Utc::now() - chrono::Duration::seconds(45);
        let result = AgentResult::success(id, "agent".to_string(), "output".to_string(), "summary".to_string(), started);

        let formatted = result.format_for_parent();

        // Should show "45s" format (no minutes)
        assert!(formatted.contains("s"));
        assert!(!formatted.contains("m "));
    }

    #[test]
    fn test_agent_result_failure_summary_format() {
        let id = Uuid::new_v4();
        let started = Utc::now();
        let result = AgentResult::failure(
            id,
            "agent".to_string(),
            vec!["Error 1".to_string(), "Error 2".to_string()],
            started,
        );

        assert!(result.summary.starts_with("Agent failed:"));
        assert!(result.summary.contains("Error 1"));
        assert!(result.summary.contains("Error 2"));
    }

    #[test]
    fn test_agent_result_serialize() {
        let id = Uuid::new_v4();
        let started = Utc::now();
        let result = AgentResult::success(id, "agent".to_string(), "output".to_string(), "summary".to_string(), started);

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("success"));
        assert!(json.contains("output"));
    }

    // ==================== Additional AgentStatus Tests ====================

    #[test]
    fn test_agent_status_variants() {
        assert_eq!(AgentStatus::Pending, AgentStatus::Pending);
        assert_eq!(AgentStatus::Running, AgentStatus::Running);
        assert_eq!(AgentStatus::Completed, AgentStatus::Completed);
        assert_eq!(AgentStatus::Failed, AgentStatus::Failed);
        assert_eq!(AgentStatus::Cancelled, AgentStatus::Cancelled);
    }

    #[test]
    fn test_agent_status_ne() {
        assert_ne!(AgentStatus::Pending, AgentStatus::Running);
        assert_ne!(AgentStatus::Running, AgentStatus::Completed);
        assert_ne!(AgentStatus::Completed, AgentStatus::Failed);
        assert_ne!(AgentStatus::Failed, AgentStatus::Cancelled);
    }

    #[test]
    fn test_agent_status_clone() {
        let status = AgentStatus::Running;
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }

    #[test]
    fn test_agent_status_debug() {
        let status = AgentStatus::Completed;
        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("Completed"));
    }

    #[test]
    fn test_agent_status_serialize() {
        let status = AgentStatus::Failed;
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("Failed"));
    }

    // ==================== Additional AgentHandle Tests ====================

    #[test]
    fn test_agent_handle_initial_state() {
        let id = Uuid::new_v4();
        let handle = AgentHandle::new(id, "test-agent".to_string());

        assert_eq!(handle.id, id);
        assert_eq!(handle.name, "test-agent");
        assert_eq!(handle.status, AgentStatus::Pending);
        assert!(handle.started_at.is_none());
    }

    #[test]
    fn test_agent_handle_start_sets_timestamp() {
        let id = Uuid::new_v4();
        let mut handle = AgentHandle::new(id, "test".to_string());

        let before = Utc::now();
        handle.start();
        let after = Utc::now();

        assert!(handle.started_at.is_some());
        let started = handle.started_at.unwrap();
        assert!(started >= before);
        assert!(started <= after);
    }

    #[test]
    fn test_agent_handle_is_running_states() {
        let id = Uuid::new_v4();
        let mut handle = AgentHandle::new(id, "test".to_string());

        // Pending is not running
        assert!(!handle.is_running());

        // Running is running
        handle.start();
        assert!(handle.is_running());

        // Completed is not running
        handle.status = AgentStatus::Completed;
        assert!(!handle.is_running());

        // Failed is not running
        handle.status = AgentStatus::Failed;
        assert!(!handle.is_running());

        // Cancelled is not running
        handle.status = AgentStatus::Cancelled;
        assert!(!handle.is_running());
    }

    #[test]
    fn test_agent_handle_clone() {
        let id = Uuid::new_v4();
        let handle = AgentHandle::new(id, "test".to_string());
        let cloned = handle.clone();

        assert_eq!(cloned.id, handle.id);
        assert_eq!(cloned.name, handle.name);
        assert_eq!(cloned.status, handle.status);
    }

    #[test]
    fn test_agent_handle_debug() {
        let id = Uuid::new_v4();
        let handle = AgentHandle::new(id, "test-agent".to_string());
        let debug_str = format!("{:?}", handle);

        assert!(debug_str.contains("AgentHandle"));
        assert!(debug_str.contains("test-agent"));
    }

    // ==================== Additional ToolPermissions Tests ====================

    #[test]
    fn test_tool_permissions_default() {
        let perms = ToolPermissions::default();
        assert!(perms.allowed.is_empty());
        assert!(perms.denied.is_empty());
    }

    #[test]
    fn test_tool_permissions_is_allowed_empty() {
        let perms = ToolPermissions::default();
        // When both allowed and denied are empty, everything is allowed
        assert!(perms.is_allowed("any_tool"));
        assert!(perms.is_allowed("file_read"));
        assert!(perms.is_allowed("shell"));
    }

    #[test]
    fn test_tool_permissions_deny_takes_precedence() {
        let mut perms = ToolPermissions::allow(&["file_read", "shell"]);
        perms.denied.insert("shell".to_string());

        // shell should be denied even though it's in allowed
        assert!(!perms.is_allowed("shell"));
        assert!(perms.is_allowed("file_read"));
    }

    #[test]
    fn test_tool_permissions_merge_allowed() {
        let mut perms1 = ToolPermissions::allow(&["tool_a"]);
        let perms2 = ToolPermissions::allow(&["tool_b", "tool_c"]);

        perms1.merge(&perms2);

        assert!(perms1.is_allowed("tool_a"));
        assert!(perms1.is_allowed("tool_b"));
        assert!(perms1.is_allowed("tool_c"));
        assert_eq!(perms1.allowed.len(), 3);
    }

    #[test]
    fn test_tool_permissions_merge_denied() {
        let mut perms1 = ToolPermissions::deny(&["tool_a"]);
        let perms2 = ToolPermissions::deny(&["tool_b"]);

        perms1.merge(&perms2);

        assert!(!perms1.is_allowed("tool_a"));
        assert!(!perms1.is_allowed("tool_b"));
        assert_eq!(perms1.denied.len(), 2);
    }

    #[test]
    fn test_tool_permissions_merge_mixed() {
        let mut perms1 = ToolPermissions::allow(&["read"]);
        let mut perms2 = ToolPermissions::default();
        perms2.denied.insert("write".to_string());

        perms1.merge(&perms2);

        assert!(perms1.is_allowed("read"));
        assert!(!perms1.is_allowed("write"));
    }

    #[test]
    fn test_tool_permissions_clone() {
        let perms = ToolPermissions::allow(&["tool_a", "tool_b"]);
        let cloned = perms.clone();

        assert_eq!(cloned.allowed.len(), perms.allowed.len());
        assert!(cloned.is_allowed("tool_a"));
    }

    #[test]
    fn test_tool_permissions_debug() {
        let perms = ToolPermissions::allow(&["test_tool"]);
        let debug_str = format!("{:?}", perms);

        assert!(debug_str.contains("ToolPermissions"));
        assert!(debug_str.contains("test_tool"));
    }
}
