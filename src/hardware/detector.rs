// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! System hardware detection and profiling

use serde::{Deserialize, Serialize};
use std::path::Path;
use sysinfo::System;

use super::tier::HardwareTier;
use crate::error::{Result, TedError};

/// CPU architecture classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CpuArchitecture {
    X86_64,
    ARM64,
    ARM32,
    Other,
}

impl CpuArchitecture {
    /// Detect the current CPU architecture
    pub fn detect() -> Self {
        let arch = std::env::consts::ARCH;
        match arch {
            "x86_64" | "amd64" => CpuArchitecture::X86_64,
            "aarch64" | "arm64" => CpuArchitecture::ARM64,
            "arm" | "armv7" => CpuArchitecture::ARM32,
            _ => CpuArchitecture::Other,
        }
    }
}

/// Complete system hardware profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemProfile {
    /// Total system RAM in GB
    pub ram_gb: usize,
    /// GPU VRAM in GB (if available)
    pub vram_gb: Option<u32>,
    /// Number of CPU cores
    pub cpu_cores: usize,
    /// Estimated CPU generation year (if detectable)
    pub cpu_year: Option<u32>,
    /// CPU brand/model name
    pub cpu_brand: String,
    /// Whether system has SSD storage
    pub has_ssd: bool,
    /// CPU architecture
    pub architecture: CpuArchitecture,
    /// Whether this is a single-board computer (Raspberry Pi, etc.)
    pub is_sbc: bool,
    /// Detected hardware tier
    pub tier: HardwareTier,
}

impl SystemProfile {
    /// Detect the current system's hardware profile
    pub fn detect() -> Result<Self> {
        let mut sys = System::new_all();

        // Refresh system information
        sys.refresh_all();

        // Get RAM in GB
        let ram_bytes = sys.total_memory();
        let ram_gb = (ram_bytes / (1024 * 1024 * 1024)) as usize;

        // Get CPU information
        let cpu_cores = sys.cpus().len();
        let cpu_brand = sys
            .cpus()
            .first()
            .map(|cpu| cpu.brand().to_string())
            .unwrap_or_else(|| "Unknown CPU".to_string());

        // Detect architecture
        let architecture = CpuArchitecture::detect();

        // Detect if this is a Raspberry Pi or other SBC
        let is_sbc = Self::detect_sbc();

        // Estimate CPU year from brand string (heuristic)
        let cpu_year = Self::estimate_cpu_year(&cpu_brand);

        // Detect SSD (heuristic based on common patterns)
        let has_ssd = Self::detect_ssd();

        // VRAM detection is platform-specific and complex
        // For now, we'll leave it as None and can enhance later
        let vram_gb = None;

        // Determine hardware tier based on all factors
        let tier = Self::determine_tier(ram_gb, cpu_cores, cpu_year, architecture, is_sbc, has_ssd);

        Ok(SystemProfile {
            ram_gb,
            vram_gb,
            cpu_cores,
            cpu_year,
            cpu_brand,
            has_ssd,
            architecture,
            is_sbc,
            tier,
        })
    }

    /// Detect if the system is a single-board computer (Raspberry Pi, etc.)
    fn detect_sbc() -> bool {
        // Check for Raspberry Pi-specific files
        if Path::new("/proc/device-tree/model").exists() {
            if let Ok(model) = std::fs::read_to_string("/proc/device-tree/model") {
                return model.to_lowercase().contains("raspberry pi");
            }
        }

        // Check for other SBC indicators
        if Path::new("/sys/firmware/devicetree/base/model").exists() {
            return true;
        }

        false
    }

