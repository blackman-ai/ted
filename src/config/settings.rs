// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Settings management for Ted
//!
//! Handles loading and saving settings from ~/.ted/settings.json

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::hardware::HardwareTier;

mod io;
mod migration;
pub mod schema;
mod validation;

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

    /// Conversation and token management settings
    #[serde(default)]
    pub conversation: ConversationConfig,

    /// Retry and resilience settings for API calls
    #[serde(default)]
    pub resilience: ResilienceConfig,

    /// Rate limiting settings for token budget allocation
    #[serde(default)]
    pub rate_limits: RateLimitsConfig,

    /// Hardware profile and tier information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hardware: Option<HardwareConfig>,

    /// Embeddings configuration (for semantic search)
    #[serde(default)]
    pub embeddings: EmbeddingsConfig,
}

/// Configuration for LLM providers
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvidersConfig {
    /// Anthropic Claude configuration
    #[serde(default)]
    pub anthropic: AnthropicConfig,

    /// Local LLM configuration (llama-server subprocess)
    #[serde(default)]
    pub local: LocalLlmConfig,

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

/// Local LLM configuration (llama-server subprocess)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLlmConfig {
    /// Port for llama-server (default: 8847)
    #[serde(default = "default_local_port")]
    pub port: u16,

    /// Optional base URL for an existing OpenAI-compatible local server.
    /// When set, Ted will use this endpoint instead of launching llama-server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    /// GPU layers to offload (None = auto-detect)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_layers: Option<i32>,

    /// Context size (None = model default)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctx_size: Option<u32>,

    /// Default model name for identification
    #[serde(default = "default_local_model")]
    pub default_model: String,

    /// Path to the GGUF model file
    #[serde(default = "default_local_model_path")]
    pub model_path: PathBuf,
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

impl Default for LocalLlmConfig {
    fn default() -> Self {
        Self {
            port: default_local_port(),
            base_url: None,
            gpu_layers: None,
            ctx_size: None,
            default_model: default_local_model(),
            model_path: default_local_model_path(),
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

/// Embeddings configuration for semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsConfig {
    /// Backend to use: "bundled" (default, no deps)
    #[serde(default = "default_embeddings_backend")]
    pub backend: String,

    /// Model name for bundled embeddings
    /// Options: "all-minilm-l6-v2", "nomic-embed-text-v1.5", "bge-small-en-v1.5"
    #[serde(default = "default_embeddings_model")]
    pub model: String,

    /// Enable semantic search in indexer
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for EmbeddingsConfig {
    fn default() -> Self {
        Self {
            backend: default_embeddings_backend(),
            model: default_embeddings_model(),
            enabled: true,
        }
    }
}

fn default_embeddings_backend() -> String {
    "bundled".to_string()
}

fn default_embeddings_model() -> String {
    "all-minilm-l6-v2".to_string()
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

/// Conversation and token management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationConfig {
    /// Buffer tokens reserved for LLM response
    #[serde(default = "default_response_buffer_tokens")]
    pub response_buffer_tokens: u32,

    /// Estimated characters per token for calculations
    #[serde(default = "default_chars_per_token")]
    pub chars_per_token: u32,

    /// Threshold (0.0-1.0) at which to start trimming conversation
    #[serde(default = "default_trimming_threshold")]
    pub trimming_threshold: f64,

    /// Overhead tokens per message for metadata
    #[serde(default = "default_message_overhead_tokens")]
    pub message_overhead_tokens: u32,

    /// Estimated tokens per image
    #[serde(default = "default_image_token_estimate")]
    pub image_token_estimate: u32,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            response_buffer_tokens: default_response_buffer_tokens(),
            chars_per_token: default_chars_per_token(),
            trimming_threshold: default_trimming_threshold(),
            message_overhead_tokens: default_message_overhead_tokens(),
            image_token_estimate: default_image_token_estimate(),
        }
    }
}

/// Retry and resilience configuration for API calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResilienceConfig {
    /// Maximum number of retry attempts
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Base delay in milliseconds for exponential backoff
    #[serde(default = "default_base_delay_ms")]
    pub base_delay_ms: u64,

    /// Maximum delay in milliseconds (cap for backoff)
    #[serde(default = "default_max_delay_ms")]
    pub max_delay_ms: u64,

    /// Jitter percentage (0.0 to 1.0) for randomizing delays
    #[serde(default = "default_jitter")]
    pub jitter: f64,

    /// Circuit breaker: max consecutive failures before opening circuit
    #[serde(default = "default_circuit_failure_threshold")]
    pub circuit_failure_threshold: u32,

    /// Circuit breaker: cooldown in seconds before half-open state
    #[serde(default = "default_circuit_cooldown_secs")]
    pub circuit_cooldown_secs: u64,
}

impl Default for ResilienceConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            base_delay_ms: default_base_delay_ms(),
            max_delay_ms: default_max_delay_ms(),
            jitter: default_jitter(),
            circuit_failure_threshold: default_circuit_failure_threshold(),
            circuit_cooldown_secs: default_circuit_cooldown_secs(),
        }
    }
}

