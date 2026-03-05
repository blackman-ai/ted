// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Cap resolver
//!
//! Handles resolving cap dependencies and merging multiple caps into
//! a single effective configuration.

use std::collections::HashSet;

use super::loader::CapLoader;
use super::schema::{
    Cap, CapDeliverables, CapIdentity, CapModelPreferences, CapToolPermissions, CapTraits,
};
use crate::error::Result;

const LEGACY_PROMPT_DELIMITER: &str = "\n\n---\n\n";

/// Resolver for loading and merging caps
pub struct CapResolver {
    loader: CapLoader,
}

impl CapResolver {
    /// Create a new resolver with the given loader
    pub fn new(loader: CapLoader) -> Self {
        Self { loader }
    }

    /// Resolve a list of cap names into loaded caps with dependencies
    /// Note: The "base" cap is always included silently to ensure good default behavior
    pub fn resolve(&self, names: &[String]) -> Result<Vec<Cap>> {
        let mut resolved: Vec<Cap> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // Always include base cap first (silently) for good default behavior
        // This ensures Ted always has a sensible system prompt
        self.resolve_one("base", &mut resolved, &mut seen)?;

        for name in names {
            self.resolve_one(name, &mut resolved, &mut seen)?;
        }

        // Sort by priority
        resolved.sort_by_key(|c| c.priority);

        Ok(resolved)
    }

    /// Resolve a single cap and its dependencies
    fn resolve_one(
        &self,
        name: &str,
        resolved: &mut Vec<Cap>,
        seen: &mut HashSet<String>,
    ) -> Result<()> {
        // Detect circular dependencies
        if seen.contains(name) {
            return Ok(()); // Already processed
        }

        seen.insert(name.to_string());

        // Load the cap
        let cap = self.loader.load(name)?;

        // First, resolve dependencies (parents)
        for parent_name in &cap.extends {
            self.resolve_one(parent_name, resolved, seen)?;
        }

        // Add this cap
        resolved.push(cap);

        Ok(())
    }

    /// Merge multiple caps into a single effective configuration
    pub fn merge(&self, caps: &[Cap]) -> MergedCap {
        let mut merged = MergedCap::default();
        let mut legacy_prompt_chunks = Vec::new();

        for cap in caps {
            // Treat legacy prompt text as an append-only tail section.
            let legacy_prompt = cap
                .legacy_system_prompt
                .as_deref()
                .unwrap_or(&cap.system_prompt)
                .trim();
            if !legacy_prompt.is_empty() {
                legacy_prompt_chunks.push(legacy_prompt.to_string());
            }

            if let Some(identity) = &cap.identity {
                merged_identity(&mut merged.resolved_identity, identity);
            }

            // Merge tool permissions
            merged.tool_permissions = merged.tool_permissions.merge(&cap.tool_permissions);

            // Later caps override model preferences
            if let Some(ref prefs) = cap.model {
                merged.model_preferences = Some(prefs.clone());
            }

            // Track source caps (excluding "base" which is always applied silently)
            if cap.name != "base" {
                merged.source_caps.push(cap.name.clone());
            }
        }

        if !legacy_prompt_chunks.is_empty() {
            merged.legacy_prompt_tail = Some(legacy_prompt_chunks.join(LEGACY_PROMPT_DELIMITER));
        }

        merged
    }

    /// Resolve and merge caps in one step
    pub fn resolve_and_merge(&self, names: &[String]) -> Result<MergedCap> {
        let caps = self.resolve(names)?;
        Ok(self.merge(&caps))
    }
}

fn merged_identity(target: &mut ResolvedCapIdentity, incoming: &CapIdentity) {
    if let Some(incoming_traits) = incoming.traits.as_ref() {
        merge_traits(&mut target.traits, incoming_traits);
    }

    for lens in &incoming.lenses {
        let normalized_lens = lens.trim();
        if normalized_lens.is_empty() {
            continue;
        }

        if !target
            .lenses
            .iter()
            .any(|existing| existing == normalized_lens)
        {
            target.lenses.push(normalized_lens.to_string());
        }
    }

    if let Some(incoming_deliverables) = incoming.deliverables.as_ref() {
        merge_deliverables(&mut target.deliverables, incoming_deliverables);
    }
}

