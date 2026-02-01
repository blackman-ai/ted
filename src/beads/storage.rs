// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Bead storage system
//!
//! Stores beads in a JSONL (JSON Lines) file format for git-friendly
//! append-only logging with an in-memory index for fast queries.
//!
//! ## Storage Location
//!
//! Beads are stored in `.beads/beads.jsonl` in the project root.
//! The format is one JSON object per line, representing log entries.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::error::{Result, TedError};

use super::schema::{Bead, BeadId, BeadLogEntry, BeadOperation, BeadPriority, BeadStatus};

/// Storage for beads with JSONL persistence and in-memory index
pub struct BeadStore {
    /// Path to the storage directory
    storage_path: PathBuf,
    /// In-memory index of beads (id -> bead)
    index: RwLock<HashMap<BeadId, Bead>>,
}

impl BeadStore {
    /// Create a new bead store at the given path
    pub fn new(storage_path: PathBuf) -> Result<Self> {
        // Create storage directory if it doesn't exist
        std::fs::create_dir_all(&storage_path)?;

        let store = Self {
            storage_path,
            index: RwLock::new(HashMap::new()),
        };

        // Load existing beads
        store.load()?;

        Ok(store)
    }

    /// Get the path to the JSONL file
    fn jsonl_path(&self) -> PathBuf {
        self.storage_path.join("beads.jsonl")
    }

    /// Load beads from the JSONL file into the index
    fn load(&self) -> Result<()> {
        let path = self.jsonl_path();
        if !path.exists() {
            return Ok(());
        }

        let file = std::fs::File::open(&path)?;
        let reader = BufReader::new(file);

        let mut index = self.index.write().unwrap();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            let entry: BeadLogEntry = serde_json::from_str(&line)
                .map_err(|e| TedError::Config(format!("Failed to parse bead log entry: {}", e)))?;

            match entry.operation {
                BeadOperation::Create | BeadOperation::Update => {
                    index.insert(entry.bead.id.clone(), entry.bead);
                }
                BeadOperation::Delete => {
                    index.remove(&entry.bead.id);
                }
            }
        }

