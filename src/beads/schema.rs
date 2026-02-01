// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Bead schema definitions
//!
//! Beads are task tracking units for long-horizon work. Each bead represents
//! a discrete unit of work with dependencies, status, and tracking metadata.
//!
//! ## Bead ID Format
//!
//! Bead IDs follow a hierarchical format:
//! - `bd-{hash}` - Root bead
//! - `bd-{hash}.1` - First sub-bead
//! - `bd-{hash}.1.1` - Nested sub-bead
//!
//! The hash is derived from the bead's title for stable identification.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use uuid::Uuid;

/// A bead ID with hierarchical structure
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BeadId(pub String);

impl BeadId {
    /// Create a new root bead ID from a title
    pub fn from_title(title: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        title.hash(&mut hasher);
        let hash = hasher.finish();
        let short_hash = format!("{:08x}", hash as u32); // 8 hex chars
        Self(format!("bd-{}", short_hash))
    }

    /// Create a new random bead ID
    pub fn random() -> Self {
        let uuid = Uuid::new_v4();
        let short = &uuid.to_string()[..8];
        Self(format!("bd-{}", short))
    }

    /// Create a sub-bead ID
    pub fn child(&self, index: u32) -> Self {
        Self(format!("{}.{}", self.0, index))
    }

    /// Get the parent ID (if this is a sub-bead)
    pub fn parent(&self) -> Option<Self> {
        let last_dot = self.0.rfind('.')?;
        Some(Self(self.0[..last_dot].to_string()))
    }

    /// Check if this is a root bead
    pub fn is_root(&self) -> bool {
        !self.0.contains('.')
    }

    /// Get the depth (root = 0)
    pub fn depth(&self) -> usize {
        self.0.matches('.').count()
    }

    /// Get as string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BeadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for BeadId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for BeadId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Status of a bead
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeadStatus {
    /// Task is defined but not yet actionable
    Pending,
    /// All dependencies are satisfied, ready to work on
    Ready,
    /// Currently being worked on
    InProgress,
    /// Blocked by something (with reason)
    Blocked { reason: String },
    /// Completed successfully
    Done,
    /// Cancelled (with reason)
    Cancelled { reason: String },
}

impl BeadStatus {
    /// Check if the bead is complete (done or cancelled)
    pub fn is_terminal(&self) -> bool {
        matches!(self, BeadStatus::Done | BeadStatus::Cancelled { .. })
    }

    /// Check if the bead can be worked on
    pub fn is_actionable(&self) -> bool {
        matches!(self, BeadStatus::Ready | BeadStatus::InProgress)
    }
}

impl Default for BeadStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Priority level for a bead
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeadPriority {
    Low,
    Medium,
    High,
    Critical,
}

impl Default for BeadPriority {
    fn default() -> Self {
        Self::Medium
    }
}

/// A bead represents a unit of work to track
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bead {
    /// Unique identifier
    pub id: BeadId,
    /// Short title
    pub title: String,
    /// Detailed description
    pub description: String,
    /// Current status
    pub status: BeadStatus,
    /// Priority level
    pub priority: BeadPriority,
    /// Beads this depends on (must be done first)
    pub depends_on: Vec<BeadId>,
    /// When the bead was created
    pub created_at: DateTime<Utc>,
    /// When the bead was last updated
    pub updated_at: DateTime<Utc>,
    /// When the bead was completed (if done)
    pub completed_at: Option<DateTime<Utc>>,
    /// Agent ID currently working on this (if in_progress)
    pub agent_id: Option<Uuid>,
    /// Files affected by this bead
    pub files_affected: Vec<PathBuf>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Compacted summary of work done
    pub compacted_summary: Option<String>,
    /// Notes and updates
    pub notes: Vec<BeadNote>,
}

/// A note or update on a bead
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeadNote {
    /// When the note was added
    pub timestamp: DateTime<Utc>,
    /// The note content
    pub content: String,
    /// Who added the note (agent name or "user")
    pub author: String,
}

impl Bead {
    /// Create a new bead
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        let title = title.into();
        let id = BeadId::from_title(&title);
        let now = Utc::now();

        Self {
            id,
            title,
            description: description.into(),
            status: BeadStatus::Pending,
            priority: BeadPriority::default(),
            depends_on: Vec::new(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            agent_id: None,
            files_affected: Vec::new(),
            tags: Vec::new(),
            compacted_summary: None,
            notes: Vec::new(),
        }
    }

