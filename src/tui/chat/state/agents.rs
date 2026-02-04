// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Agent tracking and state management
//!
//! Tracks all running and completed agents with their progress, status, and results.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use uuid::Uuid;

use super::messages::truncate_string;

/// Status of a tracked agent
#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    /// Queued, waiting to start
    Pending,
    /// Actively executing
    Running,
    /// Waiting for rate limit budget
    RateLimited { wait_secs: f64 },
    /// Finished successfully
    Completed,
    /// Finished with error
    Failed,
    /// User cancelled
    Cancelled,
}

impl AgentStatus {
    /// Get a display label for the status
    pub fn label(&self) -> &'static str {
        match self {
            AgentStatus::Pending => "pending",
            AgentStatus::Running => "running",
            AgentStatus::RateLimited { .. } => "rate-limited",
            AgentStatus::Completed => "done",
            AgentStatus::Failed => "failed",
            AgentStatus::Cancelled => "cancelled",
        }
    }

    /// Get the status indicator character
    pub fn indicator(&self) -> char {
        match self {
            AgentStatus::Pending => '○',
            AgentStatus::Running => '●',
            AgentStatus::RateLimited { .. } => '◐',
            AgentStatus::Completed => '✓',
            AgentStatus::Failed => '✗',
            AgentStatus::Cancelled => '⊘',
        }
    }

    /// Check if the agent is still active (not finished)
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            AgentStatus::Pending | AgentStatus::Running | AgentStatus::RateLimited { .. }
        )
    }

    /// Check if the agent finished (successfully or not)
    pub fn is_finished(&self) -> bool {
        matches!(
            self,
            AgentStatus::Completed | AgentStatus::Failed | AgentStatus::Cancelled
        )
    }
}

/// Progress tracking for an agent
#[derive(Debug, Clone)]
pub struct AgentProgress {
    pub iteration: u32,
    pub max_iterations: u32,
    pub tokens_used: u64,
    pub token_budget: u64,
}

impl Default for AgentProgress {
    fn default() -> Self {
        Self {
            iteration: 0,
            max_iterations: 30, // Default max iterations
            tokens_used: 0,
            token_budget: 0,
        }
    }
}

impl AgentProgress {
    /// Calculate progress as a fraction (0.0 to 1.0)
    pub fn fraction(&self) -> f64 {
        if self.max_iterations == 0 {
            0.0
        } else {
            (self.iteration as f64 / self.max_iterations as f64).min(1.0)
        }
    }

    /// Render a progress bar
    pub fn render_bar(&self, width: usize) -> String {
        let filled = ((self.fraction() * width as f64) as usize).min(width);
        let empty = width - filled;
        format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
    }
}

/// A tracked agent with all its state
#[derive(Debug, Clone)]
pub struct TrackedAgent {
    pub id: Uuid,
    pub name: String,
    pub agent_type: String,
    pub task: String,
    pub status: AgentStatus,
    pub progress: AgentProgress,
    pub started_at: Instant,
    pub completed_at: Option<Instant>,
    pub current_action: Option<String>,
    pub current_tool: Option<String>,
    pub files_changed: Vec<String>,
    pub error: Option<String>,
    pub summary: Option<String>,
}

impl TrackedAgent {
    /// Create a new tracked agent
    pub fn new(id: Uuid, name: String, agent_type: String, task: String) -> Self {
        Self {
            id,
            name,
            agent_type,
            task,
            status: AgentStatus::Pending,
            progress: AgentProgress::default(),
            started_at: Instant::now(),
            completed_at: None,
            current_action: None,
            current_tool: None,
            files_changed: Vec::new(),
            error: None,
            summary: None,
        }
    }

    /// Get elapsed time since agent started
    pub fn elapsed(&self) -> Duration {
        if let Some(completed) = self.completed_at {
            completed.duration_since(self.started_at)
        } else {
            self.started_at.elapsed()
        }
    }

    /// Format elapsed time for display
    pub fn elapsed_display(&self) -> String {
        let secs = self.elapsed().as_secs();
        if secs < 60 {
            format!("{}s", secs)
        } else {
            format!("{}m{}s", secs / 60, secs % 60)
        }
    }

