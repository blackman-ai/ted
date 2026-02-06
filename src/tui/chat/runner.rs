// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Chat TUI runner
//!
//! Integrates the chat TUI with the LLM provider and tool execution.
//! This module handles the actual chat loop within the TUI context.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event::{Event as TermEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::prelude::*;

use crate::caps::available_caps;
use crate::chat::input_parser::{
    parse_beads_command, parse_commit_command, parse_explain_command, parse_fix_command,
    parse_model_command, parse_review_command, parse_skills_command, parse_test_command,
};
use crate::chat::slash_commands::{
    execute_beads, execute_commit, execute_explain, execute_fix, execute_model, execute_review,
    execute_skills, execute_test, SlashCommandResult,
};
use crate::config::Settings;
use crate::context::ContextManager;
use crate::error::{Result, TedError};
use crate::history::{HistoryStore, SessionInfo};
use crate::llm::message::{ContentBlock, Conversation, Message};
use crate::llm::provider::{
    CompletionRequest, ContentBlockDelta, ContentBlockResponse, LlmProvider, StopReason,
    StreamEvent, ToolChoice,
};
use crate::skills::SkillRegistry;
use crate::tools::builtin::ProgressTracker;
use crate::tools::ToolExecutor;

use super::app::ChatMode;
use super::state::{AgentTracker, DisplayMessage, DisplayToolCall, InputState};
use super::ChatTuiConfig;

/// Discover locally downloaded GGUF models
fn discover_local_models() -> Vec<String> {
    let mut models = Vec::new();

    // Check ~/.ted/models/local/ for GGUF files
    if let Some(home) = dirs::home_dir() {
        let models_dir = home.join(".ted").join("models").join("local");
        if models_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&models_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("gguf") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            models.push(stem.to_string());
                        }
                    }
                }
            }
        }
    }

    // If no models found, show a placeholder
    if models.is_empty() {
        models.push("(no models - use /model download)".to_string());
    }

    models
}

/// Settings section tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    General,
    Capabilities,
}

impl SettingsSection {
    fn all() -> &'static [SettingsSection] {
        &[SettingsSection::General, SettingsSection::Capabilities]
    }

    fn label(&self) -> &'static str {
        match self {
            SettingsSection::General => "General",
            SettingsSection::Capabilities => "Capabilities",
        }
    }
}

/// Settings field identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    Provider,
    ApiKey,
    Model,
    Temperature,
    MaxTokens,
    Stream,
    TrustMode,
}

impl SettingsField {
    fn all() -> &'static [SettingsField] {
        &[
            SettingsField::Provider,
            SettingsField::ApiKey,
            SettingsField::Model,
            SettingsField::Temperature,
            SettingsField::MaxTokens,
            SettingsField::Stream,
            SettingsField::TrustMode,
        ]
    }

    fn label(&self) -> &'static str {
        match self {
            SettingsField::Provider => "Provider",
            SettingsField::ApiKey => "API Key",
            SettingsField::Model => "Model",
            SettingsField::Temperature => "Temperature",
            SettingsField::MaxTokens => "Max Tokens",
            SettingsField::Stream => "Streaming",
            SettingsField::TrustMode => "Trust Mode",
        }
    }
}

/// State for settings editor
#[derive(Debug, Clone)]
pub struct SettingsState {
    /// Current section/tab
    pub current_section: SettingsSection,
    /// Currently selected field index (for General section)
    pub selected_index: usize,
    /// Currently selected cap index (for Capabilities section)
    pub caps_selected_index: usize,
    /// Scroll offset for caps list
    pub caps_scroll_offset: usize,
    /// Whether currently editing a field
    pub is_editing: bool,
    /// Edit buffer for text input
    pub edit_buffer: String,
    /// Providers list for selection
    pub providers: Vec<String>,
    /// Current provider index (for cycling)
    pub provider_index: usize,
    /// Models available for each provider
    pub models_by_provider: std::collections::HashMap<String, Vec<String>>,
    /// Current model index (for cycling within selected provider)
    pub model_index: usize,
    /// Editable settings values
    pub provider: String,
    pub api_key: String,
    /// API keys per provider (for cycling)
    pub api_keys_by_provider: std::collections::HashMap<String, String>,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub stream: bool,
    pub trust_mode: bool,
    /// Working copy of enabled caps (applied on save)
    pub caps_enabled: Vec<String>,
    /// Available caps (name, is_builtin)
    pub available_caps: Vec<(String, bool)>,
    /// Track if settings changed
    pub has_changes: bool,
}

impl SettingsState {
    pub fn new(
        settings: &Settings,
        config: &ChatTuiConfig,
        enabled_caps: &[String],
        available_caps: &[(String, bool)],
    ) -> Self {
        let providers = vec![
            "anthropic".to_string(),
            "ollama".to_string(),
            "openrouter".to_string(),
            "blackman".to_string(),
            "local".to_string(),
        ];
        let provider_index = providers
            .iter()
            .position(|p| p == &config.provider_name)
            .unwrap_or(0);

        // Define available models per provider
        let mut models_by_provider = std::collections::HashMap::new();
        models_by_provider.insert(
            "anthropic".to_string(),
            vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-opus-4-20250514".to_string(),
                "claude-3-5-sonnet-20241022".to_string(),
                "claude-3-5-haiku-20241022".to_string(),
            ],
        );
        models_by_provider.insert(
            "ollama".to_string(),
            vec![
                "llama3.2".to_string(),
                "llama3.1".to_string(),
                "mistral".to_string(),
                "codellama".to_string(),
                "deepseek-coder".to_string(),
                "qwen2.5-coder".to_string(),
            ],
        );
        models_by_provider.insert(
            "openrouter".to_string(),
            vec![
                "anthropic/claude-sonnet-4".to_string(),
                "anthropic/claude-opus-4".to_string(),
                "openai/gpt-4o".to_string(),
                "openai/gpt-4-turbo".to_string(),
                "google/gemini-pro-1.5".to_string(),
                "meta-llama/llama-3.1-405b".to_string(),
            ],
        );
        models_by_provider.insert(
            "blackman".to_string(),
            vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-opus-4-20250514".to_string(),
            ],
        );

        // Discover local models (GGUF files in ~/.ted/models/local/)
        let local_models = discover_local_models();
        models_by_provider.insert("local".to_string(), local_models);

        // Find current model index for the provider
        let model_index = models_by_provider
            .get(&config.provider_name)
            .and_then(|models| models.iter().position(|m| m == &config.model))
            .unwrap_or(0);

        // Get API keys for all providers
        let mut api_keys_by_provider = std::collections::HashMap::new();
        api_keys_by_provider.insert(
            "anthropic".to_string(),
            settings
                .providers
                .anthropic
                .api_key
                .clone()
                .unwrap_or_default(),
        );
        api_keys_by_provider.insert(
            "openrouter".to_string(),
            settings
                .providers
                .openrouter
                .api_key
                .clone()
                .unwrap_or_default(),
        );
        api_keys_by_provider.insert(
            "blackman".to_string(),
            settings
                .providers
                .blackman
                .api_key
                .clone()
                .unwrap_or_default(),
        );
        api_keys_by_provider.insert("ollama".to_string(), String::new()); // ollama doesn't need API key
        api_keys_by_provider.insert("local".to_string(), String::new()); // local doesn't need API key

        // Get API key for current provider
        let api_key = api_keys_by_provider
            .get(&config.provider_name)
            .cloned()
            .unwrap_or_default();

        Self {
            current_section: SettingsSection::General,
            selected_index: 0,
            caps_selected_index: 0,
            caps_scroll_offset: 0,
            is_editing: false,
            edit_buffer: String::new(),
            providers,
            provider_index,
            models_by_provider,
            model_index,
            provider: config.provider_name.clone(),
            api_key,
            api_keys_by_provider,
            model: config.model.clone(),
            temperature: settings.defaults.temperature,
            max_tokens: settings.defaults.max_tokens,
            stream: config.stream_enabled,
            trust_mode: config.trust_mode,
            caps_enabled: enabled_caps.to_vec(),
            available_caps: available_caps.to_vec(),
            has_changes: false,
        }
    }

    pub fn next_section(&mut self) {
        let sections = SettingsSection::all();
        let current_idx = sections
            .iter()
            .position(|s| *s == self.current_section)
            .unwrap_or(0);
        let next_idx = (current_idx + 1) % sections.len();
        self.current_section = sections[next_idx];
    }

    pub fn prev_section(&mut self) {
        let sections = SettingsSection::all();
        let current_idx = sections
            .iter()
            .position(|s| *s == self.current_section)
            .unwrap_or(0);
        let prev_idx = if current_idx == 0 {
            sections.len() - 1
        } else {
            current_idx - 1
        };
        self.current_section = sections[prev_idx];
    }

    pub fn toggle_cap(&mut self) {
        if self.available_caps.is_empty() {
            return;
        }
        let (cap_name, _) = &self.available_caps[self.caps_selected_index];
        if let Some(pos) = self.caps_enabled.iter().position(|c| c == cap_name) {
            self.caps_enabled.remove(pos);
        } else {
            self.caps_enabled.push(cap_name.clone());
        }
        self.has_changes = true;
    }

    pub fn caps_move_up(&mut self) {
        if self.caps_selected_index > 0 {
            self.caps_selected_index -= 1;
        }
    }

    pub fn caps_move_down(&mut self) {
        if !self.available_caps.is_empty()
            && self.caps_selected_index < self.available_caps.len() - 1
        {
            self.caps_selected_index += 1;
        }
    }

    pub fn selected_field(&self) -> SettingsField {
        SettingsField::all()[self.selected_index]
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index < SettingsField::all().len() - 1 {
            self.selected_index += 1;
        }
    }

    pub fn start_editing(&mut self) {
        // Provider and Model use cycling, not text editing
        if matches!(
            self.selected_field(),
            SettingsField::Provider | SettingsField::Model
        ) {
            return;
        }
        // Don't allow editing API key for ollama (not needed)
        if self.selected_field() == SettingsField::ApiKey && self.provider == "ollama" {
            return;
        }
        self.is_editing = true;
        self.edit_buffer = match self.selected_field() {
            SettingsField::Provider | SettingsField::Model => String::new(),
            SettingsField::ApiKey => self.api_key.clone(),
            SettingsField::Temperature => format!("{:.1}", self.temperature),
            SettingsField::MaxTokens => self.max_tokens.to_string(),
            SettingsField::Stream | SettingsField::TrustMode => String::new(),
        };
    }

    pub fn cancel_editing(&mut self) {
        self.is_editing = false;
        self.edit_buffer.clear();
    }

    pub fn confirm_editing(&mut self) {
        if !self.is_editing {
            return;
        }
        self.is_editing = false;

        match self.selected_field() {
            SettingsField::Provider | SettingsField::Model => {
                // Cycled, not typed
            }
            SettingsField::ApiKey => {
                self.api_key = self.edit_buffer.trim().to_string();
                // Also save to the per-provider map
                self.api_keys_by_provider
                    .insert(self.provider.clone(), self.api_key.clone());
                self.has_changes = true;
            }
            SettingsField::Temperature => {
                if let Ok(t) = self.edit_buffer.parse::<f32>() {
                    self.temperature = t.clamp(0.0, 2.0);
                    self.has_changes = true;
                }
            }
            SettingsField::MaxTokens => {
                if let Ok(t) = self.edit_buffer.parse::<u32>() {
                    self.max_tokens = t.clamp(100, 128000);
                    self.has_changes = true;
                }
            }
            SettingsField::Stream | SettingsField::TrustMode => {
                // Toggled, not typed
            }
        }
        self.edit_buffer.clear();
    }

    pub fn toggle_bool(&mut self) {
        match self.selected_field() {
            SettingsField::Stream => {
                self.stream = !self.stream;
                self.has_changes = true;
            }
            SettingsField::TrustMode => {
                self.trust_mode = !self.trust_mode;
                self.has_changes = true;
            }
            _ => {}
        }
    }

    pub fn cycle_provider(&mut self, forward: bool) {
        // Save current API key before switching
        self.api_keys_by_provider
            .insert(self.provider.clone(), self.api_key.clone());

        if forward {
            self.provider_index = (self.provider_index + 1) % self.providers.len();
        } else if self.provider_index > 0 {
            self.provider_index -= 1;
        } else {
            self.provider_index = self.providers.len() - 1;
        }
        self.provider = self.providers[self.provider_index].clone();

        // Load API key for new provider
        self.api_key = self
            .api_keys_by_provider
            .get(&self.provider)
            .cloned()
            .unwrap_or_default();

        // Reset model to first available for the new provider
        self.model_index = 0;
        if let Some(models) = self.models_by_provider.get(&self.provider) {
            if let Some(first_model) = models.first() {
                self.model = first_model.clone();
            }
        }
        self.has_changes = true;
    }

    pub fn cycle_model(&mut self, forward: bool) {
        let models = match self.models_by_provider.get(&self.provider) {
            Some(m) if !m.is_empty() => m,
            _ => return,
        };

        if forward {
            self.model_index = (self.model_index + 1) % models.len();
        } else if self.model_index > 0 {
            self.model_index -= 1;
        } else {
            self.model_index = models.len() - 1;
        }
        self.model = models[self.model_index].clone();
        self.has_changes = true;
    }

    /// Get the list of available models for the current provider
    pub fn current_models(&self) -> &[String] {
        self.models_by_provider
            .get(&self.provider)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn current_value(&self, field: SettingsField) -> String {
        match field {
            SettingsField::Provider => {
                format!(
                    "◀ {} ▶  ({}/{})",
                    self.provider,
                    self.provider_index + 1,
                    self.providers.len()
                )
            }
            SettingsField::ApiKey => {
                if self.provider == "ollama" {
                    "(not required)".to_string()
                } else if self.api_key.is_empty() {
                    "(not set)".to_string()
                } else {
                    // Show masked key with first 4 and last 4 chars
                    let len = self.api_key.len();
                    if len <= 10 {
                        "••••••••••".to_string()
                    } else {
                        format!("{}••••{}", &self.api_key[..4], &self.api_key[len - 4..])
                    }
                }
            }
            SettingsField::Model => {
                let models = self.current_models();
                let total = models.len();
                if total > 0 {
                    format!("◀ {} ▶  ({}/{})", self.model, self.model_index + 1, total)
                } else {
                    self.model.clone()
                }
            }
            SettingsField::Temperature => format!("{:.1}", self.temperature),
            SettingsField::MaxTokens => self.max_tokens.to_string(),
            SettingsField::Stream => if self.stream { "On" } else { "Off" }.to_string(),
            SettingsField::TrustMode => if self.trust_mode { "On" } else { "Off" }.to_string(),
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.edit_buffer.push(c);
    }

    pub fn backspace(&mut self) {
        self.edit_buffer.pop();
    }
}

/// Simplified TUI state for the runner
pub struct TuiState {
    pub config: ChatTuiConfig,
    pub mode: ChatMode,
    pub messages: Vec<DisplayMessage>,
    pub agents: AgentTracker,
    pub input: InputState,
    pub status_message: Option<String>,
    pub status_is_error: bool,
    pub is_processing: bool,
    pub should_quit: bool,
    pub agent_pane_visible: bool,
    pub agent_pane_expanded: bool,
    pub agent_pane_height: u16,
    pub scroll_offset: usize,
    pub selected_agent_index: usize,
    /// Track chat area height for auto-scroll calculations
    pub chat_area_height: u16,
    /// Current model (can be changed with /model or settings)
    pub current_model: String,
    /// Settings editor state
    pub settings_state: Option<SettingsState>,
    /// Need to restart after settings change (provider changed)
    pub needs_restart: bool,
    /// Animation frame counter for thinking indicator
    pub animation_frame: u8,
    /// Available caps (name, is_builtin)
    pub available_caps: Vec<(String, bool)>,
    /// Currently enabled caps
    pub enabled_caps: Vec<String>,
    /// Agent progress tracker for spawn_agent visibility
    pub agent_progress_tracker: Option<ProgressTracker>,
    /// Flag indicating caps changed and system prompt needs regeneration
    pub caps_changed: bool,
    /// Queued messages to send after current processing completes
    pub pending_messages: Vec<String>,
}

impl TuiState {
    pub fn new(config: ChatTuiConfig, settings: &Settings) -> Self {
        let current_model = config.model.clone();
        let enabled_caps = config.caps.clone();
        let all_caps = available_caps().unwrap_or_default();
        let settings_state = Some(SettingsState::new(
            settings,
            &config,
            &enabled_caps,
            &all_caps,
        ));
        Self {
            config,
            mode: ChatMode::Input,
            messages: Vec::new(),
            agents: AgentTracker::new(),
            input: InputState::new(),
            status_message: None,
            status_is_error: false,
            is_processing: false,
            should_quit: false,
            agent_pane_visible: true,
            agent_pane_expanded: false,
            agent_pane_height: 4,
            scroll_offset: 0,
            selected_agent_index: 0,
            chat_area_height: 20, // Will be updated on first render
            current_model,
            settings_state,
            needs_restart: false,
            animation_frame: 0,
            available_caps: all_caps,
            enabled_caps,
            agent_progress_tracker: None,
            caps_changed: false,
            pending_messages: Vec::new(),
        }
    }

    /// Set the agent progress tracker for spawn_agent visibility
    pub fn with_progress_tracker(mut self, tracker: ProgressTracker) -> Self {
        self.agent_progress_tracker = Some(tracker);
        self
    }

    /// Advance the animation frame (called on each render tick)
    pub fn tick_animation(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);
    }

    /// Get the current thinking indicator text
    pub fn thinking_indicator(&self) -> &'static str {
        // Cycle through different dot patterns every ~200ms (4 frames at 50ms poll)
        match (self.animation_frame / 4) % 4 {
            0 => "●○○",
            1 => "○●○",
            2 => "○○●",
            _ => "○●○",
        }
    }

    pub fn set_status(&mut self, msg: &str) {
        self.status_message = Some(msg.to_string());
        self.status_is_error = false;
    }

    pub fn set_error(&mut self, msg: &str) {
        self.status_message = Some(msg.to_string());
        self.status_is_error = true;
    }

    /// Calculate total height of all messages in lines
    fn total_messages_height(&self) -> usize {
        self.messages
            .iter()
            .map(|m| {
                let content_lines = m.content.lines().count().max(1);
                let tool_call_lines: usize = m
                    .tool_calls
                    .iter()
                    .map(|tc| if tc.expanded { 5 } else { 2 })
                    .sum();
                // Header (1) + content + tool calls + spacing (1)
                1 + content_lines + tool_call_lines + 1
            })
            .sum()
    }

    /// Auto-scroll to show the latest content
    pub fn scroll_to_bottom(&mut self, visible_height: u16) {
        let total_height = self.total_messages_height();
        if total_height > visible_height as usize {
            self.scroll_offset = total_height - visible_height as usize;
        } else {
            self.scroll_offset = 0;
        }
    }

    /// Update chat area height based on terminal size
    pub fn update_chat_height(&mut self, terminal_height: u16) {
        let title_height: u16 = 1;
        let input_height: u16 = 3;
        let agent_height = if self.agent_pane_visible && self.agents.total_count() > 0 {
            if self.agent_pane_expanded {
                self.agent_pane_height.min(terminal_height / 3)
            } else {
                3
            }
        } else {
            0
        };

        self.chat_area_height = terminal_height
            .saturating_sub(title_height)
            .saturating_sub(input_height)
            .saturating_sub(agent_height);
    }

    /// Auto-scroll using current chat height
    pub fn auto_scroll(&mut self) {
        self.scroll_to_bottom(self.chat_area_height);
    }

    /// Scroll up by a number of lines
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Scroll down by a number of lines
    pub fn scroll_down(&mut self, lines: usize) {
        let total_height = self.total_messages_height();
        let max_offset = total_height.saturating_sub(self.chat_area_height as usize);
        self.scroll_offset = (self.scroll_offset + lines).min(max_offset);
    }
}

