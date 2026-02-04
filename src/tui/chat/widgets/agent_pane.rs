// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Agent pane widget for displaying agent status

use ratatui::{
    prelude::*,
    widgets::{Block, Borders},
};

use crate::tui::chat::state::{truncate_string, AgentStatus, AgentTracker, TrackedAgent};

/// Widget for rendering the agent status pane
pub struct AgentPane<'a> {
    tracker: &'a AgentTracker,
    expanded: bool,
    selected_index: Option<usize>,
    focused: bool,
}

impl<'a> AgentPane<'a> {
    pub fn new(tracker: &'a AgentTracker) -> Self {
        Self {
            tracker,
            expanded: false,
            selected_index: None,
            focused: false,
        }
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected_index = Some(index);
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Calculate minimum height needed
    pub fn min_height(&self) -> u16 {
        if self.tracker.total_count() == 0 {
            return 0;
        }

        if self.expanded {
            // Header + agents + footer
            (2 + self.tracker.total_count() as u16 + 1).min(12)
        } else {
            // Compact: just header line with inline status
            3
        }
    }
}

impl<'a> Widget for AgentPane<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 || self.tracker.total_count() == 0 {
            return;
        }

        let active = self.tracker.active_count();
        let done = self.tracker.completed_count();

        // Title with counts
        let title = if active > 0 {
            format!(" Agents ─ {} running │ {} done ", active, done)
        } else if done > 0 {
            format!(" Agents ─ {} done ", done)
        } else {
            " Agents ".to_string()
        };

        let border_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(border_style)
            .title(title)
            .title_style(Style::default().fg(Color::White));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 {
            return;
        }

        let agents = self.tracker.all();

        if self.expanded {
            // Expanded view: one agent per line with full details
            for (i, agent) in agents.iter().enumerate() {
                if i as u16 >= inner.height {
                    break;
                }

                let y = inner.y + i as u16;
                let is_selected = self.selected_index == Some(i);

                render_agent_line(agent, inner.x, y, inner.width, is_selected, buf);
            }
        } else {
            // Compact view: agents on a single line if possible
            render_agents_compact(&agents, inner, buf);
        }
    }
}

/// Render a single agent in expanded mode
fn render_agent_line(
    agent: &TrackedAgent,
    x: u16,
    y: u16,
    width: u16,
    selected: bool,
    buf: &mut Buffer,
) {
    let base_style = if selected {
        Style::default().bg(Color::DarkGray)
    } else {
        Style::default()
    };

    // Status indicator
    let indicator = agent.status.indicator();
    let indicator_style = match &agent.status {
        AgentStatus::Pending => Style::default().fg(Color::DarkGray),
        AgentStatus::Running => Style::default().fg(Color::Green),
        AgentStatus::RateLimited { .. } => Style::default().fg(Color::Yellow),
        AgentStatus::Completed => Style::default().fg(Color::Green),
        AgentStatus::Failed => Style::default().fg(Color::Red),
        AgentStatus::Cancelled => Style::default().fg(Color::DarkGray),
    };

    // Build the line
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(format!("{} ", indicator), indicator_style.patch(base_style)),
        Span::styled(
            &agent.name,
            Style::default().fg(Color::Cyan).patch(base_style),
        ),
        Span::raw("  "),
    ];

    // Progress bar for running agents
    if agent.status.is_active() {
        let bar = agent.progress.render_bar(12);
        spans.push(Span::styled(
            bar,
            Style::default().fg(Color::Blue).patch(base_style),
        ));
        spans.push(Span::styled(
            format!(
                " {}/{}",
                agent.progress.iteration, agent.progress.max_iterations
            ),
            Style::default().fg(Color::DarkGray).patch(base_style),
        ));
        spans.push(Span::raw("  "));
    }

    // Status text
    let status_text = agent.status_display();
    let status_max_len = width as usize - spans.iter().map(|s| s.content.len()).sum::<usize>() - 2;
    let status_truncated = truncate_string(&status_text, status_max_len);
    spans.push(Span::styled(
        status_truncated,
        Style::default().fg(Color::DarkGray).patch(base_style),
    ));

    let line = Line::from(spans);
    buf.set_line(x, y, &line, width);
}