    /// Create a bead with a specific ID
    pub fn with_id(id: BeadId, title: impl Into<String>, description: impl Into<String>) -> Self {
        let now = Utc::now();

        Self {
            id,
            title: title.into(),
            description: description.into(),
            status: BeadStatus::Pending,
            priority: BeadPriority::default(),
            depends_on: Vec::new(),
            created_at: now,
            updated_at: now,
            completed_at: None,
            agent_id: None,
            files_affected: Vec::new(),
            tags: Vec::new(),
            compacted_summary: None,
            notes: Vec::new(),
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: BeadPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Add dependencies
    pub fn with_depends_on(mut self, deps: Vec<BeadId>) -> Self {
        self.depends_on = deps;
        self
    }

    /// Add tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Update status
    pub fn set_status(&mut self, status: BeadStatus) {
        self.status = status;
        self.updated_at = Utc::now();

        if matches!(self.status, BeadStatus::Done) {
            self.completed_at = Some(Utc::now());
        }
    }

    /// Mark as in progress by an agent
    pub fn start(&mut self, agent_id: Uuid) {
        self.status = BeadStatus::InProgress;
        self.agent_id = Some(agent_id);
        self.updated_at = Utc::now();
    }

    /// Mark as completed
    pub fn complete(&mut self, summary: Option<String>) {
        self.status = BeadStatus::Done;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        self.agent_id = None;
        self.compacted_summary = summary;
    }

    /// Mark as blocked
    pub fn block(&mut self, reason: impl Into<String>) {
        self.status = BeadStatus::Blocked {
            reason: reason.into(),
        };
        self.updated_at = Utc::now();
        self.agent_id = None;
    }

    /// Cancel the bead
    pub fn cancel(&mut self, reason: impl Into<String>) {
        self.status = BeadStatus::Cancelled {
            reason: reason.into(),
        };
        self.updated_at = Utc::now();
        self.agent_id = None;
    }

    /// Add a note
    pub fn add_note(&mut self, content: impl Into<String>, author: impl Into<String>) {
        self.notes.push(BeadNote {
            timestamp: Utc::now(),
            content: content.into(),
            author: author.into(),
        });
        self.updated_at = Utc::now();
    }

    /// Add affected file
    pub fn add_file(&mut self, path: PathBuf) {
        if !self.files_affected.contains(&path) {
            self.files_affected.push(path);
            self.updated_at = Utc::now();
        }
    }

    /// Check if all dependencies are satisfied
    pub fn dependencies_satisfied(&self, completed_beads: &[BeadId]) -> bool {
        self.depends_on
            .iter()
            .all(|dep| completed_beads.contains(dep))
    }

    /// Create a sub-bead
    pub fn create_child(
        &self,
        index: u32,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let id = self.id.child(index);
        Self::with_id(id, title, description)
            .with_priority(self.priority)
            .with_tags(self.tags.clone())
    }
}

/// A log entry for bead changes (for JSONL storage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeadLogEntry {
    /// Timestamp of the entry
    pub timestamp: DateTime<Utc>,
    /// Type of operation
    pub operation: BeadOperation,
    /// The bead data (full for create, partial for update)
    pub bead: Bead,
}

/// Types of operations on beads
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeadOperation {
    /// Create a new bead
    Create,
    /// Update an existing bead
    Update,
    /// Delete a bead
    Delete,
}

impl BeadLogEntry {
    /// Create a new log entry for bead creation
    pub fn create(bead: Bead) -> Self {
        Self {
            timestamp: Utc::now(),
            operation: BeadOperation::Create,
            bead,
        }
    }

    /// Create a new log entry for bead update
    pub fn update(bead: Bead) -> Self {
        Self {
            timestamp: Utc::now(),
            operation: BeadOperation::Update,
            bead,
        }
    }

