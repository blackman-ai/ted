// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Subagent orchestration system
//!
//! This module provides multi-agent support for Ted, enabling the main agent
//! to spawn specialized subagents with isolated contexts, filtered tool access,
//! and configurable memory strategies.
//!
//! ## Agent Types
//!
//! Built-in agent types:
//! - `explore` - Read-only codebase discovery and search
//! - `plan` - Architecture design and implementation planning
//! - `implement` - Code writing and modification
//! - `bash` - Shell command execution
//! - `review` - Code review and analysis
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use ted::agents::{AgentConfig, AgentContext, AgentRunner};
//! use std::path::PathBuf;
//! use std::sync::Arc;
//!
//! // Create agent configuration
//! let config = AgentConfig::new("explore", "Find all auth files", PathBuf::from("/project"))
//!     .with_max_iterations(30);
//!
//! // Create agent context
//! let context = AgentContext::new(config);
//!
//! // Run the agent
//! let runner = AgentRunner::new(Arc::new(provider));
//! let result = runner.run(context).await?;
//!
//! println!("Agent completed: {}", result.summary);
//! ```
//!
//! ## Memory Strategies
//!
//! Subagents can use different memory management strategies:
//! - `Full` - Keep all messages, trim only when necessary
//! - `Summarizing` - LLM-summarize older messages when threshold exceeded
//! - `Windowed` - Keep a fixed sliding window of recent messages
//!
//! ## Skills Integration
//!
//! Subagents can load skills for domain-specific expertise:
//!
//! ```rust,ignore
//! let config = AgentConfig::new("implement", "Add async feature", working_dir)
//!     .with_skill("rust-async".to_string());
//! ```

pub mod builtin;
pub mod context;
pub mod memory;
pub mod runner;
pub mod types;

// Re-export commonly used types
pub use builtin::{get_agent_type, get_agent_type_names, is_valid_agent_type, AgentTypeDefinition};
pub use context::{AgentContext, AgentContextBuilder};
pub use memory::{apply_memory_strategy, MemoryAction};
pub use runner::{spawn_background_agent, AgentRunner, BackgroundAgentHandle, RunnerConfig};
pub use types::{
    AgentConfig, AgentHandle, AgentResult, AgentStatus, MemoryStrategy, ToolPermissions,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_module_exports() {
        // Verify all expected types are exported
        let _ = AgentConfig::new("explore", "test", PathBuf::from("/tmp"));
        let _ = MemoryStrategy::Full;
        let _ = AgentStatus::Pending;

        assert!(is_valid_agent_type("explore"));
        assert!(!get_agent_type_names().is_empty());
    }

    #[test]
    fn test_explore_agent_type() {
        let agent_type = get_agent_type("explore").unwrap();
        assert_eq!(agent_type.name, "explore");
        assert!(!agent_type.can_write);
        assert!(!agent_type.can_execute);
    }

    #[test]
    fn test_implement_agent_type() {
        let agent_type = get_agent_type("implement").unwrap();
        assert_eq!(agent_type.name, "implement");
        assert!(agent_type.can_write);
        assert!(agent_type.can_execute);
    }

    #[test]
    fn test_agent_config_with_options() {
        let config = AgentConfig::new("explore", "Find files", PathBuf::from("/project"))
            .with_caps(vec!["testing".to_string()])
            .with_skill("rust".to_string())
            .with_max_iterations(50)
            .with_memory_strategy(MemoryStrategy::Windowed { window_size: 20 });

        assert_eq!(config.agent_type, "explore");
        assert_eq!(config.caps, vec!["testing"]);
        assert_eq!(config.skill, Some("rust".to_string()));
        assert_eq!(config.max_iterations, 50);
    }

    #[test]
    fn test_tool_permissions() {
        let perms = ToolPermissions::allow(&["file_read", "glob"]);
        assert!(perms.is_allowed("file_read"));
        assert!(perms.is_allowed("glob"));
        assert!(!perms.is_allowed("shell"));
    }

    #[test]
    fn test_agent_context_permissions() {
        let config = AgentConfig::new("explore", "Search", PathBuf::from("/project"));
        let ctx = AgentContext::new(config);

        // Explore agent should have read-only access
        assert!(ctx.is_tool_allowed("file_read"));
        assert!(ctx.is_tool_allowed("glob"));
        assert!(!ctx.is_tool_allowed("file_write"));
        assert!(!ctx.is_tool_allowed("shell"));
    }
}