fn clamp_trait_value(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn merge_trait_field(target: &mut Option<f32>, incoming: Option<f32>) {
    if let Some(value) = incoming {
        *target = Some(clamp_trait_value(value));
    }
}

fn merge_traits(target: &mut CapTraits, incoming: &CapTraits) {
    merge_trait_field(&mut target.verbosity, incoming.verbosity);
    merge_trait_field(&mut target.cautiousness, incoming.cautiousness);
    merge_trait_field(&mut target.pedantry, incoming.pedantry);
    merge_trait_field(&mut target.planning_depth, incoming.planning_depth);
    merge_trait_field(&mut target.evidence_threshold, incoming.evidence_threshold);
    merge_trait_field(&mut target.refactor_bias, incoming.refactor_bias);
}

fn merge_deliverable_field(target: &mut Option<bool>, incoming: Option<bool>) {
    if let Some(value) = incoming {
        *target = Some(value);
    }
}

fn merge_deliverables(target: &mut CapDeliverables, incoming: &CapDeliverables) {
    merge_deliverable_field(
        &mut target.require_diff_summary,
        incoming.require_diff_summary,
    );
    merge_deliverable_field(&mut target.require_test_plan, incoming.require_test_plan);
    merge_deliverable_field(&mut target.require_risks, incoming.require_risks);
    merge_deliverable_field(
        &mut target.require_security_review,
        incoming.require_security_review,
    );
    merge_deliverable_field(&mut target.require_checklist, incoming.require_checklist);
}

/// Resolved identity/policy stack after cap merging.
#[derive(Debug, Clone, Default)]
pub struct ResolvedCapIdentity {
    /// Merged behavior traits (last explicit value wins).
    pub traits: CapTraits,
    /// Merged focus lenses (stable-order unique list).
    pub lenses: Vec<String>,
    /// Merged deliverable requirements (last explicit value wins).
    pub deliverables: CapDeliverables,
}

/// The result of merging multiple caps
#[derive(Debug, Clone, Default)]
pub struct MergedCap {
    /// Merged structured identity and policy
    pub resolved_identity: ResolvedCapIdentity,
    /// Merged tool permissions
    pub tool_permissions: CapToolPermissions,
    /// Model preferences (from last cap with preferences)
    pub model_preferences: Option<CapModelPreferences>,
    /// Joined legacy prompt text from source caps
    pub legacy_prompt_tail: Option<String>,
    /// Names of caps that were merged (in order)
    pub source_caps: Vec<String>,
}

impl MergedCap {
    /// Check if a tool is enabled after all merging
    pub fn is_tool_enabled(&self, tool_name: &str) -> bool {
        self.tool_permissions.is_tool_enabled(tool_name)
    }

    /// Get the preferred model, if any
    pub fn preferred_model(&self) -> Option<&str> {
        self.model_preferences
            .as_ref()
            .and_then(|p| p.preferred_model.as_deref())
    }

    /// Get the temperature override, if any
    pub fn temperature(&self) -> Option<f32> {
        self.model_preferences.as_ref().and_then(|p| p.temperature)
    }

    /// Get the max tokens override, if any
    pub fn max_tokens(&self) -> Option<u32> {
        self.model_preferences.as_ref().and_then(|p| p.max_tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cap_resolver_new() {
        let loader = CapLoader::new();
        let _resolver = CapResolver::new(loader);
    }

    #[test]
    fn test_resolve_base() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);

        let caps = resolver.resolve(&["base".to_string()]).unwrap();
        assert!(!caps.is_empty());
        assert_eq!(caps[0].name, "base");
    }

    #[test]
    fn test_resolve_empty() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);

        // Even with empty input, "base" is always included silently
        let caps = resolver.resolve(&[]).unwrap();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].name, "base");
    }

    #[test]
    fn test_resolve_multiple_caps() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);

        let caps = resolver
            .resolve(&["base".to_string(), "rust-expert".to_string()])
            .unwrap();

        assert!(caps.len() >= 2);
        let names: Vec<_> = caps.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"base"));
        assert!(names.contains(&"rust-expert"));
    }

    #[test]
    fn test_resolve_deduplicates() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);

        // Requesting the same cap twice should not duplicate
        let caps = resolver
            .resolve(&["base".to_string(), "base".to_string()])
            .unwrap();

        let base_count = caps.iter().filter(|c| c.name == "base").count();
        assert_eq!(base_count, 1);
    }

    #[test]
    fn test_resolve_preserves_priority_sorting() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);

        let caps = resolver
            .resolve(&["security-analyst".to_string(), "code-reviewer".to_string()])
            .unwrap();
        let priorities: Vec<i32> = caps.iter().map(|cap| cap.priority).collect();
        let mut sorted = priorities.clone();
        sorted.sort();

        assert_eq!(priorities, sorted);
    }

    #[test]
    fn test_merge_legacy_prompt_tail() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);

        let caps = resolver
            .resolve(&["base".to_string(), "rust-expert".to_string()])
            .unwrap();
        let merged = resolver.merge(&caps);
        let legacy = merged.legacy_prompt_tail.unwrap();

        assert!(legacy.contains("concise")); // From base
        assert!(legacy.contains("Rust")); // From rust-expert
    }

    #[test]
    fn test_merge_empty_caps() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);

        let merged = resolver.merge(&[]);
        assert!(merged.legacy_prompt_tail.is_none());
        assert!(merged.source_caps.is_empty());
        assert!(merged.model_preferences.is_none());
        assert!(merged.resolved_identity.lenses.is_empty());
    }

    #[test]
    fn test_merge_tracks_source_caps() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);

        let cap1 = Cap::new("test1").with_system_prompt("Prompt 1");
        let cap2 = Cap::new("test2").with_system_prompt("Prompt 2");

        let merged = resolver.merge(&[cap1, cap2]);
        assert_eq!(merged.source_caps, vec!["test1", "test2"]);
    }

    #[test]
    fn test_resolve_with_extends() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);

        // rust-expert extends base
        let caps = resolver.resolve(&["rust-expert".to_string()]).unwrap();

        // Should include base first, then rust-expert
        let names: Vec<_> = caps.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"base"));
        assert!(names.contains(&"rust-expert"));
    }

    #[test]
    fn test_identity_merge_deterministic() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);

        let cap1 = Cap {
            name: "first".to_string(),
            identity: Some(CapIdentity {
                traits: Some(CapTraits {
                    verbosity: Some(0.2),
                    cautiousness: Some(0.4),
                    ..Default::default()
                }),
                lenses: vec!["security".to_string(), "review".to_string()],
                deliverables: Some(CapDeliverables {
                    require_test_plan: Some(true),
                    require_risks: Some(true),
                    ..Default::default()
                }),
            }),
            ..Cap::new("first")
        };
        let cap2 = Cap {
            name: "second".to_string(),
            identity: Some(CapIdentity {
                traits: Some(CapTraits {
                    verbosity: Some(0.8),
                    planning_depth: Some(0.9),
                    ..Default::default()
                }),
                lenses: vec!["review".to_string(), "performance".to_string()],
                deliverables: Some(CapDeliverables {
                    require_test_plan: Some(false),
                    require_security_review: Some(true),
                    ..Default::default()
                }),
            }),
            ..Cap::new("second")
        };

        let merged = resolver.merge(&[cap1, cap2]);
        assert_eq!(merged.resolved_identity.traits.verbosity, Some(0.8));
        assert_eq!(merged.resolved_identity.traits.cautiousness, Some(0.4));
        assert_eq!(merged.resolved_identity.traits.planning_depth, Some(0.9));
        assert_eq!(
            merged.resolved_identity.lenses,
            vec![
                "security".to_string(),
                "review".to_string(),
                "performance".to_string()
            ]
        );
        assert_eq!(
            merged.resolved_identity.deliverables.require_test_plan,
            Some(false)
        );
        assert_eq!(
            merged.resolved_identity.deliverables.require_risks,
            Some(true)
        );
        assert_eq!(
            merged
                .resolved_identity
                .deliverables
                .require_security_review,
            Some(true)
        );
    }

    #[test]
    fn test_identity_trait_values_are_clamped() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);

        let cap1 = Cap {
            name: "first".to_string(),
            identity: Some(CapIdentity {
                traits: Some(CapTraits {
                    verbosity: Some(4.2),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Cap::new("first")
        };
        let cap2 = Cap {
            name: "second".to_string(),
            identity: Some(CapIdentity {
                traits: Some(CapTraits {
                    verbosity: Some(-3.0),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Cap::new("second")
        };

        let merged = resolver.merge(&[cap1, cap2]);
        assert_eq!(merged.resolved_identity.traits.verbosity, Some(0.0));
    }

    #[test]
    fn test_resolve_and_merge() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);

        // When resolving just "base", source_caps will be empty because
        // "base" is hidden from the list (always applied silently)
        let merged = resolver.resolve_and_merge(&["base".to_string()]).unwrap();
        assert!(merged.legacy_prompt_tail.is_some());
        // source_caps excludes "base"
        assert!(merged.source_caps.is_empty());

        // When resolving a non-base cap, source_caps will have that cap
        let merged2 = resolver
            .resolve_and_merge(&["rust-expert".to_string()])
            .unwrap();
        assert!(merged2.legacy_prompt_tail.is_some());
        assert!(merged2.source_caps.contains(&"rust-expert".to_string()));
    }

    #[test]
    fn test_merged_cap_default() {
        let merged = MergedCap::default();
        assert!(merged.legacy_prompt_tail.is_none());
        assert!(merged.source_caps.is_empty());
        assert!(merged.model_preferences.is_none());
        assert!(merged.resolved_identity.lenses.is_empty());
    }

    #[test]
    fn test_merged_cap_is_tool_enabled() {
        let merged = MergedCap::default();
        // Default permissions should enable all tools
        assert!(merged.is_tool_enabled("file_read"));
        assert!(merged.is_tool_enabled("shell"));
    }

    #[test]
    fn test_merged_cap_is_tool_enabled_with_disable() {
        let mut merged = MergedCap::default();
        merged.tool_permissions.disable.push("shell".to_string());

        assert!(merged.is_tool_enabled("file_read"));
        assert!(!merged.is_tool_enabled("shell"));
    }

    #[test]
    fn test_merged_cap_preferred_model_none() {
        let merged = MergedCap::default();
        assert!(merged.preferred_model().is_none());
    }

    #[test]
    fn test_merged_cap_preferred_model_some() {
        let merged = MergedCap {
            model_preferences: Some(CapModelPreferences {
                preferred_model: Some("claude-opus-4-5-20250514".to_string()),
                temperature: None,
                max_tokens: None,
            }),
            ..Default::default()
        };

        assert_eq!(merged.preferred_model(), Some("claude-opus-4-5-20250514"));
    }

    #[test]
    fn test_merged_cap_temperature_none() {
        let merged = MergedCap::default();
        assert!(merged.temperature().is_none());
    }

    #[test]
    fn test_merged_cap_temperature_some() {
        let merged = MergedCap {
            model_preferences: Some(CapModelPreferences {
                preferred_model: None,
                temperature: Some(0.7),
                max_tokens: None,
            }),
            ..Default::default()
        };

        assert_eq!(merged.temperature(), Some(0.7));
    }

    #[test]
    fn test_merged_cap_max_tokens_none() {
        let merged = MergedCap::default();
        assert!(merged.max_tokens().is_none());
    }

    #[test]
    fn test_merged_cap_max_tokens_some() {
        let merged = MergedCap {
            model_preferences: Some(CapModelPreferences {
                preferred_model: None,
                temperature: None,
                max_tokens: Some(4096),
            }),
            ..Default::default()
        };

        assert_eq!(merged.max_tokens(), Some(4096));
    }

    #[test]
    fn test_merged_cap_debug_and_clone() {
        let merged = MergedCap {
            legacy_prompt_tail: Some("Test prompt".to_string()),
            tool_permissions: CapToolPermissions::default(),
            model_preferences: None,
            source_caps: vec!["base".to_string()],
            ..Default::default()
        };

        let debug_str = format!("{:?}", merged);
        assert!(debug_str.contains("Test prompt"));

        let cloned = merged.clone();
        assert_eq!(cloned.legacy_prompt_tail, Some("Test prompt".to_string()));
        assert_eq!(cloned.source_caps, vec!["base"]);
    }

    #[test]
    fn test_merge_model_preferences_override() {
        let loader = CapLoader::new();
        let resolver = CapResolver::new(loader);

        let cap1 = Cap {
            name: "first".to_string(),
            model: Some(CapModelPreferences {
                preferred_model: Some("model-1".to_string()),
                temperature: Some(0.5),
                max_tokens: Some(1000),
            }),
            ..Cap::new("first")
        };

        let cap2 = Cap {
            name: "second".to_string(),
            model: Some(CapModelPreferences {
                preferred_model: Some("model-2".to_string()),
                temperature: Some(0.9),
                max_tokens: Some(2000),
            }),
            ..Cap::new("second")
        };

        let merged = resolver.merge(&[cap1, cap2]);
        // Later cap should override model preferences
        assert_eq!(merged.preferred_model(), Some("model-2"));
        assert_eq!(merged.temperature(), Some(0.9));
        assert_eq!(merged.max_tokens(), Some(2000));
    }
}
