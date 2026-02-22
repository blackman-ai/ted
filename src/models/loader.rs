// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Model registry loader
//!
//! Loads model definitions from:
//! 1. Built-in defaults (always available)
//! 2. User config file (`~/.ted/models.toml`) for overrides/additions

use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::Result;

use super::schema::{ModelInfo, ModelTier, ModelsConfig, Provider, ProviderModels};

/// Model registry with all known models
#[derive(Debug, Clone)]
pub struct ModelRegistry {
    /// Models indexed by provider
    models: HashMap<Provider, Vec<ModelInfo>>,
    /// Path to user config file (if loaded)
    config_path: Option<PathBuf>,
}

impl ModelRegistry {
    /// Create a new registry with built-in defaults
    pub fn new() -> Self {
        let mut registry = Self {
            models: HashMap::new(),
            config_path: None,
        };

        // Load built-in defaults
        registry.load_defaults();

        // Try to load user config
        if let Some(config_path) = Self::default_config_path() {
            if config_path.exists() {
                if let Err(e) = registry.load_from_file(&config_path) {
                    tracing::warn!("Failed to load models.toml: {}", e);
                }
            }
        }

        registry
    }

    /// Create registry with only built-in defaults (no user config)
    pub fn with_defaults_only() -> Self {
        let mut registry = Self {
            models: HashMap::new(),
            config_path: None,
        };
        registry.load_defaults();
        registry
    }

