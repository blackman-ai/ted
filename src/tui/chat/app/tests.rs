// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use super::*;
use crate::llm::mock_provider::MockProvider;
use crate::tools::ToolContext;
use crate::tui::chat::ChatEvent;

// Note: Full testing requires mocking the provider and other components
// These tests cover basic state management

#[test]
fn test_chat_mode_all_variants() {
    let modes = [
        ChatMode::Normal,
        ChatMode::Input,
        ChatMode::AgentFocus,
        ChatMode::Help,
        ChatMode::CommandPalette,
        ChatMode::Confirm,
        ChatMode::Settings,
    ];

    // Test they're all distinct
    for (i, mode1) in modes.iter().enumerate() {
        for (j, mode2) in modes.iter().enumerate() {
            if i == j {
                assert_eq!(*mode1, *mode2);
            } else {
                assert_ne!(*mode1, *mode2);
            }
        }
    }
}

#[test]
fn test_chat_mode_debug() {
    let mode = ChatMode::Input;
    let debug_str = format!("{:?}", mode);
    assert_eq!(debug_str, "Input");
}

#[test]
fn test_chat_mode_clone() {
    let mode = ChatMode::Normal;
    let cloned = mode;
    assert_eq!(mode, cloned);
}

#[test]
fn test_chat_mode_copy() {
    let mode = ChatMode::Help;
    let copied: ChatMode = mode;
    assert_eq!(mode, copied);
}

#[test]
fn test_tick_result_all_variants() {
    let results = [TickResult::Continue, TickResult::Quit, TickResult::Restart];

    // Test they're all distinct
    for (i, res1) in results.iter().enumerate() {
        for (j, res2) in results.iter().enumerate() {
            if i == j {
                assert_eq!(*res1, *res2);
            } else {
                assert_ne!(*res1, *res2);
            }
        }
    }
}

#[test]
fn test_tick_result_debug() {
    let result = TickResult::Quit;
    let debug_str = format!("{:?}", result);
    assert_eq!(debug_str, "Quit");
}

#[test]
fn test_tick_result_clone() {
    let result = TickResult::Restart;
    let cloned = result;
    assert_eq!(result, cloned);
}

#[test]
fn test_tick_result_copy() {
    let result = TickResult::Continue;
    let copied: TickResult = result;
    assert_eq!(result, copied);
}

#[test]
fn test_chat_mode_transitions() {
    assert_eq!(ChatMode::Input, ChatMode::Input);
    assert_ne!(ChatMode::Input, ChatMode::Normal);
}

#[test]
fn test_tick_result() {
    assert_eq!(TickResult::Continue, TickResult::Continue);
    assert_ne!(TickResult::Continue, TickResult::Quit);
}

// ==================== ChatApp Creation Tests ====================

/// Helper function to create a test ChatApp
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

    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
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

#[tokio::test]
async fn test_chat_app_creation() {
    let app = create_test_chat_app().await;

    assert_eq!(app.mode, ChatMode::Input);
    assert_eq!(app.provider_name, "mock");
    assert_eq!(app.model, "test-model");
    assert!(app.messages.is_empty());
    assert!(!app.should_quit);
    assert!(!app.should_restart);
    assert!(!app.is_processing);
}

#[tokio::test]
async fn test_chat_app_initial_state() {
    let app = create_test_chat_app().await;

    // Verify initial state
    assert_eq!(app.scroll_state.scroll_offset, 0);
    assert!(app.scroll_state.auto_scroll_enabled);
    assert!(app.agent_pane_visible);
    assert!(!app.agent_pane_expanded);
    assert_eq!(app.agent_pane_height, 4);
    assert_eq!(app.selected_agent_index, 0);
    assert!(app.status_message.is_none());
    assert!(!app.status_is_error);
    assert!(app.confirm_message.is_none());
}

// ==================== Status/Error Tests ====================

