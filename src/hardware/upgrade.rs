// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Upgrade suggestions for improving hardware performance

use serde::{Deserialize, Serialize};

use super::detector::SystemProfile;
use super::tier::HardwareTier;

/// A suggested hardware upgrade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upgrade {
    /// Component to upgrade (e.g., "RAM", "Storage", etc.)
    pub component: String,
    /// Current value/status
    pub current: String,
    /// Recommended value/status
    pub recommended: String,
    /// Estimated cost range
    pub estimated_cost: String,
    /// Expected performance gain
    pub performance_gain: String,
    /// Priority level (1 = highest)
    pub priority: u8,
}

impl SystemProfile {
    /// Get upgrade suggestions for this hardware profile
    pub fn get_upgrade_suggestions(&self) -> Vec<Upgrade> {
        let mut suggestions = Vec::new();

        // Storage upgrade suggestion
        if !self.has_ssd {
            suggestions.push(Upgrade {
                component: "Storage".to_string(),
                current: "HDD".to_string(),
                recommended: if self.is_sbc {
                    "NVMe HAT + 256GB SSD".to_string()
                } else {
                    "240-512GB SSD".to_string()
                },
                estimated_cost: if self.is_sbc {
                    "$45 (HAT $15 + SSD $30)".to_string()
                } else {
                    "$25-35".to_string()
                },
                performance_gain: "10x faster model loading and file operations".to_string(),
                priority: 1,
            });
        }

        // RAM upgrade suggestion
        match self.tier {
            HardwareTier::UltraTiny => {
                if self.is_sbc {
                    // Raspberry Pi can't upgrade RAM, suggest cooling instead
                    suggestions.push(Upgrade {
                        component: "Cooling".to_string(),
                        current: "Passive or basic heatsink".to_string(),
                        recommended: "Active cooling fan".to_string(),
                        estimated_cost: "$10-15".to_string(),
                        performance_gain: "Sustained performance without thermal throttling"
                            .to_string(),
                        priority: 2,
                    });
                }
            }
            HardwareTier::Ancient | HardwareTier::Tiny => {
                if self.ram_gb < 16 {
                    suggestions.push(Upgrade {
                        component: "RAM".to_string(),
                        current: format!("{}GB", self.ram_gb),
                        recommended: "16GB".to_string(),
                        estimated_cost: "$30-40 (used DDR3)".to_string(),
                        performance_gain: "Use larger 3b models, 2-3x faster responses"
                            .to_string(),
                        priority: if !self.has_ssd { 2 } else { 1 },
                    });
                }
            }
            HardwareTier::Small => {
                if self.ram_gb < 32 {
                    suggestions.push(Upgrade {
                        component: "RAM".to_string(),
                        current: format!("{}GB", self.ram_gb),
                        recommended: "32GB".to_string(),
                        estimated_cost: "$50-80".to_string(),
                        performance_gain: "Use 7b+ models, handle larger projects".to_string(),
                        priority: 2,
                    });
                }
            }
            _ => {}
        }

        // Tier upgrade path
        if let Some(next_tier) = self.get_next_tier() {
            let total_cost = self.estimate_tier_upgrade_cost();
            suggestions.push(Upgrade {
                component: "Complete Upgrade".to_string(),
                current: self.tier.to_string(),
                recommended: next_tier.to_string(),
                estimated_cost: total_cost,
                performance_gain: format!(
                    "Move from {} to {}",
                    self.tier.description(),
                    next_tier.description()
                ),
                priority: 3,
            });
        }

        suggestions
    }

    /// Get the next logical tier upgrade
    fn get_next_tier(&self) -> Option<HardwareTier> {
        match self.tier {
            HardwareTier::UltraTiny => Some(HardwareTier::Ancient),
            HardwareTier::Ancient => Some(HardwareTier::Tiny),
            HardwareTier::Tiny => Some(HardwareTier::Small),
            HardwareTier::Small => Some(HardwareTier::Medium),
            HardwareTier::Medium => Some(HardwareTier::Large),
            HardwareTier::Large | HardwareTier::Cloud => None,
        }
    }

    /// Estimate the cost to upgrade to the next tier
    fn estimate_tier_upgrade_cost(&self) -> String {
        match self.tier {
            HardwareTier::UltraTiny => {
                if self.is_sbc {
                    "$100-150 (2010 Dell OptiPlex with upgrades)".to_string()
                } else {
                    "$50-75 (RAM + SSD)".to_string()
                }
            }
            HardwareTier::Ancient => {
                if self.has_ssd {
                    "$30-40 (RAM upgrade to 16GB)".to_string()
                } else {
                    "$55-75 (SSD + RAM)".to_string()
                }
            }
            HardwareTier::Tiny => "$300-500 (newer used laptop)".to_string(),
            HardwareTier::Small => "$800-1200 (modern laptop)".to_string(),
            HardwareTier::Medium => "$2000-3000 (high-end workstation)".to_string(),
            HardwareTier::Large => "No upgrade needed".to_string(),
            HardwareTier::Cloud => "No upgrade needed".to_string(),
        }
    }

