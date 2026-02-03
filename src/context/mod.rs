// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Context management system
//!
//! This module implements a WAL-based tiered storage system for conversation context.
//! The context system operates invisibly in the background:
//!
//! - Hot tier (WAL): Recent entries in write-ahead log
//! - Warm tier (chunks/): Active chunk files
//! - Cold tier (cold/): Compressed archives
//!
//! Background compaction continuously moves data from hot → warm → cold.

pub mod backends;
pub mod chunk;
pub mod cold;
pub mod filetree;
pub mod memory;
pub mod recall;
pub mod store;
pub mod summarizer;
pub mod wal;

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::{Result, TedError};
use chunk::{Chunk, ChunkType};
pub use filetree::{FileTree, FileTreeConfig};
use store::ContextStore;

/// Session ID for identifying context storage
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(pub Uuid);

impl SessionId {
    /// Create a new random session ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from an existing UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse from a string
    pub fn parse(s: &str) -> Result<Self> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| TedError::Session(format!("Invalid session ID: {}", e)))
    }

    /// Get as string
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Context manager for handling conversation state
///
/// This is the main entry point for context operations. It manages:
/// - Session lifecycle
/// - Chunk storage across tiers
/// - Background compaction
/// - Project file tree (for LLM awareness)
pub struct ContextManager {
    /// Current session ID
    session_id: SessionId,
    /// Base storage path (~/.ted/context/) - stored for potential future use
    #[allow(dead_code)]
    storage_path: PathBuf,
    /// The context store
    store: Arc<RwLock<ContextStore>>,
    /// Next sequence number for chunks
    sequence: Arc<RwLock<u64>>,
    /// Cached project file tree
    file_tree: Arc<RwLock<Option<FileTree>>>,
    /// Project root (for file tree generation)
    project_root: Option<PathBuf>,
}

impl ContextManager {
    /// Create a new context manager for a session
    pub async fn new(storage_path: PathBuf, session_id: SessionId) -> Result<Self> {
        let session_path = storage_path.join(session_id.as_str());
        let store = ContextStore::open(session_path.clone()).await?;

        // Get the next sequence number from the store
        let sequence = store.next_sequence();

        Ok(Self {
            session_id,
            storage_path,
            store: Arc::new(RwLock::new(store)),
            sequence: Arc::new(RwLock::new(sequence)),
            file_tree: Arc::new(RwLock::new(None)),
            project_root: None,
        })
    }

    /// Create a new session
    pub async fn new_session(storage_path: PathBuf) -> Result<Self> {
        let session_id = SessionId::new();
        Self::new(storage_path, session_id).await
    }

    /// Resume an existing session
    pub async fn resume_session(storage_path: PathBuf, session_id: SessionId) -> Result<Self> {
        Self::new(storage_path, session_id).await
    }

    /// Set the project root and optionally generate the file tree
    pub async fn set_project_root(&mut self, root: PathBuf, generate_tree: bool) -> Result<()> {
        self.project_root = Some(root.clone());

        if generate_tree {
            self.refresh_file_tree().await?;
        }

        Ok(())
    }

    /// Generate or refresh the file tree and store as core memory chunk
    pub async fn refresh_file_tree(&self) -> Result<()> {
        if let Some(ref root) = self.project_root {
            let config = FileTreeConfig::default();
            let tree = FileTree::generate(root, &config)?;

            // Store the file tree as a core memory chunk (never compacted)
            self.store_file_tree(&tree).await?;

            let mut guard = self.file_tree.write().await;
            *guard = Some(tree);
        }
        Ok(())
    }

    /// Get the file tree as a string for context
    pub async fn file_tree_context(&self) -> Option<String> {
        let guard = self.file_tree.read().await;
        guard.as_ref().map(|tree| tree.to_context_string())
    }

    /// Get whether a file tree is available
    pub async fn has_file_tree(&self) -> bool {
        let guard = self.file_tree.read().await;
        guard.is_some()
    }

    /// Get project root
    pub fn project_root(&self) -> Option<&PathBuf> {
        self.project_root.as_ref()
    }

    /// Get the current session ID
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Get the next sequence number
    async fn next_sequence(&self) -> u64 {
        let mut seq = self.sequence.write().await;
        let current = *seq;
        *seq += 1;
        current
    }

    /// Store a new chunk
    pub async fn store_chunk(&self, chunk: Chunk) -> Result<Uuid> {
        let mut store = self.store.write().await;
        store.append(chunk).await
    }

