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
use crate::llm::providers::{AnthropicProvider, LocalProvider, OpenRouterProvider};
use crate::models::download::BinaryDownloader;

fn expected_instruct_slug(model: &str) -> Option<&'static str> {
    match model {
        "qwen2.5-coder:1.5b" => Some("qwen2.5-coder-1.5b-instruct"),
        "qwen2.5-coder:3b" => Some("qwen2.5-coder-3b-instruct"),
        "qwen2.5-coder:7b" => Some("qwen2.5-coder-7b-instruct"),
        "qwen3:4b" => Some("qwen3-4b-instruct"),
        "qwen3-4b" => Some("qwen3-4b-instruct"),
        "qwen3-coder-30b-a3b" => Some("qwen3-coder-30b-a3b-instruct"),
        _ => None,
    }
}

fn path_filename_lower(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

fn path_matches_instruct_slug(path: &std::path::Path, slug: &str) -> bool {
    let filename = path_filename_lower(path);
    filename.contains(slug) && filename.ends_with(".gguf")
}

/// Factory for creating LLM providers
pub struct ProviderFactory;

impl ProviderFactory {
    /// Create an LLM provider based on provider name and settings
    pub async fn create(
        provider_name: &str,
        settings: &Settings,
        _perform_health_check: bool,
    ) -> Result<Arc<dyn LlmProvider>> {
        match provider_name {
            "local" => Self::create_local(settings).await,
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

    /// Create a local LLM provider (llama-server subprocess)
    pub async fn create_local(settings: &Settings) -> Result<Arc<dyn LlmProvider>> {
        let cfg = &settings.providers.local;

        if let Some(base_url) = cfg.base_url.as_ref().map(|value| value.trim()) {
            if !base_url.is_empty() {
                let normalized = base_url.trim_end_matches('/').to_string();
                let provider = LocalProvider::with_external_server(
                    normalized,
                    cfg.default_model.clone(),
                    cfg.ctx_size,
                );
                return Ok(Arc::new(provider));
            }
        }

        let discovered = crate::models::scanner::scan_for_models();

        // Resolve model path: explicit config → instruct-aware scan → fallback scan → error
        let model_path = if let Some(expected_slug) = expected_instruct_slug(&cfg.default_model) {
            if cfg.model_path.exists() && path_matches_instruct_slug(&cfg.model_path, expected_slug)
            {
                cfg.model_path.clone()
            } else if let Some(found) = discovered
                .iter()
                .find(|candidate| path_matches_instruct_slug(&candidate.path, expected_slug))
            {
                tracing::info!(
                    "Auto-selected instruct model for {}: {} ({})",
                    cfg.default_model,
                    found.display_name(),
                    found.size_display()
                );
                found.path.clone()
            } else {
                return Err(TedError::Config(format!(
                    "Configured local model '{}' requires an Instruct GGUF.\n\
                     Expected filename containing '{}'.\n\
                     Run Teddy 'One-Click Setup Local AI' to download a compatible model.",
                    cfg.default_model, expected_slug
                )));
            }
        } else if cfg.model_path.exists() {
            cfg.model_path.clone()
        } else {
            if discovered.is_empty() {
                return Err(TedError::Config(
                    "No GGUF model files found.\n\n\
                     To use the local provider, you need a GGUF model file.\n\
                     Options:\n\
                     1. Place a .gguf file in ~/.ted/models/local/\n\
                     2. Models from LM Studio and GPT4All are detected automatically\n\
                     3. Specify a path: ted chat -p local --model-path /path/to/model.gguf"
                        .to_string(),
                ));
            }

            let selected = &discovered[0];
            tracing::info!(
                "Auto-detected model: {} ({})",
                selected.display_name(),
                selected.size_display()
            );
            selected.path.clone()
        };

        // Derive model name from filename
        let model_name = model_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&cfg.default_model)
            .to_string();

        // Find or download llama-server binary
        let downloader = BinaryDownloader::new()?;
        let binary_path = downloader.ensure_llama_server().await?;

        let provider = LocalProvider::new(
            binary_path,
            model_path,
            model_name,
            cfg.port,
            cfg.gpu_layers,
            cfg.ctx_size,
        );

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
            "local" => settings.providers.local.default_model.clone(),
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
            "local" => {
                settings
                    .providers
                    .local
                    .base_url
                    .as_deref()
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false)
                    || settings.providers.local.model_path.exists()
                    || !crate::models::scanner::scan_for_models().is_empty()
            }
            "openrouter" => settings.get_openrouter_api_key().is_some(),
            "blackman" => settings.get_blackman_api_key().is_some(),
            _ => settings.get_anthropic_api_key().is_some(),
        }
    }

    /// List all supported provider names
    pub fn supported_providers() -> &'static [&'static str] {
        &["anthropic", "local", "openrouter", "blackman"]
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
    fn test_default_model_local() {
        let settings = Settings::default();
        let model = ProviderFactory::default_model("local", &settings);
        assert!(!model.is_empty());
    }

    #[test]
    fn test_expected_instruct_slug_for_qwen3_models() {
        assert_eq!(
            expected_instruct_slug("qwen3:4b"),
            Some("qwen3-4b-instruct")
        );
        assert_eq!(
            expected_instruct_slug("qwen3-coder-30b-a3b"),
            Some("qwen3-coder-30b-a3b-instruct")
        );
    }

    #[test]
    fn test_resolve_provider_name_with_requested() {
        let settings = Settings::default();
        let name = ProviderFactory::resolve_provider_name(Some("local"), &settings);
        assert_eq!(name, "local");
    }

    #[test]
    fn test_resolve_provider_name_without_requested() {
        let settings = Settings::default();
        let name = ProviderFactory::resolve_provider_name(None, &settings);
        assert_eq!(name, settings.defaults.provider);
    }

    #[test]
    fn test_supported_providers() {
        let providers = ProviderFactory::supported_providers();
        assert!(providers.contains(&"anthropic"));
        assert!(providers.contains(&"local"));
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
        let model = ProviderFactory::default_model("unknown_provider", &settings);
        assert_eq!(model, settings.providers.anthropic.default_model);
    }

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
    fn test_resolve_provider_name_uses_settings_default() {
        let mut settings = Settings::default();
        settings.defaults.provider = "local".to_string();

        let name = ProviderFactory::resolve_provider_name(None, &settings);
        assert_eq!(name, "local");
    }

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
    fn test_is_configured_local_with_base_url() {
        let mut settings = Settings::default();
        settings.providers.local.base_url = Some("http://127.0.0.1:8847".to_string());
        settings.providers.local.model_path = std::path::PathBuf::from("/definitely/missing.gguf");

        assert!(ProviderFactory::is_configured("local", &settings));
    }

    #[test]
    fn test_supported_providers_count() {
        let providers = ProviderFactory::supported_providers();
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

    #[tokio::test]
    async fn test_create_returns_anthropic_for_unknown() {
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = None;
        settings.providers.anthropic.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let result = ProviderFactory::create("unknown_provider", &settings, false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_local_with_base_url() {
        let mut settings = Settings::default();
        settings.providers.local.base_url = Some("http://127.0.0.1:8847".to_string());
        settings.providers.local.default_model = "qwen2.5-coder:3b".to_string();
        settings.providers.local.model_path = std::path::PathBuf::from("/definitely/missing.gguf");

        let provider = ProviderFactory::create_local(&settings).await.unwrap();
        assert_eq!(provider.name(), "local");
        assert!(provider.supports_model("anything"));
    }
}
