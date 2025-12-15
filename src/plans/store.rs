// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Plan storage implementation
//!
//! Manages plan metadata in an index file and individual plan content in markdown files.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

use super::parser::{parse_plan, serialize_plan, PlanTask};
use super::{ensure_plans_dir, plans_dir, plans_index_path};
use crate::error::{Result, TedError};

/// Status of a plan
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanStatus {
    /// Currently being worked on
    Active,
    /// User paused work
    Paused,
    /// All tasks complete
    Complete,
    /// Stored for reference
    Archived,
}

impl PlanStatus {
    /// Get the display character for this status
    pub fn indicator(&self) -> char {
        match self {
            PlanStatus::Active => 'A',
            PlanStatus::Paused => 'P',
            PlanStatus::Complete => 'C',
            PlanStatus::Archived => 'X',
        }
    }

    /// Get the display name
    pub fn label(&self) -> &'static str {
        match self {
            PlanStatus::Active => "Active",
            PlanStatus::Paused => "Paused",
            PlanStatus::Complete => "Complete",
            PlanStatus::Archived => "Archived",
        }
    }
}

impl Default for PlanStatus {
    fn default() -> Self {
        PlanStatus::Active
    }
}

/// Information about a plan (stored in index.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanInfo {
    /// Unique plan ID
    pub id: Uuid,
    /// Plan title
    pub title: String,
    /// Current status
    pub status: PlanStatus,
    /// Linked session ID (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    /// When the plan was created
    pub created_at: DateTime<Utc>,
    /// When the plan was last modified
    pub modified_at: DateTime<Utc>,
    /// Project path this plan is associated with
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<PathBuf>,
    /// Total number of tasks
    pub task_count: usize,
    /// Number of completed tasks
    pub completed_count: usize,
}

impl PlanInfo {
    /// Create a new plan info
    pub fn new(title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            status: PlanStatus::Active,
            session_id: None,
            created_at: now,
            modified_at: now,
            project_path: None,
            task_count: 0,
            completed_count: 0,
        }
    }

    /// Update the modified timestamp
    pub fn touch(&mut self) {
        self.modified_at = Utc::now();
    }

    /// Calculate progress as a fraction (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        if self.task_count == 0 {
            0.0
        } else {
            self.completed_count as f64 / self.task_count as f64
        }
    }

    /// Check if the plan is complete
    pub fn is_complete(&self) -> bool {
        self.task_count > 0 && self.completed_count >= self.task_count
    }

    /// Link this plan to a session
    pub fn link_session(&mut self, session_id: Uuid) {
        self.session_id = Some(session_id);
        self.touch();
    }

    /// Set the project path
    pub fn set_project_path(&mut self, path: PathBuf) {
        self.project_path = Some(path);
        self.touch();
    }
}

/// Full plan including content
#[derive(Debug, Clone)]
pub struct Plan {
    /// Plan metadata
    pub info: PlanInfo,
    /// Full markdown content (excluding frontmatter)
    pub content: String,
    /// Parsed tasks from markdown
    pub tasks: Vec<PlanTask>,
}

impl Plan {
    /// Create a new empty plan
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            info: PlanInfo::new(title),
            content: String::new(),
            tasks: Vec::new(),
        }
    }

    /// Create a plan with content
    pub fn with_content(title: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let tasks = super::parser::extract_tasks(&content);
        let task_count = count_all_tasks(&tasks);
        let completed_count = count_completed_tasks(&tasks);

        let mut info = PlanInfo::new(title);
        info.task_count = task_count;
        info.completed_count = completed_count;

        Self {
            info,
            content,
            tasks,
        }
    }

    /// Update the content and recalculate tasks
    pub fn update_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
        self.tasks = super::parser::extract_tasks(&self.content);
        self.info.task_count = count_all_tasks(&self.tasks);
        self.info.completed_count = count_completed_tasks(&self.tasks);
        self.info.touch();
    }

    /// Append a progress log entry
    pub fn add_log_entry(&mut self, entry: impl Into<String>) {
        let now = Utc::now();
        let timestamp = now.format("%Y-%m-%d %H:%M").to_string();

        // Find or create Progress Log section
        if !self.content.contains("## Progress Log") {
            self.content.push_str("\n\n## Progress Log\n");
        }

        // Append new entry
        self.content
            .push_str(&format!("\n### {}\n{}\n", timestamp, entry.into()));
        self.info.touch();
    }
}