    /// Create and store a message chunk
    pub async fn store_message(
        &self,
        role: &str,
        content: &str,
        parent_id: Option<Uuid>,
    ) -> Result<Uuid> {
        let sequence = self.next_sequence().await;
        let chunk = Chunk::new_message(role, content, parent_id, sequence);
        self.store_chunk(chunk).await
    }

    /// Create and store a tool call chunk
    pub async fn store_tool_call(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
        output: &str,
        is_error: bool,
        parent_id: Option<Uuid>,
    ) -> Result<Uuid> {
        let sequence = self.next_sequence().await;
        let chunk = Chunk::new_tool_call(tool_name, input, output, is_error, parent_id, sequence);
        self.store_chunk(chunk).await
    }

    /// Create and store a summary chunk
    pub async fn store_summary(
        &self,
        summary: &str,
        summarized_chunks: Vec<Uuid>,
        parent_id: Option<Uuid>,
    ) -> Result<Uuid> {
        let sequence = self.next_sequence().await;
        let chunk = Chunk::new_summary(summary, summarized_chunks, parent_id, sequence);
        self.store_chunk(chunk).await
    }

    /// Store the file tree as a core memory chunk (never compacted)
    pub async fn store_file_tree(&self, file_tree: &FileTree) -> Result<Uuid> {
        let sequence = self.next_sequence().await;
        let chunk = Chunk::new_file_tree(
            &file_tree.root_name(),
            file_tree.as_string(),
            file_tree.file_count(),
            file_tree.dir_count(),
            file_tree.is_truncated(),
            sequence,
        );
        self.store_chunk(chunk).await
    }

    /// Retrieve a chunk by ID
    pub async fn get_chunk(&self, id: Uuid) -> Result<Option<Chunk>> {
        let store = self.store.read().await;
        store.get(id).await
    }

    /// Get all chunks in sequence order
    pub async fn get_all_chunks(&self) -> Result<Vec<Chunk>> {
        let store = self.store.read().await;
        store.get_all().await
    }

    /// Get recent chunks (hot and warm tiers only)
    pub async fn get_recent_chunks(&self, limit: usize) -> Result<Vec<Chunk>> {
        let store = self.store.read().await;
        store.get_recent(limit).await
    }

    /// Get chunks by type
    pub async fn get_chunks_by_type(&self, chunk_type: ChunkType) -> Result<Vec<Chunk>> {
        let store = self.store.read().await;
        store.get_by_type(chunk_type).await
    }

    /// Trigger manual compaction (normally runs in background)
    pub async fn compact(&self) -> Result<()> {
        let mut store = self.store.write().await;
        store.compact().await
    }

    /// Clear all context (for `ted clear` command)
    pub async fn clear(&self) -> Result<()> {
        let mut store = self.store.write().await;
        store.clear().await
    }

    /// Get statistics about the context
    pub async fn stats(&self) -> ContextStats {
        let store = self.store.read().await;
        store.stats().await
    }

    /// Start background compaction daemon
    pub fn start_background_compaction(&self, interval_secs: u64) -> tokio::task::JoinHandle<()> {
        let store = Arc::clone(&self.store);

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));

            loop {
                interval.tick().await;

                let mut store = store.write().await;
                if let Err(e) = store.compact().await {
                    tracing::warn!("Background compaction failed: {}", e);
                }
            }
        })
    }
}

/// Statistics about the context storage
#[derive(Debug, Clone)]
pub struct ContextStats {
    pub session_id: String,
    pub total_chunks: usize,
    pub hot_chunks: usize,
    pub warm_chunks: usize,
    pub cold_chunks: usize,
    pub total_tokens: u32,
    pub storage_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_session_id_new() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_session_id_from_uuid() {
        let uuid = Uuid::new_v4();
        let session_id = SessionId::from_uuid(uuid);
        assert_eq!(session_id.0, uuid);
    }

    #[test]
    fn test_session_id_parse() {
        let id = SessionId::new();
        let id_str = id.as_str();
        let parsed = SessionId::parse(&id_str).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_session_id_parse_invalid() {
        let result = SessionId::parse("not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn test_session_id_as_str() {
        let id = SessionId::new();
        let s = id.as_str();
        assert!(!s.is_empty());
        // UUID format: 8-4-4-4-12
        assert!(s.contains('-'));
    }

    #[test]
    fn test_session_id_default() {
        let id = SessionId::default();
        assert!(!id.as_str().is_empty());
    }

    #[test]
    fn test_session_id_display() {
        let id = SessionId::new();
        let display = format!("{}", id);
        assert_eq!(display, id.as_str());
    }

    #[tokio::test]
    async fn test_context_manager_new_session() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        assert!(!manager.session_id().as_str().is_empty());
    }

    #[tokio::test]
    async fn test_context_manager_resume_session() {
        let temp_dir = TempDir::new().unwrap();
        let session_id = SessionId::new();

        // Create initial session
        {
            let manager = ContextManager::new(temp_dir.path().to_path_buf(), session_id.clone())
                .await
                .unwrap();
            manager.store_message("user", "Hello", None).await.unwrap();
        }

        // Resume session
        let manager =
            ContextManager::resume_session(temp_dir.path().to_path_buf(), session_id.clone())
                .await
                .unwrap();

        assert_eq!(manager.session_id(), &session_id);
    }

    #[tokio::test]
    async fn test_context_manager_store_message() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let id = manager
            .store_message("user", "Hello, world!", None)
            .await
            .unwrap();

        let chunk = manager.get_chunk(id).await.unwrap().unwrap();
        assert_eq!(chunk.chunk_type, ChunkType::Message);
    }

