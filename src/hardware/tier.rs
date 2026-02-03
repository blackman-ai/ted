// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Hardware tier classification and adaptive configuration

use serde::{Deserialize, Serialize};

/// Hardware tier classification based on system capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HardwareTier {
    /// 2020+: Raspberry Pi 5, ARM SBCs - education & embedded
    UltraTiny,
    /// 2010-2015: The "IE 11 Benchmark" - refurbished PCs
    Ancient,
    /// 2015-2020: Chromebooks, old laptops
    Tiny,
    /// 2018+: Entry MacBook Air, basic laptops
    Small,
    /// 2020+: MacBook Pro M1/M2, gaming laptops
    Medium,
    /// 2021+: Pro workstations, Mac Studio
    Large,
    /// Using API providers (cloud-based)
    Cloud,
}

impl HardwareTier {
    /// Get the maximum context tokens for this tier
    pub fn max_context_tokens(&self) -> usize {
        match self {
            HardwareTier::UltraTiny => 512,
            HardwareTier::Ancient => 1024,
            HardwareTier::Tiny => 2048,
            HardwareTier::Small => 4096,
            HardwareTier::Medium => 8192,
            HardwareTier::Large => 16384,
            HardwareTier::Cloud => 100000,
        }
    }

    /// Get the maximum warm chunks for this tier
    pub fn max_warm_chunks(&self) -> usize {
        match self {
            HardwareTier::UltraTiny => 5,
            HardwareTier::Ancient => 10,
            HardwareTier::Tiny => 20,
            HardwareTier::Small => 50,
            HardwareTier::Medium => 100,
            HardwareTier::Large => 200,
            HardwareTier::Cloud => 500,
        }
    }

    /// Whether background tasks should be disabled for this tier
    pub fn disable_background_tasks(&self) -> bool {
        matches!(self, HardwareTier::UltraTiny | HardwareTier::Ancient)
    }

    /// Whether to use streaming only for this tier
    pub fn streaming_only(&self) -> bool {
        matches!(
            self,
            HardwareTier::UltraTiny | HardwareTier::Ancient | HardwareTier::Tiny
        )
    }

    /// Whether to use single-file mode for this tier
    pub fn single_file_mode(&self) -> bool {
        matches!(self, HardwareTier::UltraTiny | HardwareTier::Ancient)
    }