    /// Get a short status string for display
    pub fn status_display(&self) -> String {
        match &self.status {
            AgentStatus::Pending => "Waiting...".to_string(),
            AgentStatus::Running => {
                if let Some(action) = &self.current_action {
                    truncate_string(action, 40)
                } else if let Some(tool) = &self.current_tool {
                    format!("Running {}...", tool)
                } else {
                    "Processing...".to_string()
                }
            }
            AgentStatus::RateLimited { wait_secs } => {
                format!("Rate limited ({:.1}s)", wait_secs)
            }
            AgentStatus::Completed => {
                if self.files_changed.is_empty() {
                    format!("Done in {}", self.elapsed_display())
                } else {
                    format!(
                        "Done in {} ({} files)",
                        self.elapsed_display(),
                        self.files_changed.len()
                    )
                }
            }
            AgentStatus::Failed => {
                if let Some(error) = &self.error {
                    format!("Failed: {}", truncate_string(error, 30))
                } else {
                    "Failed".to_string()
                }
            }
            AgentStatus::Cancelled => "Cancelled".to_string(),
        }
    }
}

/// Tracks all agents across the session
#[derive(Debug, Default)]
pub struct AgentTracker {
    /// All tracked agents by ID
    agents: HashMap<Uuid, TrackedAgent>,
    /// Order agents were spawned (for display ordering)
    spawn_order: Vec<Uuid>,
}

impl AgentTracker {
    /// Create a new empty tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Track a newly spawned agent
    pub fn track(&mut self, id: Uuid, name: String, agent_type: String, task: String) {
        let agent = TrackedAgent::new(id, name, agent_type, task);
        self.agents.insert(id, agent);
        self.spawn_order.push(id);
    }

    /// Get an agent by ID
    pub fn get(&self, id: &Uuid) -> Option<&TrackedAgent> {
        self.agents.get(id)
    }

    /// Get a mutable reference to an agent by ID
    pub fn get_mut(&mut self, id: &Uuid) -> Option<&mut TrackedAgent> {
        self.agents.get_mut(id)
    }

    /// Update agent status to running
    pub fn set_running(&mut self, id: &Uuid) {
        if let Some(agent) = self.agents.get_mut(id) {
            agent.status = AgentStatus::Running;
        }
    }

    /// Update agent progress
    pub fn update_progress(
        &mut self,
        id: &Uuid,
        iteration: u32,
        max_iterations: u32,
        action: &str,
    ) {
        if let Some(agent) = self.agents.get_mut(id) {
            agent.status = AgentStatus::Running;
            agent.progress.iteration = iteration;
            agent.progress.max_iterations = max_iterations;
            agent.current_action = Some(action.to_string());
        }
    }

    /// Update agent rate limited status
    pub fn set_rate_limited(&mut self, id: &Uuid, wait_secs: f64) {
        if let Some(agent) = self.agents.get_mut(id) {
            agent.status = AgentStatus::RateLimited { wait_secs };
        }
    }

    /// Update agent current tool
    pub fn set_current_tool(&mut self, id: &Uuid, tool_name: Option<&str>) {
        if let Some(agent) = self.agents.get_mut(id) {
            agent.current_tool = tool_name.map(|s| s.to_string());
        }
    }

    /// Mark agent as completed
    pub fn set_completed(
        &mut self,
        id: &Uuid,
        files_changed: Vec<String>,
        summary: Option<String>,
    ) {
        if let Some(agent) = self.agents.get_mut(id) {
            agent.status = AgentStatus::Completed;
            agent.completed_at = Some(Instant::now());
            agent.files_changed = files_changed;
            agent.summary = summary;
            agent.current_action = None;
            agent.current_tool = None;
        }
    }

    /// Mark agent as failed
    pub fn set_failed(&mut self, id: &Uuid, error: &str) {
        if let Some(agent) = self.agents.get_mut(id) {
            agent.status = AgentStatus::Failed;
            agent.completed_at = Some(Instant::now());
            agent.error = Some(error.to_string());
            agent.current_action = None;
            agent.current_tool = None;
        }
    }

    /// Mark agent as cancelled
    pub fn set_cancelled(&mut self, id: &Uuid) {
        if let Some(agent) = self.agents.get_mut(id) {
            agent.status = AgentStatus::Cancelled;
            agent.completed_at = Some(Instant::now());
            agent.current_action = None;
            agent.current_tool = None;
        }
    }

    /// Get all agents in spawn order
    pub fn all(&self) -> Vec<&TrackedAgent> {
        self.spawn_order
            .iter()
            .filter_map(|id| self.agents.get(id))
            .collect()
    }

    /// Get all active agents (pending, running, or rate-limited)
    pub fn active(&self) -> Vec<&TrackedAgent> {
        self.all()
            .into_iter()
            .filter(|a| a.status.is_active())
            .collect()
    }

    /// Get all completed agents
    pub fn completed(&self) -> Vec<&TrackedAgent> {
        self.all()
            .into_iter()
            .filter(|a| a.status.is_finished())
            .collect()
    }

