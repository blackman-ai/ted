// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Plan management for Ted
//!
//! Plans are living documents that Ted creates and manages during conversations.
//! They track implementation progress, tasks, and provide context for resuming work.
//!
//! ## Storage
//! - Plans are stored in `~/.ted/plans/`
//! - `index.json` contains plan metadata
//! - Individual plans stored as `{uuid}.md` with YAML frontmatter
//!
//! ## Usage
//! - Ted creates plans automatically for complex tasks
//! - Plans update as work progresses (tasks completed, notes added)
//! - TUI browser allows switching between plans like sessions

pub mod parser;
pub mod store;

pub use parser::{parse_plan, serialize_plan, PlanTask};
pub use store::{Plan, PlanInfo, PlanStatus, PlanStore};

use crate::config::Settings;
use crate::error::Result;
use std::path::PathBuf;
use uuid::Uuid;

/// Get the plans directory (~/.ted/plans/)
pub fn plans_dir() -> PathBuf {
    Settings::ted_home().join("plans")
}

/// Get the plans index path (~/.ted/plans/index.json)
pub fn plans_index_path() -> PathBuf {
    plans_dir().join("index.json")
}

/// Get the path for a specific plan file
pub fn plan_file_path(id: Uuid) -> PathBuf {
    plans_dir().join(format!("{}.md", id))
}

/// Ensure the plans directory exists
pub fn ensure_plans_dir() -> Result<()> {
    let dir = plans_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plans_dir() {
        let dir = plans_dir();
        assert!(dir.ends_with("plans"));
    }

    #[test]
    fn test_plans_index_path() {
        let path = plans_index_path();
        assert!(path.ends_with("index.json"));
    }

    #[test]
    fn test_plan_file_path() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let path = plan_file_path(id);
        assert!(path
            .to_string_lossy()
            .contains("550e8400-e29b-41d4-a716-446655440000.md"));
    }
}
