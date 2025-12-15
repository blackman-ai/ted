// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Cold storage with compression
//!
//! Cold storage handles the oldest chunks, compressing them with zstd
//! to save disk space. Chunks are decompressed on-demand when accessed.

use std::io::{Read, Write};
use std::path::PathBuf;
use uuid::Uuid;

use super::chunk::Chunk;
use crate::error::{Result, TedError};

/// Cold storage handler
pub struct ColdStorage {
    /// Directory for cold storage
    cold_dir: PathBuf,
    /// Whether compression is enabled
    compression_enabled: bool,
    /// Compression level (1-22, default 3)
    compression_level: i32,
}

impl ColdStorage {
    /// Create a new cold storage handler
    pub fn new(cold_dir: PathBuf, compression_enabled: bool) -> Self {
        Self {
            cold_dir,
            compression_enabled,
            compression_level: 3, // Good balance of speed and ratio
        }
    }

    /// Get the file path for a chunk
    fn chunk_path(&self, id: Uuid) -> PathBuf {
        let ext = if self.compression_enabled {
            "json.zst"
        } else {
            "json"
        };
        self.cold_dir.join(format!("{}.{}", id, ext))
    }

    /// Store a chunk in cold storage
    pub async fn put(&self, chunk: Chunk) -> Result<()> {
        let path = self.chunk_path(chunk.id);

        let json = serde_json::to_vec(&chunk)
            .map_err(|e| TedError::Context(format!("Failed to serialize chunk: {}", e)))?;

        let data = if self.compression_enabled {
            self.compress(&json)?
        } else {
            json
        };

        tokio::fs::write(&path, data).await?;

        Ok(())
    }

    /// Retrieve and decompress a chunk
    pub async fn get(&self, id: Uuid) -> Result<Option<Chunk>> {
        let path = self.chunk_path(id);

        if !path.exists() {
            // Try the other extension
            let alt_path = if self.compression_enabled {
                self.cold_dir.join(format!("{}.json", id))
            } else {
                self.cold_dir.join(format!("{}.json.zst", id))
            };

            if !alt_path.exists() {
                return Ok(None);
            }

            // Found with alternate extension
            return self.read_chunk(&alt_path, !self.compression_enabled).await;
        }

        self.read_chunk(&path, self.compression_enabled).await
    }

    /// Read a chunk from a path
    async fn read_chunk(&self, path: &PathBuf, compressed: bool) -> Result<Option<Chunk>> {
        let data = tokio::fs::read(path).await?;

        let json = if compressed {
            self.decompress(&data)?
        } else {
            data
        };

        let chunk: Chunk = serde_json::from_slice(&json)
            .map_err(|e| TedError::Context(format!("Failed to deserialize chunk: {}", e)))?;

        Ok(Some(chunk))
    }

    /// Delete a chunk from cold storage
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        let path = self.chunk_path(id);

        if path.exists() {
            tokio::fs::remove_file(path).await?;
        }

