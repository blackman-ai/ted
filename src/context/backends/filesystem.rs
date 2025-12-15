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
}
