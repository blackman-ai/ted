// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Provider factory for creating LLM providers
//!
//! Centralizes provider creation logic that was previously duplicated
//! across main.rs and other entry points.

use std::sync::Arc;

use crate::config::Settings;
use crate::error::{Result, TedError};
use crate::llm::provider::LlmProvider;
use crate::llm::providers::{AnthropicProvider, OllamaProvider, OpenRouterProvider};

#[cfg(feature = "local-llm")]
use crate::llm::providers::{LlamaCppConfig, LlamaCppProvider};

/// Factory for creating LLM providers
pub struct ProviderFactory;

impl ProviderFactory {
    /// Create an LLM provider based on provider name and settings
    ///
    /// # Arguments
    /// * `provider_name` - One of: "anthropic", "ollama", "openrouter", "blackman"
    /// * `settings` - Application settings containing provider configuration
    /// * `perform_health_check` - Whether to perform health checks (for ollama)
    ///
    /// # Returns
    /// An Arc-wrapped provider instance
    pub async fn create(
        provider_name: &str,
        settings: &Settings,
        perform_health_check: bool,
    ) -> Result<Arc<dyn LlmProvider>> {
        match provider_name {
            "ollama" => Self::create_ollama(settings, perform_health_check).await,
            "openrouter" => Self::create_openrouter(settings),
            "blackman" => Self::create_blackman(settings),
            #[cfg(feature = "local-llm")]
            "llama-cpp" | "llamacpp" | "local" => Self::create_llama_cpp(settings),
            _ => Self::create_anthropic(settings),
        }
    }

    /// Create an Anthropic provider
    pub fn create_anthropic(settings: &Settings) -> Result<Arc<dyn LlmProvider>> {
        let api_key = settings.get_anthropic_api_key().ok_or_else(|| {
            TedError::Config(
                "No Anthropic API key found. Set ANTHROPIC_API_KEY env var or run 'ted settings'."
                    .to_string(),
            )
        })?;

        let provider = if let Some(ref base_url) = settings.providers.anthropic.base_url {
            AnthropicProvider::with_base_url(api_key, base_url)
        } else {
            AnthropicProvider::new(api_key)
        };

        Ok(Arc::new(provider))
    }

    /// Create an Ollama provider
    pub async fn create_ollama(
        settings: &Settings,
        perform_health_check: bool,
    ) -> Result<Arc<dyn LlmProvider>> {
        let provider = OllamaProvider::with_base_url(&settings.providers.ollama.base_url);

        if perform_health_check {
            provider.health_check().await.map_err(|_| {
                TedError::Config(
                    "Ollama is not running. Start Ollama with: ollama serve".to_string(),
                )
            })?;
        }

        Ok(Arc::new(provider))
    }

    /// Create an OpenRouter provider
    pub fn create_openrouter(settings: &Settings) -> Result<Arc<dyn LlmProvider>> {
        let api_key = settings.get_openrouter_api_key().ok_or_else(|| {
            TedError::Config(
                "No OpenRouter API key found. Set OPENROUTER_API_KEY env var or run 'ted settings'."
                    .to_string(),
            )
        })?;

        let provider = if let Some(ref base_url) = settings.providers.openrouter.base_url {
            OpenRouterProvider::with_base_url(api_key, base_url)
        } else {
            OpenRouterProvider::new(api_key)
        };

        Ok(Arc::new(provider))
    }

    /// Create a Blackman provider
    pub fn create_blackman(settings: &Settings) -> Result<Arc<dyn LlmProvider>> {
        let api_key = settings.get_blackman_api_key().ok_or_else(|| {
            TedError::Config(
                "No Blackman API key found. Set BLACKMAN_API_KEY env var or run 'ted settings'."
                    .to_string(),
            )
        })?;

        let base_url = settings.get_blackman_base_url();

        // Blackman uses the Anthropic-compatible API
        let provider = AnthropicProvider::with_base_url(api_key, format!("{}/v1", base_url));

        Ok(Arc::new(provider))
    }

    /// Create a LlamaCpp local model provider
    #[cfg(feature = "local-llm")]
    pub fn create_llama_cpp(settings: &Settings) -> Result<Arc<dyn LlmProvider>> {
        let cfg = &settings.providers.llama_cpp;

        // Check if model file exists
        if !cfg.model_path.exists() {
            return Err(TedError::Config(format!(
                "LlamaCpp model not found: {}. Download a model with /model download or set the path in settings.",
                cfg.model_path.display()
            )));
        }

        // Build configuration from settings
        let mut config = LlamaCppConfig::new(&cfg.model_path)
            .with_context_size(cfg.context_size)
            .with_gpu_layers(cfg.gpu_layers);

        // Add threads if specified
        if let Some(threads) = cfg.threads {
            config = config.with_threads(threads);
        }

        // Create and return provider
        let provider = LlamaCppProvider::with_config(config)?;
        Ok(Arc::new(provider))
    }

