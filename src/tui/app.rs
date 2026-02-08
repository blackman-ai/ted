// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Application state and logic
//!
//! Manages the TUI application state including current screen,
//! selection state, and settings modifications.

use std::sync::mpsc;

use crate::caps::loader::CapLoader;
use crate::config::Settings;
use crate::models::{ModelInfo, ModelRegistry, Provider};
use crate::plans::{PlanInfo, PlanStatus, PlanStore};
use crate::tui::editor::{CommandResult, Editor, EditorMode};

/// Result of input handling
pub enum AppResult {
    /// Continue running
    Continue,
    /// Quit the application
    Quit,
}

/// Current screen being displayed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Main menu with setting categories
    MainMenu,
    /// LLM provider configuration
    Providers,
    /// Caps management
    Caps,
    /// Context settings
    Context,
    /// About/help screen
    About,
    /// Plans browser
    Plans,
    /// Single plan view (read-only)
    PlanView,
    /// Plan editor (vim-style)
    PlanEdit,
}

/// Input mode for text entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal navigation mode
    Normal,
    /// Editing a text field
    Editing,
    /// Selecting from a model list
    SelectingModel,
}

/// Main menu items
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainMenuItem {
    Providers,
    Caps,
    Plans,
    Context,
    About,
}

impl MainMenuItem {
    pub fn all() -> &'static [MainMenuItem] {
        &[
            MainMenuItem::Providers,
            MainMenuItem::Caps,
            MainMenuItem::Plans,
            MainMenuItem::Context,
            MainMenuItem::About,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            MainMenuItem::Providers => "Providers",
            MainMenuItem::Caps => "Caps",
            MainMenuItem::Plans => "Plans",
            MainMenuItem::Context => "Context",
            MainMenuItem::About => "About",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            MainMenuItem::Providers => "Configure LLM API keys and models",
            MainMenuItem::Caps => "Manage persona caps",
            MainMenuItem::Plans => "View and manage work plans",
            MainMenuItem::Context => "Storage and retention settings",
            MainMenuItem::About => "Version info and help",
        }
    }
}

/// Provider screen items
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderItem {
    DefaultProvider,
    AnthropicApiKey,
    AnthropicModel,
    LocalPort,
    LocalModel,
    OpenRouterApiKey,
    OpenRouterModel,
    BlackmanApiKey,
    BlackmanModel,
    TestConnection,
    Back,
}

impl ProviderItem {
    pub fn all() -> &'static [ProviderItem] {
        &[
            ProviderItem::DefaultProvider,
            ProviderItem::AnthropicApiKey,
            ProviderItem::AnthropicModel,
            ProviderItem::LocalPort,
            ProviderItem::LocalModel,
            ProviderItem::OpenRouterApiKey,
            ProviderItem::OpenRouterModel,
            ProviderItem::BlackmanApiKey,
            ProviderItem::BlackmanModel,
            ProviderItem::TestConnection,
            ProviderItem::Back,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            ProviderItem::DefaultProvider => "Default Provider",
            ProviderItem::AnthropicApiKey => "Anthropic API Key",
            ProviderItem::AnthropicModel => "Anthropic Model",
            ProviderItem::LocalPort => "Local Port",
            ProviderItem::LocalModel => "Local Model",
            ProviderItem::OpenRouterApiKey => "OpenRouter API Key",
            ProviderItem::OpenRouterModel => "OpenRouter Model",
            ProviderItem::BlackmanApiKey => "Blackman API Key",
            ProviderItem::BlackmanModel => "Blackman Model",
            ProviderItem::TestConnection => "Test Connection",
            ProviderItem::Back => "← Back",
        }
    }
}

/// Context screen items
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextItem {
    MaxWarmChunks,
    ColdRetentionDays,
    AutoCompact,
    Back,
}

impl ContextItem {
    pub fn all() -> &'static [ContextItem] {
        &[
            ContextItem::MaxWarmChunks,
            ContextItem::ColdRetentionDays,
            ContextItem::AutoCompact,
            ContextItem::Back,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            ContextItem::MaxWarmChunks => "Max Warm Chunks",
            ContextItem::ColdRetentionDays => "Cold Retention (days)",
            ContextItem::AutoCompact => "Auto Compact",
            ContextItem::Back => "← Back",
        }
    }
}

/// Info about a cap for display in TUI
#[derive(Debug, Clone)]
pub struct CapDisplayInfo {
    pub name: String,
    pub description: String,
    pub is_builtin: bool,
    pub is_enabled: bool,
}

/// Caps screen menu items (at the bottom of the list)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapsMenuItem {
    CreateNew,
    Back,
}

/// Display info for a model in the picker
#[derive(Debug, Clone)]
pub struct ModelDisplayInfo {
    pub id: String,
    pub name: String,
    pub tier: String,
    pub description: String,
    pub recommended: bool,
}

impl From<&ModelInfo> for ModelDisplayInfo {
    fn from(model: &ModelInfo) -> Self {
        Self {
            id: model.id.clone(),
            name: model.display_name().to_string(),
            tier: model.tier.display_name().to_string(),
            description: model.description.clone(),
            recommended: model.recommended,
        }
    }
}

/// Which provider's model we're selecting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSelectionTarget {
    Anthropic,
    Local,
    OpenRouter,
    Blackman,
}

/// Application state
pub struct App {
    /// Current settings
    pub settings: Settings,
    /// Whether settings have been modified
    pub settings_modified: bool,
    /// Current screen
    pub screen: Screen,
    /// Input mode
    pub input_mode: InputMode,
    /// Main menu selection index
    pub main_menu_index: usize,
    /// Provider screen selection index
    pub provider_index: usize,
    /// Context screen selection index
    pub context_index: usize,
    /// Caps screen selection index
    pub caps_index: usize,
    /// Plans screen selection index
    pub plans_index: usize,
    /// Current input buffer for text editing
    pub input_buffer: String,
    /// Status message to display
    pub status_message: Option<String>,
    /// Whether to show the status as an error
    pub status_is_error: bool,
    /// Caps available for display
    pub available_caps: Vec<CapDisplayInfo>,
    /// Cap loader reference
    pub cap_loader: Option<CapLoader>,
    /// Plans available for display
    pub available_plans: Vec<PlanInfo>,
    /// Currently viewed plan (for PlanView screen)
    pub current_plan_id: Option<uuid::Uuid>,
    /// Current plan content (for PlanView screen)
    pub current_plan_content: String,
    /// Plan view scroll offset
    pub plan_scroll: usize,
    /// Vim editor for plan editing
    pub editor: Option<Editor>,
    /// Model registry for model picker
    pub model_registry: ModelRegistry,
    /// Available models for the model picker
    pub available_models: Vec<ModelDisplayInfo>,
    /// Selection index in model picker
    pub model_picker_index: usize,
    /// Which provider we're selecting a model for
    pub model_selection_target: Option<ModelSelectionTarget>,
    /// Scroll offset for the model picker
    pub model_picker_scroll: usize,
    /// Whether we're currently testing a connection
    pub testing_connection: bool,
    /// Receiver for async connection test results
    connection_test_rx: Option<mpsc::Receiver<Result<String, String>>>,
}

impl App {
    /// Create a new app with the given settings
    pub fn new(settings: Settings) -> Self {
        let loader = CapLoader::new();
        let available_caps = Self::load_caps_list(&loader, &settings.defaults.caps);
        let available_plans = Self::load_plans_list();
        let model_registry = ModelRegistry::new();

        Self {
            settings,
            settings_modified: false,
            screen: Screen::MainMenu,
            input_mode: InputMode::Normal,
            main_menu_index: 0,
            provider_index: 0,
            context_index: 0,
            caps_index: 0,
            plans_index: 0,
            input_buffer: String::new(),
            status_message: None,
            status_is_error: false,
            available_caps,
            cap_loader: Some(loader),
            available_plans,
            current_plan_id: None,
            current_plan_content: String::new(),
            plan_scroll: 0,
            editor: None,
            model_registry,
            available_models: Vec::new(),
            model_picker_index: 0,
            model_selection_target: None,
            model_picker_scroll: 0,
            testing_connection: false,
            connection_test_rx: None,
        }
    }

    /// Load plans list from the store
    fn load_plans_list() -> Vec<PlanInfo> {
        match PlanStore::open() {
            Ok(store) => store.list().to_vec(),
            Err(_) => Vec::new(),
        }
    }

    /// Refresh the plans list
    pub fn refresh_plans(&mut self) {
        self.available_plans = Self::load_plans_list();
    }

    /// Get total number of items in plans screen (plans + menu items)
    pub fn plans_total_items(&self) -> usize {
        self.available_plans.len() + 1 // +1 for "Back"
    }

    /// View a specific plan
    pub fn view_plan(&mut self, id: uuid::Uuid) {
        if let Ok(store) = PlanStore::open() {
            if let Ok(Some(plan)) = store.get(id) {
                self.current_plan_id = Some(id);
                self.current_plan_content = plan.content;
                self.plan_scroll = 0;
                self.go_to(Screen::PlanView);
            }
        }
    }