    /// Get recommended quantization level for this tier
    pub fn recommended_quantization(&self) -> &'static str {
        match self {
            HardwareTier::UltraTiny => "Q3_K_M", // Ultra-heavy quantization for ARM
            HardwareTier::Ancient => "Q4_K_M",   // Heavy quantization
            HardwareTier::Tiny => "Q4_K_M",
            HardwareTier::Small => "Q5_K_M",
            HardwareTier::Medium => "Q5_K_M",
            HardwareTier::Large => "Q6_K",
            HardwareTier::Cloud => "none", // No local quantization needed
        }
    }

    /// Whether to disable the indexer for this tier
    pub fn disable_indexer(&self) -> bool {
        matches!(
            self,
            HardwareTier::UltraTiny | HardwareTier::Ancient | HardwareTier::Tiny
        )
    }

    /// Whether to monitor thermal throttling for this tier
    pub fn monitor_thermal(&self) -> bool {
        matches!(self, HardwareTier::UltraTiny)
    }

    /// Get a human-readable description of this tier
    pub fn description(&self) -> &'static str {
        match self {
            HardwareTier::UltraTiny => "Raspberry Pi / ARM SBC (Education Mode)",
            HardwareTier::Ancient => "2010-2015 PC (Refurbished)",
            HardwareTier::Tiny => "2015-2020 Laptop (Budget)",
            HardwareTier::Small => "Entry-level Modern Laptop",
            HardwareTier::Medium => "Modern Laptop / Desktop",
            HardwareTier::Large => "High-end Workstation",
            HardwareTier::Cloud => "Cloud-based LLM Provider",
        }
    }

    /// Get recommended models for this tier
    pub fn recommended_models(&self) -> Vec<&'static str> {
        match self {
            HardwareTier::UltraTiny => vec!["qwen2.5-coder:1.5b"],
            HardwareTier::Ancient => vec!["qwen2.5-coder:1.5b", "phi-3-mini"],
            HardwareTier::Tiny => vec!["qwen2.5-coder:1.5b", "phi-3-mini"],
            HardwareTier::Small => vec!["qwen2.5-coder:3b", "codellama:7b"],
            HardwareTier::Medium => vec!["qwen2.5-coder:7b", "deepseek-coder:6.7b"],
            HardwareTier::Large => vec!["qwen2.5-coder:14b", "codellama:34b"],
            HardwareTier::Cloud => vec![
                "claude-sonnet-4",
                "gpt-4o",
                "deepseek-coder-v2",
                "qwen2.5-coder:72b",
            ],
        }
    }

    /// Get expected response time range in seconds
    pub fn expected_response_time(&self) -> (u32, u32) {
        match self {
            HardwareTier::UltraTiny => (15, 40),
            HardwareTier::Ancient => (30, 60),
            HardwareTier::Tiny => (15, 30),
            HardwareTier::Small => (10, 20),
            HardwareTier::Medium => (5, 10),
            HardwareTier::Large => (2, 5),
            HardwareTier::Cloud => (1, 3),
        }
    }

    /// Get what this tier can build
    pub fn capabilities(&self) -> Vec<&'static str> {
        match self {
            HardwareTier::UltraTiny => vec![
                "Simple apps",
                "Learning projects",
                "Maker tools",
                "Single-page sites",
            ],
            HardwareTier::Ancient => vec![
                "Blogs",
                "Portfolios",
                "Simple tools",
                "To-do lists",
                "Recipe sites",
            ],
            HardwareTier::Tiny => vec![
                "Small business sites",
                "Multi-page apps",
                "Simple e-commerce",
                "Dashboards",
            ],
            HardwareTier::Small => vec![
                "Full-stack apps",
                "REST APIs",
                "Database-backed apps",
                "Real-time features",
            ],
            HardwareTier::Medium => vec![
                "Complex applications",
                "Microservices",
                "Large refactorings",
                "Multi-service apps",
            ],
            HardwareTier::Large => vec![
                "Enterprise software",
                "Large-scale systems",
                "Complex architectures",
                "Any project type",
            ],
            HardwareTier::Cloud => vec![
                "Unlimited complexity",
                "Large codebases",
                "Multi-repo projects",
                "Any project type",
            ],
        }
    }

    /// Get what this tier cannot build (limitations)
    pub fn limitations(&self) -> Vec<&'static str> {
        match self {
            HardwareTier::UltraTiny => vec![
                "Professional development",
                "Complex multi-page apps",
                "Large codebases",
                "7b+ models",
            ],
            HardwareTier::Ancient => vec![
                "Complex multi-page apps",
                "Large refactorings",
                "7b+ models",
                "Real-time multi-user",
            ],
            HardwareTier::Tiny => vec![
                "Enterprise software",
                "Large-scale refactoring",
                "14b+ models",
            ],
            HardwareTier::Small => vec!["Massive codebases", "34b+ models"],
            HardwareTier::Medium => vec!["Extremely large models (70b+)"],
            HardwareTier::Large => vec!["None (hardware-wise)"],
            HardwareTier::Cloud => vec!["None"],
        }
    }
}

