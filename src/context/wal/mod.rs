// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Write-Ahead Log (WAL) for context storage
//!
//! The WAL provides durability for hot-tier chunks. All new chunks are first
//! written to the WAL before being added to the in-memory cache.

mod reader;
mod writer;

pub use reader::WalReader;
pub use writer::WalWriter;

use serde::{Deserialize, Serialize};

/// A WAL entry wraps a chunk for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalEntry {
    /// Entry sequence number within the WAL file
    pub wal_sequence: u64,
    /// The chunk data
    pub chunk: super::chunk::Chunk,
    /// Checksum for integrity
    pub checksum: u32,
}

impl WalEntry {
    /// Create a new WAL entry
    pub fn new(chunk: super::chunk::Chunk, wal_sequence: u64) -> Self {
        let checksum = Self::compute_checksum(&chunk);
        Self {
            wal_sequence,
            chunk,
            checksum,
        }
    }

    /// Compute a simple checksum for the chunk
    fn compute_checksum(chunk: &super::chunk::Chunk) -> u32 {
        // Simple CRC32-like checksum using the chunk's serialized form
        let data = serde_json::to_vec(chunk).unwrap_or_default();
        let mut hash: u32 = 0;
        for byte in data {
            hash = hash.wrapping_add(byte as u32);
            hash = hash.wrapping_mul(31);
        }
        hash
    }

    /// Verify the checksum
    pub fn verify(&self) -> bool {
        Self::compute_checksum(&self.chunk) == self.checksum
    }
}

/// WAL file naming
pub fn wal_filename(sequence: u64) -> String {
    format!("{:08}.wal", sequence)
}

/// Parse WAL sequence from filename
pub fn parse_wal_filename(filename: &str) -> Option<u64> {
    filename
        .strip_suffix(".wal")
        .and_then(|s| s.parse::<u64>().ok())
}
