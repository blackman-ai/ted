// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! WAL reader for recovery and replay
//!
//! The reader handles reading and replaying WAL entries to recover
//! state after a restart.

use std::path::PathBuf;

use super::super::chunk::Chunk;
use super::{parse_wal_filename, WalEntry};
use crate::error::Result;

/// WAL reader for recovery
pub struct WalReader {
    /// Directory containing WAL files
    wal_dir: PathBuf,
}

impl WalReader {
    /// Create a new WAL reader
    pub fn new(wal_dir: PathBuf) -> Self {
        Self { wal_dir }
    }

    /// Read all entries from all WAL files
    pub async fn read_all(&self) -> Result<Vec<Chunk>> {
        let mut all_entries: Vec<WalEntry> = Vec::new();

        // Find all WAL files
        let mut files = self.list_wal_files().await?;

        // Sort by sequence (oldest first)
        files.sort_by_key(|(seq, _)| *seq);

        // Read each file
        for (_, path) in files {
            let entries = self.read_file(&path).await?;
            all_entries.extend(entries);
        }

        // Sort by WAL sequence to ensure order
        all_entries.sort_by_key(|e| e.wal_sequence);

        // Extract chunks, verifying checksums
        let chunks: Vec<Chunk> = all_entries
            .into_iter()
            .filter(|e| {
                if !e.verify() {
                    tracing::warn!(
                        "Skipping corrupted WAL entry (checksum mismatch): {:?}",
                        e.chunk.id
                    );
                    false
                } else {
                    true
                }
            })
            .map(|e| e.chunk)
            .collect();

        Ok(chunks)
    }

    /// List all WAL files in the directory
    async fn list_wal_files(&self) -> Result<Vec<(u64, PathBuf)>> {
        let mut files = Vec::new();

        if !self.wal_dir.exists() {
            return Ok(files);
        }

        let mut entries = tokio::fs::read_dir(&self.wal_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let filename = entry.file_name();
            let filename = filename.to_string_lossy();

            if let Some(seq) = parse_wal_filename(&filename) {
                files.push((seq, path));
            }
        }

        Ok(files)
    }

