// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Provider configuration logic
//!
//! This module provides testable logic for configuring LLM providers,
//! including validation, model selection, and provider-specific setup.

use crate::config::Settings;
use crate::error::{Result, TedError};

/// Represents a configured provider
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Provider name (e.g., "anthropic", "ollama", "openrouter")
    pub name: String,
    /// API key if required
    pub api_key: Option<String>,
    /// Base URL if customizable
    pub base_url: Option<String>,
    /// Default model for this provider
    pub default_model: String,
    /// Whether the provider requires an API key
    pub requires_api_key: bool,
}

/// Result of provider validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderValidation {
    /// Provider is valid and ready to use
    Valid,
    /// Provider needs configuration (e.g., missing API key)
    NeedsConfiguration(String),
    /// Provider is invalid
    Invalid(String),
}

/// Determine which provider to use based on args and settings
pub fn resolve_provider_name(arg_provider: Option<&str>, settings: &Settings) -> String {
    arg_provider
        .map(|s| s.to_string())
        .unwrap_or_else(|| settings.defaults.provider.clone())
}

/// Resolve which model to use based on args, cap preferences, and settings
pub fn resolve_model_name(
    arg_model: Option<&str>,
    cap_preferred_model: Option<&str>,
    provider_name: &str,
    settings: &Settings,
) -> String {
    if let Some(model) = arg_model {
        return model.to_string();
    }

    if let Some(model) = cap_preferred_model {
        return model.to_string();
    }

    // Use provider-specific default
    match provider_name {
        "ollama" => settings.providers.ollama.default_model.clone(),
        "openrouter" => settings.providers.openrouter.default_model.clone(),
        _ => settings.providers.anthropic.default_model.clone(),
    }
}

/// Validate that a provider is properly configured
pub fn validate_provider_config(provider_name: &str, settings: &Settings) -> ProviderValidation {
    match provider_name {
        "ollama" => {
            // Ollama doesn't require an API key, just needs to be running
            ProviderValidation::Valid
        }
        "openrouter" => {
            if settings.get_openrouter_api_key().is_some() {
                ProviderValidation::Valid
            } else {
                ProviderValidation::NeedsConfiguration(
                    "No OpenRouter API key found. Set OPENROUTER_API_KEY env var or run 'ted settings'.".to_string()
                )
            }
        }
        "anthropic" | "" => {
            if settings.get_anthropic_api_key().is_some() {
                ProviderValidation::Valid
            } else {
                ProviderValidation::NeedsConfiguration(
                    "No Anthropic API key configured. Run 'ted' to set up.".to_string(),
                )
            }
        }
        _ => ProviderValidation::Invalid(format!("Unknown provider: {}", provider_name)),
    }
}

/// Build provider configuration from settings
pub fn build_provider_config(provider_name: &str, settings: &Settings) -> Result<ProviderConfig> {
    match provider_name {
        "ollama" => Ok(ProviderConfig {
            name: "ollama".to_string(),
            api_key: None,
            base_url: Some(settings.providers.ollama.base_url.clone()),
            default_model: settings.providers.ollama.default_model.clone(),
            requires_api_key: false,
        }),
        "openrouter" => {
            let api_key = settings
                .get_openrouter_api_key()
                .ok_or_else(|| TedError::Config("No OpenRouter API key found".to_string()))?;
            Ok(ProviderConfig {
                name: "openrouter".to_string(),
                api_key: Some(api_key),
                base_url: settings.providers.openrouter.base_url.clone(),
                default_model: settings.providers.openrouter.default_model.clone(),
                requires_api_key: true,
            })
        }
        "anthropic" | "" => {
            let api_key = settings
                .get_anthropic_api_key()
                .ok_or_else(|| TedError::Config("No Anthropic API key found".to_string()))?;
            Ok(ProviderConfig {
                name: "anthropic".to_string(),
                api_key: Some(api_key),
                base_url: settings.providers.anthropic.base_url.clone(),
                default_model: settings.providers.anthropic.default_model.clone(),
                requires_api_key: true,
            })
        }
        _ => Err(TedError::Config(format!(
            "Unknown provider: {}",
            provider_name
        ))),
    }
}

