// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Filesystem storage backend
//!
//! Stores warm chunks as individual JSON files on the filesystem.

use async_trait::async_trait;
use std::path::PathBuf;

use super::{StorageBackend, TierStats};
use crate::context::chunk::Chunk;
use crate::error::{Result, TedError};

/// Filesystem-based storage backend
pub struct FilesystemBackend {
    /// Directory for chunk storage
    base_path: PathBuf,
}

impl FilesystemBackend {
    /// Create a new filesystem backend
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Get the full path for a chunk key
    fn chunk_path(&self, key: &str) -> PathBuf {
        self.base_path.join(format!("{}.json", key))
    }
}

#[async_trait]
impl StorageBackend for FilesystemBackend {
    async fn write(&self, key: &str, chunk: &Chunk) -> Result<()> {
        // Ensure directory exists
        if !self.base_path.exists() {
            tokio::fs::create_dir_all(&self.base_path).await?;
        }

        let path = self.chunk_path(key);
        let json = serde_json::to_string_pretty(chunk)
            .map_err(|e| TedError::Context(format!("Failed to serialize chunk: {}", e)))?;

        tokio::fs::write(path, json).await?;
        Ok(())
    }

    async fn read(&self, key: &str) -> Result<Option<Chunk>> {
        let path = self.chunk_path(key);

        if !path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&path).await?;
        let chunk: Chunk = serde_json::from_str(&content)
            .map_err(|e| TedError::Context(format!("Failed to deserialize chunk: {}", e)))?;

        Ok(Some(chunk))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.chunk_path(key);

        if path.exists() {
            tokio::fs::remove_file(path).await?;
        }