    /// Estimate CPU generation year from brand string (heuristic)
    fn estimate_cpu_year(brand: &str) -> Option<u32> {
        let brand_lower = brand.to_lowercase();

        // Intel patterns
        if brand_lower.contains("intel") {
            // Core i series generations
            if brand_lower.contains("13th gen") || brand_lower.contains("i9-13") {
                return Some(2023);
            }
            if brand_lower.contains("12th gen") || brand_lower.contains("i9-12") {
                return Some(2022);
            }
            if brand_lower.contains("11th gen") || brand_lower.contains("i9-11") {
                return Some(2021);
            }
            if brand_lower.contains("10th gen") || brand_lower.contains("i9-10") {
                return Some(2020);
            }
            if brand_lower.contains("9th gen") || brand_lower.contains("i9-9") {
                return Some(2019);
            }
            if brand_lower.contains("8th gen") || brand_lower.contains("i7-8") {
                return Some(2018);
            }
            if brand_lower.contains("7th gen") || brand_lower.contains("i7-7") {
                return Some(2017);
            }
            if brand_lower.contains("6th gen") || brand_lower.contains("i7-6") {
                return Some(2016);
            }
            if brand_lower.contains("5th gen") || brand_lower.contains("i7-5") {
                return Some(2015);
            }
            if brand_lower.contains("4th gen") || brand_lower.contains("i7-4") {
                return Some(2014);
            }
            if brand_lower.contains("3rd gen") || brand_lower.contains("i7-3") {
                return Some(2012);
            }
            if brand_lower.contains("2nd gen") || brand_lower.contains("i7-2") {
                return Some(2011);
            }
            if brand_lower.contains("core 2") {
                return Some(2008);
            }
        }

        // AMD patterns
        if brand_lower.contains("amd") {
            if brand_lower.contains("ryzen 9 7") {
                return Some(2023);
            }
            if brand_lower.contains("ryzen 9 5") || brand_lower.contains("ryzen 7 5") {
                return Some(2021);
            }
            if brand_lower.contains("ryzen 9 3") || brand_lower.contains("ryzen 7 3") {
                return Some(2019);
            }
            if brand_lower.contains("ryzen 7 2") {
                return Some(2018);
            }
            if brand_lower.contains("ryzen 7 1") {
                return Some(2017);
            }
        }

        // Apple Silicon
        if brand_lower.contains("apple") {
            if brand_lower.contains("m3") {
                return Some(2023);
            }
            if brand_lower.contains("m2") {
                return Some(2022);
            }
            if brand_lower.contains("m1") {
                return Some(2021);
            }
        }

        None
    }

