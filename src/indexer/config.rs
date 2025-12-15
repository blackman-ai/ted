// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Configuration for the indexer daemon.
//!
//! Provides settings for file watching, scoring weights, and resource limits.
//! Configuration can be loaded from `~/.ted/config.toml` under the `[indexer]` section.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

use super::scorer::ScoringConfig;

/// Configuration for the indexer daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    /// Whether the indexer daemon is enabled.
    pub enabled: bool,

    /// File watch debounce interval in milliseconds.
    /// Changes within this window are batched together.
    pub debounce_ms: u64,

    /// How often to persist the index to disk (in seconds).
    pub persist_interval_secs: u64,

    /// Maximum number of files to process in a single batch.
    pub batch_size: usize,

    /// Scoring configuration.
    pub scoring: ScoringConfig,

    /// Resource limits.
    pub limits: LimitsConfig,

    /// Language-specific settings.
    pub languages: LanguagesConfig,

    /// Paths to ignore (in addition to defaults).
    pub ignore_patterns: Vec<String>,

    /// File extensions to index (empty = all code files).
    pub extensions: Vec<String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            debounce_ms: 500,
            persist_interval_secs: 60,
            batch_size: 100,
            scoring: ScoringConfig::default(),
            limits: LimitsConfig::default(),
            languages: LanguagesConfig::default(),
            ignore_patterns: Vec::new(),
            extensions: Vec::new(),
        }
    }
}

impl DaemonConfig {
    /// Get the debounce duration.
    pub fn debounce_duration(&self) -> Duration {
        Duration::from_millis(self.debounce_ms)
    }

    /// Get the persist interval duration.
    pub fn persist_interval(&self) -> Duration {
        Duration::from_secs(self.persist_interval_secs)
    }

    /// Merge with default ignore patterns.
    pub fn all_ignore_patterns(&self) -> Vec<String> {
        let mut patterns = vec![
            "node_modules".into(),
            "target".into(),
            ".git".into(),
            "dist".into(),
            "build".into(),
            "__pycache__".into(),
            ".venv".into(),
            "vendor".into(),
            ".idea".into(),
            ".vscode".into(),
            "*.lock".into(),
            "*.log".into(),
        ];
        patterns.extend(self.ignore_patterns.clone());
        patterns
    }

    /// Check if a path should be ignored.
    pub fn should_ignore(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        for pattern in self.all_ignore_patterns() {
            if let Some(suffix) = pattern.strip_prefix('*') {
                // Suffix match
                if path_str.ends_with(suffix) {
                    return true;
                }
            } else if path_str.contains(&pattern) {
                return true;
            }
        }
        false
    }

    /// Check if a file extension should be indexed.
    pub fn should_index_extension(&self, ext: &str) -> bool {
        if self.extensions.is_empty() {
            // Default: index common code files
            matches!(
                ext.to_lowercase().as_str(),
                "rs" | "ts"
                    | "tsx"
                    | "js"
                    | "jsx"
                    | "py"
                    | "go"
                    | "java"
                    | "c"
                    | "cpp"
                    | "h"
                    | "hpp"
                    | "cs"
                    | "rb"
                    | "swift"
                    | "kt"
                    | "php"
                    | "sh"
                    | "bash"
                    | "zsh"
                    | "toml"
                    | "yaml"
                    | "yml"
                    | "json"
                    | "md"
            )
        } else {
            self.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
        }
    }
}

/// Resource limits configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitsConfig {
    /// Maximum number of files to keep in hot context.
    pub max_files: usize,

    /// Maximum bytes for hot context.
    pub max_bytes: u64,

    /// Decay half-life in hours.
    pub decay_half_life_hours: f64,

    /// Maximum file size to index (bytes).
    pub max_file_size: u64,

    /// Maximum number of files to track in the index.
    pub max_indexed_files: usize,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_files: 50,
            max_bytes: 102400, // 100KB
            decay_half_life_hours: 24.0,
            max_file_size: 1024 * 1024, // 1MB
            max_indexed_files: 10000,
        }
    }
}

