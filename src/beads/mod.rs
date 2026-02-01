// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Beads task tracking system
//!
//! Beads provide git-friendly task tracking for long-horizon work.
//! Each bead represents a discrete unit of work with dependencies,
//! status tracking, and persistence in JSONL format.
//!
//! ## Features
//!
//! - Hash-based IDs for stable references
//! - Hierarchical sub-tasks (bd-{hash}.1, bd-{hash}.1.1, etc.)
//! - Dependency tracking and auto-ready detection
//! - JSONL storage for git-friendly diffs
//! - Status workflow: Pending → Ready → InProgress → Done/Blocked/Cancelled
//!
//! ## Usage
//!
//! ```rust,ignore
//! use ted::beads::{init_beads, Bead, BeadStatus, BeadPriority};
//! use std::path::Path;
//!
//! // Initialize beads storage
//! let store = init_beads(Path::new("/project"))?;
//!
//! // Create a new bead
//! let bead = Bead::new("Add user authentication", "Implement OAuth2 login flow")
//!     .with_priority(BeadPriority::High)
//!     .with_tags(vec!["feature".to_string(), "auth".to_string()]);
//!
//! let id = store.create(bead)?;
//!
//! // Update status
//! store.set_status(&id, BeadStatus::InProgress)?;
//!
//! // Get ready tasks
//! let ready = store.ready();
//!
//! // Get statistics
//! let stats = store.stats();
//! println!("Progress: {:.1}% complete", stats.completion_percentage());
//! ```
//!
//! ## Storage Format
//!
//! Beads are stored in `.beads/beads.jsonl`:
//!
//! ```json
//! {"timestamp":"2024-01-01T00:00:00Z","operation":"create","bead":{...}}
//! {"timestamp":"2024-01-01T01:00:00Z","operation":"update","bead":{...}}
//! ```
//!
//! This append-only format preserves history and works well with git.

pub mod schema;
pub mod storage;

// Re-export commonly used types
pub use schema::{Bead, BeadId, BeadLogEntry, BeadNote, BeadOperation, BeadPriority, BeadStatus};
pub use storage::{init_beads, BeadStats, BeadStore};

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_module_exports() {
        // Verify types are exported correctly
        let _ = BeadId::from_title("Test");
        let _ = BeadStatus::Pending;
        let _ = BeadPriority::High;
    }

    #[test]
    fn test_bead_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let store = init_beads(temp_dir.path()).unwrap();

        // Create a bead
        let bead = Bead::new("Implement feature", "Full description here")
            .with_priority(BeadPriority::High)
            .with_tags(vec!["feature".to_string()]);

        let id = store.create(bead).unwrap();

        // Update to ready
        store.set_status(&id, BeadStatus::Ready).unwrap();

        // Verify
        let retrieved = store.get(&id).unwrap();
        assert!(matches!(retrieved.status, BeadStatus::Ready));
        assert_eq!(retrieved.priority, BeadPriority::High);
    }

    #[test]
    fn test_bead_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        let store = init_beads(temp_dir.path()).unwrap();

        // Create dependency (and mark it in-progress so it's not actionable)
        let mut dep = Bead::new("Setup database", "Create tables");
        dep.set_status(BeadStatus::InProgress);
        let dep_id = store.create(dep).unwrap();

        // Create dependent task
        let task =
            Bead::new("Add users", "User CRUD operations").with_depends_on(vec![dep_id.clone()]);
        store.create(task).unwrap();

        // Task shouldn't be actionable yet (dependency not done)
        let actionable = store.get_actionable();
        assert!(actionable.is_empty());

        // Complete dependency
        store.set_status(&dep_id, BeadStatus::Done).unwrap();

        // Now task should be actionable
        let actionable = store.get_actionable();
        assert_eq!(actionable.len(), 1);
        assert_eq!(actionable[0].title, "Add users");
    }

    #[test]
    fn test_bead_hierarchy() {
        let parent = Bead::new("Large feature", "Multi-step implementation");
        let parent_id = parent.id.clone();

        let child1 = parent.create_child(1, "Step 1", "First step");
        let child2 = parent.create_child(2, "Step 2", "Second step");

        assert_eq!(child1.id, parent_id.child(1));
        assert_eq!(child2.id, parent_id.child(2));
        assert_eq!(child1.id.depth(), 1);
    }
}
