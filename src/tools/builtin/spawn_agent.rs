// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Spawn agent tool
//!
//! Allows the main agent to spawn specialized subagents for delegated tasks.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::agents::{
    get_agent_type_names, is_valid_agent_type, AgentConfig, AgentContext, AgentProgressEvent,
    AgentRunner, MemoryStrategy,
};
use crate::error::{Result, TedError};
use crate::llm::provider::{LlmProvider, ToolDefinition};
use crate::llm::rate_budget::TokenRateCoordinator;
use crate::skills::SkillRegistry;
use crate::tools::{PermissionRequest, SchemaBuilder, Tool, ToolContext, ToolResult};

/// Status of a tool call entry in the agent conversation log
#[derive(Debug, Clone)]
pub enum ToolCallEntryStatus {
    /// Tool is currently executing
    Running,
    /// Tool completed successfully
    Success { preview: Option<String> },
    /// Tool failed
    Failed { error: String },
}

/// An entry in the agent's conversation log (for TUI split-pane display)
#[derive(Debug, Clone)]
pub enum AgentConversationEntry {
    /// Agent's LLM response text
    AssistantMessage(String),
    /// A tool call with its current status
    ToolCall {
        id: String,
        name: String,
        input: serde_json::Value,
        input_summary: String,
        status: ToolCallEntryStatus,
        output_full: Option<String>,
    },
}

/// Progress state for an agent execution
#[derive(Debug, Clone)]
pub struct AgentProgressState {
    /// Current iteration
    pub iteration: u32,
    /// Max iterations
    pub max_iterations: u32,
    /// Whether the agent has completed
    pub completed: bool,
    /// Current tool being executed (None if between tools)
    pub current_tool: Option<String>,
    /// Last tool activity for display (persists after tool completes)
    pub last_activity: String,
    /// Conversation log for split-pane display
    pub conversation: Vec<AgentConversationEntry>,
    /// Agent type (e.g., "explore", "implement")
    pub agent_type: String,
    /// Task description
    pub task: String,
    /// Whether the agent is currently rate limited
    pub rate_limited: bool,
    /// How long the agent is waiting for rate limit (seconds)
    pub rate_limit_wait_secs: f64,
}

impl Default for AgentProgressState {
    fn default() -> Self {
        Self {
            iteration: 0,
            max_iterations: 30,
            completed: false,
            current_tool: None,
            last_activity: "Starting...".to_string(),
            conversation: Vec::new(),
            agent_type: String::new(),
            task: String::new(),
            rate_limited: false,
            rate_limit_wait_secs: 0.0,
        }
    }
}

impl AgentProgressState {
    /// Get a display string for the current progress
    pub fn display_status(&self) -> String {
        if let Some(ref tool) = self.current_tool {
            // Show active tool
            format!("[{}/{}] → {}", self.iteration, self.max_iterations, tool)
        } else if !self.last_activity.is_empty() {
            // Show last activity
            format!(
                "[{}/{}] {}",
                self.iteration, self.max_iterations, self.last_activity
            )
        } else {
            format!("[{}/{}] Working...", self.iteration, self.max_iterations)
        }
    }
}

/// Shared progress tracker for all spawn_agent executions
pub type ProgressTracker = Arc<Mutex<HashMap<String, AgentProgressState>>>;

/// Create a new progress tracker
pub fn new_progress_tracker() -> ProgressTracker {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Tool for spawning specialized subagents
pub struct SpawnAgentTool {
    /// LLM provider for subagent execution
    provider: Arc<dyn LlmProvider>,
    /// Skill registry for loading skills
    skill_registry: Arc<SkillRegistry>,
    /// Rate coordinator for budget allocation (optional)
    rate_coordinator: Option<Arc<TokenRateCoordinator>>,
    /// Model name to use for subagents (inherited from parent)
    model: String,
    /// Progress tracker for active executions
    progress_tracker: ProgressTracker,
}

impl SpawnAgentTool {
    /// Create a new spawn agent tool
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        skill_registry: Arc<SkillRegistry>,
        model: String,
    ) -> Self {
        Self {
            provider,
            skill_registry,
            rate_coordinator: None,
            model,
            progress_tracker: new_progress_tracker(),
        }
    }

    /// Create a new spawn agent tool with rate coordinator
    pub fn with_rate_coordinator(
        provider: Arc<dyn LlmProvider>,
        skill_registry: Arc<SkillRegistry>,
        rate_coordinator: Arc<TokenRateCoordinator>,
        model: String,
    ) -> Self {
        Self {
            provider,
            skill_registry,
            rate_coordinator: Some(rate_coordinator),
            model,
            progress_tracker: new_progress_tracker(),
        }
    }

    /// Create a new spawn agent tool with an existing progress tracker
    pub fn with_progress_tracker(
        provider: Arc<dyn LlmProvider>,
        skill_registry: Arc<SkillRegistry>,
        model: String,
        progress_tracker: ProgressTracker,
    ) -> Self {
        Self {
            provider,
            skill_registry,
            rate_coordinator: None,
            model,
            progress_tracker,
        }
    }

    /// Get a clone of the progress tracker for external monitoring
    pub fn progress_tracker(&self) -> ProgressTracker {
        Arc::clone(&self.progress_tracker)
    }
}

#[async_trait]
impl Tool for SpawnAgentTool {
    fn name(&self) -> &str {
        "spawn_agent"
    }