/// Rate limiting configuration for token budget allocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitsConfig {
    /// Model-specific rate limits (prefix matching, e.g., "claude-sonnet-4")
    #[serde(default = "default_rate_limit_models")]
    pub models: HashMap<String, ModelRateLimit>,

    /// Default rate limit for unknown models
    #[serde(default = "default_model_rate_limit")]
    pub default: ModelRateLimit,

    /// Whether rate budget allocation is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Rate limit for a specific model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRateLimit {
    /// Tokens per minute limit
    pub tokens_per_minute: u64,
}

impl Default for RateLimitsConfig {
    fn default() -> Self {
        Self {
            models: default_rate_limit_models(),
            default: default_model_rate_limit(),
            enabled: true,
        }
    }
}

impl RateLimitsConfig {
    /// Get the rate limit for a model by name (uses prefix matching)
    pub fn get_for_model(&self, model: &str) -> &ModelRateLimit {
        // Try exact match first
        if let Some(limit) = self.models.get(model) {
            return limit;
        }

        // Try prefix matching (e.g., "claude-sonnet-4-20250514" matches "claude-sonnet-4")
        for (prefix, limit) in &self.models {
            if model.starts_with(prefix) {
                return limit;
            }
        }

        // Fall back to default
        &self.default
    }
}

fn default_rate_limit_models() -> HashMap<String, ModelRateLimit> {
    let mut models = HashMap::new();
    // Claude Sonnet 4.5 - 450K tokens/min (Tier 3+)
    models.insert(
        "claude-sonnet-4".to_string(),
        ModelRateLimit {
            tokens_per_minute: 450_000,
        },
    );
    // Claude Opus 4 - 80K tokens/min (more limited)
    models.insert(
        "claude-opus-4".to_string(),
        ModelRateLimit {
            tokens_per_minute: 80_000,
        },
    );
    // Claude Haiku - 1M tokens/min (very fast)
    models.insert(
        "claude-haiku".to_string(),
        ModelRateLimit {
            tokens_per_minute: 1_000_000,
        },
    );
    models
}

fn default_model_rate_limit() -> ModelRateLimit {
    ModelRateLimit {
        tokens_per_minute: 100_000,
    }
}

// Default value functions
fn default_anthropic_api_key_env() -> String {
    "ANTHROPIC_API_KEY".to_string()
}

fn default_anthropic_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

fn default_local_port() -> u16 {
    8847
}

fn default_local_model() -> String {
    "local".to_string()
}

fn default_local_model_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ted")
        .join("models")
        .join("local")
        .join("model.gguf")
}

fn default_openrouter_api_key_env() -> String {
    "OPENROUTER_API_KEY".to_string()
}