    /// Generate a user-friendly upgrade message
    pub fn upgrade_message(&self) -> Option<String> {
        let suggestions = self.get_upgrade_suggestions();
        if suggestions.is_empty() {
            return None;
        }

        let mut message = format!(
            "💡 Upgrade Suggestions for Better Performance\n\n\
             Current: {}GB RAM, {} cores, {} ({})\n\n",
            self.ram_gb,
            self.cpu_cores,
            if self.has_ssd { "SSD" } else { "HDD" },
            self.tier.description()
        );

        for (i, upgrade) in suggestions.iter().take(3).enumerate() {
            message.push_str(&format!(
                "Priority {}: {} upgrade\n\
                 ▸ Current: {}\n\
                 ▸ Recommended: {}\n\
                 ▸ Cost: {}\n\
                 ▸ Gain: {}\n\n",
                i + 1,
                upgrade.component,
                upgrade.current,
                upgrade.recommended,
                upgrade.estimated_cost,
                upgrade.performance_gain
            ));
        }

        Some(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upgrade_suggestions_ancient_no_ssd() {
        let profile = SystemProfile {
            ram_gb: 8,
            vram_gb: None,
            cpu_cores: 2,
            cpu_year: Some(2010),
            cpu_brand: "Intel Core 2 Duo".to_string(),
            has_ssd: false,
            architecture: super::super::detector::CpuArchitecture::X86_64,
            is_sbc: false,
            tier: HardwareTier::Ancient,
        };

        let suggestions = profile.get_upgrade_suggestions();
        assert!(!suggestions.is_empty());

        // Should suggest SSD as priority 1
        let ssd_suggestion = suggestions.iter().find(|u| u.component == "Storage");
        assert!(ssd_suggestion.is_some());
        assert_eq!(ssd_suggestion.unwrap().priority, 1);

        // Should suggest RAM as priority 2
        let ram_suggestion = suggestions.iter().find(|u| u.component == "RAM");
        assert!(ram_suggestion.is_some());
        assert_eq!(ram_suggestion.unwrap().priority, 2);
    }

    #[test]
    fn test_upgrade_suggestions_raspberry_pi() {
        let profile = SystemProfile {
            ram_gb: 8,
            vram_gb: None,
            cpu_cores: 4,
            cpu_year: Some(2020),
            cpu_brand: "BCM2711".to_string(),
            has_ssd: false,
            architecture: super::super::detector::CpuArchitecture::ARM64,
            is_sbc: true,
            tier: HardwareTier::UltraTiny,
        };

        let suggestions = profile.get_upgrade_suggestions();

        // Should suggest NVMe HAT for storage
        let ssd_suggestion = suggestions.iter().find(|u| u.component == "Storage");
        assert!(ssd_suggestion.is_some());
        assert!(ssd_suggestion.unwrap().recommended.contains("NVMe HAT"));

        // Should suggest active cooling
        let cooling_suggestion = suggestions.iter().find(|u| u.component == "Cooling");
        assert!(cooling_suggestion.is_some());
    }

    #[test]
    fn test_upgrade_suggestions_modern_system() {
        let profile = SystemProfile {
            ram_gb: 32,
            vram_gb: Some(8),
            cpu_cores: 8,
            cpu_year: Some(2021),
            cpu_brand: "Apple M1 Pro".to_string(),
            has_ssd: true,
            architecture: super::super::detector::CpuArchitecture::ARM64,
            is_sbc: false,
            tier: HardwareTier::Large,
        };

        let suggestions = profile.get_upgrade_suggestions();
        // Modern system should have minimal or no upgrade suggestions
        assert!(suggestions.is_empty() || suggestions.len() == 1); // Might have tier upgrade suggestion
    }

    #[test]
    fn test_get_next_tier() {
        let profile = SystemProfile {
            ram_gb: 8,
            vram_gb: None,
            cpu_cores: 2,
            cpu_year: Some(2010),
            cpu_brand: "Test CPU".to_string(),
            has_ssd: false,
            architecture: super::super::detector::CpuArchitecture::X86_64,
            is_sbc: false,
            tier: HardwareTier::Ancient,
        };

        assert_eq!(profile.get_next_tier(), Some(HardwareTier::Tiny));
    }

    #[test]
    fn test_upgrade_message() {
        let profile = SystemProfile {
            ram_gb: 8,
            vram_gb: None,
            cpu_cores: 2,
            cpu_year: Some(2010),
            cpu_brand: "Intel Core 2 Duo".to_string(),
            has_ssd: false,
            architecture: super::super::detector::CpuArchitecture::X86_64,
            is_sbc: false,
            tier: HardwareTier::Ancient,
        };

        let message = profile.upgrade_message();
        assert!(message.is_some());

        let msg = message.unwrap();
        assert!(msg.contains("Upgrade Suggestions"));
        assert!(msg.contains("Priority"));
    }
}
