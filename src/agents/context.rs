// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Agent context management
//!
//! This module provides isolated conversation contexts for subagents,
//! integrating with the existing ContextManager for WAL-based storage.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::context::ContextManager;
use crate::error::Result;
use crate::llm::message::{Conversation, Message};
use crate::llm::rate_budget::RateBudgetAllocation;

use super::builtin::{get_agent_type, AgentTypeDefinition};
use super::types::{AgentConfig, ToolPermissions};

/// Context for a subagent's execution
///
/// Provides an isolated conversation space with filtered tool access
/// and optional integration with the parent's context manager.
pub struct AgentContext {
    /// The agent's configuration
    pub config: AgentConfig,

    /// The agent type definition (if using a built-in type)
    pub agent_type_def: Option<&'static AgentTypeDefinition>,

    /// The isolated conversation for this agent
    pub conversation: Conversation,

    /// Tool permissions for this agent
    pub tool_permissions: ToolPermissions,

    /// Optional parent context manager for storing chunks
    parent_context: Option<Arc<RwLock<ContextManager>>>,

    /// Root chunk ID in parent context (for linking agent work)
    root_chunk_id: Option<Uuid>,

    /// Files read during this agent's execution
    files_read: Vec<PathBuf>,

    /// Files modified during this agent's execution
    files_changed: Vec<PathBuf>,

    /// Total tokens used
    tokens_used: u32,

    /// Current iteration count
    iterations: u32,

    /// Rate budget allocation for this agent (if rate limiting is enabled)
    rate_allocation: Option<Arc<RateBudgetAllocation>>,
}

impl AgentContext {
    /// Create a new agent context
    pub fn new(config: AgentConfig) -> Self {
        let agent_type_def = get_agent_type(&config.agent_type);

        // Build tool permissions from agent type + additional caps
        let tool_permissions = agent_type_def
            .map(|def| def.tool_permissions.clone())
            .unwrap_or_default();

        // Additional caps could expand permissions (handled by caller)

        // Build the system prompt
        let system_prompt = Self::build_system_prompt(&config, agent_type_def);
        let mut conversation = Conversation::with_system(&system_prompt);

        // Add the initial task as a user message
        conversation.push(Message::user(&config.task));

        Self {
            config,
            agent_type_def,
            conversation,
            tool_permissions,
            parent_context: None,
            root_chunk_id: None,
            files_read: Vec::new(),
            files_changed: Vec::new(),
            tokens_used: 0,
            iterations: 0,
            rate_allocation: None,
        }
    }

    /// Create an agent context with a parent context manager for WAL storage
    pub async fn with_parent_context(
        config: AgentConfig,
        parent_context: Arc<RwLock<ContextManager>>,
    ) -> Result<Self> {
        let mut ctx = Self::new(config.clone());

        // Store the subagent start marker in the parent's context
        let context_mgr = parent_context.read().await;
        let root_chunk_id = context_mgr
            .store_message(
                "system",
                &format!(
                    "[Subagent '{}' started: type={}, task={}]",
                    config.name, config.agent_type, config.task
                ),
                None,
            )
            .await?;
        drop(context_mgr);

        ctx.parent_context = Some(parent_context);
        ctx.root_chunk_id = Some(root_chunk_id);

        Ok(ctx)
    }

    /// Build the system prompt for this agent
    fn build_system_prompt(
        config: &AgentConfig,
        agent_type_def: Option<&AgentTypeDefinition>,
    ) -> String {
        let mut prompt = String::new();

        // Base identity
        prompt.push_str(&format!(
            "You are a specialized {} agent named '{}'.\n\n",
            config.agent_type, config.name
        ));

        // Working directory context
        prompt.push_str(&format!(
            "Working directory: {}\n\n",
            config.working_dir.display()
        ));

        // Agent type-specific instructions
        if let Some(def) = agent_type_def {
            prompt.push_str(def.system_prompt_additions);
            prompt.push('\n');
        }

        // Constraints
        prompt.push_str(&format!(
            "\n## Constraints\n\
             - Maximum iterations: {}\n\
             - Token budget: {}\n",
            config.max_iterations, config.token_budget
        ));

        // Skill-specific instructions would be added here by the runner
        // when skills are loaded

        prompt
    }

    /// Extend the system prompt with skill instructions
    pub fn add_skill_instructions(&mut self, skill_content: &str) {
        if let Some(ref mut system) = self.conversation.system_prompt {
            system.push_str("\n\n## Skill Instructions\n");
            system.push_str(skill_content);
        } else {
            self.conversation.system_prompt =
                Some(format!("## Skill Instructions\n{}", skill_content));
        }
    }

