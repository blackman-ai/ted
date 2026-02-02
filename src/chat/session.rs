// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Chat session management
//!
//! Encapsulates all state needed for an interactive chat session.

use std::sync::Arc;

use crate::caps::resolver::MergedCap;
use crate::caps::{CapLoader, CapResolver};
use crate::config::Settings;
use crate::context::{ContextManager, SessionId};
use crate::error::Result;
use crate::history::{HistoryStore, SessionInfo};
use crate::llm::message::Conversation;
use crate::llm::provider::LlmProvider;
use crate::skills::SkillRegistry;
use crate::tools::{ToolContext, ToolExecutor};

/// Encapsulates all state for an interactive chat session
pub struct ChatSession {
    /// Unique session identifier
    pub session_id: SessionId,

    /// Session metadata and history
    pub session_info: SessionInfo,

    /// Conversation message history
    pub conversation: Conversation,

    /// Context manager for file/project awareness
    pub context_manager: ContextManager,

    /// Tool executor for running tools
    pub tool_executor: ToolExecutor,

    /// LLM provider for completions
    pub provider: Arc<dyn LlmProvider>,

    /// Provider name (e.g., "anthropic", "ollama")
    pub provider_name: String,

    /// Current model name
    pub model: String,

    /// Active cap names
    pub cap_names: Vec<String>,

    /// Merged capability configuration
    pub merged_cap: MergedCap,

    /// History store for persistence
    pub history_store: HistoryStore,

    /// Skill registry for agent spawning
    pub skill_registry: Arc<SkillRegistry>,

    /// Cap loader for reloading caps
    pub cap_loader: CapLoader,

    /// Cap resolver for merging caps
    pub cap_resolver: CapResolver,

    /// Whether this session was resumed from history
    pub is_resumed: bool,

    /// Message count in this session
    pub message_count: usize,

    /// Trust mode flag
    pub trust_mode: bool,
}

/// Builder for creating ChatSession instances
pub struct ChatSessionBuilder {
    settings: Settings,
    provider: Option<Arc<dyn LlmProvider>>,
    provider_name: String,
    model: Option<String>,
    cap_names: Vec<String>,
    trust_mode: bool,
    working_directory: std::path::PathBuf,
    resume_id: Option<String>,
    verbose: u8,
}