    /// Get the default model for a provider
    pub fn default_model(provider_name: &str, settings: &Settings) -> String {
        match provider_name {
            "ollama" => settings.providers.ollama.default_model.clone(),
            "openrouter" => settings.providers.openrouter.default_model.clone(),
            "blackman" => settings.providers.blackman.default_model.clone(),
            "llama-cpp" | "llamacpp" | "local" => settings.providers.llama_cpp.default_model.clone(),
            _ => settings.providers.anthropic.default_model.clone(),
        }
    }

    /// Get the provider name from settings, with fallback to default
    pub fn resolve_provider_name(requested: Option<&str>, settings: &Settings) -> String {
        requested
            .map(|s| s.to_string())
            .unwrap_or_else(|| settings.defaults.provider.clone())
    }

    /// Check if a provider is configured (has required credentials)
    pub fn is_configured(provider_name: &str, settings: &Settings) -> bool {
        match provider_name {
            "ollama" => true, // Ollama doesn't require API key
            "openrouter" => settings.get_openrouter_api_key().is_some(),
            "blackman" => settings.get_blackman_api_key().is_some(),
            "llama-cpp" | "llamacpp" | "local" => settings.providers.llama_cpp.model_path.exists(),
            _ => settings.get_anthropic_api_key().is_some(),
        }
    }

