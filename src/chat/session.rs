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
    use std::path::PathBuf;

    // ===== ChatSessionBuilder::new() tests =====

    #[test]
    fn test_chat_session_builder_creation() {
        let settings = Settings::default();
        let builder = ChatSession::builder(settings);
        // Builder should be created without errors
        assert!(builder.cap_names.contains(&"base".to_string()));
    }

    #[test]
    fn test_chat_session_builder_default_values() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings);

        assert!(builder.provider.is_none());
        assert!(builder.provider_name.is_empty());
        assert!(builder.model.is_none());
        assert_eq!(builder.cap_names, vec!["base".to_string()]);
        assert!(!builder.trust_mode);
        assert!(builder.resume_id.is_none());
        assert_eq!(builder.verbose, 0);
    }

    #[test]
    fn test_chat_session_builder_new_with_settings() {
        let mut settings = Settings::default();
        settings.defaults.provider = "ollama".to_string();
        let builder = ChatSessionBuilder::new(settings.clone());

        assert_eq!(builder.settings.defaults.provider, "ollama");
    }

    // ===== with_model() tests =====

    #[test]
    fn test_builder_with_model_string() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_model("gpt-4".to_string());

        assert_eq!(builder.model, Some("gpt-4".to_string()));
    }

    #[test]
    fn test_builder_with_model_str() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_model("claude-3-opus");

        assert_eq!(builder.model, Some("claude-3-opus".to_string()));
    }

    #[test]
    fn test_builder_with_model_chaining() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_model("model-1")
            .with_model("model-2");

        // Last model wins
        assert_eq!(builder.model, Some("model-2".to_string()));
    }

    #[test]
    fn test_builder_with_model_empty_string() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_model("");

        assert_eq!(builder.model, Some("".to_string()));
    }

    // ===== with_caps() tests =====

    #[test]
    fn test_builder_with_caps_single() {
        let settings = Settings::default();
        let builder =
            ChatSessionBuilder::new(settings).with_caps(vec!["code-assistant".to_string()]);

        assert_eq!(builder.cap_names, vec!["code-assistant".to_string()]);
    }

    #[test]
    fn test_builder_with_caps_multiple() {
        let settings = Settings::default();
        let caps = vec![
            "base".to_string(),
            "code-assistant".to_string(),
            "debug".to_string(),
        ];
        let builder = ChatSessionBuilder::new(settings).with_caps(caps.clone());

        assert_eq!(builder.cap_names, caps);
    }

    #[test]
    fn test_builder_with_caps_empty() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_caps(vec![]);

        assert!(builder.cap_names.is_empty());
    }

    #[test]
    fn test_builder_with_caps_replaces_default() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_caps(vec!["base".to_string()])
            .with_caps(vec!["custom".to_string()]);

        // Last caps wins
        assert_eq!(builder.cap_names, vec!["custom".to_string()]);
    }

    // ===== with_trust() tests =====

    #[test]
    fn test_builder_with_trust_true() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_trust(true);

        assert!(builder.trust_mode);
    }

    #[test]
    fn test_builder_with_trust_false() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_trust(false);

        assert!(!builder.trust_mode);
    }

    #[test]
    fn test_builder_with_trust_toggle() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_trust(true)
            .with_trust(false);

        // Last value wins
        assert!(!builder.trust_mode);
    }

    // ===== with_working_directory() tests =====

    #[test]
    fn test_builder_with_working_directory() {
        let settings = Settings::default();
        let dir = PathBuf::from("/tmp/test-dir");
        let builder = ChatSessionBuilder::new(settings).with_working_directory(dir.clone());

        assert_eq!(builder.working_directory, dir);
    }

    #[test]
    fn test_builder_with_working_directory_absolute() {
        let settings = Settings::default();
        let dir = PathBuf::from("/home/user/project");
        let builder = ChatSessionBuilder::new(settings).with_working_directory(dir.clone());

        assert_eq!(builder.working_directory, dir);
    }

    #[test]
    fn test_builder_with_working_directory_relative() {
        let settings = Settings::default();
        let dir = PathBuf::from("./relative/path");
        let builder = ChatSessionBuilder::new(settings).with_working_directory(dir.clone());

        assert_eq!(builder.working_directory, dir);
    }

    // ===== resume_session() tests =====

    #[test]
    fn test_builder_resume_session_string() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).resume_session("abc123".to_string());

        assert_eq!(builder.resume_id, Some("abc123".to_string()));
    }

    #[test]
    fn test_builder_resume_session_str() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).resume_session("session-456");

        assert_eq!(builder.resume_id, Some("session-456".to_string()));
    }

    #[test]
    fn test_builder_resume_session_uuid_format() {
        let settings = Settings::default();
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let builder = ChatSessionBuilder::new(settings).resume_session(uuid_str);

        assert_eq!(builder.resume_id, Some(uuid_str.to_string()));
    }

    #[test]
    fn test_builder_resume_session_short_id() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).resume_session("550e84");

        assert_eq!(builder.resume_id, Some("550e84".to_string()));
    }

    // ===== with_verbose() tests =====

    #[test]
    fn test_builder_with_verbose_zero() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_verbose(0);

        assert_eq!(builder.verbose, 0);
    }

    #[test]
    fn test_builder_with_verbose_one() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_verbose(1);

        assert_eq!(builder.verbose, 1);
    }

    #[test]
    fn test_builder_with_verbose_max() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_verbose(255);

        assert_eq!(builder.verbose, 255);
    }

    #[test]
    fn test_builder_with_verbose_incremental() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_verbose(1)
            .with_verbose(2)
            .with_verbose(3);

        // Last value wins
        assert_eq!(builder.verbose, 3);
    }

    // ===== Fluent API / chaining tests =====

    #[test]
    fn test_builder_full_chain() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_model("gpt-4")
            .with_caps(vec!["code".to_string()])
            .with_trust(true)
            .with_working_directory(PathBuf::from("/project"))
            .resume_session("session-123")
            .with_verbose(2);

        assert_eq!(builder.model, Some("gpt-4".to_string()));
        assert_eq!(builder.cap_names, vec!["code".to_string()]);
        assert!(builder.trust_mode);
        assert_eq!(builder.working_directory, PathBuf::from("/project"));
        assert_eq!(builder.resume_id, Some("session-123".to_string()));
        assert_eq!(builder.verbose, 2);
    }

    #[test]
    fn test_builder_partial_chain() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_model("llama3")
            .with_verbose(1);

        assert_eq!(builder.model, Some("llama3".to_string()));
        assert_eq!(builder.verbose, 1);
        // Check defaults are preserved
        assert_eq!(builder.cap_names, vec!["base".to_string()]);
        assert!(!builder.trust_mode);
        assert!(builder.resume_id.is_none());
    }

    // ===== ChatSession static method tests =====

    #[test]
    fn test_chat_session_builder_method() {
        let settings = Settings::default();
        let builder = ChatSession::builder(settings);

        // Verify it creates a ChatSessionBuilder
        assert_eq!(builder.cap_names, vec!["base".to_string()]);
    }

    // ===== Edge case tests =====

    #[test]
    fn test_builder_with_model_unicode() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_model("模型-中文");

        assert_eq!(builder.model, Some("模型-中文".to_string()));
    }

    #[test]
    fn test_builder_with_caps_duplicates() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_caps(vec![
            "base".to_string(),
            "base".to_string(),
            "base".to_string(),
        ]);

        assert_eq!(builder.cap_names.len(), 3);
    }

    #[test]
    fn test_builder_with_working_directory_empty() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_working_directory(PathBuf::new());

        assert_eq!(builder.working_directory, PathBuf::new());
    }

    #[test]
    fn test_builder_resume_session_empty_string() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).resume_session("");

        assert_eq!(builder.resume_id, Some("".to_string()));
    }

    #[test]
    fn test_builder_multiple_settings_instances() {
        let settings1 = Settings::default();
        let settings2 = Settings::default();

        let builder1 = ChatSessionBuilder::new(settings1).with_model("model-a");
        let builder2 = ChatSessionBuilder::new(settings2).with_model("model-b");

        assert_eq!(builder1.model, Some("model-a".to_string()));
        assert_eq!(builder2.model, Some("model-b".to_string()));
    }

    // ===== SessionId tests =====

    #[test]
    fn test_session_id_new() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();

        // Each new session ID should be unique
        assert_ne!(id1.0, id2.0);
    }

    #[test]
    fn test_session_id_clone() {
        let id = SessionId::new();
        let cloned = id.clone();

        assert_eq!(id.0, cloned.0);
    }

    // ==================== Additional comprehensive tests ====================

    // ===== SessionId additional tests =====

    #[test]
    fn test_session_id_not_nil() {
        let id = SessionId::new();
        assert!(!id.0.is_nil());
    }

    #[test]
    fn test_session_id_formatting() {
        let id = SessionId::new();
        let formatted = id.0.to_string();
        // UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
        assert_eq!(formatted.len(), 36);
        assert!(formatted.contains('-'));
    }

    #[test]
    fn test_session_id_multiple_unique() {
        let ids: Vec<SessionId> = (0..100).map(|_| SessionId::new()).collect();
        let unique_ids: std::collections::HashSet<_> = ids.iter().map(|id| id.0).collect();
        assert_eq!(unique_ids.len(), 100);
    }

    // ===== ChatSessionBuilder with_provider tests =====

    #[test]
    fn test_builder_with_provider_sets_name() {
        // Create a mock provider for testing
        struct MockProvider;

        #[async_trait::async_trait]
        impl crate::llm::provider::LlmProvider for MockProvider {
            fn name(&self) -> &str {
                "mock"
            }

            fn available_models(&self) -> Vec<crate::llm::provider::ModelInfo> {
                vec![]
            }

            fn supports_model(&self, _model: &str) -> bool {
                true
            }

            async fn complete(
                &self,
                _request: crate::llm::provider::CompletionRequest,
            ) -> crate::error::Result<crate::llm::provider::CompletionResponse> {
                unimplemented!()
            }

            async fn complete_stream(
                &self,
                _request: crate::llm::provider::CompletionRequest,
            ) -> crate::error::Result<
                std::pin::Pin<
                    Box<
                        dyn futures::Stream<
                                Item = crate::error::Result<crate::llm::provider::StreamEvent>,
                            > + Send,
                    >,
                >,
            > {
                unimplemented!()
            }

            fn count_tokens(&self, _text: &str, _model: &str) -> crate::error::Result<u32> {
                Ok(0)
            }
        }

        let settings = Settings::default();
        let provider = Arc::new(MockProvider);
        let builder = ChatSessionBuilder::new(settings).with_provider(provider, "test-provider");

        assert_eq!(builder.provider_name, "test-provider");
        assert!(builder.provider.is_some());
    }

    // ===== Builder default values tests =====

    #[test]
    fn test_builder_default_cap_names() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings);
        assert_eq!(builder.cap_names, vec!["base".to_string()]);
    }

    #[test]
    fn test_builder_default_trust_mode() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings);
        assert!(!builder.trust_mode);
    }

    #[test]
    fn test_builder_default_verbose() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings);
        assert_eq!(builder.verbose, 0);
    }

    #[test]
    fn test_builder_default_resume_id() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings);
        assert!(builder.resume_id.is_none());
    }

    // ===== Builder chaining order independence =====

    #[test]
    fn test_builder_chaining_order_1() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_model("model-1")
            .with_trust(true)
            .with_verbose(2);

        assert_eq!(builder.model, Some("model-1".to_string()));
        assert!(builder.trust_mode);
        assert_eq!(builder.verbose, 2);
    }

    #[test]
    fn test_builder_chaining_order_2() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_verbose(2)
            .with_trust(true)
            .with_model("model-1");

        assert_eq!(builder.model, Some("model-1".to_string()));
        assert!(builder.trust_mode);
        assert_eq!(builder.verbose, 2);
    }

    #[test]
    fn test_builder_chaining_order_3() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_trust(true)
            .with_model("model-1")
            .with_verbose(2);

        assert_eq!(builder.model, Some("model-1".to_string()));
        assert!(builder.trust_mode);
        assert_eq!(builder.verbose, 2);
    }

    // ===== Builder override behavior =====

    #[test]
    fn test_builder_caps_override() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_caps(vec!["cap1".to_string()])
            .with_caps(vec!["cap2".to_string()])
            .with_caps(vec!["cap3".to_string()]);

        assert_eq!(builder.cap_names, vec!["cap3".to_string()]);
    }

    #[test]
    fn test_builder_model_override() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_model("model-1")
            .with_model("model-2")
            .with_model("model-3");

        assert_eq!(builder.model, Some("model-3".to_string()));
    }

    #[test]
    fn test_builder_trust_override() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_trust(true)
            .with_trust(false)
            .with_trust(true);

        assert!(builder.trust_mode);
    }

    #[test]
    fn test_builder_verbose_override() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_verbose(1)
            .with_verbose(2)
            .with_verbose(0);

        assert_eq!(builder.verbose, 0);
    }

    // ===== Working directory tests =====

    #[test]
    fn test_builder_working_directory_absolute() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_working_directory(PathBuf::from("/absolute/path"));

        assert!(builder.working_directory.is_absolute());
    }

    #[test]
    fn test_builder_working_directory_relative() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_working_directory(PathBuf::from("relative/path"));

        assert!(builder.working_directory.is_relative());
    }

    #[test]
    fn test_builder_working_directory_with_dots() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_working_directory(PathBuf::from("../parent/path"));

        assert!(builder.working_directory.to_str().unwrap().contains(".."));
    }

    // ===== Resume session tests =====

    #[test]
    fn test_builder_resume_short_id() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).resume_session("abc123");

        assert_eq!(builder.resume_id, Some("abc123".to_string()));
    }

    #[test]
    fn test_builder_resume_full_uuid() {
        let settings = Settings::default();
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let builder = ChatSessionBuilder::new(settings).resume_session(uuid);

        assert_eq!(builder.resume_id, Some(uuid.to_string()));
    }

    #[test]
    fn test_builder_resume_override() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .resume_session("first")
            .resume_session("second");

        assert_eq!(builder.resume_id, Some("second".to_string()));
    }

    // ===== Model name variations =====

    #[test]
    fn test_builder_with_model_empty() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_model("");

        assert_eq!(builder.model, Some("".to_string()));
    }

    #[test]
    fn test_builder_with_model_long_name() {
        let settings = Settings::default();
        let long_name = "a".repeat(1000);
        let builder = ChatSessionBuilder::new(settings).with_model(&long_name);

        assert_eq!(builder.model, Some(long_name));
    }

    #[test]
    fn test_builder_with_model_special_chars() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings)
            .with_model("model-with-dashes_and_underscores:version");

        assert_eq!(
            builder.model,
            Some("model-with-dashes_and_underscores:version".to_string())
        );
    }

    // ===== Caps variations =====

    #[test]
    fn test_builder_with_caps_many() {
        let settings = Settings::default();
        let caps: Vec<String> = (0..50).map(|i| format!("cap-{}", i)).collect();
        let builder = ChatSessionBuilder::new(settings).with_caps(caps.clone());

        assert_eq!(builder.cap_names.len(), 50);
    }

    #[test]
    fn test_builder_with_caps_special_names() {
        let settings = Settings::default();
        let builder = ChatSessionBuilder::new(settings).with_caps(vec![
            "cap-with-dash".to_string(),
            "cap_with_underscore".to_string(),
            "CapWithCamelCase".to_string(),
        ]);

        assert_eq!(builder.cap_names.len(), 3);
    }

    // ===== Settings preservation =====

    #[test]
    fn test_builder_preserves_settings() {
        let mut settings = Settings::default();
        settings.defaults.provider = "custom_provider".to_string();

        let builder = ChatSessionBuilder::new(settings.clone());

        assert_eq!(builder.settings.defaults.provider, "custom_provider");
    }

    // ===== PathBuf edge cases =====

    #[test]
    fn test_pathbuf_equality() {
        let path1 = PathBuf::from("/test/path");
        let path2 = PathBuf::from("/test/path");
        assert_eq!(path1, path2);
    }

    #[test]
    fn test_pathbuf_clone() {
        let original = PathBuf::from("/original");
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_pathbuf_new() {
        let empty = PathBuf::new();
        assert!(empty.as_os_str().is_empty());
    }

    // ===== Mock provider for testing ChatSession::build() =====

    struct TestMockProvider {
        name: String,
    }

    impl TestMockProvider {
        fn new() -> Self {
            Self {
                name: "test_mock".to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl crate::llm::provider::LlmProvider for TestMockProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn available_models(&self) -> Vec<crate::llm::provider::ModelInfo> {
            vec![crate::llm::provider::ModelInfo {
                id: "test-model".to_string(),
                display_name: "Test Model".to_string(),
                context_window: 4096,
                max_output_tokens: 1024,
                supports_tools: true,
                supports_vision: false,
                input_cost_per_1k: 0.0,
                output_cost_per_1k: 0.0,
            }]
        }

        fn supports_model(&self, _model: &str) -> bool {
            true
        }

        async fn complete(
            &self,
            _request: crate::llm::provider::CompletionRequest,
        ) -> crate::error::Result<crate::llm::provider::CompletionResponse> {
            Ok(crate::llm::provider::CompletionResponse {
                id: "test-response-id".to_string(),
                model: "test-model".to_string(),
                content: vec![crate::llm::provider::ContentBlockResponse::Text {
                    text: "Test response".to_string(),
                }],
                stop_reason: Some(crate::llm::provider::StopReason::EndTurn),
                usage: crate::llm::provider::Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
            })
        }

        async fn complete_stream(
            &self,
            _request: crate::llm::provider::CompletionRequest,
        ) -> crate::error::Result<
            std::pin::Pin<
                Box<
                    dyn futures::Stream<
                            Item = crate::error::Result<crate::llm::provider::StreamEvent>,
                        > + Send,
                >,
            >,
        > {
            Ok(Box::pin(futures::stream::empty()))
        }

        fn count_tokens(&self, _text: &str, _model: &str) -> crate::error::Result<u32> {
            Ok(10)
        }
    }

    // ===== ChatSession::build() tests =====

    #[tokio::test]
    async fn test_build_without_provider_returns_error() {
        let settings = Settings::default();
        let result = ChatSessionBuilder::new(settings)
            .with_working_directory(std::env::temp_dir())
            .build()
            .await;

        assert!(result.is_err());
        let err = result.err().expect("Expected an error");
        assert!(err.to_string().contains("No LLM provider"));
    }

    #[tokio::test]
    async fn test_build_with_provider_succeeds() {
        let settings = Settings::default();
        let provider = Arc::new(TestMockProvider::new());

        let result = ChatSessionBuilder::new(settings)
            .with_provider(provider, "test")
            .with_working_directory(std::env::temp_dir())
            .with_model("test-model")
            .build()
            .await;

        assert!(result.is_ok());
        let session = result.unwrap();
        assert_eq!(session.model, "test-model");
        assert_eq!(session.provider_name, "test");
        assert!(!session.is_resumed);
        assert_eq!(session.message_count, 0);
    }

    #[tokio::test]
    async fn test_build_with_trust_mode() {
        let settings = Settings::default();
        let provider = Arc::new(TestMockProvider::new());

        let result = ChatSessionBuilder::new(settings)
            .with_provider(provider, "test")
            .with_working_directory(std::env::temp_dir())
            .with_trust(true)
            .build()
            .await;

        assert!(result.is_ok());
        let session = result.unwrap();
        assert!(session.trust_mode);
    }

    #[tokio::test]
    async fn test_build_with_custom_caps() {
        let settings = Settings::default();
        let provider = Arc::new(TestMockProvider::new());

        let result = ChatSessionBuilder::new(settings)
            .with_provider(provider, "test")
            .with_working_directory(std::env::temp_dir())
            .with_caps(vec!["base".to_string()])
            .build()
            .await;

        assert!(result.is_ok());
        let session = result.unwrap();
        assert_eq!(session.cap_names, vec!["base".to_string()]);
    }

    #[tokio::test]
    async fn test_build_with_verbose() {
        let settings = Settings::default();
        let provider = Arc::new(TestMockProvider::new());

        let result = ChatSessionBuilder::new(settings)
            .with_provider(provider, "test")
            .with_working_directory(std::env::temp_dir())
            .with_verbose(2)
            .build()
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_build_resume_nonexistent_session() {
        let settings = Settings::default();
        let provider = Arc::new(TestMockProvider::new());

        let result = ChatSessionBuilder::new(settings)
            .with_provider(provider, "test")
            .with_working_directory(std::env::temp_dir())
            .resume_session("nonexistent-session-id")
            .build()
            .await;

        // Should create a new session when resume fails
        assert!(result.is_ok());
        let session = result.unwrap();
        // New session since the ID wasn't found
        assert!(!session.is_resumed);
    }

    // ===== ChatSession method tests =====

    #[tokio::test]
    async fn test_session_increment_message_count() {
        let settings = Settings::default();
        let provider = Arc::new(TestMockProvider::new());

        let mut session = ChatSessionBuilder::new(settings)
            .with_provider(provider, "test")
            .with_working_directory(std::env::temp_dir())
            .build()
            .await
            .unwrap();

        assert_eq!(session.message_count, 0);
        session.increment_message_count();
        assert_eq!(session.message_count, 1);
        session.increment_message_count();
        assert_eq!(session.message_count, 2);
        assert_eq!(session.session_info.message_count, 2);
    }

    #[tokio::test]
    async fn test_session_set_model() {
        let settings = Settings::default();
        let provider = Arc::new(TestMockProvider::new());

        let mut session = ChatSessionBuilder::new(settings)
            .with_provider(provider, "test")
            .with_working_directory(std::env::temp_dir())
            .with_model("initial-model")
            .build()
            .await
            .unwrap();

        assert_eq!(session.model(), "initial-model");
        session.set_model("new-model");
        assert_eq!(session.model(), "new-model");
    }

    #[tokio::test]
    async fn test_session_set_provider() {
        let settings = Settings::default();
        let provider1 = Arc::new(TestMockProvider::new());
        let provider2 = Arc::new(TestMockProvider::new());

        let mut session = ChatSessionBuilder::new(settings)
            .with_provider(provider1, "provider1")
            .with_working_directory(std::env::temp_dir())
            .build()
            .await
            .unwrap();

        assert_eq!(session.provider_name, "provider1");
        session.set_provider(provider2, "provider2");
        assert_eq!(session.provider_name, "provider2");
    }

    #[tokio::test]
    async fn test_session_provider_accessor() {
        let settings = Settings::default();
        let provider = Arc::new(TestMockProvider::new());

        let session = ChatSessionBuilder::new(settings)
            .with_provider(provider, "test")
            .with_working_directory(std::env::temp_dir())
            .build()
            .await
            .unwrap();

        let p = session.provider();
        assert_eq!(p.name(), "test_mock");
    }

    #[tokio::test]
    async fn test_session_reload_caps() {
        let settings = Settings::default();
        let provider = Arc::new(TestMockProvider::new());

        let mut session = ChatSessionBuilder::new(settings)
            .with_provider(provider, "test")
            .with_working_directory(std::env::temp_dir())
            .with_caps(vec!["base".to_string()])
            .build()
            .await
            .unwrap();

        assert_eq!(session.cap_names, vec!["base".to_string()]);

        // Reload with same caps (should succeed)
        let result = session.reload_caps(vec!["base".to_string()]);
        assert!(result.is_ok());
        assert_eq!(session.cap_names, vec!["base".to_string()]);
    }

    #[tokio::test]
    async fn test_session_save_to_history() {
        let settings = Settings::default();
        let provider = Arc::new(TestMockProvider::new());

        let mut session = ChatSessionBuilder::new(settings)
            .with_provider(provider, "test")
            .with_working_directory(std::env::temp_dir())
            .build()
            .await
            .unwrap();

        // Should not panic
        let result = session.save_to_history();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_session_start_background_compaction() {
        let settings = Settings::default();
        let provider = Arc::new(TestMockProvider::new());

        let session = ChatSessionBuilder::new(settings)
            .with_provider(provider, "test")
            .with_working_directory(std::env::temp_dir())
            .build()
            .await
            .unwrap();

        // Start compaction task - just verify it doesn't panic
        let handle = session.start_background_compaction(3600);
        // Abort immediately since we don't want it running in tests
        handle.abort();
    }

    // ===== resume_session_from_history tests =====

    #[test]
    fn test_resume_session_from_history_invalid_uuid() {
        let history_store = crate::history::HistoryStore::open().unwrap();
        let working_dir = std::env::temp_dir();

        let result = ChatSessionBuilder::resume_session_from_history(
            &history_store,
            "invalid-uuid-format",
            &working_dir,
        );

        // Should return Ok with a new session since the ID is invalid
        assert!(result.is_ok());
        let (_, _, count, is_resumed) = result.unwrap();
        assert!(!is_resumed);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_resume_session_from_history_valid_uuid_not_found() {
        let history_store = crate::history::HistoryStore::open().unwrap();
        let working_dir = std::env::temp_dir();

        // Valid UUID format but doesn't exist in history
        let result = ChatSessionBuilder::resume_session_from_history(
            &history_store,
            "550e8400-e29b-41d4-a716-446655440000",
            &working_dir,
        );

        assert!(result.is_ok());
        let (_, _, _, is_resumed) = result.unwrap();
        // Should create new session since UUID not found
        assert!(!is_resumed);
    }

    #[test]
    fn test_resume_session_from_history_short_id() {
        let history_store = crate::history::HistoryStore::open().unwrap();
        let working_dir = std::env::temp_dir();

        // Short ID that won't match anything
        let result =
            ChatSessionBuilder::resume_session_from_history(&history_store, "abc123", &working_dir);

        assert!(result.is_ok());
        let (_, _, _, is_resumed) = result.unwrap();
        assert!(!is_resumed);
    }

    #[test]
    fn test_resume_session_from_history_empty_string() {
        let history_store = crate::history::HistoryStore::open().unwrap();
        let working_dir = std::env::temp_dir();

        let result =
            ChatSessionBuilder::resume_session_from_history(&history_store, "", &working_dir);

        // Empty string matches any session via starts_with(""), so if there are
        // recent sessions, it will resume one of them. We just verify it doesn't error.
        assert!(result.is_ok());
    }
}