        Ok(())
    }

    /// Append a log entry to the JSONL file
    fn append(&self, entry: &BeadLogEntry) -> Result<()> {
        let path = self.jsonl_path();
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        let line = serde_json::to_string(entry)?;
        writeln!(file, "{}", line)?;

        Ok(())
    }

    /// Create a new bead
    pub fn create(&self, bead: Bead) -> Result<BeadId> {
        let id = bead.id.clone();

        // Check if ID already exists
        {
            let index = self.index.read().unwrap();
            if index.contains_key(&id) {
                return Err(TedError::Config(format!(
                    "Bead with ID '{}' already exists",
                    id
                )));
            }
        }

        // Append to log
        let entry = BeadLogEntry::create(bead.clone());
        self.append(&entry)?;

        // Update index
        {
            let mut index = self.index.write().unwrap();
            index.insert(id.clone(), bead);
        }

        Ok(id)
    }

    /// Update an existing bead
    pub fn update(&self, bead: Bead) -> Result<()> {
        let id = bead.id.clone();

        // Check if exists
        {
            let index = self.index.read().unwrap();
            if !index.contains_key(&id) {
                return Err(TedError::Config(format!("Bead with ID '{}' not found", id)));
            }
        }

        // Append to log
        let entry = BeadLogEntry::update(bead.clone());
        self.append(&entry)?;

        // Update index
        {
            let mut index = self.index.write().unwrap();
            index.insert(id, bead);
        }

        Ok(())
    }

    /// Delete a bead
    pub fn delete(&self, id: &BeadId) -> Result<()> {
        let bead = {
            let index = self.index.read().unwrap();
            index
                .get(id)
                .cloned()
                .ok_or_else(|| TedError::Config(format!("Bead with ID '{}' not found", id)))?
        };

        // Append to log
        let entry = BeadLogEntry::delete(bead);
        self.append(&entry)?;

        // Update index
        {
            let mut index = self.index.write().unwrap();
            index.remove(id);
        }

        Ok(())
    }

    /// Get a bead by ID
    pub fn get(&self, id: &BeadId) -> Option<Bead> {
        let index = self.index.read().unwrap();
        index.get(id).cloned()
    }

    /// Get all beads
    pub fn all(&self) -> Vec<Bead> {
        let index = self.index.read().unwrap();
        index.values().cloned().collect()
    }

    /// Get beads by status
    pub fn by_status(&self, status: &BeadStatus) -> Vec<Bead> {
        let index = self.index.read().unwrap();
        index
            .values()
            .filter(|b| &b.status == status)
            .cloned()
            .collect()
    }

    /// Get beads that are ready to work on (Ready status)
    pub fn ready(&self) -> Vec<Bead> {
        self.by_status(&BeadStatus::Ready)
    }

    /// Get beads currently in progress
    pub fn in_progress(&self) -> Vec<Bead> {
        self.by_status(&BeadStatus::InProgress)
    }

    /// Get completed beads
    pub fn completed(&self) -> Vec<Bead> {
        self.by_status(&BeadStatus::Done)
    }

    /// Update bead status
    pub fn set_status(&self, id: &BeadId, status: BeadStatus) -> Result<()> {
        let mut bead = self
            .get(id)
            .ok_or_else(|| TedError::Config(format!("Bead with ID '{}' not found", id)))?;

        bead.set_status(status);
        self.update(bead)
    }

    /// Get beads whose dependencies are all satisfied
    pub fn get_actionable(&self) -> Vec<Bead> {
        let index = self.index.read().unwrap();

        // Get all completed bead IDs
        let completed: Vec<BeadId> = index
            .values()
            .filter(|b| matches!(b.status, BeadStatus::Done))
            .map(|b| b.id.clone())
            .collect();

        // Find pending beads with satisfied dependencies
        index
            .values()
            .filter(|b| {
                matches!(b.status, BeadStatus::Pending) && b.dependencies_satisfied(&completed)
            })
            .cloned()
            .collect()
    }

    /// Mark actionable beads as Ready
    pub fn refresh_ready(&self) -> Result<usize> {
        let actionable = self.get_actionable();
        let mut count = 0;

        for mut bead in actionable {
            bead.set_status(BeadStatus::Ready);
            self.update(bead)?;
            count += 1;
        }

        Ok(count)
    }

    /// Get beads by tag
    pub fn by_tag(&self, tag: &str) -> Vec<Bead> {
        let index = self.index.read().unwrap();
        index
            .values()
            .filter(|b| b.tags.contains(&tag.to_string()))
            .cloned()
            .collect()
    }

    /// Get beads by priority
    pub fn by_priority(&self, priority: BeadPriority) -> Vec<Bead> {
        let index = self.index.read().unwrap();
        index
            .values()
            .filter(|b| b.priority == priority)
            .cloned()
            .collect()
    }

    /// Get child beads of a parent
    pub fn children_of(&self, parent_id: &BeadId) -> Vec<Bead> {
        let index = self.index.read().unwrap();
        let prefix = format!("{}.", parent_id);

        index
            .values()
            .filter(|b| b.id.0.starts_with(&prefix))
            .cloned()
            .collect()
    }

    /// Get the count of beads
    pub fn count(&self) -> usize {
        let index = self.index.read().unwrap();
        index.len()
    }

    /// Get statistics about beads
    pub fn stats(&self) -> BeadStats {
        let index = self.index.read().unwrap();

        let mut stats = BeadStats {
            total: index.len(),
            ..Default::default()
        };

        for bead in index.values() {
            match &bead.status {
                BeadStatus::Pending => stats.pending += 1,
                BeadStatus::Ready => stats.ready += 1,
                BeadStatus::InProgress => stats.in_progress += 1,
                BeadStatus::Blocked { .. } => stats.blocked += 1,
                BeadStatus::Done => stats.done += 1,
                BeadStatus::Cancelled { .. } => stats.cancelled += 1,
            }
        }

        stats
    }

    /// Compact the JSONL file by writing only current state
    ///
    /// This reduces file size by removing historical entries.
    pub fn compact(&self) -> Result<()> {
        let path = self.jsonl_path();
        let temp_path = self.storage_path.join("beads.jsonl.tmp");

        // Write current state to temp file
        {
            let file = std::fs::File::create(&temp_path)?;
            let mut writer = std::io::BufWriter::new(file);

            let index = self.index.read().unwrap();
            for bead in index.values() {
                let entry = BeadLogEntry::create(bead.clone());
                let line = serde_json::to_string(&entry)?;
                writeln!(writer, "{}", line)?;
            }
        }

        // Atomically replace old file
        std::fs::rename(&temp_path, &path)?;

        Ok(())
    }
}