    /// Extend tool permissions with additional allowed tools
    pub fn extend_permissions(&mut self, additional: &ToolPermissions) {
        self.tool_permissions.merge(additional);
    }

    /// Check if a tool is allowed for this agent
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        self.tool_permissions.is_allowed(tool_name)
    }

    /// Add a message to the conversation
    pub async fn add_message(&mut self, message: Message) -> Result<()> {
        // Update token count
        self.tokens_used += message.estimate_tokens();

        // Store in parent context if available
        if let (Some(ref parent), Some(root_id)) = (&self.parent_context, self.root_chunk_id) {
            let role = message.role.to_string();
            let content = message.text().unwrap_or_default();
            let context_mgr = parent.read().await;
            context_mgr
                .store_message(&role, content, Some(root_id))
                .await?;
        }

        self.conversation.push(message);
        Ok(())
    }

    /// Record a file read
    pub fn record_file_read(&mut self, path: PathBuf) {
        if !self.files_read.contains(&path) {
            self.files_read.push(path);
        }
    }

    /// Record a file modification
    pub fn record_file_changed(&mut self, path: PathBuf) {
        if !self.files_changed.contains(&path) {
            self.files_changed.push(path);
        }
    }

    /// Increment iteration count
    pub fn increment_iteration(&mut self) {
        self.iterations += 1;
    }

    /// Check if the agent has exceeded its iteration limit
    pub fn exceeded_iterations(&self) -> bool {
        self.iterations >= self.config.max_iterations
    }

    /// Check if the agent has exceeded its token budget
    pub fn exceeded_token_budget(&self) -> bool {
        self.tokens_used >= self.config.token_budget
    }

    /// Get current iteration count
    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    /// Get current token usage
    pub fn tokens_used(&self) -> u32 {
        self.tokens_used
    }

    /// Get files read
    pub fn files_read(&self) -> &[PathBuf] {
        &self.files_read
    }

    /// Get files changed
    pub fn files_changed(&self) -> &[PathBuf] {
        &self.files_changed
    }

    /// Set the rate budget allocation for this agent
    pub fn set_rate_allocation(&mut self, allocation: Arc<RateBudgetAllocation>) {
        self.rate_allocation = Some(allocation);
    }

    /// Get the rate budget allocation (if any)
    pub fn rate_allocation(&self) -> Option<&Arc<RateBudgetAllocation>> {
        self.rate_allocation.as_ref()
    }

    /// Get a reference to the conversation
    pub fn conversation(&self) -> &Conversation {
        &self.conversation
    }

    /// Get a mutable reference to the conversation
    pub fn conversation_mut(&mut self) -> &mut Conversation {
        &mut self.conversation
    }

    /// Mark the agent as completed in the parent context
    pub async fn finalize(&self, success: bool, summary: &str) -> Result<()> {
        if let (Some(ref parent), Some(root_id)) = (&self.parent_context, self.root_chunk_id) {
            let context_mgr = parent.read().await;
            context_mgr
                .store_message(
                    "system",
                    &format!(
                        "[Subagent '{}' {}: {}]",
                        self.config.name,
                        if success { "completed" } else { "failed" },
                        summary
                    ),
                    Some(root_id),
                )
                .await?;
        }
        Ok(())
    }
}

/// Builder for creating agent contexts with various options
pub struct AgentContextBuilder {
    config: AgentConfig,
    parent_context: Option<Arc<RwLock<ContextManager>>>,
    skill_content: Option<String>,
    additional_permissions: Option<ToolPermissions>,
    rate_allocation: Option<Arc<RateBudgetAllocation>>,
}