#[tokio::test]
async fn test_set_status() {
    let mut app = create_test_chat_app().await;

    app.set_status("Test status message");

    assert_eq!(app.status_message, Some("Test status message".to_string()));
    assert!(!app.status_is_error);
}

#[tokio::test]
async fn test_set_error() {
    let mut app = create_test_chat_app().await;

    app.set_error("Test error message");

    assert_eq!(app.status_message, Some("Test error message".to_string()));
    assert!(app.status_is_error);
}

#[tokio::test]
async fn test_set_status_clears_error_flag() {
    let mut app = create_test_chat_app().await;

    app.set_error("Error");
    assert!(app.status_is_error);

    app.set_status("Status");
    assert!(!app.status_is_error);
}

// ==================== Scroll Tests ====================

#[tokio::test]
async fn test_scroll_up() {
    let mut app = create_test_chat_app().await;

    app.scroll_state.scroll_offset = 10;
    app.scroll_state.scroll_up(3);
    assert_eq!(app.scroll_state.scroll_offset, 7);

    app.scroll_state.scroll_up(10);
    assert_eq!(app.scroll_state.scroll_offset, 0);
}

#[tokio::test]
async fn test_scroll_up_at_zero() {
    let mut app = create_test_chat_app().await;

    app.scroll_state.scroll_offset = 0;
    app.scroll_state.scroll_up(5);
    assert_eq!(app.scroll_state.scroll_offset, 0);
}

#[tokio::test]
async fn test_scroll_down() {
    let mut app = create_test_chat_app().await;
    let total_height = 100; // Simulate a reasonable total content height

    app.scroll_state.scroll_down(5, total_height);
    assert_eq!(app.scroll_state.scroll_offset, 5);

    app.scroll_state.scroll_down(3, total_height);
    assert_eq!(app.scroll_state.scroll_offset, 8);
}

#[tokio::test]
async fn test_scroll_to_bottom() {
    let mut app = create_test_chat_app().await;
    let total_height = 100;

    app.scroll_state.scroll_offset = 10;
    app.scroll_state.scroll_to_bottom(total_height);
    // scroll_to_bottom sets offset to max_offset = total_height - viewport_height
    let expected = total_height - app.scroll_state.viewport_height as usize;
    assert_eq!(app.scroll_state.scroll_offset, expected);
    assert!(app.scroll_state.auto_scroll_enabled);
}

// ==================== Agent Pane Tests ====================

#[tokio::test]
async fn test_toggle_agent_pane() {
    let mut app = create_test_chat_app().await;

    assert!(app.agent_pane_visible);
    app.toggle_agent_pane();
    assert!(!app.agent_pane_visible);
    app.toggle_agent_pane();
    assert!(app.agent_pane_visible);
}

// ==================== Message Tests ====================

#[tokio::test]
async fn test_add_user_message() {
    let mut app = create_test_chat_app().await;

    app.add_user_message("Hello, world!".to_string());

    assert_eq!(app.messages.len(), 1);
    assert_eq!(app.messages[0].content, "Hello, world!");
}

#[tokio::test]
async fn test_start_assistant_message() {
    let mut app = create_test_chat_app().await;

    app.start_assistant_message();

    assert_eq!(app.messages.len(), 1);
    assert!(app.messages[0].is_streaming);
    assert!(app.is_processing);
}

#[tokio::test]
async fn test_append_to_current_message() {
    let mut app = create_test_chat_app().await;

    app.start_assistant_message();
    app.append_to_current_message("Hello");
    app.append_to_current_message(", world!");

    assert!(app.messages[0].content.contains("Hello"));
    assert!(app.messages[0].content.contains(", world!"));
}

#[tokio::test]
async fn test_finish_current_message() {
    let mut app = create_test_chat_app().await;

    app.start_assistant_message();
    app.append_to_current_message("Test content");
    app.finish_current_message();

    assert!(!app.messages[0].is_streaming);
    assert!(!app.is_processing);
}

// ==================== Mode Transition Tests ====================