    /// Detect if the system has SSD storage (heuristic)
    fn detect_ssd() -> bool {
        #[cfg(target_os = "linux")]
        {
            // Check /sys/block for rotational devices
            if let Ok(entries) = std::fs::read_dir("/sys/block") {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy();
                        // Check main storage devices (skip loop, dm, etc.)
                        if name_str.starts_with("sd")
                            || name_str.starts_with("nvme")
                            || name_str.starts_with("vd")
                        {
                            let rotational_path = path.join("queue/rotational");
                            if let Ok(content) = std::fs::read_to_string(&rotational_path) {
                                // If rotational = 0, it's an SSD
                                if content.trim() == "0" {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            // Default to false if we can't determine
            false
        }

        #[cfg(target_os = "macos")]
        {
            // On macOS, check if there's an SSD (most modern Macs have SSDs)
            // This is a heuristic - we'd need more complex logic for older Macs
            use std::process::Command;
            if let Ok(output) = Command::new("diskutil").arg("info").arg("/").output() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                if output_str.contains("Solid State") || output_str.contains("SSD") {
                    return true;
                }
            }
            // Modern Macs typically have SSDs
            true
        }

        #[cfg(target_os = "windows")]
        {
            // On Windows, we'd need to use WMI or similar
            // For now, assume SSD on Windows (most modern Windows machines have SSDs)
            true
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            // Unknown platform, assume no SSD
            false
        }
    }

    /// Determine hardware tier based on system specs
    fn determine_tier(
        ram_gb: usize,
        cpu_cores: usize,
        cpu_year: Option<u32>,
        architecture: CpuArchitecture,
        is_sbc: bool,
        has_ssd: bool,
    ) -> HardwareTier {
        // Raspberry Pi and other SBCs -> UltraTiny
        if is_sbc {
            return HardwareTier::UltraTiny;
        }

        // ARM architecture (non-SBC) -> likely modern devices
        if architecture == CpuArchitecture::ARM64 && ram_gb >= 16 {
            return HardwareTier::Medium;
        }

        // Very low RAM -> Ancient or Tiny
        if ram_gb < 8 {
            return HardwareTier::Ancient;
        }

        // Use CPU year as primary classifier
        if let Some(year) = cpu_year {
            if year >= 2021 && ram_gb >= 32 {
                return HardwareTier::Large;
            }
            if year >= 2020 && ram_gb >= 16 {
                return HardwareTier::Medium;
            }
            if year >= 2018 && ram_gb >= 8 {
                return HardwareTier::Small;
            }
            if year >= 2015 && ram_gb >= 8 {
                return HardwareTier::Tiny;
            }
            if year <= 2015 {
                return HardwareTier::Ancient;
            }
        }

        // Fallback to RAM and core-based heuristics
        match (ram_gb, cpu_cores, has_ssd) {
            (32.., 8.., _) => HardwareTier::Large,
            (16..=31, 8.., true) => HardwareTier::Medium,
            (16..=31, 4..=7, _) => HardwareTier::Small,
            (8..=15, 4.., true) => HardwareTier::Small,
            (8..=15, 4.., false) => HardwareTier::Tiny,
            (8..=15, ..=3, _) => HardwareTier::Ancient,
            _ => HardwareTier::Ancient,
        }
    }

    /// Check if system meets minimum requirements
    pub fn meets_minimum_requirements(&self) -> Result<()> {
        if self.ram_gb < 8 {
            return Err(TedError::Config(format!(
                "Minimum 8GB RAM required. Current: {}GB. Upgrade cost: ~$30-40.",
                self.ram_gb
            )));
        }
        Ok(())
    }

    /// Check if thermal throttling is a risk (for SBCs)
    pub fn thermal_throttle_risk(&self) -> bool {
        self.is_sbc
    }

    /// Get recommended model for this hardware profile
    pub fn recommended_model(&self) -> &'static str {
        self.tier.recommended_models()[0]
    }

    /// Should streaming be used for this hardware?
    pub fn should_use_streaming(&self) -> bool {
        self.tier.streaming_only()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_architecture_detect() {
        let arch = CpuArchitecture::detect();
        // Just ensure it doesn't panic
        assert!(matches!(
            arch,
            CpuArchitecture::X86_64
                | CpuArchitecture::ARM64
                | CpuArchitecture::ARM32
                | CpuArchitecture::Other
        ));
    }

    #[test]
    fn test_estimate_cpu_year() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel Core i9-13900K"),
            Some(2023)
        );
        assert_eq!(
            SystemProfile::estimate_cpu_year("AMD Ryzen 9 5900X"),
            Some(2021)
        );
        assert_eq!(SystemProfile::estimate_cpu_year("Apple M1"), Some(2021));
        assert_eq!(SystemProfile::estimate_cpu_year("Unknown CPU"), None);
    }

    #[test]
    fn test_determine_tier() {
        // Large tier
        assert_eq!(
            SystemProfile::determine_tier(32, 8, Some(2021), CpuArchitecture::X86_64, false, true),
            HardwareTier::Large
        );

        // Ancient tier
        assert_eq!(
            SystemProfile::determine_tier(8, 2, Some(2010), CpuArchitecture::X86_64, false, false),
            HardwareTier::Ancient
        );

        // UltraTiny tier (SBC)
        assert_eq!(
            SystemProfile::determine_tier(8, 4, Some(2020), CpuArchitecture::ARM64, true, true),
            HardwareTier::UltraTiny
        );
    }

    #[test]
    fn test_meets_minimum_requirements() {
        let profile = SystemProfile {
            ram_gb: 16,
            vram_gb: None,
            cpu_cores: 4,
            cpu_year: Some(2020),
            cpu_brand: "Test CPU".to_string(),
            has_ssd: true,
            architecture: CpuArchitecture::X86_64,
            is_sbc: false,
            tier: HardwareTier::Small,
        };
        assert!(profile.meets_minimum_requirements().is_ok());

        let low_ram_profile = SystemProfile {
            ram_gb: 4,
            ..profile
        };
        assert!(low_ram_profile.meets_minimum_requirements().is_err());
    }

    #[test]
    fn test_thermal_throttle_risk() {
        let sbc_profile = SystemProfile {
            ram_gb: 8,
            vram_gb: None,
            cpu_cores: 4,
            cpu_year: Some(2020),
            cpu_brand: "BCM2711".to_string(),
            has_ssd: true,
            architecture: CpuArchitecture::ARM64,
            is_sbc: true,
            tier: HardwareTier::UltraTiny,
        };
        assert!(sbc_profile.thermal_throttle_risk());

        let desktop_profile = SystemProfile {
            is_sbc: false,
            ..sbc_profile
        };
        assert!(!desktop_profile.thermal_throttle_risk());
    }

    #[test]
    fn test_serialization() {
        let profile = SystemProfile {
            ram_gb: 16,
            vram_gb: Some(8),
            cpu_cores: 8,
            cpu_year: Some(2021),
            cpu_brand: "Test CPU".to_string(),
            has_ssd: true,
            architecture: CpuArchitecture::X86_64,
            is_sbc: false,
            tier: HardwareTier::Medium,
        };

        let json = serde_json::to_string(&profile).unwrap();
        let parsed: SystemProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(profile.ram_gb, parsed.ram_gb);
        assert_eq!(profile.tier, parsed.tier);
    }

    // ===== Additional CpuArchitecture Tests =====

    #[test]
    fn test_cpu_architecture_variants() {
        // Test all variants are distinct
        assert_ne!(CpuArchitecture::X86_64, CpuArchitecture::ARM64);
        assert_ne!(CpuArchitecture::ARM64, CpuArchitecture::ARM32);
        assert_ne!(CpuArchitecture::ARM32, CpuArchitecture::Other);
    }

    #[test]
    fn test_cpu_architecture_equality() {
        assert_eq!(CpuArchitecture::X86_64, CpuArchitecture::X86_64);
        assert_eq!(CpuArchitecture::ARM64, CpuArchitecture::ARM64);
        assert_eq!(CpuArchitecture::ARM32, CpuArchitecture::ARM32);
        assert_eq!(CpuArchitecture::Other, CpuArchitecture::Other);
    }

    #[test]
    fn test_cpu_architecture_clone() {
        let arch = CpuArchitecture::X86_64;
        let cloned = arch;
        assert_eq!(arch, cloned);
    }

    #[test]
    fn test_cpu_architecture_debug() {
        let arch = CpuArchitecture::ARM64;
        let debug = format!("{:?}", arch);
        assert!(debug.contains("ARM64"));
    }

    #[test]
    fn test_cpu_architecture_serialization() {
        let arch = CpuArchitecture::X86_64;
        let json = serde_json::to_string(&arch).unwrap();
        let parsed: CpuArchitecture = serde_json::from_str(&json).unwrap();
        assert_eq!(arch, parsed);
    }

    // ===== Additional CPU Year Estimation Tests =====

    #[test]
    fn test_estimate_cpu_year_intel_12th_gen() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel 12th Gen Core i7-12700K"),
            Some(2022)
        );
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel Core i9-12900K"),
            Some(2022)
        );
    }

    #[test]
    fn test_estimate_cpu_year_intel_11th_gen() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel 11th Gen Core i9-11900K"),
            Some(2021)
        );
    }

    #[test]
    fn test_estimate_cpu_year_intel_10th_gen() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel Core i9-10900K 10th Gen"),
            Some(2020)
        );
    }

    #[test]
    fn test_estimate_cpu_year_intel_9th_gen() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel Core i9-9900K"),
            Some(2019)
        );
    }

    #[test]
    fn test_estimate_cpu_year_intel_8th_gen() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel Core i7-8700K 8th Gen"),
            Some(2018)
        );
    }

    #[test]
    fn test_estimate_cpu_year_intel_7th_gen() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel Core i7-7700K 7th Gen"),
            Some(2017)
        );
    }

    #[test]
    fn test_estimate_cpu_year_intel_6th_gen() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel Core i7-6700K 6th Gen"),
            Some(2016)
        );
    }

    #[test]
    fn test_estimate_cpu_year_intel_5th_gen() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel Core i7-5775C 5th Gen"),
            Some(2015)
        );
    }

    #[test]
    fn test_estimate_cpu_year_intel_4th_gen() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel Core i7-4790K 4th Gen"),
            Some(2014)
        );
    }

    #[test]
    fn test_estimate_cpu_year_intel_3rd_gen() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel Core i7-3770K 3rd Gen"),
            Some(2012)
        );
    }

    #[test]
    fn test_estimate_cpu_year_intel_2nd_gen() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel Core i7-2600K 2nd Gen"),
            Some(2011)
        );
    }

    #[test]
    fn test_estimate_cpu_year_intel_core2() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("Intel Core 2 Duo E8400"),
            Some(2008)
        );
    }

    #[test]
    fn test_estimate_cpu_year_amd_ryzen_7900() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("AMD Ryzen 9 7900X"),
            Some(2023)
        );
    }

    #[test]
    fn test_estimate_cpu_year_amd_ryzen_3700() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("AMD Ryzen 7 3700X"),
            Some(2019)
        );
    }

    #[test]
    fn test_estimate_cpu_year_amd_ryzen_2700() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("AMD Ryzen 7 2700X"),
            Some(2018)
        );
    }

    #[test]
    fn test_estimate_cpu_year_amd_ryzen_1700() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("AMD Ryzen 7 1700X"),
            Some(2017)
        );
    }

    #[test]
    fn test_estimate_cpu_year_apple_m2() {
        assert_eq!(SystemProfile::estimate_cpu_year("Apple M2 Pro"), Some(2022));
    }

    #[test]
    fn test_estimate_cpu_year_apple_m3() {
        assert_eq!(SystemProfile::estimate_cpu_year("Apple M3 Max"), Some(2023));
    }

    #[test]
    fn test_estimate_cpu_year_case_insensitive() {
        assert_eq!(
            SystemProfile::estimate_cpu_year("INTEL CORE I9-13900K"),
            Some(2023)
        );
        assert_eq!(SystemProfile::estimate_cpu_year("apple m1"), Some(2021));
    }

    // ===== Additional Determine Tier Tests =====

    #[test]
    fn test_determine_tier_medium() {
        assert_eq!(
            SystemProfile::determine_tier(16, 8, Some(2020), CpuArchitecture::X86_64, false, true),
            HardwareTier::Medium
        );
    }

    #[test]
    fn test_determine_tier_small() {
        assert_eq!(
            SystemProfile::determine_tier(8, 4, Some(2018), CpuArchitecture::X86_64, false, true),
            HardwareTier::Small
        );
    }

    #[test]
    fn test_determine_tier_tiny() {
        assert_eq!(
            SystemProfile::determine_tier(8, 4, Some(2015), CpuArchitecture::X86_64, false, true),
            HardwareTier::Tiny
        );
    }

    #[test]
    fn test_determine_tier_low_ram() {
        assert_eq!(
            SystemProfile::determine_tier(4, 8, Some(2021), CpuArchitecture::X86_64, false, true),
            HardwareTier::Ancient
        );
    }

    #[test]
    fn test_determine_tier_arm64_with_good_ram() {
        assert_eq!(
            SystemProfile::determine_tier(16, 8, Some(2020), CpuArchitecture::ARM64, false, true),
            HardwareTier::Medium
        );
    }

    #[test]
    fn test_determine_tier_no_cpu_year_large() {
        assert_eq!(
            SystemProfile::determine_tier(32, 8, None, CpuArchitecture::X86_64, false, true),
            HardwareTier::Large
        );
    }

    #[test]
    fn test_determine_tier_no_cpu_year_medium() {
        assert_eq!(
            SystemProfile::determine_tier(16, 8, None, CpuArchitecture::X86_64, false, true),
            HardwareTier::Medium
        );
    }

    #[test]
    fn test_determine_tier_no_cpu_year_small_with_ssd() {
        assert_eq!(
            SystemProfile::determine_tier(8, 4, None, CpuArchitecture::X86_64, false, true),
            HardwareTier::Small
        );
    }

    #[test]
    fn test_determine_tier_no_cpu_year_tiny_without_ssd() {
        assert_eq!(
            SystemProfile::determine_tier(8, 4, None, CpuArchitecture::X86_64, false, false),
            HardwareTier::Tiny
        );
    }

    #[test]
    fn test_determine_tier_no_cpu_year_ancient_low_cores() {
        assert_eq!(
            SystemProfile::determine_tier(8, 2, None, CpuArchitecture::X86_64, false, true),
            HardwareTier::Ancient
        );
    }

    #[test]
    fn test_determine_tier_very_old_cpu() {
        assert_eq!(
            SystemProfile::determine_tier(16, 8, Some(2014), CpuArchitecture::X86_64, false, true),
            HardwareTier::Ancient
        );
    }

    // ===== SystemProfile Method Tests =====

    #[test]
    fn test_recommended_model() {
        let profile = SystemProfile {
            ram_gb: 32,
            vram_gb: None,
            cpu_cores: 8,
            cpu_year: Some(2021),
            cpu_brand: "Test CPU".to_string(),
            has_ssd: true,
            architecture: CpuArchitecture::X86_64,
            is_sbc: false,
            tier: HardwareTier::Large,
        };
        let model = profile.recommended_model();
        assert!(!model.is_empty());
    }

    #[test]
    fn test_should_use_streaming() {
        let large_profile = SystemProfile {
            ram_gb: 32,
            vram_gb: None,
            cpu_cores: 8,
            cpu_year: Some(2021),
            cpu_brand: "Test CPU".to_string(),
            has_ssd: true,
            architecture: CpuArchitecture::X86_64,
            is_sbc: false,
            tier: HardwareTier::Large,
        };
        // Large tier doesn't require streaming-only
        assert!(!large_profile.should_use_streaming());

        let sbc_profile = SystemProfile {
            ram_gb: 8,
            vram_gb: None,
            cpu_cores: 4,
            cpu_year: None,
            cpu_brand: "BCM2711".to_string(),
            has_ssd: false,
            architecture: CpuArchitecture::ARM64,
            is_sbc: true,
            tier: HardwareTier::UltraTiny,
        };
        // UltraTiny requires streaming
        assert!(sbc_profile.should_use_streaming());
    }

    // ===== SystemProfile Debug and Clone Tests =====

    #[test]
    fn test_system_profile_debug() {
        let profile = SystemProfile {
            ram_gb: 16,
            vram_gb: None,
            cpu_cores: 8,
            cpu_year: Some(2021),
            cpu_brand: "Test CPU".to_string(),
            has_ssd: true,
            architecture: CpuArchitecture::X86_64,
            is_sbc: false,
            tier: HardwareTier::Medium,
        };
        let debug = format!("{:?}", profile);
        assert!(debug.contains("SystemProfile"));
        assert!(debug.contains("16"));
    }

    #[test]
    fn test_system_profile_clone() {
        let profile = SystemProfile {
            ram_gb: 16,
            vram_gb: Some(8),
            cpu_cores: 8,
            cpu_year: Some(2021),
            cpu_brand: "Test CPU".to_string(),
            has_ssd: true,
            architecture: CpuArchitecture::X86_64,
            is_sbc: false,
            tier: HardwareTier::Medium,
        };
        let cloned = profile.clone();
        assert_eq!(cloned.ram_gb, profile.ram_gb);
        assert_eq!(cloned.vram_gb, profile.vram_gb);
        assert_eq!(cloned.cpu_cores, profile.cpu_cores);
        assert_eq!(cloned.tier, profile.tier);
    }

    #[test]
    fn test_system_profile_with_vram() {
        let profile = SystemProfile {
            ram_gb: 32,
            vram_gb: Some(24),
            cpu_cores: 16,
            cpu_year: Some(2023),
            cpu_brand: "Test GPU Workstation".to_string(),
            has_ssd: true,
            architecture: CpuArchitecture::X86_64,
            is_sbc: false,
            tier: HardwareTier::Large,
        };
        assert_eq!(profile.vram_gb, Some(24));
    }

    #[test]
    fn test_system_profile_without_vram() {
        let profile = SystemProfile {
            ram_gb: 16,
            vram_gb: None,
            cpu_cores: 8,
            cpu_year: Some(2020),
            cpu_brand: "Test CPU".to_string(),
            has_ssd: true,
            architecture: CpuArchitecture::X86_64,
            is_sbc: false,
            tier: HardwareTier::Medium,
        };
        assert!(profile.vram_gb.is_none());
    }

    #[test]
    fn test_meets_minimum_requirements_exactly_8gb() {
        let profile = SystemProfile {
            ram_gb: 8,
            vram_gb: None,
            cpu_cores: 4,
            cpu_year: Some(2018),
            cpu_brand: "Test CPU".to_string(),
            has_ssd: true,
            architecture: CpuArchitecture::X86_64,
            is_sbc: false,
            tier: HardwareTier::Small,
        };
        assert!(profile.meets_minimum_requirements().is_ok());
    }

    #[test]
    fn test_meets_minimum_requirements_7gb() {
        let profile = SystemProfile {
            ram_gb: 7,
            vram_gb: None,
            cpu_cores: 4,
            cpu_year: Some(2018),
            cpu_brand: "Test CPU".to_string(),
            has_ssd: true,
            architecture: CpuArchitecture::X86_64,
            is_sbc: false,
            tier: HardwareTier::Ancient,
        };
        let result = profile.meets_minimum_requirements();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("8GB"));
    }

    // ==================== Additional Edge Case Tests ====================

    #[test]
    fn test_determine_tier_arm32() {
        // ARM32 with year 2020, 8GB RAM -> Small (year >= 2018 && ram >= 8)
        assert_eq!(
            SystemProfile::determine_tier(8, 4, Some(2020), CpuArchitecture::ARM32, false, true),
            HardwareTier::Small
        );
    }

    #[test]
    fn test_determine_tier_other_architecture() {
        // Other architecture falls through to RAM/core logic
        assert_eq!(
            SystemProfile::determine_tier(32, 8, None, CpuArchitecture::Other, false, true),
            HardwareTier::Large
        );
    }

    #[test]
    fn test_estimate_cpu_year_empty_string() {
        assert_eq!(SystemProfile::estimate_cpu_year(""), None);
    }

    #[test]
    fn test_estimate_cpu_year_numbers_only() {
        assert_eq!(SystemProfile::estimate_cpu_year("12345"), None);
    }

    #[test]
    fn test_estimate_cpu_year_partial_match() {
        // Contains "intel" but no generation info
        assert_eq!(SystemProfile::estimate_cpu_year("Intel Xeon E5"), None);
    }

    #[test]
    fn test_estimate_cpu_year_amd_no_ryzen() {
        // AMD but not Ryzen
        assert_eq!(SystemProfile::estimate_cpu_year("AMD Athlon 64 X2"), None);
    }

    #[test]
    fn test_determine_tier_16gb_few_cores_with_ssd() {
        // Mid-range RAM, few cores, no year info
        assert_eq!(
            SystemProfile::determine_tier(16, 4, None, CpuArchitecture::X86_64, false, true),
            HardwareTier::Small
        );
    }

    #[test]
    fn test_determine_tier_16gb_few_cores_no_ssd() {
        // Mid-range RAM, few cores, no SSD
        assert_eq!(
            SystemProfile::determine_tier(16, 4, None, CpuArchitecture::X86_64, false, false),
            HardwareTier::Small
        );
    }

    #[test]
    fn test_determine_tier_exactly_32gb() {
        // Exactly at Large threshold
        assert_eq!(
            SystemProfile::determine_tier(32, 8, None, CpuArchitecture::X86_64, false, true),
            HardwareTier::Large
        );
    }

    #[test]
    fn test_determine_tier_31gb() {
        // Just under Large threshold
        assert_eq!(
            SystemProfile::determine_tier(31, 8, None, CpuArchitecture::X86_64, false, true),
            HardwareTier::Medium
        );
    }

    #[test]
    fn test_determine_tier_arm64_low_ram() {
        // ARM64 but low RAM, falls through to RAM check
        assert_eq!(
            SystemProfile::determine_tier(4, 4, Some(2020), CpuArchitecture::ARM64, false, true),
            HardwareTier::Ancient
        );
    }

    #[test]
    fn test_determine_tier_arm64_medium_ram_non_sbc() {
        // ARM64 with 15GB falls through ARM64 check (needs >=16GB),
        // then year >= 2018 && ram >= 8 -> Small
        assert_eq!(
            SystemProfile::determine_tier(15, 8, Some(2020), CpuArchitecture::ARM64, false, true),
            HardwareTier::Small
        );
    }

    #[test]
    fn test_determine_tier_year_2019() {
        assert_eq!(
            SystemProfile::determine_tier(8, 4, Some(2019), CpuArchitecture::X86_64, false, true),
            HardwareTier::Small
        );
    }

    #[test]
    fn test_determine_tier_year_2016() {
        assert_eq!(
            SystemProfile::determine_tier(8, 4, Some(2016), CpuArchitecture::X86_64, false, true),
            HardwareTier::Tiny
        );
    }

    #[test]
    fn test_system_profile_serialization_roundtrip() {
        let original = SystemProfile {
            ram_gb: 16,
            vram_gb: Some(8),
            cpu_cores: 8,
            cpu_year: Some(2021),
            cpu_brand: "Test CPU Brand".to_string(),
            has_ssd: true,
            architecture: CpuArchitecture::ARM64,
            is_sbc: false,
            tier: HardwareTier::Medium,
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: SystemProfile = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.ram_gb, original.ram_gb);
        assert_eq!(parsed.vram_gb, original.vram_gb);
        assert_eq!(parsed.cpu_cores, original.cpu_cores);
        assert_eq!(parsed.cpu_year, original.cpu_year);
        assert_eq!(parsed.cpu_brand, original.cpu_brand);
        assert_eq!(parsed.has_ssd, original.has_ssd);
        assert_eq!(parsed.architecture, original.architecture);
        assert_eq!(parsed.is_sbc, original.is_sbc);
        assert_eq!(parsed.tier, original.tier);
    }

    #[test]
    fn test_cpu_architecture_all_serialization() {
        for arch in [
            CpuArchitecture::X86_64,
            CpuArchitecture::ARM64,
            CpuArchitecture::ARM32,
            CpuArchitecture::Other,
        ] {
            let json = serde_json::to_string(&arch).unwrap();
            let parsed: CpuArchitecture = serde_json::from_str(&json).unwrap();
            assert_eq!(arch, parsed);
        }
    }

    #[test]
    fn test_thermal_throttle_risk_various_profiles() {
        // SBC always has risk
        let sbc = SystemProfile {
            ram_gb: 8,
            vram_gb: None,
            cpu_cores: 4,
            cpu_year: Some(2022),
            cpu_brand: "Cortex".to_string(),
            has_ssd: true,
            architecture: CpuArchitecture::ARM64,
            is_sbc: true,
            tier: HardwareTier::UltraTiny,
        };
        assert!(sbc.thermal_throttle_risk());

        // Desktop never has risk
        let desktop = SystemProfile {
            is_sbc: false,
            ..sbc.clone()
        };
        assert!(!desktop.thermal_throttle_risk());
    }
}
