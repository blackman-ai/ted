// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use crate::config::Settings;

use super::ChatTuiConfig;

/// Discover locally available GGUF models from all standard locations
fn discover_local_models() -> Vec<String> {
    let discovered = crate::models::scanner::scan_for_models();

    if discovered.is_empty() {
        return vec!["(no models - use /model download)".to_string()];
    }

    discovered
        .into_iter()
        .filter_map(|m| {
            m.path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .collect()
}

/// Settings section tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    General,
    Capabilities,
}

impl SettingsSection {
    pub(super) fn all() -> &'static [SettingsSection] {
        &[SettingsSection::General, SettingsSection::Capabilities]
    }

    pub(super) fn label(&self) -> &'static str {
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
    pub(super) fn all() -> &'static [SettingsField] {
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

    pub(super) fn label(&self) -> &'static str {
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
            "local".to_string(),
            "openrouter".to_string(),
            "blackman".to_string(),
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
        // Don't allow editing API key for local (not needed)
        if self.selected_field() == SettingsField::ApiKey && self.provider == "local" {
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
                if self.provider == "local" {
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
