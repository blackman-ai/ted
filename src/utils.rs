// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Utility functions for Ted
//!
//! This module contains pure functions extracted from main.rs for testability.

use crate::error::TedError;
use crossterm::style::Color;
use std::path::{Path, PathBuf};

/// Format a size in bytes to human-readable form
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Calculate the total size of all files in a directory
pub fn calculate_dir_size(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }

    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Find the project root by looking for common manifest files
///
/// Searches from the given directory upward for project markers.
pub fn find_project_root_from(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir;

    let manifest_files = [
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "go.mod",
        "pom.xml",
        "build.gradle",
        ".git",
    ];

    loop {
        for manifest in &manifest_files {
            if current.join(manifest).exists() {
                return Some(current.to_path_buf());
            }
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// Find the project root starting from current working directory
pub fn find_project_root() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| find_project_root_from(&cwd))
}

/// Format an error for display to the user
pub fn format_error(error: &TedError) -> String {
    match error {
        TedError::Api(api_error) => match api_error {
            crate::error::ApiError::ContextTooLong { current, limit } => {
                let mut msg = String::from("Context too long: ");
                if *current > 0 && *limit > 0 {
                    msg.push_str(&format!(
                        "{} tokens exceeds {} token limit.\n",
                        format_number(*current),
                        format_number(*limit)
                    ));
                } else {
                    msg.push_str("conversation exceeds the model's context window.\n");
                }
                msg.push_str("Try using /clear to reset the conversation, or remove some context.");
                msg
            }
            _ => format!("API Error: {}", api_error),
        },
        _ => format!("Error: {}", error),
    }
}

/// Format a number with thousand separators for readability
fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, c);
    }
    result
}

/// Get colors for a cap badge based on the cap name
///
/// Returns (background_color, foreground_color)
pub fn get_cap_colors(cap_name: &str) -> (Color, Color) {
    match cap_name {
        // Core/base caps - blue
        "base" => (Color::DarkBlue, Color::White),

        // Language-specific caps - green shades
        "rust-expert" => (
            Color::Rgb {
                r: 222,
                g: 165,
                b: 132,
            },
            Color::Black,
        ), // Rust orange-ish
        "python-senior" => (
            Color::Rgb {
                r: 55,
                g: 118,
                b: 171,
            },
            Color::White,
        ), // Python blue
        "typescript-expert" => (
            Color::Rgb {
                r: 49,
                g: 120,
                b: 198,
            },
            Color::White,
        ), // TS blue

        // Security/review caps - red/orange
        "security-analyst" => (Color::DarkRed, Color::White),
        "code-reviewer" => (
            Color::Rgb {
                r: 255,
                g: 140,
                b: 0,
            },
            Color::Black,
        ), // Orange

        // Documentation - purple
        "documentation" => (Color::Magenta, Color::White),

        // Default for custom caps - grey
        _ => (Color::DarkGrey, Color::White),
    }
}

/// Parse a session resume ID, supporting both short and full UUID forms
///
/// Returns the normalized session ID string or an error if invalid.
pub fn parse_session_id(resume_id: &str) -> Result<String, TedError> {
    if resume_id.is_empty() {
        return Err(TedError::InvalidInput(
            "Session ID cannot be empty".to_string(),
        ));
    }

    // Short form is allowed (will be matched against existing sessions)
    if resume_id.len() <= 8 {
        // Validate it's a valid hex prefix
        if resume_id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Ok(resume_id.to_string());
        }
        return Err(TedError::InvalidInput(
            "Invalid session ID: must be hexadecimal".to_string(),
        ));
    }

    // Full form - validate as UUID
    uuid::Uuid::parse_str(resume_id)
        .map(|u| u.to_string())
        .map_err(|_| TedError::InvalidInput("Invalid session ID format".to_string()))
}

/// Filter active caps for display, excluding "base"
pub fn filter_display_caps(caps: &[String]) -> Vec<&String> {
    caps.iter().filter(|c| *c != "base").collect()
}

/// Check if a command is an exit command
pub fn is_exit_command(input: &str) -> bool {
    let trimmed = input.trim().to_lowercase();
    matches!(trimmed.as_str(), "exit" | "quit" | "/exit" | "/quit")
}

/// Check if a command is a slash command
pub fn is_slash_command(input: &str) -> bool {
    input.trim().starts_with('/')
}

