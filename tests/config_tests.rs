// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use ted::config::Settings;

#[test]
fn test_settings_default_values() {
    let settings = Settings::default();

    // Check default configuration values
    assert_eq!(
        settings.providers.anthropic.default_model,
        "claude-sonnet-4-20250514"
    );
    assert!((settings.defaults.temperature - 0.7).abs() < f32::EPSILON);
    assert_eq!(settings.defaults.max_tokens, 8192);
    assert!(settings.defaults.stream);
}

#[test]
fn test_settings_default_caps() {
    let settings = Settings::default();
    assert_eq!(settings.defaults.caps, vec!["base"]);
}

#[test]
fn test_settings_context_defaults() {
    let settings = Settings::default();
    assert_eq!(settings.context.max_warm_chunks, 100);
    assert_eq!(settings.context.cold_retention_days, 30);
    assert!(settings.context.auto_compact);
}

#[test]
fn test_settings_api_key_priority() {
    // Test that env var takes priority, but config key works when env not set
    // Use a custom env var name to avoid test pollution
    let mut settings = Settings::default();
    settings.providers.anthropic.api_key_env = "TED_TEST_API_KEY_12345".to_string();
    settings.providers.anthropic.api_key = Some("config-key".to_string());

    // Without env var, should use config key
    std::env::remove_var("TED_TEST_API_KEY_12345");
    assert_eq!(
        settings.get_anthropic_api_key(),
        Some("config-key".to_string())
    );

    // With env var set, should prefer env var
    std::env::set_var("TED_TEST_API_KEY_12345", "env-key");
    assert_eq!(
        settings.get_anthropic_api_key(),
        Some("env-key".to_string())
    );

    // Clean up
    std::env::remove_var("TED_TEST_API_KEY_12345");
}

#[test]
fn test_settings_serialization() {
    let settings = Settings::default();

    // Test JSON serialization
    let json = serde_json::to_string(&settings).expect("Should serialize to JSON");

    // Verify it contains expected fields
    assert!(json.contains("anthropic"));
    assert!(json.contains("default_model"));
    assert!(json.contains("temperature"));
}

#[test]
fn test_settings_deserialization() {
    let json = r#"{
        "providers": {
            "anthropic": {
                "api_key": null,
                "default_model": "claude-3-5-haiku-20241022"
            }
        },
        "defaults": {
            "provider": "anthropic",
            "caps": ["base", "rust-expert"],
            "temperature": 0.5,
            "max_tokens": 8192,
            "stream": false
        },
        "context": {
            "storage_path": "~/.ted/context",
            "max_warm_chunks": 50,
            "cold_retention_days": 14,
            "auto_compact": false
        }
    }"#;

    let settings: Settings = serde_json::from_str(json).expect("Should deserialize from JSON");

    assert_eq!(
        settings.providers.anthropic.default_model,
        "claude-3-5-haiku-20241022"
    );
    assert_eq!(settings.defaults.caps, vec!["base", "rust-expert"]);
    assert_eq!(settings.defaults.temperature, 0.5);
    assert_eq!(settings.defaults.max_tokens, 8192);
    assert!(!settings.defaults.stream);
    assert_eq!(settings.context.max_warm_chunks, 50);
    assert!(!settings.context.auto_compact);
}
