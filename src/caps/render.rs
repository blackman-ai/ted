// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Cap prompt renderer.
//!
//! Adapts structured cap identity/policy into a compact system prompt.

use super::resolver::MergedCap;
use super::schema::{CapDeliverables, CapTraits};

/// Render the final system prompt from a resolved cap stack.
pub fn render_system_prompt(resolved: &MergedCap) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Ted is wearing: {}.", render_cap_stack(resolved)));

    if !resolved.resolved_identity.lenses.is_empty() {
        lines.push(format!(
            "Lenses: {}.",
            resolved.resolved_identity.lenses.join(", ")
        ));
    }

    let trait_summary = render_traits(&resolved.resolved_identity.traits);
    if !trait_summary.is_empty() {
        lines.push(format!("Behavior traits: {}.", trait_summary.join(", ")));
    }

    let deliverables = required_deliverables(&resolved.resolved_identity.deliverables);
    if !deliverables.is_empty() {
        lines.push(format!("Deliverables: {}.", deliverables.join(", ")));
    }

    lines.push(format!(
        "Tool safety summary: {}.",
        render_tool_safety_summary(resolved)
    ));

    let mut prompt = lines.join("\n");

    if let Some(legacy_text) = resolved
        .legacy_prompt_tail
        .as_ref()
        .map(|text| text.trim())
        .filter(|text| !text.is_empty())
    {
        prompt.push_str("\n\n[Legacy cap text]\n");
        prompt.push_str(legacy_text);
    }

    prompt
}

fn render_cap_stack(resolved: &MergedCap) -> String {
    if resolved.source_caps.is_empty() {
        "base".to_string()
    } else {
        format!("base, {}", resolved.source_caps.join(", "))
    }
}

fn render_trait_value(label: &str, value: f32) -> String {
    format!("{label}={value:.2}")
}

fn render_traits(traits: &CapTraits) -> Vec<String> {
    let mut parts = Vec::new();

    if let Some(value) = traits.verbosity {
        parts.push(render_trait_value("verbosity", value));
    }
    if let Some(value) = traits.cautiousness {
        parts.push(render_trait_value("cautiousness", value));
    }
    if let Some(value) = traits.pedantry {
        parts.push(render_trait_value("pedantry", value));
    }
    if let Some(value) = traits.planning_depth {
        parts.push(render_trait_value("planning_depth", value));
    }
    if let Some(value) = traits.evidence_threshold {
        parts.push(render_trait_value("evidence_threshold", value));
    }
    if let Some(value) = traits.refactor_bias {
        parts.push(render_trait_value("refactor_bias", value));
    }

    parts
}

fn required_deliverables(deliverables: &CapDeliverables) -> Vec<&'static str> {
    let mut required = Vec::new();

    if deliverables.require_diff_summary == Some(true) {
        required.push("diff summary");
    }
    if deliverables.require_test_plan == Some(true) {
        required.push("test plan");
    }
    if deliverables.require_risks == Some(true) {
        required.push("risks");
    }
    if deliverables.require_security_review == Some(true) {
        required.push("security review");
    }
    if deliverables.require_checklist == Some(true) {
        required.push("checklist");
    }

    required
}

fn render_tool_safety_summary(resolved: &MergedCap) -> String {
    let edit_confirmation = if resolved.tool_permissions.require_edit_confirmation {
        "edit confirmation required"
    } else {
        "edit confirmation optional"
    };

    let shell_confirmation = if resolved.tool_permissions.require_shell_confirmation {
        "shell confirmation required"
    } else {
        "shell confirmation optional"
    };

    let blocked_commands = if resolved.tool_permissions.blocked_commands.is_empty() {
        "no blocked commands configured".to_string()
    } else {
        let sample = resolved
            .tool_permissions
            .blocked_commands
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if resolved.tool_permissions.blocked_commands.len() > 3 {
            format!("blocked commands include {sample}, ...")
        } else {
            format!("blocked commands include {sample}")
        }
    };

    format!("{edit_confirmation}; {shell_confirmation}; {blocked_commands}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::resolver::ResolvedCapIdentity;
    use crate::caps::schema::CapToolPermissions;

    #[test]
    fn test_render_system_prompt_includes_identity_sections() {
        let resolved = MergedCap {
            source_caps: vec!["security-analyst".to_string(), "code-reviewer".to_string()],
            resolved_identity: ResolvedCapIdentity {
                traits: CapTraits {
                    cautiousness: Some(0.9),
                    planning_depth: Some(0.7),
                    ..Default::default()
                },
                lenses: vec!["security".to_string(), "review".to_string()],
                deliverables: CapDeliverables {
                    require_test_plan: Some(true),
                    require_security_review: Some(true),
                    ..Default::default()
                },
            },
            tool_permissions: CapToolPermissions {
                require_edit_confirmation: true,
                require_shell_confirmation: true,
                blocked_commands: vec!["curl".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };

        let rendered = render_system_prompt(&resolved);
        assert!(rendered.contains("Ted is wearing: base, security-analyst, code-reviewer."));
        assert!(rendered.contains("Lenses: security, review."));
        assert!(rendered.contains("Behavior traits: cautiousness=0.90, planning_depth=0.70."));
        assert!(rendered.contains("Deliverables: test plan, security review."));
        assert!(rendered.contains("Tool safety summary:"));
    }

    #[test]
    fn test_render_system_prompt_appends_legacy_text() {
        let resolved = MergedCap {
            legacy_prompt_tail: Some("Legacy prompt block".to_string()),
            ..Default::default()
        };
        let rendered = render_system_prompt(&resolved);

        assert!(rendered.contains("[Legacy cap text]"));
        assert!(rendered.contains("Legacy prompt block"));
    }

    #[test]
    fn test_render_system_prompt_omits_optional_sections_when_unset() {
        let resolved = MergedCap::default();
        let rendered = render_system_prompt(&resolved);

        assert!(rendered.contains("Ted is wearing: base."));
        assert!(!rendered.contains("Lenses:"));
        assert!(!rendered.contains("Behavior traits:"));
        assert!(!rendered.contains("Deliverables:"));
    }
}