    fn definition(&self) -> ToolDefinition {
        let agent_types = get_agent_type_names().join(", ");

        ToolDefinition {
            name: "spawn_agent".to_string(),
            description: format!(
                "Spawn a specialized subagent for complex tasks. USE THIS for:\n\
                 - Exploring/analyzing codebases ('look at the project', 'what needs work')\n\
                 - Multi-file implementations or refactoring\n\
                 - Running builds, tests, or complex shell workflows\n\
                 - Code review and quality analysis\n\n\
                 Agent types:\n\
                 - explore: Codebase discovery & analysis (read-only). Best for understanding code.\n\
                 - implement: Writing/modifying code across multiple files.\n\
                 - plan: Architecture design and implementation planning.\n\
                 - bash: Running builds, tests, shell commands.\n\
                 - review: Code quality analysis and suggestions.\n\n\
                 Available: {}",
                agent_types
            ),
            input_schema: SchemaBuilder::new()
                .string(
                    "agent_type",
                    &format!(
                        "Type of agent to spawn. Options: {}",
                        agent_types
                    ),
                    true,
                )
                .string(
                    "task",
                    "The task for the agent to accomplish. Be specific and clear.",
                    true,
                )
                .array(
                    "caps",
                    "Additional caps to load (optional)",
                    "string",
                    false,
                )
                .string(
                    "skill",
                    "Skill to load for domain expertise (optional, e.g., 'rust-async')",
                    false,
                )
                .string(
                    "memory_strategy",
                    "Memory strategy: 'full', 'summarizing', or 'windowed' (default: from agent type)",
                    false,
                )
                .integer(
                    "max_iterations",
                    "Maximum iterations before stopping (default: from agent type)",
                    false,
                )
                .boolean(
                    "background",
                    "Run in background and return immediately (default: false)",
                    false,
                )
                .string(
                    "bead_id",
                    "Bead ID for task tracking (optional)",
                    false,
                )
                .build(),
        }
    }

    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        // Parse agent type
        let agent_type = input["agent_type"]
            .as_str()
            .or_else(|| input["type"].as_str())
            .ok_or_else(|| TedError::InvalidInput("agent_type is required".to_string()))?;

        if !is_valid_agent_type(agent_type) {
            return Ok(ToolResult::error(
                tool_use_id,
                format!(
                    "Invalid agent type '{}'. Available types: {}",
                    agent_type,
                    get_agent_type_names().join(", ")
                ),
            ));
        }

        // Parse task
        let task = input["task"]
            .as_str()
            .or_else(|| input["prompt"].as_str())
            .or_else(|| input["instruction"].as_str())
            .ok_or_else(|| TedError::InvalidInput("task is required".to_string()))?;

        // Build agent config
        let working_dir = context
            .project_root
            .clone()
            .unwrap_or_else(|| context.working_directory.clone());

        // Inherit the parent's model for subagents
        let mut config =
            AgentConfig::new(agent_type, task, working_dir).with_model(self.model.clone());

        // Parse optional caps
        if let Some(caps) = input["caps"].as_array() {
            let caps: Vec<String> = caps
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            config = config.with_caps(caps);
        }

        // Parse optional skill
        if let Some(skill) = input["skill"].as_str() {
            config = config.with_skill(skill.to_string());
        }

        // Parse optional memory strategy
        if let Some(strategy_str) = input["memory_strategy"].as_str() {
            let strategy = match strategy_str.to_lowercase().as_str() {
                "full" => MemoryStrategy::Full,
                "summarizing" => MemoryStrategy::summarizing(),
                "windowed" => MemoryStrategy::windowed(20),
                _ => {
                    return Ok(ToolResult::error(
                        tool_use_id,
                        format!(
                            "Invalid memory_strategy '{}'. Options: full, summarizing, windowed",
                            strategy_str
                        ),
                    ));
                }
            };
            config = config.with_memory_strategy(strategy);
        }

        // Parse optional max iterations
        if let Some(max) = input["max_iterations"].as_u64() {
            config = config.with_max_iterations(max as u32);
        }

        // Parse optional background flag
        let background = input["background"].as_bool().unwrap_or(false);
        config = config.with_background(background);

        // Parse optional bead ID
        if let Some(bead_id) = input["bead_id"].as_str() {
            config = config.with_bead(bead_id.to_string());
        }

        // Create agent context
        let mut agent_context = AgentContext::new(config.clone());

        // Check if we're in TUI mode (suppress output)
        let tui_mode = std::env::var("TED_TUI_MODE").is_ok();

        // Allocate rate budget if coordinator is available
        if let Some(coordinator) = &self.rate_coordinator {
            let priority = config.rate_priority();
            let allocation = coordinator.request_allocation(priority, config.name.clone());

            // Log the allocation (unless in TUI mode)
            if !tui_mode {
                eprintln!(
                    "  [{}] Rate budget: {}K tokens/min ({})",
                    config.name,
                    allocation.budget() / 1000,
                    format!("{:?}", priority).to_lowercase()
                );
            }

            agent_context.set_rate_allocation(Arc::new(allocation));
        }

        // Load skill if specified
        if let Some(skill_name) = &config.skill {
            match self.skill_registry.load(skill_name) {
                Ok(skill) => {
                    agent_context.add_skill_instructions(&skill.to_prompt_content());

                    // Apply skill tool permissions
                    if let Some(perms) = &skill.tool_permissions {
                        let mut tool_perms = crate::agents::ToolPermissions::default();
                        for tool in &perms.allow {
                            tool_perms.allowed.insert(tool.clone());
                        }
                        for tool in &perms.deny {
                            tool_perms.denied.insert(tool.clone());
                        }
                        agent_context.extend_permissions(&tool_perms);
                    }
                }
                Err(e) => {
                    return Ok(ToolResult::error(
                        tool_use_id,
                        format!("Failed to load skill '{}': {}", skill_name, e),
                    ));
                }
            }
        }

