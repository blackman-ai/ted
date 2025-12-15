// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Persistence layer for the context indexer.
//!
//! Stores index data at ~/.ted/index/{project_hash}.json

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::git::project_hash;
use super::memory::{ChunkMemory, CodeChunk, FileMemory};
use super::scorer::ScoringConfig;
use crate::error::{Result, TedError};

/// The persisted index structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedIndex {
    /// Version for format compatibility.
    pub version: u32,
    /// When this index was last updated.
    pub updated_at: DateTime<Utc>,
    /// Project root path (for verification).
    pub project_root: PathBuf,
    /// Git commit hash at time of indexing.
    pub git_commit: Option<String>,
    /// File memory entries.
    pub files: HashMap<PathBuf, FileMemory>,
    /// Code chunks.
    pub chunks: HashMap<uuid::Uuid, CodeChunk>,
    /// Chunk memory entries.
    pub chunk_memory: HashMap<uuid::Uuid, ChunkMemory>,
    /// Scoring configuration used.
    pub scoring_config: ScoringConfig,
}

impl PersistedIndex {
    /// Current format version.
    pub const VERSION: u32 = 1;

    /// Create a new empty index.
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            version: Self::VERSION,
            updated_at: Utc::now(),
            project_root,
            git_commit: None,
            files: HashMap::new(),
            chunks: HashMap::new(),
            chunk_memory: HashMap::new(),
            scoring_config: ScoringConfig::default(),
        }
    }

    /// Check if the index format is compatible.
    pub fn is_compatible(&self) -> bool {
        self.version == Self::VERSION
    }

    /// Update the timestamp.
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    /// Get a file memory entry.
    pub fn get_file(&self, path: &Path) -> Option<&FileMemory> {
        self.files.get(path)
    }

    /// Get a mutable file memory entry.
    pub fn get_file_mut(&mut self, path: &Path) -> Option<&mut FileMemory> {
        self.files.get_mut(path)
    }

    /// Insert or update a file memory entry.
    pub fn upsert_file(&mut self, file: FileMemory) {
        self.files.insert(file.path.clone(), file);
        self.touch();
    }

    /// Remove a file memory entry.
    pub fn remove_file(&mut self, path: &Path) -> Option<FileMemory> {
        let removed = self.files.remove(path);
        if removed.is_some() {
            self.touch();
        }
        removed
    }

    /// Get a code chunk.
    pub fn get_chunk(&self, id: uuid::Uuid) -> Option<&CodeChunk> {
        self.chunks.get(&id)
    }

    /// Insert or update a code chunk.
    pub fn upsert_chunk(&mut self, chunk: CodeChunk) {
        self.chunks.insert(chunk.id, chunk);
        self.touch();
    }

    /// Remove a code chunk.
    pub fn remove_chunk(&mut self, id: uuid::Uuid) -> Option<CodeChunk> {
        let removed = self.chunks.remove(&id);
        self.chunk_memory.remove(&id);
        if removed.is_some() {
            self.touch();
        }
        removed
    }

    /// Get chunk memory.
    pub fn get_chunk_memory(&self, id: uuid::Uuid) -> Option<&ChunkMemory> {
        self.chunk_memory.get(&id)
    }

    /// Get mutable chunk memory.
    pub fn get_chunk_memory_mut(&mut self, id: uuid::Uuid) -> Option<&mut ChunkMemory> {
        self.chunk_memory.get_mut(&id)
    }

    /// Insert or update chunk memory.
    pub fn upsert_chunk_memory(&mut self, memory: ChunkMemory) {
        self.chunk_memory.insert(memory.chunk_id, memory);
        self.touch();
    }

    /// Get all files sorted by retention score.
    pub fn files_by_score(&self) -> Vec<&FileMemory> {
        let mut files: Vec<_> = self.files.values().collect();
        files.sort_by(|a, b| {
            b.retention_score
                .partial_cmp(&a.retention_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        files
    }

    /// Get chunks for a specific file.
    pub fn chunks_for_file(&self, path: &Path) -> Vec<&CodeChunk> {
        self.chunks
            .values()
            .filter(|c| c.source.file_path == path)
            .collect()
    }

    /// Total number of tracked files.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Total number of chunks.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

/// Index storage manager.
pub struct IndexStore {
    /// Base directory for index storage.
    base_dir: PathBuf,
}

impl IndexStore {
    /// Create a new index store.
    ///
    /// Uses ~/.ted/index/ as the base directory.
    pub fn new() -> Result<Self> {
        let base_dir = dirs::home_dir()
            .ok_or_else(|| TedError::Config("Could not find home directory".into()))?
            .join(".ted")
            .join("index");

        fs::create_dir_all(&base_dir)
            .map_err(|e| TedError::Config(format!("Failed to create index directory: {}", e)))?;

        Ok(Self { base_dir })
    }

    /// Create an index store with a custom base directory.
    pub fn with_base_dir(base_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&base_dir)
            .map_err(|e| TedError::Config(format!("Failed to create index directory: {}", e)))?;

        Ok(Self { base_dir })
    }

    /// Get the index file path for a project.
    pub fn index_path(&self, project_root: &Path) -> PathBuf {
        let hash = project_hash(project_root);
        self.base_dir.join(format!("{}.json", hash))
    }

    /// Load an index for a project.
    pub fn load(&self, project_root: &Path) -> Result<Option<PersistedIndex>> {
        let path = self.index_path(project_root);

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| TedError::Config(format!("Failed to read index file: {}", e)))?;

        let index: PersistedIndex = serde_json::from_str(&content)
            .map_err(|e| TedError::Config(format!("Failed to parse index file: {}", e)))?;

        // Check version compatibility
        if !index.is_compatible() {
            tracing::warn!(
                "Index version {} is incompatible with current version {}",
                index.version,
                PersistedIndex::VERSION
            );
            return Ok(None);
        }

        Ok(Some(index))
    }

    /// Load or create an index for a project.
    pub fn load_or_create(&self, project_root: &Path) -> Result<PersistedIndex> {
        match self.load(project_root)? {
            Some(index) => Ok(index),
            None => Ok(PersistedIndex::new(project_root.to_path_buf())),
        }
    }

    /// Save an index.
    pub fn save(&self, index: &PersistedIndex) -> Result<()> {
        let path = self.index_path(&index.project_root);

        let content = serde_json::to_string_pretty(index)
            .map_err(|e| TedError::Config(format!("Failed to serialize index: {}", e)))?;

        // Write atomically via temp file
        let temp_path = path.with_extension("json.tmp");
        fs::write(&temp_path, &content)
            .map_err(|e| TedError::Config(format!("Failed to write index file: {}", e)))?;

        fs::rename(&temp_path, &path)
            .map_err(|e| TedError::Config(format!("Failed to rename index file: {}", e)))?;

        Ok(())
    }

    /// Delete an index.
    pub fn delete(&self, project_root: &Path) -> Result<()> {
        let path = self.index_path(project_root);

        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| TedError::Config(format!("Failed to delete index file: {}", e)))?;
        }

        Ok(())
    }

    /// List all indexed projects.
    pub fn list_projects(&self) -> Result<Vec<PathBuf>> {
        let mut projects = Vec::new();

        let entries = fs::read_dir(&self.base_dir)
            .map_err(|e| TedError::Config(format!("Failed to read index directory: {}", e)))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(index) = serde_json::from_str::<PersistedIndex>(&content) {
                        projects.push(index.project_root);
                    }
                }
            }
        }

        Ok(projects)
    }

    /// Get storage statistics.
    pub fn stats(&self) -> Result<StorageStats> {
        let mut stats = StorageStats::default();

        let entries = fs::read_dir(&self.base_dir)
            .map_err(|e| TedError::Config(format!("Failed to read index directory: {}", e)))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                stats.index_count += 1;
                if let Ok(meta) = fs::metadata(&path) {
                    stats.total_bytes += meta.len();
                }
            }
        }

        Ok(stats)
    }
}

