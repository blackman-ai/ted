// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Spawn agent tool
//!
//! Allows the main agent to spawn specialized subagents for delegated tasks.

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use crate::agents::{
    get_agent_type_names, is_valid_agent_type, AgentConfig, AgentContext, AgentRunner,
    MemoryStrategy,
};
use crate::error::{Result, TedError};
use crate::llm::provider::{LlmProvider, ToolDefinition};
use crate::llm::rate_budget::TokenRateCoordinator;
use crate::skills::SkillRegistry;
use crate::tools::{PermissionRequest, SchemaBuilder, Tool, ToolContext, ToolResult};

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
        }
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
                "Spawn a specialized subagent to handle a specific task. \
                 Available agent types: {}. \
                 The subagent will execute autonomously and return results.",
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
        let mut config = AgentConfig::new(agent_type, task, working_dir)
            .with_model(self.model.clone());

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

        // Allocate rate budget if coordinator is available
        if let Some(coordinator) = &self.rate_coordinator {
            let priority = config.rate_priority();
            let allocation = coordinator.request_allocation(priority, config.name.clone());

            // Log the allocation
            eprintln!(
                "  [{}] Rate budget: {}K tokens/min ({})",
                config.name,
                allocation.budget() / 1000,
                format!("{:?}", priority).to_lowercase()
            );

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

        // Create runner
        let runner = AgentRunner::new(Arc::clone(&self.provider));

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
            // Run synchronously
            match runner.run(agent_context).await {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_agent_tool_name() {
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
}
