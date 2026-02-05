// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Model registry system
//!
//! Provides a centralized registry of available LLM models with metadata
//! including tier classification, hardware requirements, and capabilities.
//!
//! ## Model Tiers
//!
//! Models are classified into three tiers:
//! - **High**: Best quality, highest cost/resource requirements
//! - **Medium**: Good balance of quality and efficiency
//! - **Low**: Fast and lightweight, lower quality
//!
//! ## Configuration
//!
//! Models are loaded from:
//! 1. Built-in defaults (always available)
//! 2. `~/.ted/models.toml` for user customization
//!
//! ## Example Configuration
//!
//! ```toml
//! [anthropic]
//! models = [
//!     { id = "claude-sonnet-4-20250514", tier = "high", context_size = 200000 }
//! ]
//!
//! [ollama]
//! models = [
//!     { id = "qwen2.5-coder:14b", tier = "medium", vram_gb = 12.0 }
//! ]
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use ted::models::{ModelRegistry, Provider, ModelTier};
//!
//! // Create registry (loads defaults + user config)
//! let registry = ModelRegistry::new();
//!
//! // Get all models for a provider
//! let models = registry.models_for_provider(&Provider::Anthropic);
//!
//! // Filter by tier
//! let fast_models = registry.models_by_tier(&Provider::Ollama, ModelTier::Low);
//!
//! // Filter by VRAM budget
//! let laptop_models = registry.models_for_vram(&Provider::Ollama, 8.0);
//!
//! // Find a specific model
//! if let Some((provider, model)) = registry.find_model("claude-sonnet-4-20250514") {
//!     println!("{}: {}", provider.display_name(), model.display_name());
//! }
//! ```

pub mod download;
pub mod loader;
pub mod schema;

// Re-export commonly used types
pub use download::{
    DownloadRegistry, DownloadableModel, ModelCategory, ModelDownloader, ModelVariant, Quantization,
};
pub use loader::ModelRegistry;
pub use schema::{ModelInfo, ModelTier, ModelsConfig, Provider, ProviderModels};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify types are exported correctly
        let registry = ModelRegistry::new();
        let _ = registry.models_for_provider(&Provider::Anthropic);
        let _ = ModelTier::High;
    }

    #[test]
    fn test_full_workflow() {
        let registry = ModelRegistry::new();

        // Should have Anthropic models
        let anthropic = registry.models_for_provider(&Provider::Anthropic);
        assert!(!anthropic.is_empty());

        // Should have recommended models
        let recommended = registry.recommended_models(&Provider::Anthropic);
        assert!(!recommended.is_empty());

        // Should find sonnet 4 (could be from Anthropic or Blackman since both have it)
        let (provider, model) = registry.find_model("claude-sonnet-4-20250514").unwrap();
        assert!(
            *provider == Provider::Anthropic || *provider == Provider::Blackman,
            "Expected Anthropic or Blackman"
        );
        assert_eq!(model.tier, ModelTier::High);
    }
}