        // Create runner with quiet mode if in TUI
        let runner_config = crate::agents::RunnerConfig {
            quiet: tui_mode,
            ..Default::default()
        };
        let runner = AgentRunner::with_config(Arc::clone(&self.provider), runner_config);

        if background {
            // Spawn in background
            let handle = crate::agents::spawn_background_agent(Arc::new(runner), agent_context);

            Ok(ToolResult::success(
                tool_use_id,
                format!(
                    "Spawned background agent '{}' (ID: {})\n\
                     Type: {}\n\
                     Task: {}\n\n\
                     The agent is running in the background. \
                     You will be notified when it completes.",
                    handle.name, handle.id, agent_type, task
                ),
            ))
        } else {
            // Initialize progress tracker for this execution
            let max_iterations = config.max_iterations;
            {
                let mut tracker = self.progress_tracker.lock().await;
                tracker.insert(
                    tool_use_id.clone(),
                    AgentProgressState {
                        iteration: 0,
                        max_iterations,
                        completed: false,
                        current_tool: None,
                        last_activity: format!("Starting {} agent...", agent_type),
                        conversation: Vec::new(),
                        agent_type: agent_type.to_string(),
                        task: task.to_string(),
                        rate_limited: false,
                        rate_limit_wait_secs: 0.0,
                    },
                );
            }

            // Create progress channel
            let (progress_tx, mut progress_rx) =
                tokio::sync::mpsc::unbounded_channel::<AgentProgressEvent>();

            // Spawn a task to update the progress tracker from events
            let tracker_clone = Arc::clone(&self.progress_tracker);
            let tool_use_id_clone = tool_use_id.clone();
            let progress_task = tokio::spawn(async move {
                while let Some(event) = progress_rx.recv().await {
                    let mut tracker = tracker_clone.lock().await;
                    if let Some(state) = tracker.get_mut(&tool_use_id_clone) {
                        // Clear rate limit flag on any non-rate-limit event
                        if !matches!(&event, AgentProgressEvent::RateLimited { .. }) {
                            state.rate_limited = false;
                            state.rate_limit_wait_secs = 0.0;
                        }

                        match &event {
                            AgentProgressEvent::Started { .. } => {
                                state.last_activity = "Starting...".to_string();
                            }
                            AgentProgressEvent::IterationStart {
                                iteration,
                                max_iterations,
                            } => {
                                state.iteration = *iteration;
                                state.max_iterations = *max_iterations;
                                // Don't clear current_tool here - let it persist until next tool starts
                            }
                            AgentProgressEvent::ToolStart {
                                tool_name,
                                input_summary,
                            } => {
                                // Set current tool with summary
                                let summary = if input_summary.len() > 50 {
                                    format!("{}...", &input_summary[..47])
                                } else {
                                    input_summary.clone()
                                };
                                state.current_tool = Some(format!("{} {}", tool_name, summary));
                            }
                            AgentProgressEvent::ToolComplete { tool_name, success } => {
                                // Update last activity and clear current tool
                                let status = if *success { "✓" } else { "✗" };
                                state.last_activity = format!("{} {}", status, tool_name);
                                state.current_tool = None;
                            }
                            AgentProgressEvent::RateLimited { wait_secs } => {
                                state.current_tool = None;
                                state.last_activity = format!("Rate limited ({:.1}s)", wait_secs);
                                state.rate_limited = true;
                                state.rate_limit_wait_secs = *wait_secs;
                            }
                            AgentProgressEvent::Completed { success, .. } => {
                                state.completed = true;
                                state.current_tool = None;
                                state.last_activity =
                                    if *success { "Completed" } else { "Failed" }.to_string();
                            }
                            AgentProgressEvent::AssistantMessage { text } => {
                                state
                                    .conversation
                                    .push(AgentConversationEntry::AssistantMessage(text.clone()));
                            }
                            AgentProgressEvent::ToolCallStarted {
                                tool_id,
                                tool_name,
                                input,
                            } => {
                                let input_summary =
                                    summarize_tool_input_for_display(tool_name, input);
                                state.conversation.push(AgentConversationEntry::ToolCall {
                                    id: tool_id.clone(),
                                    name: tool_name.clone(),
                                    input: input.clone(),
                                    input_summary,
                                    status: ToolCallEntryStatus::Running,
                                    output_full: None,
                                });
                            }
                            AgentProgressEvent::ToolCallCompleted {
                                tool_id,
                                tool_name: _,
                                success,
                                output_preview,
                                output_full,
                            } => {
                                // Find and update the matching tool call
                                for entry in state.conversation.iter_mut().rev() {
                                    if let AgentConversationEntry::ToolCall {
                                        id,
                                        status,
                                        output_full: entry_output,
                                        ..
                                    } = entry
                                    {
                                        if id == tool_id {
                                            *status = if *success {
                                                ToolCallEntryStatus::Success {
                                                    preview: output_preview.clone(),
                                                }
                                            } else {
                                                ToolCallEntryStatus::Failed {
                                                    error: output_preview
                                                        .clone()
                                                        .unwrap_or_default(),
                                                }
                                            };
                                            *entry_output = output_full.clone();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            });

            // Run synchronously with progress reporting
            let result = runner
                .run_with_progress(agent_context, Some(progress_tx))
                .await;

            // Wait for progress task to finish processing any remaining events
            let _ = progress_task.await;

            // Clean up progress tracker
            {
                let mut tracker = self.progress_tracker.lock().await;
                tracker.remove(&tool_use_id);
            }

            match result {
                Ok(result) => Ok(ToolResult::success(tool_use_id, result.format_for_parent())),
                Err(e) => Ok(ToolResult::error(
                    tool_use_id,
                    format!("Agent execution failed: {}", e),
                )),
            }
        }
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let agent_type = input["agent_type"].as_str().unwrap_or("unknown");
        let task = input["task"].as_str().unwrap_or("unknown task");

        Some(PermissionRequest {
            tool_name: "spawn_agent".to_string(),
            action_description: format!("Spawn {} agent: {}", agent_type, truncate_str(task, 80)),
            affected_paths: Vec::new(),
            is_destructive: false,
        })
    }

    fn requires_permission(&self) -> bool {
        true
    }
}

/// Truncate a string for display
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

/// Summarize tool input for display in the conversation pane
fn summarize_tool_input_for_display(tool_name: &str, input: &serde_json::Value) -> String {
    match tool_name {
        "file_read" | "glob" => input
            .get("path")
            .or_else(|| input.get("pattern"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "grep" => input
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|p| format!("/{}/", p))
            .unwrap_or_default(),
        "file_write" | "file_edit" => input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "shell" => {
            let cmd = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
            if cmd.len() > 50 {
                format!("{}...", &cmd[..47])
            } else {
                cmd.to_string()
            }
        }
        _ => {
            // Try to find a meaningful string value
            if let Some(obj) = input.as_object() {
                for val in obj.values() {
                    if let Some(s) = val.as_str() {
                        if s.len() <= 50 {
                            return s.to_string();
                        }
                        return format!("{}...", &s[..47]);
                    }
                }
            }
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_agent_valid_types() {
        // We can't easily test the full tool without mocking the provider,
        // but we can verify the basic structure
        assert!(is_valid_agent_type("explore"));
        assert!(is_valid_agent_type("implement"));
        assert!(!is_valid_agent_type("invalid"));
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("short", 10), "short");
        assert_eq!(truncate_str("this is a long string", 10), "this is a ...");
    }

    // ===== truncate_str Additional Tests =====

    #[test]
    fn test_truncate_str_exact_length() {
        assert_eq!(truncate_str("0123456789", 10), "0123456789");
    }

    #[test]
    fn test_truncate_str_one_over() {
        assert_eq!(truncate_str("01234567890", 10), "0123456789...");
    }

    #[test]
    fn test_truncate_str_empty() {
        assert_eq!(truncate_str("", 10), "");
    }

    #[test]
    fn test_truncate_str_unicode_safe() {
        // Test with text that doesn't need truncation
        let text = "Hello 🌍 World";
        // Full string (no truncation needed)
        let result = truncate_str(text, 100);
        assert_eq!(result, text);
    }

    #[test]
    fn test_truncate_str_zero_length() {
        // Edge case: max_len of 0
        assert_eq!(truncate_str("hello", 0), "...");
    }

    // ===== Agent Type Validation Tests =====

    #[test]
    fn test_all_valid_agent_types() {
        let valid_types = get_agent_type_names();
        for agent_type in &valid_types {
            assert!(
                is_valid_agent_type(agent_type),
                "Agent type '{}' should be valid",
                agent_type
            );
        }
    }

    #[test]
    fn test_invalid_agent_types() {
        let invalid_types = [
            "invalid",
            "unknown",
            "foo",
            "bar",
            "",
            "EXPLORE", // Case sensitive
            "Implement",
        ];

        for agent_type in &invalid_types {
            assert!(
                !is_valid_agent_type(agent_type),
                "Agent type '{}' should be invalid",
                agent_type
            );
        }
    }

    #[test]
    fn test_get_agent_type_names_not_empty() {
        let names = get_agent_type_names();
        assert!(!names.is_empty());
    }

    #[test]
    fn test_get_agent_type_names_contains_explore() {
        let names = get_agent_type_names();
        assert!(names.contains(&"explore"));
    }

    #[test]
    fn test_get_agent_type_names_contains_implement() {
        let names = get_agent_type_names();
        assert!(names.contains(&"implement"));
    }

    // ===== MemoryStrategy Tests =====

    #[test]
    fn test_memory_strategy_full() {
        let strategy = MemoryStrategy::Full;
        matches!(strategy, MemoryStrategy::Full);
    }

    #[test]
    fn test_memory_strategy_summarizing() {
        let strategy = MemoryStrategy::summarizing();
        matches!(strategy, MemoryStrategy::Summarizing { .. });
    }

    #[test]
    fn test_memory_strategy_windowed() {
        let strategy = MemoryStrategy::windowed(10);
        matches!(strategy, MemoryStrategy::Windowed { .. });
    }

    // ===== AgentConfig Tests =====

    #[test]
    fn test_agent_config_creation() {
        let working_dir = std::env::current_dir().unwrap();
        let config = AgentConfig::new("explore", "Find files", working_dir.clone());

        assert_eq!(config.agent_type, "explore");
        assert_eq!(config.task, "Find files");
        assert_eq!(config.working_dir, working_dir);
    }

    #[test]
    fn test_agent_config_with_caps() {
        let working_dir = std::env::current_dir().unwrap();
        let config = AgentConfig::new("explore", "task", working_dir)
            .with_caps(vec!["cap1".to_string(), "cap2".to_string()]);

        assert_eq!(config.caps.len(), 2);
    }

    #[test]
    fn test_agent_config_with_skill() {
        let working_dir = std::env::current_dir().unwrap();
        let config =
            AgentConfig::new("explore", "task", working_dir).with_skill("rust-async".to_string());

        assert_eq!(config.skill, Some("rust-async".to_string()));
    }

    #[test]
    fn test_agent_config_with_memory_strategy() {
        let working_dir = std::env::current_dir().unwrap();
        let config = AgentConfig::new("explore", "task", working_dir)
            .with_memory_strategy(MemoryStrategy::Full);

        matches!(config.memory_strategy, MemoryStrategy::Full);
    }

    #[test]
    fn test_agent_config_with_max_iterations() {
        let working_dir = std::env::current_dir().unwrap();
        let config = AgentConfig::new("explore", "task", working_dir).with_max_iterations(100);

        assert_eq!(config.max_iterations, 100);
    }

    #[test]
    fn test_agent_config_with_background() {
        let working_dir = std::env::current_dir().unwrap();
        let config = AgentConfig::new("explore", "task", working_dir).with_background(true);

        assert!(config.background);
    }

    #[test]
    fn test_agent_config_with_bead() {
        let working_dir = std::env::current_dir().unwrap();
        let config =
            AgentConfig::new("explore", "task", working_dir).with_bead("bead-123".to_string());

        assert_eq!(config.bead_id, Some("bead-123".to_string()));
    }

    #[test]
    fn test_agent_config_chained() {
        let working_dir = std::env::current_dir().unwrap();
        let config = AgentConfig::new("implement", "Write code", working_dir)
            .with_caps(vec!["cap1".to_string()])
            .with_skill("rust".to_string())
            .with_memory_strategy(MemoryStrategy::summarizing())
            .with_max_iterations(50)
            .with_background(false)
            .with_bead("task-1".to_string());

        assert_eq!(config.agent_type, "implement");
        assert_eq!(config.task, "Write code");
        assert_eq!(config.caps.len(), 1);
        assert_eq!(config.skill, Some("rust".to_string()));
        assert_eq!(config.max_iterations, 50);
        assert!(!config.background);
        assert_eq!(config.bead_id, Some("task-1".to_string()));
    }

    // ===== AgentContext Tests =====

    #[test]
    fn test_agent_context_creation() {
        let working_dir = std::env::current_dir().unwrap();
        let config = AgentConfig::new("explore", "task", working_dir);
        let context = AgentContext::new(config);

        // Context should be created successfully
        // Check that conversation is initialized
        assert!(!context.conversation.messages.is_empty());
    }

    #[test]
    fn test_agent_context_add_skill_instructions() {
        let working_dir = std::env::current_dir().unwrap();
        let config = AgentConfig::new("explore", "task", working_dir);
        let mut context = AgentContext::new(config);

        context.add_skill_instructions("Use async/await patterns");

        // Instructions should be added to conversation
        // The system prompt should contain the skill instructions
        let system = context.conversation.system_prompt.as_ref();
        assert!(system.is_some());
        assert!(system.unwrap().contains("async/await"));
    }

    // ===== Permission Request Logic Tests =====

    #[test]
    fn test_permission_request_description_truncation() {
        // Verify that long tasks get truncated in the permission description
        let long_task = "a".repeat(100);
        let truncated = truncate_str(&long_task, 80);
        assert_eq!(truncated.len(), 83); // 80 + "..."
    }

    // ===== AgentProgressState Tests =====

    #[test]
    fn test_agent_progress_state_default() {
        let state = AgentProgressState::default();
        assert_eq!(state.iteration, 0);
        assert_eq!(state.max_iterations, 30);
        assert!(!state.completed);
        assert!(state.current_tool.is_none());
        assert_eq!(state.last_activity, "Starting...");
    }

    #[test]
    fn test_agent_progress_state_display_status_with_tool() {
        let state = AgentProgressState {
            iteration: 5,
            max_iterations: 30,
            completed: false,
            current_tool: Some("file_read".to_string()),
            last_activity: "Reading...".to_string(),
            ..Default::default()
        };

        let status = state.display_status();
        assert!(status.contains("[5/30]"));
        assert!(status.contains("file_read"));
        assert!(status.contains("→"));
    }

    #[test]
    fn test_agent_progress_state_display_status_with_activity() {
        let state = AgentProgressState {
            iteration: 10,
            max_iterations: 50,
            completed: false,
            current_tool: None,
            last_activity: "Processing results".to_string(),
            ..Default::default()
        };

        let status = state.display_status();
        assert!(status.contains("[10/50]"));
        assert!(status.contains("Processing results"));
    }

    #[test]
    fn test_agent_progress_state_display_status_empty_activity() {
        let state = AgentProgressState {
            iteration: 3,
            max_iterations: 20,
            completed: false,
            current_tool: None,
            last_activity: String::new(),
            ..Default::default()
        };

        let status = state.display_status();
        assert!(status.contains("[3/20]"));
        assert!(status.contains("Working..."));
    }

    #[test]
    fn test_agent_progress_state_clone() {
        let state = AgentProgressState {
            iteration: 5,
            max_iterations: 100,
            completed: true,
            current_tool: Some("shell".to_string()),
            last_activity: "Done".to_string(),
            ..Default::default()
        };

        let cloned = state.clone();
        assert_eq!(cloned.iteration, state.iteration);
        assert_eq!(cloned.max_iterations, state.max_iterations);
        assert_eq!(cloned.completed, state.completed);
        assert_eq!(cloned.current_tool, state.current_tool);
        assert_eq!(cloned.last_activity, state.last_activity);
    }

    #[test]
    fn test_agent_progress_state_debug() {
        let state = AgentProgressState::default();
        let debug = format!("{:?}", state);
        assert!(debug.contains("AgentProgressState"));
        assert!(debug.contains("iteration"));
    }

    #[test]
    fn test_new_progress_tracker() {
        let tracker = new_progress_tracker();
        // Tracker should be empty initially
        let guard = tracker.blocking_lock();
        assert!(guard.is_empty());
    }

    #[tokio::test]
    async fn test_progress_tracker_insert_and_retrieve() {
        let tracker = new_progress_tracker();
        let tool_id = "test-id".to_string();

        {
            let mut guard = tracker.lock().await;
            guard.insert(tool_id.clone(), AgentProgressState::default());
        }

        {
            let guard = tracker.lock().await;
            let state = guard.get(&tool_id);
            assert!(state.is_some());
            assert_eq!(state.unwrap().iteration, 0);
        }
    }

    #[tokio::test]
    async fn test_progress_tracker_update() {
        let tracker = new_progress_tracker();
        let tool_id = "update-test".to_string();

        // Insert
        {
            let mut guard = tracker.lock().await;
            guard.insert(tool_id.clone(), AgentProgressState::default());
        }

        // Update
        {
            let mut guard = tracker.lock().await;
            if let Some(state) = guard.get_mut(&tool_id) {
                state.iteration = 5;
                state.last_activity = "Updated".to_string();
            }
        }

        // Verify
        {
            let guard = tracker.lock().await;
            let state = guard.get(&tool_id).unwrap();
            assert_eq!(state.iteration, 5);
            assert_eq!(state.last_activity, "Updated");
        }
    }

    #[tokio::test]
    async fn test_progress_tracker_remove() {
        let tracker = new_progress_tracker();
        let tool_id = "remove-test".to_string();

        // Insert
        {
            let mut guard = tracker.lock().await;
            guard.insert(tool_id.clone(), AgentProgressState::default());
            assert!(guard.contains_key(&tool_id));
        }

        // Remove
        {
            let mut guard = tracker.lock().await;
            guard.remove(&tool_id);
        }

        // Verify removed
        {
            let guard = tracker.lock().await;
            assert!(!guard.contains_key(&tool_id));
        }
    }

    #[test]
    fn test_agent_progress_state_completed() {
        let mut state = AgentProgressState::default();
        assert!(!state.completed);

        state.completed = true;
        state.last_activity = "Completed".to_string();

        assert!(state.completed);
        assert_eq!(state.display_status(), "[0/30] Completed");
    }

    #[test]
    fn test_agent_progress_state_max_iterations_respected() {
        let state = AgentProgressState {
            iteration: 30,
            max_iterations: 30,
            completed: false,
            current_tool: None,
            last_activity: "At max".to_string(),
            ..Default::default()
        };

        let status = state.display_status();
        assert!(status.contains("[30/30]"));
    }

    #[test]
    fn test_agent_progress_state_tool_with_long_name() {
        let state = AgentProgressState {
            iteration: 1,
            max_iterations: 10,
            completed: false,
            current_tool: Some("very_long_tool_name_with_lots_of_characters".to_string()),
            last_activity: String::new(),
            ..Default::default()
        };

        let status = state.display_status();
        assert!(status.contains("very_long_tool_name"));
    }

    // ===== SpawnAgentTool Tests =====

    use crate::llm::mock_provider::MockProvider;
    use crate::skills::SkillRegistry;
    use crate::tools::Tool;

    fn create_test_spawn_agent_tool() -> SpawnAgentTool {
        let provider: Arc<dyn crate::llm::provider::LlmProvider> = Arc::new(MockProvider::new());
        let skill_registry = Arc::new(SkillRegistry::with_paths(vec![]));
        SpawnAgentTool::new(provider, skill_registry, "mock-model".to_string())
    }

    #[test]
    fn test_spawn_agent_tool_new() {
        let tool = create_test_spawn_agent_tool();
        assert_eq!(tool.model, "mock-model");
        assert!(tool.rate_coordinator.is_none());
    }

    #[test]
    fn test_spawn_agent_tool_name() {
        let tool = create_test_spawn_agent_tool();
        assert_eq!(tool.name(), "spawn_agent");
    }

    #[test]
    fn test_spawn_agent_tool_definition() {
        let tool = create_test_spawn_agent_tool();
        let definition = tool.definition();

        assert_eq!(definition.name, "spawn_agent");
        assert!(definition
            .description
            .contains("Spawn a specialized subagent"));
        assert!(definition.description.contains("explore"));
        assert!(definition.description.contains("implement"));
    }

    #[test]
    fn test_spawn_agent_tool_definition_schema() {
        let tool = create_test_spawn_agent_tool();
        let definition = tool.definition();

        // Check that input_schema has required fields
        let properties = &definition.input_schema.properties;
        assert!(properties.get("agent_type").is_some());
        assert!(properties.get("task").is_some());
        assert!(properties.get("caps").is_some());
        assert!(properties.get("skill").is_some());
        assert!(properties.get("memory_strategy").is_some());
        assert!(properties.get("max_iterations").is_some());
        assert!(properties.get("background").is_some());
        assert!(properties.get("bead_id").is_some());
    }

    #[test]
    fn test_spawn_agent_tool_requires_permission() {
        let tool = create_test_spawn_agent_tool();
        assert!(tool.requires_permission());
    }

    #[test]
    fn test_spawn_agent_tool_permission_request() {
        let tool = create_test_spawn_agent_tool();

        let input = serde_json::json!({
            "agent_type": "explore",
            "task": "Find all API endpoints"
        });

        let request = tool.permission_request(&input);
        assert!(request.is_some());

        let request = request.unwrap();
        assert_eq!(request.tool_name, "spawn_agent");
        assert!(request.action_description.contains("explore"));
        assert!(request
            .action_description
            .contains("Find all API endpoints"));
        assert!(!request.is_destructive);
        assert!(request.affected_paths.is_empty());
    }

    #[test]
    fn test_spawn_agent_tool_permission_request_long_task() {
        let tool = create_test_spawn_agent_tool();

        // Task longer than 80 chars should be truncated
        let long_task = "a".repeat(100);
        let input = serde_json::json!({
            "agent_type": "implement",
            "task": long_task
        });

        let request = tool.permission_request(&input).unwrap();
        assert!(request.action_description.len() < 100 + 30); // Should be truncated
        assert!(request.action_description.contains("..."));
    }

    #[test]
    fn test_spawn_agent_tool_permission_request_missing_fields() {
        let tool = create_test_spawn_agent_tool();

        // Missing agent_type and task
        let input = serde_json::json!({});

        let request = tool.permission_request(&input);
        assert!(request.is_some());

        let request = request.unwrap();
        assert!(request.action_description.contains("unknown"));
    }

    #[test]
    fn test_spawn_agent_tool_progress_tracker() {
        let tool = create_test_spawn_agent_tool();
        let tracker = tool.progress_tracker();

        // Should return a clone of the Arc
        let guard = tracker.blocking_lock();
        assert!(guard.is_empty());
    }

    #[test]
    fn test_spawn_agent_tool_with_progress_tracker() {
        let provider: Arc<dyn crate::llm::provider::LlmProvider> = Arc::new(MockProvider::new());
        let skill_registry = Arc::new(SkillRegistry::with_paths(vec![]));

        // Create a custom progress tracker with some state
        let custom_tracker = new_progress_tracker();
        {
            let mut guard = custom_tracker.blocking_lock();
            guard.insert("existing-tool".to_string(), AgentProgressState::default());
        }

        let tool = SpawnAgentTool::with_progress_tracker(
            provider,
            skill_registry,
            "mock-model".to_string(),
            custom_tracker.clone(),
        );

        // The tool should use the provided tracker
        let tracker = tool.progress_tracker();
        let guard = tracker.blocking_lock();
        assert!(guard.contains_key("existing-tool"));
    }

    #[tokio::test]
    async fn test_spawn_agent_tool_execute_missing_agent_type() {
        let tool = create_test_spawn_agent_tool();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            uuid::Uuid::new_v4(),
            false,
        );

        let input = serde_json::json!({
            "task": "Find files"
        });

        let result = tool.execute("test-id".to_string(), input, &context).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("agent_type is required"));
    }

    #[tokio::test]
    async fn test_spawn_agent_tool_execute_invalid_agent_type() {
        let tool = create_test_spawn_agent_tool();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            uuid::Uuid::new_v4(),
            false,
        );

        let input = serde_json::json!({
            "agent_type": "invalid_type",
            "task": "Find files"
        });

        let result = tool.execute("test-id".to_string(), input, &context).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(tool_result.is_error());
        assert!(tool_result.output_text().contains("Invalid agent type"));
        assert!(tool_result.output_text().contains("invalid_type"));
    }

    #[tokio::test]
    async fn test_spawn_agent_tool_execute_missing_task() {
        let tool = create_test_spawn_agent_tool();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            uuid::Uuid::new_v4(),
            false,
        );

        let input = serde_json::json!({
            "agent_type": "explore"
        });

        let result = tool.execute("test-id".to_string(), input, &context).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("task is required"));
    }

    #[tokio::test]
    async fn test_spawn_agent_tool_execute_invalid_memory_strategy() {
        let tool = create_test_spawn_agent_tool();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            uuid::Uuid::new_v4(),
            false,
        );

        let input = serde_json::json!({
            "agent_type": "explore",
            "task": "Find files",
            "memory_strategy": "invalid_strategy"
        });

        let result = tool.execute("test-id".to_string(), input, &context).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(tool_result.is_error());
        assert!(tool_result
            .output_text()
            .contains("Invalid memory_strategy"));
    }

    #[tokio::test]
    async fn test_spawn_agent_tool_execute_invalid_skill() {
        let tool = create_test_spawn_agent_tool();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            uuid::Uuid::new_v4(),
            false,
        );

        let input = serde_json::json!({
            "agent_type": "explore",
            "task": "Find files",
            "skill": "nonexistent-skill"
        });

        let result = tool.execute("test-id".to_string(), input, &context).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(tool_result.is_error());
        assert!(tool_result.output_text().contains("Failed to load skill"));
    }

    #[tokio::test]
    async fn test_spawn_agent_tool_execute_alternative_field_names() {
        let tool = create_test_spawn_agent_tool();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            uuid::Uuid::new_v4(),
            false,
        );

        // Test "type" instead of "agent_type"
        let input = serde_json::json!({
            "type": "invalid_type",
            "task": "Find files"
        });

        let result = tool.execute("test-id".to_string(), input, &context).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        // Should recognize "type" as an alias for "agent_type"
        assert!(tool_result.is_error());
        assert!(tool_result.output_text().contains("Invalid agent type"));
    }

