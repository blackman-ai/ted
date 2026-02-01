// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Settings management for Ted
//!
//! Handles loading and saving settings from ~/.ted/settings.json

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::Result;
use crate::hardware::{HardwareTier, SystemProfile};

/// Main settings structure, stored in ~/.ted/settings.json
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    /// LLM provider configurations
    #[serde(default)]
    pub providers: ProvidersConfig,

    /// Default settings for new sessions
    #[serde(default)]
    pub defaults: DefaultsConfig,

    /// Context storage settings
    #[serde(default)]
    pub context: ContextConfig,

    /// Appearance settings
    #[serde(default)]
    pub appearance: AppearanceConfig,

    /// Hardware profile and tier information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hardware: Option<HardwareConfig>,
}

/// Configuration for LLM providers
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvidersConfig {
    /// Anthropic Claude configuration
    #[serde(default)]
    pub anthropic: AnthropicConfig,

    /// Ollama local model configuration
    #[serde(default)]
    pub ollama: OllamaConfig,

    /// OpenRouter configuration (100+ models via single API)
    #[serde(default)]
    pub openrouter: OpenRouterConfig,

    /// Blackman AI configuration (optimized routing with cost savings)
    #[serde(default)]
    pub blackman: BlackmanConfig,

    /// OpenAI configuration (future)
    #[serde(default)]
    pub openai: Option<OpenAIConfig>,

    /// Google configuration (future)
    #[serde(default)]
    pub google: Option<GoogleConfig>,
}

/// Anthropic-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    /// API key (if stored directly, not recommended)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Environment variable name for API key
    #[serde(default = "default_anthropic_api_key_env")]
    pub api_key_env: String,

    /// Default model to use
    #[serde(default = "default_anthropic_model")]
    pub default_model: String,

    /// Base URL for API (for custom endpoints)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

/// Ollama local model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Base URL for Ollama API
    #[serde(default = "default_ollama_base_url")]
    pub base_url: String,

    /// Default model to use with Ollama
    #[serde(default = "default_ollama_model")]
    pub default_model: String,
}

/// OpenRouter configuration (100+ models via single API)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterConfig {
    /// API key (if stored directly, not recommended)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Environment variable name for API key
    #[serde(default = "default_openrouter_api_key_env")]
    pub api_key_env: String,

    /// Default model to use
    #[serde(default = "default_openrouter_model")]
    pub default_model: String,

    /// Base URL for API (for custom endpoints)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

/// Blackman AI configuration (optimized routing with cost savings)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlackmanConfig {
    /// API key (if stored directly, not recommended)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Environment variable name for API key
    #[serde(default = "default_blackman_api_key_env")]
    pub api_key_env: String,

    /// Default model to use
    #[serde(default = "default_blackman_model")]
    pub default_model: String,

    /// Base URL for API
    #[serde(default = "default_blackman_base_url")]
    pub base_url: String,
}

impl Default for BlackmanConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            api_key_env: default_blackman_api_key_env(),
            default_model: default_blackman_model(),
            base_url: default_blackman_base_url(),
        }
    }
}

/// OpenAI configuration (placeholder for future)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    pub api_key: Option<String>,
    pub api_key_env: String,
    pub default_model: String,
}

/// Google configuration (placeholder for future)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleConfig {
    pub api_key: Option<String>,
    pub api_key_env: String,
    pub default_model: String,
}

/// Default settings for new sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    /// Default caps to load
    #[serde(default = "default_caps")]
    pub caps: Vec<String>,

    /// Default temperature for LLM
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Whether to use streaming by default
    #[serde(default = "default_true")]
    pub stream: bool,

    /// Default provider to use
    #[serde(default = "default_provider")]
    pub provider: String,

    /// Maximum tokens for response
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

/// Context storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Base path for context storage
    #[serde(default = "default_context_path")]
    pub storage_path: PathBuf,

    /// Maximum number of warm chunks to keep
    #[serde(default = "default_max_warm_chunks")]
    pub max_warm_chunks: usize,

    /// Days to retain cold storage
    #[serde(default = "default_cold_retention_days")]
    pub cold_retention_days: u32,

    /// Enable automatic compaction
    #[serde(default = "default_true")]
    pub auto_compact: bool,
}

/// Appearance settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    /// Enable syntax highlighting
    #[serde(default = "default_true")]
    pub syntax_highlighting: bool,

    /// Show token count in UI
    #[serde(default)]
    pub show_token_count: bool,

    /// Theme (for future TUI)
    #[serde(default = "default_theme")]
    pub theme: String,
}

