// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! UI rendering for the TUI
//!
//! Handles layout and rendering of all screens using ratatui.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use super::app::{
    App, ContextItem, InputMode, MainMenuItem, ModelSelectionTarget, ProviderItem, Screen,
};
use super::editor::EditorMode;
use crate::plans::PlanStatus;

/// Main draw function
pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(0),    // Content
            Constraint::Length(3), // Status/Help
        ])
        .split(frame.area());

    // Draw title
    draw_title(frame, chunks[0], app);

    // Draw content based on current screen
    match app.screen {
        Screen::MainMenu => draw_main_menu(frame, chunks[1], app),
        Screen::Providers => draw_providers(frame, chunks[1], app),
        Screen::Caps => draw_caps(frame, chunks[1], app),
        Screen::Context => draw_context(frame, chunks[1], app),
        Screen::About => draw_about(frame, chunks[1]),
        Screen::Plans => draw_plans(frame, chunks[1], app),
        Screen::PlanView => draw_plan_view(frame, chunks[1], app),
        Screen::PlanEdit => draw_plan_edit(frame, chunks[1], app),
    }

    // Draw status bar
    draw_status(frame, chunks[2], app);

    // Draw input popup if editing
    if app.input_mode == InputMode::Editing {
        draw_input_popup(frame, app);
    }

    // Draw model picker popup if selecting
    if app.input_mode == InputMode::SelectingModel {
        draw_model_picker_popup(frame, app);
    }
}

/// Draw the title bar
fn draw_title(frame: &mut Frame, area: Rect, app: &App) {
    let title = match app.screen {
        Screen::MainMenu => "ted settings",
        Screen::Providers => "Providers",
        Screen::Caps => "Caps",
        Screen::Context => "Context",
        Screen::About => "About",
        Screen::Plans => "Plans",
        Screen::PlanView => "Plan View",
        Screen::PlanEdit => "Edit Plan",
    };

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));

    let title_text = Paragraph::new(format!(" {} ", title))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(block);

    frame.render_widget(title_text, area);
}

/// Draw the status bar
fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let (text, style) = if let Some(ref msg) = app.status_message {
        let color = if app.status_is_error {
            Color::Red
        } else {
            Color::Green
        };
        (msg.clone(), Style::default().fg(color))
    } else {
        let help = match app.input_mode {
            InputMode::Normal => "↑↓: Navigate | Enter: Select | q: Back | ?: Help",
            InputMode::Editing => "Enter: Confirm | Esc: Cancel",
            InputMode::SelectingModel => "↑↓/jk: Navigate | Enter: Select | Esc: Cancel",
        };
        (help.to_string(), Style::default().fg(Color::DarkGray))
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));

    let status = Paragraph::new(format!(" {} ", text))
        .style(style)
        .block(block);

    frame.render_widget(status, area);
}

/// Draw the main menu
fn draw_main_menu(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = MainMenuItem::all()
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let prefix = if i == app.main_menu_index {
                "▶ "
            } else {
                "  "
            };
            let style = if i == app.main_menu_index {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let content = format!("{}{:<12} {}", prefix, item.label(), item.description());
            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::NONE))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));

    // Center the menu vertically
    let inner = centered_rect(80, 50, area);
    frame.render_widget(list, inner);
}

