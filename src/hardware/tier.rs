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

    #[test]
    fn test_max_context_tokens() {
        assert_eq!(HardwareTier::UltraTiny.max_context_tokens(), 512);
        assert_eq!(HardwareTier::Ancient.max_context_tokens(), 1024);
        assert_eq!(HardwareTier::Cloud.max_context_tokens(), 100000);
    }

    #[test]
    fn test_max_warm_chunks() {
        assert_eq!(HardwareTier::UltraTiny.max_warm_chunks(), 5);
        assert_eq!(HardwareTier::Medium.max_warm_chunks(), 100);
    }

    #[test]
    fn test_disable_background_tasks() {
        assert!(HardwareTier::UltraTiny.disable_background_tasks());
        assert!(HardwareTier::Ancient.disable_background_tasks());
        assert!(!HardwareTier::Small.disable_background_tasks());
    }

    #[test]
    fn test_recommended_models() {
        let models = HardwareTier::Small.recommended_models();
        assert!(models.contains(&"qwen2.5-coder:3b"));
    }

    #[test]
    fn test_description() {
        assert!(HardwareTier::Ancient
            .description()
            .contains("2010-2015"));
    }

    #[test]
    fn test_serialization() {
        let tier = HardwareTier::Medium;
        let json = serde_json::to_string(&tier).unwrap();
        let parsed: HardwareTier = serde_json::from_str(&json).unwrap();
        assert_eq!(tier, parsed);
    }
}
