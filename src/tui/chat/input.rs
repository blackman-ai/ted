// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Input handling for the chat TUI
//!
//! Handles keyboard events and translates them to application actions.

// Input handling is mostly done in app.rs handle_key methods
// This module provides additional utilities

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Key binding description for help display
#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub keys: &'static str,
    pub description: &'static str,
}

/// Get key bindings for a given mode
pub fn bindings_for_mode(mode: super::ChatMode) -> Vec<KeyBinding> {
    match mode {
        super::ChatMode::Input => vec![
            KeyBinding {
                keys: "Enter",
                description: "Send message",
            },
            KeyBinding {
                keys: "↑/↓",
                description: "History navigation",
            },
            KeyBinding {
                keys: "←/→",
                description: "Move cursor",
            },
            KeyBinding {
                keys: "Ctrl+A/E",
                description: "Start/End of line",
            },
            KeyBinding {
                keys: "Ctrl+W",
                description: "Delete word",
            },
            KeyBinding {
                keys: "Ctrl+U",
                description: "Clear input",
            },
            KeyBinding {
                keys: "Tab",
                description: "Toggle agent pane",
            },
            KeyBinding {
                keys: "Esc",
                description: "Normal mode",
            },
            KeyBinding {
                keys: "Ctrl+C",
                description: "Cancel/Quit",
            },
        ],
        super::ChatMode::Normal => vec![
            KeyBinding {
                keys: "Enter/i",
                description: "Input mode",
            },
            KeyBinding {
                keys: "j/k or ↑/↓",
                description: "Scroll",
            },
            KeyBinding {
                keys: "g/G",
                description: "Top/Bottom",
            },
            KeyBinding {
                keys: "Tab",
                description: "Toggle agent pane",
            },
            KeyBinding {
                keys: "Ctrl+A",
                description: "Focus agents",
            },
            KeyBinding {
                keys: "?",
                description: "Help",
            },
            KeyBinding {
                keys: "q",
                description: "Quit",
            },
        ],
        super::ChatMode::AgentFocus => vec![
            KeyBinding {
                keys: "j/k or ↑/↓",
                description: "Navigate agents",
            },
            KeyBinding {
                keys: "Enter",
                description: "View details",
            },
            KeyBinding {
                keys: "c",
                description: "Cancel agent",
            },
            KeyBinding {
                keys: "Esc/Tab",
                description: "Exit",
            },
        ],
        super::ChatMode::Help => vec![KeyBinding {
            keys: "Esc/q/?",
            description: "Close help",
        }],
        super::ChatMode::CommandPalette => vec![KeyBinding {
            keys: "Esc",
            description: "Close",
        }],
        super::ChatMode::Confirm => vec![
            KeyBinding {
                keys: "y/Enter",
                description: "Confirm",
            },
            KeyBinding {
                keys: "n/Esc",
                description: "Cancel",
            },
        ],
        super::ChatMode::Settings => vec![
            KeyBinding {
                keys: "↑/↓",
                description: "Navigate",
            },
            KeyBinding {
                keys: "Enter",
                description: "Edit/Toggle",
            },
            KeyBinding {
                keys: "Space",
                description: "Toggle",
            },
            KeyBinding {
                keys: "s",
                description: "Save",
            },
            KeyBinding {
                keys: "Esc",
                description: "Close",
            },
        ],
    }
}

/// Format a key event for display
pub fn format_key_event(key: &KeyEvent) -> String {
    let mut parts = Vec::new();

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("Ctrl");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("Shift");
    }

    let key_str = match key.code {
        KeyCode::Char(c) => c.to_uppercase().to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Up => "↑".to_string(),
        KeyCode::Down => "↓".to_string(),
        KeyCode::Left => "←".to_string(),
        KeyCode::Right => "→".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PgUp".to_string(),
        KeyCode::PageDown => "PgDn".to_string(),
        KeyCode::F(n) => format!("F{}", n),
        _ => "?".to_string(),
    };

    parts.push(&key_str);
    parts.join("+")
}

/// Check if a key combination is a "submit" action
pub fn is_submit_key(key: &KeyEvent) -> bool {
    matches!(
        (key.modifiers, key.code),
        (KeyModifiers::NONE, KeyCode::Enter) | (KeyModifiers::CONTROL, KeyCode::Enter)
    )
}