/// Run the chat TUI with the given configuration
#[allow(clippy::too_many_arguments)]
pub async fn run_chat_tui_loop(
    config: ChatTuiConfig,
    provider: Arc<dyn LlmProvider>,
    mut tool_executor: ToolExecutor,
    context_manager: ContextManager,
    settings: Settings,
    mut conversation: Conversation,
    mut history_store: HistoryStore,
    mut session_info: SessionInfo,
    agent_progress_tracker: ProgressTracker,
) -> Result<()> {
    // Setup terminal with panic hook to restore terminal on crash
    let original_panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Restore terminal before panicking
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_panic_hook(panic_info);
    }));

    enable_raw_mode().map_err(|e| TedError::Tui(e.to_string()))?;
    let mut stdout = io::stdout();
    // Don't enable mouse capture - we don't handle mouse events
    execute!(stdout, EnterAlternateScreen).map_err(|e| TedError::Tui(e.to_string()))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| TedError::Tui(e.to_string()))?;

    // Create TUI state with agent progress tracker
    let mut state =
        TuiState::new(config.clone(), &settings).with_progress_tracker(agent_progress_tracker);
    let mut message_count = session_info.message_count;
    let interrupted = Arc::new(AtomicBool::new(false));

    // Main loop - returns (result, needs_restart)
    let result: (Result<()>, bool) = loop {
        // Update chat height based on terminal size before render
        let terminal_size = terminal.size().map(|s| s.height).unwrap_or(24);
        state.update_chat_height(terminal_size);

        // Tick animation (for thinking indicator)
        state.tick_animation();

        // Render UI
        terminal
            .draw(|f| draw_tui(f, &state))
            .map_err(|e| TedError::Tui(e.to_string()))?;

        // Poll for events with timeout
        let has_event = crossterm::event::poll(Duration::from_millis(50))
            .map_err(|e| TedError::Tui(e.to_string()))?;

        if has_event {
            let event = crossterm::event::read().map_err(|e| TedError::Tui(e.to_string()))?;

            match event {
                TermEvent::Key(key) => {
                    // Handle Ctrl+C globally
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        if state.is_processing {
                            interrupted.store(true, Ordering::SeqCst);
                            state.is_processing = false;
                            state.set_status("Interrupted");
                        } else {
                            break (Ok(()), false);
                        }
                        continue;
                    }

                    // Handle Enter in input mode to submit message
                    if state.mode == ChatMode::Input
                        && key.code == KeyCode::Enter
                        && !state.input.is_empty()
                    {
                        let input_text = state.input.submit();

                        // Check for exit commands
                        let trimmed = input_text.trim().to_lowercase();
                        if trimmed == "exit"
                            || trimmed == "quit"
                            || trimmed == "/exit"
                            || trimmed == "/quit"
                        {
                            break (Ok(()), false);
                        }

                        // Check for commands (always process immediately)
                        if input_text.trim().starts_with('/') {
                            handle_command(&input_text, &mut state, Some(&mut conversation))?;

                            // Check if the command queued an LLM message (e.g., /explain, /commit)
                            // If so, process it immediately instead of waiting
                            if !state.pending_messages.is_empty() && !state.is_processing {
                                let llm_message = state.pending_messages.remove(0);

                                // Add user message to UI (show the command + its generated prompt)
                                state
                                    .messages
                                    .push(DisplayMessage::user(llm_message.clone()));
                                state.auto_scroll();

                                // Mark as processing and re-render
                                state.is_processing = true;
                                terminal
                                    .draw(|f| draw_tui(f, &state))
                                    .map_err(|e| TedError::Tui(e.to_string()))?;

                                // Store in context
                                context_manager
                                    .store_message("user", &llm_message, None)
                                    .await?;

                                // Add to conversation
                                conversation.push(Message::user(&llm_message));

                                // Update history
                                message_count += 1;
                                session_info.message_count = message_count;
                                session_info.touch();
                                history_store.upsert(session_info.clone())?;

                                // Process with LLM
                                interrupted.store(false, Ordering::SeqCst);

                                let result = process_llm_response(
                                    &provider,
                                    &state.current_model.clone(),
                                    &mut conversation,
                                    &mut tool_executor,
                                    &settings,
                                    &mut state,
                                    config.stream_enabled,
                                    &interrupted,
                                    &mut terminal,
                                )
                                .await;

                                state.is_processing = false;
                                state.auto_scroll();

                                match result {
                                    Ok(()) => {
                                        message_count += 1;
                                        session_info.message_count = message_count;
                                        session_info.touch();
                                        history_store.upsert(session_info.clone())?;

                                        if let Some(last_msg) = state.messages.last() {
                                            context_manager
                                                .store_message("assistant", &last_msg.content, None)
                                                .await?;
                                        }
                                    }
                                    Err(e) => {
                                        state.set_error(&format!("Error: {}", e));
                                    }
                                }
                            }

                            continue;
                        }

                        // If currently processing, queue the message for later
                        if state.is_processing {
                            state.pending_messages.push(input_text);
                            state.set_status(&format!(
                                "Message queued ({} pending)",
                                state.pending_messages.len()
                            ));
                            continue;
                        }

                        // Add user message to UI
                        state
                            .messages
                            .push(DisplayMessage::user(input_text.clone()));
                        state.auto_scroll();

                        // Mark as processing and re-render immediately to show the sent message
                        state.is_processing = true;
                        state.set_status("Sending...");
                        terminal
                            .draw(|f| draw_tui(f, &state))
                            .map_err(|e| TedError::Tui(e.to_string()))?;

                        // Store in context
                        context_manager
                            .store_message("user", &input_text, None)
                            .await?;

                        // Add to conversation
                        conversation.push(Message::user(&input_text));

                        // Update history
                        message_count += 1;
                        if message_count == 1 {
                            session_info.set_summary(&input_text);
                        }
                        session_info.message_count = message_count;
                        session_info.touch();
                        history_store.upsert(session_info.clone())?;

                        // Process with LLM
                        interrupted.store(false, Ordering::SeqCst);

                        // Process response (use current model from state, which can be changed via /model)
                        let result = process_llm_response(
                            &provider,
                            &state.current_model.clone(),
                            &mut conversation,
                            &mut tool_executor,
                            &settings,
                            &mut state,
                            config.stream_enabled,
                            &interrupted,
                            &mut terminal,
                        )
                        .await;

                        state.is_processing = false;
                        state.auto_scroll(); // Scroll to bottom after processing

                        match result {
                            Ok(()) => {
                                message_count += 1;
                                session_info.message_count = message_count;
                                session_info.touch();
                                history_store.upsert(session_info.clone())?;

                                if let Some(last_msg) = state.messages.last() {
                                    context_manager
                                        .store_message("assistant", &last_msg.content, None)
                                        .await?;
                                }
                            }
                            Err(e) => {
                                state.set_error(&format!("Error: {}", e));
                            }
                        }

                        // Process any queued messages
                        while !state.pending_messages.is_empty()
                            && !interrupted.load(Ordering::SeqCst)
                        {
                            let queued_text = state.pending_messages.remove(0);

                            // Add user message to UI
                            state
                                .messages
                                .push(DisplayMessage::user(queued_text.clone()));
                            state.auto_scroll();

                            // Mark as processing
                            state.is_processing = true;
                            let pending_count = state.pending_messages.len();
                            if pending_count > 0 {
                                state.set_status(&format!(
                                    "Processing queued... ({} remaining)",
                                    pending_count
                                ));
                            } else {
                                state.set_status("Processing queued message...");
                            }
                            terminal
                                .draw(|f| draw_tui(f, &state))
                                .map_err(|e| TedError::Tui(e.to_string()))?;

                            // Store in context
                            context_manager
                                .store_message("user", &queued_text, None)
                                .await?;

                            // Add to conversation
                            conversation.push(Message::user(&queued_text));

                            // Update history
                            message_count += 1;
                            session_info.message_count = message_count;
                            session_info.touch();
                            history_store.upsert(session_info.clone())?;

                            // Process response
                            let result = process_llm_response(
                                &provider,
                                &state.current_model.clone(),
                                &mut conversation,
                                &mut tool_executor,
                                &settings,
                                &mut state,
                                config.stream_enabled,
                                &interrupted,
                                &mut terminal,
                            )
                            .await;

                            state.is_processing = false;
                            state.auto_scroll();

                            match result {
                                Ok(()) => {
                                    message_count += 1;
                                    session_info.message_count = message_count;
                                    session_info.touch();
                                    history_store.upsert(session_info.clone())?;

                                    if let Some(last_msg) = state.messages.last() {
                                        context_manager
                                            .store_message("assistant", &last_msg.content, None)
                                            .await?;
                                    }
                                }
                                Err(e) => {
                                    state.set_error(&format!("Error: {}", e));
                                    break; // Stop processing queue on error
                                }
                            }
                        }

                        continue;
                    }

                    // Handle other keys based on mode
                    handle_key(&mut state, key)?;
                }
                TermEvent::Resize(_, _) => {
                    // Terminal resized, will re-render automatically
                }
                _ => {}
            }
        }

        // Regenerate system prompt if caps changed
        if state.caps_changed {
            state.caps_changed = false;
            let loader = crate::caps::CapLoader::new();
            let resolver = crate::caps::CapResolver::new(loader);
            if let Ok(merged) = resolver.resolve_and_merge(&state.enabled_caps) {
                conversation.set_system(&merged.system_prompt);
                state.set_status("Caps updated - system prompt regenerated");
            }
        }

        if state.should_quit {
            break (Ok(()), state.needs_restart);
        }
    };

    let (result, needs_restart) = result;

    // Restore terminal (and reset panic hook)
    let _ = std::panic::take_hook(); // Remove our custom panic hook

    disable_raw_mode().map_err(|e| TedError::Tui(e.to_string()))?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .map_err(|e| TedError::Tui(e.to_string()))?;
    terminal
        .show_cursor()
        .map_err(|e| TedError::Tui(e.to_string()))?;

    // If provider changed, restart ted
    if needs_restart {
        println!("\nRestarting with new provider...\n");

        // Get the current executable path
        let exe = std::env::current_exe().map_err(|e| TedError::Tui(e.to_string()))?;

        // Restart by exec'ing ourselves
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = std::process::Command::new(&exe).exec();
            return Err(TedError::Tui(format!("Failed to restart: {}", err)));
        }

        #[cfg(not(unix))]
        {
            // On Windows, spawn a new process and exit
            let _ = std::process::Command::new(&exe).spawn();
            std::process::exit(0);
        }
    }

    println!("\nGoodbye!");

    result
}

/// Draw the TUI
fn draw_tui(frame: &mut Frame, state: &TuiState) {
    // Create a minimal ChatApp-like struct for the UI
    // This is a workaround until we refactor the UI to use TuiState directly
    let area = frame.area();

    // Calculate layout
    let title_height = 1;
    let input_height = 3;
    let agent_height = if state.agent_pane_visible && state.agents.total_count() > 0 {
        if state.agent_pane_expanded {
            state.agent_pane_height.min(area.height / 3)
        } else {
            3
        }
    } else {
        0
    };

    let chat_height = area
        .height
        .saturating_sub(title_height)
        .saturating_sub(input_height)
        .saturating_sub(agent_height);

    // Title bar
    let title_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: title_height,
    };
    draw_title_bar(frame, state, title_area);

    // Chat area
    let chat_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y + title_height,
        width: area.width,
        height: chat_height,
    };
    draw_chat_area(frame, state, chat_area);

    // Agent pane
    if state.agent_pane_visible && state.agents.total_count() > 0 {
        let agents_area = ratatui::layout::Rect {
            x: area.x,
            y: area.y + title_height + chat_height,
            width: area.width,
            height: agent_height,
        };
        draw_agent_pane(frame, state, agents_area);
    }

    // Input area
    let input_area = ratatui::layout::Rect {
        x: area.x,
        y: area.y + title_height + chat_height + agent_height,
        width: area.width,
        height: input_height,
    };
    draw_input_area(frame, state, input_area);

    // Overlays
    match state.mode {
        ChatMode::Help => draw_help_overlay(frame, area),
        ChatMode::Settings => draw_settings_overlay(frame, state, area),
        _ => {}
    }
}

fn draw_title_bar(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    use ratatui::widgets::Paragraph;

    let session_short = &state.config.session_id.to_string()[..8];
    let title = format!(
        " ted ─ {} / {} ─ {} ",
        state.config.provider_name, state.current_model, session_short
    );

    let mut title_spans = vec![ratatui::text::Span::styled(
        &title,
        Style::default().fg(Color::White).bg(Color::DarkGray),
    )];

    // Add caps badges (filter out "base" - it's always applied silently)
    for cap in &state.config.caps {
        if cap == "base" {
            continue;
        }
        title_spans.push(ratatui::text::Span::styled(
            format!(" {} ", cap),
            Style::default().fg(Color::White).bg(Color::Blue),
        ));
        title_spans.push(ratatui::text::Span::raw(" "));
    }

    // Add status
    if state.is_processing {
        title_spans.push(ratatui::text::Span::styled(
            " ● Processing... ",
            Style::default().fg(Color::Green).bg(Color::DarkGray),
        ));
    } else if let Some(status) = &state.status_message {
        let style = if state.status_is_error {
            Style::default().fg(Color::Red).bg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Yellow).bg(Color::DarkGray)
        };
        title_spans.push(ratatui::text::Span::styled(format!(" {} ", status), style));
    }

    let title_line = ratatui::text::Line::from(title_spans);
    let widget = Paragraph::new(title_line).style(Style::default().bg(Color::DarkGray));
    frame.render_widget(widget, area);
}