    /// Read entries from a single WAL file
    async fn read_file(&self, path: &PathBuf) -> Result<Vec<WalEntry>> {
        let content = match tokio::fs::read_to_string(path).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to read WAL file {:?}: {}", path, e);
                return Ok(Vec::new());
            }
        };

        let mut entries = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<WalEntry>(line) {
                Ok(entry) => entries.push(entry),
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse WAL entry at {:?}:{}: {}",
                        path,
                        line_num + 1,
                        e
                    );
                    // Continue reading other entries
                }
            }
        }

        Ok(entries)
    }

    /// Read entries newer than a given sequence
    pub async fn read_since(&self, sequence: u64) -> Result<Vec<Chunk>> {
        let all = self.read_all().await?;
        Ok(all.into_iter().filter(|c| c.sequence > sequence).collect())
    }

    /// Get the latest sequence number in the WAL
    pub async fn latest_sequence(&self) -> Result<u64> {
        let chunks = self.read_all().await?;
        Ok(chunks.into_iter().map(|c| c.sequence).max().unwrap_or(0))
    }

    /// Check if the WAL directory has any files
    pub async fn has_data(&self) -> bool {
        if let Ok(files) = self.list_wal_files().await {
            !files.is_empty()
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::WalWriter;
    use super::*;
    use crate::context::chunk::Chunk;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_reader_empty_dir() {
        let dir = tempdir().unwrap();
        let reader = WalReader::new(dir.path().to_path_buf());

        let chunks = reader.read_all().await.unwrap();
        assert!(chunks.is_empty());
    }

    #[tokio::test]
    async fn test_reader_nonexistent_dir() {
        let reader = WalReader::new(PathBuf::from("/nonexistent/path"));

        let chunks = reader.read_all().await.unwrap();
        assert!(chunks.is_empty());
    }

    #[tokio::test]
    async fn test_reader_has_data_empty() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");
        std::fs::create_dir_all(&wal_dir).unwrap();
        let reader = WalReader::new(wal_dir);

        assert!(!reader.has_data().await);
    }

    #[tokio::test]
    async fn test_reader_with_written_data() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");
        std::fs::create_dir_all(&wal_dir).unwrap();

        // Write some data using WalWriter
        let mut writer = WalWriter::new(wal_dir.clone()).await.unwrap();
        let chunk1 = Chunk::new_message("user", "Hello", None, 0);
        let chunk2 = Chunk::new_message("assistant", "Hi there", None, 1);
        writer.append(&chunk1).await.unwrap();
        writer.append(&chunk2).await.unwrap();
        writer.sync().await.unwrap();

        // Read back
        let reader = WalReader::new(wal_dir);
        let chunks = reader.read_all().await.unwrap();

        assert_eq!(chunks.len(), 2);
    }

    #[tokio::test]
    async fn test_reader_has_data_with_entries() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");
        std::fs::create_dir_all(&wal_dir).unwrap();

        let mut writer = WalWriter::new(wal_dir.clone()).await.unwrap();
        let chunk = Chunk::new_message("user", "test", None, 0);
        writer.append(&chunk).await.unwrap();
        writer.sync().await.unwrap();

        let reader = WalReader::new(wal_dir);
        assert!(reader.has_data().await);
    }

    #[tokio::test]
    async fn test_reader_latest_sequence() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");
        std::fs::create_dir_all(&wal_dir).unwrap();

        let mut writer = WalWriter::new(wal_dir.clone()).await.unwrap();
        writer
            .append(&Chunk::new_message("user", "test1", None, 10))
            .await
            .unwrap();
        writer
            .append(&Chunk::new_message("user", "test2", None, 20))
            .await
            .unwrap();
        writer
            .append(&Chunk::new_message("user", "test3", None, 15))
            .await
            .unwrap();
        writer.sync().await.unwrap();

        let reader = WalReader::new(wal_dir);
        let latest = reader.latest_sequence().await.unwrap();
        assert_eq!(latest, 20);
    }

    #[tokio::test]
    async fn test_reader_latest_sequence_empty() {
        let dir = tempdir().unwrap();
        let reader = WalReader::new(dir.path().to_path_buf());

        let latest = reader.latest_sequence().await.unwrap();
        assert_eq!(latest, 0);
    }

    #[tokio::test]
    async fn test_reader_read_since() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");
        std::fs::create_dir_all(&wal_dir).unwrap();

        let mut writer = WalWriter::new(wal_dir.clone()).await.unwrap();
        writer
            .append(&Chunk::new_message("user", "test1", None, 5))
            .await
            .unwrap();
        writer
            .append(&Chunk::new_message("user", "test2", None, 10))
            .await
            .unwrap();
        writer
            .append(&Chunk::new_message("user", "test3", None, 15))
            .await
            .unwrap();
        writer.sync().await.unwrap();

        let reader = WalReader::new(wal_dir);
        let chunks = reader.read_since(8).await.unwrap();

        // Should only get chunks with sequence > 8
        assert_eq!(chunks.len(), 2);
        assert!(chunks.iter().all(|c| c.sequence > 8));
    }

    #[tokio::test]
    async fn test_reader_skips_invalid_lines() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");
        std::fs::create_dir_all(&wal_dir).unwrap();

        // Write a file with some invalid JSON
        let content = "invalid json line\n{\"not a valid entry\": true}\n";
        std::fs::write(wal_dir.join("00000001.wal"), content).unwrap();

        let reader = WalReader::new(wal_dir);
        let chunks = reader.read_all().await.unwrap();

        // Should skip invalid lines without panicking
        assert!(chunks.is_empty());
    }

    #[tokio::test]
    async fn test_reader_skips_empty_lines() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");
        std::fs::create_dir_all(&wal_dir).unwrap();

        let mut writer = WalWriter::new(wal_dir.clone()).await.unwrap();
        writer
            .append(&Chunk::new_message("user", "test", None, 0))
            .await
            .unwrap();
        writer.sync().await.unwrap();

        // Append empty lines to the file
        let wal_file = wal_dir.join("00000000.wal");
        let mut content = std::fs::read_to_string(&wal_file).unwrap();
        content.push_str("\n\n  \n");
        std::fs::write(&wal_file, content).unwrap();

        let reader = WalReader::new(wal_dir);
        let chunks = reader.read_all().await.unwrap();

        // Should still read the valid entry
        assert_eq!(chunks.len(), 1);
    }

    #[tokio::test]
    async fn test_reader_multiple_wal_files() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");
        std::fs::create_dir_all(&wal_dir).unwrap();

        let mut writer = WalWriter::new(wal_dir.clone()).await.unwrap();

        // Write initial data
        writer
            .append(&Chunk::new_message("user", "first", None, 0))
            .await
            .unwrap();
        writer.sync().await.unwrap();

        // Force rotation to create a new file
        writer.rotate().await.unwrap();

        // Write more data
        writer
            .append(&Chunk::new_message("user", "second", None, 1))
            .await
            .unwrap();
        writer.sync().await.unwrap();

        let reader = WalReader::new(wal_dir);
        let chunks = reader.read_all().await.unwrap();

        // Should read from both files
        assert_eq!(chunks.len(), 2);
    }
}