    /// Set plan status
    pub fn set_plan_status(&mut self, id: uuid::Uuid, status: PlanStatus) {
        if let Ok(mut store) = PlanStore::open() {
            if store.set_status(id, status).is_ok() {
                self.refresh_plans();
                self.set_status(&format!("Plan status set to {}", status.label()), false);
            }
        }
    }

    /// Delete a plan
    pub fn delete_plan(&mut self, id: uuid::Uuid) {
        if let Ok(mut store) = PlanStore::open() {
            if store.delete(id).unwrap_or(false) {
                self.refresh_plans();
                self.set_status("Plan deleted", false);
            }
        }
    }

    /// Start editing a plan
    pub fn edit_plan(&mut self, id: uuid::Uuid) {
        if let Ok(store) = PlanStore::open() {
            if let Ok(Some(plan)) = store.get(id) {
                self.current_plan_id = Some(id);
                self.editor = Some(Editor::new(&plan.content));
                self.go_to(Screen::PlanEdit);
            }
        }
    }

    /// Save the current editor content to the plan
    pub fn save_editor(&mut self) -> bool {
        if let (Some(plan_id), Some(ref editor)) = (self.current_plan_id, &self.editor) {
            if let Ok(mut store) = PlanStore::open() {
                if store.update(plan_id, &editor.content()).is_ok() {
                    self.refresh_plans();
                    self.set_status("Plan saved", false);
                    return true;
                }
            }
        }
        self.set_status("Failed to save plan", true);
        false
    }

    /// Get the current editor mode (for status display)
    pub fn editor_mode(&self) -> Option<EditorMode> {
        self.editor.as_ref().map(|e| e.mode())
    }

    /// Get whether the editor has unsaved changes
    pub fn editor_modified(&self) -> bool {
        self.editor
            .as_ref()
            .map(|e| e.is_modified())
            .unwrap_or(false)
    }

    /// Handle editor command result
    pub fn handle_editor_command(&mut self, result: CommandResult) -> AppResult {
        match result {
            CommandResult::Continue => AppResult::Continue,
            CommandResult::Save => {
                self.save_editor();
                AppResult::Continue
            }
            CommandResult::Quit => {
                self.editor = None;
                self.go_to(Screen::Plans);
                AppResult::Continue
            }
            CommandResult::SaveQuit => {
                self.save_editor();
                self.editor = None;
                self.go_to(Screen::Plans);
                AppResult::Continue
            }
            CommandResult::Invalid(msg) => {
                self.set_status(&msg, true);
                AppResult::Continue
            }
        }
    }

    /// Load caps list from the cap loader
    fn load_caps_list(loader: &CapLoader, enabled_caps: &[String]) -> Vec<CapDisplayInfo> {
        let mut caps = Vec::new();

        if let Ok(available) = loader.list_available() {
            for (name, is_builtin) in available {
                let description = loader
                    .load(&name)
                    .map(|c| c.description.clone())
                    .unwrap_or_default();
                let is_enabled = enabled_caps.contains(&name);

                caps.push(CapDisplayInfo {
                    name,
                    description,
                    is_builtin,
                    is_enabled,
                });
            }
        }

        caps
    }

    /// Refresh the caps list
    pub fn refresh_caps(&mut self) {
        if let Some(loader) = &self.cap_loader {
            self.available_caps = Self::load_caps_list(loader, &self.settings.defaults.caps);
        }
    }

    /// Get total number of items in caps screen (caps + menu items)
    pub fn caps_total_items(&self) -> usize {
        self.available_caps.len() + 2 // +2 for "Create New" and "Back"
    }

    /// Navigate to a screen
    pub fn go_to(&mut self, screen: Screen) {
        self.screen = screen;
        self.input_mode = InputMode::Normal;
        self.clear_status();
    }

    /// Go back to main menu
    pub fn go_back(&mut self) {
        self.go_to(Screen::MainMenu);
    }

    /// Set a status message
    pub fn set_status(&mut self, message: &str, is_error: bool) {
        self.status_message = Some(message.to_string());
        self.status_is_error = is_error;
    }

    /// Clear the status message
    pub fn clear_status(&mut self) {
        self.status_message = None;
        self.status_is_error = false;
    }

    /// Start editing with the given initial value
    pub fn start_editing(&mut self, initial: &str) {
        self.input_buffer = initial.to_string();
        self.input_mode = InputMode::Editing;
    }

    /// Cancel editing
    pub fn cancel_editing(&mut self) {
        self.input_buffer.clear();
        self.input_mode = InputMode::Normal;
    }

    /// Start model selection for a provider
    pub fn start_model_selection(&mut self, target: ModelSelectionTarget) {
        let provider = match target {
            ModelSelectionTarget::Anthropic => Provider::Anthropic,
            ModelSelectionTarget::Local => Provider::Local,
            ModelSelectionTarget::OpenRouter => Provider::OpenRouter,
            ModelSelectionTarget::Blackman => Provider::Blackman,
        };

        // Use registry models for all providers
        let models = self.model_registry.models_for_provider(&provider);
        self.available_models = models.into_iter().map(ModelDisplayInfo::from).collect();

        // For local provider, also include discovered GGUF models from the system
        if target == ModelSelectionTarget::Local {
            let discovered = crate::models::scanner::scan_for_models();
            for model in discovered {
                let id = model
                    .path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&model.filename)
                    .to_string();
                // Skip if already in registry list
                if self.available_models.iter().any(|m| m.id == id) {
                    continue;
                }
                self.available_models.push(self.create_live_model_info(&id));
            }
        }

        // Find current model in list and set selection
        let current_model = match target {
            ModelSelectionTarget::Anthropic => &self.settings.providers.anthropic.default_model,
            ModelSelectionTarget::Local => &self.settings.providers.local.default_model,
            ModelSelectionTarget::OpenRouter => &self.settings.providers.openrouter.default_model,
            ModelSelectionTarget::Blackman => &self.settings.providers.blackman.default_model,
        };

        // Find index of current model (or 0 if not found)
        self.model_picker_index = self
            .available_models
            .iter()
            .position(|m| &m.id == current_model)
            .unwrap_or(0);

