// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Plan file parsing
//!
//! Handles parsing and serializing plan files with YAML frontmatter.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

use super::store::{Plan, PlanInfo, PlanStatus};
use crate::error::{Result, TedError};

/// YAML frontmatter structure
#[derive(Debug, Serialize, Deserialize)]
struct Frontmatter {
    id: Uuid,
    title: String,
    status: PlanStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    modified_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_path: Option<PathBuf>,
}

impl From<&PlanInfo> for Frontmatter {
    fn from(info: &PlanInfo) -> Self {
        Frontmatter {
            id: info.id,
            title: info.title.clone(),
            status: info.status,
            session_id: info.session_id,
            created_at: info.created_at,
            modified_at: info.modified_at,
            project_path: info.project_path.clone(),
        }
    }
}

/// A task parsed from markdown
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanTask {
    /// Task description text
    pub description: String,
    /// Whether the task is completed
    pub completed: bool,
    /// Nested subtasks
    pub subtasks: Vec<PlanTask>,
}

impl PlanTask {
    /// Create a new task
    pub fn new(description: impl Into<String>, completed: bool) -> Self {
        Self {
            description: description.into(),
            completed,
            subtasks: Vec::new(),
        }
    }

    /// Add a subtask
    pub fn with_subtask(mut self, subtask: PlanTask) -> Self {
        self.subtasks.push(subtask);
        self
    }
}

/// Parse a plan file (YAML frontmatter + markdown content)
pub fn parse_plan(file_content: &str, existing_info: PlanInfo) -> Result<Plan> {
    let (frontmatter_str, content) = split_frontmatter(file_content);

    // If we have frontmatter, parse it to update info
    let info = if let Some(fm_str) = frontmatter_str {
        match serde_yaml::from_str::<Frontmatter>(&fm_str) {
            Ok(fm) => PlanInfo {
                id: fm.id,
                title: fm.title,
                status: fm.status,
                session_id: fm.session_id,
                created_at: fm.created_at,
                modified_at: fm.modified_at,
                project_path: fm.project_path,
                task_count: existing_info.task_count,
                completed_count: existing_info.completed_count,
            },
            Err(_) => existing_info,
        }
    } else {
        existing_info
    };

    // Extract tasks from content
    let tasks = extract_tasks(&content);

    // Update task counts
    let mut info = info;
    info.task_count = count_all_tasks(&tasks);
    info.completed_count = count_completed_tasks(&tasks);

    Ok(Plan {
        info,
        content,
        tasks,
    })
}

/// Serialize a plan to file content (YAML frontmatter + markdown)
pub fn serialize_plan(plan: &Plan) -> Result<String> {
    let frontmatter = Frontmatter::from(&plan.info);
    let yaml = serde_yaml::to_string(&frontmatter)
        .map_err(|e| TedError::Plan(format!("Failed to serialize frontmatter: {}", e)))?;

    Ok(format!("---\n{}---\n\n{}", yaml, plan.content))
}

/// Split file content into frontmatter and body
fn split_frontmatter(content: &str) -> (Option<String>, String) {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return (None, content.to_string());
    }

    // Find the closing ---
    let rest = &trimmed[3..];
    if let Some(end_idx) = rest.find("\n---") {
        let frontmatter = rest[..end_idx].trim().to_string();
        let body = rest[end_idx + 4..].trim_start().to_string();
        (Some(frontmatter), body)
    } else {
        (None, content.to_string())
    }
}