/// Draw the providers screen
fn draw_providers(frame: &mut Frame, area: Rect, app: &App) {
    let inner = centered_rect(75, 70, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Header
            Constraint::Min(0),    // Settings list
        ])
        .split(inner);

    // Header showing current provider
    let current_provider = &app.settings.defaults.provider;
    let header = Paragraph::new(format!(
        "LLM Providers (Enter to toggle/edit, current: {})",
        current_provider
    ))
    .style(Style::default().fg(Color::Yellow));
    frame.render_widget(header, chunks[0]);

    // Settings list
    let api_key_display = if app.settings.get_anthropic_api_key().is_some() {
        "••••••••••••••••"
    } else {
        "(not set)"
    };

    let items: Vec<ListItem> = ProviderItem::all()
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let prefix = if i == app.provider_index {
                "▶ "
            } else {
                "  "
            };

            // Determine if this item belongs to the active provider
            let is_active_provider_item = match item {
                ProviderItem::DefaultProvider => true,
                ProviderItem::AnthropicApiKey | ProviderItem::AnthropicModel => {
                    current_provider == "anthropic"
                }
                ProviderItem::OllamaBaseUrl | ProviderItem::OllamaModel => {
                    current_provider == "ollama"
                }
                ProviderItem::OpenRouterApiKey | ProviderItem::OpenRouterModel => {
                    current_provider == "openrouter"
                }
                ProviderItem::BlackmanApiKey | ProviderItem::BlackmanModel => {
                    current_provider == "blackman"
                }
                ProviderItem::TestConnection | ProviderItem::Back => true,
            };

            let style = if i == app.provider_index {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if !is_active_provider_item {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };

            let value = match item {
                ProviderItem::DefaultProvider => {
                    format!("[{}]", current_provider)
                }
                ProviderItem::AnthropicApiKey => api_key_display.to_string(),
                ProviderItem::AnthropicModel => {
                    app.settings.providers.anthropic.default_model.clone()
                }
                ProviderItem::OllamaBaseUrl => app.settings.providers.ollama.base_url.clone(),
                ProviderItem::OllamaModel => app.settings.providers.ollama.default_model.clone(),
                ProviderItem::OpenRouterApiKey => {
                    if app.settings.get_openrouter_api_key().is_some() {
                        "••••••••••••••••".to_string()
                    } else {
                        "(not set)".to_string()
                    }
                }
                ProviderItem::OpenRouterModel => {
                    app.settings.providers.openrouter.default_model.clone()
                }
                ProviderItem::BlackmanApiKey => {
                    if app.settings.get_blackman_api_key().is_some() {
                        "••••••••••••••••".to_string()
                    } else {
                        "(not set)".to_string()
                    }
                }
                ProviderItem::BlackmanModel => {
                    app.settings.providers.blackman.default_model.clone()
                }
                ProviderItem::TestConnection => String::new(),
                ProviderItem::Back => String::new(),
            };

            let content = if value.is_empty() {
                format!("{}{}", prefix, item.label())
            } else {
                format!("{}{:<20} {}", prefix, item.label(), value)
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::NONE));
    frame.render_widget(list, chunks[1]);
}

