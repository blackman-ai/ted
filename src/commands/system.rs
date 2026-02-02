// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! System hardware information command

use crate::cli::args::{OutputFormat, SystemArgs};
use crate::error::Result;
use crate::hardware::SystemProfile;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HardwareInfo {
    tier: String,
    tier_description: String,
    cpu_brand: String,
    cpu_cores: usize,
    ram_gb: usize,
    has_ssd: bool,
    architecture: String,
    is_sbc: bool,
    cpu_year: Option<u32>,
    recommended_models: Vec<String>,
    expected_response_time: (u32, u32),
    capabilities: Vec<String>,
    limitations: Vec<String>,
}

/// Execute the system command
pub fn execute(args: &SystemArgs, format: &OutputFormat) -> Result<()> {
    let profile = SystemProfile::detect()?;

    // JSON output
    if matches!(format, OutputFormat::Json) {
        let info = HardwareInfo {
            tier: format!("{:?}", profile.tier),
            tier_description: profile.tier.description().to_string(),
            cpu_brand: profile.cpu_brand.clone(),
            cpu_cores: profile.cpu_cores,
            ram_gb: profile.ram_gb,
            has_ssd: profile.has_ssd,
            architecture: format!("{:?}", profile.architecture),
            is_sbc: profile.is_sbc,
            cpu_year: profile.cpu_year,
            recommended_models: profile
                .tier
                .recommended_models()
                .iter()
                .map(|s| s.to_string())
                .collect(),
            expected_response_time: profile.tier.expected_response_time(),
            capabilities: profile
                .tier
                .capabilities()
                .iter()
                .map(|s| s.to_string())
                .collect(),
            limitations: profile
                .tier
                .limitations()
                .iter()
                .map(|s| s.to_string())
                .collect(),
        };
        println!("{}", serde_json::to_string_pretty(&info)?);
        return Ok(());
    }

    // Always show basic info
    println!("\n=== Ted System Hardware Profile ===\n");
    println!("Tier: {} ({})", profile.tier, profile.tier.description());
    println!("CPU: {} ({} cores)", profile.cpu_brand, profile.cpu_cores);
    println!(
        "RAM: {}GB{}",
        profile.ram_gb,
        if profile.ram_gb < 16 { " ⚠️ " } else { "" }
    );
    println!(
        "Storage: {}",
        if profile.has_ssd {
            "SSD ✓"
        } else {
            "HDD (consider upgrading) ⚠️"
        }
    );
    println!("Architecture: {:?}", profile.architecture);

    if profile.is_sbc {
        println!("System Type: Single-Board Computer (Raspberry Pi/similar)");
        if profile.thermal_throttle_risk() {
            println!("⚠️  Thermal Risk: Active cooling recommended");
        }
    }

    if let Some(year) = profile.cpu_year {
        println!("CPU Generation: ~{}", year);
    }

    // Show capabilities
    println!("\n=== What You Can Build ===");
    for capability in profile.tier.capabilities() {
        println!("  ✓ {}", capability);
    }

    let limitations = profile.tier.limitations();
    if !limitations.is_empty() {
        println!("\n=== Limitations ===");
        for limitation in limitations {
            println!("  ✗ {}", limitation);
        }
    }

    // Show recommended models
    println!("\n=== Recommended Models ===");
    let models = profile.tier.recommended_models();
    for model in models.iter().take(3) {
        println!("  • {}", model);
    }

    // Show expected performance
    let (min_time, max_time) = profile.tier.expected_response_time();
    println!("\n=== Expected Performance ===");
    println!("AI Response Time: {}-{} seconds", min_time, max_time);
    println!(
        "Context Window: {} tokens",
        profile.tier.max_context_tokens()
    );
    println!("Warm Chunks: {}", profile.tier.max_warm_chunks());

    // Show detailed info if requested
    if args.detailed {
        println!("\n=== Adaptive Configuration ===");
        println!(
            "Background Tasks: {}",
            if profile.tier.disable_background_tasks() {
                "Disabled"
            } else {
                "Enabled"
            }
        );
        println!(
            "Indexer: {}",
            if profile.tier.disable_indexer() {
                "Disabled"
            } else {
                "Enabled"
            }
        );
        println!(
            "Streaming: {}",
            if profile.tier.streaming_only() {
                "Required"
            } else {
                "Optional"
            }
        );
        println!(
            "Single-File Mode: {}",
            if profile.tier.single_file_mode() {
                "Enabled"
            } else {
                "Disabled"
            }
        );
        println!(
            "Recommended Quantization: {}",
            profile.tier.recommended_quantization()
        );
    }

    // Show upgrade suggestions if requested
    if args.upgrades {
        let upgrades = profile.get_upgrade_suggestions();
        if !upgrades.is_empty() {
            println!("\n=== Upgrade Suggestions ===\n");
            for (i, upgrade) in upgrades.iter().enumerate() {
                println!("Priority {}: {} Upgrade", i + 1, upgrade.component);
                println!("  Current: {}", upgrade.current);
                println!("  Recommended: {}", upgrade.recommended);
                println!("  Cost: {}", upgrade.estimated_cost);
                println!("  Gain: {}", upgrade.performance_gain);
                println!();
            }
        } else {
            println!("\n✓ No immediate upgrade recommendations - your system is well-equipped!");
        }
    } else {
        println!("\nRun 'ted system --upgrades' to see upgrade suggestions");
    }

    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== HardwareInfo Serialization ====================

    #[test]
    fn test_hardware_info_serialization() {
        let info = HardwareInfo {
            tier: "Medium".to_string(),
            tier_description: "Good performance for most tasks".to_string(),
            cpu_brand: "Intel Core i7".to_string(),
            cpu_cores: 8,
            ram_gb: 16,
            has_ssd: true,
            architecture: "X86_64".to_string(),
            is_sbc: false,
            cpu_year: Some(2020),
            recommended_models: vec!["claude-3-5-sonnet".to_string()],
            expected_response_time: (2, 5),
            capabilities: vec!["Code completion".to_string()],
            limitations: vec![],
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"tier\":\"Medium\""));
        assert!(json.contains("\"cpuBrand\":\"Intel Core i7\"")); // camelCase
        assert!(json.contains("\"cpuCores\":8"));
        assert!(json.contains("\"ramGb\":16"));
        assert!(json.contains("\"hasSsd\":true"));
        assert!(json.contains("\"isSbc\":false"));
    }

    #[test]
    fn test_hardware_info_json_camel_case() {
        let info = HardwareInfo {
            tier: "Small".to_string(),
            tier_description: "Basic".to_string(),
            cpu_brand: "AMD".to_string(),
            cpu_cores: 4,
            ram_gb: 8,
            has_ssd: false,
            architecture: "X86_64".to_string(),
            is_sbc: false,
            cpu_year: None,
            recommended_models: vec![],
            expected_response_time: (5, 10),
            capabilities: vec![],
            limitations: vec!["Limited context".to_string()],
        };

        let json = serde_json::to_string(&info).unwrap();
        // Verify camelCase conversion
        assert!(json.contains("tierDescription"));
        assert!(json.contains("cpuBrand"));
        assert!(json.contains("cpuCores"));
        assert!(json.contains("ramGb"));
        assert!(json.contains("hasSsd"));
        assert!(json.contains("cpuYear"));
        assert!(json.contains("recommendedModels"));
        assert!(json.contains("expectedResponseTime"));
        // Not snake_case
        assert!(!json.contains("tier_description"));
        assert!(!json.contains("cpu_brand"));
    }

    #[test]
    fn test_hardware_info_null_cpu_year() {
        let info = HardwareInfo {
            tier: "Ancient".to_string(),
            tier_description: "Legacy hardware".to_string(),
            cpu_brand: "Unknown".to_string(),
            cpu_cores: 2,
            ram_gb: 4,
            has_ssd: false,
            architecture: "X86_64".to_string(),
            is_sbc: false,
            cpu_year: None, // Unknown
            recommended_models: vec![],
            expected_response_time: (30, 60),
            capabilities: vec![],
            limitations: vec![],
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"cpuYear\":null"));
    }

    #[test]
    fn test_hardware_info_sbc_detection() {
        let info = HardwareInfo {
            tier: "UltraTiny".to_string(),
            tier_description: "SBC tier".to_string(),
            cpu_brand: "ARM Cortex".to_string(),
            cpu_cores: 4,
            ram_gb: 4,
            has_ssd: false,
            architecture: "ARM64".to_string(),
            is_sbc: true, // Raspberry Pi
            cpu_year: Some(2021),
            recommended_models: vec![],
            expected_response_time: (20, 40),
            capabilities: vec![],
            limitations: vec!["Limited to very small models".to_string()],
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"isSbc\":true"));
        assert!(json.contains("\"architecture\":\"ARM64\""));
    }

    #[test]
    fn test_hardware_info_expected_response_time_tuple() {
        let info = HardwareInfo {
            tier: "Large".to_string(),
            tier_description: "High-end".to_string(),
            cpu_brand: "Apple M2".to_string(),
            cpu_cores: 10,
            ram_gb: 32,
            has_ssd: true,
            architecture: "ARM64".to_string(),
            is_sbc: false,
            cpu_year: Some(2023),
            recommended_models: vec!["claude-3-opus".to_string()],
            expected_response_time: (1, 3),
            capabilities: vec!["Full agent capabilities".to_string()],
            limitations: vec![],
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"expectedResponseTime\":[1,3]"));
    }

    #[test]
    fn test_hardware_info_recommended_models_array() {
        let info = HardwareInfo {
            tier: "Medium".to_string(),
            tier_description: "Good".to_string(),
            cpu_brand: "Intel".to_string(),
            cpu_cores: 6,
            ram_gb: 16,
            has_ssd: true,
            architecture: "X86_64".to_string(),
            is_sbc: false,
            cpu_year: Some(2021),
            recommended_models: vec![
                "llama3:8b".to_string(),
                "mistral:7b".to_string(),
                "codellama:7b".to_string(),
            ],
            expected_response_time: (3, 8),
            capabilities: vec![],
            limitations: vec![],
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("llama3:8b"));
        assert!(json.contains("mistral:7b"));
        assert!(json.contains("codellama:7b"));
    }

    #[test]
    fn test_hardware_info_capabilities_and_limitations() {
        let info = HardwareInfo {
            tier: "Small".to_string(),
            tier_description: "Entry level".to_string(),
            cpu_brand: "Intel i5".to_string(),
            cpu_cores: 4,
            ram_gb: 8,
            has_ssd: true,
            architecture: "X86_64".to_string(),
            is_sbc: false,
            cpu_year: Some(2019),
            recommended_models: vec!["phi3:mini".to_string()],
            expected_response_time: (5, 15),
            capabilities: vec![
                "Basic code completion".to_string(),
                "Simple refactoring".to_string(),
            ],
            limitations: vec![
                "Limited context window".to_string(),
                "Slower response times".to_string(),
            ],
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("Basic code completion"));
        assert!(json.contains("Limited context window"));
    }

    #[test]
    fn test_hardware_info_pretty_json() {
        let info = HardwareInfo {
            tier: "Medium".to_string(),
            tier_description: "Good".to_string(),
            cpu_brand: "AMD Ryzen".to_string(),
            cpu_cores: 8,
            ram_gb: 32,
            has_ssd: true,
            architecture: "X86_64".to_string(),
            is_sbc: false,
            cpu_year: Some(2022),
            recommended_models: vec!["llama3:8b".to_string()],
            expected_response_time: (2, 6),
            capabilities: vec!["Full features".to_string()],
            limitations: vec![],
        };

        let pretty = serde_json::to_string_pretty(&info).unwrap();
        // Pretty format should have newlines and indentation
        assert!(pretty.contains("\n"));
        assert!(pretty.contains("  ")); // Indentation
    }

    // ==================== SystemArgs Tests ====================

    #[test]
    fn test_system_args_default() {
        let args = SystemArgs {
            upgrades: false,
            detailed: false,
        };
        assert!(!args.upgrades);
        assert!(!args.detailed);
    }

    #[test]
    fn test_system_args_with_upgrades() {
        let args = SystemArgs {
            upgrades: true,
            detailed: false,
        };
        assert!(args.upgrades);
    }

    #[test]
    fn test_system_args_with_detailed() {
        let args = SystemArgs {
            upgrades: false,
            detailed: true,
        };
        assert!(args.detailed);
    }

    #[test]
    fn test_system_args_both_flags() {
        let args = SystemArgs {
            upgrades: true,
            detailed: true,
        };
        assert!(args.upgrades);
        assert!(args.detailed);
    }

    // ==================== OutputFormat Tests ====================

    #[test]
    fn test_output_format_json_match() {
        let format = OutputFormat::Json;
        assert!(matches!(format, OutputFormat::Json));
    }

    #[test]
    fn test_output_format_text_match() {
        let format = OutputFormat::Text;
        assert!(matches!(format, OutputFormat::Text));
    }

    #[test]
    fn test_output_format_markdown_match() {
        let format = OutputFormat::Markdown;
        assert!(matches!(format, OutputFormat::Markdown));
    }
}
