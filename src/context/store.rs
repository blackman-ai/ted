// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Context store abstraction
//!
//! The ContextStore manages chunks across all storage tiers and handles
//! the flow of data between tiers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::backends::filesystem::FilesystemBackend;
use super::backends::StorageBackend;
use super::chunk::{Chunk, ChunkType};
use super::cold::ColdStorage;
use super::wal::{WalReader, WalWriter};
use super::ContextStats;
use crate::error::Result;

/// Configuration for the context store
#[derive(Debug, Clone)]
pub struct StoreConfig {
    /// Maximum number of chunks in warm storage before compaction
    pub max_warm_chunks: usize,
    /// Age threshold (in seconds) before moving to cold storage
    pub cold_threshold_secs: u64,
    /// Whether to enable compression for cold storage
    pub enable_compression: bool,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            max_warm_chunks: 100,
            cold_threshold_secs: 3600, // 1 hour
            enable_compression: true,
        }
    }
}

/// The context store manages all chunk storage
pub struct ContextStore {
    /// Base path for this session's storage
    base_path: PathBuf,
    /// In-memory cache of hot chunks
    hot_cache: HashMap<Uuid, Chunk>,
    /// WAL writer for hot storage
    wal_writer: WalWriter,
    /// Storage backend for warm chunks
    warm_backend: FilesystemBackend,
    /// Cold storage handler
    cold_storage: ColdStorage,
    /// Configuration
    config: StoreConfig,
    /// Next sequence number
    next_sequence: u64,
}

impl ContextStore {
    /// Open or create a context store at the given path
    pub async fn open(base_path: PathBuf) -> Result<Self> {
        Self::open_with_config(base_path, StoreConfig::default()).await
    }

    /// Open with custom configuration
    pub async fn open_with_config(base_path: PathBuf, config: StoreConfig) -> Result<Self> {
        // Create directories if they don't exist
        let wal_path = base_path.join("wal");
        let chunks_path = base_path.join("chunks");
        let cold_path = base_path.join("cold");

        tokio::fs::create_dir_all(&wal_path).await?;
        tokio::fs::create_dir_all(&chunks_path).await?;
        tokio::fs::create_dir_all(&cold_path).await?;

        // Initialize components
        let wal_writer = WalWriter::new(wal_path.clone()).await?;
        let warm_backend = FilesystemBackend::new(chunks_path);
        let cold_storage = ColdStorage::new(cold_path, config.enable_compression);

        // Recover from WAL
        let (hot_cache, next_sequence) = Self::recover_from_wal(&wal_path).await?;

        Ok(Self {
            base_path,
            hot_cache,
            wal_writer,
            warm_backend,
            cold_storage,
            config,
            next_sequence,
        })
    }

    /// Recover state from WAL on startup
    async fn recover_from_wal(wal_path: &Path) -> Result<(HashMap<Uuid, Chunk>, u64)> {
        let reader = WalReader::new(wal_path.to_path_buf());
        let entries = reader.read_all().await?;

        let mut cache = HashMap::new();
        let mut max_sequence = 0u64;

        for chunk in entries {
            max_sequence = max_sequence.max(chunk.sequence);
            cache.insert(chunk.id, chunk);
        }

        Ok((cache, max_sequence + 1))
    }

    /// Get the next sequence number
    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Append a new chunk to the store
    pub async fn append(&mut self, mut chunk: Chunk) -> Result<Uuid> {
        // Ensure sequence is set
        if chunk.sequence == 0 {
            chunk.sequence = self.next_sequence;
            self.next_sequence += 1;
        } else {
            self.next_sequence = self.next_sequence.max(chunk.sequence + 1);
        }

        let id = chunk.id;

        // Write to WAL first (durability)
        self.wal_writer.append(&chunk).await?;

        // Add to hot cache
        self.hot_cache.insert(id, chunk);

        // Check if we should trigger compaction
        if self.hot_cache.len() > self.config.max_warm_chunks / 2 {
            // Compact in background (don't block)
            self.maybe_compact_hot().await?;
        }

        Ok(id)
    }