fn default_openrouter_model() -> String {
    "anthropic/claude-sonnet-4.5".to_string()
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

// Conversation config defaults
fn default_response_buffer_tokens() -> u32 {
    4096
}

fn default_chars_per_token() -> u32 {
    4
}

fn default_trimming_threshold() -> f64 {
    0.8
}

fn default_message_overhead_tokens() -> u32 {
    20
}

fn default_image_token_estimate() -> u32 {
    1000
}

// Resilience config defaults
fn default_max_retries() -> u32 {
    5
}

fn default_base_delay_ms() -> u64 {
    1000
}

fn default_max_delay_ms() -> u64 {
    16000
}

fn default_jitter() -> f64 {
    0.25
}

fn default_circuit_failure_threshold() -> u32 {
    5
}

fn default_circuit_cooldown_secs() -> u64 {
    10
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
        assert_eq!(settings.defaults.provider, "local");
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
    fn test_local_llm_config_default() {
        let config = LocalLlmConfig::default();
        assert_eq!(config.port, 8847);
        assert_eq!(config.default_model, "local");
        assert!(config.base_url.is_none());
        assert!(config.gpu_layers.is_none());
        assert!(config.ctx_size.is_none());
    }

    #[test]
    fn test_local_llm_config_serialization() {
        let config = LocalLlmConfig {
            port: 9999,
            base_url: Some("http://127.0.0.1:1234".to_string()),
            gpu_layers: Some(32),
            ctx_size: Some(8192),
            default_model: "qwen2.5-coder".to_string(),
            model_path: PathBuf::from("/models/test.gguf"),
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: LocalLlmConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.port, 9999);
        assert_eq!(parsed.base_url.as_deref(), Some("http://127.0.0.1:1234"));
        assert_eq!(parsed.gpu_layers, Some(32));
        assert_eq!(parsed.ctx_size, Some(8192));
    }

    #[test]
    fn test_conversation_config_default() {
        let config = ConversationConfig::default();
        assert_eq!(config.response_buffer_tokens, 4096);
        assert_eq!(config.chars_per_token, 4);
        assert!((config.trimming_threshold - 0.8).abs() < 0.001);
        assert_eq!(config.message_overhead_tokens, 20);
        assert_eq!(config.image_token_estimate, 1000);
    }

    #[test]
    fn test_conversation_config_serialization() {
        let config = ConversationConfig {
            response_buffer_tokens: 8192,
            chars_per_token: 3,
            trimming_threshold: 0.9,
            message_overhead_tokens: 25,
            image_token_estimate: 1500,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: ConversationConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.response_buffer_tokens, 8192);
        assert_eq!(parsed.chars_per_token, 3);
        assert!((parsed.trimming_threshold - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_resilience_config_default() {
        let config = ResilienceConfig::default();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.base_delay_ms, 1000);
        assert_eq!(config.max_delay_ms, 16000);
        assert!((config.jitter - 0.25).abs() < 0.001);
        assert_eq!(config.circuit_failure_threshold, 5);
        assert_eq!(config.circuit_cooldown_secs, 10);
    }

    #[test]
    fn test_resilience_config_serialization() {
        let config = ResilienceConfig {
            max_retries: 10,
            base_delay_ms: 500,
            max_delay_ms: 30000,
            jitter: 0.5,
            circuit_failure_threshold: 3,
            circuit_cooldown_secs: 5,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: ResilienceConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.max_retries, 10);
        assert_eq!(parsed.base_delay_ms, 500);
        assert_eq!(parsed.max_delay_ms, 30000);
        assert!((parsed.jitter - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_settings_with_conversation_config() {
        let mut settings = Settings::default();
        settings.conversation.response_buffer_tokens = 2048;
        settings.conversation.trimming_threshold = 0.7;

        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.conversation.response_buffer_tokens, 2048);
        assert!((parsed.conversation.trimming_threshold - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_settings_with_resilience_config() {
        let mut settings = Settings::default();
        settings.resilience.max_retries = 3;
        settings.resilience.base_delay_ms = 2000;

        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.resilience.max_retries, 3);
        assert_eq!(parsed.resilience.base_delay_ms, 2000);
    }

    #[test]
    fn test_conversation_config_default_functions() {
        assert_eq!(default_response_buffer_tokens(), 4096);
        assert_eq!(default_chars_per_token(), 4);
        assert!((default_trimming_threshold() - 0.8).abs() < 0.001);
        assert_eq!(default_message_overhead_tokens(), 20);
        assert_eq!(default_image_token_estimate(), 1000);
    }

    #[test]
    fn test_resilience_config_default_functions() {
        assert_eq!(default_max_retries(), 5);
        assert_eq!(default_base_delay_ms(), 1000);
        assert_eq!(default_max_delay_ms(), 16000);
        assert!((default_jitter() - 0.25).abs() < 0.001);
        assert_eq!(default_circuit_failure_threshold(), 5);
        assert_eq!(default_circuit_cooldown_secs(), 10);
    }

    // ===== Additional settings tests for coverage =====

    #[test]
    fn test_get_openrouter_api_key_from_config() {
        let mut settings = Settings::default();
        settings.providers.openrouter.api_key = Some("or-key".to_string());
        settings.providers.openrouter.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let key = settings.get_openrouter_api_key();
        assert_eq!(key, Some("or-key".to_string()));
    }

    #[test]
    fn test_get_openrouter_api_key_none() {
        let mut settings = Settings::default();
        settings.providers.openrouter.api_key = None;
        settings.providers.openrouter.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let key = settings.get_openrouter_api_key();
        assert!(key.is_none());
    }

    #[test]
    fn test_get_blackman_api_key_from_config() {
        let mut settings = Settings::default();
        settings.providers.blackman.api_key = Some("bm-key".to_string());
        settings.providers.blackman.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let key = settings.get_blackman_api_key();
        assert_eq!(key, Some("bm-key".to_string()));
    }

    #[test]
    fn test_get_blackman_api_key_none() {
        let mut settings = Settings::default();
        settings.providers.blackman.api_key = None;
        settings.providers.blackman.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();

        let key = settings.get_blackman_api_key();
        assert!(key.is_none());
    }

    #[test]
    fn test_get_blackman_base_url_from_config() {
        // Ensure env var is not set
        std::env::remove_var("BLACKMAN_BASE_URL");

        let mut settings = Settings::default();
        settings.providers.blackman.base_url = "https://custom.api".to_string();

        let url = settings.get_blackman_base_url();
        assert_eq!(url, "https://custom.api");
    }

    #[test]
    fn test_apply_hardware_adaptive_config_disabled() {
        let mut settings = Settings {
            hardware: Some(HardwareConfig {
                tier: HardwareTier::Ancient,
                tier_override: None,
                adaptive_mode: false, // Disabled!
                last_detection: None,
            }),
            ..Default::default()
        };

        let original_warm_chunks = settings.context.max_warm_chunks;
        settings.apply_hardware_adaptive_config();

        // Should not change when adaptive_mode is disabled
        assert_eq!(settings.context.max_warm_chunks, original_warm_chunks);
    }

    #[test]
    fn test_apply_hardware_adaptive_config_ultratiny() {
        let mut settings = Settings {
            hardware: Some(HardwareConfig {
                tier: HardwareTier::UltraTiny,
                tier_override: None,
                adaptive_mode: true,
                last_detection: None,
            }),
            ..Default::default()
        };

        settings.apply_hardware_adaptive_config();

        // UltraTiny should set provider to local
        assert_eq!(settings.defaults.provider, "local");
    }

    #[test]
    fn test_apply_hardware_adaptive_config_tiny() {
        let mut settings = Settings {
            hardware: Some(HardwareConfig {
                tier: HardwareTier::Tiny,
                tier_override: None,
                adaptive_mode: true,
                last_detection: None,
            }),
            ..Default::default()
        };

        settings.apply_hardware_adaptive_config();

        // Tiny should set provider to local
        assert_eq!(settings.defaults.provider, "local");
    }

    #[test]
    fn test_get_hardware_warnings_ultratiny() {
        let settings = Settings {
            hardware: Some(HardwareConfig {
                tier: HardwareTier::UltraTiny,
                tier_override: None,
                adaptive_mode: true,
                last_detection: None,
            }),
            ..Default::default()
        };

        let warnings = settings.get_hardware_warnings();
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("Raspberry Pi") || warnings[0].contains("Education Mode"));
    }

    #[test]
    fn test_get_hardware_warnings_medium() {
        let settings = Settings {
            hardware: Some(HardwareConfig {
                tier: HardwareTier::Medium,
                tier_override: None,
                adaptive_mode: true,
                last_detection: None,
            }),
            ..Default::default()
        };

        let warnings = settings.get_hardware_warnings();
        // Medium tier should have no warnings
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_get_hardware_warnings_with_override() {
        let settings = Settings {
            hardware: Some(HardwareConfig {
                tier: HardwareTier::Ancient,
                tier_override: Some(HardwareTier::Medium), // Override to medium
                adaptive_mode: true,
                last_detection: None,
            }),
            ..Default::default()
        };

        let warnings = settings.get_hardware_warnings();
        // Override to medium should have no warnings
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_ensure_directories_in_temp() {
        let temp_dir = TempDir::new().unwrap();
        let test_path = temp_dir.path().join("test_dir").join("nested");

        // Create a test directory using same pattern as ensure_directories
        let dirs = [test_path.clone()];
        for dir in dirs {
            if !dir.exists() {
                std::fs::create_dir_all(&dir).unwrap();
            }
        }

        assert!(test_path.exists());
    }

    #[test]
    fn test_apply_hardware_adaptive_config_no_hardware() {
        let mut settings = Settings {
            hardware: None,
            ..Default::default()
        };

        let original_provider = settings.defaults.provider.clone();
        settings.apply_hardware_adaptive_config();

        // Should not change when hardware is None (effective_tier returns Medium)
        // Medium tier doesn't force local provider selection
        assert_eq!(settings.defaults.provider, original_provider);
    }

    #[test]
    fn test_get_hardware_warnings_no_hardware() {
        let settings = Settings {
            hardware: None,
            ..Default::default()
        };

        let warnings = settings.get_hardware_warnings();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_blackman_config_default() {
        let config = BlackmanConfig::default();
        assert!(config.api_key.is_none());
        assert_eq!(config.api_key_env, "BLACKMAN_API_KEY");
        assert!(config.base_url.contains("blackman") || !config.base_url.is_empty());
    }

    #[test]
    fn test_openrouter_config_default() {
        let config = OpenRouterConfig::default();
        assert!(config.api_key.is_none());
        assert_eq!(config.api_key_env, "OPENROUTER_API_KEY");
        assert!(config.default_model.contains("claude") || !config.default_model.is_empty());
    }

    #[test]
    fn test_hardware_config_serialization() {
        let config = HardwareConfig {
            tier: HardwareTier::Large,
            tier_override: Some(HardwareTier::Medium),
            adaptive_mode: false,
            last_detection: Some("2025-01-01T00:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: HardwareConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.tier, HardwareTier::Large);
        assert_eq!(parsed.tier_override, Some(HardwareTier::Medium));
        assert!(!parsed.adaptive_mode);
        assert_eq!(
            parsed.last_detection,
            Some("2025-01-01T00:00:00Z".to_string())
        );
    }

    #[test]
    fn test_apply_hardware_adaptive_config_large_tier() {
        let mut settings = Settings {
            hardware: Some(HardwareConfig {
                tier: HardwareTier::Large,
                tier_override: None,
                adaptive_mode: true,
                last_detection: None,
            }),
            ..Default::default()
        };

        settings.apply_hardware_adaptive_config();

        // Large tier should update context and tokens
        assert!(settings.context.max_warm_chunks > 0);
        assert!(settings.defaults.max_tokens > 0);
        // Large tier doesn't force local provider selection
        assert_eq!(settings.defaults.provider, "anthropic");
    }

    // ===== Rate Limits Config Tests =====

    #[test]
    fn test_rate_limits_config_default() {
        let config = RateLimitsConfig::default();
        assert!(config.enabled);
        assert!(!config.models.is_empty());
        assert!(config.models.contains_key("claude-sonnet-4"));
        assert!(config.models.contains_key("claude-opus-4"));
        assert!(config.models.contains_key("claude-haiku"));
    }

    #[test]
    fn test_rate_limits_get_for_model_exact() {
        let config = RateLimitsConfig::default();
        let limit = config.get_for_model("claude-sonnet-4");
        assert_eq!(limit.tokens_per_minute, 450_000);
    }

    #[test]
    fn test_rate_limits_get_for_model_prefix() {
        let config = RateLimitsConfig::default();
        // Full model name with version should match prefix
        let limit = config.get_for_model("claude-sonnet-4-20250514");
        assert_eq!(limit.tokens_per_minute, 450_000);
    }

    #[test]
    fn test_rate_limits_get_for_model_opus() {
        let config = RateLimitsConfig::default();
        let limit = config.get_for_model("claude-opus-4-20250514");
        assert_eq!(limit.tokens_per_minute, 80_000);
    }

    #[test]
    fn test_rate_limits_get_for_model_haiku() {
        let config = RateLimitsConfig::default();
        let limit = config.get_for_model("claude-haiku-3-20240307");
        assert_eq!(limit.tokens_per_minute, 1_000_000);
    }

    #[test]
    fn test_rate_limits_get_for_model_unknown() {
        let config = RateLimitsConfig::default();
        // Unknown model should return default
        let limit = config.get_for_model("gpt-4-turbo");
        assert_eq!(limit.tokens_per_minute, 100_000);
    }

    #[test]
    fn test_rate_limits_config_serialization() {
        let config = RateLimitsConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: RateLimitsConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.enabled, config.enabled);
        assert_eq!(parsed.models.len(), config.models.len());
        assert_eq!(
            parsed.default.tokens_per_minute,
            config.default.tokens_per_minute
        );
    }

    #[test]
    fn test_rate_limits_in_settings() {
        let settings = Settings::default();
        assert!(settings.rate_limits.enabled);
        assert!(!settings.rate_limits.models.is_empty());
    }

    #[test]
    fn test_rate_limits_config_disabled() {
        let config = RateLimitsConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(!config.enabled);
    }

    #[test]
    fn test_model_rate_limit_clone() {
        let limit = ModelRateLimit {
            tokens_per_minute: 500_000,
        };
        let cloned = limit.clone();
        assert_eq!(cloned.tokens_per_minute, 500_000);
    }

    #[test]
    fn test_rate_limits_custom_model() {
        let mut config = RateLimitsConfig::default();
        config.models.insert(
            "custom-model".to_string(),
            ModelRateLimit {
                tokens_per_minute: 200_000,
            },
        );

        let limit = config.get_for_model("custom-model");
        assert_eq!(limit.tokens_per_minute, 200_000);
    }

    // ===== Deep merge and merge-save tests =====

    #[test]
    fn test_deep_merge() {
        let base: serde_json::Value = serde_json::json!({
            "a": 1,
            "b": {"c": 2, "d": 3},
            "e": "old"
        });
        let overlay: serde_json::Value = serde_json::json!({
            "b": {"c": 99},
            "e": "new",
            "f": true
        });

        let merged = migration::deep_merge(base, overlay);

        assert_eq!(merged["a"], 1); // from base only
        assert_eq!(merged["b"]["c"], 99); // overlay wins
        assert_eq!(merged["b"]["d"], 3); // from base only (nested)
        assert_eq!(merged["e"], "new"); // overlay wins
        assert_eq!(merged["f"], true); // from overlay only
    }

    #[test]
    fn test_save_preserves_unknown_keys() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("settings.json");

        // Write a file with an extra key the struct doesn't know about
        let initial = r#"{
            "defaults": {"temperature": 0.5},
            "future_feature": {"enabled": true, "threshold": 42}
        }"#;
        std::fs::write(&path, initial).unwrap();

        // Load, modify, save
        let mut settings = Settings::load_from(&path).unwrap();
        settings.defaults.temperature = 0.9;
        settings.save_to(&path).unwrap();

        // Read back as raw Value and verify the unknown key survived
        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();

        let temp = value["defaults"]["temperature"].as_f64().unwrap();
        assert!((temp - 0.9).abs() < 0.001);
        assert_eq!(value["future_feature"]["enabled"], true);
        assert_eq!(value["future_feature"]["threshold"], 42);
    }

    #[test]
    fn test_save_preserves_nested_unknown_keys() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("settings.json");

        // Write a file with nested extra keys inside a known section
        let initial = r#"{
            "providers": {
                "anthropic": {"api_key": "sk-old"},
                "future_provider": {"url": "https://example.com"}
            }
        }"#;
        std::fs::write(&path, initial).unwrap();

        let mut settings = Settings::load_from(&path).unwrap();
        settings.providers.anthropic.api_key = Some("sk-new".to_string());
        settings.save_to(&path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(value["providers"]["anthropic"]["api_key"], "sk-new");
        assert_eq!(
            value["providers"]["future_provider"]["url"],
            "https://example.com"
        );
    }

    #[test]
    fn test_save_to_new_file_works() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("new_settings.json");

        let settings = Settings::default();
        settings.save_to(&path).unwrap();

        assert!(path.exists());
        let loaded = Settings::load_from(&path).unwrap();
        assert_eq!(loaded.defaults.temperature, 0.7);
    }

    #[test]
    fn test_save_overwrites_corrupt_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("settings.json");

        std::fs::write(&path, "this is not json{{{").unwrap();

        let settings = Settings::default();
        settings.save_to(&path).unwrap();

        let loaded = Settings::load_from(&path).unwrap();
        assert_eq!(loaded.defaults.temperature, 0.7);
    }

    #[test]
    fn test_save_clean_does_full_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("settings.json");

        let initial = r#"{"future_feature": {"enabled": true}}"#;
        std::fs::write(&path, initial).unwrap();

        let settings = Settings::default();
        settings.save_to_clean(&path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&content).unwrap();

        // The unknown key should NOT survive a clean save
        assert!(value.get("future_feature").is_none());
    }

    // ===== is_provider_configured tests =====

    #[test]
    fn test_is_provider_configured_local_with_model() {
        let mut settings = Settings::default();
        // Point to a file that exists — use Cargo.toml as a stand-in
        settings.providers.local.model_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(settings.is_provider_configured("local"));
    }

    #[test]
    fn test_is_provider_configured_local_no_model() {
        let mut settings = Settings::default();
        // Point to a file that doesn't exist and no system models
        settings.providers.local.model_path =
            std::path::PathBuf::from("/nonexistent/path/model.gguf");
        // Result depends on whether the test machine has GGUF files installed;
        // we just verify it doesn't panic
        let _ = settings.is_provider_configured("local");
    }

    #[test]
    fn test_is_provider_configured_anthropic_no_key() {
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = None;
        settings.providers.anthropic.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();
        assert!(!settings.is_provider_configured("anthropic"));
    }

    #[test]
    fn test_is_provider_configured_anthropic_with_key() {
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = Some("sk-test".to_string());
        settings.providers.anthropic.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();
        assert!(settings.is_provider_configured("anthropic"));
    }

    #[test]
    fn test_is_provider_configured_openrouter_with_key() {
        let mut settings = Settings::default();
        settings.providers.openrouter.api_key = Some("or-test".to_string());
        settings.providers.openrouter.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();
        assert!(settings.is_provider_configured("openrouter"));
    }

    #[test]
    fn test_is_provider_configured_blackman_with_key() {
        let mut settings = Settings::default();
        settings.providers.blackman.api_key = Some("bm-test".to_string());
        settings.providers.blackman.api_key_env = "NONEXISTENT_ENV_VAR_12345".to_string();
        assert!(settings.is_provider_configured("blackman"));
    }

    #[test]
    fn test_is_provider_configured_unknown() {
        let settings = Settings::default();
        assert!(!settings.is_provider_configured("nonexistent"));
    }
}