/// Check if a key is a navigation key (shouldn't be inserted as text)
pub fn is_navigation_key(key: &KeyEvent) -> bool {
    matches!(
        key.code,
        KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Tab
            | KeyCode::Esc
            | KeyCode::Enter
            | KeyCode::Backspace
            | KeyCode::Delete
    ) || key.modifiers.contains(KeyModifiers::CONTROL)
        || key.modifiers.contains(KeyModifiers::ALT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bindings_for_mode() {
        let input_bindings = bindings_for_mode(super::super::ChatMode::Input);
        assert!(!input_bindings.is_empty());

        let normal_bindings = bindings_for_mode(super::super::ChatMode::Normal);
        assert!(!normal_bindings.is_empty());
    }

    #[test]
    fn test_bindings_for_all_modes() {
        use super::super::ChatMode;

        let modes = [
            ChatMode::Input,
            ChatMode::Normal,
            ChatMode::AgentFocus,
            ChatMode::Help,
            ChatMode::CommandPalette,
            ChatMode::Confirm,
            ChatMode::Settings,
        ];

        for mode in modes {
            let bindings = bindings_for_mode(mode);
            assert!(!bindings.is_empty(), "Mode {:?} should have bindings", mode);
        }
    }

    #[test]
    fn test_bindings_input_mode() {
        let bindings = bindings_for_mode(super::super::ChatMode::Input);
        let keys: Vec<&str> = bindings.iter().map(|b| b.keys).collect();
        assert!(keys.contains(&"Enter"));
        assert!(keys.contains(&"Esc"));
    }

    #[test]
    fn test_bindings_normal_mode() {
        let bindings = bindings_for_mode(super::super::ChatMode::Normal);
        let keys: Vec<&str> = bindings.iter().map(|b| b.keys).collect();
        assert!(keys.contains(&"q"));
    }

    #[test]
    fn test_bindings_agent_focus_mode() {
        let bindings = bindings_for_mode(super::super::ChatMode::AgentFocus);
        let keys: Vec<&str> = bindings.iter().map(|b| b.keys).collect();
        assert!(keys.contains(&"c"));
    }

    #[test]
    fn test_bindings_help_mode() {
        let bindings = bindings_for_mode(super::super::ChatMode::Help);
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn test_bindings_command_palette_mode() {
        let bindings = bindings_for_mode(super::super::ChatMode::CommandPalette);
        assert_eq!(bindings.len(), 1);
    }

    #[test]
    fn test_bindings_confirm_mode() {
        let bindings = bindings_for_mode(super::super::ChatMode::Confirm);
        assert_eq!(bindings.len(), 2);
    }

    #[test]
    fn test_bindings_settings_mode() {
        let bindings = bindings_for_mode(super::super::ChatMode::Settings);
        let keys: Vec<&str> = bindings.iter().map(|b| b.keys).collect();
        assert!(keys.contains(&"s"));
    }

    #[test]
    fn test_key_binding_debug() {
        let binding = KeyBinding {
            keys: "Enter",
            description: "Submit",
        };
        let debug_str = format!("{:?}", binding);
        assert!(debug_str.contains("Enter"));
        assert!(debug_str.contains("Submit"));
    }

    #[test]
    fn test_key_binding_clone() {
        let binding = KeyBinding {
            keys: "Tab",
            description: "Toggle",
        };
        let cloned = binding.clone();
        assert_eq!(cloned.keys, "Tab");
        assert_eq!(cloned.description, "Toggle");
    }

    #[test]
    fn test_format_key_event() {
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(format_key_event(&key), "Ctrl+A");

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(format_key_event(&key), "Enter");
    }

    #[test]
    fn test_format_key_event_all_modifiers() {
        let key = KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT,
        );
        let result = format_key_event(&key);
        assert!(result.contains("Ctrl"));
        assert!(result.contains("Alt"));
        assert!(result.contains("Shift"));
    }

    #[test]
    fn test_format_key_event_special_keys() {
        let tests = vec![
            (KeyCode::Esc, "Esc"),
            (KeyCode::Tab, "Tab"),
            (KeyCode::Backspace, "Backspace"),
            (KeyCode::Delete, "Delete"),
            (KeyCode::Up, "↑"),
            (KeyCode::Down, "↓"),
            (KeyCode::Left, "←"),
            (KeyCode::Right, "→"),
            (KeyCode::Home, "Home"),
            (KeyCode::End, "End"),
            (KeyCode::PageUp, "PgUp"),
            (KeyCode::PageDown, "PgDn"),
        ];

        for (code, expected) in tests {
            let key = KeyEvent::new(code, KeyModifiers::NONE);
            assert_eq!(format_key_event(&key), expected);
        }
    }

    #[test]
    fn test_format_key_event_function_keys() {
        let key = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        assert_eq!(format_key_event(&key), "F1");

        let key = KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE);
        assert_eq!(format_key_event(&key), "F12");
    }

    #[test]
    fn test_format_key_event_unknown() {
        let key = KeyEvent::new(KeyCode::Insert, KeyModifiers::NONE);
        assert_eq!(format_key_event(&key), "?");
    }

    #[test]
    fn test_is_submit_key() {
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert!(is_submit_key(&enter));

        let char_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(!is_submit_key(&char_a));
    }

    #[test]
    fn test_is_submit_key_ctrl_enter() {
        let ctrl_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL);
        assert!(is_submit_key(&ctrl_enter));
    }

    #[test]
    fn test_is_submit_key_shift_enter() {
        let shift_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT);
        assert!(!is_submit_key(&shift_enter));
    }

    #[test]
    fn test_is_navigation_key() {
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert!(is_navigation_key(&up));

        let char_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(!is_navigation_key(&char_a));

        let ctrl_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert!(is_navigation_key(&ctrl_a));
    }

    #[test]
    fn test_is_navigation_key_all_nav_keys() {
        let nav_keys = vec![
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Tab,
            KeyCode::Esc,
            KeyCode::Enter,
            KeyCode::Backspace,
            KeyCode::Delete,
        ];

        for code in nav_keys {
            let key = KeyEvent::new(code, KeyModifiers::NONE);
            assert!(
                is_navigation_key(&key),
                "{:?} should be navigation key",
                code
            );
        }
    }

    #[test]
    fn test_is_navigation_key_with_alt() {
        let alt_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT);
        assert!(is_navigation_key(&alt_a));
    }

    #[test]
    fn test_is_navigation_key_regular_chars() {
        for c in 'a'..='z' {
            let key = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            assert!(!is_navigation_key(&key));
        }
    }

    #[test]
    fn test_is_navigation_key_shift_char() {
        // Shift+A should not be navigation (it's uppercase letter)
        let shift_a = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT);
        assert!(!is_navigation_key(&shift_a));
    }
}