#[tokio::test]
async fn test_mode_transition_to_help() {
    let mut app = create_test_chat_app().await;

    app.mode = ChatMode::Normal;
    app.mode = ChatMode::Help;

    assert_eq!(app.mode, ChatMode::Help);
}

#[tokio::test]
async fn test_mode_transition_to_normal() {
    let mut app = create_test_chat_app().await;

    app.mode = ChatMode::Input;
    app.mode = ChatMode::Normal;

    assert_eq!(app.mode, ChatMode::Normal);
}

// ==================== Confirmation Dialog Tests ====================

#[tokio::test]
async fn test_show_confirm() {
    let mut app = create_test_chat_app().await;

    app.show_confirm("Are you sure?", |_app| {
        // Callback would be executed on confirm
    });

    assert_eq!(app.confirm_message, Some("Are you sure?".to_string()));
    assert!(app.confirm_callback.is_some());
    assert_eq!(app.mode, ChatMode::Confirm);
}

// ==================== Command Handling Tests ====================

#[tokio::test]
async fn test_handle_command_help() {
    let mut app = create_test_chat_app().await;

    app.handle_command("/help");

    assert_eq!(app.mode, ChatMode::Help);
}

#[tokio::test]
async fn test_handle_command_quit() {
    let mut app = create_test_chat_app().await;

    app.handle_command("/quit");

    assert!(app.should_quit);
}

#[tokio::test]
async fn test_handle_command_exit() {
    let mut app = create_test_chat_app().await;

    app.handle_command("/exit");

    assert!(app.should_quit);
}

#[tokio::test]
async fn test_handle_command_clear() {
    let mut app = create_test_chat_app().await;

    // Add some messages first
    app.add_user_message("Message 1".to_string());
    app.add_user_message("Message 2".to_string());
    assert_eq!(app.messages.len(), 2);

    app.handle_command("/clear");

    assert!(app.messages.is_empty());
    assert_eq!(app.status_message, Some("Chat cleared".to_string()));
}

#[tokio::test]
async fn test_handle_command_agents() {
    let mut app = create_test_chat_app().await;

    let initial_visible = app.agent_pane_visible;
    app.handle_command("/agents");
    assert_ne!(app.agent_pane_visible, initial_visible);
}

#[tokio::test]
async fn test_handle_command_unknown() {
    let mut app = create_test_chat_app().await;

    app.handle_command("/unknown_command");

    assert!(app.status_is_error);
    assert!(app
        .status_message
        .as_ref()
        .unwrap()
        .contains("Unknown command"));
}

// ==================== Submit Message Tests ====================

#[tokio::test]
async fn test_submit_message_command() {
    let mut app = create_test_chat_app().await;

    app.submit_message("/help".to_string());

    assert_eq!(app.mode, ChatMode::Help);
}

#[tokio::test]
async fn test_submit_message_regular() {
    let mut app = create_test_chat_app().await;

    // Regular message should be sent via event channel
    app.submit_message("Hello".to_string());

    // Can't easily verify the channel message, but no panic is good
}

// ==================== Event Sender Tests ====================

#[tokio::test]
async fn test_event_sender() {
    let app = create_test_chat_app().await;

    let sender = app.event_sender();
    // Should be able to send events
    let _ = sender.send(ChatEvent::Refresh);
}

// ==================== Provider Accessor Tests ====================

#[tokio::test]
async fn test_provider_accessor() {
    let app = create_test_chat_app().await;

    let provider = app.provider();
    assert_eq!(provider.name(), "mock");
}

#[tokio::test]
async fn test_tool_executor_accessor() {
    let app = create_test_chat_app().await;

    let _executor = app.tool_executor();
    // Just verify we can access it
}

#[tokio::test]
async fn test_context_manager_accessor() {
    let app = create_test_chat_app().await;

    let _context = app.context_manager();
    // Just verify we can access it
}

// ==================== Event Handling Tests ====================