        Ok(())
    }

    /// List all chunks in cold storage
    pub async fn list_all(&self) -> Result<Vec<Chunk>> {
        let mut chunks = Vec::new();

        if !self.cold_dir.exists() {
            return Ok(chunks);
        }

        let mut entries = tokio::fs::read_dir(&self.cold_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let filename = entry.file_name();
            let filename = filename.to_string_lossy();

            // Parse the UUID from the filename
            let id_str = if filename.ends_with(".json.zst") {
                filename.strip_suffix(".json.zst")
            } else if filename.ends_with(".json") {
                filename.strip_suffix(".json")
            } else {
                continue;
            };

            if let Some(id_str) = id_str {
                if Uuid::parse_str(id_str).is_ok() {
                    let compressed = filename.ends_with(".zst");
                    if let Ok(Some(chunk)) = self.read_chunk(&path, compressed).await {
                        chunks.push(chunk);
                    }
                }
            }
        }

        Ok(chunks)
    }

    /// Clear all cold storage
    pub async fn clear(&mut self) -> Result<()> {
        if !self.cold_dir.exists() {
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(&self.cold_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                tokio::fs::remove_file(path).await?;
            }
        }

        Ok(())
    }

    /// Compress data using zstd
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = zstd::Encoder::new(Vec::new(), self.compression_level)
            .map_err(|e| TedError::Context(format!("Failed to create compressor: {}", e)))?;

        encoder
            .write_all(data)
            .map_err(|e| TedError::Context(format!("Failed to compress: {}", e)))?;

        encoder
            .finish()
            .map_err(|e| TedError::Context(format!("Failed to finish compression: {}", e)))
    }

    /// Decompress data using zstd
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = zstd::Decoder::new(data)
            .map_err(|e| TedError::Context(format!("Failed to create decompressor: {}", e)))?;

        let mut result = Vec::new();
        decoder
            .read_to_end(&mut result)
            .map_err(|e| TedError::Context(format!("Failed to decompress: {}", e)))?;

        Ok(result)
    }

    /// Get storage statistics (fast, doesn't read file contents)
    pub async fn stats(&self) -> ColdStorageStats {
        let mut total_files = 0usize;
        let mut total_bytes = 0u64;
        let mut compressed_bytes = 0u64;
        let mut uncompressed_bytes = 0u64;

        if let Ok(mut entries) = tokio::fs::read_dir(&self.cold_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(metadata) = entry.metadata().await {
                    total_files += 1;
                    let size = metadata.len();
                    total_bytes += size;

                    let filename = entry.file_name();
                    if filename.to_string_lossy().ends_with(".zst") {
                        compressed_bytes += size;
                    } else {
                        uncompressed_bytes += size;
                    }
                }
            }
        }

        ColdStorageStats {
            total_files,
            total_bytes,
            compressed_bytes,
            uncompressed_bytes,
            total_tokens: 0, // Not calculated in fast mode
        }
    }

    /// Get full statistics including token count (requires reading all files)
    pub async fn full_stats(&self) -> ColdStorageStats {
        let mut stats = ColdStorageStats::default();

        if !self.cold_dir.exists() {
            return stats;
        }

        if let Ok(mut entries) = tokio::fs::read_dir(&self.cold_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                let filename = entry.file_name();
                let filename_str = filename.to_string_lossy();

                // Get file metadata
                if let Ok(metadata) = entry.metadata().await {
                    let size = metadata.len();
                    stats.total_bytes += size;

                    if filename_str.ends_with(".zst") {
                        stats.compressed_bytes += size;
                    } else {
                        stats.uncompressed_bytes += size;
                    }
                }

                // Parse the UUID from the filename
                let id_str = if filename_str.ends_with(".json.zst") {
                    filename_str.strip_suffix(".json.zst")
                } else if filename_str.ends_with(".json") {
                    filename_str.strip_suffix(".json")
                } else {
                    continue;
                };

                if let Some(id_str) = id_str {
                    if Uuid::parse_str(id_str).is_ok() {
                        let compressed = filename_str.ends_with(".zst");
                        if let Ok(Some(chunk)) = self.read_chunk(&path, compressed).await {
                            stats.total_files += 1;
                            stats.total_tokens += chunk.token_count;
                        }
                    }
                }
            }
        }

        stats
    }
}

/// Statistics for cold storage
#[derive(Debug, Clone, Default)]
pub struct ColdStorageStats {
    pub total_files: usize,
    pub total_bytes: u64,
    pub compressed_bytes: u64,
    pub uncompressed_bytes: u64,
    /// Total tokens across all cold chunks
    pub total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::chunk::Chunk;
    use tempfile::tempdir;

    #[test]
    fn test_cold_storage_new_with_compression() {
        let storage = ColdStorage::new(PathBuf::from("/test"), true);
        assert!(storage.compression_enabled);
        assert_eq!(storage.compression_level, 3);
    }

    #[test]
    fn test_cold_storage_new_without_compression() {
        let storage = ColdStorage::new(PathBuf::from("/test"), false);
        assert!(!storage.compression_enabled);
    }

    #[test]
    fn test_chunk_path_compressed() {
        let storage = ColdStorage::new(PathBuf::from("/test"), true);
        let id = Uuid::new_v4();
        let path = storage.chunk_path(id);
        assert!(path.to_string_lossy().ends_with(".json.zst"));
        assert!(path.to_string_lossy().contains(&id.to_string()));
    }

