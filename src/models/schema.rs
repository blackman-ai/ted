// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Model registry schema
//!
//! Defines the structure for model metadata including tier classification,
//! hardware requirements, and capabilities.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Model tier classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    /// High-end models (best quality, highest cost/resources)
    High,
    /// Mid-range models (good balance)
    Medium,
    /// Budget/fast models (lower quality, faster/cheaper)
    Low,
}

impl ModelTier {
    /// Get display name for the tier
    pub fn display_name(&self) -> &'static str {
        match self {
            ModelTier::High => "High",
            ModelTier::Medium => "Medium",
            ModelTier::Low => "Low",
        }
    }
}

/// Provider type for model categorization
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    Ollama,
    OpenRouter,
    Blackman,
}

impl Provider {
    /// Get display name for the provider
    pub fn display_name(&self) -> &'static str {
        match self {
            Provider::Anthropic => "Anthropic",
            Provider::Ollama => "Ollama",
            Provider::OpenRouter => "OpenRouter",
            Provider::Blackman => "Blackman",
        }
    }
}

impl FromStr for Provider {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "anthropic" => Ok(Provider::Anthropic),
            "ollama" => Ok(Provider::Ollama),
            "openrouter" => Ok(Provider::OpenRouter),
            "blackman" => Ok(Provider::Blackman),
            _ => Err(format!("Unknown provider: {}", s)),
        }
    }
}

/// Model definition with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier (e.g., "claude-sonnet-4-20250514", "qwen2.5-coder:14b")
    pub id: String,

    /// Human-readable display name
    #[serde(default)]
    pub name: String,

    /// Quality/cost tier
    pub tier: ModelTier,

    /// Maximum context window in tokens
    #[serde(default)]
    pub context_size: Option<u32>,

    /// VRAM required in GB (for local models)
    #[serde(default)]
    pub vram_gb: Option<f32>,

    /// Whether this model supports tool use
    #[serde(default = "default_true")]
    pub supports_tools: bool,

    /// Whether this model supports vision/images
    #[serde(default)]
    pub supports_vision: bool,

    /// Short description of the model
    #[serde(default)]
    pub description: String,

    /// Whether this is a recommended/featured model
    #[serde(default)]
    pub recommended: bool,
}

fn default_true() -> bool {
    true
}

impl ModelInfo {
    /// Create a new model info
    pub fn new(id: impl Into<String>, tier: ModelTier) -> Self {
        let id = id.into();
        Self {
            name: id.clone(),
            id,
            tier,
            context_size: None,
            vram_gb: None,
            supports_tools: true,
            supports_vision: false,
            description: String::new(),
            recommended: false,
        }
    }

    /// Builder: set display name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Builder: set context size
    pub fn with_context(mut self, tokens: u32) -> Self {
        self.context_size = Some(tokens);
        self
    }

    /// Builder: set VRAM requirement
    pub fn with_vram(mut self, gb: f32) -> Self {
        self.vram_gb = Some(gb);
        self
    }

    /// Builder: set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Builder: mark as recommended
    pub fn recommended(mut self) -> Self {
        self.recommended = true;
        self
    }

    /// Builder: enable vision support
    pub fn with_vision(mut self) -> Self {
        self.supports_vision = true;
        self
    }

    /// Get display name (falls back to id)
    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            &self.id
        } else {
            &self.name
        }
    }
}

/// Provider model list in config file
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderModels {
    /// List of models for this provider
    #[serde(default)]
    pub models: Vec<ModelInfo>,
}

/// Root config file structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelsConfig {
    #[serde(default)]
    pub anthropic: ProviderModels,

    #[serde(default)]
    pub ollama: ProviderModels,

    #[serde(default)]
    pub openrouter: ProviderModels,

    #[serde(default)]
    pub blackman: ProviderModels,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_tier_display() {
        assert_eq!(ModelTier::High.display_name(), "High");
        assert_eq!(ModelTier::Medium.display_name(), "Medium");
        assert_eq!(ModelTier::Low.display_name(), "Low");
    }

    #[test]
    fn test_provider_from_str() {
        assert_eq!(Provider::from_str("anthropic"), Ok(Provider::Anthropic));
        assert_eq!(Provider::from_str("OLLAMA"), Ok(Provider::Ollama));
        assert_eq!(Provider::from_str("OpenRouter"), Ok(Provider::OpenRouter));
        assert!(Provider::from_str("unknown").is_err());
    }

    #[test]
    fn test_model_info_builder() {
        let model = ModelInfo::new("claude-sonnet-4", ModelTier::High)
            .with_name("Claude Sonnet 4")
            .with_context(200000)
            .with_description("Latest Sonnet model")
            .recommended();

        assert_eq!(model.id, "claude-sonnet-4");
        assert_eq!(model.name, "Claude Sonnet 4");
        assert_eq!(model.tier, ModelTier::High);
        assert_eq!(model.context_size, Some(200000));
        assert!(model.recommended);
    }

    #[test]
    fn test_model_info_display_name() {
        let with_name = ModelInfo::new("test-id", ModelTier::Low).with_name("Test Model");
        assert_eq!(with_name.display_name(), "Test Model");

        let without_name = ModelInfo::new("test-id", ModelTier::Low);
        assert_eq!(without_name.display_name(), "test-id");
    }

    #[test]
    fn test_models_config_serde() {
        let toml = r#"
[anthropic]
models = [
    { id = "claude-sonnet-4", tier = "high", context_size = 200000 }
]

[ollama]
models = [
    { id = "qwen2.5-coder:14b", tier = "medium", vram_gb = 12.0 }
]
"#;

        let config: ModelsConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.anthropic.models.len(), 1);
        assert_eq!(config.anthropic.models[0].id, "claude-sonnet-4");
        assert_eq!(config.ollama.models.len(), 1);
        assert_eq!(config.ollama.models[0].vram_gb, Some(12.0));
    }
}
