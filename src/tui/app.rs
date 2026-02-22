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
mod tests;
