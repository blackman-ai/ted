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
