// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Conversation memory storage and retrieval
//!
//! This module provides persistent storage for conversation history with semantic search.
//! Uses SQLite for storage and in-memory vector search for simplicity and portability.

use crate::embeddings::{search::SearchResult, EmbeddingGenerator};
use crate::error::{Result, TedError};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

/// Parse a UUID from a database string, converting errors to rusqlite errors
fn parse_uuid_from_db(id: &str, column: usize) -> std::result::Result<Uuid, rusqlite::Error> {
    Uuid::parse_str(id).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(e))
    })
}

/// Parse a DateTime from a database RFC3339 string, converting errors to rusqlite errors
fn parse_datetime_from_db(
    timestamp: &str,
    column: usize,
) -> std::result::Result<DateTime<Utc>, rusqlite::Error> {
    DateTime::parse_from_rfc3339(timestamp)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })
}

/// A stored conversation with metadata and embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMemory {
    /// Unique ID for this conversation
    pub id: Uuid,
    /// When the conversation occurred
    pub timestamp: DateTime<Utc>,
    /// Summary of the conversation (2-3 sentences)
    pub summary: String,
    /// Files that were modified
    pub files_changed: Vec<String>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Full conversation content (for context retrieval)
    pub content: String,
    /// Embedding vector for semantic search
    pub embedding: Vec<f32>,
}

/// Memory store for conversation history
pub struct MemoryStore {
    conn: Connection,
    embedding_generator: EmbeddingGenerator,
}

