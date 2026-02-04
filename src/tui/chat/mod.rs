// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Chat TUI module
//!
//! A full-featured terminal interface for the main chat experience, providing:
//! - Real-time message display with streaming support
//! - Agent tracking and progress visualization
//! - Interactive input with history navigation
//! - Tool call display with expand/collapse

pub mod app;
pub mod events;
pub mod input;
pub mod runner;
pub mod state;
pub mod ui;
pub mod widgets;
pub mod wrapper;

use std::io;
use std::sync::Arc;

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use tokio::sync::mpsc;

use crate::config::Settings;
use crate::context::ContextManager;
use crate::error::{Result, TedError};
use crate::llm::provider::LlmProvider;
use crate::tools::ToolExecutor;

pub use app::{ChatApp, ChatMode};
pub use events::{ChatEvent, EventEmitter, EventSender};
pub use runner::run_chat_tui_loop;
pub use state::agents::{AgentStatus, AgentTracker, TrackedAgent};
pub use wrapper::{AgentObserver, AgentObserverFactory, StreamingWrapper, ToolExecutorWrapper};

/// Configuration for the chat TUI
#[derive(Clone)]
pub struct ChatTuiConfig {
    pub session_id: uuid::Uuid,
    pub provider_name: String,
    pub model: String,
    pub caps: Vec<String>,
    pub trust_mode: bool,
    pub stream_enabled: bool,
}

/// Run the chat TUI
///
/// This is the main entry point for the TUI-based chat interface.
/// Returns Ok(true) if the user wants to continue (e.g., after changing settings),
/// or Ok(false) if the user wants to exit.
pub async fn run_chat_tui(
    config: ChatTuiConfig,
    provider: Arc<dyn LlmProvider>,
    tool_executor: ToolExecutor,
    context_manager: ContextManager,
    settings: Settings,
) -> Result<bool> {
    // Setup terminal with panic hook to restore terminal on crash
    let original_panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_panic_hook(panic_info);
    }));

    enable_raw_mode().map_err(|e| TedError::Tui(e.to_string()))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| TedError::Tui(e.to_string()))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| TedError::Tui(e.to_string()))?;

    // Create event channel
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    // Create app
    let mut app = ChatApp::new(
        config,
        event_tx.clone(),
        event_rx,
        provider,
        tool_executor,
        context_manager,
        settings,
    );

    // Run the main loop
    let result = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    let _ = std::panic::take_hook();

    disable_raw_mode().map_err(|e| TedError::Tui(e.to_string()))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|e| TedError::Tui(e.to_string()))?;
    terminal
        .show_cursor()
        .map_err(|e| TedError::Tui(e.to_string()))?;

    result
}