/// Count all tasks including subtasks
fn count_all_tasks(tasks: &[PlanTask]) -> usize {
    tasks
        .iter()
        .map(|t| 1 + count_all_tasks(&t.subtasks))
        .sum()
}

/// Count completed tasks including subtasks
fn count_completed_tasks(tasks: &[PlanTask]) -> usize {
    tasks
        .iter()
        .map(|t| {
            let self_complete = if t.completed { 1 } else { 0 };
            self_complete + count_completed_tasks(&t.subtasks)
        })
        .sum()
}

/// Plan store for managing plans
pub struct PlanStore {
    /// Path to the plans directory
    plans_dir: PathBuf,
    /// Path to the index file
    index_path: PathBuf,
    /// Cached plan metadata
    plans: Vec<PlanInfo>,
}

impl PlanStore {
    /// Open or create a plan store
    pub fn open() -> Result<Self> {
        ensure_plans_dir()?;

        let plans_dir = plans_dir();
        let index_path = plans_index_path();

        let plans = if index_path.exists() {
            let content = std::fs::read_to_string(&index_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(Self {
            plans_dir,
            index_path,
            plans,
        })
    }

    /// Save the index to disk
    fn save_index(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.plans)?;
        std::fs::write(&self.index_path, content)?;
        Ok(())
    }

    /// Get the file path for a plan
    fn plan_path(&self, id: Uuid) -> PathBuf {
        self.plans_dir.join(format!("{}.md", id))
    }

    /// Create a new plan
    pub fn create(&mut self, title: &str, content: &str) -> Result<Plan> {
        let plan = Plan::with_content(title, content);

        // Save the plan file
        let plan_content = serialize_plan(&plan)?;
        std::fs::write(self.plan_path(plan.info.id), plan_content)?;

        // Add to index
        self.plans.push(plan.info.clone());
        self.save_index()?;

        Ok(plan)
    }

    /// Get a plan by ID (loads full content)
    pub fn get(&self, id: Uuid) -> Result<Option<Plan>> {
        let info = match self.plans.iter().find(|p| p.id == id) {
            Some(info) => info.clone(),
            None => return Ok(None),
        };

        let path = self.plan_path(id);
        if !path.exists() {
            return Ok(None);
        }

        let file_content = std::fs::read_to_string(&path)?;
        let plan = parse_plan(&file_content, info)?;
        Ok(Some(plan))
    }

    /// Get plan info by ID (without loading content)
    pub fn get_info(&self, id: Uuid) -> Option<&PlanInfo> {
        self.plans.iter().find(|p| p.id == id)
    }

    /// Update plan content
    pub fn update(&mut self, id: Uuid, content: &str) -> Result<()> {
        // Find and update info
        let info = self
            .plans
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| TedError::Plan(format!("Plan not found: {}", id)))?;

        // Recalculate task counts
        let tasks = super::parser::extract_tasks(content);
        info.task_count = count_all_tasks(&tasks);
        info.completed_count = count_completed_tasks(&tasks);
        info.touch();

        // Build a plan to serialize
        let plan = Plan {
            info: info.clone(),
            content: content.to_string(),
            tasks,
        };

        // Save files
        let plan_content = serialize_plan(&plan)?;
        std::fs::write(self.plan_path(id), plan_content)?;
        self.save_index()?;

        Ok(())
    }

    /// Set plan status
    pub fn set_status(&mut self, id: Uuid, status: PlanStatus) -> Result<()> {
        let info = self
            .plans
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| TedError::Plan(format!("Plan not found: {}", id)))?;

        info.status = status;
        info.touch();

        // If we have the plan file, update its frontmatter too
        if let Ok(Some(mut plan)) = self.get(id) {
            plan.info.status = status;
            let plan_content = serialize_plan(&plan)?;
            std::fs::write(self.plan_path(id), plan_content)?;
        }

