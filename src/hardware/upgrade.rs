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
                        performance_gain: "Use larger 3b models, 2-3x faster responses".to_string(),
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

    // Helper to create test profiles
    fn make_profile(tier: HardwareTier, ram: usize, has_ssd: bool, is_sbc: bool) -> SystemProfile {
        SystemProfile {
            ram_gb: ram,
            vram_gb: None,
            cpu_cores: 4,
            cpu_year: Some(2020),
            cpu_brand: "Test CPU".to_string(),
            has_ssd,
            architecture: super::super::detector::CpuArchitecture::X86_64,
            is_sbc,
            tier,
        }
    }

    // ==================== Upgrade struct tests ====================

    #[test]
    fn test_upgrade_struct_clone() {
        let upgrade = Upgrade {
            component: "RAM".to_string(),
            current: "8GB".to_string(),
            recommended: "16GB".to_string(),
            estimated_cost: "$50".to_string(),
            performance_gain: "2x faster".to_string(),
            priority: 1,
        };
        let cloned = upgrade.clone();
        assert_eq!(cloned.component, "RAM");
        assert_eq!(cloned.priority, 1);
    }

    #[test]
    fn test_upgrade_struct_serialization() {
        let upgrade = Upgrade {
            component: "Storage".to_string(),
            current: "HDD".to_string(),
            recommended: "SSD".to_string(),
            estimated_cost: "$30".to_string(),
            performance_gain: "10x faster".to_string(),
            priority: 1,
        };
        let json = serde_json::to_string(&upgrade).unwrap();
        assert!(json.contains("\"component\":\"Storage\""));
        assert!(json.contains("\"priority\":1"));
    }

    #[test]
    fn test_upgrade_struct_deserialization() {
        let json = r#"{
            "component": "RAM",
            "current": "8GB",
            "recommended": "16GB",
            "estimated_cost": "$50",
            "performance_gain": "Faster",
            "priority": 2
        }"#;
        let upgrade: Upgrade = serde_json::from_str(json).unwrap();
        assert_eq!(upgrade.component, "RAM");
        assert_eq!(upgrade.priority, 2);
    }

    #[test]
    fn test_upgrade_struct_debug() {
        let upgrade = Upgrade {
            component: "Test".to_string(),
            current: "Old".to_string(),
            recommended: "New".to_string(),
            estimated_cost: "$0".to_string(),
            performance_gain: "Better".to_string(),
            priority: 1,
        };
        let debug_str = format!("{:?}", upgrade);
        assert!(debug_str.contains("Test"));
    }

    // ==================== get_upgrade_suggestions tests ====================

    #[test]
    fn test_upgrade_suggestions_ancient_no_ssd() {
        let profile = make_profile(HardwareTier::Ancient, 8, false, false);
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
    fn test_upgrade_suggestions_ancient_with_ssd() {
        let profile = make_profile(HardwareTier::Ancient, 8, true, false);
        let suggestions = profile.get_upgrade_suggestions();

        // Should NOT suggest SSD
        let ssd_suggestion = suggestions.iter().find(|u| u.component == "Storage");
        assert!(ssd_suggestion.is_none());

        // RAM should now be priority 1
        let ram_suggestion = suggestions.iter().find(|u| u.component == "RAM");
        assert!(ram_suggestion.is_some());
        assert_eq!(ram_suggestion.unwrap().priority, 1);
    }

    #[test]
    fn test_upgrade_suggestions_tiny_tier() {
        let profile = make_profile(HardwareTier::Tiny, 8, true, false);
        let suggestions = profile.get_upgrade_suggestions();

        // Should suggest RAM upgrade
        let ram_suggestion = suggestions.iter().find(|u| u.component == "RAM");
        assert!(ram_suggestion.is_some());
        assert!(ram_suggestion.unwrap().recommended.contains("16GB"));
    }

    #[test]
    fn test_upgrade_suggestions_tiny_with_16gb() {
        let profile = make_profile(HardwareTier::Tiny, 16, true, false);
        let suggestions = profile.get_upgrade_suggestions();

        // Should NOT suggest RAM upgrade
        let ram_suggestion = suggestions.iter().find(|u| u.component == "RAM");
        assert!(ram_suggestion.is_none());
    }

    #[test]
    fn test_upgrade_suggestions_small_tier() {
        let profile = make_profile(HardwareTier::Small, 16, true, false);
        let suggestions = profile.get_upgrade_suggestions();

        // Should suggest RAM upgrade to 32GB
        let ram_suggestion = suggestions.iter().find(|u| u.component == "RAM");
        assert!(ram_suggestion.is_some());
        assert!(ram_suggestion.unwrap().recommended.contains("32GB"));
    }

    #[test]
    fn test_upgrade_suggestions_small_with_32gb() {
        let profile = make_profile(HardwareTier::Small, 32, true, false);
        let suggestions = profile.get_upgrade_suggestions();

        // Should NOT suggest RAM upgrade
        let ram_suggestion = suggestions.iter().find(|u| u.component == "RAM");
        assert!(ram_suggestion.is_none());
    }

    #[test]
    fn test_upgrade_suggestions_raspberry_pi() {
        let mut profile = make_profile(HardwareTier::UltraTiny, 8, false, true);
        profile.architecture = super::super::detector::CpuArchitecture::ARM64;
        profile.cpu_brand = "BCM2711".to_string();

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
    fn test_upgrade_suggestions_raspberry_pi_with_ssd() {
        let mut profile = make_profile(HardwareTier::UltraTiny, 8, true, true);
        profile.architecture = super::super::detector::CpuArchitecture::ARM64;

        let suggestions = profile.get_upgrade_suggestions();

        // Should NOT suggest SSD
        let ssd_suggestion = suggestions.iter().find(|u| u.component == "Storage");
        assert!(ssd_suggestion.is_none());

        // Should still suggest active cooling
        let cooling_suggestion = suggestions.iter().find(|u| u.component == "Cooling");
        assert!(cooling_suggestion.is_some());
    }

    #[test]
    fn test_upgrade_suggestions_medium_tier() {
        let profile = make_profile(HardwareTier::Medium, 32, true, false);
        let suggestions = profile.get_upgrade_suggestions();

        // Medium tier should only have tier upgrade suggestion
        assert!(suggestions.len() <= 1);
        if !suggestions.is_empty() {
            assert_eq!(suggestions[0].component, "Complete Upgrade");
        }
    }

    #[test]
    fn test_upgrade_suggestions_large_tier() {
        let profile = make_profile(HardwareTier::Large, 64, true, false);
        let suggestions = profile.get_upgrade_suggestions();

        // Large tier should have no suggestions
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_upgrade_suggestions_cloud_tier() {
        let profile = make_profile(HardwareTier::Cloud, 128, true, false);
        let suggestions = profile.get_upgrade_suggestions();

        // Cloud tier should have no suggestions
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_upgrade_suggestions_includes_tier_upgrade() {
        let profile = make_profile(HardwareTier::Small, 16, true, false);
        let suggestions = profile.get_upgrade_suggestions();

        // Should include tier upgrade suggestion
        let tier_upgrade = suggestions
            .iter()
            .find(|u| u.component == "Complete Upgrade");
        assert!(tier_upgrade.is_some());
        assert_eq!(tier_upgrade.unwrap().priority, 3);
    }

    // ==================== get_next_tier tests ====================

    #[test]
    fn test_get_next_tier_ultra_tiny() {
        let profile = make_profile(HardwareTier::UltraTiny, 4, false, false);
        assert_eq!(profile.get_next_tier(), Some(HardwareTier::Ancient));
    }

    #[test]
    fn test_get_next_tier_ancient() {
        let profile = make_profile(HardwareTier::Ancient, 8, false, false);
        assert_eq!(profile.get_next_tier(), Some(HardwareTier::Tiny));
    }

    #[test]
    fn test_get_next_tier_tiny() {
        let profile = make_profile(HardwareTier::Tiny, 8, true, false);
        assert_eq!(profile.get_next_tier(), Some(HardwareTier::Small));
    }

    #[test]
    fn test_get_next_tier_small() {
        let profile = make_profile(HardwareTier::Small, 16, true, false);
        assert_eq!(profile.get_next_tier(), Some(HardwareTier::Medium));
    }

    #[test]
    fn test_get_next_tier_medium() {
        let profile = make_profile(HardwareTier::Medium, 32, true, false);
        assert_eq!(profile.get_next_tier(), Some(HardwareTier::Large));
    }

    #[test]
    fn test_get_next_tier_large() {
        let profile = make_profile(HardwareTier::Large, 64, true, false);
        assert_eq!(profile.get_next_tier(), None);
    }

    #[test]
    fn test_get_next_tier_cloud() {
        let profile = make_profile(HardwareTier::Cloud, 128, true, false);
        assert_eq!(profile.get_next_tier(), None);
    }

    // ==================== estimate_tier_upgrade_cost tests ====================

    #[test]
    fn test_estimate_cost_ultra_tiny_sbc() {
        let profile = make_profile(HardwareTier::UltraTiny, 4, false, true);
        let cost = profile.estimate_tier_upgrade_cost();
        assert!(cost.contains("Dell OptiPlex"));
    }

    #[test]
    fn test_estimate_cost_ultra_tiny_non_sbc() {
        let profile = make_profile(HardwareTier::UltraTiny, 4, false, false);
        let cost = profile.estimate_tier_upgrade_cost();
        assert!(cost.contains("RAM + SSD"));
    }

    #[test]
    fn test_estimate_cost_ancient_with_ssd() {
        let profile = make_profile(HardwareTier::Ancient, 8, true, false);
        let cost = profile.estimate_tier_upgrade_cost();
        assert!(cost.contains("RAM upgrade"));
    }

    #[test]
    fn test_estimate_cost_ancient_without_ssd() {
        let profile = make_profile(HardwareTier::Ancient, 8, false, false);
        let cost = profile.estimate_tier_upgrade_cost();
        assert!(cost.contains("SSD + RAM"));
    }

    #[test]
    fn test_estimate_cost_tiny() {
        let profile = make_profile(HardwareTier::Tiny, 8, true, false);
        let cost = profile.estimate_tier_upgrade_cost();
        assert!(cost.contains("laptop"));
    }

    #[test]
    fn test_estimate_cost_small() {
        let profile = make_profile(HardwareTier::Small, 16, true, false);
        let cost = profile.estimate_tier_upgrade_cost();
        assert!(cost.contains("modern laptop"));
    }

    #[test]
    fn test_estimate_cost_medium() {
        let profile = make_profile(HardwareTier::Medium, 32, true, false);
        let cost = profile.estimate_tier_upgrade_cost();
        assert!(cost.contains("workstation"));
    }

    #[test]
    fn test_estimate_cost_large() {
        let profile = make_profile(HardwareTier::Large, 64, true, false);
        let cost = profile.estimate_tier_upgrade_cost();
        assert!(cost.contains("No upgrade needed"));
    }

    // ==================== upgrade_message tests ====================

    #[test]
    fn test_upgrade_message_ancient() {
        let profile = make_profile(HardwareTier::Ancient, 8, false, false);
        let message = profile.upgrade_message();
        assert!(message.is_some());

        let msg = message.unwrap();
        assert!(msg.contains("Upgrade Suggestions"));
        assert!(msg.contains("Priority"));
        assert!(msg.contains("8GB RAM"));
        assert!(msg.contains("HDD"));
    }

    #[test]
    fn test_upgrade_message_large() {
        let profile = make_profile(HardwareTier::Large, 64, true, false);
        let message = profile.upgrade_message();
        // Large tier has no suggestions
        assert!(message.is_none());
    }

    #[test]
    fn test_upgrade_message_format() {
        let profile = make_profile(HardwareTier::Tiny, 8, false, false);
        let message = profile.upgrade_message();
        assert!(message.is_some());

        let msg = message.unwrap();
        assert!(msg.contains("▸ Current:"));
        assert!(msg.contains("▸ Recommended:"));
        assert!(msg.contains("▸ Cost:"));
        assert!(msg.contains("▸ Gain:"));
    }

    #[test]
    fn test_upgrade_message_limits_to_three() {
        let profile = make_profile(HardwareTier::Ancient, 4, false, false);
        let message = profile.upgrade_message();
        assert!(message.is_some());

        let msg = message.unwrap();
        // Should have Priority 1, 2, 3 but not 4
        assert!(msg.contains("Priority 1:"));
        // May or may not have 2 and 3 depending on suggestions
    }

    #[test]
    fn test_upgrade_message_shows_ssd_status() {
        let profile_hdd = make_profile(HardwareTier::Small, 16, false, false);
        let msg_hdd = profile_hdd.upgrade_message().unwrap();
        assert!(msg_hdd.contains("HDD"));

        let profile_ssd = make_profile(HardwareTier::Small, 16, true, false);
        let msg_ssd = profile_ssd.upgrade_message().unwrap();
        assert!(msg_ssd.contains("SSD"));
    }
}