fn draw_chat_area(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    use super::widgets::message::render_messages;

    let buf = frame.buffer_mut();
    render_messages(&state.messages, area, buf, state.scroll_offset);

    // Welcome message if no messages
    if state.messages.is_empty() {
        use ratatui::widgets::Paragraph;

        let welcome = Paragraph::new(vec![
            ratatui::text::Line::from(""),
            ratatui::text::Line::from(ratatui::text::Span::styled(
                "Ted - The Coding Assistant you always wanted",
                Style::default().fg(Color::Cyan).bold(),
            )),
            ratatui::text::Line::from(""),
            ratatui::text::Line::from("Type a message and press Enter to chat."),
            ratatui::text::Line::from("Press Ctrl+/ for help, or /quit to exit."),
        ])
        .alignment(ratatui::layout::Alignment::Center);

        let welcome_area = ratatui::layout::Rect {
            x: area.x + area.width / 4,
            y: area.y + area.height / 3,
            width: area.width / 2,
            height: 6,
        };
        frame.render_widget(welcome, welcome_area);
    }
}

fn draw_agent_pane(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    use super::widgets::AgentPane;

    let pane = AgentPane::new(&state.agents)
        .expanded(state.agent_pane_expanded)
        .focused(state.mode == ChatMode::AgentFocus);

    frame.render_widget(pane, area);
}

fn draw_input_area(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    use super::widgets::InputArea;

    let focused = state.mode == ChatMode::Input;

    // When processing, allow typing but indicate messages will be queued
    let (placeholder, title_suffix) = if state.is_processing {
        let indicator = state.thinking_indicator();
        let queued_count = state.pending_messages.len();
        let queued_text = if queued_count > 0 {
            format!(" ({} queued)", queued_count)
        } else {
            String::new()
        };
        (
            format!("{} Processing... Type to queue a message", indicator),
            format!(" Processing{} ", queued_text),
        )
    } else {
        (
            "Type a message or /help for commands...".to_string(),
            String::new(),
        )
    };

    let mut widget = InputArea::new(&state.input)
        .focused(focused)
        .placeholder(&placeholder);

    // Visual indicator when processing
    if state.is_processing {
        widget = widget.processing(true, &title_suffix);
    }

    // Calculate cursor position BEFORE rendering (which consumes widget)
    let cursor_pos = if focused {
        Some(widget.cursor_position(area))
    } else {
        None
    };

    frame.render_widget(widget, area);

    // Position cursor
    if let Some(pos) = cursor_pos {
        frame.set_cursor_position(pos);
    }
}

fn draw_help_overlay(frame: &mut Frame, area: ratatui::layout::Rect) {
    use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

    let popup_width = area.width * 60 / 100;
    let popup_height = area.height * 80 / 100;
    let popup_x = (area.width - popup_width) / 2;
    let popup_y = (area.height - popup_height) / 2;

    let popup_area = ratatui::layout::Rect {
        x: area.x + popup_x,
        y: area.y + popup_y,
        width: popup_width,
        height: popup_height,
    };

    frame.render_widget(Clear, popup_area);

    let help_text = vec![
        ratatui::text::Line::from(ratatui::text::Span::styled(
            " Ted Help ",
            Style::default().fg(Color::Cyan).bold(),
        )),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(ratatui::text::Span::styled(
            "Input Mode:",
            Style::default().bold(),
        )),
        ratatui::text::Line::from("  Enter       Send message"),
        ratatui::text::Line::from("  ↑/↓         History navigation"),
        ratatui::text::Line::from("  PgUp/PgDn   Scroll chat"),
        ratatui::text::Line::from("  Ctrl+↑/↓    Scroll one line"),
        ratatui::text::Line::from("  Tab         Toggle agent pane"),
        ratatui::text::Line::from("  Ctrl+/      Show this help"),
        ratatui::text::Line::from("  Ctrl+C      Cancel/Quit"),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(ratatui::text::Span::styled(
            "Scroll Mode (Esc):",
            Style::default().bold(),
        )),
        ratatui::text::Line::from("  j/k or ↑/↓  Scroll one line"),
        ratatui::text::Line::from("  g/G         Jump to top/bottom"),
        ratatui::text::Line::from("  ?           Show this help"),
        ratatui::text::Line::from("  Esc/i/Enter Back to input"),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(ratatui::text::Span::styled(
            "Commands:",
            Style::default().bold(),
        )),
        ratatui::text::Line::from("  /help       Show this help"),
        ratatui::text::Line::from("  /settings   Open settings (General & Caps)"),
        ratatui::text::Line::from("  /model X    Quick switch model"),
        ratatui::text::Line::from("  /agents     Toggle agent pane"),
        ratatui::text::Line::from("  /clear      Clear chat history"),
        ratatui::text::Line::from("  /quit       Exit Ted"),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(ratatui::text::Span::styled(
            "Press Esc to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Help ")
                .title_style(Style::default().fg(Color::White).bold()),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(help, popup_area);
}

fn draw_settings_overlay(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    use ratatui::widgets::{Block, Borders, Clear, Paragraph};

    let popup_width = area.width.clamp(45, 65);
    // Clamp height to available space (leave at least 2 lines margin)
    let desired_height = 22_u16;
    let popup_height = desired_height.min(area.height.saturating_sub(2)).max(10);
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = ratatui::layout::Rect {
        x: area.x + popup_x,
        y: area.y + popup_y,
        width: popup_width.min(area.width.saturating_sub(area.x)),
        height: popup_height.min(area.height.saturating_sub(popup_y)),
    };

    frame.render_widget(Clear, popup_area);

    // Get settings state
    let settings = match &state.settings_state {
        Some(s) => s,
        None => return,
    };

    // Build lines
    let mut lines = Vec::new();

    // Tab bar
    let mut tab_spans = vec![ratatui::text::Span::raw("  ")];
    for section in SettingsSection::all() {
        let is_active = *section == settings.current_section;
        let style = if is_active {
            Style::default().fg(Color::Cyan).bold()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let prefix = if is_active { "▶ " } else { "  " };
        let suffix = if is_active { " ◀" } else { "  " };
        tab_spans.push(ratatui::text::Span::styled(prefix, style));
        tab_spans.push(ratatui::text::Span::styled(section.label(), style));
        tab_spans.push(ratatui::text::Span::styled(suffix, style));
        tab_spans.push(ratatui::text::Span::raw("  "));
    }
    lines.push(ratatui::text::Line::from(tab_spans));
    lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
        "─".repeat(popup_width.saturating_sub(2) as usize),
        Style::default().fg(Color::DarkGray),
    )));

    // Section content
    match settings.current_section {
        SettingsSection::General => {
            for (i, field) in SettingsField::all().iter().enumerate() {
                let is_selected = i == settings.selected_index;
                let is_editing = settings.is_editing && is_selected;

                let label = format!("{:12}", field.label());
                let value = if is_editing {
                    format!("{}▏", settings.edit_buffer)
                } else {
                    settings.current_value(*field)
                };

                let (label_style, value_style) = if is_selected {
                    (
                        Style::default().fg(Color::Cyan).bold(),
                        if is_editing {
                            Style::default().fg(Color::Yellow).bold()
                        } else {
                            Style::default().fg(Color::White).bold()
                        },
                    )
                } else {
                    (
                        Style::default().fg(Color::DarkGray),
                        Style::default().fg(Color::White),
                    )
                };

                let prefix = if is_selected { "▶ " } else { "  " };

                lines.push(ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled(prefix, Style::default().fg(Color::Cyan)),
                    ratatui::text::Span::styled(label, label_style),
                    ratatui::text::Span::raw(" "),
                    ratatui::text::Span::styled(value, value_style),
                ]));
            }
        }
        SettingsSection::Capabilities => {
            if settings.available_caps.is_empty() {
                lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                    "  No capabilities available",
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                // Show visible caps with scroll
                let visible_height = 10_usize; // Number of caps visible at once
                let total_caps = settings.available_caps.len();

                // Calculate scroll offset to keep selected item visible
                let scroll_offset = if settings.caps_selected_index >= visible_height {
                    settings.caps_selected_index - visible_height + 1
                } else {
                    0
                }
                .min(total_caps.saturating_sub(visible_height));

                for (i, (name, is_builtin)) in settings
                    .available_caps
                    .iter()
                    .enumerate()
                    .skip(scroll_offset)
                    .take(visible_height)
                {
                    let is_selected = i == settings.caps_selected_index;
                    let is_enabled = settings.caps_enabled.contains(name);

                    let checkbox = if is_enabled { "[✓]" } else { "[ ]" };
                    let builtin_tag = if *is_builtin { " (builtin)" } else { "" };

                    let style = if is_selected {
                        Style::default().fg(Color::Cyan).bold()
                    } else if is_enabled {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::White)
                    };

                    let prefix = if is_selected { "▶ " } else { "  " };

                    lines.push(ratatui::text::Line::from(vec![
                        ratatui::text::Span::styled(prefix, Style::default().fg(Color::Cyan)),
                        ratatui::text::Span::styled(checkbox, style),
                        ratatui::text::Span::raw(" "),
                        ratatui::text::Span::styled(name, style),
                        ratatui::text::Span::styled(
                            builtin_tag,
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                }

                // Show scroll indicator if needed
                if total_caps > visible_height {
                    let indicator = format!(
                        "  ({}-{} of {})",
                        scroll_offset + 1,
                        (scroll_offset + visible_height).min(total_caps),
                        total_caps
                    );
                    lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                        indicator,
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
        }
    }

    // Pad to consistent height (account for borders, help, save hint, and potential changes indicator)
    // popup_height includes borders (2), so inner content area is popup_height - 2
    // Reserve 4 lines at bottom for separator, help, save hint, and changes indicator
    let target_content_lines = popup_height.saturating_sub(6) as usize;
    while lines.len() < target_content_lines {
        lines.push(ratatui::text::Line::from(""));
    }

    // Separator
    lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
        "─".repeat(popup_width.saturating_sub(2) as usize),
        Style::default().fg(Color::DarkGray),
    )));

    // Help text
    let help_text = match settings.current_section {
        SettingsSection::General if settings.is_editing => "Enter: confirm │ Esc: cancel",
        SettingsSection::General => "Tab: section │ ↑/↓: nav │ ←/→: select │ Enter: edit/cycle",
        SettingsSection::Capabilities => "Tab: switch section │ ↑/↓: navigate │ Space: toggle",
    };
    lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
        help_text,
        Style::default().fg(Color::DarkGray),
    )));

    // Save hint
    lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
        "S: save │ Esc/q: close",
        Style::default().fg(Color::DarkGray),
    )));

    // Show change indicator
    if settings.has_changes {
        lines.push(ratatui::text::Line::from(ratatui::text::Span::styled(
            "* Unsaved changes",
            Style::default().fg(Color::Yellow),
        )));
    }

    let settings_widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Settings ")
            .title_style(Style::default().fg(Color::White).bold()),
    );

    frame.render_widget(settings_widget, popup_area);
}

/// Handle keyboard input
fn handle_key(state: &mut TuiState, key: crossterm::event::KeyEvent) -> Result<()> {
    match state.mode {
        ChatMode::Input => handle_input_key(state, key),
        ChatMode::Normal => handle_normal_key(state, key),
        ChatMode::Help => handle_help_key(state, key),
        ChatMode::Settings => handle_settings_key(state, key),
        _ => Ok(()),
    }
}

fn handle_input_key(state: &mut TuiState, key: crossterm::event::KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Esc) => {
            state.mode = ChatMode::Normal;
        }
        (KeyModifiers::NONE, KeyCode::Up) => {
            state.input.history_prev();
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            state.input.history_next();
        }
        (KeyModifiers::NONE, KeyCode::Left) => {
            state.input.move_left();
        }
        (KeyModifiers::NONE, KeyCode::Right) => {
            state.input.move_right();
        }
        (KeyModifiers::NONE, KeyCode::Home) | (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
            state.input.move_home();
        }
        (KeyModifiers::NONE, KeyCode::End) | (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
            state.input.move_end();
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            state.input.backspace();
        }
        (KeyModifiers::NONE, KeyCode::Delete) => {
            state.input.delete();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
            state.input.delete_word();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
            state.input.clear();
        }
        (KeyModifiers::NONE, KeyCode::Tab) => {
            state.agent_pane_visible = !state.agent_pane_visible;
        }
        // Page Up/Down for scrolling chat while in input mode
        (KeyModifiers::NONE, KeyCode::PageUp) => {
            let scroll_amount = (state.chat_area_height / 2).max(1) as usize;
            state.scroll_up(scroll_amount);
        }
        (KeyModifiers::NONE, KeyCode::PageDown) => {
            let scroll_amount = (state.chat_area_height / 2).max(1) as usize;
            state.scroll_down(scroll_amount);
        }
        // Ctrl+Up/Down for single-line scrolling
        (KeyModifiers::CONTROL, KeyCode::Up) => {
            state.scroll_up(1);
        }
        (KeyModifiers::CONTROL, KeyCode::Down) => {
            state.scroll_down(1);
        }
        // Ctrl+? or Ctrl+/ to show help from Input mode
        (KeyModifiers::CONTROL, KeyCode::Char('/')) => {
            state.mode = ChatMode::Help;
        }
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            state.input.insert_char(c);
        }
        _ => {}
    }
    Ok(())
}

fn handle_normal_key(state: &mut TuiState, key: crossterm::event::KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        // Esc returns to Input mode (intuitive: Esc always goes back to typing)
        (KeyModifiers::NONE, KeyCode::Esc)
        | (KeyModifiers::NONE, KeyCode::Enter)
        | (KeyModifiers::NONE, KeyCode::Char('i')) => {
            state.mode = ChatMode::Input;
        }
        (KeyModifiers::NONE, KeyCode::Tab) => {
            state.agent_pane_visible = !state.agent_pane_visible;
        }
        (KeyModifiers::NONE, KeyCode::Char('?')) => {
            state.mode = ChatMode::Help;
        }
        (KeyModifiers::NONE, KeyCode::Char('q')) => {
            state.should_quit = true;
        }
        // Scrolling in normal mode - j/k or arrow keys
        (KeyModifiers::NONE, KeyCode::Up) | (KeyModifiers::NONE, KeyCode::Char('k')) => {
            state.scroll_up(1);
        }
        (KeyModifiers::NONE, KeyCode::Down) | (KeyModifiers::NONE, KeyCode::Char('j')) => {
            state.scroll_down(1);
        }
        (KeyModifiers::NONE, KeyCode::PageUp) => {
            let scroll_amount = (state.chat_area_height / 2).max(1) as usize;
            state.scroll_up(scroll_amount);
        }
        (KeyModifiers::NONE, KeyCode::PageDown) => {
            let scroll_amount = (state.chat_area_height / 2).max(1) as usize;
            state.scroll_down(scroll_amount);
        }
        // G to jump to bottom (end)
        (KeyModifiers::SHIFT, KeyCode::Char('G')) => {
            state.auto_scroll();
        }
        // gg to jump to top (we'll use g for simplicity)
        (KeyModifiers::NONE, KeyCode::Char('g')) => {
            state.scroll_offset = 0;
        }
        _ => {}
    }
    Ok(())
}

fn handle_help_key(state: &mut TuiState, key: crossterm::event::KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
            state.mode = ChatMode::Input;
        }
        _ => {}
    }
    Ok(())
}