/// Render agents in compact mode (single line)
fn render_agents_compact(agents: &[&TrackedAgent], area: Rect, buf: &mut Buffer) {
    if area.width < 10 {
        return;
    }

    let mut x = area.x + 1;
    let y = area.y;
    let max_x = area.x + area.width - 1;

    for (i, agent) in agents.iter().enumerate() {
        if x >= max_x - 10 {
            // Not enough space, show "..." indicator
            let more = format!(" +{}", agents.len() - i);
            buf.set_string(x, y, &more, Style::default().fg(Color::DarkGray));
            break;
        }

        // Status indicator
        let indicator = agent.status.indicator();
        let indicator_style = match &agent.status {
            AgentStatus::Pending => Style::default().fg(Color::DarkGray),
            AgentStatus::Running => Style::default().fg(Color::Green),
            AgentStatus::RateLimited { .. } => Style::default().fg(Color::Yellow),
            AgentStatus::Completed => Style::default().fg(Color::Green),
            AgentStatus::Failed => Style::default().fg(Color::Red),
            AgentStatus::Cancelled => Style::default().fg(Color::DarkGray),
        };

        buf.set_string(x, y, format!("{}", indicator), indicator_style);
        x += 2;

        // Agent name (truncated)
        let name = truncate_string(&agent.name, 15);
        buf.set_string(x, y, &name, Style::default().fg(Color::Cyan));
        x += name.len() as u16;

        // Progress for running agents
        if matches!(agent.status, AgentStatus::Running) {
            let progress = format!(
                " {}/{}",
                agent.progress.iteration, agent.progress.max_iterations
            );
            buf.set_string(x, y, &progress, Style::default().fg(Color::DarkGray));
            x += progress.len() as u16;
        }

        // Separator
        if i < agents.len() - 1 {
            buf.set_string(x, y, " │ ", Style::default().fg(Color::DarkGray));
            x += 3;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use uuid::Uuid;

    #[test]
    fn test_agent_pane_empty() {
        let tracker = AgentTracker::new();
        let pane = AgentPane::new(&tracker);
        assert_eq!(pane.min_height(), 0);
    }

    #[test]
    fn test_agent_pane_with_agents() {
        let mut tracker = AgentTracker::new();
        tracker.track(
            Uuid::new_v4(),
            "agent-1".to_string(),
            "explore".to_string(),
            "Task 1".to_string(),
        );
        tracker.track(
            Uuid::new_v4(),
            "agent-2".to_string(),
            "implement".to_string(),
            "Task 2".to_string(),
        );

        let pane = AgentPane::new(&tracker).expanded(true);
        assert!(pane.min_height() >= 4);
    }

    #[test]
    fn test_agent_pane_new() {
        let tracker = AgentTracker::new();
        let pane = AgentPane::new(&tracker);
        assert!(!pane.expanded);
        assert!(pane.selected_index.is_none());
        assert!(!pane.focused);
    }

    #[test]
    fn test_agent_pane_expanded_builder() {
        let tracker = AgentTracker::new();
        let pane = AgentPane::new(&tracker).expanded(true);
        assert!(pane.expanded);
    }

    #[test]
    fn test_agent_pane_selected_builder() {
        let tracker = AgentTracker::new();
        let pane = AgentPane::new(&tracker).selected(2);
        assert_eq!(pane.selected_index, Some(2));
    }

    #[test]
    fn test_agent_pane_focused_builder() {
        let tracker = AgentTracker::new();
        let pane = AgentPane::new(&tracker).focused(true);
        assert!(pane.focused);
    }

    #[test]
    fn test_agent_pane_min_height_compact() {
        let mut tracker = AgentTracker::new();
        tracker.track(
            Uuid::new_v4(),
            "agent-1".to_string(),
            "explore".to_string(),
            "Task".to_string(),
        );

        let pane = AgentPane::new(&tracker).expanded(false);
        assert_eq!(pane.min_height(), 3);
    }

    #[test]
    fn test_agent_pane_min_height_expanded_max() {
        let mut tracker = AgentTracker::new();
        // Add many agents to test max height
        for i in 0..20 {
            tracker.track(
                Uuid::new_v4(),
                format!("agent-{}", i),
                "explore".to_string(),
                format!("Task {}", i),
            );
        }

        let pane = AgentPane::new(&tracker).expanded(true);
        // Should be capped at 12
        assert!(pane.min_height() <= 12);
    }

    #[test]
    fn test_agent_pane_render_empty() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let tracker = AgentTracker::new();

        terminal
            .draw(|f| {
                let pane = AgentPane::new(&tracker);
                f.render_widget(pane, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_agent_pane_render_compact() {
        let backend = TestBackend::new(80, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut tracker = AgentTracker::new();
        let id = Uuid::new_v4();
        tracker.track(
            id,
            "research-agent".to_string(),
            "explore".to_string(),
            "Find API endpoints".to_string(),
        );
        tracker.set_running(&id);

        terminal
            .draw(|f| {
                let pane = AgentPane::new(&tracker);
                f.render_widget(pane, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_agent_pane_render_expanded() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut tracker = AgentTracker::new();

        let id1 = Uuid::new_v4();
        tracker.track(
            id1,
            "agent-1".to_string(),
            "explore".to_string(),
            "Task 1".to_string(),
        );
        tracker.set_running(&id1);
        tracker.update_progress(&id1, 5, 30, "Reading files...");

        let id2 = Uuid::new_v4();
        tracker.track(
            id2,
            "agent-2".to_string(),
            "implement".to_string(),
            "Task 2".to_string(),
        );
        tracker.set_completed(&id2, vec!["file.rs".to_string()], Some("Done".to_string()));

        terminal
            .draw(|f| {
                let pane = AgentPane::new(&tracker).expanded(true);
                f.render_widget(pane, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_agent_pane_render_focused() {
        let backend = TestBackend::new(80, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut tracker = AgentTracker::new();
        tracker.track(
            Uuid::new_v4(),
            "agent".to_string(),
            "explore".to_string(),
            "Task".to_string(),
        );

        terminal
            .draw(|f| {
                let pane = AgentPane::new(&tracker).focused(true);
                f.render_widget(pane, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_agent_pane_render_selected() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut tracker = AgentTracker::new();

        for i in 0..3 {
            tracker.track(
                Uuid::new_v4(),
                format!("agent-{}", i),
                "explore".to_string(),
                format!("Task {}", i),
            );
        }

        terminal
            .draw(|f| {
                let pane = AgentPane::new(&tracker).expanded(true).selected(1);
                f.render_widget(pane, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_agent_pane_render_all_statuses() {
        let backend = TestBackend::new(100, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut tracker = AgentTracker::new();

        // Pending
        let id1 = Uuid::new_v4();
        tracker.track(
            id1,
            "pending-agent".to_string(),
            "explore".to_string(),
            "Task 1".to_string(),
        );

        // Running
        let id2 = Uuid::new_v4();
        tracker.track(
            id2,
            "running-agent".to_string(),
            "explore".to_string(),
            "Task 2".to_string(),
        );
        tracker.set_running(&id2);

        // Rate limited
        let id3 = Uuid::new_v4();
        tracker.track(
            id3,
            "rate-limited".to_string(),
            "explore".to_string(),
            "Task 3".to_string(),
        );
        tracker.set_rate_limited(&id3, 30.5);

        // Completed
        let id4 = Uuid::new_v4();
        tracker.track(
            id4,
            "completed-agent".to_string(),
            "explore".to_string(),
            "Task 4".to_string(),
        );
        tracker.set_completed(&id4, vec![], None);

        // Failed
        let id5 = Uuid::new_v4();
        tracker.track(
            id5,
            "failed-agent".to_string(),
            "explore".to_string(),
            "Task 5".to_string(),
        );
        tracker.set_failed(&id5, "Something went wrong");

        // Cancelled
        let id6 = Uuid::new_v4();
        tracker.track(
            id6,
            "cancelled-agent".to_string(),
            "explore".to_string(),
            "Task 6".to_string(),
        );
        tracker.set_cancelled(&id6);

        terminal
            .draw(|f| {
                let pane = AgentPane::new(&tracker).expanded(true);
                f.render_widget(pane, f.area());
            })
            .unwrap();
    }

    #[test]
    fn test_agent_pane_render_narrow() {
        let backend = TestBackend::new(30, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut tracker = AgentTracker::new();

        for i in 0..5 {
            tracker.track(
                Uuid::new_v4(),
                format!("very-long-agent-name-{}", i),
                "explore".to_string(),
                format!("Task {}", i),
            );
        }

        terminal
            .draw(|f| {
                let pane = AgentPane::new(&tracker);
                f.render_widget(pane, f.area());
            })
            .unwrap();
        // Should handle narrow width gracefully
    }

    #[test]
    fn test_agent_pane_render_tiny_area() {
        let backend = TestBackend::new(10, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut tracker = AgentTracker::new();
        tracker.track(
            Uuid::new_v4(),
            "agent".to_string(),
            "explore".to_string(),
            "Task".to_string(),
        );

        terminal
            .draw(|f| {
                let pane = AgentPane::new(&tracker);
                f.render_widget(pane, f.area());
            })
            .unwrap();
        // Should not panic
    }

    #[test]
    fn test_agent_pane_title_with_done_only() {
        let backend = TestBackend::new(80, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut tracker = AgentTracker::new();

        let id = Uuid::new_v4();
        tracker.track(
            id,
            "agent".to_string(),
            "explore".to_string(),
            "Task".to_string(),
        );
        tracker.set_completed(&id, vec![], None);

        terminal
            .draw(|f| {
                let pane = AgentPane::new(&tracker);
                f.render_widget(pane, f.area());
            })
            .unwrap();
    }
}
