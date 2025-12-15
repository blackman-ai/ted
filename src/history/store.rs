// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! History store implementation
//!
//! Stores session metadata in a JSON index file for quick access.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

use crate::config::Settings;
use crate::error::Result;

/// Information about a session stored in history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Session ID
    pub id: Uuid,
    /// When the session started
    pub started_at: DateTime<Utc>,
    /// When the session was last active
    pub last_active: DateTime<Utc>,
    /// Working directory when session started
    pub working_directory: PathBuf,
    /// Project root (if detected)
    pub project_root: Option<PathBuf>,
    /// Number of messages in the session
    pub message_count: usize,
    /// Brief summary or first user message
    pub summary: Option<String>,
    /// Caps that were active
    pub caps: Vec<String>,
}

impl SessionInfo {
    /// Create a new session info
    pub fn new(id: Uuid, working_directory: PathBuf) -> Self {
        let now = Utc::now();
        Self {
            id,
            started_at: now,
            last_active: now,
            working_directory,
            project_root: None,
            message_count: 0,
            summary: None,
            caps: vec![],
        }
    }

    /// Update the last active timestamp
    pub fn touch(&mut self) {
        self.last_active = Utc::now();
    }

    /// Set the summary (usually first user message)
    pub fn set_summary(&mut self, summary: impl Into<String>) {
        let s: String = summary.into();
        // Truncate to first 100 chars
        self.summary = Some(if s.len() > 100 {
            format!("{}...", &s[..97])
        } else {
            s
        });
    }
}

/// History store for managing session metadata
pub struct HistoryStore {
    /// Path to the history index file
    index_path: PathBuf,
    /// Cached sessions
    sessions: Vec<SessionInfo>,
}