/// Hardware-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareConfig {
    /// Detected hardware tier
    pub tier: HardwareTier,

    /// Override detected tier (user preference)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier_override: Option<HardwareTier>,

    /// Enable hardware-adaptive behavior
    #[serde(default = "default_true")]
    pub adaptive_mode: bool,

    /// Last hardware detection timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_detection: Option<String>,
}

// Default value functions
fn default_anthropic_api_key_env() -> String {
    "ANTHROPIC_API_KEY".to_string()
}

fn default_anthropic_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

fn default_ollama_base_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_ollama_model() -> String {
    "qwen2.5-coder:14b".to_string()
}

fn default_openrouter_api_key_env() -> String {
    "OPENROUTER_API_KEY".to_string()
}

fn default_openrouter_model() -> String {
    "anthropic/claude-3.5-sonnet".to_string()
}

fn default_blackman_api_key_env() -> String {
    "BLACKMAN_API_KEY".to_string()
}

fn default_blackman_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

fn default_blackman_base_url() -> String {
    "https://app.useblackman.ai".to_string()
}

fn default_caps() -> Vec<String> {
    vec!["base".to_string()]
}

fn default_temperature() -> f32 {
    0.7
}

fn default_true() -> bool {
    true
}

fn default_provider() -> String {
    "anthropic".to_string()
}

fn default_max_tokens() -> u32 {
    8192
}

fn default_context_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ted")
        .join("context")
}

fn default_max_warm_chunks() -> usize {
    100
}

fn default_cold_retention_days() -> u32 {
    30
}

fn default_theme() -> String {
    "default".to_string()
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            api_key_env: default_anthropic_api_key_env(),
            default_model: default_anthropic_model(),
            base_url: None,
        }
    }
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: default_ollama_base_url(),
            default_model: default_ollama_model(),
        }
    }
}

impl Default for OpenRouterConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            api_key_env: default_openrouter_api_key_env(),
            default_model: default_openrouter_model(),
            base_url: None,
        }
    }
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            caps: default_caps(),
            temperature: default_temperature(),
            stream: true,
            provider: default_provider(),
            max_tokens: default_max_tokens(),
        }
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            storage_path: default_context_path(),
            max_warm_chunks: default_max_warm_chunks(),
            cold_retention_days: default_cold_retention_days(),
            auto_compact: true,
        }
    }
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            syntax_highlighting: true,
            show_token_count: false,
            theme: default_theme(),
        }
    }
}