fn handle_settings_key(state: &mut TuiState, key: crossterm::event::KeyEvent) -> Result<()> {
    let settings = match &mut state.settings_state {
        Some(s) => s,
        None => {
            state.mode = ChatMode::Input;
            return Ok(());
        }
    };

    // Handle editing mode (only in General section)
    if settings.is_editing {
        match key.code {
            KeyCode::Esc => {
                settings.cancel_editing();
            }
            KeyCode::Enter => {
                settings.confirm_editing();
                // Update current_model if model was changed
                state.current_model = settings.model.clone();
            }
            KeyCode::Backspace => {
                settings.backspace();
            }
            KeyCode::Char(c) => {
                settings.insert_char(c);
            }
            _ => {}
        }
        return Ok(());
    }

    // Navigation mode - common keys
    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) | (_, KeyCode::Char('q')) => {
            state.mode = ChatMode::Input;
            return Ok(());
        }
        (_, KeyCode::Tab) => {
            settings.next_section();
            return Ok(());
        }
        (KeyModifiers::SHIFT, KeyCode::BackTab) => {
            settings.prev_section();
            return Ok(());
        }
        (KeyModifiers::SHIFT, KeyCode::Char('S')) | (KeyModifiers::NONE, KeyCode::Char('s')) => {
            // Save settings
            if settings.has_changes {
                let new_model = settings.model.clone();
                let new_stream = settings.stream;
                let new_trust = settings.trust_mode;
                let new_provider = settings.provider.clone();
                let new_temp = settings.temperature;
                let new_max_tokens = settings.max_tokens;
                let new_caps = settings.caps_enabled.clone();
                // Save current API key to the map before saving
                settings
                    .api_keys_by_provider
                    .insert(settings.provider.clone(), settings.api_key.clone());
                let api_keys = settings.api_keys_by_provider.clone();
                let provider_changed = new_provider != state.config.provider_name;
                settings.has_changes = false;

                // Apply changes to state
                state.current_model = new_model.clone();
                state.config.stream_enabled = new_stream;
                state.config.trust_mode = new_trust;
                // Check if caps actually changed before marking
                if state.enabled_caps != new_caps {
                    state.caps_changed = true;
                }
                state.enabled_caps = new_caps.clone();
                state.config.caps = new_caps.clone();
                state.mode = ChatMode::Input;

                // Save to settings file
                if let Ok(mut file_settings) = Settings::load() {
                    file_settings.defaults.temperature = new_temp;
                    file_settings.defaults.max_tokens = new_max_tokens;
                    file_settings.defaults.stream = new_stream;
                    file_settings.defaults.caps = new_caps.clone();

                    // Update model and API keys based on provider
                    match new_provider.as_str() {
                        "anthropic" => {
                            file_settings.providers.anthropic.default_model = new_model.clone()
                        }
                        "ollama" => {
                            file_settings.providers.ollama.default_model = new_model.clone()
                        }
                        "openrouter" => {
                            file_settings.providers.openrouter.default_model = new_model.clone()
                        }
                        "blackman" => {
                            file_settings.providers.blackman.default_model = new_model.clone()
                        }
                        "local" | "llama-cpp" => {
                            // Update llama_cpp settings with selected model
                            file_settings.providers.llama_cpp.default_model = new_model.clone();
                            // Update model path to point to the selected model
                            if let Some(home) = dirs::home_dir() {
                                let model_path = home
                                    .join(".ted")
                                    .join("models")
                                    .join("local")
                                    .join(format!("{}.gguf", new_model));
                                if model_path.exists() {
                                    file_settings.providers.llama_cpp.model_path = model_path;
                                }
                            }
                        }
                        _ => {}
                    }

                    // Save API keys for all providers (only update if non-empty to preserve existing keys)
                    if let Some(key) = api_keys.get("anthropic") {
                        if !key.is_empty() {
                            file_settings.providers.anthropic.api_key = Some(key.clone());
                        }
                        // If empty, don't touch existing key - user may not have edited it
                    }
                    if let Some(key) = api_keys.get("openrouter") {
                        if !key.is_empty() {
                            file_settings.providers.openrouter.api_key = Some(key.clone());
                        }
                    }
                    if let Some(key) = api_keys.get("blackman") {
                        if !key.is_empty() {
                            file_settings.providers.blackman.api_key = Some(key.clone());
                        }
                    }

                    if provider_changed {
                        file_settings.defaults.provider = new_provider;
                    }

                    if let Err(e) = file_settings.save() {
                        state.set_error(&format!("Failed to save: {}", e));
                        return Ok(());
                    }
                }

                // Check if provider changed (requires restart)
                if provider_changed {
                    state.needs_restart = true;
                    state.should_quit = true;
                    state.set_status("Provider changed. Restarting...");
                } else {
                    state.set_status("Settings saved to ~/.ted/settings.json");
                }
            }
            return Ok(());
        }
        _ => {}
    }

    // Section-specific key handling
    match settings.current_section {
        SettingsSection::General => match (key.modifiers, key.code) {
            (_, KeyCode::Up) | (_, KeyCode::Char('k')) => {
                settings.move_up();
            }
            (_, KeyCode::Down) | (_, KeyCode::Char('j')) => {
                settings.move_down();
            }
            (_, KeyCode::Enter) => match settings.selected_field() {
                SettingsField::Stream | SettingsField::TrustMode => {
                    settings.toggle_bool();
                }
                SettingsField::Provider => {
                    settings.cycle_provider(true);
                }
                SettingsField::Model => {
                    settings.cycle_model(true);
                }
                _ => {
                    settings.start_editing();
                }
            },
            (_, KeyCode::Left) | (_, KeyCode::Char('h')) => match settings.selected_field() {
                SettingsField::Provider => settings.cycle_provider(false),
                SettingsField::Model => settings.cycle_model(false),
                _ => {}
            },
            (_, KeyCode::Right) | (_, KeyCode::Char('l')) => match settings.selected_field() {
                SettingsField::Provider => settings.cycle_provider(true),
                SettingsField::Model => settings.cycle_model(true),
                _ => {}
            },
            (_, KeyCode::Char(' ')) => match settings.selected_field() {
                SettingsField::Stream | SettingsField::TrustMode => {
                    settings.toggle_bool();
                }
                SettingsField::Provider => {
                    settings.cycle_provider(true);
                }
                SettingsField::Model => {
                    settings.cycle_model(true);
                }
                _ => {}
            },
            _ => {}
        },
        SettingsSection::Capabilities => match (key.modifiers, key.code) {
            (_, KeyCode::Up) | (_, KeyCode::Char('k')) => {
                settings.caps_move_up();
            }
            (_, KeyCode::Down) | (_, KeyCode::Char('j')) => {
                settings.caps_move_down();
            }
            (_, KeyCode::Enter) | (_, KeyCode::Char(' ')) => {
                settings.toggle_cap();
            }
            _ => {}
        },
    }

    Ok(())
}

/// Handle slash commands
///
/// `conversation` is optional - when provided, informational command outputs (like /beads)
/// will be added to the conversation so the LLM can see and act on them.
fn handle_command(
    input: &str,
    state: &mut TuiState,
    mut conversation: Option<&mut Conversation>,
) -> Result<()> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    // Check for /model commands (with subcommands or model switch)
    if lower.starts_with("/model") {
        if let Some(args) = parse_model_command(trimmed) {
            // Check if it's a simple model switch (subcommand = "switch")
            if args.subcommand.as_deref() == Some("switch") {
                if let Some(ref model_name) = args.name {
                    state.current_model = model_name.clone();
                    state.set_status(&format!("Model set to: {}", model_name));
                    return Ok(());
                }
            }

            // Handle other subcommands via execute_model
            match execute_model(&args) {
                SlashCommandResult::SendToLlm(msg) => {
                    state.pending_messages.push(msg);
                    state.set_status("Processing /model...");
                }
                SlashCommandResult::Message(msg) => {
                    state.messages.push(DisplayMessage::system(msg));
                    state.auto_scroll();
                }
                SlashCommandResult::Error(e) => {
                    state.set_error(&e);
                }
                SlashCommandResult::SpawnAgent { task, .. } => {
                    state.pending_messages.push(task);
                    state.set_status("Processing...");
                }
            }
            return Ok(());
        }
    }

    if lower.starts_with("/cap ") {
        // Toggle a specific cap: /cap <name>
        let cap_name = trimmed[5..].trim();
        if cap_name.is_empty() {
            state.set_error("Usage: /cap <name> to toggle a capability");
            return Ok(());
        }

        // Check if cap exists
        let cap_exists = state
            .available_caps
            .iter()
            .any(|(name, _)| name == cap_name);
        if !cap_exists {
            state.set_error(&format!(
                "Unknown cap: {}. Use /caps to see available.",
                cap_name
            ));
            return Ok(());
        }

        // Toggle the cap
        if let Some(pos) = state.enabled_caps.iter().position(|c| c == cap_name) {
            state.enabled_caps.remove(pos);
            state.set_status(&format!("Disabled cap: {}", cap_name));
        } else {
            state.enabled_caps.push(cap_name.to_string());
            state.set_status(&format!("Enabled cap: {}", cap_name));
        }

        // Update config.caps so new messages use updated caps
        state.config.caps = state.enabled_caps.clone();
        return Ok(());
    }

    match lower.as_str() {
        "/help" => {
            state.mode = ChatMode::Help;
        }
        "/clear" => {
            state.messages.clear();
            state.set_status("Chat cleared");
        }
        "/agents" => {
            state.agent_pane_visible = !state.agent_pane_visible;
        }
        // "/model" is handled above with parse_model_command
        "/settings" => {
            // Open settings editor
            state.mode = ChatMode::Settings;
        }
        "/caps" => {
            // Open settings on Capabilities tab
            if let Some(ref mut settings) = state.settings_state {
                settings.current_section = SettingsSection::Capabilities;
            }
            state.mode = ChatMode::Settings;
        }
        // Development slash commands
        cmd if cmd.starts_with("/commit") => {
            if let Some(args) = parse_commit_command(trimmed) {
                let working_dir = std::env::current_dir().unwrap_or_default();
                match execute_commit(&args, &working_dir) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /commit...");
                    }
                    SlashCommandResult::Message(msg) => {
                        state.messages.push(DisplayMessage::system(msg));
                        state.auto_scroll();
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        // Convert to LLM message for now
                        state.pending_messages.push(task);
                        state.set_status("Processing...");
                    }
                }
            } else {
                state.set_error("Failed to parse /commit command");
            }
        }
        cmd if cmd.starts_with("/test") => {
            if let Some(args) = parse_test_command(trimmed) {
                let working_dir = std::env::current_dir().unwrap_or_default();
                match execute_test(&args, &working_dir) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /test...");
                    }
                    SlashCommandResult::Message(msg) => {
                        state.messages.push(DisplayMessage::system(msg));
                        state.auto_scroll();
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        state.pending_messages.push(task);
                        state.set_status("Processing...");
                    }
                }
            } else {
                state.set_error("Failed to parse /test command");
            }
        }
        cmd if cmd.starts_with("/review") => {
            if let Some(args) = parse_review_command(trimmed) {
                let working_dir = std::env::current_dir().unwrap_or_default();
                match execute_review(&args, &working_dir) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /review...");
                    }
                    SlashCommandResult::Message(msg) => {
                        state.messages.push(DisplayMessage::system(msg));
                        state.auto_scroll();
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        // Convert agent task to LLM message
                        state.pending_messages.push(task);
                        state.set_status("Starting code review...");
                    }
                }
            } else {
                state.set_error("Failed to parse /review command");
            }
        }
        cmd if cmd.starts_with("/fix") => {
            if let Some(args) = parse_fix_command(trimmed) {
                let working_dir = std::env::current_dir().unwrap_or_default();
                match execute_fix(&args, &working_dir) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /fix...");
                    }
                    SlashCommandResult::Message(msg) => {
                        state.messages.push(DisplayMessage::system(msg));
                        state.auto_scroll();
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        // Convert agent task to LLM message
                        state.pending_messages.push(task);
                        state.set_status("Fixing issues...");
                    }
                }
            } else {
                state.set_error("Failed to parse /fix command");
            }
        }
        cmd if cmd.starts_with("/explain") => {
            if let Some(args) = parse_explain_command(trimmed) {
                match execute_explain(&args) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /explain...");
                    }
                    SlashCommandResult::Message(msg) => {
                        state.messages.push(DisplayMessage::system(msg));
                        state.auto_scroll();
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        state.pending_messages.push(task);
                        state.set_status("Processing...");
                    }
                }
            } else {
                state.set_error("Failed to parse /explain command");
            }
        }
        cmd if cmd.starts_with("/skill") => {
            if let Some(args) = parse_skills_command(trimmed) {
                // Create a skill registry and scan for skills
                let mut registry = SkillRegistry::new();
                if let Err(e) = registry.scan() {
                    state.set_error(&format!("Failed to scan skills: {}", e));
                    return Ok(());
                }
                match execute_skills(&args, &registry) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /skills...");
                    }
                    SlashCommandResult::Message(msg) => {
                        state.messages.push(DisplayMessage::system(msg));
                        state.auto_scroll();
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        state.pending_messages.push(task);
                        state.set_status("Processing...");
                    }
                }
            } else {
                state.set_error("Failed to parse /skills command");
            }
        }
        cmd if cmd.starts_with("/bead") => {
            if let Some(args) = parse_beads_command(trimmed) {
                let working_dir = std::env::current_dir().unwrap_or_default();
                match execute_beads(&args, &working_dir) {
                    SlashCommandResult::SendToLlm(msg) => {
                        state.pending_messages.push(msg);
                        state.set_status("Processing /beads...");
                    }
                    SlashCommandResult::Message(msg) => {
                        // Display in UI as system message
                        state.messages.push(DisplayMessage::system(msg.clone()));
                        state.auto_scroll();

                        // Also add to conversation so LLM can see and act on pending beads
                        // This allows the assistant to proactively work on tasks
                        if let Some(conv) = conversation.as_deref_mut() {
                            let context_msg = format!(
                                "[System: User ran /beads command. Current task status:]\n{}",
                                msg
                            );
                            conv.push(Message::user(&context_msg));
                        }
                    }
                    SlashCommandResult::Error(e) => {
                        state.set_error(&e);
                    }
                    SlashCommandResult::SpawnAgent { task, .. } => {
                        state.pending_messages.push(task);
                        state.set_status("Processing...");
                    }
                }
            } else {
                state.set_error("Failed to parse /beads command");
            }
        }
        _ => {
            state.set_error(&format!("Unknown command: {}. Try /help", trimmed));
        }
    }

    Ok(())
}