        self.model_selection_target = Some(target);
        self.model_picker_scroll = 0;
        self.input_mode = InputMode::SelectingModel;
    }

    /// Cancel model selection
    pub fn cancel_model_selection(&mut self) {
        self.available_models.clear();
        self.model_selection_target = None;
        self.model_picker_index = 0;
        self.model_picker_scroll = 0;
        self.input_mode = InputMode::Normal;
    }

    /// Start an async connection test for the current provider
    pub fn start_connection_test(&mut self) {
        let provider = &self.settings.defaults.provider;

        if provider == "local" {
            // For local provider, check basic configuration
            let model = &self.settings.providers.local.default_model;
            if model.is_empty() {
                self.set_status("No local model configured", true);
            } else {
                self.set_status(
                    &format!("Local provider configured with model: {}", model),
                    false,
                );
            }
        } else if provider == "anthropic" {
            // For Anthropic, just check if the API key is set
            if self.settings.get_anthropic_api_key().is_some() {
                self.set_status("Anthropic API key is configured", false);
            } else {
                self.set_status("No Anthropic API key configured", true);
            }
        } else {
            self.set_status(
                &format!("Connection test not available for {}", provider),
                true,
            );
        }
    }

    /// Check for async connection test results (non-blocking)
    pub fn check_connection_test_results(&mut self) {
        if let Some(ref rx) = self.connection_test_rx {
            match rx.try_recv() {
                Ok(Ok(msg)) => {
                    self.testing_connection = false;
                    self.connection_test_rx = None;
                    self.set_status(&msg, false);
                }
                Ok(Err(err)) => {
                    self.testing_connection = false;
                    self.connection_test_rx = None;
                    self.set_status(&err, true);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still testing, do nothing
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.testing_connection = false;
                    self.connection_test_rx = None;
                    self.set_status("Connection test interrupted", true);
                }
            }
        }
    }

    /// Confirm model selection
    pub fn confirm_model_selection(&mut self) {
        if let Some(target) = self.model_selection_target {
            if let Some(model) = self.available_models.get(self.model_picker_index) {
                let model_id = model.id.clone();
                let model_name = model.name.clone();

                match target {
                    ModelSelectionTarget::Anthropic => {
                        self.settings.providers.anthropic.default_model = model_id;
                    }
                    ModelSelectionTarget::Local => {
                        self.settings.providers.local.default_model = model_id;
                    }
                    ModelSelectionTarget::OpenRouter => {
                        self.settings.providers.openrouter.default_model = model_id;
                    }
                    ModelSelectionTarget::Blackman => {
                        self.settings.providers.blackman.default_model = model_id;
                    }
                }

                self.mark_modified();
                self.set_status(&format!("Model set to: {}", model_name), false);
            }
        }

        self.cancel_model_selection();
    }

    /// Move model picker selection up
    pub fn model_picker_up(&mut self) {
        if !self.available_models.is_empty() {
            if self.model_picker_index > 0 {
                self.model_picker_index -= 1;
            } else {
                self.model_picker_index = self.available_models.len() - 1;
            }
        }
    }

    /// Move model picker selection down
    pub fn model_picker_down(&mut self) {
        if !self.available_models.is_empty() {
            if self.model_picker_index < self.available_models.len() - 1 {
                self.model_picker_index += 1;
            } else {
                self.model_picker_index = 0;
            }
        }
    }

    /// Mark settings as modified
    pub fn mark_modified(&mut self) {
        self.settings_modified = true;
        self.set_status("Settings modified (will save on exit)", false);
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        match self.screen {
            Screen::MainMenu => {
                let len = MainMenuItem::all().len();
                if self.main_menu_index > 0 {
                    self.main_menu_index -= 1;
                } else {
                    self.main_menu_index = len - 1;
                }
            }
            Screen::Providers => {
                let len = ProviderItem::all().len();
                if self.provider_index > 0 {
                    self.provider_index -= 1;
                } else {
                    self.provider_index = len - 1;
                }
            }
            Screen::Context => {
                let len = ContextItem::all().len();
                if self.context_index > 0 {
                    self.context_index -= 1;
                } else {
                    self.context_index = len - 1;
                }
            }
            Screen::Caps => {
                let len = self.caps_total_items();
                if self.caps_index > 0 {
                    self.caps_index -= 1;
                } else {
                    self.caps_index = len - 1;
                }
            }
            Screen::Plans => {
                let len = self.plans_total_items();
                if self.plans_index > 0 {
                    self.plans_index -= 1;
                } else {
                    self.plans_index = len - 1;
                }
            }
            Screen::PlanView => {
                // Scroll up
                if self.plan_scroll > 0 {
                    self.plan_scroll -= 1;
                }
            }
            Screen::PlanEdit => {
                // Handled by editor
            }
            Screen::About => {}
        }
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        match self.screen {
            Screen::MainMenu => {
                let len = MainMenuItem::all().len();
                if self.main_menu_index < len - 1 {
                    self.main_menu_index += 1;
                } else {
                    self.main_menu_index = 0;
                }
            }
            Screen::Providers => {
                let len = ProviderItem::all().len();
                if self.provider_index < len - 1 {
                    self.provider_index += 1;
                } else {
                    self.provider_index = 0;
                }
            }
            Screen::Context => {
                let len = ContextItem::all().len();
                if self.context_index < len - 1 {
                    self.context_index += 1;
                } else {
                    self.context_index = 0;
                }
            }
            Screen::Caps => {
                let len = self.caps_total_items();
                if self.caps_index < len - 1 {
                    self.caps_index += 1;
                } else {
                    self.caps_index = 0;
                }
            }
            Screen::Plans => {
                let len = self.plans_total_items();
                if self.plans_index < len - 1 {
                    self.plans_index += 1;
                } else {
                    self.plans_index = 0;
                }
            }
            Screen::PlanView => {
                // Scroll down
                self.plan_scroll += 1;
            }
            Screen::PlanEdit => {
                // Handled by editor
            }
            Screen::About => {}
        }
    }

    /// Handle selection (Enter key)
    pub fn select(&mut self) {
        match self.screen {
            Screen::MainMenu => {
                let item = MainMenuItem::all()[self.main_menu_index];
                match item {
                    MainMenuItem::Providers => self.go_to(Screen::Providers),
                    MainMenuItem::Caps => self.go_to(Screen::Caps),
                    MainMenuItem::Plans => {
                        self.refresh_plans();
                        self.go_to(Screen::Plans);
                    }
                    MainMenuItem::Context => self.go_to(Screen::Context),
                    MainMenuItem::About => self.go_to(Screen::About),
                }
            }
            Screen::Providers => {
                let item = ProviderItem::all()[self.provider_index];
                match item {
                    ProviderItem::DefaultProvider => {
                        // Toggle between providers
                        let current = &self.settings.defaults.provider;
                        let new_provider = if current == "anthropic" {
                            "local"
                        } else {
                            "anthropic"
                        };
                        self.settings.defaults.provider = new_provider.to_string();
                        self.mark_modified();
                        self.set_status(&format!("Provider set to: {}", new_provider), false);
                    }
                    ProviderItem::AnthropicApiKey => {
                        // Start editing API key
                        let current = self
                            .settings
                            .providers
                            .anthropic
                            .api_key
                            .clone()
                            .unwrap_or_default();
                        self.start_editing(&current);
                    }
                    ProviderItem::AnthropicModel => {
                        // Open model picker for Anthropic
                        self.start_model_selection(ModelSelectionTarget::Anthropic);
                    }
                    ProviderItem::LocalPort => {
                        // Start editing local port
                        let current = self.settings.providers.local.port.to_string();
                        self.start_editing(&current);
                    }
                    ProviderItem::LocalModel => {
                        // Open model picker for Local
                        self.start_model_selection(ModelSelectionTarget::Local);
                    }
                    ProviderItem::OpenRouterApiKey => {
                        // Start editing OpenRouter API key
                        let current = self
                            .settings
                            .providers
                            .openrouter
                            .api_key
                            .clone()
                            .unwrap_or_default();
                        self.start_editing(&current);
                    }
                    ProviderItem::OpenRouterModel => {
                        // Open model picker for OpenRouter
                        self.start_model_selection(ModelSelectionTarget::OpenRouter);
                    }
                    ProviderItem::BlackmanApiKey => {
                        // Start editing Blackman API key
                        let current = self
                            .settings
                            .providers
                            .blackman
                            .api_key
                            .clone()
                            .unwrap_or_default();
                        self.start_editing(&current);
                    }
                    ProviderItem::BlackmanModel => {
                        // Open model picker for Blackman
                        self.start_model_selection(ModelSelectionTarget::Blackman);
                    }
                    ProviderItem::TestConnection => {
                        // Test connection based on current provider
                        self.start_connection_test();
                    }
                    ProviderItem::Back => self.go_back(),
                }
            }
            Screen::Context => {
                let item = ContextItem::all()[self.context_index];
                match item {
                    ContextItem::MaxWarmChunks => {
                        let current = self.settings.context.max_warm_chunks.to_string();
                        self.start_editing(&current);
                    }
                    ContextItem::ColdRetentionDays => {
                        let current = self.settings.context.cold_retention_days.to_string();
                        self.start_editing(&current);
                    }
                    ContextItem::AutoCompact => {
                        // Toggle boolean
                        self.settings.context.auto_compact = !self.settings.context.auto_compact;
                        self.mark_modified();
                    }
                    ContextItem::Back => self.go_back(),
                }
            }
            Screen::Caps => {
                let cap_count = self.available_caps.len();
                // Layout: caps first, then "Create New", then "Back"
                if self.caps_index < cap_count {
                    // Toggle the cap
                    self.toggle_cap(self.caps_index);
                } else if self.caps_index == cap_count {
                    // Create New - start editing for cap name
                    self.start_editing("");
                } else {
                    // Back
                    self.go_back();
                }
            }
            Screen::Plans => {
                let plan_count = self.available_plans.len();
                if self.plans_index < plan_count {
                    // View the selected plan
                    let plan_id = self.available_plans[self.plans_index].id;
                    self.view_plan(plan_id);
                } else {
                    // Back
                    self.go_back();
                }
            }
            Screen::PlanView => {
                // Enter from plan view goes back to plans list
                self.go_to(Screen::Plans);
            }
            Screen::PlanEdit => {
                // Handled by editor (insert newline)
            }
            Screen::About => {
                self.go_back();
            }
        }
    }

    /// Confirm editing (Enter while in edit mode)
    pub fn confirm_edit(&mut self) {
        let value = self.input_buffer.trim().to_string();

        match self.screen {
            Screen::Providers => {
                let item = ProviderItem::all()[self.provider_index];
                match item {
                    ProviderItem::AnthropicApiKey => {
                        if value.is_empty() {
                            self.settings.providers.anthropic.api_key = None;
                        } else {
                            self.settings.providers.anthropic.api_key = Some(value);
                        }
                        self.mark_modified();
                    }
                    ProviderItem::AnthropicModel => {
                        if !value.is_empty() {
                            self.settings.providers.anthropic.default_model = value;
                            self.mark_modified();
                        }
                    }
                    ProviderItem::LocalPort => {
                        if !value.is_empty() {
                            self.settings.providers.local.port =
                                value.parse::<u16>().unwrap_or(8847);
                            self.mark_modified();
                        }
                    }
                    ProviderItem::LocalModel => {
                        if !value.is_empty() {
                            self.settings.providers.local.default_model = value;
                            self.mark_modified();
                        }
                    }
                    ProviderItem::OpenRouterApiKey => {
                        if value.is_empty() {
                            self.settings.providers.openrouter.api_key = None;
                        } else {
                            self.settings.providers.openrouter.api_key = Some(value);
                        }
                        self.mark_modified();
                    }
                    ProviderItem::OpenRouterModel => {
                        if !value.is_empty() {
                            self.settings.providers.openrouter.default_model = value;
                            self.mark_modified();
                        }
                    }
                    ProviderItem::BlackmanApiKey => {
                        if value.is_empty() {
                            self.settings.providers.blackman.api_key = None;
                        } else {
                            self.settings.providers.blackman.api_key = Some(value);
                        }
                        self.mark_modified();
                    }
                    ProviderItem::BlackmanModel => {
                        if !value.is_empty() {
                            self.settings.providers.blackman.default_model = value;
                            self.mark_modified();
                        }
                    }
                    _ => {}
                }
            }
            Screen::Context => {
                let item = ContextItem::all()[self.context_index];
                match item {
                    ContextItem::MaxWarmChunks => {
                        if let Ok(v) = value.parse::<usize>() {
                            self.settings.context.max_warm_chunks = v;
                            self.mark_modified();
                        } else {
                            self.set_status("Invalid number", true);
                        }
                    }
                    ContextItem::ColdRetentionDays => {
                        if let Ok(v) = value.parse::<u32>() {
                            self.settings.context.cold_retention_days = v;
                            self.mark_modified();
                        } else {
                            self.set_status("Invalid number", true);
                        }
                    }
                    _ => {}
                }
            }
            Screen::Caps => {
                // Creating a new cap - value is the name
                if !value.is_empty() {
                    if let Err(e) = self.create_new_cap(&value) {
                        self.set_status(&format!("Error: {}", e), true);
                    } else {
                        self.set_status(&format!("Created cap: {}", value), false);
                        self.refresh_caps();
                    }
                }
            }
            _ => {}
        }

        self.cancel_editing();
    }

    /// Toggle a cap's enabled state (in default caps)
    pub fn toggle_cap(&mut self, index: usize) {
        if let Some(cap) = self.available_caps.get_mut(index) {
            cap.is_enabled = !cap.is_enabled;

            // Update settings
            if cap.is_enabled {
                if !self.settings.defaults.caps.contains(&cap.name) {
                    self.settings.defaults.caps.push(cap.name.clone());
                }
            } else {
                self.settings.defaults.caps.retain(|c| c != &cap.name);
            }

            self.mark_modified();
        }
    }

    /// Create ModelDisplayInfo from a local model name
    fn create_live_model_info(&self, model_name: &str) -> ModelDisplayInfo {
        // Check if we have this model in our registry for metadata
        if let Some(registry_model) = self
            .model_registry
            .find_model_for_provider(&Provider::Local, model_name)
        {
            // Use registry metadata
            ModelDisplayInfo::from(registry_model)
        } else {
            // Create basic info for unknown models
            ModelDisplayInfo {
                id: model_name.to_string(),
                name: model_name.to_string(),
                tier: "Unknown".to_string(),
                description: "Local model".to_string(),
                recommended: false,
            }
        }
    }
    pub fn create_new_cap(&mut self, name: &str) -> Result<(), String> {
        use crate::config::Settings;

        let caps_dir = Settings::caps_dir();
        if let Err(e) = std::fs::create_dir_all(&caps_dir) {
            return Err(format!("Failed to create caps directory: {}", e));
        }

        let cap_path = caps_dir.join(format!("{}.toml", name));
        if cap_path.exists() {
            return Err(format!("Cap '{}' already exists", name));
        }

        let template = format!(
            r#"# Cap definition for {name}
name = "{name}"
description = "Custom {name} persona"
version = "1.0.0"
priority = 10

# Inherit from other caps (optional)
extends = ["base"]

# System prompt - must be defined BEFORE any [tables]
system_prompt = """
You are an AI assistant with the {name} persona.

Follow best practices and be helpful.
"""

# Tool permissions (optional)
[tool_permissions]
enable = []
disable = []
require_edit_confirmation = true
require_shell_confirmation = true
auto_approve_paths = []
blocked_commands = []
"#
        );

        if let Err(e) = std::fs::write(&cap_path, template) {
            return Err(format!("Failed to write cap file: {}", e));
        }

        // Reload the cap loader
        self.cap_loader = Some(CapLoader::new());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== AppResult Tests =====

    #[test]
    fn test_app_result_continue() {
        let result = AppResult::Continue;
        matches!(result, AppResult::Continue);
    }

    #[test]
    fn test_app_result_quit() {
        let result = AppResult::Quit;
        matches!(result, AppResult::Quit);
    }

    // ===== Screen Tests =====

    #[test]
    fn test_screen_equality() {
        assert_eq!(Screen::MainMenu, Screen::MainMenu);
        assert_eq!(Screen::Providers, Screen::Providers);
        assert_eq!(Screen::Caps, Screen::Caps);
        assert_eq!(Screen::Context, Screen::Context);
        assert_eq!(Screen::About, Screen::About);
    }

    #[test]
    fn test_screen_inequality() {
        assert_ne!(Screen::MainMenu, Screen::Providers);
        assert_ne!(Screen::Caps, Screen::Context);
    }

    #[test]
    fn test_screen_debug() {
        let screen = Screen::MainMenu;
        assert!(format!("{:?}", screen).contains("MainMenu"));
    }

    #[test]
    fn test_screen_clone() {
        let screen = Screen::Providers;
        let cloned = screen;
        assert_eq!(screen, cloned);
    }

    // ===== InputMode Tests =====

    #[test]
    fn test_input_mode_equality() {
        assert_eq!(InputMode::Normal, InputMode::Normal);
        assert_eq!(InputMode::Editing, InputMode::Editing);
    }

    #[test]
    fn test_input_mode_inequality() {
        assert_ne!(InputMode::Normal, InputMode::Editing);
    }

    #[test]
    fn test_input_mode_debug() {
        let mode = InputMode::Editing;
        assert!(format!("{:?}", mode).contains("Editing"));
    }

    // ===== MainMenuItem Tests =====

    #[test]
    fn test_main_menu_item_all() {
        let items = MainMenuItem::all();
        assert_eq!(items.len(), 5);
        assert_eq!(items[0], MainMenuItem::Providers);
        assert_eq!(items[1], MainMenuItem::Caps);
        assert_eq!(items[2], MainMenuItem::Plans);
        assert_eq!(items[3], MainMenuItem::Context);
        assert_eq!(items[4], MainMenuItem::About);
    }

    #[test]
    fn test_main_menu_item_label() {
        assert_eq!(MainMenuItem::Providers.label(), "Providers");
        assert_eq!(MainMenuItem::Caps.label(), "Caps");
        assert_eq!(MainMenuItem::Plans.label(), "Plans");
        assert_eq!(MainMenuItem::Context.label(), "Context");
        assert_eq!(MainMenuItem::About.label(), "About");
    }

    #[test]
    fn test_main_menu_item_description() {
        assert!(!MainMenuItem::Providers.description().is_empty());
        assert!(MainMenuItem::Providers.description().contains("API"));
        assert!(MainMenuItem::Caps.description().contains("persona"));
        assert!(MainMenuItem::Plans.description().contains("plans"));
        assert!(MainMenuItem::Context.description().contains("Storage"));
        assert!(MainMenuItem::About.description().contains("Version"));
    }

    #[test]
    fn test_main_menu_item_equality() {
        assert_eq!(MainMenuItem::Providers, MainMenuItem::Providers);
        assert_ne!(MainMenuItem::Providers, MainMenuItem::Caps);
    }

    // ===== ProviderItem Tests =====

    #[test]
    fn test_provider_item_all() {
        let items = ProviderItem::all();
        assert_eq!(items.len(), 11);
        assert_eq!(items[0], ProviderItem::DefaultProvider);
        assert_eq!(items[1], ProviderItem::AnthropicApiKey);
        assert_eq!(items[2], ProviderItem::AnthropicModel);
        assert_eq!(items[3], ProviderItem::LocalPort);
        assert_eq!(items[4], ProviderItem::LocalModel);
        assert_eq!(items[5], ProviderItem::OpenRouterApiKey);
        assert_eq!(items[6], ProviderItem::OpenRouterModel);
        assert_eq!(items[7], ProviderItem::BlackmanApiKey);
        assert_eq!(items[8], ProviderItem::BlackmanModel);
        assert_eq!(items[9], ProviderItem::TestConnection);
        assert_eq!(items[10], ProviderItem::Back);
    }

    #[test]
    fn test_provider_item_label() {
        assert_eq!(ProviderItem::DefaultProvider.label(), "Default Provider");
        assert_eq!(ProviderItem::AnthropicApiKey.label(), "Anthropic API Key");
        assert_eq!(ProviderItem::AnthropicModel.label(), "Anthropic Model");
        assert_eq!(ProviderItem::LocalPort.label(), "Local Port");
        assert_eq!(ProviderItem::LocalModel.label(), "Local Model");
        assert_eq!(ProviderItem::TestConnection.label(), "Test Connection");
        assert_eq!(ProviderItem::Back.label(), "← Back");
    }

    #[test]
    fn test_provider_item_equality() {
        assert_eq!(ProviderItem::DefaultProvider, ProviderItem::DefaultProvider);
        assert_ne!(ProviderItem::DefaultProvider, ProviderItem::AnthropicApiKey);
    }

    // ===== ContextItem Tests =====

    #[test]
    fn test_context_item_all() {
        let items = ContextItem::all();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0], ContextItem::MaxWarmChunks);
        assert_eq!(items[1], ContextItem::ColdRetentionDays);
        assert_eq!(items[2], ContextItem::AutoCompact);
        assert_eq!(items[3], ContextItem::Back);
    }

    #[test]
    fn test_context_item_label() {
        assert_eq!(ContextItem::MaxWarmChunks.label(), "Max Warm Chunks");
        assert_eq!(
            ContextItem::ColdRetentionDays.label(),
            "Cold Retention (days)"
        );
        assert_eq!(ContextItem::AutoCompact.label(), "Auto Compact");
        assert_eq!(ContextItem::Back.label(), "← Back");
    }

    #[test]
    fn test_context_item_equality() {
        assert_eq!(ContextItem::MaxWarmChunks, ContextItem::MaxWarmChunks);
        assert_ne!(ContextItem::MaxWarmChunks, ContextItem::Back);
    }

    // ===== CapDisplayInfo Tests =====

    #[test]
    fn test_cap_display_info_creation() {
        let cap = CapDisplayInfo {
            name: "test-cap".to_string(),
            description: "A test cap".to_string(),
            is_builtin: true,
            is_enabled: false,
        };
        assert_eq!(cap.name, "test-cap");
        assert_eq!(cap.description, "A test cap");
        assert!(cap.is_builtin);
        assert!(!cap.is_enabled);
    }

    #[test]
    fn test_cap_display_info_clone() {
        let cap = CapDisplayInfo {
            name: "my-cap".to_string(),
            description: "My cap".to_string(),
            is_builtin: false,
            is_enabled: true,
        };
        let cloned = cap.clone();
        assert_eq!(cloned.name, cap.name);
        assert_eq!(cloned.description, cap.description);
        assert_eq!(cloned.is_builtin, cap.is_builtin);
        assert_eq!(cloned.is_enabled, cap.is_enabled);
    }

    #[test]
    fn test_cap_display_info_debug() {
        let cap = CapDisplayInfo {
            name: "test".to_string(),
            description: "desc".to_string(),
            is_builtin: true,
            is_enabled: true,
        };
        let debug = format!("{:?}", cap);
        assert!(debug.contains("CapDisplayInfo"));
        assert!(debug.contains("test"));
    }

    // ===== CapsMenuItem Tests =====

    #[test]
    fn test_caps_menu_item_equality() {
        assert_eq!(CapsMenuItem::CreateNew, CapsMenuItem::CreateNew);
        assert_eq!(CapsMenuItem::Back, CapsMenuItem::Back);
        assert_ne!(CapsMenuItem::CreateNew, CapsMenuItem::Back);
    }

    // ===== App Tests =====

    #[test]
    fn test_app_new() {
        let settings = Settings::default();
        let app = App::new(settings);

        assert_eq!(app.screen, Screen::MainMenu);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.main_menu_index, 0);
        assert_eq!(app.provider_index, 0);
        assert_eq!(app.context_index, 0);
        assert_eq!(app.caps_index, 0);
        assert!(app.input_buffer.is_empty());
        assert!(app.status_message.is_none());
        assert!(!app.status_is_error);
        assert!(!app.settings_modified);
    }

    #[test]
    fn test_app_go_to() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.go_to(Screen::Providers);
        assert_eq!(app.screen, Screen::Providers);
        assert_eq!(app.input_mode, InputMode::Normal);

        app.go_to(Screen::Caps);
        assert_eq!(app.screen, Screen::Caps);

        app.go_to(Screen::Context);
        assert_eq!(app.screen, Screen::Context);

        app.go_to(Screen::About);
        assert_eq!(app.screen, Screen::About);
    }

    #[test]
    fn test_app_go_back() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.go_to(Screen::Providers);
        app.go_back();
        assert_eq!(app.screen, Screen::MainMenu);
    }

    #[test]
    fn test_app_set_status() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.set_status("Test message", false);
        assert_eq!(app.status_message.as_ref().unwrap(), "Test message");
        assert!(!app.status_is_error);

        app.set_status("Error message", true);
        assert_eq!(app.status_message.as_ref().unwrap(), "Error message");
        assert!(app.status_is_error);
    }

    #[test]
    fn test_app_clear_status() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.set_status("Test", false);
        app.clear_status();
        assert!(app.status_message.is_none());
        assert!(!app.status_is_error);
    }

    #[test]
    fn test_app_start_editing() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.start_editing("initial value");
        assert_eq!(app.input_mode, InputMode::Editing);
        assert_eq!(app.input_buffer, "initial value");
    }

    #[test]
    fn test_app_cancel_editing() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.start_editing("some value");
        app.cancel_editing();
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.input_buffer.is_empty());
    }

    #[test]
    fn test_app_mark_modified() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        assert!(!app.settings_modified);
        app.mark_modified();
        assert!(app.settings_modified);
        assert!(app.status_message.is_some());
    }

    #[test]
    fn test_app_move_up_main_menu() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        assert_eq!(app.main_menu_index, 0);
        app.move_up(); // Should wrap to last item
        assert_eq!(app.main_menu_index, MainMenuItem::all().len() - 1);

        app.move_up();
        assert_eq!(app.main_menu_index, MainMenuItem::all().len() - 2);
    }

    #[test]
    fn test_app_move_down_main_menu() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        assert_eq!(app.main_menu_index, 0);
        app.move_down();
        assert_eq!(app.main_menu_index, 1);

        app.move_down();
        assert_eq!(app.main_menu_index, 2);
    }

    #[test]
    fn test_app_move_down_wraps() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.main_menu_index = MainMenuItem::all().len() - 1;
        app.move_down(); // Should wrap to 0
        assert_eq!(app.main_menu_index, 0);
    }

    #[test]
    fn test_app_move_up_providers() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;

        assert_eq!(app.provider_index, 0);
        app.move_up(); // Should wrap to last
        assert_eq!(app.provider_index, ProviderItem::all().len() - 1);
    }

    #[test]
    fn test_app_move_down_providers() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;

        app.move_down();
        assert_eq!(app.provider_index, 1);
    }

    #[test]
    fn test_app_move_up_context() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Context;

        assert_eq!(app.context_index, 0);
        app.move_up(); // Should wrap
        assert_eq!(app.context_index, ContextItem::all().len() - 1);
    }

    #[test]
    fn test_app_move_down_context() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Context;

        app.move_down();
        assert_eq!(app.context_index, 1);
    }

    #[test]
    fn test_app_move_up_caps() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Caps;

        // Caps screen includes available_caps + 2 menu items
        let total = app.caps_total_items();
        assert_eq!(app.caps_index, 0);
        app.move_up(); // Should wrap to last
        assert_eq!(app.caps_index, total - 1);
    }

    #[test]
    fn test_app_move_down_caps() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Caps;

        app.move_down();
        assert_eq!(app.caps_index, 1);
    }

    #[test]
    fn test_app_move_up_about_no_change() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::About;

        // About screen doesn't have navigation
        app.move_up();
        app.move_down();
        // Just verify it doesn't panic
    }

    #[test]
    fn test_app_caps_total_items() {
        let settings = Settings::default();
        let app = App::new(settings);

        // Total items = available_caps.len() + 2 (Create New, Back)
        let expected = app.available_caps.len() + 2;
        assert_eq!(app.caps_total_items(), expected);
    }

    #[test]
    fn test_app_select_main_menu_providers() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.main_menu_index = 0; // Providers
        app.select();
        assert_eq!(app.screen, Screen::Providers);
    }

    #[test]
    fn test_app_select_main_menu_caps() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.main_menu_index = 1; // Caps
        app.select();
        assert_eq!(app.screen, Screen::Caps);
    }

    #[test]
    fn test_app_select_main_menu_plans() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.main_menu_index = 2; // Plans
        app.select();
        assert_eq!(app.screen, Screen::Plans);
    }

    #[test]
    fn test_app_select_main_menu_context() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.main_menu_index = 3; // Context
        app.select();
        assert_eq!(app.screen, Screen::Context);
    }

    #[test]
    fn test_app_select_main_menu_about() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.main_menu_index = 4; // About
        app.select();
        assert_eq!(app.screen, Screen::About);
    }

    #[test]
    fn test_app_select_providers_back() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;

        app.provider_index = 10; // Back
        app.select();
        assert_eq!(app.screen, Screen::MainMenu);
    }

    #[test]
    fn test_app_select_providers_default_provider_toggles() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;

        app.provider_index = 0; // DefaultProvider
        let original = app.settings.defaults.provider.clone();
        app.select();
        // Should toggle provider
        assert_ne!(app.settings.defaults.provider, original);
        assert!(app.settings_modified);
    }

    #[test]
    fn test_app_select_providers_api_key_starts_editing() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;

        app.provider_index = 1; // AnthropicApiKey
        app.select();
        assert_eq!(app.input_mode, InputMode::Editing);
    }

    #[test]
    fn test_app_select_providers_model_opens_picker() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;

        app.provider_index = 2; // AnthropicModel
        app.select();
        assert_eq!(app.input_mode, InputMode::SelectingModel);
        assert_eq!(
            app.model_selection_target,
            Some(ModelSelectionTarget::Anthropic)
        );
        assert!(!app.available_models.is_empty());
    }

    #[test]
    fn test_app_select_providers_local_port_starts_editing() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;

        app.provider_index = 3; // LocalPort
        app.select();
        assert_eq!(app.input_mode, InputMode::Editing);
    }

    #[test]
    fn test_app_select_providers_local_model_opens_picker() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;

        app.provider_index = 4; // LocalModel
        app.select();
        assert_eq!(app.input_mode, InputMode::SelectingModel);
        assert_eq!(
            app.model_selection_target,
            Some(ModelSelectionTarget::Local)
        );
        // Uses registry models directly
        assert!(!app.available_models.is_empty());
    }

    #[test]
    fn test_app_select_providers_test_connection() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;

        app.provider_index = 9; // Test Connection
        app.select();
        // Should set a status message
        assert!(app.status_message.is_some());
    }

    #[test]
    fn test_app_select_context_back() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Context;

        app.context_index = 3; // Back
        app.select();
        assert_eq!(app.screen, Screen::MainMenu);
    }

    #[test]
    fn test_app_select_context_max_warm_chunks() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Context;

        app.context_index = 0; // MaxWarmChunks
        app.select();
        assert_eq!(app.input_mode, InputMode::Editing);
    }

    #[test]
    fn test_app_select_context_cold_retention() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Context;

        app.context_index = 1; // ColdRetentionDays
        app.select();
        assert_eq!(app.input_mode, InputMode::Editing);
    }

    #[test]
    fn test_app_select_context_auto_compact_toggle() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Context;

        let original = app.settings.context.auto_compact;
        app.context_index = 2; // AutoCompact
        app.select();
        assert_ne!(app.settings.context.auto_compact, original);
        assert!(app.settings_modified);
    }

    #[test]
    fn test_app_select_about_goes_back() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::About;

        app.select();
        assert_eq!(app.screen, Screen::MainMenu);
    }

    #[test]
    fn test_app_confirm_edit_api_key() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;
        app.provider_index = 1; // AnthropicApiKey

        app.start_editing("sk-test-key");
        app.confirm_edit();

        assert_eq!(
            app.settings.providers.anthropic.api_key,
            Some("sk-test-key".to_string())
        );
        assert!(app.settings_modified);
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_app_confirm_edit_api_key_empty_clears() {
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = Some("old-key".to_string());
        let mut app = App::new(settings);
        app.screen = Screen::Providers;
        app.provider_index = 1; // AnthropicApiKey

        app.start_editing("");
        app.confirm_edit();

        assert_eq!(app.settings.providers.anthropic.api_key, None);
    }

    #[test]
    fn test_app_confirm_edit_model() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;
        app.provider_index = 2; // AnthropicModel

        app.start_editing("claude-3-5-haiku-20241022");
        app.confirm_edit();

        assert_eq!(
            app.settings.providers.anthropic.default_model,
            "claude-3-5-haiku-20241022"
        );
        assert!(app.settings_modified);
    }

    #[test]
    fn test_app_confirm_edit_model_empty_ignored() {
        let settings = Settings::default();
        let original_model = settings.providers.anthropic.default_model.clone();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;
        app.provider_index = 2; // AnthropicModel

        app.start_editing("");
        app.confirm_edit();

        // Empty model should be ignored
        assert_eq!(
            app.settings.providers.anthropic.default_model,
            original_model
        );
    }

    #[test]
    fn test_app_confirm_edit_local_port() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;
        app.provider_index = 3; // LocalPort

        app.start_editing("9090");
        app.confirm_edit();

        assert_eq!(app.settings.providers.local.port, 9090);
        assert!(app.settings_modified);
    }

    #[test]
    fn test_app_confirm_edit_local_model() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;
        app.provider_index = 4; // LocalModel

        app.start_editing("llama3.2:latest");
        app.confirm_edit();

        assert_eq!(
            app.settings.providers.local.default_model,
            "llama3.2:latest"
        );
        assert!(app.settings_modified);
    }

    #[test]
    fn test_app_confirm_edit_max_warm_chunks() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Context;
        app.context_index = 0; // MaxWarmChunks

        app.start_editing("200");
        app.confirm_edit();

        assert_eq!(app.settings.context.max_warm_chunks, 200);
        assert!(app.settings_modified);
    }

    #[test]
    fn test_app_confirm_edit_max_warm_chunks_invalid() {
        let settings = Settings::default();
        let original = settings.context.max_warm_chunks;
        let mut app = App::new(settings);
        app.screen = Screen::Context;
        app.context_index = 0; // MaxWarmChunks

        app.start_editing("not a number");
        app.confirm_edit();

        // Should not change and should show error
        assert_eq!(app.settings.context.max_warm_chunks, original);
        assert!(app.status_is_error);
    }

    #[test]
    fn test_app_confirm_edit_cold_retention() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Context;
        app.context_index = 1; // ColdRetentionDays

        app.start_editing("60");
        app.confirm_edit();

        assert_eq!(app.settings.context.cold_retention_days, 60);
        assert!(app.settings_modified);
    }

    #[test]
    fn test_app_confirm_edit_cold_retention_invalid() {
        let settings = Settings::default();
        let original = settings.context.cold_retention_days;
        let mut app = App::new(settings);
        app.screen = Screen::Context;
        app.context_index = 1; // ColdRetentionDays

        app.start_editing("invalid");
        app.confirm_edit();

        assert_eq!(app.settings.context.cold_retention_days, original);
        assert!(app.status_is_error);
    }

    #[test]
    fn test_app_toggle_cap() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        // Ensure we have at least one cap
        if !app.available_caps.is_empty() {
            let original_enabled = app.available_caps[0].is_enabled;
            app.toggle_cap(0);
            assert_ne!(app.available_caps[0].is_enabled, original_enabled);
            assert!(app.settings_modified);
        }
    }

    #[test]
    fn test_app_toggle_cap_updates_settings() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        if !app.available_caps.is_empty() {
            let cap_name = app.available_caps[0].name.clone();
            let was_in_defaults = app.settings.defaults.caps.contains(&cap_name);

            app.toggle_cap(0);

            let is_in_defaults = app.settings.defaults.caps.contains(&cap_name);
            // Should have toggled
            if was_in_defaults {
                assert!(!is_in_defaults);
            } else {
                assert!(is_in_defaults);
            }
        }
    }

    #[test]
    fn test_app_toggle_cap_invalid_index() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        // Toggle an invalid index - should not panic
        app.toggle_cap(9999);
    }

    #[test]
    fn test_app_refresh_caps() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        // Should not panic
        app.refresh_caps();
        // available_caps should still be populated
        assert!(!app.available_caps.is_empty());
    }

    #[test]
    fn test_app_go_to_clears_status() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.set_status("Some status", false);
        app.go_to(Screen::Providers);
        assert!(app.status_message.is_none());
    }

    #[test]
    fn test_app_select_caps_back() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Caps;

        // Back is the last item
        app.caps_index = app.caps_total_items() - 1;
        app.select();
        assert_eq!(app.screen, Screen::MainMenu);
    }

    #[test]
    fn test_app_select_caps_create_new() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Caps;

        // Create New is second to last
        app.caps_index = app.available_caps.len();
        app.select();
        assert_eq!(app.input_mode, InputMode::Editing);
        assert!(app.input_buffer.is_empty());
    }

    #[test]
    fn test_app_provider_index_wraps() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;

        app.provider_index = ProviderItem::all().len() - 1;
        app.move_down();
        assert_eq!(app.provider_index, 0);
    }

    #[test]
    fn test_app_context_index_wraps() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Context;

        app.context_index = ContextItem::all().len() - 1;
        app.move_down();
        assert_eq!(app.context_index, 0);
    }

    #[test]
    fn test_app_caps_index_wraps() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Caps;

        app.caps_index = app.caps_total_items() - 1;
        app.move_down();
        assert_eq!(app.caps_index, 0);
    }

    // ===== Model Selection Tests =====

    #[test]
    fn test_cancel_model_selection_clears_state() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        // Set up as if we're in the middle of model selection
        app.model_selection_target = Some(ModelSelectionTarget::Local);
        app.input_mode = InputMode::SelectingModel;

        // Cancel
        app.cancel_model_selection();

        // Should have cleared everything
        assert!(app.model_selection_target.is_none());
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_start_model_selection_local_uses_registry() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.start_model_selection(ModelSelectionTarget::Local);

        // Should have used registry models directly
        assert!(!app.available_models.is_empty());
        assert_eq!(app.input_mode, InputMode::SelectingModel);
    }

    #[test]
    fn test_start_model_selection_anthropic_uses_registry() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.start_model_selection(ModelSelectionTarget::Anthropic);

        // Should have used registry models
        assert!(!app.available_models.is_empty());
        assert_eq!(app.input_mode, InputMode::SelectingModel);
        assert_eq!(
            app.model_selection_target,
            Some(ModelSelectionTarget::Anthropic)
        );
    }

    #[test]
    fn test_start_model_selection_finds_current_model() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        // First, see what models are available and pick one
        app.start_model_selection(ModelSelectionTarget::Anthropic);
        let available_model = app.available_models[1].id.clone(); // Pick the second model
        app.cancel_model_selection();

        // Now set that model as the default
        app.settings.providers.anthropic.default_model = available_model.clone();

        // Start selection again
        app.start_model_selection(ModelSelectionTarget::Anthropic);

        // Should have found the current model at index 1
        let selected_model = &app.available_models[app.model_picker_index];
        assert_eq!(selected_model.id, available_model);
        assert_eq!(app.model_picker_index, 1);
    }

    // ===== Plan Management Tests =====

    #[test]
    fn test_plans_total_items() {
        let settings = Settings::default();
        let app = App::new(settings);
        // Total = plans count + 1 (Back button)
        assert_eq!(app.plans_total_items(), app.available_plans.len() + 1);
    }

    #[test]
    fn test_refresh_plans() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        // Should not panic
        app.refresh_plans();
    }

    #[test]
    fn test_view_plan_nonexistent() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        let fake_id = uuid::Uuid::new_v4();

        // Should not panic, should stay on current screen
        let original_screen = app.screen;
        app.view_plan(fake_id);
        // If plan doesn't exist, screen should not change
        assert_eq!(app.screen, original_screen);
    }

    #[test]
    fn test_set_plan_status_nonexistent() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        let fake_id = uuid::Uuid::new_v4();

        // Should not panic
        app.set_plan_status(fake_id, PlanStatus::Active);
    }

    #[test]
    fn test_delete_plan_nonexistent() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        let fake_id = uuid::Uuid::new_v4();

        // Should not panic
        app.delete_plan(fake_id);
    }

    #[test]
    fn test_edit_plan_nonexistent() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        let fake_id = uuid::Uuid::new_v4();

        let original_screen = app.screen;
        app.edit_plan(fake_id);
        // If plan doesn't exist, screen should not change
        assert_eq!(app.screen, original_screen);
    }

    // ===== Editor Tests =====

    #[test]
    fn test_editor_mode_none() {
        let settings = Settings::default();
        let app = App::new(settings);
        assert!(app.editor_mode().is_none());
    }

    #[test]
    fn test_editor_mode_some() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.editor = Some(Editor::new("test content"));

        assert!(app.editor_mode().is_some());
        assert_eq!(app.editor_mode().unwrap(), EditorMode::Normal);
    }

    #[test]
    fn test_editor_modified_no_editor() {
        let settings = Settings::default();
        let app = App::new(settings);
        assert!(!app.editor_modified());
    }

    #[test]
    fn test_editor_modified_unmodified() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.editor = Some(Editor::new("test content"));

        assert!(!app.editor_modified());
    }

    #[test]
    fn test_save_editor_no_plan_id() {
        use crate::tui::editor::Editor;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.editor = Some(Editor::new("test content"));
        app.current_plan_id = None;

        // Should fail and set error status
        let result = app.save_editor();
        assert!(!result);
        assert!(app.status_is_error);
    }

    #[test]
    fn test_save_editor_no_editor() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.current_plan_id = Some(uuid::Uuid::new_v4());
        app.editor = None;

        // Should fail and set error status
        let result = app.save_editor();
        assert!(!result);
        assert!(app.status_is_error);
    }

    #[test]
    fn test_handle_editor_command_continue() {
        use crate::tui::editor::CommandResult;

        let settings = Settings::default();
        let mut app = App::new(settings);

        let result = app.handle_editor_command(CommandResult::Continue);
        assert!(matches!(result, AppResult::Continue));
    }

    #[test]
    fn test_handle_editor_command_quit() {
        use crate::tui::editor::CommandResult;

        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;

        let result = app.handle_editor_command(CommandResult::Quit);
        assert!(matches!(result, AppResult::Continue));
        assert!(app.editor.is_none());
        assert_eq!(app.screen, Screen::Plans);
    }

    #[test]
    fn test_handle_editor_command_invalid() {
        use crate::tui::editor::CommandResult;

        let settings = Settings::default();
        let mut app = App::new(settings);

        let result =
            app.handle_editor_command(CommandResult::Invalid("Unknown command".to_string()));
        assert!(matches!(result, AppResult::Continue));
        assert!(app.status_is_error);
        assert!(app
            .status_message
            .as_ref()
            .unwrap()
            .contains("Unknown command"));
    }

    // ===== Plan View Navigation Tests =====

    #[test]
    fn test_move_up_plan_view_scrolls() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanView;
        app.plan_scroll = 5;

        app.move_up();
        assert_eq!(app.plan_scroll, 4);
    }

    #[test]
    fn test_move_up_plan_view_at_top() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanView;
        app.plan_scroll = 0;

        app.move_up();
        assert_eq!(app.plan_scroll, 0); // Should not go negative
    }

    #[test]
    fn test_move_down_plan_view_scrolls() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanView;
        app.plan_scroll = 0;

        app.move_down();
        assert_eq!(app.plan_scroll, 1);
    }

    #[test]
    fn test_move_up_plans() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Plans;
        app.plans_index = 0;

        app.move_up();
        // Should wrap to last item
        assert_eq!(app.plans_index, app.plans_total_items() - 1);
    }

    #[test]
    fn test_move_down_plans() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Plans;
        app.plans_index = 0;

        app.move_down();
        if app.plans_total_items() > 1 {
            assert_eq!(app.plans_index, 1);
        }
    }

    #[test]
    fn test_move_down_plans_wraps() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Plans;
        app.plans_index = app.plans_total_items() - 1;

        app.move_down();
        assert_eq!(app.plans_index, 0);
    }

    #[test]
    fn test_select_plan_view_goes_back() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanView;

        app.select();
        assert_eq!(app.screen, Screen::Plans);
    }

    #[test]
    fn test_select_plans_back() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Plans;
        // Select "Back" which is at the end
        app.plans_index = app.available_plans.len();

        app.select();
        assert_eq!(app.screen, Screen::MainMenu);
    }

    // ===== create_live_model_info Tests =====

    #[test]
    fn test_create_live_model_info_unknown_model() {
        let settings = Settings::default();
        let app = App::new(settings);

        let info = app.create_live_model_info("totally-unknown-model:latest");

        assert_eq!(info.id, "totally-unknown-model:latest");
        assert_eq!(info.name, "totally-unknown-model:latest");
        assert_eq!(info.tier, "Unknown");
        assert!(!info.recommended);
    }

    #[test]
    fn test_create_live_model_info_known_model() {
        let settings = Settings::default();
        let app = App::new(settings);

        // qwen2.5-coder:14b should be in the registry
        let info = app.create_live_model_info("qwen2.5-coder:14b");

        assert_eq!(info.id, "qwen2.5-coder:14b");
        // Should have metadata from registry
        assert!(!info.description.is_empty() || info.tier != "Unknown");
    }

    // ===== Model Selection Confirmation Tests =====

    #[test]
    fn test_confirm_model_selection_anthropic() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.start_model_selection(ModelSelectionTarget::Anthropic);
        assert!(!app.available_models.is_empty());

        // Select a model
        app.model_picker_index = 0;
        let expected_model = app.available_models[0].id.clone();

        app.confirm_model_selection();

        assert_eq!(
            app.settings.providers.anthropic.default_model,
            expected_model
        );
        assert!(app.settings_modified);
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_confirm_model_selection_local() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.start_model_selection(ModelSelectionTarget::Local);
        assert!(!app.available_models.is_empty());

        app.model_picker_index = 0;
        let expected_model = app.available_models[0].id.clone();

        app.confirm_model_selection();

        assert_eq!(app.settings.providers.local.default_model, expected_model);
        assert!(app.settings_modified);
    }

    #[test]
    fn test_confirm_model_selection_openrouter() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.start_model_selection(ModelSelectionTarget::OpenRouter);
        if !app.available_models.is_empty() {
            app.model_picker_index = 0;
            let expected_model = app.available_models[0].id.clone();

            app.confirm_model_selection();

            assert_eq!(
                app.settings.providers.openrouter.default_model,
                expected_model
            );
        }
    }

    #[test]
    fn test_confirm_model_selection_blackman() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.start_model_selection(ModelSelectionTarget::Blackman);
        if !app.available_models.is_empty() {
            app.model_picker_index = 0;
            let expected_model = app.available_models[0].id.clone();

            app.confirm_model_selection();

            assert_eq!(
                app.settings.providers.blackman.default_model,
                expected_model
            );
        }
    }

    #[test]
    fn test_confirm_model_selection_no_target() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.model_selection_target = None;
        app.available_models = vec![ModelDisplayInfo {
            id: "test".to_string(),
            name: "Test".to_string(),
            tier: "Standard".to_string(),
            description: "".to_string(),
            recommended: false,
        }];

        // Should not panic
        app.confirm_model_selection();
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_confirm_model_selection_empty_models() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.model_selection_target = Some(ModelSelectionTarget::Anthropic);
        app.available_models = vec![];

        // Should not panic
        app.confirm_model_selection();
    }

    // ===== Model Picker Navigation Tests =====

    #[test]
    fn test_model_picker_up_not_at_top() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.available_models = vec![
            ModelDisplayInfo {
                id: "m1".to_string(),
                name: "M1".to_string(),
                tier: "S".to_string(),
                description: "".to_string(),
                recommended: false,
            },
            ModelDisplayInfo {
                id: "m2".to_string(),
                name: "M2".to_string(),
                tier: "S".to_string(),
                description: "".to_string(),
                recommended: false,
            },
        ];
        app.model_picker_index = 1;

        app.model_picker_up();
        assert_eq!(app.model_picker_index, 0);
    }

    #[test]
    fn test_model_picker_up_at_top_wraps() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.available_models = vec![
            ModelDisplayInfo {
                id: "m1".to_string(),
                name: "M1".to_string(),
                tier: "S".to_string(),
                description: "".to_string(),
                recommended: false,
            },
            ModelDisplayInfo {
                id: "m2".to_string(),
                name: "M2".to_string(),
                tier: "S".to_string(),
                description: "".to_string(),
                recommended: false,
            },
        ];
        app.model_picker_index = 0;

        app.model_picker_up();
        assert_eq!(app.model_picker_index, 1); // Wraps to last
    }

    #[test]
    fn test_model_picker_up_empty() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.available_models = vec![];
        app.model_picker_index = 0;

        app.model_picker_up();
        assert_eq!(app.model_picker_index, 0); // No change
    }

    #[test]
    fn test_model_picker_down_not_at_bottom() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.available_models = vec![
            ModelDisplayInfo {
                id: "m1".to_string(),
                name: "M1".to_string(),
                tier: "S".to_string(),
                description: "".to_string(),
                recommended: false,
            },
            ModelDisplayInfo {
                id: "m2".to_string(),
                name: "M2".to_string(),
                tier: "S".to_string(),
                description: "".to_string(),
                recommended: false,
            },
        ];
        app.model_picker_index = 0;

        app.model_picker_down();
        assert_eq!(app.model_picker_index, 1);
    }

    #[test]
    fn test_model_picker_down_at_bottom_wraps() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.available_models = vec![
            ModelDisplayInfo {
                id: "m1".to_string(),
                name: "M1".to_string(),
                tier: "S".to_string(),
                description: "".to_string(),
                recommended: false,
            },
            ModelDisplayInfo {
                id: "m2".to_string(),
                name: "M2".to_string(),
                tier: "S".to_string(),
                description: "".to_string(),
                recommended: false,
            },
        ];
        app.model_picker_index = 1;

        app.model_picker_down();
        assert_eq!(app.model_picker_index, 0); // Wraps to first
    }

    #[test]
    fn test_model_picker_down_empty() {
        let settings = Settings::default();
        let mut app = App::new(settings);

        app.available_models = vec![];
        app.model_picker_index = 0;

        app.model_picker_down();
        assert_eq!(app.model_picker_index, 0); // No change
    }

    // ===== Provider Navigation Edge Cases =====

    #[test]
    fn test_move_up_providers_not_at_top() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::Providers;
        app.provider_index = 2;

        app.move_up();
        assert_eq!(app.provider_index, 1);
    }

    // ===== PlanEdit Navigation =====

    #[test]
    fn test_move_up_plan_edit_does_nothing() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;

        // Should not panic - handled by editor
        app.move_up();
    }

    #[test]
    fn test_move_down_plan_edit_does_nothing() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;

        // Should not panic - handled by editor
        app.move_down();
    }

    #[test]
    fn test_select_plan_edit_does_nothing() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.screen = Screen::PlanEdit;

        // Should not panic - handled by editor (insert newline)
        app.select();
        assert_eq!(app.screen, Screen::PlanEdit);
    }

    // ===== Connection Test Tests =====

    #[test]
    fn test_connection_test_fields_initialized() {
        let settings = Settings::default();
        let app = App::new(settings);
        assert!(!app.testing_connection);
        assert!(app.connection_test_rx.is_none());
    }

    #[test]
    fn test_start_connection_test_local_provider() {
        let mut settings = Settings::default();
        settings.defaults.provider = "local".to_string();
        let mut app = App::new(settings);

        app.start_connection_test();

        // Should set a status message about local provider configuration
        assert!(app.status_message.is_some());
    }

    #[test]
    fn test_start_connection_test_anthropic_with_key() {
        let mut settings = Settings::default();
        settings.defaults.provider = "anthropic".to_string();
        settings.providers.anthropic.api_key = Some("sk-test-key".to_string());
        let mut app = App::new(settings);

        app.start_connection_test();

        assert!(!app.status_is_error);
        assert!(app.status_message.as_ref().unwrap().contains("configured"));
    }

    #[test]
    fn test_start_connection_test_anthropic_no_key() {
        let mut settings = Settings::default();
        settings.defaults.provider = "anthropic".to_string();
        settings.providers.anthropic.api_key = None;
        let mut app = App::new(settings);

        app.start_connection_test();

        assert!(app.status_is_error);
        assert!(app
            .status_message
            .as_ref()
            .unwrap()
            .contains("No Anthropic API key"));
    }

    #[test]
    fn test_start_connection_test_unknown_provider() {
        let mut settings = Settings::default();
        settings.defaults.provider = "unknown".to_string();
        let mut app = App::new(settings);

        app.start_connection_test();

        assert!(app.status_is_error);
        assert!(app
            .status_message
            .as_ref()
            .unwrap()
            .contains("not available"));
    }

    #[test]
    fn test_check_connection_test_results_no_receiver() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.connection_test_rx = None;

        // Should not panic
        app.check_connection_test_results();
    }

    #[test]
    fn test_check_connection_test_results_success() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.testing_connection = true;

        // Create a channel and send success
        let (tx, rx) = mpsc::channel();
        app.connection_test_rx = Some(rx);
        tx.send(Ok("Connected successfully!".to_string())).unwrap();

        app.check_connection_test_results();

        assert!(!app.testing_connection);
        assert!(app.connection_test_rx.is_none());
        assert!(!app.status_is_error);
        assert!(app.status_message.as_ref().unwrap().contains("Connected"));
    }

    #[test]
    fn test_check_connection_test_results_error() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.testing_connection = true;

        // Create a channel and send error
        let (tx, rx) = mpsc::channel();
        app.connection_test_rx = Some(rx);
        tx.send(Err("Connection refused".to_string())).unwrap();

        app.check_connection_test_results();

        assert!(!app.testing_connection);
        assert!(app.connection_test_rx.is_none());
        assert!(app.status_is_error);
        assert!(app
            .status_message
            .as_ref()
            .unwrap()
            .contains("Connection refused"));
    }

    #[test]
    fn test_check_connection_test_results_empty() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.testing_connection = true;

        // Create a channel but don't send anything
        let (_tx, rx) = mpsc::channel::<Result<String, String>>();
        app.connection_test_rx = Some(rx);

        app.check_connection_test_results();

        // Should still be testing
        assert!(app.testing_connection);
        assert!(app.connection_test_rx.is_some());
    }

    #[test]
    fn test_check_connection_test_results_disconnected() {
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.testing_connection = true;

        // Create a channel and drop sender
        let (tx, rx) = mpsc::channel::<Result<String, String>>();
        app.connection_test_rx = Some(rx);
        drop(tx);

        app.check_connection_test_results();

        assert!(!app.testing_connection);
        assert!(app.connection_test_rx.is_none());
        assert!(app.status_is_error);
        assert!(app.status_message.as_ref().unwrap().contains("interrupted"));
    }

    #[test]
    fn test_select_providers_test_connection_calls_method() {
        let mut settings = Settings::default();
        settings.defaults.provider = "anthropic".to_string();
        settings.providers.anthropic.api_key = Some("test-key".to_string());
        let mut app = App::new(settings);
        app.screen = Screen::Providers;

        // Find TestConnection index
        let test_conn_index = ProviderItem::all()
            .iter()
            .position(|&p| p == ProviderItem::TestConnection)
            .unwrap();
        app.provider_index = test_conn_index;

        app.select();

        // Should have set a status message
        assert!(app.status_message.is_some());
    }
}