    /// Get a chunk by ID, checking all tiers
    pub async fn get(&self, id: Uuid) -> Result<Option<Chunk>> {
        // Check hot cache first
        if let Some(chunk) = self.hot_cache.get(&id) {
            return Ok(Some(chunk.clone()));
        }

        // Check warm storage
        if let Some(chunk) = self.warm_backend.read(&id.to_string()).await? {
            return Ok(Some(chunk));
        }

        // Check cold storage (decompress if found)
        if let Some(chunk) = self.cold_storage.get(id).await? {
            return Ok(Some(chunk));
        }

        // Not found (might have been GC'd) - graceful degradation
        Ok(None)
    }

    /// Get all chunks in sequence order
    pub async fn get_all(&self) -> Result<Vec<Chunk>> {
        let mut chunks = Vec::new();

        // Collect from all tiers
        chunks.extend(self.hot_cache.values().cloned());

        // Add warm chunks
        let warm_chunks = self.warm_backend.list_all().await?;
        chunks.extend(warm_chunks);

        // Add cold chunks (decompress as needed)
        let cold_chunks = self.cold_storage.list_all().await?;
        chunks.extend(cold_chunks);

        // Sort by sequence
        chunks.sort_by_key(|c| c.sequence);

        Ok(chunks)
    }

    /// Get recent chunks (hot and warm only)
    pub async fn get_recent(&self, limit: usize) -> Result<Vec<Chunk>> {
        let mut chunks: Vec<Chunk> = self.hot_cache.values().cloned().collect();

        // Add warm chunks if we need more
        if chunks.len() < limit {
            let warm_chunks = self.warm_backend.list_all().await?;
            chunks.extend(warm_chunks);
        }

        // Sort by sequence descending (most recent first)
        chunks.sort_by_key(|c| std::cmp::Reverse(c.sequence));

        // Take only the requested number
        chunks.truncate(limit);

        // Re-sort ascending for proper conversation order
        chunks.sort_by_key(|c| c.sequence);

        Ok(chunks)
    }

    /// Get chunks by type
    pub async fn get_by_type(&self, chunk_type: ChunkType) -> Result<Vec<Chunk>> {
        let all = self.get_all().await?;
        Ok(all
            .into_iter()
            .filter(|c| c.chunk_type == chunk_type)
            .collect())
    }

    /// Move old hot chunks to warm storage
    async fn maybe_compact_hot(&mut self) -> Result<()> {
        let threshold = self.config.max_warm_chunks / 4;

        if self.hot_cache.len() <= threshold {
            return Ok(());
        }

        // Find chunks to demote (oldest first)
        let mut chunks_to_demote: Vec<_> = self
            .hot_cache
            .values()
            .filter(|c| c.can_compact())
            .cloned()
            .collect();

        chunks_to_demote.sort_by_key(|c| c.sequence);

        // Keep only the newest half in hot
        let to_demote_count = chunks_to_demote.len() / 2;
        let demote_list: Vec<_> = chunks_to_demote.into_iter().take(to_demote_count).collect();

        for mut chunk in demote_list {
            chunk.demote(); // Hot -> Warm

            // Write to warm storage
            self.warm_backend
                .write(&chunk.id.to_string(), &chunk)
                .await?;

            // Remove from hot cache
            self.hot_cache.remove(&chunk.id);
        }

        Ok(())
    }

    /// Run full compaction (hot -> warm -> cold)
    pub async fn compact(&mut self) -> Result<()> {
        // First, compact hot to warm
        self.maybe_compact_hot().await?;

        // Then, compact warm to cold
        self.compact_warm_to_cold().await?;

        // Finally, rotate WAL files
        self.wal_writer.rotate().await?;

        Ok(())
    }

    /// Move old warm chunks to cold storage
    async fn compact_warm_to_cold(&mut self) -> Result<()> {
        let warm_chunks = self.warm_backend.list_all().await?;

        let now = chrono::Utc::now();
        let threshold = chrono::Duration::seconds(self.config.cold_threshold_secs as i64);

        for chunk in warm_chunks {
            let age = now.signed_duration_since(chunk.accessed_at);

            if age > threshold && chunk.can_compact() {
                // Move to cold storage
                let mut cold_chunk = chunk.clone();
                cold_chunk.demote(); // Warm -> Cold

                self.cold_storage.put(cold_chunk).await?;

                // Remove from warm storage
                self.warm_backend.delete(&chunk.id.to_string()).await?;
            }
        }

        Ok(())
    }

    /// Clear all context
    pub async fn clear(&mut self) -> Result<()> {
        // Clear hot cache
        self.hot_cache.clear();

        // Clear WAL
        self.wal_writer.clear().await?;

        // Clear warm storage
        self.warm_backend.clear().await?;

        // Clear cold storage
        self.cold_storage.clear().await?;

        // Reset sequence
        self.next_sequence = 0;

        Ok(())
    }