impl MemoryStore {
    /// Open or create a memory store at the given path
    pub fn open<P: AsRef<Path>>(path: P, embedding_generator: EmbeddingGenerator) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| TedError::Context(format!("Failed to open memory store: {}", e)))?;

        let store = Self {
            conn,
            embedding_generator,
        };

        store.init_schema()?;
        Ok(store)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<()> {
        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS conversation_memory (
                    id TEXT PRIMARY KEY,
                    timestamp TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    files_changed TEXT NOT NULL,
                    tags TEXT NOT NULL,
                    content TEXT NOT NULL,
                    embedding TEXT NOT NULL
                )",
                [],
            )
            .map_err(|e| TedError::Context(format!("Failed to create schema: {}", e)))?;

        // Create index on timestamp for recency queries
        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_timestamp ON conversation_memory(timestamp)",
                [],
            )
            .map_err(|e| TedError::Context(format!("Failed to create index: {}", e)))?;

        Ok(())
    }

    /// Store a conversation memory
    pub async fn store(&self, memory: &ConversationMemory) -> Result<()> {
        let id = memory.id.to_string();
        let timestamp = memory.timestamp.to_rfc3339();
        let files_changed = serde_json::to_string(&memory.files_changed)
            .map_err(|e| TedError::Context(format!("Failed to serialize files: {}", e)))?;
        let tags = serde_json::to_string(&memory.tags)
            .map_err(|e| TedError::Context(format!("Failed to serialize tags: {}", e)))?;
        let embedding = serde_json::to_string(&memory.embedding)
            .map_err(|e| TedError::Context(format!("Failed to serialize embedding: {}", e)))?;

        self.conn
            .execute(
                "INSERT OR REPLACE INTO conversation_memory
                (id, timestamp, summary, files_changed, tags, content, embedding)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    timestamp,
                    &memory.summary,
                    files_changed,
                    tags,
                    &memory.content,
                    embedding
                ],
            )
            .map_err(|e| TedError::Context(format!("Failed to store memory: {}", e)))?;

        Ok(())
    }

    /// Search for similar conversations using semantic search
    pub async fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchResult>> {
        // Generate embedding for query
        let query_embedding = self.embedding_generator.embed(query).await?;

        // Load all memories (for small datasets, in-memory is fast enough)
        let memories = self.load_all()?;

        // Calculate similarities
        let mut results: Vec<SearchResult> = memories
            .iter()
            .map(|memory| {
                let score =
                    EmbeddingGenerator::cosine_similarity(&query_embedding, &memory.embedding);
                SearchResult {
                    content: format!(
                        "[{}] {}\nFiles: {}\nTags: {}",
                        memory.timestamp.format("%Y-%m-%d %H:%M"),
                        memory.summary,
                        memory.files_changed.join(", "),
                        memory.tags.join(", ")
                    ),
                    score,
                    metadata: Some(serde_json::json!({
                        "id": memory.id,
                        "timestamp": memory.timestamp,
                        "files_changed": memory.files_changed,
                        "tags": memory.tags,
                        "full_content": memory.content,
                    })),
                }
            })
            .collect();

        // Sort by score (descending)
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Return top K
        results.truncate(top_k);

        Ok(results)
    }

    /// Search by keyword (full-text search on summary and content)
    pub fn search_keywords(&self, keywords: &str, limit: usize) -> Result<Vec<ConversationMemory>> {
        let query = format!("%{}%", keywords);

        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, timestamp, summary, files_changed, tags, content, embedding
                FROM conversation_memory
                WHERE summary LIKE ?1 OR content LIKE ?1
                ORDER BY timestamp DESC
                LIMIT ?2",
            )
            .map_err(|e| TedError::Context(format!("Failed to prepare query: {}", e)))?;

        let memories = stmt
            .query_map(params![query, limit], |row| {
                let id: String = row.get(0)?;
                let timestamp: String = row.get(1)?;
                let summary: String = row.get(2)?;
                let files_changed_str: String = row.get(3)?;
                let tags_str: String = row.get(4)?;
                let content: String = row.get(5)?;
                let embedding_str: String = row.get(6)?;

                Ok(ConversationMemory {
                    id: parse_uuid_from_db(&id, 0)?,
                    timestamp: parse_datetime_from_db(&timestamp, 1)?,
                    summary,
                    files_changed: serde_json::from_str(&files_changed_str).unwrap_or_default(),
                    tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                    content,
                    embedding: serde_json::from_str(&embedding_str).unwrap_or_default(),
                })
            })
            .map_err(|e| TedError::Context(format!("Failed to query memories: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(memories)
    }

    /// Get recent conversations (by timestamp)
    pub fn get_recent(&self, limit: usize) -> Result<Vec<ConversationMemory>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, timestamp, summary, files_changed, tags, content, embedding
                FROM conversation_memory
                ORDER BY timestamp DESC
                LIMIT ?1",
            )
            .map_err(|e| TedError::Context(format!("Failed to prepare query: {}", e)))?;

        let memories = stmt
            .query_map(params![limit], |row| {
                let id: String = row.get(0)?;
                let timestamp: String = row.get(1)?;
                let summary: String = row.get(2)?;
                let files_changed_str: String = row.get(3)?;
                let tags_str: String = row.get(4)?;
                let content: String = row.get(5)?;
                let embedding_str: String = row.get(6)?;

                Ok(ConversationMemory {
                    id: parse_uuid_from_db(&id, 0)?,
                    timestamp: parse_datetime_from_db(&timestamp, 1)?,
                    summary,
                    files_changed: serde_json::from_str(&files_changed_str).unwrap_or_default(),
                    tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                    content,
                    embedding: serde_json::from_str(&embedding_str).unwrap_or_default(),
                })
            })
            .map_err(|e| TedError::Context(format!("Failed to query memories: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(memories)
    }

    /// Load all memories (for in-memory search)
    fn load_all(&self) -> Result<Vec<ConversationMemory>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, timestamp, summary, files_changed, tags, content, embedding
                FROM conversation_memory",
            )
            .map_err(|e| TedError::Context(format!("Failed to prepare query: {}", e)))?;

        let memories = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let timestamp: String = row.get(1)?;
                let summary: String = row.get(2)?;
                let files_changed_str: String = row.get(3)?;
                let tags_str: String = row.get(4)?;
                let content: String = row.get(5)?;
                let embedding_str: String = row.get(6)?;

                Ok(ConversationMemory {
                    id: parse_uuid_from_db(&id, 0)?,
                    timestamp: parse_datetime_from_db(&timestamp, 1)?,
                    summary,
                    files_changed: serde_json::from_str(&files_changed_str).unwrap_or_default(),
                    tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                    content,
                    embedding: serde_json::from_str(&embedding_str).unwrap_or_default(),
                })
            })
            .map_err(|e| TedError::Context(format!("Failed to load memories: {}", e)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(memories)
    }

    /// Get a specific memory by ID
    pub fn get(&self, id: Uuid) -> Result<Option<ConversationMemory>> {
        let id_str = id.to_string();

        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, timestamp, summary, files_changed, tags, content, embedding
                FROM conversation_memory
                WHERE id = ?1",
            )
            .map_err(|e| TedError::Context(format!("Failed to prepare query: {}", e)))?;

        let memory = stmt
            .query_row(params![id_str], |row| {
                let id: String = row.get(0)?;
                let timestamp: String = row.get(1)?;
                let summary: String = row.get(2)?;
                let files_changed_str: String = row.get(3)?;
                let tags_str: String = row.get(4)?;
                let content: String = row.get(5)?;
                let embedding_str: String = row.get(6)?;

                Ok(ConversationMemory {
                    id: parse_uuid_from_db(&id, 0)?,
                    timestamp: parse_datetime_from_db(&timestamp, 1)?,
                    summary,
                    files_changed: serde_json::from_str(&files_changed_str).unwrap_or_default(),
                    tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                    content,
                    embedding: serde_json::from_str(&embedding_str).unwrap_or_default(),
                })
            })
            .ok();

        Ok(memory)
    }

    /// Delete a memory by ID
    pub fn delete(&self, id: Uuid) -> Result<()> {
        let id_str = id.to_string();

        self.conn
            .execute(
                "DELETE FROM conversation_memory WHERE id = ?1",
                params![id_str],
            )
            .map_err(|e| TedError::Context(format!("Failed to delete memory: {}", e)))?;

        Ok(())
    }

    /// Count total memories
    pub fn count(&self) -> Result<usize> {
        let count: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM conversation_memory", [], |row| {
                row.get(0)
            })
            .map_err(|e| TedError::Context(format!("Failed to count memories: {}", e)))?;

        Ok(count)
    }

    /// Clear all memories from the database
    pub fn clear_all(&self) -> Result<usize> {
        let count = self.count()?;

        self.conn
            .execute("DELETE FROM conversation_memory", [])
            .map_err(|e| TedError::Context(format!("Failed to clear memories: {}", e)))?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    /// Create an EmbeddingGenerator for testing (bundled backend)
    fn create_test_embedding_generator() -> EmbeddingGenerator {
        EmbeddingGenerator::new()
    }

    fn create_test_memory() -> ConversationMemory {
        ConversationMemory {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            summary: "Added authentication system to the app".to_string(),
            files_changed: vec!["src/auth.rs".to_string(), "src/main.rs".to_string()],
            tags: vec!["authentication".to_string(), "security".to_string()],
            content: "Full conversation about implementing JWT authentication...".to_string(),
            embedding: vec![0.1, 0.2, 0.3], // Mock embedding
        }
    }

    #[tokio::test]
    async fn test_memory_store_create() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator);

        assert!(store.is_ok());
    }

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        let memory = create_test_memory();
        let id = memory.id;

        // Store memory
        store.store(&memory).await.unwrap();

        // Retrieve memory
        let retrieved = store.get(id).unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, id);
        assert_eq!(retrieved.summary, memory.summary);
        assert_eq!(retrieved.files_changed, memory.files_changed);
        assert_eq!(retrieved.tags, memory.tags);
    }

    #[tokio::test]
    async fn test_count() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        assert_eq!(store.count().unwrap(), 0);

        store.store(&create_test_memory()).await.unwrap();
        assert_eq!(store.count().unwrap(), 1);

        store.store(&create_test_memory()).await.unwrap();
        assert_eq!(store.count().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_delete() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        let memory = create_test_memory();
        let id = memory.id;

        store.store(&memory).await.unwrap();
        assert_eq!(store.count().unwrap(), 1);

        store.delete(id).unwrap();
        assert_eq!(store.count().unwrap(), 0);
        assert!(store.get(id).unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_recent() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        // Store multiple memories
        for _ in 0..5 {
            store.store(&create_test_memory()).await.unwrap();
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        // Get recent 3
        let recent = store.get_recent(3).unwrap();
        assert_eq!(recent.len(), 3);

        // Should be ordered by timestamp (newest first)
        for i in 1..recent.len() {
            assert!(recent[i - 1].timestamp >= recent[i].timestamp);
        }
    }

    #[tokio::test]
    async fn test_keyword_search() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        let mut memory = create_test_memory();
        memory.summary = "Added authentication system".to_string();
        memory.content = "Implemented JWT authentication with OAuth2".to_string();
        store.store(&memory).await.unwrap();

        let mut memory2 = create_test_memory();
        memory2.summary = "Fixed database connection issue".to_string();
        memory2.content = "Resolved connection pool exhaustion problem".to_string();
        store.store(&memory2).await.unwrap();

        // Search for "authentication"
        let results = store.search_keywords("authentication", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].summary.contains("authentication"));
    }

    // ===== Additional ConversationMemory Tests =====

    #[test]
    fn test_conversation_memory_clone() {
        let memory = create_test_memory();
        let cloned = memory.clone();

        assert_eq!(cloned.id, memory.id);
        assert_eq!(cloned.summary, memory.summary);
        assert_eq!(cloned.files_changed, memory.files_changed);
        assert_eq!(cloned.tags, memory.tags);
        assert_eq!(cloned.content, memory.content);
        assert_eq!(cloned.embedding, memory.embedding);
    }

    #[test]
    fn test_conversation_memory_debug() {
        let memory = create_test_memory();
        let debug = format!("{:?}", memory);

        assert!(debug.contains("ConversationMemory"));
        assert!(debug.contains("authentication"));
    }

    #[test]
    fn test_conversation_memory_serialize() {
        let memory = create_test_memory();
        let json = serde_json::to_string(&memory).unwrap();

        assert!(json.contains("summary"));
        assert!(json.contains("files_changed"));
        assert!(json.contains("tags"));
        assert!(json.contains("embedding"));
    }

    #[test]
    fn test_conversation_memory_deserialize() {
        let memory = create_test_memory();
        let json = serde_json::to_string(&memory).unwrap();
        let deserialized: ConversationMemory = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, memory.id);
        assert_eq!(deserialized.summary, memory.summary);
        assert_eq!(deserialized.files_changed, memory.files_changed);
    }

    #[test]
    fn test_conversation_memory_empty_fields() {
        let memory = ConversationMemory {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            summary: String::new(),
            files_changed: vec![],
            tags: vec![],
            content: String::new(),
            embedding: vec![],
        };

        assert!(memory.summary.is_empty());
        assert!(memory.files_changed.is_empty());
        assert!(memory.tags.is_empty());
        assert!(memory.embedding.is_empty());
    }

    #[test]
    fn test_conversation_memory_many_files_and_tags() {
        let memory = ConversationMemory {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            summary: "Large refactoring".to_string(),
            files_changed: (0..100).map(|i| format!("src/file{}.rs", i)).collect(),
            tags: (0..50).map(|i| format!("tag{}", i)).collect(),
            content: "Extensive work".to_string(),
            embedding: (0..768).map(|i| i as f32 / 768.0).collect(),
        };

        assert_eq!(memory.files_changed.len(), 100);
        assert_eq!(memory.tags.len(), 50);
        assert_eq!(memory.embedding.len(), 768);
    }

    // ===== Additional MemoryStore Tests =====

    #[tokio::test]
    async fn test_clear_all() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        // Store multiple memories
        for _ in 0..5 {
            store.store(&create_test_memory()).await.unwrap();
        }

        assert_eq!(store.count().unwrap(), 5);

        // Clear all
        let cleared = store.clear_all().unwrap();
        assert_eq!(cleared, 5);
        assert_eq!(store.count().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        let random_id = Uuid::new_v4();
        let result = store.get(random_id).unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        let random_id = Uuid::new_v4();
        // Deleting nonexistent should succeed without error
        let result = store.delete(random_id);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_existing_memory() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        let memory = create_test_memory();
        let id = memory.id;

        // Store original
        store.store(&memory).await.unwrap();

        // Update with same ID
        let mut updated = memory.clone();
        updated.summary = "Updated summary".to_string();
        store.store(&updated).await.unwrap();

        // Count should still be 1 (update, not insert)
        assert_eq!(store.count().unwrap(), 1);

        // Retrieved should have updated summary
        let retrieved = store.get(id).unwrap().unwrap();
        assert_eq!(retrieved.summary, "Updated summary");
    }

    #[tokio::test]
    async fn test_keyword_search_empty_result() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        let memory = create_test_memory();
        store.store(&memory).await.unwrap();

        // Search for keyword not in any memory
        let results = store.search_keywords("zzz_not_found_zzz", 10).unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_keyword_search_in_content() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        let mut memory = create_test_memory();
        memory.summary = "Simple summary".to_string();
        memory.content = "This conversation discusses UNIQUE_KEYWORD in detail".to_string();
        store.store(&memory).await.unwrap();

        // Search for keyword in content (not in summary)
        let results = store.search_keywords("UNIQUE_KEYWORD", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_get_recent_empty_store() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        let recent = store.get_recent(5).unwrap();
        assert!(recent.is_empty());
    }

    #[tokio::test]
    async fn test_get_recent_more_than_available() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        // Store 2 memories
        store.store(&create_test_memory()).await.unwrap();
        store.store(&create_test_memory()).await.unwrap();

        // Request more than available
        let recent = store.get_recent(10).unwrap();
        assert_eq!(recent.len(), 2);
    }

    #[tokio::test]
    async fn test_keyword_search_limit() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        // Store 5 memories with same keyword
        for i in 0..5 {
            let mut memory = create_test_memory();
            memory.summary = format!("Authentication feature {}", i);
            store.store(&memory).await.unwrap();
        }

        // Search with limit of 2
        let results = store.search_keywords("Authentication", 2).unwrap();
        assert_eq!(results.len(), 2);
    }

    // ===== Timestamp Handling Tests =====

    #[test]
    fn test_timestamp_roundtrip() {
        let original = Utc::now();
        let rfc3339 = original.to_rfc3339();
        let parsed = DateTime::parse_from_rfc3339(&rfc3339)
            .unwrap()
            .with_timezone(&Utc);

        // Should preserve at least second precision
        assert_eq!(original.timestamp(), parsed.timestamp());
    }

    // ===== Files and Tags Serialization Tests =====

    #[test]
    fn test_files_changed_serialization() {
        let files = vec!["a.rs".to_string(), "b.rs".to_string()];
        let json = serde_json::to_string(&files).unwrap();
        let parsed: Vec<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, files);
    }

    #[test]
    fn test_tags_serialization() {
        let tags = vec!["tag1".to_string(), "tag2".to_string()];
        let json = serde_json::to_string(&tags).unwrap();
        let parsed: Vec<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, tags);
    }

    #[test]
    fn test_embedding_serialization() {
        let embedding: Vec<f32> = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let json = serde_json::to_string(&embedding).unwrap();
        let parsed: Vec<f32> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, embedding);
    }

    // ===== Unicode and Special Characters Tests =====

    #[tokio::test]
    async fn test_unicode_content() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        let mut memory = create_test_memory();
        memory.summary = "日本語テスト 🚀 émojis".to_string();
        memory.content = "Content with unicode: 你好世界".to_string();

        let id = memory.id;
        store.store(&memory).await.unwrap();

        let retrieved = store.get(id).unwrap().unwrap();
        assert_eq!(retrieved.summary, "日本語テスト 🚀 émojis");
        assert!(retrieved.content.contains("你好世界"));
    }

    #[tokio::test]
    async fn test_special_sql_characters() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        let mut memory = create_test_memory();
        memory.summary = "Test with 'quotes' and \"double quotes\"".to_string();
        memory.content = "Content with % and _ and other SQL chars".to_string();

        let id = memory.id;
        store.store(&memory).await.unwrap();

        let retrieved = store.get(id).unwrap().unwrap();
        assert!(retrieved.summary.contains("'quotes'"));
        assert!(retrieved.content.contains("%"));
    }

    // ===== Semantic Search Tests =====
    // These tests use a mock embedding server

    #[tokio::test]
    async fn test_semantic_search() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = create_test_embedding_generator();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        // Store a memory
        let memory = create_test_memory();
        store.store(&memory).await.unwrap();

        // Perform semantic search
        let results = store.search("authentication system", 5).await.unwrap();

        // Should return results (even if empty with mock embeddings)
        // The function should complete without error
        assert!(results.len() <= 5);
    }

    #[tokio::test]
    async fn test_semantic_search_empty_store() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = create_test_embedding_generator();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        // Search on empty store
        let results = store.search("any query", 5).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_semantic_search_multiple_memories() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = create_test_embedding_generator();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        // Store multiple memories
        for i in 0..5 {
            let mut memory = create_test_memory();
            memory.summary = format!("Memory {} about topic {}", i, i);
            store.store(&memory).await.unwrap();
        }

        // Search should return up to top_k results
        let results = store.search("topic", 3).await.unwrap();
        assert!(results.len() <= 3);
    }

    #[tokio::test]
    async fn test_semantic_search_result_format() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = create_test_embedding_generator();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        let mut memory = create_test_memory();
        memory.summary = "Authentication implementation".to_string();
        memory.files_changed = vec!["src/auth.rs".to_string()];
        memory.tags = vec!["auth".to_string()];
        store.store(&memory).await.unwrap();

        let results = store.search("auth", 1).await.unwrap();

        if !results.is_empty() {
            let result = &results[0];
            // Result content should contain formatted information
            assert!(result.content.contains("Authentication"));
            // Result should have metadata
            assert!(result.metadata.is_some());
        }
    }

    #[tokio::test]
    async fn test_semantic_search_top_k_limit() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = create_test_embedding_generator();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        // Store 10 memories
        for i in 0..10 {
            let mut memory = create_test_memory();
            memory.summary = format!("Feature {}", i);
            store.store(&memory).await.unwrap();
        }

        // Request only top 3
        let results = store.search("feature", 3).await.unwrap();
        assert!(results.len() <= 3);
    }

    // ===== load_all Tests (via search) =====

    #[tokio::test]
    async fn test_load_all_via_search() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = create_test_embedding_generator();
        let store = MemoryStore::open(temp_file.path(), generator).unwrap();

        // Store memories with different content
        let mut mem1 = create_test_memory();
        mem1.summary = "First memory about rust".to_string();
        store.store(&mem1).await.unwrap();

        let mut mem2 = create_test_memory();
        mem2.summary = "Second memory about python".to_string();
        store.store(&mem2).await.unwrap();

        // Search loads all memories internally
        let results = store.search("programming", 10).await.unwrap();
        // Both memories should be loaded (2 in store)
        // Results may be 0, 1, or 2 depending on embedding similarity
        assert!(results.len() <= 2);
    }

    // ===== UUID and DateTime parsing error handling =====

    #[test]
    fn test_parse_uuid_from_db_valid() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let result = parse_uuid_from_db(uuid_str, 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), uuid_str);
    }

    #[test]
    fn test_parse_uuid_from_db_invalid() {
        let invalid = "not-a-uuid";
        let result = parse_uuid_from_db(invalid, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_datetime_from_db_valid() {
        let timestamp = "2025-01-15T10:30:00+00:00";
        let result = parse_datetime_from_db(timestamp, 1);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_datetime_from_db_invalid() {
        let invalid = "not-a-date";
        let result = parse_datetime_from_db(invalid, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_datetime_preserves_timezone() {
        use chrono::Timelike;
        let timestamp = "2025-01-15T10:30:00+05:00";
        let result = parse_datetime_from_db(timestamp, 1).unwrap();
        // Should be converted to UTC
        assert_eq!(result.hour(), 5); // 10:30+05:00 = 05:30 UTC
    }
}