    /// Get the default config file path
    pub fn default_config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".ted/models.toml"))
    }

    /// Load built-in default models
    fn load_defaults(&mut self) {
        // Anthropic models (updated late 2025)
        self.models.insert(
            Provider::Anthropic,
            vec![
                // Latest 4.5 series
                ModelInfo::new("claude-opus-4-5-20251124", ModelTier::High)
                    .with_name("Claude Opus 4.5")
                    .with_context(200000)
                    .with_description("Newest flagship - best for multi-day projects")
                    .with_vision(),
                ModelInfo::new("claude-sonnet-4-20250514", ModelTier::High)
                    .with_name("Claude Sonnet 4")
                    .with_context(200000)
                    .with_description("Fast and capable coding model")
                    .with_vision()
                    .recommended(),
                ModelInfo::new("claude-haiku-4-5-20251022", ModelTier::Low)
                    .with_name("Claude Haiku 4.5")
                    .with_context(200000)
                    .with_description("Fastest, matches Sonnet 4 at 1/3 cost")
                    .with_vision(),
                // 4.x series (still available)
                ModelInfo::new("claude-opus-4-1-20250805", ModelTier::High)
                    .with_name("Claude Opus 4.1")
                    .with_context(200000)
                    .with_description("Deep thinker for complex code review")
                    .with_vision(),
            ],
        );

        // Local models (GGUF via llama-server) - updated late 2025
        self.models.insert(
            Provider::Local,
            vec![
                // High tier - requires significant VRAM (24GB+)
                ModelInfo::new("qwen2.5-coder:72b", ModelTier::High)
                    .with_name("Qwen 2.5 Coder 72B")
                    .with_vram(48.0)
                    .with_context(32768)
                    .with_description("Best open-source coding model"),
                ModelInfo::new("deepseek-coder-v2:latest", ModelTier::High)
                    .with_name("DeepSeek Coder V2")
                    .with_vram(24.0)
                    .with_context(128000)
                    .with_description("MoE model, rivals GPT-4 Turbo"),
                ModelInfo::new("qwen2.5-coder:32b", ModelTier::High)
                    .with_name("Qwen 2.5 Coder 32B")
                    .with_vram(24.0)
                    .with_context(32768)
                    .with_description("Best for 40+ languages, rivals GPT-4o")
                    .recommended(),
                ModelInfo::new("deepseek-r1:32b", ModelTier::High)
                    .with_name("DeepSeek R1 32B")
                    .with_vram(24.0)
                    .with_context(64000)
                    .with_description("Strong reasoning capabilities"),
                ModelInfo::new("codellama:34b", ModelTier::High)
                    .with_name("Code Llama 34B")
                    .with_vram(24.0)
                    .with_context(16384)
                    .with_description("Meta's coding-focused model"),
                // Medium tier (8-16GB VRAM)
                ModelInfo::new("qwen2.5-coder:14b", ModelTier::Medium)
                    .with_name("Qwen 2.5 Coder 14B")
                    .with_vram(12.0)
                    .with_context(32768)
                    .with_description("Great balance for coding tasks"),
                ModelInfo::new("deepseek-coder:16b", ModelTier::Medium)
                    .with_name("DeepSeek Coder 16B")
                    .with_vram(12.0)
                    .with_context(16384)
                    .with_description("Strong coding specialist"),
                ModelInfo::new("qwen2.5-coder:7b", ModelTier::Medium)
                    .with_name("Qwen 2.5 Coder 7B")
                    .with_vram(6.0)
                    .with_context(32768)
                    .with_description("Good for laptops with discrete GPU"),
                ModelInfo::new("deepseek-coder:6.7b", ModelTier::Medium)
                    .with_name("DeepSeek Coder 6.7B")
                    .with_vram(6.0)
                    .with_context(16384)
                    .with_description("Efficient coding model"),
                ModelInfo::new("codellama:13b", ModelTier::Medium)
                    .with_name("Code Llama 13B")
                    .with_vram(10.0)
                    .with_context(16384)
                    .with_description("Balanced Code Llama"),
                ModelInfo::new("codellama:7b", ModelTier::Medium)
                    .with_name("Code Llama 7B")
                    .with_vram(6.0)
                    .with_context(16384)
                    .with_description("Compact coding model"),
                // Low tier - runs on most hardware (4-8GB)
                ModelInfo::new("qwen2.5-coder:3b", ModelTier::Low)
                    .with_name("Qwen 2.5 Coder 3B")
                    .with_vram(3.0)
                    .with_context(32768)
                    .with_description("Lightweight for older hardware"),
                ModelInfo::new("qwen3:4b", ModelTier::Low)
                    .with_name("Qwen3 4B")
                    .with_vram(4.0)
                    .with_context(40960)
                    .with_description("Agentic small model for constrained systems"),
                ModelInfo::new("qwen2.5-coder:1.5b", ModelTier::Low)
                    .with_name("Qwen 2.5 Coder 1.5B")
                    .with_vram(2.0)
                    .with_context(32768)
                    .with_description("Minimal - for Raspberry Pi/ARM"),
                ModelInfo::new("deepseek-coder:1.3b", ModelTier::Low)
                    .with_name("DeepSeek Coder 1.3B")
                    .with_vram(2.0)
                    .with_context(16384)
                    .with_description("Ultra-lightweight coding"),
                ModelInfo::new("phi3:mini", ModelTier::Low)
                    .with_name("Phi-3 Mini")
                    .with_vram(2.5)
                    .with_context(4096)
                    .with_description("Microsoft's small model"),
            ],
        );

        // OpenRouter models (aggregator) - updated late 2025
        self.models.insert(
            Provider::OpenRouter,
            vec![
                // High tier
                ModelInfo::new("anthropic/claude-sonnet-4.5", ModelTier::High)
                    .with_name("Claude Sonnet 4.5")
                    .with_context(1000000)
                    .with_description("Best coding model, 1M context")
                    .with_vision()
                    .recommended(),
                ModelInfo::new("anthropic/claude-opus-4.5", ModelTier::High)
                    .with_name("Claude Opus 4.5")
                    .with_context(200000)
                    .with_description("Newest flagship")
                    .with_vision(),
                ModelInfo::new("openai/gpt-5", ModelTier::High)
                    .with_name("GPT-5")
                    .with_context(128000)
                    .with_description("OpenAI's latest (BYOK)")
                    .with_vision(),
                ModelInfo::new("openai/gpt-4o", ModelTier::High)
                    .with_name("GPT-4o")
                    .with_context(128000)
                    .with_description("OpenAI multimodal flagship")
                    .with_vision(),
                ModelInfo::new("google/gemini-2.0-flash", ModelTier::High)
                    .with_name("Gemini 2.0 Flash")
                    .with_context(1000000)
                    .with_description("Fast with huge context")
                    .with_vision(),
                ModelInfo::new("google/gemini-3-flash-preview", ModelTier::High)
                    .with_name("Gemini 3 Flash Preview")
                    .with_context(1000000)
                    .with_description("Latest Gemini, strong reasoning")
                    .with_vision(),
                // Medium tier
                ModelInfo::new("anthropic/claude-haiku-4.5", ModelTier::Medium)
                    .with_name("Claude Haiku 4.5")
                    .with_context(200000)
                    .with_description("Fast, matches Sonnet 4")
                    .with_vision(),
                ModelInfo::new("anthropic/claude-sonnet-4", ModelTier::Medium)
                    .with_name("Claude Sonnet 4")
                    .with_context(200000)
                    .with_description("Reliable workhorse")
                    .with_vision(),
                ModelInfo::new("deepseek/deepseek-v3", ModelTier::Medium)
                    .with_name("DeepSeek V3")
                    .with_context(64000)
                    .with_description("Strong performance, very cheap"),
                ModelInfo::new("openai/gpt-4o-mini", ModelTier::Medium)
                    .with_name("GPT-4o Mini")
                    .with_context(128000)
                    .with_description("Cheaper GPT-4o")
                    .with_vision(),
                ModelInfo::new("google/gemini-2.0-flash-lite", ModelTier::Medium)
                    .with_name("Gemini 2.0 Flash Lite")
                    .with_context(1000000)
                    .with_description("Fast TTFT, very cheap")
                    .with_vision(),
                // Low tier
                ModelInfo::new("mistralai/mistral-small", ModelTier::Low)
                    .with_name("Mistral Small")
                    .with_context(32000)
                    .with_description("Budget-friendly"),
                ModelInfo::new("openai/gpt-5-mini", ModelTier::Low)
                    .with_name("GPT-5 Mini")
                    .with_context(128000)
                    .with_description("Cheaper GPT-5")
                    .with_vision(),
            ],
        );

        // Blackman models (mirrors Anthropic) - updated late 2025
        self.models.insert(
            Provider::Blackman,
            vec![
                ModelInfo::new("claude-sonnet-4-20250514", ModelTier::High)
                    .with_name("Claude Sonnet 4")
                    .with_context(200000)
                    .with_description("Fast and capable coding model")
                    .with_vision()
                    .recommended(),
                ModelInfo::new("claude-opus-4-5-20251124", ModelTier::High)
                    .with_name("Claude Opus 4.5")
                    .with_context(200000)
                    .with_description("Newest flagship")
                    .with_vision(),
                ModelInfo::new("claude-haiku-4-5-20251022", ModelTier::Low)
                    .with_name("Claude Haiku 4.5")
                    .with_context(200000)
                    .with_description("Fast, matches Sonnet 4")
                    .with_vision(),
            ],
        );
    }

    /// Load models from a TOML config file
    pub fn load_from_file(&mut self, path: &PathBuf) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        let config: ModelsConfig = toml::from_str(&content)
            .map_err(|e| crate::error::TedError::Config(format!("Invalid models.toml: {}", e)))?;

        self.config_path = Some(path.clone());

        // Merge user models (user models take precedence)
        self.merge_provider_models(Provider::Anthropic, config.anthropic);
        self.merge_provider_models(Provider::Local, config.local);
        self.merge_provider_models(Provider::OpenRouter, config.openrouter);
        self.merge_provider_models(Provider::Blackman, config.blackman);

        Ok(())
    }

    /// Merge provider models, with new models taking precedence
    fn merge_provider_models(&mut self, provider: Provider, user_models: ProviderModels) {
        if user_models.models.is_empty() {
            return;
        }

        let existing = self.models.entry(provider).or_default();

        for user_model in user_models.models {
            // Remove any existing model with same ID
            existing.retain(|m| m.id != user_model.id);
            // Add the user's version
            existing.push(user_model);
        }
    }

    /// Get all models for a provider
    pub fn models_for_provider(&self, provider: &Provider) -> Vec<&ModelInfo> {
        self.models
            .get(provider)
            .map(|models| models.iter().collect())
            .unwrap_or_default()
    }

    /// Get models for a provider filtered by tier
    pub fn models_by_tier(&self, provider: &Provider, tier: ModelTier) -> Vec<&ModelInfo> {
        self.models_for_provider(provider)
            .into_iter()
            .filter(|m| m.tier == tier)
            .collect()
    }

    /// Get recommended models for a provider
    pub fn recommended_models(&self, provider: &Provider) -> Vec<&ModelInfo> {
        self.models_for_provider(provider)
            .into_iter()
            .filter(|m| m.recommended)
            .collect()
    }

    /// Get models that fit within a VRAM budget
    pub fn models_for_vram(&self, provider: &Provider, max_vram_gb: f32) -> Vec<&ModelInfo> {
        self.models_for_provider(provider)
            .into_iter()
            .filter(|m| m.vram_gb.map(|v| v <= max_vram_gb).unwrap_or(true))
            .collect()
    }

    /// Find a model by ID across all providers
    pub fn find_model(&self, id: &str) -> Option<(&Provider, &ModelInfo)> {
        for (provider, models) in &self.models {
            if let Some(model) = models.iter().find(|m| m.id == id) {
                return Some((provider, model));
            }
        }
        None
    }

    /// Find a model by ID for a specific provider
    pub fn find_model_for_provider(&self, provider: &Provider, id: &str) -> Option<&ModelInfo> {
        self.models
            .get(provider)
            .and_then(|models| models.iter().find(|m| m.id == id))
    }

    /// Get all provider names that have models
    pub fn providers(&self) -> Vec<&Provider> {
        self.models.keys().collect()
    }

    /// Generate a sample models.toml content
    pub fn generate_sample_config() -> String {
        r#"# Ted Model Registry
# Add or override models here. User models take precedence over built-in defaults.
# See: https://docs.ted.dev/models for the full list

[anthropic]
models = [
    # { id = "claude-sonnet-4-20250514", name = "Claude Sonnet 4", tier = "high", context_size = 200000, recommended = true }
]

[local]
models = [
    # { id = "qwen2.5-coder:14b", name = "Qwen 2.5 Coder 14B", tier = "medium", vram_gb = 12.0, recommended = true }
    # { id = "custom-model:latest", name = "My Custom Model", tier = "medium", vram_gb = 8.0 }
]

[openrouter]
models = [
    # { id = "anthropic/claude-sonnet-4", name = "Claude Sonnet 4", tier = "high", recommended = true }
]
"#
        .to_string()
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let registry = ModelRegistry::new();

        // Should have models for each provider
        assert!(!registry
            .models_for_provider(&Provider::Anthropic)
            .is_empty());
        assert!(!registry.models_for_provider(&Provider::Local).is_empty());
        assert!(!registry
            .models_for_provider(&Provider::OpenRouter)
            .is_empty());
    }

    #[test]
    fn test_registry_models_by_tier() {
        let registry = ModelRegistry::with_defaults_only();

        let high_tier = registry.models_by_tier(&Provider::Anthropic, ModelTier::High);
        assert!(high_tier.iter().any(|m| m.id.contains("sonnet")));

        let low_tier = registry.models_by_tier(&Provider::Anthropic, ModelTier::Low);
        assert!(low_tier.iter().any(|m| m.id.contains("haiku")));
    }

    #[test]
    fn test_registry_recommended() {
        let registry = ModelRegistry::with_defaults_only();

        let recommended = registry.recommended_models(&Provider::Anthropic);
        assert!(!recommended.is_empty());
        assert!(recommended.iter().all(|m| m.recommended));
    }

    #[test]
    fn test_registry_vram_filter() {
        let registry = ModelRegistry::with_defaults_only();

        let low_vram = registry.models_for_vram(&Provider::Local, 4.0);
        for model in &low_vram {
            if let Some(vram) = model.vram_gb {
                assert!(vram <= 4.0);
            }
        }
    }

    #[test]
    fn test_registry_find_model() {
        let registry = ModelRegistry::with_defaults_only();

        let (provider, model) = registry.find_model("claude-sonnet-4-20250514").unwrap();
        // Model exists in both Anthropic and Blackman, so just check it's one of them
        assert!(
            *provider == Provider::Anthropic || *provider == Provider::Blackman,
            "Expected Anthropic or Blackman, got {:?}",
            provider
        );
        assert_eq!(model.tier, ModelTier::High);
    }

    #[test]
    fn test_registry_find_model_for_provider() {
        let registry = ModelRegistry::with_defaults_only();

        let model = registry
            .find_model_for_provider(&Provider::Local, "qwen2.5-coder:14b")
            .unwrap();
        assert_eq!(model.tier, ModelTier::Medium);
    }

    #[test]
    fn test_registry_merge_user_models() {
        let mut registry = ModelRegistry::with_defaults_only();

        let user_config = ModelsConfig {
            anthropic: ProviderModels {
                models: vec![
                    ModelInfo::new("claude-sonnet-4-20250514", ModelTier::Medium)
                        .with_name("User Override"),
                ],
            },
            ..Default::default()
        };

        registry.merge_provider_models(Provider::Anthropic, user_config.anthropic);

        let model = registry
            .find_model_for_provider(&Provider::Anthropic, "claude-sonnet-4-20250514")
            .unwrap();
        // User version should take precedence
        assert_eq!(model.name, "User Override");
        assert_eq!(model.tier, ModelTier::Medium);
    }

    #[test]
    fn test_generate_sample_config() {
        let sample = ModelRegistry::generate_sample_config();
        assert!(sample.contains("[anthropic]"));
        assert!(sample.contains("[local]"));
        assert!(sample.contains("[openrouter]"));
    }
}