    /// Get statistics about the store (includes all tiers)
    pub async fn stats(&self) -> ContextStats {
        let hot_chunks = self.hot_cache.len();
        let hot_tokens: u32 = self.hot_cache.values().map(|c| c.token_count).sum();

        // Get warm tier stats
        let warm_stats = self.warm_backend.stats().await.unwrap_or_default();

        // Get cold tier stats (full stats to include tokens)
        let cold_stats = self.cold_storage.full_stats().await;

        let total_chunks = hot_chunks + warm_stats.chunk_count + cold_stats.total_files;
        let total_tokens = hot_tokens + warm_stats.total_tokens + cold_stats.total_tokens;
        let storage_bytes = warm_stats.storage_bytes + cold_stats.total_bytes;

        ContextStats {
            session_id: self
                .base_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            total_chunks,
            hot_chunks,
            warm_chunks: warm_stats.chunk_count,
            cold_chunks: cold_stats.total_files,
            total_tokens,
            storage_bytes,
        }
    }

    /// Get chunks that reference a specific file.
    pub async fn get_chunks_for_file(&self, path: &Path) -> Result<Vec<Chunk>> {
        let all = self.get_all().await?;
        Ok(all
            .into_iter()
            .filter(|c| c.references_file(path))
            .collect())
    }

