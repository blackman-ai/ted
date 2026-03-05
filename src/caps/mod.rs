// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Caps system for stackable personas
//!
//! Caps are stackable persona/prompt layers that can be loaded from TOML files.
//! They define system prompts, tool permissions, and other configuration.
//!
//! Search paths (in priority order):
//! 1. `./.ted/caps/` - Project-local caps
//! 2. `~/.ted/caps/` - User-global caps
//! 3. Built-in caps (embedded in binary)

pub mod builtin;
pub mod loader;
pub mod render;
pub mod resolver;
pub mod schema;

pub use loader::CapLoader;
pub use resolver::CapResolver;
pub use schema::{Cap, CapToolPermissions};

use crate::error::{Result, TedError};

/// Load and resolve caps by name
pub fn load_caps(names: &[String]) -> Result<Vec<Cap>> {
    let loader = CapLoader::new();
    let resolver = CapResolver::new(loader);
    resolver.resolve(names)
}

/// Get all available cap names with builtin status
pub fn available_caps() -> Result<Vec<(String, bool)>> {
    let loader = CapLoader::new();
    loader.list_available()
}

/// Apply organization cap governance rules to a cap stack.
///
/// When `enforce_policy` is false, this is a no-op.
/// When enabled:
/// - disallowed caps are rejected
/// - required caps are appended if missing
/// - final cap list is de-duplicated with stable ordering
pub fn enforce_governance(
    cap_names: &mut Vec<String>,
    enforce_policy: bool,
    required_caps: &[String],
    disallowed_caps: &[String],
) -> Result<()> {
    if !enforce_policy {
        return Ok(());
    }

    let disallowed: std::collections::HashSet<String> = disallowed_caps.iter().cloned().collect();

    if let Some(conflict) = required_caps.iter().find(|cap| disallowed.contains(*cap)) {
        return Err(TedError::Config(format!(
            "Invalid governance settings: required cap '{}' is also disallowed",
            conflict
        )));
    }

    if let Some(conflict) = cap_names.iter().find(|cap| disallowed.contains(*cap)) {
        return Err(TedError::InvalidInput(format!(
            "Cap '{}' is disallowed by governance policy",
            conflict
        )));
    }

    for required in required_caps {
        if !cap_names.iter().any(|cap| cap == required) {
            cap_names.push(required.clone());
        }
    }

    let mut seen = std::collections::HashSet::new();
    cap_names.retain(|cap| seen.insert(cap.clone()));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_caps_empty() {
        // Even with empty input, "base" is always included silently
        let caps = load_caps(&[]).unwrap();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].name, "base");
    }

    #[test]
    fn test_load_caps_base() {
        let caps = load_caps(&["base".to_string()]).unwrap();
        assert!(!caps.is_empty());
        assert!(caps.iter().any(|c| c.name == "base"));
    }

    #[test]
    fn test_load_caps_with_extends() {
        // rust-expert extends base, so both should be loaded
        let caps = load_caps(&["rust-expert".to_string()]).unwrap();
        assert!(caps.len() >= 2);
        assert!(caps.iter().any(|c| c.name == "base"));
        assert!(caps.iter().any(|c| c.name == "rust-expert"));
    }

    #[test]
    fn test_load_caps_multiple() {
        let caps = load_caps(&["base".to_string(), "rust-expert".to_string()]).unwrap();
        assert!(caps.iter().any(|c| c.name == "base"));
        assert!(caps.iter().any(|c| c.name == "rust-expert"));
    }

    #[test]
    fn test_load_caps_nonexistent() {
        let result = load_caps(&["nonexistent-cap-12345".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_available_caps() {
        let caps = available_caps().unwrap();
        assert!(!caps.is_empty());

        // Should include known builtins (but NOT "base" which is hidden)
        let names: Vec<_> = caps.iter().map(|(name, _)| name.as_str()).collect();
        assert!(!names.contains(&"base")); // base is always applied silently
        assert!(names.contains(&"rust-expert"));
    }

    #[test]
    fn test_available_caps_includes_builtins() {
        let caps = available_caps().unwrap();

        // Filter to just builtins
        let builtins: Vec<_> = caps.iter().filter(|(_, is_builtin)| *is_builtin).collect();

        assert!(!builtins.is_empty());
    }

    #[test]
    fn test_cap_loader_reexport() {
        // Verify re-exports work
        let loader = CapLoader::new();
        assert!(loader.exists("base"));
    }

    #[test]
    fn test_cap_resolver_reexport() {
        // Verify re-exports work
        let loader = CapLoader::new();
        let _resolver = CapResolver::new(loader);
    }

    #[test]
    fn test_cap_reexport() {
        // Verify Cap struct is re-exported
        let cap = Cap::new("test");
        assert_eq!(cap.name, "test");
    }

    #[test]
    fn test_cap_tool_permissions_reexport() {
        // Verify CapToolPermissions is re-exported
        let perms = CapToolPermissions::default();
        assert!(perms.enable.is_empty());
    }

    #[test]
    fn test_enforce_governance_disabled_is_noop() {
        let mut caps = vec!["code-reviewer".to_string()];
        enforce_governance(
            &mut caps,
            false,
            &["security-analyst".to_string()],
            &["code-reviewer".to_string()],
        )
        .expect("governance should be disabled");

        assert_eq!(caps, vec!["code-reviewer".to_string()]);
    }

    #[test]
    fn test_enforce_governance_adds_required_and_dedupes() {
        let mut caps = vec![
            "code-reviewer".to_string(),
            "code-reviewer".to_string(),
            "documentation".to_string(),
        ];
        enforce_governance(&mut caps, true, &["security-analyst".to_string()], &[])
            .expect("governance should succeed");

        assert_eq!(
            caps,
            vec![
                "code-reviewer".to_string(),
                "documentation".to_string(),
                "security-analyst".to_string(),
            ]
        );
    }

    #[test]
    fn test_enforce_governance_rejects_disallowed_active_cap() {
        let mut caps = vec!["security-analyst".to_string()];
        let err = enforce_governance(&mut caps, true, &[], &["security-analyst".to_string()])
            .expect_err("disallowed cap should fail");
        assert!(err.to_string().contains("disallowed by governance policy"));
    }

    #[test]
    fn test_enforce_governance_rejects_conflicting_required_and_disallowed() {
        let mut caps = vec!["base".to_string()];
        let err = enforce_governance(
            &mut caps,
            true,
            &["security-analyst".to_string()],
            &["security-analyst".to_string()],
        )
        .expect_err("conflicting governance should fail");
        assert!(err
            .to_string()
            .contains("required cap 'security-analyst' is also disallowed"));
    }
}
