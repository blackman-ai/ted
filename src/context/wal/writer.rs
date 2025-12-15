// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! WAL writer for append-only durability
//!
//! The writer handles appending chunks to WAL files and rotating
//! files when they get too large.

use std::path::PathBuf;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};

use super::super::chunk::Chunk;
use super::{wal_filename, WalEntry};
use crate::error::{Result, TedError};

/// Maximum size of a single WAL file before rotation (1MB)
const MAX_WAL_SIZE: u64 = 1024 * 1024;

/// WAL writer for appending chunks
pub struct WalWriter {
    /// Directory containing WAL files
    wal_dir: PathBuf,
    /// Current WAL file sequence
    current_file_seq: u64,
    /// Current entry sequence within the file
    entry_seq: u64,
    /// Current file writer
    writer: Option<BufWriter<File>>,
    /// Current file size
    current_size: u64,
}

impl WalWriter {
    /// Create a new WAL writer
    pub async fn new(wal_dir: PathBuf) -> Result<Self> {
        // Find the latest WAL file
        let (file_seq, entry_seq) = Self::find_latest_sequence(&wal_dir).await?;

        let mut writer = Self {
            wal_dir,
            current_file_seq: file_seq,
            entry_seq,
            writer: None,
            current_size: 0,
        };

        // Open or create the current WAL file
        writer.ensure_writer().await?;

        Ok(writer)
    }

    /// Find the latest WAL file sequence
    async fn find_latest_sequence(wal_dir: &PathBuf) -> Result<(u64, u64)> {
        let mut max_file_seq = 0u64;
        let mut max_entry_seq = 0u64;

        let mut entries = tokio::fs::read_dir(wal_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let filename = entry.file_name();
            let filename = filename.to_string_lossy();

            if let Some(seq) = super::parse_wal_filename(&filename) {
                if seq >= max_file_seq {
                    max_file_seq = seq;

                    // Read the file to find max entry sequence
                    let content = tokio::fs::read_to_string(entry.path()).await?;
                    for line in content.lines() {
                        if let Ok(entry) = serde_json::from_str::<WalEntry>(line) {
                            max_entry_seq = max_entry_seq.max(entry.wal_sequence);
                        }
                    }
                }
            }
        }

        Ok((max_file_seq, max_entry_seq))
    }

    /// Ensure we have an open writer
    async fn ensure_writer(&mut self) -> Result<()> {
        if self.writer.is_none() {
            let path = self.wal_dir.join(wal_filename(self.current_file_seq));

            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await?;

            self.current_size = file.metadata().await?.len();
            self.writer = Some(BufWriter::new(file));
        }

        Ok(())
    }

    /// Append a chunk to the WAL
    pub async fn append(&mut self, chunk: &Chunk) -> Result<()> {
        self.ensure_writer().await?;

        self.entry_seq += 1;
        let entry = WalEntry::new(chunk.clone(), self.entry_seq);

        let line = serde_json::to_string(&entry)
            .map_err(|e| TedError::Context(format!("Failed to serialize WAL entry: {}", e)))?;

        if let Some(writer) = &mut self.writer {
            writer.write_all(line.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;

            self.current_size += line.len() as u64 + 1;
        }

        // Check if we need to rotate
        if self.current_size >= MAX_WAL_SIZE {
            self.rotate().await?;
        }

        Ok(())
    }

    /// Rotate to a new WAL file
    pub async fn rotate(&mut self) -> Result<()> {
        // Flush and close current writer
        if let Some(mut writer) = self.writer.take() {
            writer.flush().await?;
        }

        // Increment file sequence
        self.current_file_seq += 1;
        self.current_size = 0;

        // Create new file
        let path = self.wal_dir.join(wal_filename(self.current_file_seq));

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;

        self.writer = Some(BufWriter::new(file));

        // Clean up old WAL files (keep last 3)
        self.cleanup_old_files().await?;

        Ok(())
    }

    /// Clean up old WAL files
    async fn cleanup_old_files(&self) -> Result<()> {
        let keep_count = 3;

        let mut entries = tokio::fs::read_dir(&self.wal_dir).await?;
        let mut files: Vec<(u64, PathBuf)> = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let filename = entry.file_name();
            let filename = filename.to_string_lossy();

            if let Some(seq) = super::parse_wal_filename(&filename) {
                files.push((seq, entry.path()));
            }
        }

        // Sort by sequence (oldest first)
        files.sort_by_key(|(seq, _)| *seq);

        // Delete all but the last `keep_count` files
        if files.len() > keep_count {
            let to_delete = files.len() - keep_count;
            for (_, path) in files.into_iter().take(to_delete) {
                if let Err(e) = tokio::fs::remove_file(&path).await {
                    tracing::warn!("Failed to delete old WAL file {:?}: {}", path, e);
                }
            }
        }

        Ok(())
    }

    /// Clear all WAL files
    pub async fn clear(&mut self) -> Result<()> {
        // Close current writer
        if let Some(mut writer) = self.writer.take() {
            writer.flush().await?;
        }

        // Delete all WAL files
        let mut entries = tokio::fs::read_dir(&self.wal_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("wal") {
                tokio::fs::remove_file(path).await?;
            }
        }

        // Reset state
        self.current_file_seq = 0;
        self.entry_seq = 0;
        self.current_size = 0;

        Ok(())
    }