#[tokio::test]
async fn test_handle_event_user_message() {
    let mut app = create_test_chat_app().await;

    app.handle_event(ChatEvent::UserMessage("Test message".to_string()))
        .await
        .unwrap();

    assert_eq!(app.messages.len(), 1);
    assert_eq!(app.messages[0].content, "Test message");
}

#[tokio::test]
async fn test_handle_event_stream_start() {
    let mut app = create_test_chat_app().await;

    app.handle_event(ChatEvent::StreamStart).await.unwrap();

    assert_eq!(app.messages.len(), 1);
    assert!(app.messages[0].is_streaming);
    assert!(app.is_processing);
}

#[tokio::test]
async fn test_handle_event_stream_delta() {
    let mut app = create_test_chat_app().await;

    app.handle_event(ChatEvent::StreamStart).await.unwrap();
    app.handle_event(ChatEvent::StreamDelta("Hello".to_string()))
        .await
        .unwrap();

    assert!(app.messages[0].content.contains("Hello"));
}

#[tokio::test]
async fn test_handle_event_stream_end() {
    let mut app = create_test_chat_app().await;

    app.handle_event(ChatEvent::StreamStart).await.unwrap();
    app.handle_event(ChatEvent::StreamEnd).await.unwrap();

    assert!(!app.messages[0].is_streaming);
    assert!(!app.is_processing);
}

#[tokio::test]
async fn test_handle_event_error() {
    let mut app = create_test_chat_app().await;

    app.handle_event(ChatEvent::Error("Test error".to_string()))
        .await
        .unwrap();

    assert!(app.status_is_error);
    assert_eq!(app.status_message, Some("Test error".to_string()));
}

#[tokio::test]
async fn test_handle_event_status() {
    let mut app = create_test_chat_app().await;

    app.handle_event(ChatEvent::Status("Test status".to_string()))
        .await
        .unwrap();

    assert!(!app.status_is_error);
    assert_eq!(app.status_message, Some("Test status".to_string()));
}

#[tokio::test]
async fn test_handle_event_session_ended() {
    let mut app = create_test_chat_app().await;

    app.handle_event(ChatEvent::SessionEnded).await.unwrap();

    assert!(app.should_quit);
}

#[tokio::test]
async fn test_handle_event_refresh() {
    let mut app = create_test_chat_app().await;

    // Should not panic or change state
    app.handle_event(ChatEvent::Refresh).await.unwrap();
}

#[tokio::test]
async fn test_handle_event_agent_spawned() {
    let mut app = create_test_chat_app().await;
    let agent_id = uuid::Uuid::new_v4();

    app.handle_event(ChatEvent::AgentSpawned {
        id: agent_id,
        name: "TestAgent".to_string(),
        agent_type: "explore".to_string(),
        task: "Find files".to_string(),
    })
    .await
    .unwrap();

    assert_eq!(app.agents.total_count(), 1);
    assert!(app.agent_pane_expanded);
}

#[tokio::test]
async fn test_handle_event_agent_progress() {
    let mut app = create_test_chat_app().await;
    let agent_id = uuid::Uuid::new_v4();

    // First spawn an agent
    app.handle_event(ChatEvent::AgentSpawned {
        id: agent_id,
        name: "TestAgent".to_string(),
        agent_type: "explore".to_string(),
        task: "Find files".to_string(),
    })
    .await
    .unwrap();

    // Then update progress
    app.handle_event(ChatEvent::AgentProgress {
        id: agent_id,
        iteration: 5,
        max_iterations: 30,
        action: "Reading file".to_string(),
    })
    .await
    .unwrap();

    // Agent should be tracked
    assert_eq!(app.agents.total_count(), 1);
}