/// Extract tasks from markdown content
pub fn extract_tasks(content: &str) -> Vec<PlanTask> {
    let mut tasks: Vec<PlanTask> = Vec::new();
    let mut task_stack: Vec<(usize, usize)> = Vec::new(); // (indent_level, index in parent)

    for line in content.lines() {
        if let Some((indent, completed, description)) = parse_task_line(line) {
            let task = PlanTask::new(description, completed);

            if indent == 0 {
                // Top-level task
                tasks.push(task);
                task_stack.clear();
                task_stack.push((0, tasks.len() - 1));
            } else {
                // Find parent based on indentation
                while let Some(&(parent_indent, _)) = task_stack.last() {
                    if parent_indent < indent {
                        break;
                    }
                    task_stack.pop();
                }

                if task_stack.last().is_some() {
                    // Add as subtask to appropriate parent
                    add_subtask_at_path(&mut tasks, &task_stack, task.clone());
                    let subtask_idx = get_subtask_count(&tasks, &task_stack) - 1;
                    task_stack.push((indent, subtask_idx));
                } else {
                    // No valid parent, add as top-level
                    tasks.push(task);
                    task_stack.clear();
                    task_stack.push((0, tasks.len() - 1));
                }
            }
        }
    }

    tasks
}

/// Parse a single task line
/// Returns (indent_level, completed, description)
fn parse_task_line(line: &str) -> Option<(usize, bool, String)> {
    // Count leading whitespace
    let trimmed_start = line.trim_start();
    let indent = line.len() - trimmed_start.len();

    // Check for task marker: - [ ] or - [x] or - [X]
    let trimmed = trimmed_start.trim_start_matches('-').trim_start();

    if let Some(rest) = trimmed.strip_prefix("[ ]") {
        let description = rest.trim().to_string();
        if !description.is_empty() {
            return Some((indent, false, description));
        }
    } else if let Some(rest) = trimmed
        .strip_prefix("[x]")
        .or_else(|| trimmed.strip_prefix("[X]"))
    {
        let description = rest.trim().to_string();
        if !description.is_empty() {
            return Some((indent, true, description));
        }
    }

    None
}

/// Add a subtask at the given path
fn add_subtask_at_path(tasks: &mut [PlanTask], path: &[(usize, usize)], task: PlanTask) {
    if path.is_empty() {
        return;
    }

    let mut current_tasks = tasks;
    for (i, &(_, idx)) in path.iter().enumerate() {
        if i == path.len() - 1 {
            // Last element - add subtask here
            if idx < current_tasks.len() {
                current_tasks[idx].subtasks.push(task);
            }
            return;
        } else if idx < current_tasks.len() {
            current_tasks = &mut current_tasks[idx].subtasks;
        }
    }
}

/// Get the count of subtasks at the given path
fn get_subtask_count(tasks: &[PlanTask], path: &[(usize, usize)]) -> usize {
    if path.is_empty() {
        return 0;
    }

    let mut current_tasks = tasks;
    for &(_, idx) in path.iter() {
        if idx < current_tasks.len() {
            current_tasks = &current_tasks[idx].subtasks;
        }
    }
    current_tasks.len()
}