impl ChatSessionBuilder {
    /// Create a new builder with settings
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            provider: None,
            provider_name: String::new(),
            model: None,
            cap_names: vec!["base".to_string()],
            trust_mode: false,
            working_directory: std::env::current_dir().unwrap_or_default(),
            resume_id: None,
            verbose: 0,
        }
    }

    /// Set the LLM provider
    pub fn with_provider(mut self, provider: Arc<dyn LlmProvider>, name: &str) -> Self {
        self.provider = Some(provider);
        self.provider_name = name.to_string();
        self
    }

    /// Set the model name
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the cap names to load
    pub fn with_caps(mut self, caps: Vec<String>) -> Self {
        self.cap_names = caps;
        self
    }

    /// Enable trust mode
    pub fn with_trust(mut self, trust: bool) -> Self {
        self.trust_mode = trust;
        self
    }

    /// Set the working directory
    pub fn with_working_directory(mut self, dir: std::path::PathBuf) -> Self {
        self.working_directory = dir;
        self
    }

    /// Set a session ID to resume
    pub fn resume_session(mut self, session_id: impl Into<String>) -> Self {
        self.resume_id = Some(session_id.into());
        self
    }

    /// Set verbosity level
    pub fn with_verbose(mut self, level: u8) -> Self {
        self.verbose = level;
        self
    }

    /// Build the ChatSession
    pub async fn build(self) -> Result<ChatSession> {
        let provider = self
            .provider
            .ok_or_else(|| crate::error::TedError::Config("No LLM provider set".into()))?;

        // Initialize cap loader and resolver
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader.clone());
        let merged_cap = resolver.resolve_and_merge(&self.cap_names)?;

        // Determine model
        let model = self.model.unwrap_or_else(|| {
            merged_cap
                .preferred_model()
                .map(|s| s.to_string())
                .unwrap_or_else(|| match self.provider_name.as_str() {
                    "ollama" => self.settings.providers.ollama.default_model.clone(),
                    "openrouter" => self.settings.providers.openrouter.default_model.clone(),
                    _ => self.settings.providers.anthropic.default_model.clone(),
                })
        });

        // Create conversation with system prompt from caps
        let mut conversation = Conversation::new();
        if !merged_cap.system_prompt.is_empty() {
            conversation.set_system(&merged_cap.system_prompt);
        }

        // Initialize history tracking
        let history_store = HistoryStore::open()?;

        // Create or resume session
        let (session_id, session_info, message_count, is_resumed) = if let Some(ref resume_id) =
            self.resume_id
        {
            Self::resume_session_from_history(&history_store, resume_id, &self.working_directory)?
        } else {
            let sid = SessionId::new();
            let info = SessionInfo::new(sid.0, self.working_directory.clone());
            (sid, info, 0, false)
        };

        // Initialize context manager
        let context_storage_path = Settings::context_path();
        let mut context_manager =
            ContextManager::new(context_storage_path, session_id.clone()).await?;

        // Set project root if found
        let project_root = crate::utils::find_project_root();
        if let Some(ref root) = project_root {
            context_manager.set_project_root(root.clone(), true).await?;
        }

        // Append file tree to system prompt
        if let Some(file_tree_context) = context_manager.file_tree_context().await {
            let current_system = conversation.system_prompt.clone().unwrap_or_default();
            let enhanced_system = if current_system.is_empty() {
                file_tree_context
            } else {
                format!("{}\n\n{}", current_system, file_tree_context)
            };
            conversation.set_system(&enhanced_system);
        }

        // Create tool executor
        let tool_context = ToolContext::new(
            self.working_directory.clone(),
            project_root,
            session_id.0,
            self.trust_mode,
        );
        let mut tool_executor = ToolExecutor::new(tool_context, self.trust_mode);

        // Initialize skill registry
        let mut skill_registry = SkillRegistry::new();
        if let Err(e) = skill_registry.scan() {
            if self.verbose > 0 {
                eprintln!("[verbose] Failed to scan for skills: {}", e);
            }
        }
        let skill_registry = Arc::new(skill_registry);
        tool_executor
            .registry_mut()
            .register_spawn_agent(provider.clone(), skill_registry.clone());

        Ok(ChatSession {
            session_id,
            session_info,
            conversation,
            context_manager,
            tool_executor,
            provider,
            provider_name: self.provider_name,
            model,
            cap_names: self.cap_names,
            merged_cap,
            history_store,
            skill_registry,
            cap_loader: loader,
            cap_resolver: resolver,
            is_resumed,
            message_count,
            trust_mode: self.trust_mode,
        })
    }

    /// Resume a session from history
    fn resume_session_from_history(
        history_store: &HistoryStore,
        resume_id: &str,
        working_directory: &std::path::Path,
    ) -> Result<(SessionId, SessionInfo, usize, bool)> {
        // Try to parse as UUID first
        if let Ok(uuid) = uuid::Uuid::parse_str(resume_id) {
            if let Some(info) = history_store.get(uuid) {
                let count = info.message_count;
                return Ok((SessionId(uuid), info.clone(), count, true));
            }
        }

        // Try to find by short ID prefix in recent sessions
        let recent_sessions = history_store.list_recent(100);
        for session in recent_sessions {
            if session.id.to_string().starts_with(resume_id) {
                let count = session.message_count;
                return Ok((SessionId(session.id), session.clone(), count, true));
            }
        }

        // Not found - create new session
        eprintln!("Session '{}' not found, starting new session", resume_id);
        let sid = SessionId::new();
        let info = SessionInfo::new(sid.0, working_directory.to_path_buf());
        Ok((sid, info, 0, false))
    }
}

impl ChatSession {
    /// Create a builder for constructing a ChatSession
    pub fn builder(settings: Settings) -> ChatSessionBuilder {
        ChatSessionBuilder::new(settings)
    }

    /// Update the session info in history
    pub fn save_to_history(&mut self) -> Result<()> {
        self.history_store.upsert(self.session_info.clone())?;
        Ok(())
    }

    /// Increment the message count
    pub fn increment_message_count(&mut self) {
        self.message_count += 1;
        self.session_info.message_count = self.message_count;
    }

    /// Reload caps with new names
    pub fn reload_caps(&mut self, cap_names: Vec<String>) -> Result<()> {
        self.cap_names = cap_names.clone();
        self.merged_cap = self.cap_resolver.resolve_and_merge(&self.cap_names)?;

        if !self.merged_cap.system_prompt.is_empty() {
            self.conversation.set_system(&self.merged_cap.system_prompt);
        }

        self.session_info.caps = cap_names;
        Ok(())
    }

    /// Change the LLM provider
    pub fn set_provider(&mut self, provider: Arc<dyn LlmProvider>, name: &str) {
        self.provider = provider.clone();
        self.provider_name = name.to_string();

        // Re-register spawn_agent with new provider
        self.tool_executor
            .registry_mut()
            .register_spawn_agent(provider, self.skill_registry.clone());
    }

    /// Change the model
    pub fn set_model(&mut self, model: impl Into<String>) {
        self.model = model.into();
    }

    /// Get the current provider
    pub fn provider(&self) -> &Arc<dyn LlmProvider> {
        &self.provider
    }

    /// Get the current model
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Start background compaction task
    pub fn start_background_compaction(&self, interval_secs: u64) -> tokio::task::JoinHandle<()> {
        self.context_manager
            .start_background_compaction(interval_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_session_builder_creation() {
        let settings = Settings::default();
        let builder = ChatSession::builder(settings);
        // Builder should be created without errors
        assert!(builder.cap_names.contains(&"base".to_string()));
    }
}