/// Main application loop
async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut ChatApp) -> Result<bool> {
    loop {
        // Render UI
        terminal
            .draw(|f| ui::draw(f, app))
            .map_err(|e| TedError::Tui(e.to_string()))?;

        // Handle events with timeout for smooth updates
        match app.tick().await? {
            app::TickResult::Continue => {}
            app::TickResult::Quit => return Ok(false),
            app::TickResult::Restart => return Ok(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::mock_provider::MockProvider;
    use crate::tools::ToolContext;
    use ratatui::backend::TestBackend;
    use uuid::Uuid;

    // ==================== ChatTuiConfig Tests ====================

    #[test]
    fn test_chat_tui_config_creation() {
        let config = ChatTuiConfig {
            session_id: uuid::Uuid::new_v4(),
            provider_name: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            caps: vec!["rust-expert".to_string()],
            trust_mode: false,
            stream_enabled: true,
        };

        assert_eq!(config.provider_name, "anthropic");
        assert!(!config.trust_mode);
    }

    #[test]
    fn test_chat_tui_config_all_fields() {
        let session_id = uuid::Uuid::new_v4();
        let config = ChatTuiConfig {
            session_id,
            provider_name: "ollama".to_string(),
            model: "llama3".to_string(),
            caps: vec!["base".to_string(), "code-review".to_string()],
            trust_mode: true,
            stream_enabled: false,
        };

        assert_eq!(config.session_id, session_id);
        assert_eq!(config.provider_name, "ollama");
        assert_eq!(config.model, "llama3");
        assert_eq!(config.caps.len(), 2);
        assert!(config.trust_mode);
        assert!(!config.stream_enabled);
    }

    #[test]
    fn test_chat_tui_config_clone() {
        let config = ChatTuiConfig {
            session_id: uuid::Uuid::new_v4(),
            provider_name: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            caps: vec!["base".to_string()],
            trust_mode: false,
            stream_enabled: true,
        };

        let cloned = config.clone();
        assert_eq!(cloned.session_id, config.session_id);
        assert_eq!(cloned.provider_name, config.provider_name);
        assert_eq!(cloned.model, config.model);
        assert_eq!(cloned.caps, config.caps);
        assert_eq!(cloned.trust_mode, config.trust_mode);
        assert_eq!(cloned.stream_enabled, config.stream_enabled);
    }

    #[test]
    fn test_chat_tui_config_empty_caps() {
        let config = ChatTuiConfig {
            session_id: uuid::Uuid::new_v4(),
            provider_name: "openrouter".to_string(),
            model: "gpt-4".to_string(),
            caps: vec![],
            trust_mode: false,
            stream_enabled: true,
        };

        assert!(config.caps.is_empty());
    }

    #[test]
    fn test_chat_tui_config_provider_variants() {
        let providers = ["anthropic", "ollama", "openrouter", "blackman"];

        for provider in providers {
            let config = ChatTuiConfig {
                session_id: uuid::Uuid::new_v4(),
                provider_name: provider.to_string(),
                model: "test-model".to_string(),
                caps: vec![],
                trust_mode: false,
                stream_enabled: true,
            };
            assert_eq!(config.provider_name, provider);
        }
    }

    #[test]
    fn test_chat_mode_reexport() {
        // Test that ChatMode is properly re-exported
        let mode = ChatMode::Input;
        assert_eq!(mode, ChatMode::Input);
    }

    // ==================== Re-export Tests ====================

    #[test]
    fn test_chat_app_reexport() {
        // ChatApp and ChatMode are re-exported
        let _ = ChatMode::Normal;
        let _ = ChatMode::Help;
        let _ = ChatMode::AgentFocus;
    }

    #[test]
    fn test_event_types_reexport() {
        // Test that ChatEvent and EventSender are accessible
        let (tx, _rx) = mpsc::unbounded_channel::<ChatEvent>();
        let _ = tx.send(ChatEvent::Refresh);
    }

    #[test]
    fn test_agent_types_reexport() {
        // Test that AgentTracker and related types are accessible
        let tracker = AgentTracker::new();
        assert_eq!(tracker.total_count(), 0);
    }

    #[test]
    fn test_agent_status_reexport() {
        // AgentStatus enum is accessible
        let status = AgentStatus::Running;
        assert_eq!(status, AgentStatus::Running);
    }

    // ==================== Helper function for creating test app ====================

    async fn create_test_chat_app() -> ChatApp {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let storage_path = temp_dir.path().to_path_buf();

        let config = ChatTuiConfig {
            session_id: Uuid::new_v4(),
            provider_name: "mock".to_string(),
            model: "test-model".to_string(),
            caps: vec!["test-cap".to_string()],
            trust_mode: false,
            stream_enabled: true,
        };

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::new());

        let tool_context = ToolContext::new(
            storage_path.clone(),
            Some(storage_path.clone()),
            config.session_id,
            false,
        );
        let tool_executor = ToolExecutor::new(tool_context, false);

        let context_manager = ContextManager::new_session(storage_path).await.unwrap();
        let settings = Settings::default();

        ChatApp::new(
            config,
            event_tx,
            event_rx,
            provider,
            tool_executor,
            context_manager,
            settings,
        )
    }

    // ==================== run_app Tests ====================

    #[tokio::test]
    async fn test_run_app_quit_immediately() {
        let mut app = create_test_chat_app().await;
        app.should_quit = true;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let result = run_app(&mut terminal, &mut app).await;

        assert!(result.is_ok());
        assert!(!result.unwrap()); // false = quit
    }

    #[tokio::test]
    async fn test_run_app_restart_immediately() {
        let mut app = create_test_chat_app().await;
        app.should_restart = true;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let result = run_app(&mut terminal, &mut app).await;

        assert!(result.is_ok());
        assert!(result.unwrap()); // true = restart
    }

    #[tokio::test]
    async fn test_run_app_renders_ui() {
        let mut app = create_test_chat_app().await;
        app.should_quit = true;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Add some state to render
        app.messages
            .push(state::DisplayMessage::user("Test message".to_string()));

        let result = run_app(&mut terminal, &mut app).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_app_with_agents() {
        let mut app = create_test_chat_app().await;

        // Add an agent
        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            "Explorer".to_string(),
            "explore".to_string(),
            "Find files".to_string(),
        );

        app.should_quit = true;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let result = run_app(&mut terminal, &mut app).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_app_with_help_mode() {
        let mut app = create_test_chat_app().await;
        app.mode = ChatMode::Help;
        app.should_quit = true;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let result = run_app(&mut terminal, &mut app).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_app_with_confirm_mode() {
        let mut app = create_test_chat_app().await;
        app.mode = ChatMode::Confirm;
        app.confirm_message = Some("Are you sure?".to_string());
        app.should_quit = true;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let result = run_app(&mut terminal, &mut app).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_app_small_terminal() {
        let mut app = create_test_chat_app().await;
        app.should_quit = true;

        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        let result = run_app(&mut terminal, &mut app).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_app_large_terminal() {
        let mut app = create_test_chat_app().await;
        app.should_quit = true;

        let backend = TestBackend::new(200, 60);
        let mut terminal = Terminal::new(backend).unwrap();

        let result = run_app(&mut terminal, &mut app).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_app_with_status_message() {
        let mut app = create_test_chat_app().await;
        app.status_message = Some("Processing...".to_string());
        app.status_is_error = false;
        app.should_quit = true;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let result = run_app(&mut terminal, &mut app).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_app_with_error_message() {
        let mut app = create_test_chat_app().await;
        app.status_message = Some("Something went wrong".to_string());
        app.status_is_error = true;
        app.should_quit = true;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let result = run_app(&mut terminal, &mut app).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_app_processing_state() {
        let mut app = create_test_chat_app().await;
        app.is_processing = true;
        app.should_quit = true;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let result = run_app(&mut terminal, &mut app).await;
        assert!(result.is_ok());
    }

    // ==================== Integration Tests ====================

    #[tokio::test]
    async fn test_full_chat_flow_simulation() {
        let mut app = create_test_chat_app().await;

        // Simulate a chat flow
        app.messages
            .push(state::DisplayMessage::user("Hello".to_string()));
        app.messages.push(state::DisplayMessage::assistant(
            "Hi there!".to_string(),
            vec![],
        ));
        app.messages
            .push(state::DisplayMessage::user("How are you?".to_string()));
        app.messages.push(state::DisplayMessage::assistant(
            "I'm doing well, thanks!".to_string(),
            vec![],
        ));

        app.should_quit = true;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let result = run_app(&mut terminal, &mut app).await;
        assert!(result.is_ok());
        assert_eq!(app.messages.len(), 4);
    }

    #[tokio::test]
    async fn test_agent_lifecycle_simulation() {
        let mut app = create_test_chat_app().await;

        // Track agent
        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            "Explorer".to_string(),
            "explore".to_string(),
            "Find files".to_string(),
        );

        // Update progress
        app.agents.update_progress(&agent_id, 1, 10, "Searching...");

        // Complete agent
        app.agents.set_completed(
            &agent_id,
            vec!["file.rs".to_string()],
            Some("Found 5 files".to_string()),
        );

        app.should_quit = true;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let result = run_app(&mut terminal, &mut app).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_tool_call_simulation() {
        let mut app = create_test_chat_app().await;

        // Create a message with a tool call
        let mut msg = state::DisplayMessage::assistant("Reading file...".to_string(), vec![]);
        msg.add_tool_call(state::DisplayToolCall::new(
            "tool-1".to_string(),
            "file_read".to_string(),
            serde_json::json!({"path": "/test.txt"}),
        ));

        // Complete the tool call
        if let Some(tc) = msg.tool_calls.iter_mut().find(|tc| tc.id == "tool-1") {
            tc.complete_success(
                Some("File content here".to_string()),
                Some("Full file content...".to_string()),
            );
        }

        app.messages.push(msg);
        app.should_quit = true;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let result = run_app(&mut terminal, &mut app).await;
        assert!(result.is_ok());
    }
}
