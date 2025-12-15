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
pub mod resolver;
pub mod schema;

pub use loader::CapLoader;
pub use resolver::CapResolver;
pub use schema::{Cap, CapToolPermissions};

use crate::error::Result;

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
}