impl HistoryStore {
    /// Open or create a history store
    pub fn open() -> Result<Self> {
        let index_path = Settings::ted_home().join("history.json");

        let sessions = if index_path.exists() {
            let content = std::fs::read_to_string(&index_path)?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(Self {
            index_path,
            sessions,
        })
    }

    /// Save the history index
    fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.sessions)?;
        std::fs::write(&self.index_path, content)?;
        Ok(())
    }

    /// Add or update a session
    pub fn upsert(&mut self, session: SessionInfo) -> Result<()> {
        if let Some(existing) = self.sessions.iter_mut().find(|s| s.id == session.id) {
            *existing = session;
        } else {
            self.sessions.push(session);
        }
        self.save()
    }

    /// Get a session by ID
    pub fn get(&self, id: Uuid) -> Option<&SessionInfo> {
        self.sessions.iter().find(|s| s.id == id)
    }

    /// List recent sessions
    pub fn list_recent(&self, limit: usize) -> Vec<&SessionInfo> {
        let mut sorted: Vec<_> = self.sessions.iter().collect();
        sorted.sort_by(|a, b| b.last_active.cmp(&a.last_active));
        sorted.into_iter().take(limit).collect()
    }

    /// Search sessions by summary content
    pub fn search(&self, query: &str) -> Vec<&SessionInfo> {
        let query_lower = query.to_lowercase();
        self.sessions
            .iter()
            .filter(|s| {
                s.summary
                    .as_ref()
                    .map(|sum| sum.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
                    || s.working_directory
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&query_lower)
            })
            .collect()
    }

    /// Delete a session from history
    pub fn delete(&mut self, id: Uuid) -> Result<bool> {
        let initial_len = self.sessions.len();
        self.sessions.retain(|s| s.id != id);

        if self.sessions.len() < initial_len {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Clean up old sessions (older than days_to_keep)
    pub fn cleanup(&mut self, days_to_keep: i64) -> Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::days(days_to_keep);
        let initial_len = self.sessions.len();

        self.sessions.retain(|s| s.last_active > cutoff);

        let removed = initial_len - self.sessions.len();
        if removed > 0 {
            self.save()?;
        }

        Ok(removed)
    }

    /// Get all sessions for a specific working directory
    pub fn sessions_for_directory(&self, dir: &PathBuf) -> Vec<&SessionInfo> {
        self.sessions
            .iter()
            .filter(|s| &s.working_directory == dir)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_session_info_creation() {
        let id = Uuid::new_v4();
        let session = SessionInfo::new(id, PathBuf::from("/tmp/test"));

        assert_eq!(session.id, id);
        assert_eq!(session.message_count, 0);
        assert!(session.summary.is_none());
        assert!(session.caps.is_empty());
        assert!(session.project_root.is_none());
    }

    #[test]
    fn test_session_info_touch() {
        let id = Uuid::new_v4();
        let mut session = SessionInfo::new(id, PathBuf::from("/tmp/test"));
        let original_time = session.last_active;

        // Small delay to ensure time changes
        std::thread::sleep(std::time::Duration::from_millis(10));
        session.touch();

        assert!(session.last_active >= original_time);
    }

    #[test]
    fn test_session_summary_short() {
        let id = Uuid::new_v4();
        let mut session = SessionInfo::new(id, PathBuf::from("/tmp/test"));

        session.set_summary("Short summary");
        assert_eq!(session.summary.as_ref().unwrap(), "Short summary");
    }

    #[test]
    fn test_session_summary_truncation() {
        let id = Uuid::new_v4();
        let mut session = SessionInfo::new(id, PathBuf::from("/tmp/test"));

        let long_summary = "a".repeat(200);
        session.set_summary(&long_summary);

        assert!(session.summary.as_ref().unwrap().len() <= 103); // 97 + "..."
        assert!(session.summary.as_ref().unwrap().ends_with("..."));
    }

    #[test]
    fn test_session_summary_exactly_100() {
        let id = Uuid::new_v4();
        let mut session = SessionInfo::new(id, PathBuf::from("/tmp/test"));

        let exactly_100 = "a".repeat(100);
        session.set_summary(&exactly_100);

        // Should not be truncated since it's exactly 100
        assert_eq!(session.summary.as_ref().unwrap().len(), 100);
        assert!(!session.summary.as_ref().unwrap().ends_with("..."));
    }

    #[test]
    fn test_session_info_with_caps() {
        let id = Uuid::new_v4();
        let mut session = SessionInfo::new(id, PathBuf::from("/tmp/test"));
        session.caps = vec!["rust-expert".to_string(), "security-analyst".to_string()];

        assert_eq!(session.caps.len(), 2);
        assert!(session.caps.contains(&"rust-expert".to_string()));
    }

    #[test]
    fn test_session_info_serialization() {
        let id = Uuid::new_v4();
        let mut session = SessionInfo::new(id, PathBuf::from("/tmp/test"));
        session.set_summary("Test summary");
        session.message_count = 5;

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: SessionInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, session.id);
        assert_eq!(deserialized.message_count, 5);
        assert_eq!(deserialized.summary, session.summary);
    }

    #[test]
    fn test_session_info_with_project_root() {
        let id = Uuid::new_v4();
        let mut session = SessionInfo::new(id, PathBuf::from("/tmp/test"));
        session.project_root = Some(PathBuf::from("/home/user/project"));

        assert_eq!(
            session.project_root,
            Some(PathBuf::from("/home/user/project"))
        );
    }

    // HistoryStore tests with temp directory

    fn create_test_store(temp_dir: &TempDir) -> HistoryStore {
        HistoryStore {
            index_path: temp_dir.path().join("history.json"),
            sessions: Vec::new(),
        }
    }

    #[test]
    fn test_history_store_upsert_new() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = create_test_store(&temp_dir);

        let id = Uuid::new_v4();
        let session = SessionInfo::new(id, PathBuf::from("/test"));
        store.upsert(session).unwrap();

        assert_eq!(store.sessions.len(), 1);
        assert!(store.get(id).is_some());
    }

    #[test]
    fn test_history_store_upsert_update() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = create_test_store(&temp_dir);

        let id = Uuid::new_v4();
        let mut session = SessionInfo::new(id, PathBuf::from("/test"));
        store.upsert(session.clone()).unwrap();

        session.message_count = 10;
        store.upsert(session).unwrap();

        assert_eq!(store.sessions.len(), 1);
        assert_eq!(store.get(id).unwrap().message_count, 10);
    }

    #[test]
    fn test_history_store_get() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = create_test_store(&temp_dir);

        let id = Uuid::new_v4();
        let session = SessionInfo::new(id, PathBuf::from("/test"));
        store.upsert(session).unwrap();

        assert!(store.get(id).is_some());
        assert!(store.get(Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_history_store_list_recent() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = create_test_store(&temp_dir);

        for i in 0..5 {
            let mut session =
                SessionInfo::new(Uuid::new_v4(), PathBuf::from(format!("/test{}", i)));
            session.message_count = i;
            store.upsert(session).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let recent = store.list_recent(3);
        assert_eq!(recent.len(), 3);
        // Most recent should be first (highest message_count added last)
        assert_eq!(recent[0].message_count, 4);
    }

    #[test]
    fn test_history_store_search_by_summary() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = create_test_store(&temp_dir);

        let mut session1 = SessionInfo::new(Uuid::new_v4(), PathBuf::from("/test1"));
        session1.set_summary("How to implement authentication");
        store.upsert(session1).unwrap();

        let mut session2 = SessionInfo::new(Uuid::new_v4(), PathBuf::from("/test2"));
        session2.set_summary("Database optimization");
        store.upsert(session2).unwrap();

        let results = store.search("auth");
        assert_eq!(results.len(), 1);
        assert!(results[0]
            .summary
            .as_ref()
            .unwrap()
            .contains("authentication"));
    }

    #[test]
    fn test_history_store_search_by_directory() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = create_test_store(&temp_dir);

        let session1 = SessionInfo::new(Uuid::new_v4(), PathBuf::from("/home/user/project_a"));
        store.upsert(session1).unwrap();

        let session2 = SessionInfo::new(Uuid::new_v4(), PathBuf::from("/home/user/project_b"));
        store.upsert(session2).unwrap();

        let results = store.search("project_a");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_history_store_search_case_insensitive() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = create_test_store(&temp_dir);

        let mut session = SessionInfo::new(Uuid::new_v4(), PathBuf::from("/test"));
        session.set_summary("UPPERCASE Summary");
        store.upsert(session).unwrap();

        let results = store.search("uppercase");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_history_store_delete() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = create_test_store(&temp_dir);

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        store
            .upsert(SessionInfo::new(id1, PathBuf::from("/test1")))
            .unwrap();
        store
            .upsert(SessionInfo::new(id2, PathBuf::from("/test2")))
            .unwrap();

        assert!(store.delete(id1).unwrap());
        assert_eq!(store.sessions.len(), 1);
        assert!(store.get(id1).is_none());
        assert!(store.get(id2).is_some());
    }

    #[test]
    fn test_history_store_delete_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = create_test_store(&temp_dir);

        let result = store.delete(Uuid::new_v4()).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_history_store_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = create_test_store(&temp_dir);

        // Add old session (manually set timestamp)
        let mut old_session = SessionInfo::new(Uuid::new_v4(), PathBuf::from("/old"));
        old_session.last_active = Utc::now() - chrono::Duration::days(100);
        store.sessions.push(old_session);

        // Add recent session
        store
            .upsert(SessionInfo::new(Uuid::new_v4(), PathBuf::from("/recent")))
            .unwrap();

        let removed = store.cleanup(30).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(store.sessions.len(), 1);
    }

    #[test]
    fn test_history_store_cleanup_nothing_removed() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = create_test_store(&temp_dir);

        store
            .upsert(SessionInfo::new(Uuid::new_v4(), PathBuf::from("/test")))
            .unwrap();

        let removed = store.cleanup(30).unwrap();
        assert_eq!(removed, 0);
        assert_eq!(store.sessions.len(), 1);
    }

    #[test]
    fn test_history_store_sessions_for_directory() {
        let temp_dir = TempDir::new().unwrap();
        let mut store = create_test_store(&temp_dir);

        let target_dir = PathBuf::from("/target/dir");
        store
            .upsert(SessionInfo::new(Uuid::new_v4(), target_dir.clone()))
            .unwrap();
        store
            .upsert(SessionInfo::new(Uuid::new_v4(), target_dir.clone()))
            .unwrap();
        store
            .upsert(SessionInfo::new(
                Uuid::new_v4(),
                PathBuf::from("/other/dir"),
            ))
            .unwrap();

        let sessions = store.sessions_for_directory(&target_dir);
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_history_store_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let index_path = temp_dir.path().join("history.json");

        // Create and save
        {
            let mut store = HistoryStore {
                index_path: index_path.clone(),
                sessions: Vec::new(),
            };
            let mut session = SessionInfo::new(Uuid::new_v4(), PathBuf::from("/test"));
            session.set_summary("Test session");
            store.upsert(session).unwrap();
        }

        // Verify file exists
        assert!(index_path.exists());

        // Load and verify
        let content = std::fs::read_to_string(&index_path).unwrap();
        let sessions: Vec<SessionInfo> = serde_json::from_str(&content).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].summary.as_ref().unwrap(), "Test session");
    }
}