    /// List all supported provider names
    pub fn supported_providers() -> &'static [&'static str] {
        #[cfg(feature = "local-llm")]
        return &["anthropic", "ollama", "openrouter", "blackman", "llama-cpp"];

        #[cfg(not(feature = "local-llm"))]
        return &["anthropic", "ollama", "openrouter", "blackman"];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_model_anthropic() {
        let settings = Settings::default();
        let model = ProviderFactory::default_model("anthropic", &settings);
        assert!(model.contains("claude"));
    }

    #[test]
    fn test_default_model_ollama() {
        let settings = Settings::default();
        let model = ProviderFactory::default_model("ollama", &settings);
        assert!(!model.is_empty());
    }

    #[test]
    fn test_resolve_provider_name_with_requested() {
        let settings = Settings::default();
        let name = ProviderFactory::resolve_provider_name(Some("ollama"), &settings);
        assert_eq!(name, "ollama");
    }

    #[test]
    fn test_resolve_provider_name_without_requested() {
        let settings = Settings::default();
        let name = ProviderFactory::resolve_provider_name(None, &settings);
        assert_eq!(name, settings.defaults.provider);
    }

    #[test]
    fn test_is_configured_ollama() {
        let settings = Settings::default();
        // Ollama doesn't require API key
        assert!(ProviderFactory::is_configured("ollama", &settings));
    }

    #[test]
    fn test_supported_providers() {
        let providers = ProviderFactory::supported_providers();
        assert!(providers.contains(&"anthropic"));
        assert!(providers.contains(&"ollama"));
        assert!(providers.contains(&"openrouter"));
        assert!(providers.contains(&"blackman"));
    }

    #[test]
    fn test_create_anthropic_no_key() {
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = None;
        settings.providers.anthropic.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let result = ProviderFactory::create_anthropic(&settings);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_openrouter_no_key() {
        let mut settings = Settings::default();
        settings.providers.openrouter.api_key = None;
        settings.providers.openrouter.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let result = ProviderFactory::create_openrouter(&settings);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_blackman_no_key() {
        let mut settings = Settings::default();
        settings.providers.blackman.api_key = None;
        settings.providers.blackman.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let result = ProviderFactory::create_blackman(&settings);
        assert!(result.is_err());
    }

    // ===== Additional default_model Tests =====

    #[test]
    fn test_default_model_openrouter() {
        let settings = Settings::default();
        let model = ProviderFactory::default_model("openrouter", &settings);
        assert!(!model.is_empty());
    }

    #[test]
    fn test_default_model_blackman() {
        let settings = Settings::default();
        let model = ProviderFactory::default_model("blackman", &settings);
        assert!(!model.is_empty());
    }

    #[test]
    fn test_default_model_unknown_provider() {
        let settings = Settings::default();
        // Unknown provider should default to anthropic's model
        let model = ProviderFactory::default_model("unknown_provider", &settings);
        assert_eq!(model, settings.providers.anthropic.default_model);
    }

    #[test]
    fn test_default_model_custom_settings() {
        let mut settings = Settings::default();
        settings.providers.ollama.default_model = "custom-llama-model".to_string();

        let model = ProviderFactory::default_model("ollama", &settings);
        assert_eq!(model, "custom-llama-model");
    }

    // ===== Additional resolve_provider_name Tests =====

    #[test]
    fn test_resolve_provider_name_with_anthropic() {
        let settings = Settings::default();
        let name = ProviderFactory::resolve_provider_name(Some("anthropic"), &settings);
        assert_eq!(name, "anthropic");
    }

    #[test]
    fn test_resolve_provider_name_with_openrouter() {
        let settings = Settings::default();
        let name = ProviderFactory::resolve_provider_name(Some("openrouter"), &settings);
        assert_eq!(name, "openrouter");
    }

    #[test]
    fn test_resolve_provider_name_with_blackman() {
        let settings = Settings::default();
        let name = ProviderFactory::resolve_provider_name(Some("blackman"), &settings);
        assert_eq!(name, "blackman");
    }

    #[test]
    fn test_resolve_provider_name_uses_settings_default() {
        let mut settings = Settings::default();
        settings.defaults.provider = "ollama".to_string();

        let name = ProviderFactory::resolve_provider_name(None, &settings);
        assert_eq!(name, "ollama");
    }

    // ===== Additional is_configured Tests =====

    #[test]
    fn test_is_configured_anthropic_no_key() {
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = None;
        settings.providers.anthropic.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        assert!(!ProviderFactory::is_configured("anthropic", &settings));
    }

    #[test]
    fn test_is_configured_openrouter_no_key() {
        let mut settings = Settings::default();
        settings.providers.openrouter.api_key = None;
        settings.providers.openrouter.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        assert!(!ProviderFactory::is_configured("openrouter", &settings));
    }

    #[test]
    fn test_is_configured_blackman_no_key() {
        let mut settings = Settings::default();
        settings.providers.blackman.api_key = None;
        settings.providers.blackman.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        assert!(!ProviderFactory::is_configured("blackman", &settings));
    }

    #[test]
    fn test_is_configured_unknown_provider() {
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = None;
        settings.providers.anthropic.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        // Unknown provider falls back to anthropic check
        assert!(!ProviderFactory::is_configured("unknown", &settings));
    }

    // ===== supported_providers Tests =====

    #[test]
    fn test_supported_providers_count() {
        let providers = ProviderFactory::supported_providers();
        #[cfg(feature = "local-llm")]
        assert_eq!(providers.len(), 5);
        #[cfg(not(feature = "local-llm"))]
        assert_eq!(providers.len(), 4);
    }

    #[test]
    fn test_supported_providers_are_unique() {
        let providers = ProviderFactory::supported_providers();
        let mut unique: Vec<&str> = providers.to_vec();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), providers.len());
    }

    // ===== Error message Tests =====

    #[test]
    fn test_create_anthropic_error_message() {
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = None;
        settings.providers.anthropic.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let result = ProviderFactory::create_anthropic(&settings);
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(msg.contains("Anthropic"));
            assert!(msg.contains("API key"));
        } else {
            panic!("Expected error");
        }
    }

    #[test]
    fn test_create_openrouter_error_message() {
        let mut settings = Settings::default();
        settings.providers.openrouter.api_key = None;
        settings.providers.openrouter.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let result = ProviderFactory::create_openrouter(&settings);
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(msg.contains("OpenRouter"));
            assert!(msg.contains("API key"));
        } else {
            panic!("Expected error");
        }
    }

    #[test]
    fn test_create_blackman_error_message() {
        let mut settings = Settings::default();
        settings.providers.blackman.api_key = None;
        settings.providers.blackman.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let result = ProviderFactory::create_blackman(&settings);
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(msg.contains("Blackman"));
            assert!(msg.contains("API key"));
        } else {
            panic!("Expected error");
        }
    }

    // ===== Async create Tests =====

    #[tokio::test]
    async fn test_create_ollama_without_health_check() {
        let settings = Settings::default();
        let result = ProviderFactory::create_ollama(&settings, false).await;
        // Should succeed without health check
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_returns_anthropic_for_unknown() {
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = None;
        settings.providers.anthropic.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        // Unknown provider falls back to anthropic, which should fail without key
        let result = ProviderFactory::create("unknown_provider", &settings, false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_ollama_via_factory() {
        let settings = Settings::default();
        let result = ProviderFactory::create("ollama", &settings, false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_openrouter_via_factory_no_key() {
        let mut settings = Settings::default();
        settings.providers.openrouter.api_key = None;
        settings.providers.openrouter.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let result = ProviderFactory::create("openrouter", &settings, false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_blackman_via_factory_no_key() {
        let mut settings = Settings::default();
        settings.providers.blackman.api_key = None;
        settings.providers.blackman.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let result = ProviderFactory::create("blackman", &settings, false).await;
        assert!(result.is_err());
    }
}