    #[tokio::test]
    async fn test_context_manager_store_tool_call() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let input = serde_json::json!({"path": "/test.txt"});
        let id = manager
            .store_tool_call("file_read", &input, "File contents", false, None)
            .await
            .unwrap();

        let chunk = manager.get_chunk(id).await.unwrap().unwrap();
        assert_eq!(chunk.chunk_type, ChunkType::ToolCall);
    }

    #[tokio::test]
    async fn test_context_manager_store_summary() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let msg_id = manager.store_message("user", "Hello", None).await.unwrap();
        let summary_id = manager
            .store_summary("User said hello", vec![msg_id], None)
            .await
            .unwrap();

        let chunk = manager.get_chunk(summary_id).await.unwrap().unwrap();
        assert_eq!(chunk.chunk_type, ChunkType::Summary);
    }

    #[tokio::test]
    async fn test_context_manager_get_all_chunks() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        manager.store_message("user", "First", None).await.unwrap();
        manager
            .store_message("assistant", "Second", None)
            .await
            .unwrap();
        manager.store_message("user", "Third", None).await.unwrap();

        let chunks = manager.get_all_chunks().await.unwrap();
        assert_eq!(chunks.len(), 3);
    }

    #[tokio::test]
    async fn test_context_manager_get_recent_chunks() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        for i in 0..10 {
            manager
                .store_message("user", &format!("Message {}", i), None)
                .await
                .unwrap();
        }

        let recent = manager.get_recent_chunks(5).await.unwrap();
        assert_eq!(recent.len(), 5);
    }

    #[tokio::test]
    async fn test_context_manager_get_chunks_by_type() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        manager.store_message("user", "Hello", None).await.unwrap();
        let input = serde_json::json!({});
        manager
            .store_tool_call("test", &input, "output", false, None)
            .await
            .unwrap();
        manager
            .store_message("assistant", "Hi", None)
            .await
            .unwrap();

        let messages = manager
            .get_chunks_by_type(ChunkType::Message)
            .await
            .unwrap();
        assert_eq!(messages.len(), 2);

        let tool_calls = manager
            .get_chunks_by_type(ChunkType::ToolCall)
            .await
            .unwrap();
        assert_eq!(tool_calls.len(), 1);
    }

    #[tokio::test]
    async fn test_context_manager_clear() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        manager.store_message("user", "Hello", None).await.unwrap();
        manager
            .store_message("assistant", "Hi", None)
            .await
            .unwrap();

        let chunks_before = manager.get_all_chunks().await.unwrap();
        assert_eq!(chunks_before.len(), 2);

        manager.clear().await.unwrap();

        let chunks_after = manager.get_all_chunks().await.unwrap();
        assert_eq!(chunks_after.len(), 0);
    }

    #[tokio::test]
    async fn test_context_manager_stats() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        manager.store_message("user", "Hello", None).await.unwrap();
        manager
            .store_message("assistant", "Hi", None)
            .await
            .unwrap();

        let stats = manager.stats().await;
        assert_eq!(stats.hot_chunks, 2);
        assert!(stats.total_tokens > 0);
    }

    #[tokio::test]
    async fn test_context_manager_compact() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        for i in 0..20 {
            manager
                .store_message("user", &format!("Message {}", i), None)
                .await
                .unwrap();
        }

        // Compact should run without error
        manager.compact().await.unwrap();
    }

    #[tokio::test]
    async fn test_context_manager_get_nonexistent_chunk() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        let random_id = Uuid::new_v4();
        let result = manager.get_chunk(random_id).await.unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_context_stats_fields() {
        let stats = ContextStats {
            session_id: "test-id".to_string(),
            total_chunks: 10,
            hot_chunks: 5,
            warm_chunks: 3,
            cold_chunks: 2,
            total_tokens: 500,
            storage_bytes: 1024,
        };

        assert_eq!(stats.session_id, "test-id");
        assert_eq!(stats.total_chunks, 10);
        assert_eq!(stats.hot_chunks, 5);
        assert_eq!(stats.warm_chunks, 3);
        assert_eq!(stats.cold_chunks, 2);
        assert_eq!(stats.total_tokens, 500);
        assert_eq!(stats.storage_bytes, 1024);
    }

    // ===== Additional Coverage Tests =====

    #[tokio::test]
    async fn test_context_manager_project_root() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        // Initially no project root
        assert!(manager.project_root().is_none());
    }

    #[tokio::test]
    async fn test_context_manager_set_project_root_no_tree() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = TempDir::new().unwrap();

        let mut manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        // Set project root without generating tree
        manager
            .set_project_root(project_dir.path().to_path_buf(), false)
            .await
            .unwrap();

        assert!(manager.project_root().is_some());
        assert_eq!(
            manager.project_root().unwrap(),
            &project_dir.path().to_path_buf()
        );
    }

    #[tokio::test]
    async fn test_context_manager_has_file_tree_false() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        // No file tree initially
        assert!(!manager.has_file_tree().await);
    }

    #[tokio::test]
    async fn test_context_manager_file_tree_context_none() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        // No file tree context initially
        assert!(manager.file_tree_context().await.is_none());
    }

    #[tokio::test]
    async fn test_context_manager_set_project_root_with_tree() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = TempDir::new().unwrap();

        // Create some files in the project directory
        std::fs::write(project_dir.path().join("main.rs"), "fn main() {}").unwrap();
        std::fs::create_dir(project_dir.path().join("src")).unwrap();
        std::fs::write(project_dir.path().join("src/lib.rs"), "// lib").unwrap();

        let mut manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        // Set project root with tree generation
        manager
            .set_project_root(project_dir.path().to_path_buf(), true)
            .await
            .unwrap();

        assert!(manager.has_file_tree().await);
        let context = manager.file_tree_context().await;
        assert!(context.is_some());
    }

    #[tokio::test]
    async fn test_context_manager_refresh_file_tree() {
        let temp_dir = TempDir::new().unwrap();
        let project_dir = TempDir::new().unwrap();

        // Create a file
        std::fs::write(project_dir.path().join("test.rs"), "// test").unwrap();

        let mut manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        // Set project root without generating tree
        manager
            .set_project_root(project_dir.path().to_path_buf(), false)
            .await
            .unwrap();

        assert!(!manager.has_file_tree().await);

        // Now refresh the tree
        manager.refresh_file_tree().await.unwrap();

        assert!(manager.has_file_tree().await);
    }

    #[tokio::test]
    async fn test_context_manager_refresh_file_tree_no_project_root() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ContextManager::new_session(temp_dir.path().to_path_buf())
            .await
            .unwrap();

        // Refresh without project root should succeed but do nothing
        manager.refresh_file_tree().await.unwrap();
        assert!(!manager.has_file_tree().await);
    }

    #[test]
    fn test_context_stats_clone() {
        let stats = ContextStats {
            session_id: "test".to_string(),
            total_chunks: 5,
            hot_chunks: 2,
            warm_chunks: 2,
            cold_chunks: 1,
            total_tokens: 100,
            storage_bytes: 512,
        };

        let cloned = stats.clone();
        assert_eq!(cloned.session_id, stats.session_id);
        assert_eq!(cloned.total_chunks, stats.total_chunks);
    }

    #[test]
    fn test_context_stats_debug() {
        let stats = ContextStats {
            session_id: "debug-test".to_string(),
            total_chunks: 1,
            hot_chunks: 1,
            warm_chunks: 0,
            cold_chunks: 0,
            total_tokens: 10,
            storage_bytes: 100,
        };

        let debug_str = format!("{:?}", stats);
        assert!(debug_str.contains("ContextStats"));
        assert!(debug_str.contains("debug-test"));
    }

    #[test]
    fn test_session_id_clone() {
        let id = SessionId::new();
        let cloned = id.clone();
        assert_eq!(id, cloned);
    }

    #[test]
    fn test_session_id_hash() {
        use std::collections::HashSet;
        let id1 = SessionId::new();
        let id2 = SessionId::new();

        let mut set = HashSet::new();
        set.insert(id1.clone());
        set.insert(id2.clone());
        set.insert(id1.clone()); // Duplicate

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_session_id_debug() {
        let id = SessionId::new();
        let debug_str = format!("{:?}", id);
        assert!(debug_str.contains("SessionId"));
    }
}