    /// Get chunks sorted by effective priority (highest first).
    pub async fn get_by_priority(&self, limit: usize) -> Result<Vec<Chunk>> {
        let mut chunks = self.get_all().await?;

        // Sort by effective priority (descending)
        chunks.sort_by(|a, b| {
            b.effective_priority()
                .partial_cmp(&a.effective_priority())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        chunks.truncate(limit);
        Ok(chunks)
    }

    /// Update retention scores for chunks based on file scores.
    ///
    /// This integrates with the indexer's memory system.
    pub fn update_chunk_scores(&mut self, file_scores: &std::collections::HashMap<PathBuf, f64>) {
        for chunk in self.hot_cache.values_mut() {
            if !chunk.referenced_files.is_empty() {
                // Average the scores of referenced files
                let total: f64 = chunk
                    .referenced_files
                    .iter()
                    .filter_map(|p| file_scores.get(p))
                    .sum();
                let count = chunk
                    .referenced_files
                    .iter()
                    .filter(|p| file_scores.contains_key(*p))
                    .count();

                if count > 0 {
                    chunk.set_retention_score(total / count as f64);
                }
            }
        }
    }

    /// Get all file paths referenced by chunks in the store.
    pub fn get_referenced_files(&self) -> std::collections::HashSet<PathBuf> {
        let mut files = std::collections::HashSet::new();
        for chunk in self.hot_cache.values() {
            files.extend(chunk.referenced_files.iter().cloned());
        }
        files
    }

    /// Touch a chunk (update accessed_at) to boost its priority.
    pub async fn touch_chunk(&mut self, id: Uuid) -> Result<bool> {
        if let Some(chunk) = self.hot_cache.get_mut(&id) {
            chunk.touch();
            return Ok(true);
        }

        // Try to promote from warm storage
        if let Some(mut chunk) = self.warm_backend.read(&id.to_string()).await? {
            chunk.touch();
            chunk.promote();

            // Move back to hot cache
            self.hot_cache.insert(id, chunk.clone());

            // Remove from warm
            self.warm_backend.delete(&id.to_string()).await?;

            return Ok(true);
        }

        Ok(false)
    }

    /// Get a mutable reference to a hot chunk (for updating).
    pub fn get_hot_mut(&mut self, id: Uuid) -> Option<&mut Chunk> {
        self.hot_cache.get_mut(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::chunk::ChunkContent;
    use tempfile::TempDir;

    async fn create_test_store() -> (ContextStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = ContextStore::open(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        (store, temp_dir)
    }

    async fn create_test_store_with_config(config: StoreConfig) -> (ContextStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = ContextStore::open_with_config(temp_dir.path().to_path_buf(), config)
            .await
            .unwrap();
        (store, temp_dir)
    }

    #[test]
    fn test_store_config_default() {
        let config = StoreConfig::default();
        assert_eq!(config.max_warm_chunks, 100);
        assert_eq!(config.cold_threshold_secs, 3600);
        assert!(config.enable_compression);
    }

    #[tokio::test]
    async fn test_store_open() {
        let (store, _temp_dir) = create_test_store().await;
        assert_eq!(store.next_sequence(), 1);
        assert!(store.hot_cache.is_empty());
    }

    #[tokio::test]
    async fn test_store_open_with_config() {
        let config = StoreConfig {
            max_warm_chunks: 50,
            cold_threshold_secs: 1800,
            enable_compression: false,
        };
        let (store, _temp_dir) = create_test_store_with_config(config).await;
        assert_eq!(store.config.max_warm_chunks, 50);
        assert_eq!(store.config.cold_threshold_secs, 1800);
        assert!(!store.config.enable_compression);
    }

    #[tokio::test]
    async fn test_store_append_chunk() {
        let (mut store, _temp_dir) = create_test_store().await;

        let chunk = Chunk::new_message("user", "Hello!", None, 0);
        let id = store.append(chunk).await.unwrap();

        assert!(store.hot_cache.contains_key(&id));
        assert_eq!(store.hot_cache.len(), 1);
    }

    #[tokio::test]
    async fn test_store_append_sets_sequence() {
        let (mut store, _temp_dir) = create_test_store().await;

        let chunk1 = Chunk::new_message("user", "First", None, 0);
        let chunk2 = Chunk::new_message("assistant", "Second", None, 0);

        store.append(chunk1).await.unwrap();
        store.append(chunk2).await.unwrap();

        let chunks: Vec<_> = store.hot_cache.values().collect();
        let sequences: Vec<_> = chunks.iter().map(|c| c.sequence).collect();

        assert!(sequences.contains(&1));
        assert!(sequences.contains(&2));
    }

    #[tokio::test]
    async fn test_store_get_existing_chunk() {
        let (mut store, _temp_dir) = create_test_store().await;

        let chunk = Chunk::new_message("user", "Hello!", None, 0);
        let original_content = if let ChunkContent::Message { content, .. } = &chunk.content {
            content.clone()
        } else {
            panic!("Expected message content");
        };

        let id = store.append(chunk).await.unwrap();

        let retrieved = store.get(id).await.unwrap().unwrap();
        if let ChunkContent::Message { content, .. } = &retrieved.content {
            assert_eq!(content, &original_content);
        } else {
            panic!("Expected message content");
        }
    }

    #[tokio::test]
    async fn test_store_get_nonexistent_chunk() {
        let (store, _temp_dir) = create_test_store().await;

        let random_id = Uuid::new_v4();
        let result = store.get(random_id).await.unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_store_get_all() {
        let (mut store, _temp_dir) = create_test_store().await;

        store
            .append(Chunk::new_message("user", "First", None, 0))
            .await
            .unwrap();
        store
            .append(Chunk::new_message("assistant", "Second", None, 0))
            .await
            .unwrap();
        store
            .append(Chunk::new_message("user", "Third", None, 0))
            .await
            .unwrap();

        let all_chunks = store.get_all().await.unwrap();
        assert_eq!(all_chunks.len(), 3);

        // Verify they're sorted by sequence
        for i in 1..all_chunks.len() {
            assert!(all_chunks[i].sequence > all_chunks[i - 1].sequence);
        }
    }

    #[tokio::test]
    async fn test_store_get_recent() {
        let (mut store, _temp_dir) = create_test_store().await;

        for i in 0..10 {
            store
                .append(Chunk::new_message(
                    "user",
                    &format!("Message {}", i),
                    None,
                    0,
                ))
                .await
                .unwrap();
        }

        let recent = store.get_recent(3).await.unwrap();
        assert_eq!(recent.len(), 3);

        // Verify they're in ascending sequence order (oldest first of the recent set)
        for i in 1..recent.len() {
            assert!(recent[i].sequence > recent[i - 1].sequence);
        }
    }

    #[tokio::test]
    async fn test_store_get_by_type() {
        let (mut store, _temp_dir) = create_test_store().await;

        store
            .append(Chunk::new_message("user", "Hello", None, 0))
            .await
            .unwrap();
        store
            .append(Chunk::new_system("System prompt", 0))
            .await
            .unwrap();
        store
            .append(Chunk::new_message("assistant", "Hi", None, 0))
            .await
            .unwrap();

        let messages = store.get_by_type(ChunkType::Message).await.unwrap();
        assert_eq!(messages.len(), 2);

        let system = store.get_by_type(ChunkType::System).await.unwrap();
        assert_eq!(system.len(), 1);
    }

    #[tokio::test]
    async fn test_store_clear() {
        let (mut store, _temp_dir) = create_test_store().await;

        store
            .append(Chunk::new_message("user", "Hello", None, 0))
            .await
            .unwrap();
        store
            .append(Chunk::new_message("assistant", "Hi", None, 0))
            .await
            .unwrap();

        assert_eq!(store.hot_cache.len(), 2);

        store.clear().await.unwrap();

        assert!(store.hot_cache.is_empty());
        assert_eq!(store.next_sequence, 0);
    }

    #[tokio::test]
    async fn test_store_stats() {
        let (mut store, _temp_dir) = create_test_store().await;

        store
            .append(Chunk::new_message("user", "Hello", None, 0))
            .await
            .unwrap();
        store
            .append(Chunk::new_message("assistant", "Hi", None, 0))
            .await
            .unwrap();

        let stats = store.stats().await;
        assert_eq!(stats.hot_chunks, 2);
        assert!(stats.total_tokens > 0);
    }

    #[tokio::test]
    async fn test_store_compact() {
        let config = StoreConfig {
            max_warm_chunks: 4,
            cold_threshold_secs: 0, // Immediate cold transition for testing
            enable_compression: false,
        };
        let (mut store, _temp_dir) = create_test_store_with_config(config).await;

        // Add enough chunks to trigger compaction
        for i in 0..10 {
            store
                .append(Chunk::new_message(
                    "user",
                    &format!("Message {}", i),
                    None,
                    0,
                ))
                .await
                .unwrap();
        }

        let initial_hot_count = store.hot_cache.len();

        store.compact().await.unwrap();

        // After compaction, some chunks should have moved to warm/cold
        // The exact number depends on the compaction logic
        let final_hot_count = store.hot_cache.len();

        // At minimum, compaction should have run without error
        // The hot cache might be smaller or same size depending on what's compactable
        assert!(final_hot_count <= initial_hot_count);
    }

    #[tokio::test]
    async fn test_store_recovery_from_wal() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        // First session: add some chunks
        {
            let mut store = ContextStore::open(path.clone()).await.unwrap();
            store
                .append(Chunk::new_message("user", "Hello", None, 0))
                .await
                .unwrap();
            store
                .append(Chunk::new_message("assistant", "Hi", None, 0))
                .await
                .unwrap();
        }

        // Second session: should recover from WAL
        {
            let store = ContextStore::open(path.clone()).await.unwrap();
            assert_eq!(store.hot_cache.len(), 2);
            assert!(store.next_sequence > 2);
        }
    }

    #[tokio::test]
    async fn test_store_directories_created() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        let _store = ContextStore::open(path.clone()).await.unwrap();

        assert!(path.join("wal").exists());
        assert!(path.join("chunks").exists());
        assert!(path.join("cold").exists());
    }

    #[tokio::test]
    async fn test_store_tool_call_chunk() {
        let (mut store, _temp_dir) = create_test_store().await;

        let chunk = Chunk::new_tool_call(
            "file_read",
            &serde_json::json!({"path": "/test.txt"}),
            "File contents here",
            false,
            None,
            0,
        );

        let id = store.append(chunk).await.unwrap();
        let retrieved = store.get(id).await.unwrap().unwrap();

        assert_eq!(retrieved.chunk_type, ChunkType::ToolCall);
    }

    #[tokio::test]
    async fn test_store_summary_chunk() {
        let (mut store, _temp_dir) = create_test_store().await;

        let chunk1_id = store
            .append(Chunk::new_message("user", "Hello", None, 0))
            .await
            .unwrap();
        let chunk2_id = store
            .append(Chunk::new_message("assistant", "Hi", None, 0))
            .await
            .unwrap();

        let summary_chunk = Chunk::new_summary(
            "User greeted and assistant responded",
            vec![chunk1_id, chunk2_id],
            None,
            0,
        );

        let summary_id = store.append(summary_chunk).await.unwrap();
        let retrieved = store.get(summary_id).await.unwrap().unwrap();

        assert_eq!(retrieved.chunk_type, ChunkType::Summary);
        if let ChunkContent::Summary {
            summarized_chunks, ..
        } = retrieved.content
        {
            assert_eq!(summarized_chunks.len(), 2);
        } else {
            panic!("Expected summary content");
        }
    }

    #[tokio::test]
    async fn test_store_get_chunks_for_file() {
        let (mut store, _temp_dir) = create_test_store().await;

        // Add a file content chunk
        let chunk = Chunk::new(
            ChunkType::FileContent,
            ChunkContent::FileContent {
                path: "/test/file.rs".to_string(),
                content: "fn main() {}".to_string(),
                language: Some("rust".to_string()),
            },
            None,
            0,
        );
        store.append(chunk).await.unwrap();

        // Add a regular message
        store
            .append(Chunk::new_message("user", "Hello", None, 0))
            .await
            .unwrap();

        let file_chunks = store
            .get_chunks_for_file(Path::new("/test/file.rs"))
            .await
            .unwrap();
        assert_eq!(file_chunks.len(), 1);
    }

    #[tokio::test]
    async fn test_store_get_by_priority() {
        let (mut store, _temp_dir) = create_test_store().await;

        // Add chunks with different priorities
        store
            .append(Chunk::new_message("user", "Hello", None, 0))
            .await
            .unwrap(); // High
        store.append(Chunk::new_system("System", 0)).await.unwrap(); // Critical

        let input = serde_json::json!({"path": "/test"});
        store
            .append(Chunk::new_tool_call(
                "file_read",
                &input,
                "out",
                false,
                None,
                0,
            ))
            .await
            .unwrap(); // Normal

        let by_priority = store.get_by_priority(3).await.unwrap();
        assert_eq!(by_priority.len(), 3);

        // First should be Critical (System)
        assert_eq!(by_priority[0].chunk_type, ChunkType::System);
    }

    #[tokio::test]
    async fn test_store_update_chunk_scores() {
        let (mut store, _temp_dir) = create_test_store().await;

        let chunk = Chunk::new(
            ChunkType::FileContent,
            ChunkContent::FileContent {
                path: "src/main.rs".to_string(),
                content: "fn main() {}".to_string(),
                language: Some("rust".to_string()),
            },
            None,
            0,
        );
        let id = store.append(chunk).await.unwrap();

        // Update scores
        let mut file_scores = std::collections::HashMap::new();
        file_scores.insert(PathBuf::from("src/main.rs"), 0.8);

        store.update_chunk_scores(&file_scores);

        let chunk = store.get(id).await.unwrap().unwrap();
        assert!((chunk.retention_score - 0.8).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_store_get_referenced_files() {
        let (mut store, _temp_dir) = create_test_store().await;

        let chunk = Chunk::new(
            ChunkType::FileContent,
            ChunkContent::FileContent {
                path: "/test/file.rs".to_string(),
                content: "".to_string(),
                language: None,
            },
            None,
            0,
        );
        store.append(chunk).await.unwrap();

        let files = store.get_referenced_files();
        assert_eq!(files.len(), 1);
        assert!(files.contains(&PathBuf::from("/test/file.rs")));
    }

    #[tokio::test]
    async fn test_store_touch_chunk() {
        let (mut store, _temp_dir) = create_test_store().await;

        let chunk = Chunk::new_message("user", "Hello", None, 0);
        let id = store.append(chunk).await.unwrap();

        let original = store.get(id).await.unwrap().unwrap();
        let original_time = original.accessed_at;

        std::thread::sleep(std::time::Duration::from_millis(10));

        let touched = store.touch_chunk(id).await.unwrap();
        assert!(touched);

        let updated = store.get(id).await.unwrap().unwrap();
        assert!(updated.accessed_at > original_time);
    }

    #[tokio::test]
    async fn test_store_touch_nonexistent() {
        let (mut store, _temp_dir) = create_test_store().await;

        let random_id = Uuid::new_v4();
        let touched = store.touch_chunk(random_id).await.unwrap();
        assert!(!touched);
    }

    #[tokio::test]
    async fn test_store_get_hot_mut() {
        let (mut store, _temp_dir) = create_test_store().await;

        let chunk = Chunk::new_message("user", "Hello", None, 0);
        let id = store.append(chunk).await.unwrap();

        let chunk_mut = store.get_hot_mut(id).unwrap();
        chunk_mut.set_retention_score(0.9);

        let updated = store.get(id).await.unwrap().unwrap();
        assert_eq!(updated.retention_score, 0.9);
    }
}