/// Count all tasks including subtasks
fn count_all_tasks(tasks: &[PlanTask]) -> usize {
    tasks.iter().map(|t| 1 + count_all_tasks(&t.subtasks)).sum()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_frontmatter_with_fm() {
        let content = "---\ntitle: Test\n---\n\nBody content";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_some());
        assert!(fm.unwrap().contains("title: Test"));
        assert_eq!(body, "Body content");
    }

    #[test]
    fn test_split_frontmatter_without_fm() {
        let content = "Just body content";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body, "Just body content");
    }

    #[test]
    fn test_parse_task_line_unchecked() {
        let result = parse_task_line("- [ ] Task description");
        assert!(result.is_some());
        let (indent, completed, desc) = result.unwrap();
        assert_eq!(indent, 0);
        assert!(!completed);
        assert_eq!(desc, "Task description");
    }

    #[test]
    fn test_parse_task_line_checked() {
        let result = parse_task_line("- [x] Completed task");
        assert!(result.is_some());
        let (_, completed, desc) = result.unwrap();
        assert!(completed);
        assert_eq!(desc, "Completed task");
    }

    #[test]
    fn test_parse_task_line_indented() {
        let result = parse_task_line("  - [ ] Subtask");
        assert!(result.is_some());
        let (indent, _, _) = result.unwrap();
        assert_eq!(indent, 2);
    }

    #[test]
    fn test_parse_task_line_not_a_task() {
        assert!(parse_task_line("Regular text").is_none());
        assert!(parse_task_line("- Regular bullet").is_none());
        assert!(parse_task_line("- [ ]").is_none()); // Empty task
    }

    #[test]
    fn test_extract_tasks_flat() {
        let content = "- [ ] Task 1\n- [x] Task 2\n- [ ] Task 3";
        let tasks = extract_tasks(content);

        assert_eq!(tasks.len(), 3);
        assert!(!tasks[0].completed);
        assert!(tasks[1].completed);
        assert!(!tasks[2].completed);
    }

    #[test]
    fn test_extract_tasks_nested() {
        let content = "- [ ] Task 1\n  - [ ] Subtask 1.1\n  - [x] Subtask 1.2\n- [ ] Task 2";
        let tasks = extract_tasks(content);

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].subtasks.len(), 2);
        assert!(!tasks[0].subtasks[0].completed);
        assert!(tasks[0].subtasks[1].completed);
    }

    #[test]
    fn test_extract_tasks_deeply_nested() {
        let content =
            "- [ ] Level 1\n  - [ ] Level 2\n    - [ ] Level 3\n      - [x] Level 4\n- [ ] Another";
        let tasks = extract_tasks(content);

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].subtasks.len(), 1);
        assert_eq!(tasks[0].subtasks[0].subtasks.len(), 1);
    }

    #[test]
    fn test_count_all_tasks() {
        let tasks = vec![
            PlanTask::new("Task 1", false)
                .with_subtask(PlanTask::new("Subtask 1.1", false))
                .with_subtask(PlanTask::new("Subtask 1.2", true)),
            PlanTask::new("Task 2", false),
        ];

        assert_eq!(count_all_tasks(&tasks), 4);
    }

    #[test]
    fn test_count_completed_tasks() {
        let tasks = vec![
            PlanTask::new("Task 1", true)
                .with_subtask(PlanTask::new("Subtask 1.1", false))
                .with_subtask(PlanTask::new("Subtask 1.2", true)),
            PlanTask::new("Task 2", false),
        ];

        assert_eq!(count_completed_tasks(&tasks), 2);
    }

    #[test]
    fn test_serialize_and_parse_roundtrip() {
        let mut info = PlanInfo::new("Test Plan");
        info.task_count = 2;
        info.completed_count = 1;

        let plan = Plan {
            info: info.clone(),
            content: "# Test\n\n- [x] Task 1\n- [ ] Task 2".to_string(),
            tasks: vec![
                PlanTask::new("Task 1", true),
                PlanTask::new("Task 2", false),
            ],
        };

        let serialized = serialize_plan(&plan).unwrap();
        assert!(serialized.contains("---"));
        assert!(serialized.contains("title: Test Plan"));

        let parsed = parse_plan(&serialized, info).unwrap();
        assert_eq!(parsed.info.title, "Test Plan");
        assert_eq!(parsed.tasks.len(), 2);
    }

    #[test]
    fn test_plan_task_new() {
        let task = PlanTask::new("Test task", false);
        assert_eq!(task.description, "Test task");
        assert!(!task.completed);
        assert!(task.subtasks.is_empty());
    }

    #[test]
    fn test_plan_task_with_subtask() {
        let task = PlanTask::new("Parent", false)
            .with_subtask(PlanTask::new("Child 1", false))
            .with_subtask(PlanTask::new("Child 2", true));

        assert_eq!(task.subtasks.len(), 2);
        assert!(!task.subtasks[0].completed);
        assert!(task.subtasks[1].completed);
    }

    #[test]
    fn test_frontmatter_from_plan_info() {
        let info = PlanInfo::new("Test");
        let fm = Frontmatter::from(&info);

        assert_eq!(fm.id, info.id);
        assert_eq!(fm.title, "Test");
        assert_eq!(fm.status, PlanStatus::Active);
    }
}