impl AgentContextBuilder {
    /// Create a new builder with the given config
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            parent_context: None,
            skill_content: None,
            additional_permissions: None,
            rate_allocation: None,
        }
    }

    /// Set the parent context manager
    pub fn with_parent_context(mut self, context: Arc<RwLock<ContextManager>>) -> Self {
        self.parent_context = Some(context);
        self
    }

    /// Add skill instructions
    pub fn with_skill(mut self, content: String) -> Self {
        self.skill_content = Some(content);
        self
    }

    /// Add additional tool permissions
    pub fn with_additional_permissions(mut self, permissions: ToolPermissions) -> Self {
        self.additional_permissions = Some(permissions);
        self
    }

    /// Set rate budget allocation
    pub fn with_rate_allocation(mut self, allocation: Arc<RateBudgetAllocation>) -> Self {
        self.rate_allocation = Some(allocation);
        self
    }

    /// Build the agent context
    pub async fn build(self) -> Result<AgentContext> {
        let mut ctx = if let Some(parent) = self.parent_context {
            AgentContext::with_parent_context(self.config, parent).await?
        } else {
            AgentContext::new(self.config)
        };

        if let Some(skill) = self.skill_content {
            ctx.add_skill_instructions(&skill);
        }

        if let Some(permissions) = self.additional_permissions {
            ctx.extend_permissions(&permissions);
        }

        if let Some(allocation) = self.rate_allocation {
            ctx.set_rate_allocation(allocation);
        }

        Ok(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_context_new() {
        let config = AgentConfig::new("explore", "Find auth files", PathBuf::from("/project"));
        let ctx = AgentContext::new(config);

        assert_eq!(ctx.config.agent_type, "explore");
        assert!(!ctx.conversation.is_empty()); // Has the task message
        assert_eq!(ctx.iterations(), 0);
        assert_eq!(ctx.tokens_used(), 0);
    }

    #[test]
    fn test_agent_context_tool_permissions() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let ctx = AgentContext::new(config);

        // Explore agent should only allow read operations
        assert!(ctx.is_tool_allowed("file_read"));
        assert!(ctx.is_tool_allowed("glob"));
        assert!(!ctx.is_tool_allowed("file_write"));
        assert!(!ctx.is_tool_allowed("shell"));
    }

    #[test]
    fn test_agent_context_implement_permissions() {
        let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/project"));
        let ctx = AgentContext::new(config);

        // Implement agent should have write and shell access
        assert!(ctx.is_tool_allowed("file_read"));
        assert!(ctx.is_tool_allowed("file_write"));
        assert!(ctx.is_tool_allowed("shell"));
    }

    #[test]
    fn test_agent_context_iteration_tracking() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"))
            .with_max_iterations(5);
        let mut ctx = AgentContext::new(config);

        for _ in 0..4 {
            ctx.increment_iteration();
            assert!(!ctx.exceeded_iterations());
        }

        ctx.increment_iteration();
        assert!(ctx.exceeded_iterations());
    }

    #[test]
    fn test_agent_context_file_tracking() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let mut ctx = AgentContext::new(config);

        ctx.record_file_read(PathBuf::from("/project/src/main.rs"));
        ctx.record_file_read(PathBuf::from("/project/src/lib.rs"));
        ctx.record_file_read(PathBuf::from("/project/src/main.rs")); // Duplicate

        assert_eq!(ctx.files_read().len(), 2);
    }

    #[test]
    fn test_agent_context_add_skill() {
        let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/project"));
        let mut ctx = AgentContext::new(config);

        ctx.add_skill_instructions("Use async/await patterns.");

        let system = ctx.conversation.system_prompt.as_ref().unwrap();
        assert!(system.contains("Skill Instructions"));
        assert!(system.contains("async/await"));
    }

    #[test]
    fn test_agent_context_extend_permissions() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let mut ctx = AgentContext::new(config);

        // Initially can't use database
        assert!(!ctx.is_tool_allowed("database_query"));

        // Extend with additional permissions
        ctx.extend_permissions(&ToolPermissions::allow(&["database_query"]));

        assert!(ctx.is_tool_allowed("database_query"));
    }

    #[tokio::test]
    async fn test_agent_context_builder() {
        let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/project"));

        let ctx = AgentContextBuilder::new(config)
            .with_skill("Use dependency injection.".to_string())
            .with_additional_permissions(ToolPermissions::allow(&["custom_tool"]))
            .build()
            .await
            .unwrap();

        assert!(ctx.is_tool_allowed("custom_tool"));
        let system = ctx.conversation.system_prompt.as_ref().unwrap();
        assert!(system.contains("dependency injection"));
    }

    // ===== Additional Agent Type Permission Tests =====

    #[test]
    fn test_agent_context_unknown_type_permissions() {
        // Unknown agent types get default permissions (everything allowed when allowed set is empty)
        let config = AgentConfig::new("unknown_type", "Some task", PathBuf::from("/project"));
        let ctx = AgentContext::new(config);

        // Unknown agent type has no type definition
        assert!(ctx.agent_type_def.is_none());

        // With default permissions (empty allowed/denied), everything is permitted
        assert!(ctx.is_tool_allowed("file_read"));
        assert!(ctx.is_tool_allowed("file_write"));
        assert!(ctx.is_tool_allowed("shell"));
        assert!(ctx.is_tool_allowed("any_custom_tool"));
    }

    #[test]
    fn test_agent_context_explore_has_agent_type_def() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let ctx = AgentContext::new(config);

        // Explore agent should have a type definition
        assert!(ctx.agent_type_def.is_some());
    }

    #[test]
    fn test_agent_context_implement_has_agent_type_def() {
        let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/project"));
        let ctx = AgentContext::new(config);

        // Implement agent should have a type definition
        assert!(ctx.agent_type_def.is_some());
    }

    // ===== ToolPermissions Tests =====

    #[test]
    fn test_tool_permissions_default() {
        let perms = ToolPermissions::default();
        assert!(perms.allowed.is_empty());
        assert!(perms.denied.is_empty());
    }

    #[test]
    fn test_tool_permissions_allow() {
        let perms = ToolPermissions::allow(&["tool1", "tool2"]);
        assert!(perms.allowed.contains("tool1"));
        assert!(perms.allowed.contains("tool2"));
        assert!(!perms.allowed.contains("tool3"));
    }

    #[test]
    fn test_tool_permissions_deny() {
        let perms = ToolPermissions::deny(&["dangerous_tool"]);
        assert!(perms.denied.contains("dangerous_tool"));
    }

    #[test]
    fn test_tool_permissions_clone() {
        let perms = ToolPermissions::allow(&["tool1"]);
        let cloned = perms.clone();
        assert!(cloned.allowed.contains("tool1"));
    }

    #[test]
    fn test_tool_permissions_debug() {
        let perms = ToolPermissions::allow(&["tool1"]);
        let debug_str = format!("{:?}", perms);
        assert!(debug_str.contains("tool1"));
    }

    // ===== Token Tracking Tests =====

    #[test]
    fn test_agent_context_tokens_used_initial() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let ctx = AgentContext::new(config);

        assert_eq!(ctx.tokens_used(), 0);
    }

    // ===== Conversation Tests =====

    #[test]
    fn test_agent_context_conversation_not_empty() {
        let config = AgentConfig::new("explore", "Find auth files", PathBuf::from("/project"));
        let ctx = AgentContext::new(config);

        // Conversation should have at least the task message
        assert!(!ctx.conversation.is_empty());
    }

    #[test]
    fn test_agent_context_conversation_has_system_prompt() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let ctx = AgentContext::new(config);

        assert!(ctx.conversation.system_prompt.is_some());
    }

    // ===== Config Access Tests =====

    #[test]
    fn test_agent_context_config_access() {
        let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/my/project"))
            .with_caps(vec!["rust".to_string()])
            .with_skill("expertise".to_string());
        let ctx = AgentContext::new(config);

        assert_eq!(ctx.config.agent_type, "implement");
        assert_eq!(ctx.config.task, "Add feature");
        assert_eq!(ctx.config.working_dir, PathBuf::from("/my/project"));
        assert_eq!(ctx.config.caps, vec!["rust".to_string()]);
        assert_eq!(ctx.config.skill, Some("expertise".to_string()));
    }

    // ===== Iteration Edge Cases =====

    #[test]
    fn test_agent_context_max_iterations_zero() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"))
            .with_max_iterations(0);
        let ctx = AgentContext::new(config);

        // With max_iterations = 0, it should immediately exceed
        assert!(ctx.exceeded_iterations());
    }

    #[test]
    fn test_agent_context_max_iterations_one() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"))
            .with_max_iterations(1);
        let mut ctx = AgentContext::new(config);

        assert!(!ctx.exceeded_iterations());
        ctx.increment_iteration();
        assert!(ctx.exceeded_iterations());
    }

    // ===== Multiple Skill Instructions =====

    #[test]
    fn test_agent_context_multiple_skill_instructions() {
        let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/project"));
        let mut ctx = AgentContext::new(config);

        ctx.add_skill_instructions("First skill instruction.");
        ctx.add_skill_instructions("Second skill instruction.");

        let system = ctx.conversation.system_prompt.as_ref().unwrap();
        assert!(system.contains("First skill"));
        assert!(system.contains("Second skill"));
    }

    // ===== Permission Extension Edge Cases =====

    #[test]
    fn test_agent_context_extend_permissions_with_deny() {
        let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/project"));
        let mut ctx = AgentContext::new(config);

        // Implement should have shell access
        assert!(ctx.is_tool_allowed("shell"));

        // Deny shell access
        ctx.extend_permissions(&ToolPermissions::deny(&["shell"]));

        // Should now be denied
        assert!(!ctx.is_tool_allowed("shell"));
    }

    #[test]
    fn test_agent_context_extend_permissions_multiple_times() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let mut ctx = AgentContext::new(config);

        ctx.extend_permissions(&ToolPermissions::allow(&["tool1"]));
        ctx.extend_permissions(&ToolPermissions::allow(&["tool2"]));
        ctx.extend_permissions(&ToolPermissions::allow(&["tool3"]));

        assert!(ctx.is_tool_allowed("tool1"));
        assert!(ctx.is_tool_allowed("tool2"));
        assert!(ctx.is_tool_allowed("tool3"));
    }

    // ===== AgentContextBuilder Tests =====

    #[tokio::test]
    async fn test_agent_context_builder_minimal() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let ctx = AgentContextBuilder::new(config).build().await.unwrap();

        assert_eq!(ctx.config.agent_type, "explore");
        assert!(!ctx.conversation.is_empty());
    }

    #[tokio::test]
    async fn test_agent_context_builder_with_skill_only() {
        let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/project"));
        let ctx = AgentContextBuilder::new(config)
            .with_skill("Use clean architecture.".to_string())
            .build()
            .await
            .unwrap();

        let system = ctx.conversation.system_prompt.as_ref().unwrap();
        assert!(system.contains("clean architecture"));
    }

    #[tokio::test]
    async fn test_agent_context_builder_with_permissions_only() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let ctx = AgentContextBuilder::new(config)
            .with_additional_permissions(ToolPermissions::allow(&["my_custom_tool"]))
            .build()
            .await
            .unwrap();

        assert!(ctx.is_tool_allowed("my_custom_tool"));
    }

    // ===== File Path Handling Tests =====

    #[test]
    fn test_agent_context_file_tracking_various_paths() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let mut ctx = AgentContext::new(config);

        ctx.record_file_read(PathBuf::from("/absolute/path.rs"));
        ctx.record_file_read(PathBuf::from("relative/path.rs"));
        ctx.record_file_read(PathBuf::from("./dot/path.rs"));

        assert_eq!(ctx.files_read().len(), 3);
    }

    // ===== Additional Coverage Tests =====

    #[test]
    fn test_agent_context_files_changed() {
        let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/project"));
        let mut ctx = AgentContext::new(config);

        assert!(ctx.files_changed().is_empty());

        ctx.record_file_changed(PathBuf::from("/project/src/main.rs"));
        ctx.record_file_changed(PathBuf::from("/project/src/lib.rs"));

        assert_eq!(ctx.files_changed().len(), 2);
    }

    #[test]
    fn test_agent_context_files_changed_duplicates() {
        let config = AgentConfig::new("implement", "Add feature", PathBuf::from("/project"));
        let mut ctx = AgentContext::new(config);

        ctx.record_file_changed(PathBuf::from("/project/src/main.rs"));
        ctx.record_file_changed(PathBuf::from("/project/src/main.rs")); // Duplicate

        assert_eq!(ctx.files_changed().len(), 1);
    }

    #[test]
    fn test_agent_context_conversation_getter() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let ctx = AgentContext::new(config);

        let conversation = ctx.conversation();
        assert!(!conversation.is_empty());
        assert!(conversation.system_prompt.is_some());
    }

    #[test]
    fn test_agent_context_conversation_mut() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let mut ctx = AgentContext::new(config);

        let initial_len = ctx.conversation().messages.len();

        // Modify conversation via mut reference
        ctx.conversation_mut()
            .push(crate::llm::message::Message::assistant("Test message"));

        assert_eq!(ctx.conversation().messages.len(), initial_len + 1);
    }

    #[test]
    fn test_agent_context_exceeded_token_budget_false() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"))
            .with_token_budget(10000);
        let ctx = AgentContext::new(config);

        // No tokens used yet
        assert!(!ctx.exceeded_token_budget());
    }

    #[test]
    fn test_agent_context_exceeded_token_budget_true() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"))
            .with_token_budget(0);
        let ctx = AgentContext::new(config);

        // With budget of 0, should immediately exceed
        assert!(ctx.exceeded_token_budget());
    }

    #[test]
    fn test_agent_context_add_skill_instructions_no_system_prompt() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"));
        let mut ctx = AgentContext::new(config);

        // Clear system prompt to test the else branch
        ctx.conversation.system_prompt = None;

        ctx.add_skill_instructions("New skill instruction");

        // Should create a new system prompt
        assert!(ctx.conversation.system_prompt.is_some());
        let system = ctx.conversation.system_prompt.as_ref().unwrap();
        assert!(system.contains("Skill Instructions"));
        assert!(system.contains("New skill instruction"));
    }
}