    /// Create a new log entry for bead deletion
    pub fn delete(bead: Bead) -> Self {
        Self {
            timestamp: Utc::now(),
            operation: BeadOperation::Delete,
            bead,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bead_id_from_title() {
        let id1 = BeadId::from_title("Add user authentication");
        let id2 = BeadId::from_title("Add user authentication");
        let id3 = BeadId::from_title("Different title");

        assert!(id1.0.starts_with("bd-"));
        assert_eq!(id1, id2); // Same title = same ID
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_bead_id_child() {
        let parent = BeadId::from_title("Parent task");
        let child1 = parent.child(1);
        let child2 = parent.child(2);
        let grandchild = child1.child(1);

        assert!(child1.0.ends_with(".1"));
        assert!(child2.0.ends_with(".2"));
        assert!(grandchild.0.ends_with(".1.1"));
    }

    #[test]
    fn test_bead_id_parent() {
        let root = BeadId::from_title("Root");
        let child = root.child(1);
        let grandchild = child.child(2);

        assert!(root.parent().is_none());
        assert_eq!(child.parent(), Some(root.clone()));
        assert_eq!(grandchild.parent(), Some(child));
    }

    #[test]
    fn test_bead_id_depth() {
        let root = BeadId::from_title("Root");
        let child = root.child(1);
        let grandchild = child.child(2);

        assert_eq!(root.depth(), 0);
        assert_eq!(child.depth(), 1);
        assert_eq!(grandchild.depth(), 2);
    }

    #[test]
    fn test_bead_new() {
        let bead = Bead::new("Test task", "Description of test task");

        assert!(!bead.id.0.is_empty());
        assert_eq!(bead.title, "Test task");
        assert_eq!(bead.status, BeadStatus::Pending);
        assert_eq!(bead.priority, BeadPriority::Medium);
    }

    #[test]
    fn test_bead_with_options() {
        let bead = Bead::new("Task", "Description")
            .with_priority(BeadPriority::High)
            .with_tags(vec!["feature".to_string(), "backend".to_string()])
            .with_depends_on(vec![BeadId::from_title("Prerequisite")]);

        assert_eq!(bead.priority, BeadPriority::High);
        assert_eq!(bead.tags.len(), 2);
        assert_eq!(bead.depends_on.len(), 1);
    }

    #[test]
    fn test_bead_status_transitions() {
        let mut bead = Bead::new("Task", "Description");
        let agent_id = Uuid::new_v4();

        assert!(matches!(bead.status, BeadStatus::Pending));

        bead.start(agent_id);
        assert!(matches!(bead.status, BeadStatus::InProgress));
        assert_eq!(bead.agent_id, Some(agent_id));

        bead.complete(Some("Task completed successfully".to_string()));
        assert!(matches!(bead.status, BeadStatus::Done));
        assert!(bead.completed_at.is_some());
        assert!(bead.agent_id.is_none());
    }

    #[test]
    fn test_bead_block() {
        let mut bead = Bead::new("Task", "Description");

        bead.block("Waiting for API access");

        if let BeadStatus::Blocked { reason } = &bead.status {
            assert_eq!(reason, "Waiting for API access");
        } else {
            panic!("Expected Blocked status");
        }
    }

    #[test]
    fn test_bead_cancel() {
        let mut bead = Bead::new("Task", "Description");

        bead.cancel("No longer needed");

        if let BeadStatus::Cancelled { reason } = &bead.status {
            assert_eq!(reason, "No longer needed");
        } else {
            panic!("Expected Cancelled status");
        }
    }

    #[test]
    fn test_bead_notes() {
        let mut bead = Bead::new("Task", "Description");

        bead.add_note("Started investigation", "explore-agent");
        bead.add_note("Found issue in auth module", "explore-agent");

        assert_eq!(bead.notes.len(), 2);
        assert_eq!(bead.notes[0].author, "explore-agent");
    }

    #[test]
    fn test_bead_dependencies_satisfied() {
        let dep1 = BeadId::from_title("Dep 1");
        let dep2 = BeadId::from_title("Dep 2");

        let bead =
            Bead::new("Task", "Description").with_depends_on(vec![dep1.clone(), dep2.clone()]);

        // Not satisfied with empty list
        assert!(!bead.dependencies_satisfied(&[]));

        // Not satisfied with partial
        assert!(!bead.dependencies_satisfied(std::slice::from_ref(&dep1)));

        // Satisfied with all deps
        assert!(bead.dependencies_satisfied(&[dep1, dep2]));
    }

    #[test]
    fn test_bead_create_child() {
        let parent = Bead::new("Parent task", "Parent description")
            .with_priority(BeadPriority::High)
            .with_tags(vec!["feature".to_string()]);

        let child = parent.create_child(1, "Child task", "Child description");

        assert_eq!(child.id, parent.id.child(1));
        assert_eq!(child.priority, BeadPriority::High); // Inherited
        assert_eq!(child.tags, vec!["feature"]); // Inherited
    }

    #[test]
    fn test_bead_status_is_terminal() {
        assert!(!BeadStatus::Pending.is_terminal());
        assert!(!BeadStatus::Ready.is_terminal());
        assert!(!BeadStatus::InProgress.is_terminal());
        assert!(!BeadStatus::Blocked {
            reason: "test".to_string()
        }
        .is_terminal());
        assert!(BeadStatus::Done.is_terminal());
        assert!(BeadStatus::Cancelled {
            reason: "test".to_string()
        }
        .is_terminal());
    }

    #[test]
    fn test_bead_status_is_actionable() {
        assert!(!BeadStatus::Pending.is_actionable());
        assert!(BeadStatus::Ready.is_actionable());
        assert!(BeadStatus::InProgress.is_actionable());
        assert!(!BeadStatus::Blocked {
            reason: "test".to_string()
        }
        .is_actionable());
        assert!(!BeadStatus::Done.is_actionable());
    }

    #[test]
    fn test_bead_log_entry() {
        let bead = Bead::new("Task", "Description");

        let create_entry = BeadLogEntry::create(bead.clone());
        assert!(matches!(create_entry.operation, BeadOperation::Create));

        let update_entry = BeadLogEntry::update(bead.clone());
        assert!(matches!(update_entry.operation, BeadOperation::Update));

        let delete_entry = BeadLogEntry::delete(bead);
        assert!(matches!(delete_entry.operation, BeadOperation::Delete));
    }
}