    #[test]
    fn test_chunk_path_uncompressed() {
        let storage = ColdStorage::new(PathBuf::from("/test"), false);
        let id = Uuid::new_v4();
        let path = storage.chunk_path(id);
        assert!(path.to_string_lossy().ends_with(".json"));
        assert!(!path.to_string_lossy().ends_with(".json.zst"));
    }

    #[tokio::test]
    async fn test_cold_storage_roundtrip() {
        let dir = tempdir().unwrap();
        let storage = ColdStorage::new(dir.path().to_path_buf(), true);

        let chunk = Chunk::new_message("user", "Test message for cold storage", None, 1);
        let id = chunk.id;

        // Store
        storage.put(chunk.clone()).await.unwrap();

        // Retrieve
        let retrieved = storage.get(id).await.unwrap().unwrap();
        assert_eq!(retrieved.id, id);

        // Delete
        storage.delete(id).await.unwrap();
        let deleted = storage.get(id).await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_cold_storage_roundtrip_uncompressed() {
        let dir = tempdir().unwrap();
        let storage = ColdStorage::new(dir.path().to_path_buf(), false);

        let chunk = Chunk::new_message("assistant", "Uncompressed test", None, 2);
        let id = chunk.id;

        storage.put(chunk.clone()).await.unwrap();
        let retrieved = storage.get(id).await.unwrap().unwrap();
        assert_eq!(retrieved.id, id);
    }

    #[tokio::test]
    async fn test_cold_storage_get_nonexistent() {
        let dir = tempdir().unwrap();
        let storage = ColdStorage::new(dir.path().to_path_buf(), true);

        let result = storage.get(Uuid::new_v4()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cold_storage_delete_nonexistent() {
        let dir = tempdir().unwrap();
        let storage = ColdStorage::new(dir.path().to_path_buf(), true);

        // Should not error when deleting nonexistent chunk
        storage.delete(Uuid::new_v4()).await.unwrap();
    }

    #[tokio::test]
    async fn test_cold_storage_list_all_empty() {
        let dir = tempdir().unwrap();
        let storage = ColdStorage::new(dir.path().to_path_buf(), true);

        let chunks = storage.list_all().await.unwrap();
        assert!(chunks.is_empty());
    }

    #[tokio::test]
    async fn test_cold_storage_list_all_nonexistent_dir() {
        let storage = ColdStorage::new(PathBuf::from("/nonexistent/path"), true);

        let chunks = storage.list_all().await.unwrap();
        assert!(chunks.is_empty());
    }

    #[tokio::test]
    async fn test_cold_storage_list_all_with_chunks() {
        let dir = tempdir().unwrap();
        let storage = ColdStorage::new(dir.path().to_path_buf(), true);

        let chunk1 = Chunk::new_message("user", "First", None, 1);
        let chunk2 = Chunk::new_message("assistant", "Second", None, 2);

        storage.put(chunk1.clone()).await.unwrap();
        storage.put(chunk2.clone()).await.unwrap();

        let chunks = storage.list_all().await.unwrap();
        assert_eq!(chunks.len(), 2);
    }

    #[tokio::test]
    async fn test_cold_storage_clear() {
        let dir = tempdir().unwrap();
        let mut storage = ColdStorage::new(dir.path().to_path_buf(), true);

        let chunk = Chunk::new_message("user", "Test", None, 1);
        storage.put(chunk).await.unwrap();

        let before = storage.list_all().await.unwrap();
        assert_eq!(before.len(), 1);

        storage.clear().await.unwrap();

        let after = storage.list_all().await.unwrap();
        assert!(after.is_empty());
    }

    #[tokio::test]
    async fn test_cold_storage_clear_nonexistent_dir() {
        let mut storage = ColdStorage::new(PathBuf::from("/nonexistent/path"), true);

        // Should not error when clearing nonexistent directory
        storage.clear().await.unwrap();
    }

    #[test]
    fn test_compression() {
        let storage = ColdStorage::new(PathBuf::new(), true);

        let data = b"Hello, this is a test message that should compress well. ".repeat(100);
        let compressed = storage.compress(&data).unwrap();
        let decompressed = storage.decompress(&compressed).unwrap();

        assert_eq!(data.as_slice(), decompressed.as_slice());
        assert!(compressed.len() < data.len()); // Should be smaller
    }

    #[test]
    fn test_compression_small_data() {
        let storage = ColdStorage::new(PathBuf::new(), true);

        let data = b"tiny";
        let compressed = storage.compress(data).unwrap();
        let decompressed = storage.decompress(&compressed).unwrap();

        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn test_compression_empty_data() {
        let storage = ColdStorage::new(PathBuf::new(), true);

        let data = b"";
        let compressed = storage.compress(data).unwrap();
        let decompressed = storage.decompress(&compressed).unwrap();

        assert_eq!(data.as_slice(), decompressed.as_slice());
    }

    #[tokio::test]
    async fn test_cold_storage_stats_empty() {
        let dir = tempdir().unwrap();
        let storage = ColdStorage::new(dir.path().to_path_buf(), true);

        let stats = storage.stats().await;
        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.total_bytes, 0);
    }

    #[tokio::test]
    async fn test_cold_storage_stats_with_files() {
        let dir = tempdir().unwrap();
        let storage = ColdStorage::new(dir.path().to_path_buf(), true);

        let chunk = Chunk::new_message("user", "Test message for stats", None, 1);
        storage.put(chunk).await.unwrap();

        let stats = storage.stats().await;
        assert_eq!(stats.total_files, 1);
        assert!(stats.total_bytes > 0);
        assert!(stats.compressed_bytes > 0);
        assert_eq!(stats.uncompressed_bytes, 0);
    }

    #[tokio::test]
    async fn test_cold_storage_stats_mixed_files() {
        let dir = tempdir().unwrap();

        // Create one compressed
        let storage_comp = ColdStorage::new(dir.path().to_path_buf(), true);
        let chunk1 = Chunk::new_message("user", "Compressed", None, 1);
        storage_comp.put(chunk1).await.unwrap();

        // Create one uncompressed
        let storage_uncomp = ColdStorage::new(dir.path().to_path_buf(), false);
        let chunk2 = Chunk::new_message("user", "Uncompressed", None, 2);
        storage_uncomp.put(chunk2).await.unwrap();

        let stats = storage_comp.stats().await;
        assert_eq!(stats.total_files, 2);
        assert!(stats.compressed_bytes > 0);
        assert!(stats.uncompressed_bytes > 0);
    }

    #[tokio::test]
    async fn test_cold_storage_stats_nonexistent_dir() {
        let storage = ColdStorage::new(PathBuf::from("/nonexistent/path"), true);

        let stats = storage.stats().await;
        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.total_bytes, 0);
    }

    #[test]
    fn test_cold_storage_stats_debug() {
        let stats = ColdStorageStats {
            total_files: 5,
            total_bytes: 1000,
            compressed_bytes: 800,
            uncompressed_bytes: 200,
            total_tokens: 500,
        };

        let debug_str = format!("{:?}", stats);
        assert!(debug_str.contains("5"));
        assert!(debug_str.contains("1000"));
    }

    #[test]
    fn test_cold_storage_stats_clone() {
        let stats = ColdStorageStats {
            total_files: 3,
            total_bytes: 500,
            compressed_bytes: 300,
            uncompressed_bytes: 200,
            total_tokens: 100,
        };

        let cloned = stats.clone();
        assert_eq!(cloned.total_files, 3);
        assert_eq!(cloned.total_bytes, 500);
    }

    #[tokio::test]
    async fn test_cold_storage_get_alternate_extension() {
        let dir = tempdir().unwrap();

        // Store with compression enabled
        let storage_comp = ColdStorage::new(dir.path().to_path_buf(), true);
        let chunk = Chunk::new_message("user", "Test", None, 1);
        let id = chunk.id;
        storage_comp.put(chunk).await.unwrap();

        // Try to get with compression disabled (should find alternate extension)
        let storage_uncomp = ColdStorage::new(dir.path().to_path_buf(), false);
        let retrieved = storage_uncomp.get(id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, id);
    }
}