/// Language-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LanguagesConfig {
    /// Rust file extensions.
    pub rust: Vec<String>,

    /// TypeScript/JavaScript file extensions.
    pub typescript: Vec<String>,

    /// Python file extensions.
    pub python: Vec<String>,

    /// Go file extensions.
    pub go: Vec<String>,
}

impl Default for LanguagesConfig {
    fn default() -> Self {
        Self {
            rust: vec!["rs".into()],
            typescript: vec![
                "ts".into(),
                "tsx".into(),
                "js".into(),
                "jsx".into(),
                "mjs".into(),
                "cjs".into(),
            ],
            python: vec!["py".into(), "pyi".into()],
            go: vec!["go".into()],
        }
    }
}

/// Events that the daemon can emit.
#[derive(Debug, Clone)]
pub enum DaemonEvent {
    /// A file was created.
    FileCreated(std::path::PathBuf),

    /// A file was modified.
    FileModified(std::path::PathBuf),

    /// A file was deleted.
    FileDeleted(std::path::PathBuf),

    /// A file was renamed.
    FileRenamed {
        from: std::path::PathBuf,
        to: std::path::PathBuf,
    },

    /// The index was persisted to disk.
    IndexPersisted,

    /// An error occurred.
    Error(String),

    /// The daemon was stopped.
    Stopped,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_config_default() {
        let config = DaemonConfig::default();

        assert!(config.enabled);
        assert_eq!(config.debounce_ms, 500);
        assert_eq!(config.persist_interval_secs, 60);
        assert_eq!(config.batch_size, 100);
    }

    #[test]
    fn test_debounce_duration() {
        let config = DaemonConfig {
            debounce_ms: 1000,
            ..Default::default()
        };

        assert_eq!(config.debounce_duration(), Duration::from_secs(1));
    }

    #[test]
    fn test_persist_interval() {
        let config = DaemonConfig {
            persist_interval_secs: 120,
            ..Default::default()
        };

        assert_eq!(config.persist_interval(), Duration::from_secs(120));
    }

    #[test]
    fn test_all_ignore_patterns() {
        let config = DaemonConfig {
            ignore_patterns: vec!["custom_dir".into()],
            ..Default::default()
        };

        let patterns = config.all_ignore_patterns();

        assert!(patterns.contains(&"node_modules".to_string()));
        assert!(patterns.contains(&"target".to_string()));
        assert!(patterns.contains(&"custom_dir".to_string()));
    }

    #[test]
    fn test_should_ignore() {
        let config = DaemonConfig::default();

        assert!(config.should_ignore(Path::new("node_modules/lib.js")));
        assert!(config.should_ignore(Path::new("target/debug/main")));
        assert!(config.should_ignore(Path::new(".git/config")));
        assert!(config.should_ignore(Path::new("Cargo.lock"))); // *.lock
        assert!(!config.should_ignore(Path::new("src/main.rs")));
    }

    #[test]
    fn test_should_index_extension_default() {
        let config = DaemonConfig::default();

        assert!(config.should_index_extension("rs"));
        assert!(config.should_index_extension("ts"));
        assert!(config.should_index_extension("py"));
        assert!(config.should_index_extension("go"));
        assert!(!config.should_index_extension("png"));
        assert!(!config.should_index_extension("exe"));
    }

    #[test]
    fn test_should_index_extension_custom() {
        let config = DaemonConfig {
            extensions: vec!["rs".into(), "toml".into()],
            ..Default::default()
        };

        assert!(config.should_index_extension("rs"));
        assert!(config.should_index_extension("toml"));
        assert!(!config.should_index_extension("ts"));
        assert!(!config.should_index_extension("py"));
    }

    #[test]
    fn test_limits_config_default() {
        let limits = LimitsConfig::default();

        assert_eq!(limits.max_files, 50);
        assert_eq!(limits.max_bytes, 102400);
        assert_eq!(limits.decay_half_life_hours, 24.0);
        assert_eq!(limits.max_file_size, 1024 * 1024);
    }

    #[test]
    fn test_languages_config_default() {
        let languages = LanguagesConfig::default();

        assert!(languages.rust.contains(&"rs".to_string()));
        assert!(languages.typescript.contains(&"ts".to_string()));
        assert!(languages.python.contains(&"py".to_string()));
        assert!(languages.go.contains(&"go".to_string()));
    }
}