    /// Count of active agents
    pub fn active_count(&self) -> usize {
        self.agents
            .values()
            .filter(|a| a.status.is_active())
            .count()
    }

    /// Count of completed agents
    pub fn completed_count(&self) -> usize {
        self.agents
            .values()
            .filter(|a| a.status.is_finished())
            .count()
    }

    /// Total count of tracked agents
    pub fn total_count(&self) -> usize {
        self.agents.len()
    }

    /// Check if any agents are currently running
    pub fn has_active(&self) -> bool {
        self.agents.values().any(|a| a.status.is_active())
    }

    /// Remove all finished agents (cleanup)
    pub fn clear_finished(&mut self) {
        let finished_ids: Vec<Uuid> = self
            .agents
            .iter()
            .filter(|(_, a)| a.status.is_finished())
            .map(|(id, _)| *id)
            .collect();

        for id in finished_ids {
            self.agents.remove(&id);
            self.spawn_order.retain(|i| i != &id);
        }
    }

    /// Clear all tracked agents
    pub fn clear(&mut self) {
        self.agents.clear();
        self.spawn_order.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_status_labels() {
        assert_eq!(AgentStatus::Pending.label(), "pending");
        assert_eq!(AgentStatus::Running.label(), "running");
        assert_eq!(AgentStatus::Completed.label(), "done");
        assert_eq!(AgentStatus::Failed.label(), "failed");
    }

    #[test]
    fn test_agent_status_indicators() {
        assert_eq!(AgentStatus::Pending.indicator(), '○');
        assert_eq!(AgentStatus::Running.indicator(), '●');
        assert_eq!(AgentStatus::Completed.indicator(), '✓');
    }

    #[test]
    fn test_agent_progress_fraction() {
        let mut progress = AgentProgress::default();
        assert_eq!(progress.fraction(), 0.0);

        progress.iteration = 15;
        progress.max_iterations = 30;
        assert_eq!(progress.fraction(), 0.5);

        progress.iteration = 30;
        assert_eq!(progress.fraction(), 1.0);
    }

    #[test]
    fn test_agent_progress_bar() {
        let progress = AgentProgress {
            iteration: 5,
            max_iterations: 10,
            ..Default::default()
        };

        let bar = progress.render_bar(10);
        assert_eq!(bar, "[█████░░░░░]");
    }

    #[test]
    fn test_tracker_lifecycle() {
        let mut tracker = AgentTracker::new();
        let id = Uuid::new_v4();

        // Track new agent
        tracker.track(
            id,
            "test-agent".to_string(),
            "implement".to_string(),
            "Test task".to_string(),
        );
        assert_eq!(tracker.total_count(), 1);
        assert_eq!(tracker.active_count(), 1);

        // Update to running
        tracker.set_running(&id);
        assert!(matches!(
            tracker.get(&id).unwrap().status,
            AgentStatus::Running
        ));

        // Update progress
        tracker.update_progress(&id, 5, 30, "Reading files...");
        assert_eq!(tracker.get(&id).unwrap().progress.iteration, 5);

        // Complete
        tracker.set_completed(&id, vec!["file.rs".to_string()], Some("Done".to_string()));
        assert_eq!(tracker.active_count(), 0);
        assert_eq!(tracker.completed_count(), 1);
    }

    #[test]
    fn test_tracker_multiple_agents() {
        let mut tracker = AgentTracker::new();

        for i in 0..5 {
            tracker.track(
                Uuid::new_v4(),
                format!("agent-{}", i),
                "explore".to_string(),
                format!("Task {}", i),
            );
        }

        assert_eq!(tracker.total_count(), 5);
        assert_eq!(tracker.active_count(), 5);
        assert!(tracker.has_active());

        // Verify spawn order preserved
        let all = tracker.all();
        assert_eq!(all.len(), 5);
        assert_eq!(all[0].name, "agent-0");
        assert_eq!(all[4].name, "agent-4");
    }

    #[test]
    fn test_tracker_clear_finished() {
        let mut tracker = AgentTracker::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        tracker.track(
            id1,
            "agent-1".to_string(),
            "explore".to_string(),
            "Task 1".to_string(),
        );
        tracker.track(
            id2,
            "agent-2".to_string(),
            "explore".to_string(),
            "Task 2".to_string(),
        );

        tracker.set_completed(&id1, vec![], None);

        tracker.clear_finished();

        assert_eq!(tracker.total_count(), 1);
        assert!(tracker.get(&id1).is_none());
        assert!(tracker.get(&id2).is_some());
    }
}