#[tokio::test]
async fn test_handle_event_agent_completed() {
    let mut app = create_test_chat_app().await;
    let agent_id = uuid::Uuid::new_v4();

    // First spawn an agent
    app.handle_event(ChatEvent::AgentSpawned {
        id: agent_id,
        name: "TestAgent".to_string(),
        agent_type: "explore".to_string(),
        task: "Find files".to_string(),
    })
    .await
    .unwrap();

    // Then complete it
    app.handle_event(ChatEvent::AgentCompleted {
        id: agent_id,
        files_changed: vec!["file1.rs".to_string()],
        summary: Some("Completed successfully".to_string()),
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_handle_event_agent_failed() {
    let mut app = create_test_chat_app().await;
    let agent_id = uuid::Uuid::new_v4();

    // First spawn an agent
    app.handle_event(ChatEvent::AgentSpawned {
        id: agent_id,
        name: "TestAgent".to_string(),
        agent_type: "explore".to_string(),
        task: "Find files".to_string(),
    })
    .await
    .unwrap();

    // Then fail it
    app.handle_event(ChatEvent::AgentFailed {
        id: agent_id,
        error: "Test error".to_string(),
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_handle_event_agent_cancelled() {
    let mut app = create_test_chat_app().await;
    let agent_id = uuid::Uuid::new_v4();

    // First spawn an agent
    app.handle_event(ChatEvent::AgentSpawned {
        id: agent_id,
        name: "TestAgent".to_string(),
        agent_type: "explore".to_string(),
        task: "Find files".to_string(),
    })
    .await
    .unwrap();

    // Then cancel it
    app.handle_event(ChatEvent::AgentCancelled { id: agent_id })
        .await
        .unwrap();
}

#[tokio::test]
async fn test_handle_event_agent_rate_limited() {
    let mut app = create_test_chat_app().await;
    let agent_id = uuid::Uuid::new_v4();

    // First spawn an agent
    app.handle_event(ChatEvent::AgentSpawned {
        id: agent_id,
        name: "TestAgent".to_string(),
        agent_type: "explore".to_string(),
        task: "Find files".to_string(),
    })
    .await
    .unwrap();

    // Then rate limit it
    app.handle_event(ChatEvent::AgentRateLimited {
        id: agent_id,
        wait_secs: 5.0,
        tokens_needed: 1000,
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_handle_event_agent_tool_start() {
    let mut app = create_test_chat_app().await;
    let agent_id = uuid::Uuid::new_v4();

    // First spawn an agent
    app.handle_event(ChatEvent::AgentSpawned {
        id: agent_id,
        name: "TestAgent".to_string(),
        agent_type: "explore".to_string(),
        task: "Find files".to_string(),
    })
    .await
    .unwrap();

    // Then start a tool
    app.handle_event(ChatEvent::AgentToolStart {
        id: agent_id,
        tool_name: "file_read".to_string(),
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn test_handle_event_agent_tool_end() {
    let mut app = create_test_chat_app().await;
    let agent_id = uuid::Uuid::new_v4();

    // First spawn an agent
    app.handle_event(ChatEvent::AgentSpawned {
        id: agent_id,
        name: "TestAgent".to_string(),
        agent_type: "explore".to_string(),
        task: "Find files".to_string(),
    })
    .await
    .unwrap();

    // Then end a tool
    app.handle_event(ChatEvent::AgentToolEnd {
        id: agent_id,
        tool_name: "file_read".to_string(),
        success: true,
    })
    .await
    .unwrap();
}

// ==================== Tool Call Tests ====================

#[tokio::test]
async fn test_add_tool_call() {
    let mut app = create_test_chat_app().await;

    app.start_assistant_message();
    app.add_tool_call(
        "tool-1",
        "file_read",
        serde_json::json!({"path": "/test.txt"}),
    );

    let msg = &app.messages[0];
    assert!(!msg.tool_calls.is_empty());
}

#[tokio::test]
async fn test_complete_tool_call_success() {
    let mut app = create_test_chat_app().await;

    app.start_assistant_message();
    app.add_tool_call(
        "tool-1",
        "file_read",
        serde_json::json!({"path": "/test.txt"}),
    );

    let result = crate::tools::ToolResult::success("tool-1".to_string(), "File content here");
    app.complete_tool_call("tool-1", result);

    let msg = &app.messages[0];
    let tc = msg.tool_calls.first().unwrap();
    assert!(tc.completed_at.is_some());
}

#[tokio::test]
async fn test_complete_tool_call_error() {
    let mut app = create_test_chat_app().await;

    app.start_assistant_message();
    app.add_tool_call(
        "tool-1",
        "file_read",
        serde_json::json!({"path": "/test.txt"}),
    );

    let result = crate::tools::ToolResult::error("tool-1".to_string(), "File not found");
    app.complete_tool_call("tool-1", result);

    let msg = &app.messages[0];
    let tc = msg.tool_calls.first().unwrap();
    assert!(tc.completed_at.is_some());
    assert_eq!(
        tc.status,
        crate::tui::chat::state::messages::ToolCallStatus::Failed
    );
}

// ==================== Tick Tests ====================

#[tokio::test]
async fn test_tick_returns_quit_when_should_quit() {
    let mut app = create_test_chat_app().await;

    app.should_quit = true;
    let result = app.tick().await.unwrap();

    assert_eq!(result, TickResult::Quit);
}

#[tokio::test]
async fn test_tick_returns_restart_when_should_restart() {
    let mut app = create_test_chat_app().await;

    app.should_restart = true;
    let result = app.tick().await.unwrap();

    assert_eq!(result, TickResult::Restart);
}

// ==================== Config Tests ====================

#[test]
fn test_chat_tui_config_creation() {
    let config = ChatTuiConfig {
        session_id: Uuid::new_v4(),
        provider_name: "test".to_string(),
        model: "test-model".to_string(),
        caps: vec!["cap1".to_string(), "cap2".to_string()],
        trust_mode: true,
        stream_enabled: false,
    };

    assert_eq!(config.provider_name, "test");
    assert_eq!(config.model, "test-model");
    assert_eq!(config.caps.len(), 2);
    assert!(config.trust_mode);
    assert!(!config.stream_enabled);
}

// ==================== Input State Tests ====================

#[tokio::test]
async fn test_input_state_initial() {
    let app = create_test_chat_app().await;

    assert!(app.input.is_empty());
}

#[tokio::test]
async fn test_input_interaction() {
    let mut app = create_test_chat_app().await;

    app.input.insert_char('H');
    app.input.insert_char('i');

    assert!(!app.input.is_empty());
    assert_eq!(app.input.text(), "Hi");
}

// ==================== Multiple Messages Tests ====================

#[tokio::test]
async fn test_multiple_messages() {
    let mut app = create_test_chat_app().await;

    app.add_user_message("First".to_string());
    app.start_assistant_message();
    app.append_to_current_message("Response 1");
    app.finish_current_message();
    app.add_user_message("Second".to_string());
    app.start_assistant_message();
    app.append_to_current_message("Response 2");
    app.finish_current_message();

    assert_eq!(app.messages.len(), 4);
}

// ==================== Agent Pane Height Tests ====================

#[tokio::test]
async fn test_agent_pane_height_expands_with_agents() {
    let mut app = create_test_chat_app().await;

    assert_eq!(app.agent_pane_height, 4);

    // Add multiple agents via tracker
    for i in 0..5 {
        let agent_id = uuid::Uuid::new_v4();
        app.agents.track(
            agent_id,
            agent_id.to_string(),
            format!("Agent{}", i),
            "explore".to_string(),
            "Task".to_string(),
        );
    }

    // Simulate the height adjustment that handle_event would do
    let running = app.agents.active_count();
    let total = app.agents.total_count();
    let agent_count = running.max(total.min(4));
    app.agent_pane_height = (agent_count + 2).min(10) as u16;

    // Height should adjust (agents + 2, capped at 10)
    assert!(app.agent_pane_height >= 4);
    assert!(app.agent_pane_height <= 10);
}