/// Parse a slash command into (command_name, arguments)
///
/// Returns None if the input is not a slash command.
pub fn parse_slash_command(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let without_slash = &trimmed[1..];
    match without_slash.find(char::is_whitespace) {
        Some(idx) => Some((&without_slash[..idx], without_slash[idx..].trim())),
        None => Some((without_slash, "")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== format_size tests ====================

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(2048), "2.0 KB");
        assert_eq!(format_size(1024 * 1023), "1023.0 KB");
    }

    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(1024 * 1024 * 5), "5.00 MB");
        assert_eq!(format_size(1024 * 1024 + 1024 * 512), "1.50 MB");
    }

    #[test]
    fn test_format_size_gigabytes() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_size(1024 * 1024 * 1024 * 2), "2.00 GB");
        assert_eq!(
            format_size(1024 * 1024 * 1024 + 1024 * 1024 * 512),
            "1.50 GB"
        );
    }

    #[test]
    fn test_format_size_boundary_values() {
        // Just under 1 KB
        assert_eq!(format_size(1023), "1023 B");
        // Exactly 1 KB
        assert_eq!(format_size(1024), "1.0 KB");
        // Just under 1 MB
        assert_eq!(format_size(1024 * 1024 - 1), "1024.0 KB");
        // Exactly 1 MB
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        // Just under 1 GB
        assert_eq!(format_size(1024 * 1024 * 1024 - 1), "1024.00 MB");
        // Exactly 1 GB
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
    }

    // ==================== calculate_dir_size tests ====================

    #[test]
    fn test_calculate_dir_size_nonexistent() {
        let path = Path::new("/nonexistent/path/that/does/not/exist");
        assert_eq!(calculate_dir_size(path), 0);
    }

    #[test]
    fn test_calculate_dir_size_empty_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        assert_eq!(calculate_dir_size(temp_dir.path()), 0);
    }

    #[test]
    fn test_calculate_dir_size_with_files() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create some files with known sizes
        std::fs::write(temp_dir.path().join("file1.txt"), "hello").unwrap(); // 5 bytes
        std::fs::write(temp_dir.path().join("file2.txt"), "world!").unwrap(); // 6 bytes

        assert_eq!(calculate_dir_size(temp_dir.path()), 11);
    }

    #[test]
    fn test_calculate_dir_size_nested_dirs() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create nested structure
        let subdir = temp_dir.path().join("subdir");
        std::fs::create_dir(&subdir).unwrap();

        std::fs::write(temp_dir.path().join("root.txt"), "abc").unwrap(); // 3 bytes
        std::fs::write(subdir.join("nested.txt"), "defgh").unwrap(); // 5 bytes

        assert_eq!(calculate_dir_size(temp_dir.path()), 8);
    }

    #[test]
    fn test_calculate_dir_size_deeply_nested() {
        let temp_dir = tempfile::tempdir().unwrap();

        // Create deeply nested structure
        let deep = temp_dir.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&deep).unwrap();

        std::fs::write(deep.join("deep.txt"), "deep content").unwrap(); // 12 bytes

        assert_eq!(calculate_dir_size(temp_dir.path()), 12);
    }

    // ==================== find_project_root_from tests ====================

    #[test]
    fn test_find_project_root_with_cargo_toml() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join("Cargo.toml"), "[package]").unwrap();

        let subdir = temp_dir.path().join("src");
        std::fs::create_dir(&subdir).unwrap();

        // Should find project root from subdir
        let result = find_project_root_from(&subdir);
        assert_eq!(result, Some(temp_dir.path().to_path_buf()));
    }

    #[test]
    fn test_find_project_root_with_package_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join("package.json"), "{}").unwrap();

        let result = find_project_root_from(temp_dir.path());
        assert_eq!(result, Some(temp_dir.path().to_path_buf()));
    }

    #[test]
    fn test_find_project_root_with_git() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp_dir.path().join(".git")).unwrap();

        let result = find_project_root_from(temp_dir.path());
        assert_eq!(result, Some(temp_dir.path().to_path_buf()));
    }

    #[test]
    fn test_find_project_root_with_pyproject() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join("pyproject.toml"), "[tool.poetry]").unwrap();

        let result = find_project_root_from(temp_dir.path());
        assert_eq!(result, Some(temp_dir.path().to_path_buf()));
    }

    #[test]
    fn test_find_project_root_with_go_mod() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join("go.mod"), "module example").unwrap();

        let result = find_project_root_from(temp_dir.path());
        assert_eq!(result, Some(temp_dir.path().to_path_buf()));
    }

    #[test]
    fn test_find_project_root_no_manifest() {
        let temp_dir = tempfile::tempdir().unwrap();
        // Create a directory with no manifest files
        let subdir = temp_dir.path().join("empty");
        std::fs::create_dir(&subdir).unwrap();

        // In a temp directory, it might find the actual system's project roots
        // so we test that it doesn't panic and returns Some or None
        let _ = find_project_root_from(&subdir);
    }

    // ==================== format_error tests ====================

    #[test]
    fn test_format_error_api_error() {
        use crate::error::ApiError;
        let error = TedError::Api(ApiError::RateLimited(60));
        let formatted = format_error(&error);
        assert!(formatted.starts_with("API Error:"));
        assert!(formatted.contains("Rate limited"));
    }

    #[test]
    fn test_format_error_config_error() {
        let error = TedError::Config("Missing key".to_string());
        let formatted = format_error(&error);
        assert!(formatted.starts_with("Error:"));
        assert!(formatted.contains("Missing key"));
    }

    #[test]
    fn test_format_error_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error = TedError::Io(io_err);
        let formatted = format_error(&error);
        assert!(formatted.starts_with("Error:"));
    }

    #[test]
    fn test_format_error_invalid_input() {
        let error = TedError::InvalidInput("bad input".to_string());
        let formatted = format_error(&error);
        assert!(formatted.contains("bad input"));
    }

    // ==================== get_cap_colors tests ====================

    #[test]
    fn test_get_cap_colors_base() {
        let (bg, fg) = get_cap_colors("base");
        assert_eq!(bg, Color::DarkBlue);
        assert_eq!(fg, Color::White);
    }

    #[test]
    fn test_get_cap_colors_rust_expert() {
        let (bg, fg) = get_cap_colors("rust-expert");
        assert!(matches!(
            bg,
            Color::Rgb {
                r: 222,
                g: 165,
                b: 132
            }
        ));
        assert_eq!(fg, Color::Black);
    }

    #[test]
    fn test_get_cap_colors_python_senior() {
        let (bg, fg) = get_cap_colors("python-senior");
        assert!(matches!(
            bg,
            Color::Rgb {
                r: 55,
                g: 118,
                b: 171
            }
        ));
        assert_eq!(fg, Color::White);
    }

    #[test]
    fn test_get_cap_colors_typescript_expert() {
        let (bg, fg) = get_cap_colors("typescript-expert");
        assert!(matches!(
            bg,
            Color::Rgb {
                r: 49,
                g: 120,
                b: 198
            }
        ));
        assert_eq!(fg, Color::White);
    }

    #[test]
    fn test_get_cap_colors_security_analyst() {
        let (bg, fg) = get_cap_colors("security-analyst");
        assert_eq!(bg, Color::DarkRed);
        assert_eq!(fg, Color::White);
    }

    #[test]
    fn test_get_cap_colors_code_reviewer() {
        let (bg, fg) = get_cap_colors("code-reviewer");
        assert!(matches!(
            bg,
            Color::Rgb {
                r: 255,
                g: 140,
                b: 0
            }
        ));
        assert_eq!(fg, Color::Black);
    }

    #[test]
    fn test_get_cap_colors_documentation() {
        let (bg, fg) = get_cap_colors("documentation");
        assert_eq!(bg, Color::Magenta);
        assert_eq!(fg, Color::White);
    }

    #[test]
    fn test_get_cap_colors_unknown() {
        let (bg, fg) = get_cap_colors("my-custom-cap");
        assert_eq!(bg, Color::DarkGrey);
        assert_eq!(fg, Color::White);
    }

    #[test]
    fn test_get_cap_colors_empty_string() {
        let (bg, fg) = get_cap_colors("");
        assert_eq!(bg, Color::DarkGrey);
        assert_eq!(fg, Color::White);
    }

    // ==================== parse_session_id tests ====================

    #[test]
    fn test_parse_session_id_short_valid() {
        let result = parse_session_id("abc123");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "abc123");
    }

    #[test]
    fn test_parse_session_id_short_with_dashes() {
        let result = parse_session_id("abc-123");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_session_id_full_uuid() {
        let result = parse_session_id("550e8400-e29b-41d4-a716-446655440000");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_parse_session_id_empty() {
        let result = parse_session_id("");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TedError::InvalidInput(_)));
    }

    #[test]
    fn test_parse_session_id_invalid_short() {
        let result = parse_session_id("xyz!@#");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_session_id_invalid_long() {
        let result = parse_session_id("not-a-valid-uuid-format-at-all");
        assert!(result.is_err());
    }

    // ==================== filter_display_caps tests ====================

    #[test]
    fn test_filter_display_caps_removes_base() {
        let caps = vec!["base".to_string(), "rust-expert".to_string()];
        let filtered = filter_display_caps(&caps);
        assert_eq!(filtered.len(), 1);
        assert_eq!(*filtered[0], "rust-expert");
    }

    #[test]
    fn test_filter_display_caps_empty() {
        let caps: Vec<String> = vec![];
        let filtered = filter_display_caps(&caps);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_display_caps_only_base() {
        let caps = vec!["base".to_string()];
        let filtered = filter_display_caps(&caps);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_display_caps_multiple() {
        let caps = vec![
            "base".to_string(),
            "rust-expert".to_string(),
            "security-analyst".to_string(),
        ];
        let filtered = filter_display_caps(&caps);
        assert_eq!(filtered.len(), 2);
    }

    // ==================== is_exit_command tests ====================

    #[test]
    fn test_is_exit_command_exit() {
        assert!(is_exit_command("exit"));
        assert!(is_exit_command("EXIT"));
        assert!(is_exit_command("Exit"));
        assert!(is_exit_command("  exit  "));
    }

    #[test]
    fn test_is_exit_command_quit() {
        assert!(is_exit_command("quit"));
        assert!(is_exit_command("QUIT"));
        assert!(is_exit_command("  quit  "));
    }

    #[test]
    fn test_is_exit_command_slash_exit() {
        assert!(is_exit_command("/exit"));
        assert!(is_exit_command("/EXIT"));
        assert!(is_exit_command("  /exit  "));
    }

    #[test]
    fn test_is_exit_command_slash_quit() {
        assert!(is_exit_command("/quit"));
        assert!(is_exit_command("/QUIT"));
    }

    #[test]
    fn test_is_exit_command_not_exit() {
        assert!(!is_exit_command("hello"));
        assert!(!is_exit_command("exiting"));
        assert!(!is_exit_command(""));
        assert!(!is_exit_command("/help"));
    }

    // ==================== is_slash_command tests ====================

    #[test]
    fn test_is_slash_command_true() {
        assert!(is_slash_command("/help"));
        assert!(is_slash_command("/settings"));
        assert!(is_slash_command("  /model  "));
    }

    #[test]
    fn test_is_slash_command_false() {
        assert!(!is_slash_command("help"));
        assert!(!is_slash_command(""));
        assert!(!is_slash_command("hello /world"));
    }

    // ==================== parse_slash_command tests ====================

    #[test]
    fn test_parse_slash_command_simple() {
        let result = parse_slash_command("/help");
        assert_eq!(result, Some(("help", "")));
    }

    #[test]
    fn test_parse_slash_command_with_args() {
        let result = parse_slash_command("/model sonnet");
        assert_eq!(result, Some(("model", "sonnet")));
    }

    #[test]
    fn test_parse_slash_command_with_multiple_args() {
        let result = parse_slash_command("/cap add rust-expert");
        assert_eq!(result, Some(("cap", "add rust-expert")));
    }

    #[test]
    fn test_parse_slash_command_with_whitespace() {
        let result = parse_slash_command("  /help  ");
        assert_eq!(result, Some(("help", "")));
    }

    #[test]
    fn test_parse_slash_command_not_slash() {
        let result = parse_slash_command("help");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_slash_command_empty() {
        let result = parse_slash_command("");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_slash_command_just_slash() {
        let result = parse_slash_command("/");
        assert_eq!(result, Some(("", "")));
    }

    // ==================== format_error context too long tests ====================

    #[test]
    fn test_format_error_context_too_long_with_limits() {
        use crate::error::ApiError;
        let error = TedError::Api(ApiError::ContextTooLong {
            current: 150000,
            limit: 100000,
        });
        let formatted = format_error(&error);
        assert!(formatted.contains("Context too long"));
        assert!(formatted.contains("150,000"));
        assert!(formatted.contains("100,000"));
        assert!(formatted.contains("/clear"));
    }

    #[test]
    fn test_format_error_context_too_long_without_limits() {
        use crate::error::ApiError;
        let error = TedError::Api(ApiError::ContextTooLong {
            current: 0,
            limit: 0,
        });
        let formatted = format_error(&error);
        assert!(formatted.contains("Context too long"));
        assert!(formatted.contains("exceeds the model's context window"));
    }

    #[test]
    fn test_format_error_context_too_long_current_zero() {
        use crate::error::ApiError;
        let error = TedError::Api(ApiError::ContextTooLong {
            current: 0,
            limit: 100000,
        });
        let formatted = format_error(&error);
        // When current is 0, falls through to generic message
        assert!(formatted.contains("exceeds the model's context window"));
    }

    #[test]
    fn test_format_error_context_too_long_limit_zero() {
        use crate::error::ApiError;
        let error = TedError::Api(ApiError::ContextTooLong {
            current: 100000,
            limit: 0,
        });
        let formatted = format_error(&error);
        // When limit is 0, falls through to generic message
        assert!(formatted.contains("exceeds the model's context window"));
    }

    // ==================== format_number tests ====================

    #[test]
    fn test_format_number_small() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(1), "1");
        assert_eq!(format_number(12), "12");
        assert_eq!(format_number(123), "123");
    }

    #[test]
    fn test_format_number_thousands() {
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234), "1,234");
        assert_eq!(format_number(12345), "12,345");
        assert_eq!(format_number(123456), "123,456");
    }

    #[test]
    fn test_format_number_millions() {
        assert_eq!(format_number(1000000), "1,000,000");
        assert_eq!(format_number(1234567), "1,234,567");
        assert_eq!(format_number(12345678), "12,345,678");
        assert_eq!(format_number(123456789), "123,456,789");
    }

    #[test]
    fn test_format_number_exact_thousands() {
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(999999), "999,999");
        assert_eq!(format_number(1000000), "1,000,000");
    }

    // ==================== find_project_root tests ====================

    #[test]
    fn test_find_project_root_from_current_crate() {
        // This test runs from within the ted project, so it should find the root
        let result = find_project_root();
        assert!(result.is_some());
        let root = result.unwrap();
        // The root should contain Cargo.toml
        assert!(root.join("Cargo.toml").exists());
    }

    // ==================== Additional format_error tests ====================

    #[test]
    fn test_format_error_generic_other_errors() {
        let error = TedError::InvalidInput("bad input".to_string());
        let formatted = format_error(&error);
        assert!(formatted.starts_with("Error:"));
        assert!(formatted.contains("bad input"));
    }

    // ==================== Additional parse_session_id edge cases ====================

    #[test]
    fn test_parse_session_id_exactly_8_chars() {
        // 8 chars is still considered short form
        let result = parse_session_id("abcd1234");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "abcd1234");
    }

    #[test]
    fn test_parse_session_id_9_chars_invalid() {
        // 9 chars is considered long form, must be valid UUID
        let result = parse_session_id("abcd12345");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_session_id_uppercase_hex() {
        let result = parse_session_id("ABCDEF");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_session_id_mixed_case_hex() {
        let result = parse_session_id("aBcDeF");
        assert!(result.is_ok());
    }

    // ==================== Additional is_slash_command edge cases ====================

    #[test]
    fn test_is_slash_command_only_whitespace() {
        assert!(!is_slash_command("   "));
    }

    #[test]
    fn test_is_slash_command_embedded_slash() {
        // Slash in the middle, not at start
        assert!(!is_slash_command("path/to/file"));
    }

    // ==================== Additional filter_display_caps tests ====================

    #[test]
    fn test_filter_display_caps_preserves_order() {
        let caps = vec![
            "rust-expert".to_string(),
            "base".to_string(),
            "security-analyst".to_string(),
        ];
        let filtered = filter_display_caps(&caps);
        assert_eq!(*filtered[0], "rust-expert");
        assert_eq!(*filtered[1], "security-analyst");
    }

    #[test]
    fn test_filter_display_caps_multiple_base() {
        // Even if "base" appears multiple times, all are filtered
        let caps = vec![
            "base".to_string(),
            "rust-expert".to_string(),
            "base".to_string(),
        ];
        let filtered = filter_display_caps(&caps);
        assert_eq!(filtered.len(), 1);
    }

    // ==================== Additional get_cap_colors tests ====================

    #[test]
    fn test_get_cap_colors_case_sensitive() {
        // Caps are case-sensitive
        let (bg_lower, _) = get_cap_colors("base");
        let (bg_upper, _) = get_cap_colors("BASE");
        assert_ne!(bg_lower, bg_upper); // BASE returns default color
    }

    #[test]
    fn test_get_cap_colors_with_special_chars() {
        let (bg, fg) = get_cap_colors("my-cap-123");
        assert_eq!(bg, Color::DarkGrey);
        assert_eq!(fg, Color::White);
    }
}