/// Validate an API key format (basic checks)
pub fn validate_api_key_format(provider: &str, key: &str) -> ApiKeyValidation {
    if key.is_empty() {
        return ApiKeyValidation::Empty;
    }

    match provider {
        "anthropic" => {
            if key.starts_with("sk-ant-") || key.starts_with("sk-") {
                ApiKeyValidation::Valid
            } else {
                ApiKeyValidation::Warning(
                    "API key doesn't start with expected prefix 'sk-'".to_string(),
                )
            }
        }
        "openrouter" => {
            if key.starts_with("sk-or-") {
                ApiKeyValidation::Valid
            } else if key.len() > 20 {
                ApiKeyValidation::Warning(
                    "API key doesn't have expected OpenRouter prefix".to_string(),
                )
            } else {
                ApiKeyValidation::Invalid("API key appears too short".to_string())
            }
        }
        _ => {
            // For unknown providers, just check it's not empty
            if key.len() > 10 {
                ApiKeyValidation::Valid
            } else {
                ApiKeyValidation::Warning("API key appears short".to_string())
            }
        }
    }
}

/// Result of API key validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeyValidation {
    /// Key appears valid
    Valid,
    /// Key is valid but has a warning
    Warning(String),
    /// Key is empty
    Empty,
    /// Key appears invalid
    Invalid(String),
}

impl ApiKeyValidation {
    pub fn is_usable(&self) -> bool {
        matches!(self, ApiKeyValidation::Valid | ApiKeyValidation::Warning(_))
    }

    pub fn warning_message(&self) -> Option<&str> {
        match self {
            ApiKeyValidation::Warning(msg) => Some(msg),
            _ => None,
        }
    }
}

/// Get the default model for a provider
pub fn get_default_model(provider_name: &str, settings: &Settings) -> String {
    match provider_name {
        "ollama" => settings.providers.ollama.default_model.clone(),
        "openrouter" => settings.providers.openrouter.default_model.clone(),
        _ => settings.providers.anthropic.default_model.clone(),
    }
}

/// Check if a model is supported by a provider
pub fn is_model_supported_by_provider(model: &str, provider_name: &str) -> bool {
    match provider_name {
        "anthropic" => model.starts_with("claude-"),
        "ollama" => {
            // Ollama can run any model, so always return true
            true
        }
        "openrouter" => {
            // OpenRouter supports many models
            true
        }
        _ => true,
    }
}

/// Get a list of known models for a provider
pub fn get_known_models(provider_name: &str) -> Vec<&'static str> {
    match provider_name {
        "anthropic" => vec![
            "claude-opus-4-20250514",
            "claude-sonnet-4-20250514",
            "claude-3-5-sonnet-20241022",
            "claude-3-5-haiku-20241022",
        ],
        "ollama" => vec![
            "llama3.2",
            "llama3.1",
            "codellama",
            "mistral",
            "deepseek-coder-v2",
        ],
        "openrouter" => vec![
            "anthropic/claude-3.5-sonnet",
            "anthropic/claude-3-opus",
            "google/gemini-pro",
            "openai/gpt-4-turbo",
        ],
        _ => vec![],
    }
}