    /// Sync to disk (fsync)
    pub async fn sync(&mut self) -> Result<()> {
        if let Some(writer) = &mut self.writer {
            writer.flush().await?;
            writer.get_ref().sync_all().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::chunk::Chunk;
    use tempfile::TempDir;

    async fn create_test_writer() -> (WalWriter, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let writer = WalWriter::new(temp_dir.path().to_path_buf()).await.unwrap();
        (writer, temp_dir)
    }

    #[tokio::test]
    async fn test_writer_new() {
        let (writer, _temp_dir) = create_test_writer().await;
        assert_eq!(writer.current_file_seq, 0);
        assert_eq!(writer.entry_seq, 0);
    }

    #[tokio::test]
    async fn test_writer_append() {
        let (mut writer, temp_dir) = create_test_writer().await;

        let chunk = Chunk::new_message("user", "Hello, world!", None, 1);
        writer.append(&chunk).await.unwrap();

        // Check that a WAL file was created
        let wal_file = temp_dir.path().join("00000000.wal");
        assert!(wal_file.exists());

        // Check that the file has content
        let content = tokio::fs::read_to_string(&wal_file).await.unwrap();
        assert!(!content.is_empty());
        assert!(content.contains("Hello, world!"));
    }

    #[tokio::test]
    async fn test_writer_append_multiple() {
        let (mut writer, _temp_dir) = create_test_writer().await;

        let chunk1 = Chunk::new_message("user", "First message", None, 1);
        let chunk2 = Chunk::new_message("assistant", "Second message", None, 2);
        let chunk3 = Chunk::new_message("user", "Third message", None, 3);

        writer.append(&chunk1).await.unwrap();
        writer.append(&chunk2).await.unwrap();
        writer.append(&chunk3).await.unwrap();

        assert_eq!(writer.entry_seq, 3);
    }

    #[tokio::test]
    async fn test_writer_rotate() {
        let (mut writer, temp_dir) = create_test_writer().await;

        let chunk = Chunk::new_message("user", "Hello", None, 1);
        writer.append(&chunk).await.unwrap();

        // Manually rotate
        writer.rotate().await.unwrap();

        // Check that a new WAL file was created
        let new_wal_file = temp_dir.path().join("00000001.wal");
        assert!(new_wal_file.exists());
        assert_eq!(writer.current_file_seq, 1);
    }

    #[tokio::test]
    async fn test_writer_clear() {
        let (mut writer, temp_dir) = create_test_writer().await;

        let chunk = Chunk::new_message("user", "Hello", None, 1);
        writer.append(&chunk).await.unwrap();

        // Verify file exists
        let wal_file = temp_dir.path().join("00000000.wal");
        assert!(wal_file.exists());

        // Clear
        writer.clear().await.unwrap();

        // File should be deleted
        assert!(!wal_file.exists());
        assert_eq!(writer.current_file_seq, 0);
        assert_eq!(writer.entry_seq, 0);
    }

    #[tokio::test]
    async fn test_writer_sync() {
        let (mut writer, _temp_dir) = create_test_writer().await;

        let chunk = Chunk::new_message("user", "Hello", None, 1);
        writer.append(&chunk).await.unwrap();

        // Sync should complete without error
        writer.sync().await.unwrap();
    }

    #[tokio::test]
    async fn test_writer_append_increments_entry_seq() {
        let (mut writer, _temp_dir) = create_test_writer().await;

        assert_eq!(writer.entry_seq, 0);

        let chunk = Chunk::new_message("user", "Hello", None, 1);
        writer.append(&chunk).await.unwrap();
        assert_eq!(writer.entry_seq, 1);

        let chunk2 = Chunk::new_message("user", "World", None, 2);
        writer.append(&chunk2).await.unwrap();
        assert_eq!(writer.entry_seq, 2);
    }

    #[tokio::test]
    async fn test_writer_preserves_sequence_after_reopen() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        // First writer session
        {
            let mut writer = WalWriter::new(path.clone()).await.unwrap();
            let chunk = Chunk::new_message("user", "Hello", None, 1);
            writer.append(&chunk).await.unwrap();
            writer.append(&chunk).await.unwrap();
        }

        // Second writer session - should recover sequence
        {
            let writer = WalWriter::new(path.clone()).await.unwrap();
            assert!(writer.entry_seq >= 2);
        }
    }

    #[tokio::test]
    async fn test_writer_cleanup_old_files() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_path_buf();

        let mut writer = WalWriter::new(path.clone()).await.unwrap();

        // Create multiple WAL files by rotating
        for _ in 0..5 {
            let chunk = Chunk::new_message("user", "Hello", None, 1);
            writer.append(&chunk).await.unwrap();
            writer.rotate().await.unwrap();
        }

        // Count remaining WAL files (should keep last 3)
        let mut count = 0;
        let mut entries = tokio::fs::read_dir(&path).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            if entry.file_name().to_string_lossy().ends_with(".wal") {
                count += 1;
            }
        }

        // Should have at most 3 files (might be 4 if current file is counted)
        assert!(count <= 4);
    }

    #[tokio::test]
    async fn test_writer_different_chunk_types() {
        let (mut writer, _temp_dir) = create_test_writer().await;

        // Message chunk
        let msg_chunk = Chunk::new_message("user", "Hello", None, 1);
        writer.append(&msg_chunk).await.unwrap();

        // System chunk
        let sys_chunk = Chunk::new_system("System prompt", 2);
        writer.append(&sys_chunk).await.unwrap();

        // Tool call chunk
        let tool_chunk = Chunk::new_tool_call(
            "file_read",
            &serde_json::json!({"path": "/test"}),
            "content",
            false,
            None,
            3,
        );
        writer.append(&tool_chunk).await.unwrap();

        assert_eq!(writer.entry_seq, 3);
    }
}