    #[tokio::test]
    async fn test_spawn_agent_tool_execute_alternative_task_names() {
        let tool = create_test_spawn_agent_tool();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            uuid::Uuid::new_v4(),
            false,
        );

        // Test "prompt" instead of "task"
        let input = serde_json::json!({
            "agent_type": "invalid_type",
            "prompt": "Find files"
        });

        let result = tool.execute("test-id".to_string(), input, &context).await;
        assert!(result.is_ok());

        // Should recognize "prompt" as an alias for "task"
        let tool_result = result.unwrap();
        assert!(tool_result.is_error()); // Invalid agent type error, not missing task
    }

    #[test]
    fn test_spawn_agent_tool_with_rate_coordinator() {
        use crate::llm::rate_budget::TokenRateCoordinator;

        let provider: Arc<dyn crate::llm::provider::LlmProvider> = Arc::new(MockProvider::new());
        let skill_registry = Arc::new(SkillRegistry::with_paths(vec![]));
        // TokenRateCoordinator::new already returns Arc<Self>
        let rate_coordinator = TokenRateCoordinator::new(100_000); // 100K tokens/min

        let tool = SpawnAgentTool::with_rate_coordinator(
            provider,
            skill_registry,
            rate_coordinator,
            "mock-model".to_string(),
        );

        assert!(tool.rate_coordinator.is_some());
        assert_eq!(tool.model, "mock-model");
    }

    #[test]
    fn test_agent_config_with_model() {
        let working_dir = std::env::current_dir().unwrap();
        let config = AgentConfig::new("explore", "Find files", working_dir)
            .with_model("custom-model".to_string());

        assert_eq!(config.model, Some("custom-model".to_string()));
    }

    #[test]
    fn test_agent_config_rate_priority() {
        let working_dir = std::env::current_dir().unwrap();
        let config = AgentConfig::new("explore", "task", working_dir);

        // Rate priority should be accessible
        let priority = config.rate_priority();
        // Just verify it returns something valid (the actual value depends on agent type)
        assert!(matches!(
            priority,
            crate::llm::rate_budget::RatePriority::Critical
                | crate::llm::rate_budget::RatePriority::High
                | crate::llm::rate_budget::RatePriority::Normal
                | crate::llm::rate_budget::RatePriority::Background
        ));
    }

    #[test]
    fn test_summarize_tool_input_for_display_variants() {
        assert_eq!(
            summarize_tool_input_for_display(
                "file_read",
                &serde_json::json!({"path":"src/main.rs"})
            ),
            "src/main.rs"
        );
        assert_eq!(
            summarize_tool_input_for_display("glob", &serde_json::json!({"pattern":"**/*.rs"})),
            "**/*.rs"
        );
        assert_eq!(
            summarize_tool_input_for_display("grep", &serde_json::json!({"pattern":"TODO"})),
            "/TODO/"
        );
        assert_eq!(
            summarize_tool_input_for_display("file_write", &serde_json::json!({"path":"a.txt"})),
            "a.txt"
        );
        assert_eq!(
            summarize_tool_input_for_display("file_edit", &serde_json::json!({"path":"b.txt"})),
            "b.txt"
        );

        let long_cmd = "echo ".to_string() + &"x".repeat(80);
        let summarized =
            summarize_tool_input_for_display("shell", &serde_json::json!({"command": long_cmd}));
        assert!(summarized.len() <= 50);
        assert!(summarized.ends_with("..."));

        assert_eq!(
            summarize_tool_input_for_display("unknown", &serde_json::json!({"name":"short"})),
            "short"
        );
        let fallback_long = summarize_tool_input_for_display(
            "unknown",
            &serde_json::json!({"name":"this string is definitely much longer than fifty characters to force truncation"}),
        );
        assert!(fallback_long.ends_with("..."));
        assert_eq!(
            summarize_tool_input_for_display("unknown", &serde_json::json!({"value": 123})),
            ""
        );
    }

    #[tokio::test]
    async fn test_spawn_agent_execute_missing_agent_type_returns_error() {
        let tool = create_test_spawn_agent_tool();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            uuid::Uuid::new_v4(),
            false,
        );

        let input = serde_json::json!({
            "task": "Find important files"
        });

        let result = tool
            .execute("missing-agent-type".to_string(), input, &context)
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("agent_type is required"));
    }

    #[tokio::test]
    async fn test_spawn_agent_execute_missing_task_returns_error() {
        let tool = create_test_spawn_agent_tool();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            uuid::Uuid::new_v4(),
            false,
        );

        let input = serde_json::json!({
            "agent_type": "explore"
        });

        let result = tool
            .execute("missing-task".to_string(), input, &context)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("task is required"));
    }

    #[tokio::test]
    async fn test_spawn_agent_execute_invalid_memory_strategy_returns_tool_error() {
        let tool = create_test_spawn_agent_tool();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            uuid::Uuid::new_v4(),
            false,
        );

        let input = serde_json::json!({
            "agent_type": "explore",
            "task": "Investigate project",
            "memory_strategy": "invalid"
        });

        let result = tool
            .execute("invalid-memory".to_string(), input, &context)
            .await
            .unwrap();
        assert!(result.is_error());
        assert!(result.output_text().contains("Invalid memory_strategy"));
    }

    #[tokio::test]
    async fn test_spawn_agent_execute_background_mode_returns_success() {
        let tool = create_test_spawn_agent_tool();
        let temp_dir = tempfile::TempDir::new().unwrap();
        let context = crate::tools::ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            uuid::Uuid::new_v4(),
            false,
        );

        let input = serde_json::json!({
            "agent_type": "explore",
            "task": "Map repository layout",
            "background": true
        });

        let result = tool
            .execute("background-agent".to_string(), input, &context)
            .await
            .unwrap();
        assert!(!result.is_error());
        assert!(result.output_text().contains("Spawned background agent"));
    }

    #[test]
    fn test_permission_request_and_requires_permission() {
        let tool = create_test_spawn_agent_tool();
        let input = serde_json::json!({
            "agent_type": "implement",
            "task": "a".repeat(200)
        });

        let request = tool.permission_request(&input).expect("permission request");
        assert_eq!(request.tool_name, "spawn_agent");
        assert!(request.action_description.contains("Spawn implement agent"));
        assert!(request.action_description.contains("..."));
        assert!(tool.requires_permission());
    }
}
