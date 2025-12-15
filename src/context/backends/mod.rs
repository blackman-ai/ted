// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Storage backends
//!
//! This module provides the storage backend abstraction for warm storage.

pub mod filesystem;

use super::chunk::Chunk;
use crate::error::Result;
use async_trait::async_trait;

/// Statistics for a storage tier
#[derive(Debug, Clone, Default)]
pub struct TierStats {
    /// Number of chunks in this tier
    pub chunk_count: usize,
    /// Total estimated tokens in this tier
    pub total_tokens: u32,
    /// Total storage bytes used
    pub storage_bytes: u64,
}

/// Storage backend trait for warm storage
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Write a chunk to storage
    async fn write(&self, key: &str, chunk: &Chunk) -> Result<()>;

    /// Read a chunk from storage
    async fn read(&self, key: &str) -> Result<Option<Chunk>>;

    /// Delete a chunk from storage
    async fn delete(&self, key: &str) -> Result<()>;

    /// List all chunks
    async fn list_all(&self) -> Result<Vec<Chunk>>;

    /// Check if a chunk exists
    async fn exists(&self, key: &str) -> Result<bool>;

    /// Clear all stored chunks
    async fn clear(&self) -> Result<()>;

    /// Get statistics for this storage tier
    async fn stats(&self) -> Result<TierStats>;
}