/// Draw the caps screen
fn draw_caps(frame: &mut Frame, area: Rect, app: &App) {
    let inner = centered_rect(80, 80, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Header
            Constraint::Min(0),    // Caps list
            Constraint::Length(2), // Menu items
        ])
        .split(inner);

    // Header
    let header = Paragraph::new("Select default caps (Space/Enter to toggle)")
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(header, chunks[0]);

    // Caps list
    let cap_count = app.available_caps.len();
    let mut items: Vec<ListItem> = Vec::new();

    for (i, cap) in app.available_caps.iter().enumerate() {
        let prefix = if app.caps_index == i { "▶ " } else { "  " };
        let checkbox = if cap.is_enabled { "[x]" } else { "[ ]" };
        let builtin_tag = if cap.is_builtin { " [builtin]" } else { "" };
        let style = if app.caps_index == i {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if cap.is_enabled {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        let content = format!(
            "{}{} {:<20} {}{}",
            prefix, checkbox, cap.name, cap.description, builtin_tag
        );
        items.push(ListItem::new(content).style(style));
    }

    let list = List::new(items).block(Block::default().borders(Borders::NONE));
    frame.render_widget(list, chunks[1]);

    // Menu items at bottom
    let mut menu_items: Vec<ListItem> = Vec::new();

    // Create New
    let create_idx = cap_count;
    let create_prefix = if app.caps_index == create_idx {
        "▶ "
    } else {
        "  "
    };
    let create_style = if app.caps_index == create_idx {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    menu_items
        .push(ListItem::new(format!("{}+ Create New Cap", create_prefix)).style(create_style));

    // Back
    let back_idx = cap_count + 1;
    let back_prefix = if app.caps_index == back_idx {
        "▶ "
    } else {
        "  "
    };
    let back_style = if app.caps_index == back_idx {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    menu_items.push(ListItem::new(format!("{}← Back", back_prefix)).style(back_style));

    let menu_list = List::new(menu_items).block(Block::default().borders(Borders::NONE));
    frame.render_widget(menu_list, chunks[2]);
}

/// Draw the context settings screen
fn draw_context(frame: &mut Frame, area: Rect, app: &App) {
    let inner = centered_rect(70, 60, area);

    let items: Vec<ListItem> = ContextItem::all()
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let prefix = if i == app.context_index { "▶ " } else { "  " };
            let style = if i == app.context_index {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let value = match item {
                ContextItem::MaxWarmChunks => app.settings.context.max_warm_chunks.to_string(),
                ContextItem::ColdRetentionDays => {
                    app.settings.context.cold_retention_days.to_string()
                }
                ContextItem::AutoCompact => if app.settings.context.auto_compact {
                    "On"
                } else {
                    "Off"
                }
                .to_string(),
                ContextItem::Back => String::new(),
            };

            let content = if value.is_empty() {
                format!("{}{}", prefix, item.label())
            } else {
                format!("{}{:<22} {}", prefix, item.label(), value)
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::NONE));
    frame.render_widget(list, inner);
}

/// Draw the about screen
fn draw_about(frame: &mut Frame, area: Rect) {
    let inner = centered_rect(60, 50, area);

    let text = format!(
        r#"
ted v{}

AI coding assistant for your terminal

Written in Rust with ♥

Press Enter or q to go back
"#,
        env!("CARGO_PKG_VERSION")
    );

    let about = Paragraph::new(text)
        .style(Style::default())
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    frame.render_widget(about, inner);
}

/// Draw the plans browser screen
fn draw_plans(frame: &mut Frame, area: Rect, app: &App) {
    let inner = centered_rect(85, 85, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Header
            Constraint::Min(0),    // Plans list
            Constraint::Length(2), // Menu items
        ])
        .split(inner);

    // Header
    let header = Paragraph::new("Work plans (Enter to view, d to delete)")
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(header, chunks[0]);

    // Plans list
    let plan_count = app.available_plans.len();
    let mut items: Vec<ListItem> = Vec::new();

    if plan_count == 0 {
        items.push(
            ListItem::new("  No plans yet. Use the plan_update tool to create one.")
                .style(Style::default().fg(Color::DarkGray)),
        );
    } else {
        for (i, plan) in app.available_plans.iter().enumerate() {
            let prefix = if app.plans_index == i { "▶ " } else { "  " };

            // Status indicator
            let status_char = match plan.status {
                PlanStatus::Active => "[A]",
                PlanStatus::Paused => "[P]",
                PlanStatus::Complete => "[C]",
                PlanStatus::Archived => "[X]",
            };

            // Progress bar
            let progress = if plan.task_count > 0 {
                let filled = (plan.completed_count * 8) / plan.task_count;
                let empty = 8 - filled;
                format!(
                    "[{}{}] {}/{}",
                    "#".repeat(filled),
                    "-".repeat(empty),
                    plan.completed_count,
                    plan.task_count
                )
            } else {
                "[--------] 0/0".to_string()
            };

            // Time since modified
            let time_ago = format_time_ago(plan.modified_at);

            let style = if app.plans_index == i {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                match plan.status {
                    PlanStatus::Active => Style::default().fg(Color::Green),
                    PlanStatus::Paused => Style::default().fg(Color::Yellow),
                    PlanStatus::Complete => Style::default().fg(Color::DarkGray),
                    PlanStatus::Archived => Style::default().fg(Color::DarkGray),
                }
            };

            let content = format!(
                "{}{} {:<30} {} {:>6}",
                prefix, status_char, plan.title, progress, time_ago
            );
            items.push(ListItem::new(content).style(style));
        }
    }

    let list = List::new(items).block(Block::default().borders(Borders::NONE));
    frame.render_widget(list, chunks[1]);

    // Menu items at bottom
    let mut menu_items: Vec<ListItem> = Vec::new();

    // Back
    let back_idx = plan_count;
    let back_prefix = if app.plans_index == back_idx {
        "▶ "
    } else {
        "  "
    };
    let back_style = if app.plans_index == back_idx {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    menu_items.push(ListItem::new(format!("{}← Back", back_prefix)).style(back_style));

    let menu_list = List::new(menu_items).block(Block::default().borders(Borders::NONE));
    frame.render_widget(menu_list, chunks[2]);
}

/// Draw a single plan view
fn draw_plan_view(frame: &mut Frame, area: Rect, app: &App) {
    let inner = centered_rect(90, 90, area);

    // Get current plan title for header
    let title = app
        .available_plans
        .iter()
        .find(|p| Some(p.id) == app.current_plan_id)
        .map(|p| p.title.as_str())
        .unwrap_or("Plan");

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner_area = block.inner(inner);
    frame.render_widget(block, inner);

    // Split content into lines and apply scroll
    let lines: Vec<&str> = app.current_plan_content.lines().collect();
    let visible_height = inner_area.height as usize;

    // Clamp scroll to valid range
    let max_scroll = lines.len().saturating_sub(visible_height);
    let scroll = app.plan_scroll.min(max_scroll);

    let visible_lines: Vec<Line> = lines
        .iter()
        .skip(scroll)
        .take(visible_height)
        .map(|line| {
            // Style checkboxes
            if line.contains("- [x]") || line.contains("- [X]") {
                Line::from(Span::styled(*line, Style::default().fg(Color::Green)))
            } else if line.contains("- [ ]") {
                Line::from(Span::styled(*line, Style::default().fg(Color::White)))
            } else if line.starts_with('#') {
                Line::from(Span::styled(
                    *line,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(*line)
            }
        })
        .collect();

    let content = Paragraph::new(visible_lines);
    frame.render_widget(content, inner_area);
}

/// Draw the plan editor screen (vim-style)
fn draw_plan_edit(frame: &mut Frame, area: Rect, app: &App) {
    let Some(ref editor) = app.editor else {
        return;
    };

    // Get current plan title for header
    let title = app
        .available_plans
        .iter()
        .find(|p| Some(p.id) == app.current_plan_id)
        .map(|p| p.title.as_str())
        .unwrap_or("Plan");

    let modified_indicator = if editor.is_modified() { " [+]" } else { "" };

    let block = Block::default()
        .title(format!(" EDIT: {}{} ", title, modified_indicator))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    // Calculate visible area for editor
    let visible_height = inner_area.height.saturating_sub(1) as usize; // Leave room for status

    // Split inner area into editor and mode line
    let editor_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Editor content
            Constraint::Length(1), // Mode/command line
        ])
        .split(inner_area);

    // Render the editor content with line numbers
    let lines = editor.lines();
    let scroll = editor.scroll_offset();
    let (cursor_line, cursor_col) = editor.cursor();

    let mut text_lines: Vec<Line> = Vec::new();

    for (i, line) in lines.iter().skip(scroll).take(visible_height).enumerate() {
        let line_num = scroll + i + 1;
        let is_cursor_line = scroll + i == cursor_line;

        // Line number
        let num_style = if is_cursor_line {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        // Line content with syntax highlighting for checkboxes
        let content_style = if line.contains("- [x]") || line.contains("- [X]") {
            Style::default().fg(Color::Green)
        } else if line.contains("- [ ]") {
            Style::default().fg(Color::White)
        } else if line.starts_with('#') {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        // Build the line with line number
        text_lines.push(Line::from(vec![
            Span::styled(format!("{:>4} │ ", line_num), num_style),
            Span::styled(line.to_string(), content_style),
        ]));
    }

    let content = Paragraph::new(text_lines);
    frame.render_widget(content, editor_chunks[0]);

    // Draw cursor (if in visible area)
    if cursor_line >= scroll && cursor_line < scroll + visible_height {
        let cursor_y = editor_chunks[0].y + (cursor_line - scroll) as u16;
        let cursor_x = editor_chunks[0].x + 7 + cursor_col as u16; // 7 = line number width (4 + " │ ")

        if cursor_x < editor_chunks[0].x + editor_chunks[0].width {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    // Draw mode/command line
    let mode_line = match editor.mode() {
        EditorMode::Normal => Span::styled("-- NORMAL --", Style::default().fg(Color::Blue)),
        EditorMode::Insert => Span::styled("-- INSERT --", Style::default().fg(Color::Green)),
        EditorMode::Command => {
            let cmd = format!(":{}", editor.command_buffer());
            Span::styled(cmd, Style::default().fg(Color::Yellow))
        }
    };

    let help_text = match editor.mode() {
        EditorMode::Normal => " | :w save | :q quit | :wq save & quit | i insert",
        EditorMode::Insert => " | Esc to normal",
        EditorMode::Command => " | Enter to execute | Esc to cancel",
    };

    let mode_line_widget = Paragraph::new(Line::from(vec![
        mode_line,
        Span::styled(help_text, Style::default().fg(Color::DarkGray)),
    ]));

    frame.render_widget(mode_line_widget, editor_chunks[1]);
}

/// Format time difference from now
fn format_time_ago(time: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(time);

    if diff.num_minutes() < 1 {
        "now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h", diff.num_hours())
    } else if diff.num_days() < 7 {
        format!("{}d", diff.num_days())
    } else {
        format!("{}w", diff.num_weeks())
    }
}

/// Draw input popup for editing values
fn draw_input_popup(frame: &mut Frame, app: &App) {
    let popup_area = centered_rect(50, 20, frame.area());

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Draw the popup
    let block = Block::default()
        .title(" Edit Value ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Show current input
    let input_display = format!("{}_", app.input_buffer);
    let input = Paragraph::new(input_display).style(Style::default().fg(Color::White));

    frame.render_widget(input, inner);
}

/// Draw model picker popup for selecting models
fn draw_model_picker_popup(frame: &mut Frame, app: &App) {
    let popup_area = centered_rect(70, 70, frame.area());

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Determine title based on target
    let title = match app.model_selection_target {
        Some(ModelSelectionTarget::Anthropic) => " Select Anthropic Model ",
        Some(ModelSelectionTarget::Ollama) => " Select Ollama Model ",
        Some(ModelSelectionTarget::OpenRouter) => " Select OpenRouter Model ",
        Some(ModelSelectionTarget::Blackman) => " Select Blackman Model ",
        None => " Select Model ",
    };

    // Draw the popup
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Show loading indicator for Ollama
    if app.loading_ollama_models {
        let loading_msg = Paragraph::new("Loading models from Ollama...")
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(loading_msg, inner);
        return;
    }

    if app.available_models.is_empty() {
        let empty_msg = Paragraph::new("No models available for this provider")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty_msg, inner);
        return;
    }

    // Calculate visible area
    let visible_height = inner.height as usize;
    let total_models = app.available_models.len();

    // Adjust scroll to keep selected item visible
    let scroll = if app.model_picker_index < visible_height / 2 {
        0
    } else if app.model_picker_index + visible_height / 2 >= total_models {
        total_models.saturating_sub(visible_height)
    } else {
        app.model_picker_index.saturating_sub(visible_height / 2)
    };

    // Build list items
    let items: Vec<ListItem> = app
        .available_models
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible_height)
        .map(|(i, model)| {
            let is_selected = i == app.model_picker_index;
            let prefix = if is_selected { "▶ " } else { "  " };

            // Show tier indicator
            let tier_badge = match model.tier.as_str() {
                "High" => "[H]",
                "Medium" => "[M]",
                "Low" => "[L]",
                _ => "[?]",
            };

            // Show recommended badge
            let recommended = if model.recommended { " ★" } else { "" };

            // Style based on selection and tier
            let style = if is_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                match model.tier.as_str() {
                    "High" => Style::default().fg(Color::Green),
                    "Medium" => Style::default().fg(Color::Yellow),
                    "Low" => Style::default().fg(Color::White),
                    _ => Style::default(),
                }
            };

            // Format: [H] Model Name ★
            //         Description...
            let line1 = format!("{}{} {}{}", prefix, tier_badge, model.name, recommended);
            let line2 = if !model.description.is_empty() && is_selected {
                format!("     {}", model.description)
            } else {
                String::new()
            };

            if line2.is_empty() {
                ListItem::new(line1).style(style)
            } else {
                ListItem::new(vec![
                    Line::from(Span::styled(line1, style)),
                    Line::from(Span::styled(line2, Style::default().fg(Color::DarkGray))),
                ])
            }
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::NONE));
    frame.render_widget(list, inner);
}

/// Helper function to create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::tui::app::CapDisplayInfo;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    // ===== centered_rect Tests =====

    #[test]
    fn test_centered_rect_basic() {
        let area = Rect::new(0, 0, 100, 100);
        let result = centered_rect(50, 50, area);

        // With 50% width/height, the result should be centered
        // x: (100 - 50) / 2 = 25, width: 50
        // y: (100 - 50) / 2 = 25, height: 50
        assert!(result.x >= 20);
        assert!(result.y >= 20);
        assert!(result.width > 0);
        assert!(result.height > 0);
    }

    #[test]
    fn test_centered_rect_80_percent() {
        let area = Rect::new(0, 0, 100, 100);
        let result = centered_rect(80, 80, area);

        // With 80% width/height, the result should be mostly filling the area
        assert!(result.width >= 70);
        assert!(result.height >= 70);
    }

    #[test]
    fn test_centered_rect_small_area() {
        let area = Rect::new(0, 0, 20, 10);
        let result = centered_rect(50, 50, area);

        // Even with a small area, it should produce valid output
        assert!(result.x <= area.width);
        assert!(result.y <= area.height);
    }

    #[test]
    fn test_centered_rect_100_percent() {
        let area = Rect::new(0, 0, 100, 100);
        let result = centered_rect(100, 100, area);

        // With 100% width/height, the result should fill the entire area
        assert_eq!(result.width, area.width);
        assert_eq!(result.height, area.height);
    }

    #[test]
    fn test_centered_rect_offset_area() {
        let area = Rect::new(10, 10, 100, 100);
        let result = centered_rect(50, 50, area);

        // Result should be offset from the area's origin
        assert!(result.x >= area.x);
        assert!(result.y >= area.y);
    }

    #[test]
    fn test_centered_rect_asymmetric() {
        let area = Rect::new(0, 0, 200, 100);
        let result = centered_rect(60, 40, area);

        // Test with different x and y percentages
        assert!(result.width > result.height);
    }

    // ===== Draw Function Tests using TestBackend =====

    fn create_test_terminal() -> Terminal<TestBackend> {
        let backend = TestBackend::new(80, 24);
        Terminal::new(backend).unwrap()
    }

    #[test]
    fn test_draw_main_menu_screen() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let app = App::new(settings);

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw main menu");

        // Verify the buffer contains expected content
        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        // Should contain the title and menu items
        assert!(content.contains("ted settings"));
        assert!(content.contains("Providers"));
        assert!(content.contains("Caps"));
        assert!(content.contains("Context"));
        assert!(content.contains("About"));
    }

    #[test]
    fn test_draw_providers_screen() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(Screen::Providers);

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw providers screen");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        // Should contain provider-related content
        assert!(content.contains("Providers"));
        assert!(content.contains("Default Provider"));
        assert!(content.contains("Anthropic API Key"));
        assert!(content.contains("Anthropic Model"));
        assert!(content.contains("Ollama"));
    }

    #[test]
    fn test_draw_providers_screen_with_api_key_set() {
        let mut terminal = create_test_terminal();
        let mut settings = Settings::default();
        settings.providers.anthropic.api_key = Some("sk-test-key".to_string());
        let mut app = App::new(settings);
        app.go_to(Screen::Providers);

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw providers screen");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        // Should show masked API key (dots)
        assert!(content.contains("••••"));
    }

    #[test]
    fn test_draw_caps_screen() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(Screen::Caps);

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw caps screen");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        // Should contain caps-related content
        assert!(content.contains("Caps"));
        assert!(content.contains("Create New"));
        assert!(content.contains("Back"));
    }

    #[test]
    fn test_draw_context_screen() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(Screen::Context);

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw context screen");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        // Should contain context-related content
        assert!(content.contains("Context"));
        assert!(content.contains("Max Warm Chunks"));
        assert!(content.contains("Cold Retention"));
        assert!(content.contains("Auto Compact"));
    }

    #[test]
    fn test_draw_about_screen() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(Screen::About);

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw about screen");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        // Should contain about content
        assert!(content.contains("About"));
        assert!(content.contains("ted"));
        assert!(content.contains("AI coding assistant"));
    }

    #[test]
    fn test_draw_with_status_message() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.set_status("Test status message", false);

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw with status message");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        assert!(content.contains("Test status message"));
    }

    #[test]
    fn test_draw_with_error_status() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.set_status("Error occurred", true);

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw with error status");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        assert!(content.contains("Error occurred"));
    }

    #[test]
    fn test_draw_input_popup() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(Screen::Providers);
        app.provider_index = 0; // API Key
        app.start_editing("test-input");

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw input popup");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        // Should show the input popup with Edit Value title
        assert!(content.contains("Edit Value"));
        // Should show the current input buffer with cursor
        assert!(content.contains("test-input"));
    }

    #[test]
    fn test_draw_input_popup_empty_buffer() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.start_editing("");

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw input popup with empty buffer");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        assert!(content.contains("Edit Value"));
    }

    #[test]
    fn test_draw_main_menu_with_selection() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.main_menu_index = 2; // Context selected

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw main menu with selection");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        // Selection indicator should be present
        assert!(content.contains("▶"));
    }

    #[test]
    fn test_draw_providers_with_selection() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(Screen::Providers);
        app.provider_index = 1; // Model selected

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw providers with selection");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        assert!(content.contains("▶"));
    }

    #[test]
    fn test_draw_context_with_auto_compact_on() {
        let mut terminal = create_test_terminal();
        let mut settings = Settings::default();
        settings.context.auto_compact = true;
        let mut app = App::new(settings);
        app.go_to(Screen::Context);

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw context with auto_compact on");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        assert!(content.contains("On"));
    }

    #[test]
    fn test_draw_context_with_auto_compact_off() {
        let mut terminal = create_test_terminal();
        let mut settings = Settings::default();
        settings.context.auto_compact = false;
        let mut app = App::new(settings);
        app.go_to(Screen::Context);

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw context with auto_compact off");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        assert!(content.contains("Off"));
    }

    #[test]
    fn test_draw_caps_with_enabled_cap() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(Screen::Caps);

        // Manually enable first cap if available
        if !app.available_caps.is_empty() {
            app.available_caps[0].is_enabled = true;
        }

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw caps with enabled cap");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        // Should show checkbox checked for enabled caps
        if !app.available_caps.is_empty() {
            assert!(content.contains("[x]"));
        }
    }

    #[test]
    fn test_draw_caps_with_disabled_cap() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(Screen::Caps);

        // Manually disable first cap if available
        if !app.available_caps.is_empty() {
            app.available_caps[0].is_enabled = false;
        }

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw caps with disabled cap");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        // Should show unchecked checkbox
        if !app.available_caps.is_empty() {
            assert!(content.contains("[ ]"));
        }
    }

    #[test]
    fn test_draw_normal_mode_status() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let app = App::new(settings);
        // No status message set, should show default help

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw normal mode status");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        // Should show default navigation help
        assert!(content.contains("Navigate"));
    }

    #[test]
    fn test_draw_editing_mode_status() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.input_mode = InputMode::Editing;

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw editing mode status");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        // Should show editing mode help
        assert!(content.contains("Confirm") || content.contains("Cancel"));
    }

    #[test]
    fn test_draw_small_terminal() {
        // Test with a very small terminal size
        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let app = App::new(settings);

        // Should not panic even with small size
        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw on small terminal");
    }

    #[test]
    fn test_draw_large_terminal() {
        // Test with a large terminal size
        let backend = TestBackend::new(200, 60);
        let mut terminal = Terminal::new(backend).unwrap();
        let settings = Settings::default();
        let app = App::new(settings);

        // Should not panic with large size
        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw on large terminal");
    }

    #[test]
    fn test_draw_caps_screen_with_builtin_tag() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(Screen::Caps);

        // Add a builtin cap
        app.available_caps.push(CapDisplayInfo {
            name: "test-builtin".to_string(),
            description: "A builtin cap".to_string(),
            is_builtin: true,
            is_enabled: false,
        });

        terminal
            .draw(|f| draw(f, &app))
            .expect("Failed to draw caps with builtin");

        let buffer = terminal.backend().buffer();
        let content = buffer_to_string(buffer);

        assert!(content.contains("[builtin]"));
    }

    #[test]
    fn test_draw_title_for_each_screen() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);

        // Test MainMenu title
        app.go_to(Screen::MainMenu);
        terminal.draw(|f| draw(f, &app)).unwrap();
        let content = buffer_to_string(terminal.backend().buffer());
        assert!(content.contains("ted settings"));

        // Test Providers title
        app.go_to(Screen::Providers);
        terminal.draw(|f| draw(f, &app)).unwrap();
        let content = buffer_to_string(terminal.backend().buffer());
        assert!(content.contains("Providers"));

        // Test Caps title
        app.go_to(Screen::Caps);
        terminal.draw(|f| draw(f, &app)).unwrap();
        let content = buffer_to_string(terminal.backend().buffer());
        assert!(content.contains("Caps"));

        // Test Context title
        app.go_to(Screen::Context);
        terminal.draw(|f| draw(f, &app)).unwrap();
        let content = buffer_to_string(terminal.backend().buffer());
        assert!(content.contains("Context"));

        // Test About title
        app.go_to(Screen::About);
        terminal.draw(|f| draw(f, &app)).unwrap();
        let content = buffer_to_string(terminal.backend().buffer());
        assert!(content.contains("About"));
    }

    #[test]
    fn test_draw_context_values() {
        let mut terminal = create_test_terminal();
        let mut settings = Settings::default();
        settings.context.max_warm_chunks = 42;
        settings.context.cold_retention_days = 99;
        let mut app = App::new(settings);
        app.go_to(Screen::Context);

        terminal.draw(|f| draw(f, &app)).unwrap();
        let content = buffer_to_string(terminal.backend().buffer());

        assert!(content.contains("42"));
        assert!(content.contains("99"));
    }

    #[test]
    fn test_draw_provider_model_value() {
        let mut terminal = create_test_terminal();
        let mut settings = Settings::default();
        settings.providers.anthropic.default_model = "claude-test-model".to_string();
        let mut app = App::new(settings);
        app.go_to(Screen::Providers);

        terminal.draw(|f| draw(f, &app)).unwrap();
        let content = buffer_to_string(terminal.backend().buffer());

        assert!(content.contains("claude-test-model"));
    }

    #[test]
    fn test_draw_caps_create_new_selected() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(Screen::Caps);
        // Select "Create New" which is at index = available_caps.len()
        app.caps_index = app.available_caps.len();

        terminal.draw(|f| draw(f, &app)).unwrap();
        let content = buffer_to_string(terminal.backend().buffer());

        assert!(content.contains("Create New"));
    }

    #[test]
    fn test_draw_caps_back_selected() {
        let mut terminal = create_test_terminal();
        let settings = Settings::default();
        let mut app = App::new(settings);
        app.go_to(Screen::Caps);
        // Select "Back" which is the last item
        app.caps_index = app.caps_total_items() - 1;

        terminal.draw(|f| draw(f, &app)).unwrap();
        let content = buffer_to_string(terminal.backend().buffer());

        assert!(content.contains("Back"));
    }

    // Helper function to convert buffer to string for assertions
    fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
        let mut result = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = buffer.cell((x, y)).unwrap();
                result.push_str(cell.symbol());
            }
            result.push('\n');
        }
        result
    }
}