impl Default for IndexStore {
    fn default() -> Self {
        Self::new().expect("Failed to create default IndexStore")
    }
}

/// Storage statistics.
#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    /// Number of index files.
    pub index_count: usize,
    /// Total storage in bytes.
    pub total_bytes: u64,
}

impl StorageStats {
    /// Format total bytes as human-readable string.
    pub fn formatted_size(&self) -> String {
        if self.total_bytes < 1024 {
            format!("{} B", self.total_bytes)
        } else if self.total_bytes < 1024 * 1024 {
            format!("{:.1} KB", self.total_bytes as f64 / 1024.0)
        } else {
            format!("{:.1} MB", self.total_bytes as f64 / (1024.0 * 1024.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (IndexStore, TempDir) {
        let temp = TempDir::new().unwrap();
        let store = IndexStore::with_base_dir(temp.path().to_path_buf()).unwrap();
        (store, temp)
    }

    #[test]
    fn test_persisted_index_creation() {
        let index = PersistedIndex::new(PathBuf::from("/test/project"));

        assert_eq!(index.version, PersistedIndex::VERSION);
        assert_eq!(index.project_root, PathBuf::from("/test/project"));
        assert!(index.files.is_empty());
        assert!(index.chunks.is_empty());
    }

    #[test]
    fn test_persisted_index_file_operations() {
        let mut index = PersistedIndex::new(PathBuf::from("/test"));

        let file = FileMemory::new(PathBuf::from("src/main.rs"));
        index.upsert_file(file);

        assert_eq!(index.file_count(), 1);
        assert!(index.get_file(Path::new("src/main.rs")).is_some());

        index.remove_file(Path::new("src/main.rs"));
        assert_eq!(index.file_count(), 0);
    }

    #[test]
    fn test_persisted_index_chunk_operations() {
        use super::super::memory::{SourceLocation, SymbolType};

        let mut index = PersistedIndex::new(PathBuf::from("/test"));

        let source = SourceLocation::new(PathBuf::from("src/lib.rs"), 1, 10);
        let chunk = CodeChunk::with_symbol(
            "fn main() {}".to_string(),
            source,
            "main".to_string(),
            SymbolType::Function,
        );
        let chunk_id = chunk.id;

        index.upsert_chunk(chunk);
        assert_eq!(index.chunk_count(), 1);
        assert!(index.get_chunk(chunk_id).is_some());

        index.remove_chunk(chunk_id);
        assert_eq!(index.chunk_count(), 0);
    }

    #[test]
    fn test_index_store_save_load() {
        let (store, _temp) = create_test_store();
        let project_root = PathBuf::from("/test/project");

        let mut index = PersistedIndex::new(project_root.clone());
        let file = FileMemory::new(PathBuf::from("src/main.rs"));
        index.upsert_file(file);

        // Save
        store.save(&index).unwrap();

        // Load
        let loaded = store.load(&project_root).unwrap().unwrap();
        assert_eq!(loaded.file_count(), 1);
        assert!(loaded.get_file(Path::new("src/main.rs")).is_some());
    }

    #[test]
    fn test_index_store_load_nonexistent() {
        let (store, _temp) = create_test_store();
        let result = store.load(Path::new("/nonexistent")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_index_store_load_or_create() {
        let (store, _temp) = create_test_store();
        let project_root = PathBuf::from("/test/project");

        // First call creates new
        let index1 = store.load_or_create(&project_root).unwrap();
        assert_eq!(index1.file_count(), 0);

        // Modify and save
        let mut index2 = index1;
        index2.upsert_file(FileMemory::new(PathBuf::from("test.rs")));
        store.save(&index2).unwrap();

        // Second call loads existing
        let index3 = store.load_or_create(&project_root).unwrap();
        assert_eq!(index3.file_count(), 1);
    }

    #[test]
    fn test_index_store_delete() {
        let (store, _temp) = create_test_store();
        let project_root = PathBuf::from("/test/project");

        let index = PersistedIndex::new(project_root.clone());
        store.save(&index).unwrap();

        assert!(store.load(&project_root).unwrap().is_some());

        store.delete(&project_root).unwrap();
        assert!(store.load(&project_root).unwrap().is_none());
    }

    #[test]
    fn test_index_store_list_projects() {
        let (store, _temp) = create_test_store();

        let index1 = PersistedIndex::new(PathBuf::from("/project1"));
        let index2 = PersistedIndex::new(PathBuf::from("/project2"));

        store.save(&index1).unwrap();
        store.save(&index2).unwrap();

        let projects = store.list_projects().unwrap();
        assert_eq!(projects.len(), 2);
    }

    #[test]
    fn test_index_store_stats() {
        let (store, _temp) = create_test_store();

        let index = PersistedIndex::new(PathBuf::from("/test"));
        store.save(&index).unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.index_count, 1);
        assert!(stats.total_bytes > 0);
    }

    #[test]
    fn test_storage_stats_formatting() {
        let stats_bytes = StorageStats {
            total_bytes: 500,
            ..Default::default()
        };
        assert!(stats_bytes.formatted_size().contains("B"));

        let stats_kb = StorageStats {
            total_bytes: 1500,
            ..Default::default()
        };
        assert!(stats_kb.formatted_size().contains("KB"));

        let stats_mb = StorageStats {
            total_bytes: 1500000,
            ..Default::default()
        };
        assert!(stats_mb.formatted_size().contains("MB"));
    }

    #[test]
    fn test_files_by_score() {
        let mut index = PersistedIndex::new(PathBuf::from("/test"));

        let mut file1 = FileMemory::new(PathBuf::from("low.rs"));
        file1.retention_score = 0.2;

        let mut file2 = FileMemory::new(PathBuf::from("high.rs"));
        file2.retention_score = 0.8;

        let mut file3 = FileMemory::new(PathBuf::from("medium.rs"));
        file3.retention_score = 0.5;

        index.upsert_file(file1);
        index.upsert_file(file2);
        index.upsert_file(file3);

        let sorted = index.files_by_score();
        assert_eq!(sorted[0].path, PathBuf::from("high.rs"));
        assert_eq!(sorted[1].path, PathBuf::from("medium.rs"));
        assert_eq!(sorted[2].path, PathBuf::from("low.rs"));
    }

    #[test]
    fn test_chunks_for_file() {
        use super::super::memory::SourceLocation;

        let mut index = PersistedIndex::new(PathBuf::from("/test"));

        let source1 = SourceLocation::new(PathBuf::from("src/main.rs"), 1, 10);
        let chunk1 = CodeChunk::new("fn a() {}".to_string(), source1);

        let source2 = SourceLocation::new(PathBuf::from("src/main.rs"), 11, 20);
        let chunk2 = CodeChunk::new("fn b() {}".to_string(), source2);

        let source3 = SourceLocation::new(PathBuf::from("src/lib.rs"), 1, 10);
        let chunk3 = CodeChunk::new("fn c() {}".to_string(), source3);

        index.upsert_chunk(chunk1);
        index.upsert_chunk(chunk2);
        index.upsert_chunk(chunk3);

        let main_chunks = index.chunks_for_file(Path::new("src/main.rs"));
        assert_eq!(main_chunks.len(), 2);

        let lib_chunks = index.chunks_for_file(Path::new("src/lib.rs"));
        assert_eq!(lib_chunks.len(), 1);
    }
}