        Ok(())
    }

    async fn list_all(&self) -> Result<Vec<Chunk>> {
        let mut chunks = Vec::new();

        if !self.base_path.exists() {
            return Ok(chunks);
        }

        let mut entries = tokio::fs::read_dir(&self.base_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            match tokio::fs::read_to_string(&path).await {
                Ok(content) => match serde_json::from_str::<Chunk>(&content) {
                    Ok(chunk) => chunks.push(chunk),
                    Err(e) => {
                        tracing::warn!("Failed to parse chunk file {:?}: {}", path, e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read chunk file {:?}: {}", path, e);
                }
            }
        }

        Ok(chunks)
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.chunk_path(key).exists())
    }

    async fn clear(&self) -> Result<()> {
        if !self.base_path.exists() {
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(&self.base_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
                tokio::fs::remove_file(path).await?;
            }
        }

        Ok(())
    }

    async fn stats(&self) -> Result<TierStats> {
        let mut stats = TierStats::default();

        if !self.base_path.exists() {
            return Ok(stats);
        }

        let mut entries = tokio::fs::read_dir(&self.base_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            // Get file size
            if let Ok(metadata) = entry.metadata().await {
                stats.storage_bytes += metadata.len();
            }

            // Read and parse to get token count
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => match serde_json::from_str::<Chunk>(&content) {
                    Ok(chunk) => {
                        stats.chunk_count += 1;
                        stats.total_tokens += chunk.token_count;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse chunk file {:?}: {}", path, e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read chunk file {:?}: {}", path, e);
                }
            }
        }

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_filesystem_backend() {
        let dir = tempdir().unwrap();
        let backend = FilesystemBackend::new(dir.path().to_path_buf());

        // Create a test chunk
        let chunk = Chunk::new_message("user", "Hello, world!", None, 1);
        let key = chunk.id.to_string();

        // Write
        backend.write(&key, &chunk).await.unwrap();
        assert!(backend.exists(&key).await.unwrap());

        // Read
        let read_chunk = backend.read(&key).await.unwrap().unwrap();
        assert_eq!(read_chunk.id, chunk.id);

        // List
        let all = backend.list_all().await.unwrap();
        assert_eq!(all.len(), 1);

        // Delete
        backend.delete(&key).await.unwrap();
        assert!(!backend.exists(&key).await.unwrap());
    }

    #[tokio::test]
    async fn test_filesystem_backend_read_nonexistent() {
        let dir = tempdir().unwrap();
        let backend = FilesystemBackend::new(dir.path().to_path_buf());

        // Read nonexistent key should return None
        let result = backend.read("nonexistent-key").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_filesystem_backend_read_invalid_json() {
        let dir = tempdir().unwrap();
        let backend = FilesystemBackend::new(dir.path().to_path_buf());

        // Write invalid JSON to a chunk file
        let path = dir.path().join("invalid-key.json");
        std::fs::write(&path, "{ invalid json }").unwrap();

        // Read should fail with deserialization error
        let result = backend.read("invalid-key").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("deserialize"));
    }

    #[tokio::test]
    async fn test_filesystem_backend_list_all_skips_non_json() {
        let dir = tempdir().unwrap();
        let backend = FilesystemBackend::new(dir.path().to_path_buf());

        // Write a valid chunk
        let chunk = Chunk::new_message("user", "Hello", None, 1);
        backend.write(&chunk.id.to_string(), &chunk).await.unwrap();

        // Write a non-json file (should be skipped)
        std::fs::write(dir.path().join("readme.txt"), "not a chunk").unwrap();
        std::fs::write(dir.path().join("data.xml"), "<data>xml</data>").unwrap();

        // List should only return the valid chunk
        let all = backend.list_all().await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn test_filesystem_backend_list_all_skips_invalid_json() {
        let dir = tempdir().unwrap();
        let backend = FilesystemBackend::new(dir.path().to_path_buf());

        // Write a valid chunk
        let chunk = Chunk::new_message("user", "Hello", None, 1);
        backend.write(&chunk.id.to_string(), &chunk).await.unwrap();

        // Write invalid JSON file (should be skipped with warning)
        std::fs::write(dir.path().join("broken.json"), "{ not valid json").unwrap();

        // List should only return the valid chunk
        let all = backend.list_all().await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn test_filesystem_backend_clear_skips_non_json() {
        let dir = tempdir().unwrap();
        let backend = FilesystemBackend::new(dir.path().to_path_buf());

        // Write a valid chunk
        let chunk = Chunk::new_message("user", "Hello", None, 1);
        backend.write(&chunk.id.to_string(), &chunk).await.unwrap();

        // Write a non-json file (should NOT be deleted)
        let txt_path = dir.path().join("readme.txt");
        std::fs::write(&txt_path, "important file").unwrap();

        // Clear should only delete JSON files
        backend.clear().await.unwrap();

        // Chunk should be gone
        assert!(!backend.exists(&chunk.id.to_string()).await.unwrap());

        // Non-json file should still exist
        assert!(txt_path.exists());
    }

    #[tokio::test]
    async fn test_filesystem_backend_stats_skips_non_json() {
        let dir = tempdir().unwrap();
        let backend = FilesystemBackend::new(dir.path().to_path_buf());

        // Write valid chunks
        let chunk1 = Chunk::new_message("user", "Hello", None, 1);
        let chunk2 = Chunk::new_message("assistant", "Hi", None, 2);
        backend
            .write(&chunk1.id.to_string(), &chunk1)
            .await
            .unwrap();
        backend
            .write(&chunk2.id.to_string(), &chunk2)
            .await
            .unwrap();

        // Write a non-json file (should be skipped in stats)
        std::fs::write(dir.path().join("readme.txt"), "not a chunk").unwrap();

        // Stats should only count JSON files
        let stats = backend.stats().await.unwrap();
        assert_eq!(stats.chunk_count, 2);
    }

    #[tokio::test]
    async fn test_filesystem_backend_stats_skips_invalid_json() {
        let dir = tempdir().unwrap();
        let backend = FilesystemBackend::new(dir.path().to_path_buf());

        // Write a valid chunk
        let chunk = Chunk::new_message("user", "Hello", None, 1);
        backend.write(&chunk.id.to_string(), &chunk).await.unwrap();

        // Write invalid JSON file (should be skipped with warning)
        std::fs::write(dir.path().join("broken.json"), "{ not valid json").unwrap();

        // Stats should only count valid chunks
        let stats = backend.stats().await.unwrap();
        assert_eq!(stats.chunk_count, 1);
    }

    #[tokio::test]
    async fn test_filesystem_backend_stats_empty_dir() {
        let dir = tempdir().unwrap();
        let backend = FilesystemBackend::new(dir.path().to_path_buf());

        let stats = backend.stats().await.unwrap();
        assert_eq!(stats.chunk_count, 0);
        assert_eq!(stats.total_tokens, 0);
        assert_eq!(stats.storage_bytes, 0);
    }

    #[tokio::test]
    async fn test_filesystem_backend_stats_nonexistent_dir() {
        let backend = FilesystemBackend::new(PathBuf::from("/nonexistent/path"));

        // Stats for nonexistent dir should return default stats
        let stats = backend.stats().await.unwrap();
        assert_eq!(stats.chunk_count, 0);
    }

    #[tokio::test]
    async fn test_filesystem_backend_clear_empty_dir() {
        let dir = tempdir().unwrap();
        let backend = FilesystemBackend::new(dir.path().to_path_buf());

        // Clear on empty dir should succeed
        let result = backend.clear().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_filesystem_backend_clear_nonexistent_dir() {
        let backend = FilesystemBackend::new(PathBuf::from("/nonexistent/path"));

        // Clear on nonexistent dir should succeed (early return)
        let result = backend.clear().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_filesystem_backend_list_all_nonexistent_dir() {
        let backend = FilesystemBackend::new(PathBuf::from("/nonexistent/path"));

        // List all on nonexistent dir should return empty
        let all = backend.list_all().await.unwrap();
        assert!(all.is_empty());
    }

    #[tokio::test]
    async fn test_filesystem_backend_delete_nonexistent() {
        let dir = tempdir().unwrap();
        let backend = FilesystemBackend::new(dir.path().to_path_buf());

        // Delete nonexistent key should succeed
        let result = backend.delete("nonexistent-key").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_filesystem_backend_creates_dir() {
        let dir = tempdir().unwrap();
        let nested_path = dir.path().join("subdir/nested");
        let backend = FilesystemBackend::new(nested_path.clone());

        // Writing should create the directory
        let chunk = Chunk::new_message("user", "Hello", None, 1);
        backend.write(&chunk.id.to_string(), &chunk).await.unwrap();

        assert!(nested_path.exists());
    }
}
