// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Lightweight thermal monitoring helpers for adaptive runtime safeguards.

#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "macos")]
use std::process::Command;

use super::SystemProfile;

/// Coarse thermal state derived from available system telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalLevel {
    Cool,
    Warm,
    Hot,
    Critical,
}

/// Runtime thermal status sample.
#[derive(Debug, Clone, PartialEq)]
pub struct ThermalStatus {
    pub temperature_c: Option<f32>,
    pub cpu_speed_limit_percent: Option<u8>,
    pub level: ThermalLevel,
    pub source: &'static str,
}

impl ThermalStatus {
    /// Whether runtime guardrails should reduce load.
    pub fn needs_throttle(&self) -> bool {
        matches!(self.level, ThermalLevel::Hot | ThermalLevel::Critical)
    }
}

/// Sample thermal status when the detected profile indicates thermal risk monitoring.
pub fn sample_thermal_status(profile: &SystemProfile) -> Option<ThermalStatus> {
    if !profile.tier.monitor_thermal() && !profile.thermal_throttle_risk() {
        return None;
    }

    #[cfg(target_os = "linux")]
    if let Some(status) = read_linux_thermal_status() {
        return Some(status);
    }

    #[cfg(target_os = "macos")]
    if let Some(status) = read_macos_thermal_status() {
        return Some(status);
    }

    None
}

#[cfg_attr(not(test), allow(dead_code))]
fn classify_temperature_c(temp: f32) -> ThermalLevel {
    if temp >= 85.0 {
        ThermalLevel::Critical
    } else if temp >= 75.0 {
        ThermalLevel::Hot
    } else if temp >= 65.0 {
        ThermalLevel::Warm
    } else {
        ThermalLevel::Cool
    }
}

#[cfg(target_os = "linux")]
fn read_linux_thermal_status() -> Option<ThermalStatus> {
    let raw = fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").ok()?;
    let temperature_c = parse_linux_thermal_zone_temp(&raw)?;
    Some(ThermalStatus {
        temperature_c: Some(temperature_c),
        cpu_speed_limit_percent: None,
        level: classify_temperature_c(temperature_c),
        source: "sysfs",
    })
}

#[cfg(target_os = "macos")]
fn read_macos_thermal_status() -> Option<ThermalStatus> {
    let output = Command::new("pmset").args(["-g", "therm"]).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let speed_limit = parse_macos_cpu_speed_limit(&stdout)?;
    let level = if speed_limit < 70 {
        ThermalLevel::Critical
    } else if speed_limit < 85 {
        ThermalLevel::Hot
    } else if speed_limit < 100 {
        ThermalLevel::Warm
    } else {
        ThermalLevel::Cool
    };

    Some(ThermalStatus {
        temperature_c: None,
        cpu_speed_limit_percent: Some(speed_limit),
        level,
        source: "pmset",
    })
}

#[cfg_attr(not(test), allow(dead_code))]
fn parse_linux_thermal_zone_temp(raw: &str) -> Option<f32> {
    let value: f32 = raw.trim().parse().ok()?;
    if value > 1000.0 {
        Some(value / 1000.0)
    } else {
        Some(value)
    }
}

fn parse_macos_cpu_speed_limit(output: &str) -> Option<u8> {
    output.lines().find_map(|line| {
        if !line.contains("CPU_Speed_Limit") {
            return None;
        }
        let value = line.split('=').nth(1)?.trim();
        value.parse::<u8>().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_linux_thermal_zone_temp_millicelsius() {
        let parsed = parse_linux_thermal_zone_temp("65000\n").unwrap();
        assert!((parsed - 65.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_linux_thermal_zone_temp_celsius() {
        let parsed = parse_linux_thermal_zone_temp("63.5").unwrap();
        assert!((parsed - 63.5).abs() < 0.01);
    }

    #[test]
    fn test_parse_macos_cpu_speed_limit() {
        let output = r#"
CPU_Scheduler_Limit = 100
CPU_Available_CPUs = 8
CPU_Speed_Limit = 86
"#;
        assert_eq!(parse_macos_cpu_speed_limit(output), Some(86));
    }

    #[test]
    fn test_classify_temperature_c() {
        assert_eq!(classify_temperature_c(50.0), ThermalLevel::Cool);
        assert_eq!(classify_temperature_c(67.0), ThermalLevel::Warm);
        assert_eq!(classify_temperature_c(78.0), ThermalLevel::Hot);
        assert_eq!(classify_temperature_c(90.0), ThermalLevel::Critical);
    }

    #[test]
    fn test_needs_throttle() {
        let hot = ThermalStatus {
            temperature_c: Some(80.0),
            cpu_speed_limit_percent: None,
            level: ThermalLevel::Hot,
            source: "test",
        };
        assert!(hot.needs_throttle());

        let cool = ThermalStatus {
            temperature_c: Some(55.0),
            cpu_speed_limit_percent: None,
            level: ThermalLevel::Cool,
            source: "test",
        };
        assert!(!cool.needs_throttle());
    }
}
