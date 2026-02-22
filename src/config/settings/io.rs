// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::path::PathBuf;

use crate::error::Result;

use super::migration;
use super::Settings;

impl Settings {
    /// Get the default settings file path.
    pub fn default_path() -> PathBuf {
        Self::ted_home().join("settings.json")
    }

    /// Load settings from the default path.
    pub fn load() -> Result<Self> {
        Self::load_from(&Self::default_path())
    }

    /// Load settings from a specific path.
    pub fn load_from(path: &PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)?;
        let raw_value: serde_json::Value = serde_json::from_str(&content)?;
        let migrated = migration::migrate_on_load(raw_value);
        let settings: Settings = serde_json::from_value(migrated)?;
        Ok(settings)
    }

    /// Save settings to the default path.
    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::default_path())
    }

    /// Save settings to a specific path, merging with existing file content
    /// to preserve unknown keys from other code versions or hand edits.
    pub fn save_to(&self, path: &PathBuf) -> Result<()> {
        // Ensure parent directory exists.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Serialize current struct to Value.
        let new_value = serde_json::to_value(self)?;

        // If an existing file exists, load it and deep-merge.
        let merged = if path.exists() {
            let existing_content = std::fs::read_to_string(path)?;
            match serde_json::from_str::<serde_json::Value>(&existing_content) {
                Ok(existing_value) => migration::deep_merge(existing_value, new_value),
                Err(_) => new_value, // Corrupt file, overwrite entirely.
            }
        } else {
            new_value
        };

        let content = serde_json::to_string_pretty(&merged)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Save settings to the default path, fully overwriting (no merge).
    /// Used for explicit resets.
    pub fn save_clean(&self) -> Result<()> {
        self.save_to_clean(&Self::default_path())
    }

    /// Save settings to a specific path, fully overwriting (no merge).
    pub fn save_to_clean(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Get the ted home directory (~/.ted or $TED_HOME).
    pub fn ted_home() -> PathBuf {
        if let Ok(home) = std::env::var("TED_HOME") {
            return PathBuf::from(home);
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ted")
    }

    /// Get the caps directory.
    pub fn caps_dir() -> PathBuf {
        Self::ted_home().join("caps")
    }

    /// Get the commands directory.
    pub fn commands_dir() -> PathBuf {
        Self::ted_home().join("commands")
    }

    /// Get the history directory.
    pub fn history_dir() -> PathBuf {
        Self::ted_home().join("history")
    }

    /// Get the context storage directory.
    pub fn context_path() -> PathBuf {
        Self::ted_home().join("context")
    }

    /// Get the plans directory.
    pub fn plans_dir() -> PathBuf {
        Self::ted_home().join("plans")
    }

    /// Ensure all required directories exist.
    pub fn ensure_directories() -> Result<()> {
        let mut dirs = vec![
            Self::ted_home(),
            Self::caps_dir(),
            Self::commands_dir(),
            Self::context_path(),
            Self::plans_dir(),
        ];

        // Add the parent directory of the default settings path if it exists.
        if let Some(parent) = Self::default_path().parent() {
            dirs.push(parent.to_path_buf());
        }

        for dir in dirs {
            if !dir.exists() {
                std::fs::create_dir_all(&dir)?;
            }
        }

        Ok(())
    }
}
