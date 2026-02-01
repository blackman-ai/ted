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
                    id: Uuid::parse_str(&id).unwrap(),
                    timestamp: DateTime::parse_from_rfc3339(&timestamp)
                        .unwrap()
                        .with_timezone(&Utc),
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
                    id: Uuid::parse_str(&id).unwrap(),
                    timestamp: DateTime::parse_from_rfc3339(&timestamp)
                        .unwrap()
                        .with_timezone(&Utc),
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
                    id: Uuid::parse_str(&id).unwrap(),
                    timestamp: DateTime::parse_from_rfc3339(&timestamp)
                        .unwrap()
                        .with_timezone(&Utc),
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
                    id: Uuid::parse_str(&id).unwrap(),
                    timestamp: DateTime::parse_from_rfc3339(&timestamp)
                        .unwrap()
                        .with_timezone(&Utc),
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
}