/// Statistics about beads
#[derive(Debug, Clone, Default)]
pub struct BeadStats {
    pub total: usize,
    pub pending: usize,
    pub ready: usize,
    pub in_progress: usize,
    pub blocked: usize,
    pub done: usize,
    pub cancelled: usize,
}

impl BeadStats {
    /// Get percentage complete
    pub fn completion_percentage(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.done as f64 / self.total as f64) * 100.0
    }
}

/// Initialize beads storage in a project directory
pub fn init_beads(project_dir: &Path) -> Result<BeadStore> {
    let beads_dir = project_dir.join(".beads");
    BeadStore::new(beads_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (TempDir, BeadStore) {
        let temp_dir = TempDir::new().unwrap();
        let store = BeadStore::new(temp_dir.path().to_path_buf()).unwrap();
        (temp_dir, store)
    }

    #[test]
    fn test_bead_store_create() {
        let (_temp, store) = create_test_store();

        let bead = Bead::new("Test task", "Test description");
        let id = store.create(bead).unwrap();

        assert!(store.get(&id).is_some());
        assert_eq!(store.count(), 1);
    }

    #[test]
    fn test_bead_store_update() {
        let (_temp, store) = create_test_store();

        let mut bead = Bead::new("Test task", "Description");
        let id = store.create(bead.clone()).unwrap();

        bead.set_status(BeadStatus::Ready);
        store.update(bead).unwrap();

        let retrieved = store.get(&id).unwrap();
        assert!(matches!(retrieved.status, BeadStatus::Ready));
    }

    #[test]
    fn test_bead_store_delete() {
        let (_temp, store) = create_test_store();

        let bead = Bead::new("Test task", "Description");
        let id = store.create(bead).unwrap();

        assert_eq!(store.count(), 1);

        store.delete(&id).unwrap();

        assert_eq!(store.count(), 0);
        assert!(store.get(&id).is_none());
    }

    #[test]
    fn test_bead_store_persistence() {
        let temp_dir = TempDir::new().unwrap();

        // Create and store a bead
        {
            let store = BeadStore::new(temp_dir.path().to_path_buf()).unwrap();
            let bead = Bead::new("Persistent task", "Description");
            store.create(bead).unwrap();
        }

        // Reload and verify
        {
            let store = BeadStore::new(temp_dir.path().to_path_buf()).unwrap();
            assert_eq!(store.count(), 1);

            let beads = store.all();
            assert_eq!(beads[0].title, "Persistent task");
        }
    }

    #[test]
    fn test_bead_store_by_status() {
        let (_temp, store) = create_test_store();

        let mut bead1 = Bead::new("Task 1", "Description");
        let mut bead2 = Bead::new("Task 2", "Description");
        let bead3 = Bead::new("Task 3", "Description");

        bead1.set_status(BeadStatus::Ready);
        bead2.set_status(BeadStatus::Ready);

        store.create(bead1).unwrap();
        store.create(bead2).unwrap();
        store.create(bead3).unwrap();

        let ready = store.ready();
        assert_eq!(ready.len(), 2);

        let pending = store.by_status(&BeadStatus::Pending);
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn test_bead_store_get_actionable() {
        let (_temp, store) = create_test_store();

        // Create dependency (start as InProgress so it's not actionable)
        let mut dep = Bead::new("Dependency", "Must be done first");
        dep.set_status(BeadStatus::InProgress);
        let dep_id = store.create(dep).unwrap();

        // Create dependent task
        let task = Bead::new("Task", "Depends on something").with_depends_on(vec![dep_id.clone()]);
        store.create(task).unwrap();

        // Create independent task
        let independent = Bead::new("Independent", "No dependencies");
        store.create(independent).unwrap();

        // Only independent task should be actionable (dep is InProgress, task has unmet deps)
        let actionable = store.get_actionable();
        assert_eq!(actionable.len(), 1);
        assert_eq!(actionable[0].title, "Independent");

        // Complete the dependency
        store.set_status(&dep_id, BeadStatus::Done).unwrap();

        // Now dependent task is also actionable (has satisfied deps), independent was already actioned
        // But independent is still Pending so it's still actionable
        let actionable = store.get_actionable();
        assert_eq!(actionable.len(), 2);

        // Mark independent as done to test only dependent task is actionable
        let independent_bead = store
            .all()
            .into_iter()
            .find(|b| b.title == "Independent")
            .unwrap();
        store
            .set_status(&independent_bead.id, BeadStatus::Done)
            .unwrap();

        let actionable = store.get_actionable();
        assert_eq!(actionable.len(), 1);
        assert_eq!(actionable[0].title, "Task");
    }

    #[test]
    fn test_bead_store_stats() {
        let (_temp, store) = create_test_store();

        let mut bead1 = Bead::new("Task 1", "Description");
        let mut bead2 = Bead::new("Task 2", "Description");
        let bead3 = Bead::new("Task 3", "Description");

        bead1.set_status(BeadStatus::Done);
        bead2.set_status(BeadStatus::InProgress);

        store.create(bead1).unwrap();
        store.create(bead2).unwrap();
        store.create(bead3).unwrap();

        let stats = store.stats();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.done, 1);
        assert_eq!(stats.in_progress, 1);
        assert_eq!(stats.pending, 1);
    }

    #[test]
    fn test_bead_store_compact() {
        let temp_dir = TempDir::new().unwrap();
        let store = BeadStore::new(temp_dir.path().to_path_buf()).unwrap();

        // Create and update beads multiple times
        let mut bead = Bead::new("Task", "Description");
        let id = store.create(bead.clone()).unwrap();

        for i in 0..10 {
            bead.add_note(format!("Update {}", i), "test");
            store.update(bead.clone()).unwrap();
        }

        // Compact
        store.compact().unwrap();

        // Reload and verify
        let new_store = BeadStore::new(temp_dir.path().to_path_buf()).unwrap();
        assert_eq!(new_store.count(), 1);

        let retrieved = new_store.get(&id).unwrap();
        assert_eq!(retrieved.notes.len(), 10);
    }

    #[test]
    fn test_bead_store_by_tag() {
        let (_temp, store) = create_test_store();

        let bead1 = Bead::new("Feature 1", "Description")
            .with_tags(vec!["feature".to_string(), "backend".to_string()]);
        let bead2 = Bead::new("Feature 2", "Description")
            .with_tags(vec!["feature".to_string(), "frontend".to_string()]);
        let bead3 = Bead::new("Bug fix", "Description").with_tags(vec!["bug".to_string()]);

        store.create(bead1).unwrap();
        store.create(bead2).unwrap();
        store.create(bead3).unwrap();

        let features = store.by_tag("feature");
        assert_eq!(features.len(), 2);

        let backend = store.by_tag("backend");
        assert_eq!(backend.len(), 1);
    }

    #[test]
    fn test_bead_store_children_of() {
        let (_temp, store) = create_test_store();

        let parent = Bead::new("Parent task", "Description");
        let parent_id = store.create(parent.clone()).unwrap();

        // Create child beads
        let child1 = Bead::with_id(parent_id.child(1), "Child 1", "Description");
        let child2 = Bead::with_id(parent_id.child(2), "Child 2", "Description");

        store.create(child1).unwrap();
        store.create(child2).unwrap();

        let children = store.children_of(&parent_id);
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_bead_stats_completion_percentage() {
        let stats = BeadStats {
            total: 10,
            done: 3,
            ..Default::default()
        };

        assert!((stats.completion_percentage() - 30.0).abs() < 0.01);

        let empty_stats = BeadStats::default();
        assert_eq!(empty_stats.completion_percentage(), 0.0);
    }
}