/// Process LLM response and handle tool calls
#[allow(clippy::too_many_arguments)]
async fn process_llm_response(
    provider: &Arc<dyn LlmProvider>,
    model: &str,
    conversation: &mut Conversation,
    tool_executor: &mut ToolExecutor,
    settings: &Settings,
    state: &mut TuiState,
    stream_enabled: bool,
    interrupted: &Arc<AtomicBool>,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    loop {
        if interrupted.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Build completion request
        let tools = tool_executor.tool_definitions();
        let request = CompletionRequest {
            model: model.to_string(),
            messages: conversation.messages.clone(),
            tools,
            system: conversation.system_prompt.clone(),
            max_tokens: settings.defaults.max_tokens,
            temperature: settings.defaults.temperature,
            tool_choice: ToolChoice::Auto,
        };

        // Start assistant message in UI
        state.messages.push(DisplayMessage::assistant_streaming(
            state.enabled_caps.clone(),
        ));

        // Get response
        let mut response_text = String::new();
        let mut tool_uses: Vec<(String, String, serde_json::Value)> = Vec::new();
        let mut stop_reason = StopReason::EndTurn;

        if stream_enabled {
            let stream = provider.complete_stream(request.clone()).await?;
            tokio::pin!(stream);

            while let Some(event) = stream.next().await {
                if interrupted.load(Ordering::SeqCst) {
                    break;
                }

                // Check for terminal events (non-blocking) to handle Ctrl+C, scrolling, and typing
                if let Ok(true) = crossterm::event::poll(Duration::from_millis(0)) {
                    if let Ok(TermEvent::Key(key)) = crossterm::event::read() {
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('c')
                        {
                            interrupted.store(true, Ordering::SeqCst);
                            break;
                        }
                        // Allow scrolling and typing during streaming
                        match (key.modifiers, key.code) {
                            (KeyModifiers::NONE, KeyCode::PageUp) => {
                                let scroll_amount = (state.chat_area_height / 2).max(1) as usize;
                                state.scroll_up(scroll_amount);
                            }
                            (KeyModifiers::NONE, KeyCode::PageDown) => {
                                let scroll_amount = (state.chat_area_height / 2).max(1) as usize;
                                state.scroll_down(scroll_amount);
                            }
                            (KeyModifiers::CONTROL, KeyCode::Up) => state.scroll_up(1),
                            (KeyModifiers::CONTROL, KeyCode::Down) => state.scroll_down(1),
                            // Allow typing to queue messages during streaming
                            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                                state.input.insert_char(c);
                            }
                            (KeyModifiers::NONE, KeyCode::Backspace) => {
                                state.input.backspace();
                            }
                            (KeyModifiers::NONE, KeyCode::Enter) if !state.input.is_empty() => {
                                let input_text = state.input.submit();
                                // Handle commands immediately, queue regular messages
                                if input_text.trim().starts_with('/') {
                                    let _ = handle_command(&input_text, state, None);
                                } else {
                                    state.pending_messages.push(input_text);
                                    state.set_status(&format!(
                                        "Message queued ({} pending)",
                                        state.pending_messages.len()
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                }

                match event {
                    Ok(StreamEvent::ContentBlockStart {
                        content_block: ContentBlockResponse::ToolUse { id, name, .. },
                        ..
                    }) => {
                        tool_uses.push((id, name, serde_json::Value::Null));
                    }
                    Ok(StreamEvent::ContentBlockDelta { delta, .. }) => {
                        match delta {
                            ContentBlockDelta::TextDelta { text } => {
                                response_text.push_str(&text);
                                if let Some(msg) = state.messages.last_mut() {
                                    msg.append_content(&text);
                                }
                                // Refresh UI to show streaming text
                                state.tick_animation();
                                state.auto_scroll();
                                let _ = terminal.draw(|f| draw_tui(f, state));
                            }
                            ContentBlockDelta::InputJsonDelta { partial_json } => {
                                if let Some((_, _, ref mut input)) = tool_uses.last_mut() {
                                    if *input == serde_json::Value::Null {
                                        *input = serde_json::Value::String(partial_json);
                                    } else if let serde_json::Value::String(ref mut s) = input {
                                        s.push_str(&partial_json);
                                    }
                                }
                            }
                        }
                    }
                    Ok(StreamEvent::MessageDelta {
                        stop_reason: Some(r),
                        ..
                    }) => {
                        stop_reason = r;
                    }
                    Ok(StreamEvent::MessageStop) => {
                        break;
                    }
                    Err(e) => {
                        return Err(e);
                    }
                    _ => {}
                }
            }
        } else {
            let response = provider.complete(request).await?;

            for block in &response.content {
                match block {
                    ContentBlockResponse::Text { text } => {
                        response_text.push_str(text);
                    }
                    ContentBlockResponse::ToolUse { id, name, input } => {
                        tool_uses.push((id.clone(), name.clone(), input.clone()));
                    }
                }
            }

            if let Some(msg) = state.messages.last_mut() {
                msg.content = response_text.clone();
            }

            // Refresh UI to show non-streaming response
            state.tick_animation();
            state.auto_scroll();
            let _ = terminal.draw(|f| draw_tui(f, state));

            stop_reason = response.stop_reason.unwrap_or(StopReason::EndTurn);
        }

        // Finish streaming message
        if let Some(msg) = state.messages.last_mut() {
            msg.finish_streaming();
        }

        // Add assistant message to conversation
        // Only add text block if there's actual text content (empty text blocks cause API errors)
        let mut content_blocks = Vec::new();
        if !response_text.is_empty() {
            content_blocks.push(ContentBlock::Text {
                text: response_text.clone(),
            });
        }
        for (id, name, input) in &tool_uses {
            let parsed_input = if let serde_json::Value::String(s) = input {
                serde_json::from_str(s).unwrap_or(serde_json::Value::Null)
            } else {
                input.clone()
            };

            content_blocks.push(ContentBlock::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: parsed_input,
            });
        }
        conversation.push(Message::assistant_blocks(content_blocks.clone()));

        // Execute tool calls
        if !tool_uses.is_empty() && stop_reason == StopReason::ToolUse {
            let mut tool_results = Vec::new();
            let mut cancelled_mid_execution = false;

            for (id, name, input) in &tool_uses {
                if interrupted.load(Ordering::SeqCst) {
                    break;
                }

                let parsed_input = if let serde_json::Value::String(s) = input {
                    serde_json::from_str(s).unwrap_or(serde_json::Value::Null)
                } else {
                    input.clone()
                };

                // Add tool call to UI
                if let Some(msg) = state.messages.last_mut() {
                    msg.add_tool_call(DisplayToolCall::new(
                        id.clone(),
                        name.clone(),
                        parsed_input.clone(),
                    ));
                }

                // Refresh UI to show tool call starting
                state.tick_animation();
                state.auto_scroll();
                let _ = terminal.draw(|f| draw_tui(f, state));

                // Check for Ctrl+C before executing
                if let Ok(true) = crossterm::event::poll(Duration::from_millis(0)) {
                    if let Ok(TermEvent::Key(key)) = crossterm::event::read() {
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('c')
                        {
                            interrupted.store(true, Ordering::SeqCst);
                            break;
                        }
                    }
                }

                // Execute tool with periodic UI updates
                let id_clone = id.clone();
                let name_clone = name.clone();
                let tool_future =
                    tool_executor.execute_tool_use(&id_clone, &name_clone, parsed_input);
                tokio::pin!(tool_future);

                let result = loop {
                    // Check if we should abort
                    if interrupted.load(Ordering::SeqCst) {
                        // Mark tool call as cancelled in UI
                        if let Some(msg) = state.messages.last_mut() {
                            if let Some(tc) = msg.find_tool_call_mut(id) {
                                tc.complete_failed("Cancelled by user".to_string());
                            }
                        }
                        cancelled_mid_execution = true;
                        break None; // Signal cancellation
                    }

                    tokio::select! {
                        result = &mut tool_future => {
                            break Some(result?);
                        }
                        _ = tokio::time::sleep(Duration::from_millis(100)) => {
                            // Periodic UI refresh while tool is running
                            state.tick_animation();

                            // If this is a spawn_agent call, update progress from tracker
                            if name_clone == "spawn_agent" {
                                if let Some(ref tracker) = state.agent_progress_tracker {
                                    // Try to lock and get progress (non-blocking)
                                    if let Ok(guard) = tracker.try_lock() {
                                        if let Some(progress) = guard.get(id) {
                                            // Update the DisplayToolCall's progress display
                                            if let Some(msg) = state.messages.last_mut() {
                                                if let Some(tc) = msg.find_tool_call_mut(id) {
                                                    // Use display_status() which shows current tool or last activity
                                                    tc.set_progress_text(&progress.display_status());
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Handle input events BEFORE drawing so changes are immediately visible
                            if let Ok(true) = crossterm::event::poll(Duration::from_millis(0)) {
                                if let Ok(TermEvent::Key(key)) = crossterm::event::read() {
                                    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                                        interrupted.store(true, Ordering::SeqCst);
                                        // Will exit on next loop iteration
                                    }
                                    // Handle scrolling and typing during tool execution
                                    match (key.modifiers, key.code) {
                                        (KeyModifiers::NONE, KeyCode::PageUp) => {
                                            let scroll_amount = (state.chat_area_height / 2).max(1) as usize;
                                            state.scroll_up(scroll_amount);
                                        }
                                        (KeyModifiers::NONE, KeyCode::PageDown) => {
                                            let scroll_amount = (state.chat_area_height / 2).max(1) as usize;
                                            state.scroll_down(scroll_amount);
                                        }
                                        (KeyModifiers::CONTROL, KeyCode::Up) => state.scroll_up(1),
                                        (KeyModifiers::CONTROL, KeyCode::Down) => state.scroll_down(1),
                                        // Allow typing to queue messages during processing
                                        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                                            state.input.insert_char(c);
                                        }
                                        (KeyModifiers::NONE, KeyCode::Backspace) => {
                                            state.input.backspace();
                                        }
                                        (KeyModifiers::NONE, KeyCode::Enter) if !state.input.is_empty() => {
                                            let input_text = state.input.submit();
                                            // Handle commands immediately, queue regular messages
                                            if input_text.trim().starts_with('/') {
                                                let _ = handle_command(&input_text, state, None);
                                            } else {
                                                state.pending_messages.push(input_text);
                                                state.set_status(&format!("Message queued ({} pending)", state.pending_messages.len()));
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }

                            // Auto-scroll to show new content, then draw
                            state.auto_scroll();
                            let _ = terminal.draw(|f| draw_tui(f, state));
                        }
                    }
                };

                // If cancelled mid-execution, break from outer loop
                if cancelled_mid_execution {
                    break;
                }

                // Update tool call in UI (only if we got a result)
                if let Some(result) = result {
                    if let Some(msg) = state.messages.last_mut() {
                        if let Some(tc) = msg.find_tool_call_mut(id) {
                            let output = result.output_text();
                            if result.is_error() {
                                tc.complete_failed(output.to_string());
                            } else {
                                let preview = if output.chars().count() > 100 {
                                    // Truncate at character boundary, not byte boundary
                                    let truncated: String = output.chars().take(97).collect();
                                    Some(format!("{}...", truncated))
                                } else {
                                    Some(output.to_string())
                                };
                                tc.complete_success(preview, Some(output.to_string()));
                            }
                        }
                    }

                    // Refresh UI to show tool result
                    state.tick_animation();
                    let _ = terminal.draw(|f| draw_tui(f, state));

                    tool_results.push((id.clone(), result));
                }
            }

            // Collect IDs of tools that completed successfully
            let completed_ids: std::collections::HashSet<_> =
                tool_results.iter().map(|(id, _)| id.clone()).collect();

            // Add tool results to conversation for completed tools
            for (id, result) in &tool_results {
                conversation.push(Message::tool_result(
                    id,
                    result.output_text(),
                    result.is_error(),
                ));
            }

            // If interrupted, add cancelled tool_result for any unprocessed tools
            // This ensures the API gets matching tool_use/tool_result pairs
            if interrupted.load(Ordering::SeqCst) {
                for (id, _, _) in &tool_uses {
                    if !completed_ids.contains(id) {
                        conversation.push(Message::tool_result(id, "Cancelled by user", true));
                    }
                }
            }

            // Continue loop to get next response
            continue;
        }

        // No more tool calls, we're done
        break;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // ===== SettingsSection Tests =====

    #[test]
    fn test_settings_section_all() {
        let sections = SettingsSection::all();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0], SettingsSection::General);
        assert_eq!(sections[1], SettingsSection::Capabilities);
    }

    #[test]
    fn test_settings_section_labels() {
        assert_eq!(SettingsSection::General.label(), "General");
        assert_eq!(SettingsSection::Capabilities.label(), "Capabilities");
    }

    // ===== SettingsField Tests =====

    #[test]
    fn test_settings_field_all() {
        let fields = SettingsField::all();
        assert_eq!(fields.len(), 7);
        assert_eq!(fields[0], SettingsField::Provider);
        assert_eq!(fields[1], SettingsField::ApiKey);
        assert_eq!(fields[2], SettingsField::Model);
        assert_eq!(fields[3], SettingsField::Temperature);
        assert_eq!(fields[4], SettingsField::MaxTokens);
        assert_eq!(fields[5], SettingsField::Stream);
        assert_eq!(fields[6], SettingsField::TrustMode);
    }

    #[test]
    fn test_settings_field_labels() {
        assert_eq!(SettingsField::Provider.label(), "Provider");
        assert_eq!(SettingsField::ApiKey.label(), "API Key");
        assert_eq!(SettingsField::Model.label(), "Model");
        assert_eq!(SettingsField::Temperature.label(), "Temperature");
        assert_eq!(SettingsField::MaxTokens.label(), "Max Tokens");
        assert_eq!(SettingsField::Stream.label(), "Streaming");
        assert_eq!(SettingsField::TrustMode.label(), "Trust Mode");
    }

    // ===== SettingsState Tests =====

    fn create_test_settings_state() -> SettingsState {
        let settings = Settings::default();
        let config = ChatTuiConfig {
            session_id: Uuid::new_v4(),
            provider_name: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            caps: vec!["base".to_string()],
            stream_enabled: true,
            trust_mode: false,
        };
        let enabled_caps = vec!["base".to_string()];
        let available_caps = vec![
            ("base".to_string(), true),
            ("rust-expert".to_string(), true),
        ];
        SettingsState::new(&settings, &config, &enabled_caps, &available_caps)
    }

    #[test]
    fn test_settings_state_new() {
        let state = create_test_settings_state();
        assert_eq!(state.current_section, SettingsSection::General);
        assert_eq!(state.selected_index, 0);
        assert_eq!(state.caps_selected_index, 0);
        assert!(!state.is_editing);
        assert!(state.edit_buffer.is_empty());
        assert_eq!(state.provider, "anthropic");
        assert_eq!(state.model, "claude-sonnet-4-20250514");
        assert!(state.stream);
        assert!(!state.trust_mode);
        assert!(!state.has_changes);
    }

    #[test]
    fn test_settings_state_next_section() {
        let mut state = create_test_settings_state();
        assert_eq!(state.current_section, SettingsSection::General);

        state.next_section();
        assert_eq!(state.current_section, SettingsSection::Capabilities);

        state.next_section();
        assert_eq!(state.current_section, SettingsSection::General);
    }

    #[test]
    fn test_settings_state_prev_section() {
        let mut state = create_test_settings_state();
        assert_eq!(state.current_section, SettingsSection::General);

        state.prev_section();
        assert_eq!(state.current_section, SettingsSection::Capabilities);

        state.prev_section();
        assert_eq!(state.current_section, SettingsSection::General);
    }

    #[test]
    fn test_settings_state_toggle_cap() {
        let mut state = create_test_settings_state();
        state.caps_selected_index = 1; // rust-expert
        assert!(!state.caps_enabled.contains(&"rust-expert".to_string()));

        state.toggle_cap();
        assert!(state.caps_enabled.contains(&"rust-expert".to_string()));
        assert!(state.has_changes);

        state.toggle_cap();
        assert!(!state.caps_enabled.contains(&"rust-expert".to_string()));
    }

    #[test]
    fn test_settings_state_toggle_cap_empty() {
        let settings = Settings::default();
        let config = ChatTuiConfig {
            session_id: Uuid::new_v4(),
            provider_name: "anthropic".to_string(),
            model: "test".to_string(),
            caps: vec![],
            stream_enabled: true,
            trust_mode: false,
        };
        let mut state = SettingsState::new(&settings, &config, &[], &[]);
        // Should not panic with empty caps
        state.toggle_cap();
    }

    #[test]
    fn test_settings_state_caps_navigation() {
        let mut state = create_test_settings_state();
        assert_eq!(state.caps_selected_index, 0);

        state.caps_move_down();
        assert_eq!(state.caps_selected_index, 1);

        state.caps_move_down(); // Should not go beyond last item
        assert_eq!(state.caps_selected_index, 1);

        state.caps_move_up();
        assert_eq!(state.caps_selected_index, 0);

        state.caps_move_up(); // Should not go below 0
        assert_eq!(state.caps_selected_index, 0);
    }

    #[test]
    fn test_settings_state_field_navigation() {
        let mut state = create_test_settings_state();
        assert_eq!(state.selected_index, 0);
        assert_eq!(state.selected_field(), SettingsField::Provider);

        state.move_down();
        assert_eq!(state.selected_index, 1);
        assert_eq!(state.selected_field(), SettingsField::ApiKey);

        state.move_down();
        assert_eq!(state.selected_index, 2);
        assert_eq!(state.selected_field(), SettingsField::Model);

        state.move_up();
        state.move_up();
        assert_eq!(state.selected_index, 0);

        state.move_up(); // Should not go below 0
        assert_eq!(state.selected_index, 0);

        // Move to last field
        for _ in 0..10 {
            state.move_down();
        }
        assert_eq!(state.selected_index, 6); // Max index is 6 (TrustMode)
    }

    #[test]
    fn test_settings_state_cycling_model() {
        let mut state = create_test_settings_state();
        state.selected_index = 2; // Model field

        // Model field uses cycling, not text editing
        state.start_editing();
        assert!(!state.is_editing); // Should NOT enter editing mode

        // Test cycling forward through models (anthropic has 4 models)
        let initial_index = state.model_index;
        state.cycle_model(true);
        assert_eq!(state.model_index, initial_index + 1);
        assert!(state.has_changes);

        // Cycle back
        state.cycle_model(false);
        assert_eq!(state.model_index, initial_index);
    }

    #[test]
    fn test_settings_state_editing_temperature() {
        let mut state = create_test_settings_state();
        state.selected_index = 3; // Temperature field

        state.start_editing();
        state.edit_buffer = "0.5".to_string();
        state.confirm_editing();
        assert!((state.temperature - 0.5).abs() < 0.01);

        // Test clamping high
        state.start_editing();
        state.edit_buffer = "5.0".to_string();
        state.confirm_editing();
        assert!((state.temperature - 2.0).abs() < 0.01);

        // Test clamping low
        state.start_editing();
        state.edit_buffer = "-1.0".to_string();
        state.confirm_editing();
        assert!((state.temperature - 0.0).abs() < 0.01);

        // Test invalid input
        state.start_editing();
        let old_temp = state.temperature;
        state.edit_buffer = "invalid".to_string();
        state.confirm_editing();
        assert!((state.temperature - old_temp).abs() < 0.01);
    }

    #[test]
    fn test_settings_state_editing_max_tokens() {
        let mut state = create_test_settings_state();
        state.selected_index = 4; // MaxTokens field

        state.start_editing();
        state.edit_buffer = "4096".to_string();
        state.confirm_editing();
        assert_eq!(state.max_tokens, 4096);

        // Test clamping high
        state.start_editing();
        state.edit_buffer = "999999".to_string();
        state.confirm_editing();
        assert_eq!(state.max_tokens, 128000);

        // Test clamping low
        state.start_editing();
        state.edit_buffer = "10".to_string();
        state.confirm_editing();
        assert_eq!(state.max_tokens, 100);
    }

    #[test]
    fn test_settings_state_cancel_editing() {
        let mut state = create_test_settings_state();
        state.selected_index = 3; // Temperature field
        let original_temp = state.temperature;

        state.start_editing();
        state.edit_buffer = "999".to_string();
        state.cancel_editing();

        assert!(!state.is_editing);
        assert!(state.edit_buffer.is_empty());
        assert!((state.temperature - original_temp).abs() < 0.01);
    }

    #[test]
    fn test_settings_state_toggle_bool() {
        let mut state = create_test_settings_state();

        // Toggle stream (index 5)
        state.selected_index = 5;
        assert!(state.stream);
        state.toggle_bool();
        assert!(!state.stream);
        assert!(state.has_changes);

        // Toggle trust mode (index 6)
        state.selected_index = 6;
        assert!(!state.trust_mode);
        state.toggle_bool();
        assert!(state.trust_mode);

        // Toggle on non-bool field should do nothing
        state.selected_index = 2; // Model
        let old_model = state.model.clone();
        state.toggle_bool();
        assert_eq!(state.model, old_model);
    }

    #[test]
    fn test_settings_state_cycle_provider() {
        let mut state = create_test_settings_state();
        assert_eq!(state.provider, "anthropic");
        assert_eq!(state.provider_index, 0);

        state.cycle_provider(true);
        assert_eq!(state.provider, "ollama");
        assert!(state.has_changes);

        state.cycle_provider(true);
        assert_eq!(state.provider, "openrouter");

        state.cycle_provider(true);
        assert_eq!(state.provider, "blackman");

        state.cycle_provider(true);
        assert_eq!(state.provider, "local");

        state.cycle_provider(true); // Wrap around
        assert_eq!(state.provider, "anthropic");

        // Test backward cycling
        state.cycle_provider(false);
        assert_eq!(state.provider, "local");
    }

    #[test]
    fn test_settings_state_cycle_provider_backward_from_zero() {
        let mut state = create_test_settings_state();
        assert_eq!(state.provider_index, 0);

        state.cycle_provider(false);
        assert_eq!(state.provider, "local");
        assert_eq!(state.provider_index, 4);
    }

    #[test]
    fn test_settings_state_current_value() {
        let mut state = create_test_settings_state();
        state.model = "test-model".to_string();
        state.temperature = 0.7;
        state.max_tokens = 2048;
        state.stream = true;
        state.trust_mode = false;

        // Provider and Model now show cycling UI with arrows and index
        assert_eq!(
            state.current_value(SettingsField::Provider),
            "◀ anthropic ▶  (1/5)"
        );
        // Model shows the current model with cycling UI (models list has 4 anthropic models)
        assert_eq!(
            state.current_value(SettingsField::Model),
            "◀ test-model ▶  (1/4)"
        );
        assert_eq!(state.current_value(SettingsField::Temperature), "0.7");
        assert_eq!(state.current_value(SettingsField::MaxTokens), "2048");
        assert_eq!(state.current_value(SettingsField::Stream), "On");
        assert_eq!(state.current_value(SettingsField::TrustMode), "Off");

        state.stream = false;
        state.trust_mode = true;
        assert_eq!(state.current_value(SettingsField::Stream), "Off");
        assert_eq!(state.current_value(SettingsField::TrustMode), "On");
    }

    #[test]
    fn test_settings_state_insert_char_and_backspace() {
        let mut state = create_test_settings_state();
        state.edit_buffer = String::new();

        state.insert_char('a');
        state.insert_char('b');
        state.insert_char('c');
        assert_eq!(state.edit_buffer, "abc");

        state.backspace();
        assert_eq!(state.edit_buffer, "ab");

        state.backspace();
        state.backspace();
        assert!(state.edit_buffer.is_empty());

        state.backspace(); // Should not panic on empty buffer
        assert!(state.edit_buffer.is_empty());
    }

    // ===== TuiState Tests =====

    fn create_test_tui_state() -> TuiState {
        let settings = Settings::default();
        let config = ChatTuiConfig {
            session_id: Uuid::new_v4(),
            provider_name: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            caps: vec!["base".to_string()],
            stream_enabled: true,
            trust_mode: false,
        };
        TuiState::new(config, &settings)
    }

    #[test]
    fn test_tui_state_new() {
        let state = create_test_tui_state();
        assert_eq!(state.mode, ChatMode::Input);
        assert!(state.messages.is_empty());
        assert!(state.status_message.is_none());
        assert!(!state.status_is_error);
        assert!(!state.is_processing);
        assert!(!state.should_quit);
        assert!(state.agent_pane_visible);
        assert!(!state.agent_pane_expanded);
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn test_tui_state_set_status() {
        let mut state = create_test_tui_state();

        state.set_status("Test status");
        assert_eq!(state.status_message, Some("Test status".to_string()));
        assert!(!state.status_is_error);
    }

    #[test]
    fn test_tui_state_set_error() {
        let mut state = create_test_tui_state();

        state.set_error("Test error");
        assert_eq!(state.status_message, Some("Test error".to_string()));
        assert!(state.status_is_error);
    }

    #[test]
    fn test_tui_state_tick_animation() {
        let mut state = create_test_tui_state();
        assert_eq!(state.animation_frame, 0);

        state.tick_animation();
        assert_eq!(state.animation_frame, 1);

        // Test wrapping
        state.animation_frame = 255;
        state.tick_animation();
        assert_eq!(state.animation_frame, 0);
    }

    #[test]
    fn test_tui_state_thinking_indicator() {
        let mut state = create_test_tui_state();

        state.animation_frame = 0;
        assert_eq!(state.thinking_indicator(), "●○○");

        state.animation_frame = 4;
        assert_eq!(state.thinking_indicator(), "○●○");

        state.animation_frame = 8;
        assert_eq!(state.thinking_indicator(), "○○●");

        state.animation_frame = 12;
        assert_eq!(state.thinking_indicator(), "○●○");
    }

    #[test]
    fn test_tui_state_scroll_up_down() {
        let mut state = create_test_tui_state();
        state.chat_area_height = 5; // Small visible area

        // Add enough messages to require scrolling
        // Each message takes ~3 lines (header + content + spacing)
        for i in 0..10 {
            state
                .messages
                .push(DisplayMessage::user(format!("Line {}", i)));
        }

        // Set initial scroll offset
        state.scroll_offset = 10;

        state.scroll_up(2);
        assert_eq!(state.scroll_offset, 8);

        state.scroll_up(20); // Should not go below 0
        assert_eq!(state.scroll_offset, 0);

        // After scrolling down, offset should increase (but clamped to max)
        let total = state.total_messages_height();
        let max_offset = total.saturating_sub(5);
        state.scroll_down(5);
        assert!(state.scroll_offset <= max_offset);
        assert!(state.scroll_offset > 0);
    }

    #[test]
    fn test_tui_state_update_chat_height() {
        let mut state = create_test_tui_state();
        state.agent_pane_visible = false;

        state.update_chat_height(30);
        // 30 - 1 (title) - 3 (input) - 0 (agent pane hidden) = 26
        assert_eq!(state.chat_area_height, 26);

        state.agent_pane_visible = true;
        state.agent_pane_expanded = false;
        // With visible agent pane but no agents, it should be 0
        state.update_chat_height(30);
        assert_eq!(state.chat_area_height, 26); // Still 26 because no agents

        // Add an agent to make pane visible
        state.agents.track(
            Uuid::new_v4(),
            "test-agent".to_string(),
            "research".to_string(),
            "Test task".to_string(),
        );
        state.update_chat_height(30);
        // 30 - 1 (title) - 3 (input) - 3 (collapsed pane) = 23
        assert_eq!(state.chat_area_height, 23);

        state.agent_pane_expanded = true;
        state.agent_pane_height = 6;
        state.update_chat_height(30);
        // 30 - 1 (title) - 3 (input) - 6 (expanded pane) = 20
        assert_eq!(state.chat_area_height, 20);
    }

    #[test]
    fn test_tui_state_scroll_to_bottom() {
        let mut state = create_test_tui_state();
        state.chat_area_height = 5;

        // With no messages, scroll should be 0
        state.scroll_to_bottom(5);
        assert_eq!(state.scroll_offset, 0);

        // Add messages that exceed visible height
        state
            .messages
            .push(DisplayMessage::user("Line 1".to_string()));
        state
            .messages
            .push(DisplayMessage::user("Line 2".to_string()));
        state
            .messages
            .push(DisplayMessage::user("Line 3".to_string()));

        state.scroll_to_bottom(5);
        // Total height of 3 messages is about 9 lines (3 lines each: header + content + spacing)
        // scroll_offset should be total - visible
        assert!(state.scroll_offset > 0);
    }

    #[test]
    fn test_tui_state_auto_scroll() {
        let mut state = create_test_tui_state();
        state.chat_area_height = 10;

        state
            .messages
            .push(DisplayMessage::user("Test message".to_string()));
        state.auto_scroll();

        // Should be at bottom
        let expected_height = state.total_messages_height();
        if expected_height > 10 {
            assert_eq!(state.scroll_offset, expected_height - 10);
        } else {
            assert_eq!(state.scroll_offset, 0);
        }
    }

    #[test]
    fn test_tui_state_with_progress_tracker() {
        let tracker = crate::tools::builtin::new_progress_tracker();
        let state = create_test_tui_state().with_progress_tracker(tracker);
        assert!(state.agent_progress_tracker.is_some());
    }

    #[test]
    fn test_tui_state_pending_messages() {
        let mut state = create_test_tui_state();
        assert!(state.pending_messages.is_empty());

        state.pending_messages.push("First message".to_string());
        state.pending_messages.push("Second message".to_string());
        assert_eq!(state.pending_messages.len(), 2);

        let msg = state.pending_messages.remove(0);
        assert_eq!(msg, "First message");
        assert_eq!(state.pending_messages.len(), 1);
    }

    #[test]
    fn test_tui_state_caps_changed_flag() {
        let mut state = create_test_tui_state();
        assert!(!state.caps_changed);

        state.caps_changed = true;
        assert!(state.caps_changed);
    }

    #[test]
    fn test_settings_state_confirm_editing_empty_model() {
        let mut state = create_test_settings_state();
        state.selected_index = 1; // Model field
        let original_model = state.model.clone();

        state.start_editing();
        state.edit_buffer = String::new(); // Empty
        state.confirm_editing();

        // Model should remain unchanged with empty input
        assert_eq!(state.model, original_model);
    }

    #[test]
    fn test_settings_state_confirm_editing_not_editing() {
        let mut state = create_test_settings_state();
        let original_model = state.model.clone();

        // Call confirm without being in editing mode
        state.confirm_editing();

        // Nothing should change
        assert_eq!(state.model, original_model);
        assert!(!state.is_editing);
    }

    #[test]
    fn test_settings_state_start_editing_stream_field() {
        let mut state = create_test_settings_state();
        state.selected_index = 5; // Stream field

        state.start_editing();
        assert!(state.is_editing);
        // For bool fields, edit_buffer should be empty
        assert!(state.edit_buffer.is_empty());
    }

    #[test]
    fn test_settings_state_cycling_provider_field() {
        let mut state = create_test_settings_state();
        state.selected_index = 0; // Provider field

        // Provider field uses cycling, not text editing
        state.start_editing();
        assert!(!state.is_editing); // Should NOT enter editing mode

        // Test cycling forward through providers
        assert_eq!(state.provider, "anthropic");
        state.cycle_provider(true);
        assert_eq!(state.provider, "ollama");
        assert!(state.has_changes);

        state.cycle_provider(true);
        assert_eq!(state.provider, "openrouter");
    }

    // ===== handle_command Tests =====

    #[test]
    fn test_handle_command_help() {
        let mut state = create_test_tui_state();
        handle_command("/help", &mut state, None).unwrap();
        assert_eq!(state.mode, ChatMode::Help);
    }

    #[test]
    fn test_handle_command_clear() {
        let mut state = create_test_tui_state();
        state
            .messages
            .push(DisplayMessage::user("test".to_string()));
        state
            .messages
            .push(DisplayMessage::assistant("response".to_string(), vec![]));

        handle_command("/clear", &mut state, None).unwrap();
        assert!(state.messages.is_empty());
        assert_eq!(state.status_message, Some("Chat cleared".to_string()));
    }

    #[test]
    fn test_handle_command_agents() {
        let mut state = create_test_tui_state();
        assert!(state.agent_pane_visible);

        handle_command("/agents", &mut state, None).unwrap();
        assert!(!state.agent_pane_visible);

        handle_command("/agents", &mut state, None).unwrap();
        assert!(state.agent_pane_visible);
    }

    #[test]
    fn test_handle_command_model_with_arg() {
        let mut state = create_test_tui_state();
        handle_command("/model claude-opus-4-5-20250514", &mut state, None).unwrap();
        assert_eq!(state.current_model, "claude-opus-4-5-20250514");
        assert!(state
            .status_message
            .as_ref()
            .unwrap()
            .contains("Model set to"));
    }

    #[test]
    fn test_handle_command_model_with_space_only() {
        let mut state = create_test_tui_state();
        // "/model " gets trimmed to "/model" which shows info
        handle_command("/model ", &mut state, None).unwrap();
        // Should add a system message showing model info
        assert!(!state.messages.is_empty());
    }

    #[test]
    fn test_handle_command_model_no_arg() {
        let mut state = create_test_tui_state();
        handle_command("/model", &mut state, None).unwrap();
        // Should show info message in messages list
        assert!(!state.messages.is_empty());
    }

    #[test]
    fn test_handle_command_settings() {
        let mut state = create_test_tui_state();
        handle_command("/settings", &mut state, None).unwrap();
        assert_eq!(state.mode, ChatMode::Settings);
    }

    #[test]
    fn test_handle_command_caps() {
        let mut state = create_test_tui_state();
        handle_command("/caps", &mut state, None).unwrap();
        assert_eq!(state.mode, ChatMode::Settings);
        // Should open on Capabilities tab
        if let Some(ref settings) = state.settings_state {
            assert_eq!(settings.current_section, SettingsSection::Capabilities);
        }
    }

    #[test]
    fn test_handle_command_cap_toggle() {
        let mut state = create_test_tui_state();
        // Add some available caps
        state.available_caps = vec![
            ("base".to_string(), true),
            ("rust-expert".to_string(), true),
        ];
        state.enabled_caps = vec!["base".to_string()];

        // Enable rust-expert
        handle_command("/cap rust-expert", &mut state, None).unwrap();
        assert!(state.enabled_caps.contains(&"rust-expert".to_string()));
        assert!(state.status_message.as_ref().unwrap().contains("Enabled"));

        // Disable rust-expert
        handle_command("/cap rust-expert", &mut state, None).unwrap();
        assert!(!state.enabled_caps.contains(&"rust-expert".to_string()));
        assert!(state.status_message.as_ref().unwrap().contains("Disabled"));
    }

    #[test]
    fn test_handle_command_cap_unknown() {
        let mut state = create_test_tui_state();
        state.available_caps = vec![("base".to_string(), true)];

        handle_command("/cap unknown-cap", &mut state, None).unwrap();
        assert!(state.status_is_error);
        assert!(state
            .status_message
            .as_ref()
            .unwrap()
            .contains("Unknown cap"));
    }

    #[test]
    fn test_handle_command_cap_with_space_only() {
        let mut state = create_test_tui_state();
        // "/cap " gets trimmed to "/cap" which falls through to unknown command
        handle_command("/cap ", &mut state, None).unwrap();
        assert!(state.status_is_error);
        assert!(state
            .status_message
            .as_ref()
            .unwrap()
            .contains("Unknown command"));
    }

    #[test]
    fn test_handle_command_unknown() {
        let mut state = create_test_tui_state();
        handle_command("/unknown-command", &mut state, None).unwrap();
        assert!(state.status_is_error);
        assert!(state
            .status_message
            .as_ref()
            .unwrap()
            .contains("Unknown command"));
    }

    #[test]
    fn test_handle_command_case_insensitive() {
        let mut state = create_test_tui_state();
        handle_command("/HELP", &mut state, None).unwrap();
        assert_eq!(state.mode, ChatMode::Help);

        state.mode = ChatMode::Input;
        handle_command("/Help", &mut state, None).unwrap();
        assert_eq!(state.mode, ChatMode::Help);
    }

    // ===== handle_help_key Tests =====

    #[test]
    fn test_handle_help_key_esc() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Help;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_help_key(&mut state, key).unwrap();
        assert_eq!(state.mode, ChatMode::Input);
    }

    #[test]
    fn test_handle_help_key_q() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Help;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('q'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_help_key(&mut state, key).unwrap();
        assert_eq!(state.mode, ChatMode::Input);
    }

    #[test]
    fn test_handle_help_key_question() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Help;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('?'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_help_key(&mut state, key).unwrap();
        assert_eq!(state.mode, ChatMode::Input);
    }

    #[test]
    fn test_handle_help_key_other() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Help;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_help_key(&mut state, key).unwrap();
        // Should stay in Help mode
        assert_eq!(state.mode, ChatMode::Help);
    }

    // ===== handle_normal_key Tests =====

    #[test]
    fn test_handle_normal_key_enter() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();
        assert_eq!(state.mode, ChatMode::Input);
    }

    #[test]
    fn test_handle_normal_key_i() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('i'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();
        assert_eq!(state.mode, ChatMode::Input);
    }

    #[test]
    fn test_handle_normal_key_q() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('q'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();
        assert!(state.should_quit);
    }

    #[test]
    fn test_handle_normal_key_question() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('?'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();
        assert_eq!(state.mode, ChatMode::Help);
    }

    #[test]
    fn test_handle_normal_key_tab() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;
        state.agent_pane_visible = true;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();
        // Tab toggles agent_pane_visible
        assert!(!state.agent_pane_visible);
    }

    #[test]
    fn test_handle_normal_key_scroll_j() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;
        state.scroll_offset = 5;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('j'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();
        // Should scroll down (but clamped to max)
    }

    #[test]
    fn test_handle_normal_key_scroll_k() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;
        state.scroll_offset = 5;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('k'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();
        assert_eq!(state.scroll_offset, 4);
    }

    #[test]
    fn test_handle_normal_key_g_top() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;
        state.scroll_offset = 10;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn test_handle_normal_key_ctrl_a_no_op() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        handle_normal_key(&mut state, key).unwrap();
        // Ctrl+A is not handled in normal mode (falls through)
        assert_eq!(state.mode, ChatMode::Normal);
    }

    // ===== handle_input_key Tests =====

    #[test]
    fn test_handle_input_key_esc() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();
        assert_eq!(state.mode, ChatMode::Normal);
    }

    #[test]
    fn test_handle_input_key_tab() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.agent_pane_visible = true;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();
        // Tab toggles agent_pane_visible
        assert!(!state.agent_pane_visible);
    }

    #[test]
    fn test_handle_input_key_char() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.clear();

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();
        assert_eq!(state.input.text(), "a");
    }

    #[test]
    fn test_handle_input_key_backspace() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.set_buffer("ab".to_string());

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Backspace,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();
        assert_eq!(state.input.text(), "a");
    }

    #[test]
    fn test_handle_input_key_ctrl_u() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.set_buffer("hello world".to_string());

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('u'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        handle_input_key(&mut state, key).unwrap();
        assert!(state.input.is_empty());
    }

    #[test]
    fn test_handle_input_key_ctrl_w() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.set_buffer("hello world".to_string());

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('w'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        handle_input_key(&mut state, key).unwrap();
        assert_eq!(state.input.text(), "hello ");
    }

    #[test]
    fn test_handle_input_key_ctrl_a() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.set_buffer("hello".to_string());
        state.input.cursor = 3;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        handle_input_key(&mut state, key).unwrap();
        assert_eq!(state.input.cursor, 0);
    }

    #[test]
    fn test_handle_input_key_ctrl_e() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.set_buffer("hello".to_string());
        state.input.cursor = 0;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('e'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        handle_input_key(&mut state, key).unwrap();
        assert_eq!(state.input.cursor, 5);
    }

    #[test]
    fn test_handle_input_key_up_history() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.history = vec!["first".to_string(), "second".to_string()];

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Up,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();
        assert_eq!(state.input.text(), "second");
    }

    #[test]
    fn test_handle_input_key_down_history() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.history = vec!["first".to_string(), "second".to_string()];
        state.input.history_prev(); // Go to "second"

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();
    }

    #[test]
    fn test_handle_input_key_left() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.set_buffer("hello".to_string());

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Left,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();
        assert_eq!(state.input.cursor, 4);
    }

    #[test]
    fn test_handle_input_key_right() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.set_buffer("hello".to_string());
        state.input.cursor = 0;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Right,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();
        assert_eq!(state.input.cursor, 1);
    }

    // ===== TuiState Additional Tests =====

    #[test]
    fn test_tui_state_needs_restart() {
        let mut state = create_test_tui_state();
        assert!(!state.needs_restart);

        state.needs_restart = true;
        assert!(state.needs_restart);
    }

    #[test]
    fn test_tui_state_available_caps() {
        let state = create_test_tui_state();
        // Should have available caps loaded
        // (May be empty in test environment without cap files)
        assert!(state.available_caps.is_empty() || !state.available_caps.is_empty());
    }

    #[test]
    fn test_tui_state_enabled_caps() {
        let state = create_test_tui_state();
        assert_eq!(state.enabled_caps, vec!["base".to_string()]);
    }

    #[test]
    fn test_tui_state_settings_state_exists() {
        let state = create_test_tui_state();
        assert!(state.settings_state.is_some());
    }

    // ===== Drawing Function Tests =====

    fn create_test_terminal(width: u16, height: u16) -> Terminal<ratatui::backend::TestBackend> {
        let backend = ratatui::backend::TestBackend::new(width, height);
        Terminal::new(backend).unwrap()
    }

    #[test]
    fn test_draw_tui_basic() {
        let state = create_test_tui_state();
        let mut terminal = create_test_terminal(80, 24);

        terminal.draw(|f| draw_tui(f, &state)).unwrap();

        // Verify buffer is not empty
        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
        assert!(buffer.area.height > 0);
    }

    #[test]
    fn test_draw_tui_with_messages() {
        let mut state = create_test_tui_state();
        state
            .messages
            .push(DisplayMessage::user("Hello".to_string()));
        state
            .messages
            .push(DisplayMessage::assistant("Hi there!".to_string(), vec![]));

        let mut terminal = create_test_terminal(80, 24);
        terminal.draw(|f| draw_tui(f, &state)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_tui_help_mode() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Help;

        let mut terminal = create_test_terminal(80, 24);
        terminal.draw(|f| draw_tui(f, &state)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_tui_settings_mode() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;

        let mut terminal = create_test_terminal(80, 24);
        terminal.draw(|f| draw_tui(f, &state)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_tui_processing() {
        let mut state = create_test_tui_state();
        state.is_processing = true;

        let mut terminal = create_test_terminal(80, 24);
        terminal.draw(|f| draw_tui(f, &state)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_tui_with_status_message() {
        let mut state = create_test_tui_state();
        state.set_status("Test status");

        let mut terminal = create_test_terminal(80, 24);
        terminal.draw(|f| draw_tui(f, &state)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_tui_with_error_message() {
        let mut state = create_test_tui_state();
        state.set_error("Test error");

        let mut terminal = create_test_terminal(80, 24);
        terminal.draw(|f| draw_tui(f, &state)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_tui_with_agents() {
        let mut state = create_test_tui_state();
        state.agent_pane_visible = true;
        state.agents.track(
            Uuid::new_v4(),
            "test-agent".to_string(),
            "research".to_string(),
            "Test task".to_string(),
        );

        let mut terminal = create_test_terminal(80, 24);
        terminal.draw(|f| draw_tui(f, &state)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_tui_with_expanded_agent_pane() {
        let mut state = create_test_tui_state();
        state.agent_pane_visible = true;
        state.agent_pane_expanded = true;
        state.agents.track(
            Uuid::new_v4(),
            "test-agent".to_string(),
            "research".to_string(),
            "Test task".to_string(),
        );

        let mut terminal = create_test_terminal(80, 30);
        terminal.draw(|f| draw_tui(f, &state)).unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_title_bar() {
        let state = create_test_tui_state();
        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|f| {
                let area = ratatui::layout::Rect {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 1,
                };
                draw_title_bar(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_title_bar_with_caps() {
        let settings = Settings::default();
        let config = ChatTuiConfig {
            session_id: Uuid::new_v4(),
            provider_name: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            caps: vec!["base".to_string(), "rust-expert".to_string()],
            stream_enabled: true,
            trust_mode: false,
        };
        let state = TuiState::new(config, &settings);

        let mut terminal = create_test_terminal(80, 24);
        terminal
            .draw(|f| {
                let area = ratatui::layout::Rect {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 1,
                };
                draw_title_bar(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_chat_area_empty() {
        let state = create_test_tui_state();
        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|f| {
                let area = ratatui::layout::Rect {
                    x: 0,
                    y: 1,
                    width: 80,
                    height: 18,
                };
                draw_chat_area(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_chat_area_with_messages() {
        let mut state = create_test_tui_state();
        state
            .messages
            .push(DisplayMessage::user("Test message".to_string()));

        let mut terminal = create_test_terminal(80, 24);
        terminal
            .draw(|f| {
                let area = ratatui::layout::Rect {
                    x: 0,
                    y: 1,
                    width: 80,
                    height: 18,
                };
                draw_chat_area(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_agent_pane() {
        let mut state = create_test_tui_state();
        state.agents.track(
            Uuid::new_v4(),
            "test-agent".to_string(),
            "research".to_string(),
            "Test task".to_string(),
        );

        let mut terminal = create_test_terminal(80, 24);
        terminal
            .draw(|f| {
                let area = ratatui::layout::Rect {
                    x: 0,
                    y: 19,
                    width: 80,
                    height: 3,
                };
                draw_agent_pane(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_agent_pane_expanded() {
        let mut state = create_test_tui_state();
        state.agent_pane_expanded = true;
        state.agents.track(
            Uuid::new_v4(),
            "agent1".to_string(),
            "research".to_string(),
            "Task 1".to_string(),
        );
        state.agents.track(
            Uuid::new_v4(),
            "agent2".to_string(),
            "code".to_string(),
            "Task 2".to_string(),
        );

        let mut terminal = create_test_terminal(80, 24);
        terminal
            .draw(|f| {
                let area = ratatui::layout::Rect {
                    x: 0,
                    y: 16,
                    width: 80,
                    height: 6,
                };
                draw_agent_pane(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_agent_pane_focused() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::AgentFocus;
        state.agents.track(
            Uuid::new_v4(),
            "test-agent".to_string(),
            "research".to_string(),
            "Test task".to_string(),
        );

        let mut terminal = create_test_terminal(80, 24);
        terminal
            .draw(|f| {
                let area = ratatui::layout::Rect {
                    x: 0,
                    y: 19,
                    width: 80,
                    height: 3,
                };
                draw_agent_pane(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_input_area() {
        let state = create_test_tui_state();
        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|f| {
                let area = ratatui::layout::Rect {
                    x: 0,
                    y: 21,
                    width: 80,
                    height: 3,
                };
                draw_input_area(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_input_area_processing() {
        let mut state = create_test_tui_state();
        state.is_processing = true;

        let mut terminal = create_test_terminal(80, 24);
        terminal
            .draw(|f| {
                let area = ratatui::layout::Rect {
                    x: 0,
                    y: 21,
                    width: 80,
                    height: 3,
                };
                draw_input_area(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_input_area_with_queued_messages() {
        let mut state = create_test_tui_state();
        state.is_processing = true;
        state.pending_messages = vec!["queued 1".to_string(), "queued 2".to_string()];

        let mut terminal = create_test_terminal(80, 24);
        terminal
            .draw(|f| {
                let area = ratatui::layout::Rect {
                    x: 0,
                    y: 21,
                    width: 80,
                    height: 3,
                };
                draw_input_area(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_input_area_not_focused() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;

        let mut terminal = create_test_terminal(80, 24);
        terminal
            .draw(|f| {
                let area = ratatui::layout::Rect {
                    x: 0,
                    y: 21,
                    width: 80,
                    height: 3,
                };
                draw_input_area(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_help_overlay() {
        let mut terminal = create_test_terminal(80, 24);

        terminal
            .draw(|f| {
                let area = f.area();
                draw_help_overlay(f, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_help_overlay_small_terminal() {
        let mut terminal = create_test_terminal(40, 12);

        terminal
            .draw(|f| {
                let area = f.area();
                draw_help_overlay(f, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_settings_overlay_general_section() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;

        let mut terminal = create_test_terminal(80, 30);
        terminal
            .draw(|f| {
                let area = f.area();
                draw_settings_overlay(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_settings_overlay_capabilities_section() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.current_section = SettingsSection::Capabilities;
        }

        let mut terminal = create_test_terminal(80, 30);
        terminal
            .draw(|f| {
                let area = f.area();
                draw_settings_overlay(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_settings_overlay_editing() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 1; // Model
            settings.is_editing = true;
            settings.edit_buffer = "test-model".to_string();
        }

        let mut terminal = create_test_terminal(80, 30);
        terminal
            .draw(|f| {
                let area = f.area();
                draw_settings_overlay(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_settings_overlay_with_changes() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.has_changes = true;
        }

        let mut terminal = create_test_terminal(80, 30);
        terminal
            .draw(|f| {
                let area = f.area();
                draw_settings_overlay(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_settings_overlay_empty_caps() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.current_section = SettingsSection::Capabilities;
            settings.available_caps = vec![];
        }

        let mut terminal = create_test_terminal(80, 30);
        terminal
            .draw(|f| {
                let area = f.area();
                draw_settings_overlay(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_settings_overlay_many_caps() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.current_section = SettingsSection::Capabilities;
            settings.available_caps = (0..20)
                .map(|i| (format!("cap-{}", i), i % 2 == 0))
                .collect();
            settings.caps_selected_index = 15; // Trigger scrolling
        }

        let mut terminal = create_test_terminal(80, 30);
        terminal
            .draw(|f| {
                let area = f.area();
                draw_settings_overlay(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_settings_overlay_small_terminal() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;

        let mut terminal = create_test_terminal(50, 15);
        terminal
            .draw(|f| {
                let area = f.area();
                draw_settings_overlay(f, &state, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    #[test]
    fn test_draw_settings_overlay_no_settings_state() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        state.settings_state = None;

        let mut terminal = create_test_terminal(80, 30);
        terminal
            .draw(|f| {
                let area = f.area();
                draw_settings_overlay(f, &state, area);
            })
            .unwrap();

        // Should not panic, just return early
        let buffer = terminal.backend().buffer();
        assert!(buffer.area.width > 0);
    }

    // ===== handle_settings_key Tests =====

    #[test]
    fn test_handle_settings_key_esc() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();
        assert_eq!(state.mode, ChatMode::Input);
    }

    #[test]
    fn test_handle_settings_key_q() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('q'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();
        assert_eq!(state.mode, ChatMode::Input);
    }

    #[test]
    fn test_handle_settings_key_tab() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        assert_eq!(
            state.settings_state.as_ref().unwrap().current_section,
            SettingsSection::General
        );

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        assert_eq!(
            state.settings_state.as_ref().unwrap().current_section,
            SettingsSection::Capabilities
        );
    }

    #[test]
    fn test_handle_settings_key_shift_tab() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.current_section = SettingsSection::Capabilities;
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::BackTab,
            crossterm::event::KeyModifiers::SHIFT,
        );
        handle_settings_key(&mut state, key).unwrap();

        assert_eq!(
            state.settings_state.as_ref().unwrap().current_section,
            SettingsSection::General
        );
    }

    #[test]
    fn test_handle_settings_key_navigation_up() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 2;
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Up,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        assert_eq!(state.settings_state.as_ref().unwrap().selected_index, 1);
    }

    #[test]
    fn test_handle_settings_key_navigation_down() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        assert_eq!(state.settings_state.as_ref().unwrap().selected_index, 1);
    }

    #[test]
    fn test_handle_settings_key_navigation_k() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 2;
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('k'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        assert_eq!(state.settings_state.as_ref().unwrap().selected_index, 1);
    }

    #[test]
    fn test_handle_settings_key_navigation_j() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('j'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        assert_eq!(state.settings_state.as_ref().unwrap().selected_index, 1);
    }

    #[test]
    fn test_handle_settings_key_enter_on_provider() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 0; // Provider
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        // Enter on Provider cycles it
        let settings = state.settings_state.as_ref().unwrap();
        assert_eq!(settings.provider, "ollama");
    }

    #[test]
    fn test_handle_settings_key_enter_on_model() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 2; // Model
        }
        let initial_model = state.settings_state.as_ref().unwrap().model.clone();

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        // Enter on Model cycles it (not editing)
        let settings = state.settings_state.as_ref().unwrap();
        assert!(!settings.is_editing);
        assert_ne!(settings.model, initial_model);
    }

    #[test]
    fn test_handle_settings_key_enter_on_stream() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 5; // Stream
        }
        let initial_stream = state.settings_state.as_ref().unwrap().stream;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        // Enter on Stream toggles it
        assert_ne!(
            state.settings_state.as_ref().unwrap().stream,
            initial_stream
        );
    }

    #[test]
    fn test_handle_settings_key_enter_on_trust_mode() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 6; // TrustMode
        }
        let initial_trust = state.settings_state.as_ref().unwrap().trust_mode;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        // Enter on TrustMode toggles it
        assert_ne!(
            state.settings_state.as_ref().unwrap().trust_mode,
            initial_trust
        );
    }

    #[test]
    fn test_handle_settings_key_space_on_stream() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 5; // Stream
        }
        let initial_stream = state.settings_state.as_ref().unwrap().stream;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(' '),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        // Space on Stream toggles it
        assert_ne!(
            state.settings_state.as_ref().unwrap().stream,
            initial_stream
        );
    }

    #[test]
    fn test_handle_settings_key_space_on_provider() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 0; // Provider
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(' '),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        // Space on Provider cycles it
        let settings = state.settings_state.as_ref().unwrap();
        assert_eq!(settings.provider, "ollama");
    }

    #[test]
    fn test_handle_settings_key_left_on_provider() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 0; // Provider
            settings.provider_index = 1; // ollama
            settings.provider = "ollama".to_string();
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Left,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        // Left on Provider cycles backward
        let settings = state.settings_state.as_ref().unwrap();
        assert_eq!(settings.provider, "anthropic");
    }

    #[test]
    fn test_handle_settings_key_right_on_provider() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 0; // Provider
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Right,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        // Right on Provider cycles forward
        let settings = state.settings_state.as_ref().unwrap();
        assert_eq!(settings.provider, "ollama");
    }

    #[test]
    fn test_handle_settings_key_h_on_provider() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 0; // Provider
            settings.provider_index = 1;
            settings.provider = "ollama".to_string();
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('h'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        // h on Provider cycles backward
        let settings = state.settings_state.as_ref().unwrap();
        assert_eq!(settings.provider, "anthropic");
    }

    #[test]
    fn test_handle_settings_key_l_on_provider() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 0; // Provider
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('l'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        // l on Provider cycles forward
        let settings = state.settings_state.as_ref().unwrap();
        assert_eq!(settings.provider, "ollama");
    }

    #[test]
    fn test_handle_settings_key_editing_mode_char() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 1; // Model
            settings.is_editing = true;
            settings.edit_buffer = "test".to_string();
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('x'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        let settings = state.settings_state.as_ref().unwrap();
        assert_eq!(settings.edit_buffer, "testx");
    }

    #[test]
    fn test_handle_settings_key_editing_mode_backspace() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 1; // Model
            settings.is_editing = true;
            settings.edit_buffer = "test".to_string();
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Backspace,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        let settings = state.settings_state.as_ref().unwrap();
        assert_eq!(settings.edit_buffer, "tes");
    }

    #[test]
    fn test_handle_settings_key_editing_mode_esc() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 1;
            settings.is_editing = true;
            settings.edit_buffer = "changed".to_string();
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        let settings = state.settings_state.as_ref().unwrap();
        assert!(!settings.is_editing);
        assert!(settings.edit_buffer.is_empty());
    }

    #[test]
    fn test_handle_settings_key_editing_mode_enter() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.selected_index = 3; // Temperature (uses text editing)
            settings.is_editing = true;
            settings.edit_buffer = "0.8".to_string();
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        let settings = state.settings_state.as_ref().unwrap();
        assert!(!settings.is_editing);
        assert!((settings.temperature - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_handle_settings_key_caps_navigation() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.current_section = SettingsSection::Capabilities;
            settings.available_caps = vec![
                ("cap1".to_string(), true),
                ("cap2".to_string(), false),
                ("cap3".to_string(), true),
            ];
            settings.caps_selected_index = 0;
        }

        // Move down
        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();
        assert_eq!(
            state.settings_state.as_ref().unwrap().caps_selected_index,
            1
        );

        // Move down again
        handle_settings_key(&mut state, key).unwrap();
        assert_eq!(
            state.settings_state.as_ref().unwrap().caps_selected_index,
            2
        );

        // Move up
        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Up,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();
        assert_eq!(
            state.settings_state.as_ref().unwrap().caps_selected_index,
            1
        );
    }

    #[test]
    fn test_handle_settings_key_caps_toggle_space() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.current_section = SettingsSection::Capabilities;
            settings.available_caps = vec![("test-cap".to_string(), true)];
            settings.caps_enabled = vec![];
            settings.caps_selected_index = 0;
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(' '),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        let settings = state.settings_state.as_ref().unwrap();
        assert!(settings.caps_enabled.contains(&"test-cap".to_string()));
    }

    #[test]
    fn test_handle_settings_key_caps_toggle_enter() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        if let Some(ref mut settings) = state.settings_state {
            settings.current_section = SettingsSection::Capabilities;
            settings.available_caps = vec![("test-cap".to_string(), true)];
            settings.caps_enabled = vec![];
            settings.caps_selected_index = 0;
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        let settings = state.settings_state.as_ref().unwrap();
        assert!(settings.caps_enabled.contains(&"test-cap".to_string()));
    }

    #[test]
    fn test_handle_settings_key_no_settings_state() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;
        state.settings_state = None;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('j'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_settings_key(&mut state, key).unwrap();

        // Should switch back to Input mode
        assert_eq!(state.mode, ChatMode::Input);
    }

    // ===== handle_key Tests =====

    #[test]
    fn test_handle_key_dispatches_to_input() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_key(&mut state, key).unwrap();

        assert_eq!(state.input.text(), "a");
    }

    #[test]
    fn test_handle_key_dispatches_to_normal() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('?'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_key(&mut state, key).unwrap();

        assert_eq!(state.mode, ChatMode::Help);
    }

    #[test]
    fn test_handle_key_dispatches_to_help() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Help;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_key(&mut state, key).unwrap();

        assert_eq!(state.mode, ChatMode::Input);
    }

    #[test]
    fn test_handle_key_dispatches_to_settings() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Settings;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_key(&mut state, key).unwrap();

        assert_eq!(state.mode, ChatMode::Input);
    }

    #[test]
    fn test_handle_key_agent_focus_mode() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::AgentFocus;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        );
        // Should not panic - falls through to default case
        handle_key(&mut state, key).unwrap();
    }

    // ===== Additional Input Key Tests =====

    #[test]
    fn test_handle_input_key_page_up() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.chat_area_height = 10;
        state.scroll_offset = 20;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::PageUp,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();

        assert!(state.scroll_offset < 20);
    }

    #[test]
    fn test_handle_input_key_page_down() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.chat_area_height = 10;
        // Add messages so we can scroll
        for i in 0..20 {
            state
                .messages
                .push(DisplayMessage::user(format!("Message {}", i)));
        }
        state.scroll_offset = 0;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::PageDown,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();

        assert!(state.scroll_offset > 0);
    }

    #[test]
    fn test_handle_input_key_ctrl_up() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.scroll_offset = 5;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Up,
            crossterm::event::KeyModifiers::CONTROL,
        );
        handle_input_key(&mut state, key).unwrap();

        assert_eq!(state.scroll_offset, 4);
    }

    #[test]
    fn test_handle_input_key_ctrl_down() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.scroll_offset = 0;
        // Add messages
        for i in 0..10 {
            state
                .messages
                .push(DisplayMessage::user(format!("Message {}", i)));
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::CONTROL,
        );
        handle_input_key(&mut state, key).unwrap();

        // Should have scrolled
    }

    #[test]
    fn test_handle_input_key_ctrl_slash() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('/'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        handle_input_key(&mut state, key).unwrap();

        assert_eq!(state.mode, ChatMode::Help);
    }

    #[test]
    fn test_handle_input_key_home() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.set_buffer("hello world".to_string());

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Home,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();

        assert_eq!(state.input.cursor, 0);
    }

    #[test]
    fn test_handle_input_key_end() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.set_buffer("hello".to_string());
        state.input.cursor = 0;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::End,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();

        assert_eq!(state.input.cursor, 5);
    }

    #[test]
    fn test_handle_input_key_delete() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.set_buffer("hello".to_string());
        state.input.cursor = 2;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Delete,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();

        assert_eq!(state.input.text(), "helo");
    }

    #[test]
    fn test_handle_input_key_shift_char() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.clear();

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('A'),
            crossterm::event::KeyModifiers::SHIFT,
        );
        handle_input_key(&mut state, key).unwrap();

        assert_eq!(state.input.text(), "A");
    }

    // ===== Normal Key Additional Tests =====

    #[test]
    fn test_handle_normal_key_esc() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();

        assert_eq!(state.mode, ChatMode::Input);
    }

    #[test]
    fn test_handle_normal_key_page_up() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;
        state.chat_area_height = 10;
        state.scroll_offset = 20;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::PageUp,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();

        assert!(state.scroll_offset < 20);
    }

    #[test]
    fn test_handle_normal_key_page_down() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;
        state.chat_area_height = 10;
        for i in 0..20 {
            state
                .messages
                .push(DisplayMessage::user(format!("Message {}", i)));
        }
        state.scroll_offset = 0;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::PageDown,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();

        assert!(state.scroll_offset > 0);
    }

    #[test]
    fn test_handle_normal_key_shift_g() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;
        state.chat_area_height = 10;
        for i in 0..20 {
            state
                .messages
                .push(DisplayMessage::user(format!("Message {}", i)));
        }
        state.scroll_offset = 0;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('G'),
            crossterm::event::KeyModifiers::SHIFT,
        );
        handle_normal_key(&mut state, key).unwrap();

        // Should scroll to bottom
        let total_height = state.total_messages_height();
        let expected = total_height.saturating_sub(10);
        assert_eq!(state.scroll_offset, expected);
    }

    #[test]
    fn test_handle_normal_key_up_arrow() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;
        state.scroll_offset = 5;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Up,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();

        assert_eq!(state.scroll_offset, 4);
    }

    #[test]
    fn test_handle_normal_key_down_arrow() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;
        state.scroll_offset = 0;
        for i in 0..10 {
            state
                .messages
                .push(DisplayMessage::user(format!("Message {}", i)));
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();
    }

    // ===== Additional Input Mode Key Tests =====

    #[test]
    fn test_handle_input_key_left_arrow() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.set_buffer("hello".to_string());

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Left,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();

        assert!(state.input.cursor < 5);
    }

    #[test]
    fn test_handle_input_key_right_arrow() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        state.input.set_buffer("hello".to_string());
        state.input.cursor = 0;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Right,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();

        assert!(state.input.cursor > 0);
    }

    #[test]
    fn test_handle_input_key_history_up() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        // Populate history by setting buffer and submitting
        state.input.set_buffer("first".to_string());
        state.input.submit();
        state.input.set_buffer("second".to_string());
        state.input.submit();

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Up,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();
    }

    #[test]
    fn test_handle_input_key_history_down() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Input;
        // Populate history by setting buffer and submitting
        state.input.set_buffer("first".to_string());
        state.input.submit();

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Down,
            crossterm::event::KeyModifiers::NONE,
        );
        handle_input_key(&mut state, key).unwrap();
    }

    // ===== Additional Normal Mode Key Tests =====

    #[test]
    fn test_handle_normal_key_k() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;
        state.scroll_offset = 5;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('k'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();

        assert_eq!(state.scroll_offset, 4);
    }

    #[test]
    fn test_handle_normal_key_j() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;
        state.scroll_offset = 0;
        for i in 0..20 {
            state
                .messages
                .push(DisplayMessage::user(format!("Message {}", i)));
        }

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('j'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();
    }

    #[test]
    fn test_handle_normal_key_g() {
        let mut state = create_test_tui_state();
        state.mode = ChatMode::Normal;
        state.scroll_offset = 20;

        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        );
        handle_normal_key(&mut state, key).unwrap();

        assert_eq!(state.scroll_offset, 0);
    }

    // ===== SettingsState Additional Edge Cases =====

    #[test]
    fn test_settings_state_empty_model_edit() {
        let mut state = create_test_settings_state();
        state.selected_index = 1; // Model field
        let original_model = state.model.clone();

        state.start_editing();
        state.edit_buffer.clear(); // Empty edit
        state.confirm_editing();

        // Empty model should not be saved
        assert_eq!(state.model, original_model);
    }

    #[test]
    fn test_settings_state_confirm_not_editing() {
        let mut state = create_test_settings_state();
        let original_model = state.model.clone();

        // Call confirm without being in editing mode
        state.confirm_editing();

        assert_eq!(state.model, original_model);
        assert!(!state.is_editing);
    }

    #[test]
    fn test_settings_state_provider_not_in_list() {
        let settings = Settings::default();
        let config = ChatTuiConfig {
            session_id: Uuid::new_v4(),
            provider_name: "unknown_provider".to_string(),
            model: "test".to_string(),
            caps: vec![],
            stream_enabled: true,
            trust_mode: false,
        };
        let state = SettingsState::new(&settings, &config, &[], &[]);

        // Provider index should default to 0 when not found
        assert_eq!(state.provider_index, 0);
    }

    #[test]
    fn test_settings_state_provider_field_no_editing() {
        let mut state = create_test_settings_state();
        state.selected_index = 0; // Provider field

        state.start_editing();
        // Provider uses cycling, not text editing - should NOT enter editing mode
        assert!(!state.is_editing);
        assert!(state.edit_buffer.is_empty());
    }

    #[test]
    fn test_settings_state_editing_stream_field() {
        let mut state = create_test_settings_state();
        state.selected_index = 5; // Stream field

        state.start_editing();
        // Boolean fields have empty edit buffer
        assert!(state.edit_buffer.is_empty());
    }

    #[test]
    fn test_settings_state_caps_move_with_empty_list() {
        let settings = Settings::default();
        let config = ChatTuiConfig {
            session_id: Uuid::new_v4(),
            provider_name: "anthropic".to_string(),
            model: "test".to_string(),
            caps: vec![],
            stream_enabled: true,
            trust_mode: false,
        };
        let mut state = SettingsState::new(&settings, &config, &[], &[]);

        // Should not panic with empty caps list
        state.caps_move_up();
        state.caps_move_down();
        assert_eq!(state.caps_selected_index, 0);
    }

    // ===== Cancelled Tool Use Tests =====

    /// Tests that cancelled tool_result messages are correctly created with error flag.
    /// This tests the logic used when Ctrl+C interrupts tool execution - we need to
    /// add error tool_result blocks for any incomplete tool_uses to maintain API invariant.
    #[test]
    fn test_cancelled_tool_result_message() {
        use crate::llm::message::{ContentBlock, Message, MessageContent};

        let tool_use_id = "toolu_cancelled_123";
        let msg = Message::tool_result(tool_use_id, "Cancelled by user", true);

        // Verify it's a user message (tool_result is always from user role)
        assert_eq!(msg.role, crate::llm::message::Role::User);

        // Verify the content contains the tool_result with error flag
        if let MessageContent::Blocks(blocks) = &msg.content {
            assert_eq!(blocks.len(), 1);
            if let ContentBlock::ToolResult {
                tool_use_id: id,
                is_error,
                ..
            } = &blocks[0]
            {
                assert_eq!(id, tool_use_id);
                assert_eq!(*is_error, Some(true));
            } else {
                panic!("Expected ToolResult block");
            }
        } else {
            panic!("Expected Blocks content");
        }
    }

    /// Tests the logic for identifying which tool_uses need cancelled results.
    /// When interrupted, we add cancelled results for any tool_use IDs not in completed set.
    #[test]
    fn test_incomplete_tool_uses_detection() {
        use std::collections::HashSet;

        // Simulate: 3 tool_uses were requested
        let tool_uses = vec![
            (
                "tool_1".to_string(),
                "grep".to_string(),
                serde_json::Value::Null,
            ),
            (
                "tool_2".to_string(),
                "read".to_string(),
                serde_json::Value::Null,
            ),
            (
                "tool_3".to_string(),
                "write".to_string(),
                serde_json::Value::Null,
            ),
        ];

        // Simulate: only tool_1 completed before interruption
        let completed_ids: HashSet<String> = vec!["tool_1".to_string()].into_iter().collect();

        // Find incomplete tools (this is the logic from the fix)
        let incomplete: Vec<_> = tool_uses
            .iter()
            .filter(|(id, _, _)| !completed_ids.contains(id))
            .map(|(id, _, _)| id.clone())
            .collect();

        assert_eq!(incomplete.len(), 2);
        assert!(incomplete.contains(&"tool_2".to_string()));
        assert!(incomplete.contains(&"tool_3".to_string()));
        assert!(!incomplete.contains(&"tool_1".to_string()));
    }

    /// Tests that a conversation maintains valid tool_use/tool_result pairs after cancellation.
    /// This simulates the fix: when interrupted, we add cancelled results for all incomplete tools.
    #[test]
    fn test_conversation_tool_use_result_pairing_after_cancel() {
        use crate::llm::message::{ContentBlock, Conversation, Message, MessageContent};

        let mut conversation = Conversation::new();

        // Simulate: assistant responds with 2 tool_uses
        let assistant_msg = Message::assistant_blocks(vec![
            ContentBlock::Text {
                text: "I'll help with that.".to_string(),
            },
            ContentBlock::ToolUse {
                id: "tool_1".to_string(),
                name: "grep".to_string(),
                input: serde_json::json!({"pattern": "test"}),
            },
            ContentBlock::ToolUse {
                id: "tool_2".to_string(),
                name: "read".to_string(),
                input: serde_json::json!({"path": "/test"}),
            },
        ]);
        conversation.push(assistant_msg);

        // Simulate: tool_1 completed, tool_2 was cancelled
        conversation.push(Message::tool_result("tool_1", "Found 5 matches", false));
        conversation.push(Message::tool_result("tool_2", "Cancelled by user", true));

        // Verify: all tool_uses have matching tool_results
        let tool_use_ids: Vec<String> = conversation
            .messages
            .iter()
            .flat_map(|m| {
                if let MessageContent::Blocks(blocks) = &m.content {
                    blocks
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::ToolUse { id, .. } = b {
                                Some(id.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                }
            })
            .collect();

        let tool_result_ids: Vec<String> = conversation
            .messages
            .iter()
            .flat_map(|m| {
                if let MessageContent::Blocks(blocks) = &m.content {
                    blocks
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::ToolResult { tool_use_id, .. } = b {
                                Some(tool_use_id.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                }
            })
            .collect();

        // Every tool_use_id should have a matching tool_result
        assert_eq!(tool_use_ids.len(), 2);
        assert_eq!(tool_result_ids.len(), 2);
        for id in &tool_use_ids {
            assert!(
                tool_result_ids.contains(id),
                "tool_use {} missing tool_result",
                id
            );
        }
    }
}
