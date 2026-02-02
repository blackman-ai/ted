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

    /// Get the default model for a provider
    pub fn default_model(provider_name: &str, settings: &Settings) -> String {
        match provider_name {
            "ollama" => settings.providers.ollama.default_model.clone(),
            "openrouter" => settings.providers.openrouter.default_model.clone(),
            "blackman" => settings.providers.blackman.default_model.clone(),
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
            _ => settings.get_anthropic_api_key().is_some(),
        }
    }

    /// List all supported provider names
    pub fn supported_providers() -> &'static [&'static str] {
        &["anthropic", "ollama", "openrouter", "blackman"]
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
}