impl std::fmt::Display for HardwareTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HardwareTier::UltraTiny => write!(f, "UltraTiny"),
            HardwareTier::Ancient => write!(f, "Ancient"),
            HardwareTier::Tiny => write!(f, "Tiny"),
            HardwareTier::Small => write!(f, "Small"),
            HardwareTier::Medium => write!(f, "Medium"),
            HardwareTier::Large => write!(f, "Large"),
            HardwareTier::Cloud => write!(f, "Cloud"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== max_context_tokens tests =====

    #[test]
    fn test_max_context_tokens() {
        assert_eq!(HardwareTier::UltraTiny.max_context_tokens(), 512);
        assert_eq!(HardwareTier::Ancient.max_context_tokens(), 1024);
        assert_eq!(HardwareTier::Tiny.max_context_tokens(), 2048);
        assert_eq!(HardwareTier::Small.max_context_tokens(), 4096);
        assert_eq!(HardwareTier::Medium.max_context_tokens(), 8192);
        assert_eq!(HardwareTier::Large.max_context_tokens(), 16384);
        assert_eq!(HardwareTier::Cloud.max_context_tokens(), 100000);
    }

    // ===== max_warm_chunks tests =====

    #[test]
    fn test_max_warm_chunks() {
        assert_eq!(HardwareTier::UltraTiny.max_warm_chunks(), 5);
        assert_eq!(HardwareTier::Ancient.max_warm_chunks(), 10);
        assert_eq!(HardwareTier::Tiny.max_warm_chunks(), 20);
        assert_eq!(HardwareTier::Small.max_warm_chunks(), 50);
        assert_eq!(HardwareTier::Medium.max_warm_chunks(), 100);
        assert_eq!(HardwareTier::Large.max_warm_chunks(), 200);
        assert_eq!(HardwareTier::Cloud.max_warm_chunks(), 500);
    }

    // ===== disable_background_tasks tests =====

    #[test]
    fn test_disable_background_tasks() {
        assert!(HardwareTier::UltraTiny.disable_background_tasks());
        assert!(HardwareTier::Ancient.disable_background_tasks());
        assert!(!HardwareTier::Tiny.disable_background_tasks());
        assert!(!HardwareTier::Small.disable_background_tasks());
        assert!(!HardwareTier::Medium.disable_background_tasks());
        assert!(!HardwareTier::Large.disable_background_tasks());
        assert!(!HardwareTier::Cloud.disable_background_tasks());
    }

    // ===== streaming_only tests =====

    #[test]
    fn test_streaming_only() {
        assert!(HardwareTier::UltraTiny.streaming_only());
        assert!(HardwareTier::Ancient.streaming_only());
        assert!(HardwareTier::Tiny.streaming_only());
        assert!(!HardwareTier::Small.streaming_only());
        assert!(!HardwareTier::Medium.streaming_only());
        assert!(!HardwareTier::Large.streaming_only());
        assert!(!HardwareTier::Cloud.streaming_only());
    }

    // ===== single_file_mode tests =====

    #[test]
    fn test_single_file_mode() {
        assert!(HardwareTier::UltraTiny.single_file_mode());
        assert!(HardwareTier::Ancient.single_file_mode());
        assert!(!HardwareTier::Tiny.single_file_mode());
        assert!(!HardwareTier::Small.single_file_mode());
        assert!(!HardwareTier::Cloud.single_file_mode());
    }

    // ===== recommended_quantization tests =====

    #[test]
    fn test_recommended_quantization() {
        assert_eq!(HardwareTier::UltraTiny.recommended_quantization(), "Q3_K_M");
        assert_eq!(HardwareTier::Ancient.recommended_quantization(), "Q4_K_M");
        assert_eq!(HardwareTier::Tiny.recommended_quantization(), "Q4_K_M");
        assert_eq!(HardwareTier::Small.recommended_quantization(), "Q5_K_M");
        assert_eq!(HardwareTier::Medium.recommended_quantization(), "Q5_K_M");
        assert_eq!(HardwareTier::Large.recommended_quantization(), "Q6_K");
        assert_eq!(HardwareTier::Cloud.recommended_quantization(), "none");
    }

    // ===== disable_indexer tests =====

    #[test]
    fn test_disable_indexer() {
        assert!(HardwareTier::UltraTiny.disable_indexer());
        assert!(HardwareTier::Ancient.disable_indexer());
        assert!(HardwareTier::Tiny.disable_indexer());
        assert!(!HardwareTier::Small.disable_indexer());
        assert!(!HardwareTier::Medium.disable_indexer());
        assert!(!HardwareTier::Large.disable_indexer());
        assert!(!HardwareTier::Cloud.disable_indexer());
    }

    // ===== monitor_thermal tests =====

    #[test]
    fn test_monitor_thermal() {
        assert!(HardwareTier::UltraTiny.monitor_thermal());
        assert!(!HardwareTier::Ancient.monitor_thermal());
        assert!(!HardwareTier::Tiny.monitor_thermal());
        assert!(!HardwareTier::Cloud.monitor_thermal());
    }

    // ===== description tests =====

    #[test]
    fn test_description() {
        assert!(HardwareTier::UltraTiny
            .description()
            .contains("Raspberry Pi"));
        assert!(HardwareTier::Ancient.description().contains("2010-2015"));
        assert!(HardwareTier::Tiny.description().contains("2015-2020"));
        assert!(HardwareTier::Small.description().contains("Entry-level"));
        assert!(HardwareTier::Medium.description().contains("Modern"));
        assert!(HardwareTier::Large.description().contains("Workstation"));
        assert!(HardwareTier::Cloud.description().contains("Cloud"));
    }

    // ===== recommended_models tests =====

    #[test]
    fn test_recommended_models() {
        let models = HardwareTier::UltraTiny.recommended_models();
        assert!(models.contains(&"qwen2.5-coder:1.5b"));

        let models = HardwareTier::Small.recommended_models();
        assert!(models.contains(&"qwen2.5-coder:3b"));

        let models = HardwareTier::Cloud.recommended_models();
        assert!(models.contains(&"claude-sonnet-4"));
    }

    // ===== expected_response_time tests =====

    #[test]
    fn test_expected_response_time() {
        let (min, max) = HardwareTier::UltraTiny.expected_response_time();
        assert_eq!(min, 15);
        assert_eq!(max, 40);

        let (min, max) = HardwareTier::Cloud.expected_response_time();
        assert_eq!(min, 1);
        assert_eq!(max, 3);

        // Higher tiers should have faster response times
        let (_, ultra_max) = HardwareTier::UltraTiny.expected_response_time();
        let (_, cloud_max) = HardwareTier::Cloud.expected_response_time();
        assert!(cloud_max < ultra_max);
    }

    // ===== capabilities tests =====

    #[test]
    fn test_capabilities() {
        let caps = HardwareTier::UltraTiny.capabilities();
        assert!(caps.contains(&"Simple apps"));

        let caps = HardwareTier::Cloud.capabilities();
        assert!(caps.contains(&"Unlimited complexity"));
    }

    // ===== limitations tests =====

    #[test]
    fn test_limitations() {
        let limits = HardwareTier::UltraTiny.limitations();
        assert!(limits.contains(&"Professional development"));

        let limits = HardwareTier::Cloud.limitations();
        assert!(limits.contains(&"None"));
    }

    // ===== Display tests =====

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", HardwareTier::UltraTiny), "UltraTiny");
        assert_eq!(format!("{}", HardwareTier::Ancient), "Ancient");
        assert_eq!(format!("{}", HardwareTier::Tiny), "Tiny");
        assert_eq!(format!("{}", HardwareTier::Small), "Small");
        assert_eq!(format!("{}", HardwareTier::Medium), "Medium");
        assert_eq!(format!("{}", HardwareTier::Large), "Large");
        assert_eq!(format!("{}", HardwareTier::Cloud), "Cloud");
    }

    // ===== Serialization tests =====

    #[test]
    fn test_serialization() {
        let tier = HardwareTier::Medium;
        let json = serde_json::to_string(&tier).unwrap();
        let parsed: HardwareTier = serde_json::from_str(&json).unwrap();
        assert_eq!(tier, parsed);
    }

    #[test]
    fn test_all_tiers_serialize_roundtrip() {
        let tiers = [
            HardwareTier::UltraTiny,
            HardwareTier::Ancient,
            HardwareTier::Tiny,
            HardwareTier::Small,
            HardwareTier::Medium,
            HardwareTier::Large,
            HardwareTier::Cloud,
        ];

        for tier in tiers {
            let json = serde_json::to_string(&tier).unwrap();
            let parsed: HardwareTier = serde_json::from_str(&json).unwrap();
            assert_eq!(tier, parsed);
        }
    }

    // ===== Tier ordering tests =====

    #[test]
    fn test_context_tokens_increase_with_tier() {
        let tiers = [
            HardwareTier::UltraTiny,
            HardwareTier::Ancient,
            HardwareTier::Tiny,
            HardwareTier::Small,
            HardwareTier::Medium,
            HardwareTier::Large,
            HardwareTier::Cloud,
        ];

        for i in 1..tiers.len() {
            assert!(
                tiers[i].max_context_tokens() >= tiers[i - 1].max_context_tokens(),
                "Context tokens should increase with tier"
            );
        }
    }

    #[test]
    fn test_warm_chunks_increase_with_tier() {
        let tiers = [
            HardwareTier::UltraTiny,
            HardwareTier::Ancient,
            HardwareTier::Tiny,
            HardwareTier::Small,
            HardwareTier::Medium,
            HardwareTier::Large,
            HardwareTier::Cloud,
        ];

        for i in 1..tiers.len() {
            assert!(
                tiers[i].max_warm_chunks() >= tiers[i - 1].max_warm_chunks(),
                "Warm chunks should increase with tier"
            );
        }
    }

    // ===== Additional recommended_models Tests =====

    #[test]
    fn test_recommended_models_ancient() {
        let models = HardwareTier::Ancient.recommended_models();
        assert!(!models.is_empty());
        assert!(models.contains(&"qwen2.5-coder:1.5b"));
        assert!(models.contains(&"phi-3-mini"));
    }

    #[test]
    fn test_recommended_models_tiny() {
        let models = HardwareTier::Tiny.recommended_models();
        assert!(!models.is_empty());
        assert!(models.contains(&"qwen2.5-coder:1.5b"));
    }

    #[test]
    fn test_recommended_models_medium() {
        let models = HardwareTier::Medium.recommended_models();
        assert!(!models.is_empty());
        assert!(models.contains(&"qwen2.5-coder:7b"));
        assert!(models.contains(&"deepseek-coder:6.7b"));
    }

    #[test]
    fn test_recommended_models_large() {
        let models = HardwareTier::Large.recommended_models();
        assert!(!models.is_empty());
        assert!(models.contains(&"qwen2.5-coder:14b"));
        assert!(models.contains(&"codellama:34b"));
    }

    #[test]
    fn test_recommended_models_cloud_has_many() {
        let models = HardwareTier::Cloud.recommended_models();
        assert!(models.len() >= 4);
        assert!(models.contains(&"gpt-4o"));
        assert!(models.contains(&"deepseek-coder-v2"));
        assert!(models.contains(&"qwen2.5-coder:72b"));
    }

    // ===== Additional expected_response_time Tests =====

    #[test]
    fn test_expected_response_time_ancient() {
        let (min, max) = HardwareTier::Ancient.expected_response_time();
        assert_eq!(min, 30);
        assert_eq!(max, 60);
    }

    #[test]
    fn test_expected_response_time_tiny() {
        let (min, max) = HardwareTier::Tiny.expected_response_time();
        assert_eq!(min, 15);
        assert_eq!(max, 30);
    }

    #[test]
    fn test_expected_response_time_small() {
        let (min, max) = HardwareTier::Small.expected_response_time();
        assert_eq!(min, 10);
        assert_eq!(max, 20);
    }

    #[test]
    fn test_expected_response_time_medium() {
        let (min, max) = HardwareTier::Medium.expected_response_time();
        assert_eq!(min, 5);
        assert_eq!(max, 10);
    }

    #[test]
    fn test_expected_response_time_large() {
        let (min, max) = HardwareTier::Large.expected_response_time();
        assert_eq!(min, 2);
        assert_eq!(max, 5);
    }

    // ===== Additional capabilities Tests =====

    #[test]
    fn test_capabilities_ancient() {
        let caps = HardwareTier::Ancient.capabilities();
        assert!(!caps.is_empty());
        assert!(caps.contains(&"Blogs"));
        assert!(caps.contains(&"Portfolios"));
        assert!(caps.contains(&"Simple tools"));
    }

    #[test]
    fn test_capabilities_tiny() {
        let caps = HardwareTier::Tiny.capabilities();
        assert!(!caps.is_empty());
        assert!(caps.contains(&"Small business sites"));
        assert!(caps.contains(&"Multi-page apps"));
    }

    #[test]
    fn test_capabilities_small() {
        let caps = HardwareTier::Small.capabilities();
        assert!(!caps.is_empty());
        assert!(caps.contains(&"Full-stack apps"));
        assert!(caps.contains(&"REST APIs"));
    }

    #[test]
    fn test_capabilities_medium() {
        let caps = HardwareTier::Medium.capabilities();
        assert!(!caps.is_empty());
        assert!(caps.contains(&"Complex applications"));
        assert!(caps.contains(&"Microservices"));
    }

    #[test]
    fn test_capabilities_large() {
        let caps = HardwareTier::Large.capabilities();
        assert!(!caps.is_empty());
        assert!(caps.contains(&"Enterprise software"));
        assert!(caps.contains(&"Any project type"));
    }

    // ===== Additional limitations Tests =====

    #[test]
    fn test_limitations_ancient() {
        let limits = HardwareTier::Ancient.limitations();
        assert!(!limits.is_empty());
        assert!(limits.contains(&"Complex multi-page apps"));
        assert!(limits.contains(&"7b+ models"));
    }

    #[test]
    fn test_limitations_tiny() {
        let limits = HardwareTier::Tiny.limitations();
        assert!(!limits.is_empty());
        assert!(limits.contains(&"Enterprise software"));
        assert!(limits.contains(&"14b+ models"));
    }

    #[test]
    fn test_limitations_small() {
        let limits = HardwareTier::Small.limitations();
        assert!(!limits.is_empty());
        assert!(limits.contains(&"Massive codebases"));
        assert!(limits.contains(&"34b+ models"));
    }

    #[test]
    fn test_limitations_medium() {
        let limits = HardwareTier::Medium.limitations();
        assert!(!limits.is_empty());
        assert!(limits.contains(&"Extremely large models (70b+)"));
    }

    #[test]
    fn test_limitations_large() {
        let limits = HardwareTier::Large.limitations();
        assert!(!limits.is_empty());
        assert!(limits.contains(&"None (hardware-wise)"));
    }

    // ===== Clone and Copy Tests =====

    #[test]
    fn test_hardware_tier_clone() {
        let tier = HardwareTier::Medium;
        let cloned = tier;
        assert_eq!(tier, cloned);
    }

    #[test]
    fn test_hardware_tier_copy() {
        let tier = HardwareTier::Large;
        let copied = tier;
        assert_eq!(tier, copied);
    }

    // ===== Equality Tests =====

    #[test]
    fn test_hardware_tier_eq() {
        assert_eq!(HardwareTier::Cloud, HardwareTier::Cloud);
        assert_ne!(HardwareTier::Cloud, HardwareTier::UltraTiny);
    }

    // ===== Debug Tests =====

    #[test]
    fn test_hardware_tier_debug() {
        let debug_str = format!("{:?}", HardwareTier::Medium);
        assert!(debug_str.contains("Medium"));
    }

    // ===== All Tiers Tests =====

    #[test]
    fn test_all_tiers_have_capabilities() {
        let tiers = [
            HardwareTier::UltraTiny,
            HardwareTier::Ancient,
            HardwareTier::Tiny,
            HardwareTier::Small,
            HardwareTier::Medium,
            HardwareTier::Large,
            HardwareTier::Cloud,
        ];

        for tier in tiers {
            assert!(
                !tier.capabilities().is_empty(),
                "Tier {:?} should have capabilities",
                tier
            );
        }
    }

    #[test]
    fn test_all_tiers_have_limitations() {
        let tiers = [
            HardwareTier::UltraTiny,
            HardwareTier::Ancient,
            HardwareTier::Tiny,
            HardwareTier::Small,
            HardwareTier::Medium,
            HardwareTier::Large,
            HardwareTier::Cloud,
        ];

        for tier in tiers {
            assert!(
                !tier.limitations().is_empty(),
                "Tier {:?} should have limitations",
                tier
            );
        }
    }

    #[test]
    fn test_all_tiers_have_description() {
        let tiers = [
            HardwareTier::UltraTiny,
            HardwareTier::Ancient,
            HardwareTier::Tiny,
            HardwareTier::Small,
            HardwareTier::Medium,
            HardwareTier::Large,
            HardwareTier::Cloud,
        ];

        for tier in tiers {
            assert!(
                !tier.description().is_empty(),
                "Tier {:?} should have description",
                tier
            );
        }
    }

    #[test]
    fn test_all_tiers_have_recommended_models() {
        let tiers = [
            HardwareTier::UltraTiny,
            HardwareTier::Ancient,
            HardwareTier::Tiny,
            HardwareTier::Small,
            HardwareTier::Medium,
            HardwareTier::Large,
            HardwareTier::Cloud,
        ];

        for tier in tiers {
            assert!(
                !tier.recommended_models().is_empty(),
                "Tier {:?} should have models",
                tier
            );
        }
    }

    #[test]
    fn test_all_tiers_have_valid_response_times() {
        let tiers = [
            HardwareTier::UltraTiny,
            HardwareTier::Ancient,
            HardwareTier::Tiny,
            HardwareTier::Small,
            HardwareTier::Medium,
            HardwareTier::Large,
            HardwareTier::Cloud,
        ];

        for tier in tiers {
            let (min, max) = tier.expected_response_time();
            assert!(
                min > 0,
                "Min response time should be positive for {:?}",
                tier
            );
            assert!(max >= min, "Max should be >= min for {:?}", tier);
        }
    }
}
