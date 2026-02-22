// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use crate::error::Result;
use crate::hardware::{HardwareTier, SystemProfile};

use super::{HardwareConfig, Settings};

impl Settings {
    /// Get the API key for Anthropic, checking env var first.
    pub fn get_anthropic_api_key(&self) -> Option<String> {
        // Priority: env var > config file.
        std::env::var(&self.providers.anthropic.api_key_env)
            .ok()
            .or_else(|| self.providers.anthropic.api_key.clone())
    }

    /// Get the API key for OpenRouter, checking env var first.
    pub fn get_openrouter_api_key(&self) -> Option<String> {
        // Priority: env var > config file.
        std::env::var(&self.providers.openrouter.api_key_env)
            .ok()
            .or_else(|| self.providers.openrouter.api_key.clone())
    }

    /// Get the API key for Blackman AI, checking env var first.
    pub fn get_blackman_api_key(&self) -> Option<String> {
        // Priority: env var > config file.
        std::env::var(&self.providers.blackman.api_key_env)
            .ok()
            .or_else(|| self.providers.blackman.api_key.clone())
    }

    /// Check if the given provider has a usable configuration.
    /// Local provider needs a model file (configured path or system scan).
    /// API-based providers need an API key.
    pub fn is_provider_configured(&self, provider: &str) -> bool {
        match provider {
            "local" => {
                // Local provider is always "configured" — the user chose it.
                // Model availability is checked at runtime by the provider factory,
                // which gives a clear error with download instructions if no models exist.
                true
            }
            "anthropic" => self.get_anthropic_api_key().is_some(),
            "openrouter" => self.get_openrouter_api_key().is_some(),
            "blackman" => self.get_blackman_api_key().is_some(),
            _ => false,
        }
    }

    /// Get the Blackman base URL, checking env var first.
    pub fn get_blackman_base_url(&self) -> String {
        // Priority: env var > config file.
        std::env::var("BLACKMAN_BASE_URL")
            .ok()
            .unwrap_or_else(|| self.providers.blackman.base_url.clone())
    }

    /// Detect hardware and update configuration.
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

    /// Get the effective hardware tier (considering overrides).
    pub fn effective_tier(&self) -> HardwareTier {
        if let Some(ref hw) = self.hardware {
            hw.tier_override.unwrap_or(hw.tier)
        } else {
            // Default to Medium if not detected.
            HardwareTier::Medium
        }
    }

    /// Apply hardware-adaptive configuration to context settings.
    pub fn apply_hardware_adaptive_config(&mut self) {
        let tier = self.effective_tier();

        // Only apply if adaptive mode is enabled.
        if let Some(ref hw) = self.hardware {
            if !hw.adaptive_mode {
                return;
            }
        }

        // Update context config based on tier.
        self.context.max_warm_chunks = tier.max_warm_chunks();

        // Update defaults based on tier.
        self.defaults.max_tokens = tier.max_context_tokens() as u32;

        // For ancient/tiny tiers, prefer local models.
        if matches!(
            tier,
            HardwareTier::UltraTiny | HardwareTier::Ancient | HardwareTier::Tiny
        ) {
            self.defaults.provider = "local".to_string();
            let models = tier.recommended_models();
            if !models.is_empty() {
                self.providers.local.default_model = models[0].to_string();
            }
        }
    }

    /// Get hardware-specific warnings or recommendations.
    pub fn get_hardware_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        if let Some(ref hw) = self.hardware {
            let tier = hw.tier_override.unwrap_or(hw.tier);

            match tier {
                HardwareTier::UltraTiny => {
                    warnings.push(
                        "🎓 Raspberry Pi detected (Education Mode). Expected AI response: 20-40 seconds."
                            .to_string(),
                    );
                }
                HardwareTier::Ancient => {
                    warnings.push(
                        "🐢 Ancient hardware detected. Expected response time: 30-60 seconds. Consider upgrading RAM or SSD for better performance."
                            .to_string(),
                    );
                }
                _ => {}
            }
        }

        warnings
    }
}