/// Format provider info for display
pub fn format_provider_info(config: &ProviderConfig) -> String {
    let mut info = format!("Provider: {}\n", config.name);
    info.push_str(&format!("Default model: {}\n", config.default_model));

    if let Some(ref url) = config.base_url {
        info.push_str(&format!("Base URL: {}\n", url));
    }

    if config.requires_api_key {
        if config.api_key.is_some() {
            info.push_str("API key: configured\n");
        } else {
            info.push_str("API key: not configured\n");
        }
    } else {
        info.push_str("API key: not required\n");
    }

    info
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_settings() -> Settings {
        let mut settings = Settings::default();
        settings.providers.anthropic.default_model = "claude-3-5-sonnet".to_string();
        settings.providers.ollama.default_model = "llama3.2".to_string();
        settings.providers.ollama.base_url = "http://localhost:11434".to_string();
        settings.providers.openrouter.default_model = "anthropic/claude-3.5-sonnet".to_string();
        settings
    }

    // ==================== resolve_provider_name tests ====================

    #[test]
    fn test_resolve_provider_name_from_arg() {
        let settings = create_test_settings();
        let result = resolve_provider_name(Some("ollama"), &settings);
        assert_eq!(result, "ollama");
    }

    #[test]
    fn test_resolve_provider_name_from_settings() {
        let mut settings = create_test_settings();
        settings.defaults.provider = "openrouter".to_string();
        let result = resolve_provider_name(None, &settings);
        assert_eq!(result, "openrouter");
    }

    #[test]
    fn test_resolve_provider_name_arg_overrides_settings() {
        let mut settings = create_test_settings();
        settings.defaults.provider = "anthropic".to_string();
        let result = resolve_provider_name(Some("ollama"), &settings);
        assert_eq!(result, "ollama");
    }

    // ==================== resolve_model_name tests ====================

    #[test]
    fn test_resolve_model_name_from_arg() {
        let settings = create_test_settings();
        let result = resolve_model_name(Some("custom-model"), None, "anthropic", &settings);
        assert_eq!(result, "custom-model");
    }

    #[test]
    fn test_resolve_model_name_from_cap() {
        let settings = create_test_settings();
        let result = resolve_model_name(None, Some("cap-model"), "anthropic", &settings);
        assert_eq!(result, "cap-model");
    }

    #[test]
    fn test_resolve_model_name_from_settings() {
        let settings = create_test_settings();
        let result = resolve_model_name(None, None, "anthropic", &settings);
        assert_eq!(result, "claude-3-5-sonnet");
    }

    #[test]
    fn test_resolve_model_name_ollama() {
        let settings = create_test_settings();
        let result = resolve_model_name(None, None, "ollama", &settings);
        assert_eq!(result, "llama3.2");
    }

    #[test]
    fn test_resolve_model_name_arg_overrides_cap() {
        let settings = create_test_settings();
        let result =
            resolve_model_name(Some("arg-model"), Some("cap-model"), "anthropic", &settings);
        assert_eq!(result, "arg-model");
    }

    // ==================== validate_provider_config tests ====================

    #[test]
    fn test_validate_provider_config_ollama() {
        let settings = create_test_settings();
        let result = validate_provider_config("ollama", &settings);
        assert_eq!(result, ProviderValidation::Valid);
    }

    #[test]
    fn test_validate_provider_config_anthropic_no_key() {
        let settings = create_test_settings();
        let result = validate_provider_config("anthropic", &settings);
        assert!(matches!(result, ProviderValidation::NeedsConfiguration(_)));
    }

    #[test]
    fn test_validate_provider_config_anthropic_with_key() {
        let mut settings = create_test_settings();
        settings.providers.anthropic.api_key = Some("sk-test-key".to_string());
        let result = validate_provider_config("anthropic", &settings);
        assert_eq!(result, ProviderValidation::Valid);
    }

    #[test]
    fn test_validate_provider_config_unknown() {
        let settings = create_test_settings();
        let result = validate_provider_config("unknown_provider", &settings);
        assert!(matches!(result, ProviderValidation::Invalid(_)));
    }

    #[test]
    fn test_validate_provider_config_empty_string() {
        let mut settings = create_test_settings();
        settings.providers.anthropic.api_key = Some("sk-test".to_string());
        let result = validate_provider_config("", &settings);
        assert_eq!(result, ProviderValidation::Valid);
    }

    // ==================== build_provider_config tests ====================

    #[test]
    fn test_build_provider_config_ollama() {
        let settings = create_test_settings();
        let result = build_provider_config("ollama", &settings);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.name, "ollama");
        assert!(!config.requires_api_key);
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_build_provider_config_anthropic_no_key() {
        let settings = create_test_settings();
        let result = build_provider_config("anthropic", &settings);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_provider_config_anthropic_with_key() {
        let mut settings = create_test_settings();
        settings.providers.anthropic.api_key = Some("sk-test".to_string());
        let result = build_provider_config("anthropic", &settings);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.name, "anthropic");
        assert!(config.requires_api_key);
        assert_eq!(config.api_key, Some("sk-test".to_string()));
    }

    #[test]
    fn test_build_provider_config_unknown() {
        let settings = create_test_settings();
        let result = build_provider_config("unknown", &settings);
        assert!(result.is_err());
    }

    // ==================== validate_api_key_format tests ====================

    #[test]
    fn test_validate_api_key_format_anthropic_valid() {
        let result = validate_api_key_format("anthropic", "sk-ant-api123");
        assert_eq!(result, ApiKeyValidation::Valid);
    }

    #[test]
    fn test_validate_api_key_format_anthropic_sk_prefix() {
        let result = validate_api_key_format("anthropic", "sk-test123");
        assert_eq!(result, ApiKeyValidation::Valid);
    }

    #[test]
    fn test_validate_api_key_format_anthropic_no_prefix() {
        let result = validate_api_key_format("anthropic", "my-api-key");
        assert!(matches!(result, ApiKeyValidation::Warning(_)));
    }

    #[test]
    fn test_validate_api_key_format_empty() {
        let result = validate_api_key_format("anthropic", "");
        assert_eq!(result, ApiKeyValidation::Empty);
    }

    #[test]
    fn test_validate_api_key_format_openrouter() {
        let result = validate_api_key_format("openrouter", "sk-or-v1-key");
        assert_eq!(result, ApiKeyValidation::Valid);
    }

    #[test]
    fn test_validate_api_key_format_openrouter_warning() {
        let result = validate_api_key_format("openrouter", "some-long-api-key-here");
        assert!(matches!(result, ApiKeyValidation::Warning(_)));
    }

    #[test]
    fn test_validate_api_key_format_openrouter_short() {
        let result = validate_api_key_format("openrouter", "short");
        assert!(matches!(result, ApiKeyValidation::Invalid(_)));
    }

    // ==================== ApiKeyValidation tests ====================

    #[test]
    fn test_api_key_validation_is_usable() {
        assert!(ApiKeyValidation::Valid.is_usable());
        assert!(ApiKeyValidation::Warning("test".to_string()).is_usable());
        assert!(!ApiKeyValidation::Empty.is_usable());
        assert!(!ApiKeyValidation::Invalid("test".to_string()).is_usable());
    }

    #[test]
    fn test_api_key_validation_warning_message() {
        let valid = ApiKeyValidation::Valid;
        assert!(valid.warning_message().is_none());

        let warning = ApiKeyValidation::Warning("test warning".to_string());
        assert_eq!(warning.warning_message(), Some("test warning"));
    }

    // ==================== get_default_model tests ====================

    #[test]
    fn test_get_default_model_anthropic() {
        let settings = create_test_settings();
        let model = get_default_model("anthropic", &settings);
        assert_eq!(model, "claude-3-5-sonnet");
    }

    #[test]
    fn test_get_default_model_ollama() {
        let settings = create_test_settings();
        let model = get_default_model("ollama", &settings);
        assert_eq!(model, "llama3.2");
    }

    #[test]
    fn test_get_default_model_unknown() {
        let settings = create_test_settings();
        let model = get_default_model("unknown", &settings);
        // Falls back to anthropic default
        assert_eq!(model, "claude-3-5-sonnet");
    }

    // ==================== is_model_supported_by_provider tests ====================

    #[test]
    fn test_is_model_supported_anthropic_claude() {
        assert!(is_model_supported_by_provider(
            "claude-3-5-sonnet",
            "anthropic"
        ));
    }

    #[test]
    fn test_is_model_supported_anthropic_non_claude() {
        assert!(!is_model_supported_by_provider("gpt-4", "anthropic"));
    }

    #[test]
    fn test_is_model_supported_ollama() {
        assert!(is_model_supported_by_provider("llama3.2", "ollama"));
        assert!(is_model_supported_by_provider("any-model", "ollama"));
    }

    #[test]
    fn test_is_model_supported_openrouter() {
        assert!(is_model_supported_by_provider(
            "anthropic/claude-3.5",
            "openrouter"
        ));
    }

    // ==================== get_known_models tests ====================

    #[test]
    fn test_get_known_models_anthropic() {
        let models = get_known_models("anthropic");
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.contains("claude")));
    }

    #[test]
    fn test_get_known_models_ollama() {
        let models = get_known_models("ollama");
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.contains("llama")));
    }

    #[test]
    fn test_get_known_models_unknown() {
        let models = get_known_models("unknown");
        assert!(models.is_empty());
    }

    // ==================== format_provider_info tests ====================

    #[test]
    fn test_format_provider_info_basic() {
        let config = ProviderConfig {
            name: "test".to_string(),
            api_key: None,
            base_url: None,
            default_model: "model".to_string(),
            requires_api_key: false,
        };
        let info = format_provider_info(&config);
        assert!(info.contains("Provider: test"));
        assert!(info.contains("Default model: model"));
        assert!(info.contains("not required"));
    }

    #[test]
    fn test_format_provider_info_with_url() {
        let config = ProviderConfig {
            name: "test".to_string(),
            api_key: None,
            base_url: Some("http://localhost:8080".to_string()),
            default_model: "model".to_string(),
            requires_api_key: false,
        };
        let info = format_provider_info(&config);
        assert!(info.contains("http://localhost:8080"));
    }

    #[test]
    fn test_format_provider_info_with_api_key() {
        let config = ProviderConfig {
            name: "test".to_string(),
            api_key: Some("secret".to_string()),
            base_url: None,
            default_model: "model".to_string(),
            requires_api_key: true,
        };
        let info = format_provider_info(&config);
        assert!(info.contains("configured"));
        assert!(!info.contains("secret")); // Should not leak the key
    }

    #[test]
    fn test_format_provider_info_missing_api_key() {
        let config = ProviderConfig {
            name: "test".to_string(),
            api_key: None,
            base_url: None,
            default_model: "model".to_string(),
            requires_api_key: true,
        };
        let info = format_provider_info(&config);
        assert!(info.contains("not configured"));
    }

    // ==================== ProviderConfig tests ====================

    #[test]
    fn test_provider_config_debug() {
        let config = ProviderConfig {
            name: "test".to_string(),
            api_key: None,
            base_url: None,
            default_model: "model".to_string(),
            requires_api_key: false,
        };
        let debug = format!("{:?}", config);
        assert!(debug.contains("test"));
    }

    #[test]
    fn test_provider_config_clone() {
        let config = ProviderConfig {
            name: "test".to_string(),
            api_key: Some("key".to_string()),
            base_url: Some("url".to_string()),
            default_model: "model".to_string(),
            requires_api_key: true,
        };
        let cloned = config.clone();
        assert_eq!(config.name, cloned.name);
        assert_eq!(config.api_key, cloned.api_key);
    }

    // ==================== ProviderValidation tests ====================

    #[test]
    fn test_provider_validation_eq() {
        assert_eq!(ProviderValidation::Valid, ProviderValidation::Valid);
        assert_ne!(
            ProviderValidation::Valid,
            ProviderValidation::Invalid("".to_string())
        );
    }

    #[test]
    fn test_provider_validation_debug() {
        let valid = ProviderValidation::Valid;
        let debug = format!("{:?}", valid);
        assert!(debug.contains("Valid"));
    }

    // ==================== Edge cases ====================

    #[test]
    fn test_resolve_model_empty_strings() {
        let settings = create_test_settings();
        let result = resolve_model_name(Some(""), None, "anthropic", &settings);
        // Empty string arg should still be used (caller's responsibility to validate)
        assert_eq!(result, "");
    }

    #[test]
    fn test_validate_api_key_whitespace() {
        let result = validate_api_key_format("anthropic", "   ");
        // Whitespace-only is not empty but also not valid
        assert!(matches!(result, ApiKeyValidation::Warning(_)));
    }
}