        self.save_index()?;
        Ok(())
    }

    /// Link plan to session
    pub fn link_session(&mut self, plan_id: Uuid, session_id: Uuid) -> Result<()> {
        let info = self
            .plans
            .iter_mut()
            .find(|p| p.id == plan_id)
            .ok_or_else(|| TedError::Plan(format!("Plan not found: {}", plan_id)))?;

        info.link_session(session_id);
        self.save_index()?;
        Ok(())
    }

    /// List all plans (metadata only)
    pub fn list(&self) -> &[PlanInfo] {
        &self.plans
    }

    /// List plans by status
    pub fn list_by_status(&self, status: PlanStatus) -> Vec<&PlanInfo> {
        self.plans.iter().filter(|p| p.status == status).collect()
    }

    /// List plans sorted by last modified (most recent first)
    pub fn list_recent(&self, limit: usize) -> Vec<&PlanInfo> {
        let mut sorted: Vec<_> = self.plans.iter().collect();
        sorted.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
        sorted.into_iter().take(limit).collect()
    }

    /// Find plans for a project path
    pub fn find_by_project(&self, path: &PathBuf) -> Vec<&PlanInfo> {
        self.plans
            .iter()
            .filter(|p| p.project_path.as_ref() == Some(path))
            .collect()
    }

    /// Delete a plan
    pub fn delete(&mut self, id: Uuid) -> Result<bool> {
        let initial_len = self.plans.len();
        self.plans.retain(|p| p.id != id);

        if self.plans.len() < initial_len {
            // Remove plan file
            let path = self.plan_path(id);
            if path.exists() {
                std::fs::remove_file(path)?;
            }
            self.save_index()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get the active plan (most recently modified active plan)
    pub fn get_active(&self) -> Option<&PlanInfo> {
        self.plans
            .iter()
            .filter(|p| p.status == PlanStatus::Active)
            .max_by_key(|p| p.modified_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (TempDir, PlanStore) {
        let dir = TempDir::new().unwrap();

        // Override the plans directory for testing
        let plans_dir = dir.path().to_path_buf();
        let index_path = plans_dir.join("index.json");

        let store = PlanStore {
            plans_dir,
            index_path,
            plans: Vec::new(),
        };

        (dir, store)
    }

    #[test]
    fn test_plan_status_indicator() {
        assert_eq!(PlanStatus::Active.indicator(), 'A');
        assert_eq!(PlanStatus::Paused.indicator(), 'P');
        assert_eq!(PlanStatus::Complete.indicator(), 'C');
        assert_eq!(PlanStatus::Archived.indicator(), 'X');
    }

    #[test]
    fn test_plan_status_label() {
        assert_eq!(PlanStatus::Active.label(), "Active");
        assert_eq!(PlanStatus::Complete.label(), "Complete");
    }

    #[test]
    fn test_plan_info_new() {
        let info = PlanInfo::new("Test Plan");
        assert_eq!(info.title, "Test Plan");
        assert_eq!(info.status, PlanStatus::Active);
        assert_eq!(info.task_count, 0);
        assert_eq!(info.completed_count, 0);
    }

    #[test]
    fn test_plan_info_progress() {
        let mut info = PlanInfo::new("Test");
        assert_eq!(info.progress(), 0.0);

        info.task_count = 4;
        info.completed_count = 2;
        assert!((info.progress() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_plan_info_is_complete() {
        let mut info = PlanInfo::new("Test");
        assert!(!info.is_complete());

        info.task_count = 3;
        info.completed_count = 3;
        assert!(info.is_complete());
    }

    #[test]
    fn test_plan_info_link_session() {
        let mut info = PlanInfo::new("Test");
        assert!(info.session_id.is_none());

        let session_id = Uuid::new_v4();
        info.link_session(session_id);
        assert_eq!(info.session_id, Some(session_id));
    }

    #[test]
    fn test_plan_new() {
        let plan = Plan::new("Test Plan");
        assert_eq!(plan.info.title, "Test Plan");
        assert!(plan.content.is_empty());
        assert!(plan.tasks.is_empty());
    }

    #[test]
    fn test_plan_with_content() {
        let content = "# Test\n\n- [ ] Task 1\n- [x] Task 2\n";
        let plan = Plan::with_content("Test", content);

        assert_eq!(plan.info.task_count, 2);
        assert_eq!(plan.info.completed_count, 1);
    }

    #[test]
    fn test_plan_update_content() {
        let mut plan = Plan::new("Test");
        plan.update_content("- [ ] Task 1\n- [ ] Task 2\n- [x] Task 3");

        assert_eq!(plan.info.task_count, 3);
        assert_eq!(plan.info.completed_count, 1);
    }

    #[test]
    fn test_plan_add_log_entry() {
        let mut plan = Plan::new("Test");
        plan.add_log_entry("Started work");

        assert!(plan.content.contains("## Progress Log"));
        assert!(plan.content.contains("Started work"));
    }

    #[test]
    fn test_store_create_and_get() {
        let (_dir, mut store) = create_test_store();

        let plan = store
            .create("Test Plan", "- [ ] Task 1\n- [x] Task 2")
            .unwrap();
        assert_eq!(plan.info.title, "Test Plan");
        assert_eq!(plan.info.task_count, 2);

        let loaded = store.get(plan.info.id).unwrap().unwrap();
        assert_eq!(loaded.info.title, "Test Plan");
        assert_eq!(loaded.info.task_count, 2);
    }

    #[test]
    fn test_store_update() {
        let (_dir, mut store) = create_test_store();

        let plan = store.create("Test", "- [ ] Task 1").unwrap();
        let id = plan.info.id;

        store
            .update(id, "- [x] Task 1\n- [ ] Task 2\n- [ ] Task 3")
            .unwrap();

        let updated = store.get(id).unwrap().unwrap();
        assert_eq!(updated.info.task_count, 3);
        assert_eq!(updated.info.completed_count, 1);
    }

    #[test]
    fn test_store_set_status() {
        let (_dir, mut store) = create_test_store();

        let plan = store.create("Test", "").unwrap();
        let id = plan.info.id;

        store.set_status(id, PlanStatus::Paused).unwrap();
        assert_eq!(store.get_info(id).unwrap().status, PlanStatus::Paused);
    }

    #[test]
    fn test_store_list_by_status() {
        let (_dir, mut store) = create_test_store();

        let p1 = store.create("Plan 1", "").unwrap();
        let p2 = store.create("Plan 2", "").unwrap();
        store.set_status(p2.info.id, PlanStatus::Paused).unwrap();

        let active = store.list_by_status(PlanStatus::Active);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, p1.info.id);

        let paused = store.list_by_status(PlanStatus::Paused);
        assert_eq!(paused.len(), 1);
    }

    #[test]
    fn test_store_delete() {
        let (_dir, mut store) = create_test_store();

        let plan = store.create("Test", "").unwrap();
        let id = plan.info.id;

        assert!(store.delete(id).unwrap());
        assert!(store.get(id).unwrap().is_none());

        // Delete non-existent returns false
        assert!(!store.delete(Uuid::new_v4()).unwrap());
    }

    #[test]
    fn test_store_get_active() {
        let (_dir, mut store) = create_test_store();

        assert!(store.get_active().is_none());

        let plan = store.create("Active Plan", "").unwrap();
        assert_eq!(store.get_active().unwrap().id, plan.info.id);
    }

    #[test]
    fn test_count_tasks_with_subtasks() {
        let tasks = vec![
            PlanTask {
                description: "Task 1".to_string(),
                completed: true,
                subtasks: vec![
                    PlanTask {
                        description: "Subtask 1.1".to_string(),
                        completed: true,
                        subtasks: vec![],
                    },
                    PlanTask {
                        description: "Subtask 1.2".to_string(),
                        completed: false,
                        subtasks: vec![],
                    },
                ],
            },
            PlanTask {
                description: "Task 2".to_string(),
                completed: false,
                subtasks: vec![],
            },
        ];

        assert_eq!(count_all_tasks(&tasks), 4);
        assert_eq!(count_completed_tasks(&tasks), 2);
    }
}