impl Settings {
    /// Get the default settings file path
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ted")
            .join("settings.json")
    }

    /// Load settings from the default path
    pub fn load() -> Result<Self> {
        Self::load_from(&Self::default_path())
    }

    /// Load settings from a specific path
    pub fn load_from(path: &PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)?;
        let settings: Settings = serde_json::from_str(&content)?;
        Ok(settings)
    }

    /// Save settings to the default path
    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::default_path())
    }

    /// Save settings to a specific path
    pub fn save_to(&self, path: &PathBuf) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get the API key for Anthropic, checking env var first
    pub fn get_anthropic_api_key(&self) -> Option<String> {
        // Priority: env var > config file
        std::env::var(&self.providers.anthropic.api_key_env)
            .ok()
            .or_else(|| self.providers.anthropic.api_key.clone())
    }

    /// Get the API key for OpenRouter, checking env var first
    pub fn get_openrouter_api_key(&self) -> Option<String> {
        // Priority: env var > config file
        std::env::var(&self.providers.openrouter.api_key_env)
            .ok()
            .or_else(|| self.providers.openrouter.api_key.clone())
    }

    /// Get the API key for Blackman AI, checking env var first
    pub fn get_blackman_api_key(&self) -> Option<String> {
        // Priority: env var > config file
        std::env::var(&self.providers.blackman.api_key_env)
            .ok()
            .or_else(|| self.providers.blackman.api_key.clone())
    }

    /// Get the Blackman base URL, checking env var first
    pub fn get_blackman_base_url(&self) -> String {
        // Priority: env var > config file
        std::env::var("BLACKMAN_BASE_URL")
            .ok()
            .unwrap_or_else(|| self.providers.blackman.base_url.clone())
    }

    /// Get the ted home directory (~/.ted)
    pub fn ted_home() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ted")
    }

    /// Get the caps directory
    pub fn caps_dir() -> PathBuf {
        Self::ted_home().join("caps")
    }

    /// Get the commands directory
    pub fn commands_dir() -> PathBuf {
        Self::ted_home().join("commands")
    }

    /// Get the history directory
    pub fn history_dir() -> PathBuf {
        Self::ted_home().join("history")
    }

    /// Get the context storage directory
    pub fn context_path() -> PathBuf {
        Self::ted_home().join("context")
    }

    /// Get the plans directory
    pub fn plans_dir() -> PathBuf {
        Self::ted_home().join("plans")
    }

    /// Ensure all required directories exist
    pub fn ensure_directories() -> Result<()> {
        let dirs = [
            Self::ted_home(),
            Self::caps_dir(),
            Self::commands_dir(),
            Self::context_path(),
            Self::plans_dir(),
            Self::default_path().parent().unwrap().to_path_buf(),
        ];

        for dir in dirs {
            if !dir.exists() {
                std::fs::create_dir_all(&dir)?;
            }
        }

        Ok(())
    }

    /// Detect hardware and update configuration
    pub fn detect_hardware(&mut self) -> Result<()> {
        let profile = SystemProfile::detect()?;
        let now = chrono::Utc::now().to_rfc3339();

        self.hardware = Some(HardwareConfig {
            tier: profile.tier,
            tier_override: None,
            adaptive_mode: true,
            last_detection: Some(now),
        });

        Ok(())
    }

    /// Get the effective hardware tier (considering overrides)
    pub fn effective_tier(&self) -> HardwareTier {
        if let Some(ref hw) = self.hardware {
            hw.tier_override.unwrap_or(hw.tier)
        } else {
            // Default to Medium if not detected
            HardwareTier::Medium
        }
    }

    /// Apply hardware-adaptive configuration to context settings
    pub fn apply_hardware_adaptive_config(&mut self) {
        let tier = self.effective_tier();

        // Only apply if adaptive mode is enabled
        if let Some(ref hw) = self.hardware {
            if !hw.adaptive_mode {
                return;
            }
        }

        // Update context config based on tier
        self.context.max_warm_chunks = tier.max_warm_chunks();

        // Update defaults based on tier
        self.defaults.max_tokens = tier.max_context_tokens() as u32;

        // For ancient/tiny tiers, prefer local models
        if matches!(
            tier,
            HardwareTier::UltraTiny | HardwareTier::Ancient | HardwareTier::Tiny
        ) {
            self.defaults.provider = "ollama".to_string();
            let models = tier.recommended_models();
            if !models.is_empty() {
                self.providers.ollama.default_model = models[0].to_string();
            }
        }
    }

    /// Get hardware-specific warnings or recommendations
    pub fn get_hardware_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        if let Some(ref hw) = self.hardware {
            let tier = hw.tier_override.unwrap_or(hw.tier);

            match tier {
                HardwareTier::UltraTiny => {
                    warnings.push(
                        "🎓 Raspberry Pi detected (Education Mode). Expected AI response: 20-40 seconds.".to_string()
                    );
                }
                HardwareTier::Ancient => {
                    warnings.push(
                        "🐢 Ancient hardware detected. Expected response time: 30-60 seconds. Consider upgrading RAM or SSD for better performance.".to_string()
                    );
                }
                _ => {}
            }
        }

        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_settings_default() {
        let settings = Settings::default();
        assert!(!settings.defaults.caps.is_empty());
        assert!(settings.defaults.caps.contains(&"base".to_string()));
    }

    #[test]
    fn test_anthropic_config_default() {
        let config = AnthropicConfig::default();
        assert!(config.api_key.is_none());
        assert_eq!(config.api_key_env, "ANTHROPIC_API_KEY");
        assert!(config.default_model.contains("claude"));
    }

    #[test]
    fn test_defaults_config_default() {
        let config = DefaultsConfig::default();
        assert_eq!(config.temperature, 0.7);
        assert!(config.stream);
        assert_eq!(config.provider, "anthropic");
        assert_eq!(config.max_tokens, 8192);
    }

    #[test]
    fn test_context_config_default() {
        let config = ContextConfig::default();
        assert_eq!(config.max_warm_chunks, 100);
        assert_eq!(config.cold_retention_days, 30);
        assert!(config.auto_compact);
    }

    #[test]
    fn test_appearance_config_default() {
        let config = AppearanceConfig::default();
        assert!(config.syntax_highlighting);
        assert!(!config.show_token_count);
        assert_eq!(config.theme, "default");
    }

    #[test]
    fn test_providers_config_default() {
        let config = ProvidersConfig::default();
        assert!(config.openai.is_none());
        assert!(config.google.is_none());
    }

    #[test]
    fn test_settings_load_from_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("nonexistent.json");

        let settings = Settings::load_from(&path).unwrap();
        // Should return default settings
        assert!(settings.defaults.caps.contains(&"base".to_string()));
    }

    #[test]
    fn test_settings_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test_settings.json");

        let mut settings = Settings::default();
        settings.defaults.temperature = 0.5;
        settings.defaults.caps = vec!["rust-expert".to_string()];

        settings.save_to(&path).unwrap();

        let loaded = Settings::load_from(&path).unwrap();
        assert_eq!(loaded.defaults.temperature, 0.5);
        assert!(loaded.defaults.caps.contains(&"rust-expert".to_string()));
    }

    #[test]
    fn test_settings_save_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir
            .path()
            .join("nested")
            .join("dir")
            .join("settings.json");

        let settings = Settings::default();
        settings.save_to(&path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_settings_clone() {
        let settings = Settings::default();
        let cloned = settings.clone();
        assert_eq!(cloned.defaults.temperature, settings.defaults.temperature);
    }

    #[test]
    fn test_settings_debug() {
        let settings = Settings::default();
        let debug_str = format!("{:?}", settings);
        assert!(debug_str.contains("Settings"));
    }

    #[test]
    fn test_default_path() {
        let path = Settings::default_path();
        assert!(path.ends_with("settings.json"));
    }

    #[test]
    fn test_ted_home() {
        let home = Settings::ted_home();
        assert!(home.ends_with(".ted"));
    }

    #[test]
    fn test_caps_dir() {
        let caps = Settings::caps_dir();
        assert!(caps.ends_with("caps"));
    }

    #[test]
    fn test_commands_dir() {
        let commands = Settings::commands_dir();
        assert!(commands.ends_with("commands"));
    }

    #[test]
    fn test_history_dir() {
        let history = Settings::history_dir();
        assert!(history.ends_with("history"));
    }

    #[test]
    fn test_context_path() {
        let context = Settings::context_path();
        assert!(context.ends_with("context"));
    }

    #[test]
    fn test_plans_dir() {
        let plans = Settings::plans_dir();
        assert!(plans.ends_with("plans"));
    }

    #[test]
    fn test_get_anthropic_api_key_from_config() {
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = Some("test-key".to_string());
        settings.providers.anthropic.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let key = settings.get_anthropic_api_key();
        assert_eq!(key, Some("test-key".to_string()));
    }

    #[test]
    fn test_get_anthropic_api_key_none() {
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = None;
        settings.providers.anthropic.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let key = settings.get_anthropic_api_key();
        assert!(key.is_none());
    }

    #[test]
    fn test_settings_serialization_roundtrip() {
        let settings = Settings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.defaults.temperature, settings.defaults.temperature);
        assert_eq!(parsed.defaults.caps, settings.defaults.caps);
    }

    #[test]
    fn test_settings_partial_json() {
        // Test that partial JSON still works with defaults
        let json = r#"{"defaults": {"temperature": 0.9}}"#;
        let settings: Settings = serde_json::from_str(json).unwrap();

        assert_eq!(settings.defaults.temperature, 0.9);
        // Other fields should use defaults
        assert!(settings.defaults.stream);
        assert!(settings.defaults.caps.contains(&"base".to_string()));
    }

    #[test]
    fn test_anthropic_config_with_base_url() {
        let json = r#"{"base_url": "https://custom.api.com"}"#;
        let config: AnthropicConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.base_url, Some("https://custom.api.com".to_string()));
    }

    #[test]
    fn test_openai_config() {
        let config = OpenAIConfig {
            api_key: Some("sk-test".to_string()),
            api_key_env: "OPENAI_API_KEY".to_string(),
            default_model: "gpt-4".to_string(),
        };

        assert_eq!(config.api_key, Some("sk-test".to_string()));
        assert_eq!(config.default_model, "gpt-4");
    }

    #[test]
    fn test_google_config() {
        let config = GoogleConfig {
            api_key: Some("google-key".to_string()),
            api_key_env: "GOOGLE_API_KEY".to_string(),
            default_model: "gemini-pro".to_string(),
        };

        assert_eq!(config.api_key, Some("google-key".to_string()));
        assert_eq!(config.default_model, "gemini-pro");
    }

    #[test]
    fn test_settings_with_all_providers() {
        let mut settings = Settings::default();
        settings.providers.openai = Some(OpenAIConfig {
            api_key: None,
            api_key_env: "OPENAI_API_KEY".to_string(),
            default_model: "gpt-4".to_string(),
        });
        settings.providers.google = Some(GoogleConfig {
            api_key: None,
            api_key_env: "GOOGLE_API_KEY".to_string(),
            default_model: "gemini-pro".to_string(),
        });

        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();

        assert!(parsed.providers.openai.is_some());
        assert!(parsed.providers.google.is_some());
    }

    #[test]
    fn test_default_functions() {
        assert_eq!(default_anthropic_api_key_env(), "ANTHROPIC_API_KEY");
        assert!(default_anthropic_model().contains("claude"));
        assert_eq!(default_caps(), vec!["base".to_string()]);
        assert_eq!(default_temperature(), 0.7);
        assert!(default_true());
        assert_eq!(default_provider(), "anthropic");
        assert_eq!(default_max_tokens(), 8192);
        assert_eq!(default_max_warm_chunks(), 100);
        assert_eq!(default_cold_retention_days(), 30);
        assert_eq!(default_theme(), "default");
    }

    #[test]
    fn test_effective_tier_with_override() {
        let settings = Settings {
            hardware: Some(HardwareConfig {
                tier: HardwareTier::Ancient,
                tier_override: Some(HardwareTier::Small),
                adaptive_mode: true,
                last_detection: None,
            }),
            ..Default::default()
        };

        assert_eq!(settings.effective_tier(), HardwareTier::Small);
    }

    #[test]
    fn test_effective_tier_without_override() {
        let settings = Settings {
            hardware: Some(HardwareConfig {
                tier: HardwareTier::Medium,
                tier_override: None,
                adaptive_mode: true,
                last_detection: None,
            }),
            ..Default::default()
        };

        assert_eq!(settings.effective_tier(), HardwareTier::Medium);
    }

    #[test]
    fn test_effective_tier_default() {
        let settings = Settings::default();
        assert_eq!(settings.effective_tier(), HardwareTier::Medium);
    }

    #[test]
    fn test_apply_hardware_adaptive_config() {
        let mut settings = Settings {
            hardware: Some(HardwareConfig {
                tier: HardwareTier::Ancient,
                tier_override: None,
                adaptive_mode: true,
                last_detection: None,
            }),
            ..Default::default()
        };

        settings.apply_hardware_adaptive_config();

        assert_eq!(settings.context.max_warm_chunks, 10);
        assert_eq!(settings.defaults.max_tokens, 1024);
        assert_eq!(settings.defaults.provider, "ollama");
    }

    #[test]
    fn test_get_hardware_warnings() {
        let settings = Settings {
            hardware: Some(HardwareConfig {
                tier: HardwareTier::Ancient,
                tier_override: None,
                adaptive_mode: true,
                last_detection: None,
            }),
            ..Default::default()
        };

        let warnings = settings.get_hardware_warnings();
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("Ancient hardware"));
    }

    #[test]
    fn test_context_config_storage_path() {
        let config = ContextConfig::default();
        // Storage path should contain .ted/context
        let path_str = config.storage_path.to_string_lossy();
        assert!(path_str.contains(".ted"));
        assert!(path_str.contains("context"));
    }

    #[test]
    fn test_ollama_config_default() {
        let config = OllamaConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
        assert_eq!(config.default_model, "qwen2.5-coder:14b");
    }

    #[test]
    fn test_ollama_config_serialization() {
        let config = OllamaConfig {
            base_url: "http://custom:8080".to_string(),
            default_model: "llama3.2:latest".to_string(),
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: OllamaConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.base_url, "http://custom:8080");
        assert_eq!(parsed.default_model, "llama3.2:latest");
    }

    #[test]
    fn test_ollama_default_functions() {
        assert_eq!(default_ollama_base_url(), "http://localhost:11434");
        assert_eq!(default_ollama_model(), "qwen2.5-coder:14b");
    }

    #[test]
    fn test_settings_with_ollama_provider() {
        let mut settings = Settings::default();
        settings.providers.ollama.base_url = "http://custom:8080".to_string();
        settings.providers.ollama.default_model = "codellama:latest".to_string();

        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.providers.ollama.base_url, "http://custom:8080");
        assert_eq!(parsed.providers.ollama.default_model, "codellama:latest");
    }
}
