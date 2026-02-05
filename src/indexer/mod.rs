// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Memory-based context prioritization system.
//!
//! This module implements a smart context loader that models human memory:
//! - Recency + frequency = retention
//! - Decay over time
//! - Recall promotes back to active
//! - Associative memory (connected items get reinforced)
//!
//! # Architecture
//!
//! The indexer tracks files and code chunks with memory-like properties:
//! - **FileMemory**: Per-file metadata including access patterns, git metrics, and dependencies
//! - **ChunkMemory**: Per-chunk metadata with both global and session-specific state
//! - **Scorer**: Calculates retention scores using configurable weights
//!
//! # Usage
//!
//! ```no_run
//! use ted::indexer::{Indexer, IndexerConfig};
//! use std::path::Path;
//!
//! # async fn example() -> ted::Result<()> {
//! let config = IndexerConfig::default();
//! let mut indexer = Indexer::new(Path::new("/path/to/project"), config)?;
//!
//! // Full scan of the project
//! indexer.full_scan()?;
//!
//! // Record file access
//! indexer.record_file_access(Path::new("src/main.rs"));
//!
//! // Get top files for context
//! let top_files = indexer.top_files(10);
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod daemon;
pub mod git;
pub mod graph;
pub mod languages;
pub mod memory;
pub mod persistence;
pub mod recall;
pub mod scorer;
pub mod vector;

use std::path::{Path, PathBuf};

use chrono::Utc;

pub use config::{DaemonConfig, DaemonEvent, LanguagesConfig, LimitsConfig};
pub use daemon::{Daemon, DaemonBuilder, DaemonHandle};
pub use git::{FileGitMetrics, GitAnalyzer};
pub use graph::{DependencyGraph, GraphNode, GraphStats};
pub use languages::{ExportKind, ExportRef, ImportKind, ImportRef, LanguageParser, ParserRegistry};
pub use memory::{ChunkMemory, CodeChunk, FileMemory, Language, SourceLocation, SymbolType};
pub use persistence::{IndexStore, PersistedIndex, StorageStats};
pub use recall::{
    extract_paths_from_text, recall_channel, FileChangeType, ProcessedRecalls, RecallEvent,
    RecallProcessor, RecallReceiver, RecallSender,
};
pub use scorer::{Scorer, ScoringConfig};
pub use vector::{cosine_similarity, reciprocal_rank_fusion, HybridSearchResult, VectorIndex};

use crate::error::{Result, TedError};

/// Configuration for semantic search
#[derive(Debug, Clone)]
pub struct SemanticSearchConfig {
    /// Enable semantic search indexing (requires embeddings)
    pub enabled: bool,
    /// Embedding dimension (depends on model used)
    pub embedding_dimension: usize,
    /// Minimum similarity threshold for search results
    pub similarity_threshold: f32,
    /// Maximum results to return
    pub max_results: usize,
}

impl Default for SemanticSearchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            embedding_dimension: 384, // all-MiniLM-L6-v2 default
            similarity_threshold: 0.3,
            max_results: 20,
        }
    }
}

/// Configuration for the indexer.
#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// Scoring configuration.
    pub scoring: ScoringConfig,
    /// Maximum number of files to keep in hot context.
    pub max_files: usize,
    /// Maximum bytes for hot context.
    pub max_bytes: u64,
    /// File extensions to index (empty = all).
    pub extensions: Vec<String>,
    /// Patterns to ignore.
    pub ignore_patterns: Vec<String>,
    /// Semantic search configuration
    pub semantic_search: SemanticSearchConfig,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            scoring: ScoringConfig::default(),
            max_files: 50,
            max_bytes: 102400, // 100KB
            extensions: Vec::new(),
            ignore_patterns: vec![
                "node_modules".into(),
                "target".into(),
                ".git".into(),
                "dist".into(),
                "build".into(),
                "__pycache__".into(),
                ".venv".into(),
                "vendor".into(),
            ],
            semantic_search: SemanticSearchConfig::default(),
        }
    }
}

impl IndexerConfig {
    /// Check if a file should be indexed.
    pub fn should_index(&self, path: &Path) -> bool {
        // Check ignore patterns
        let path_str = path.to_string_lossy();
        for pattern in &self.ignore_patterns {
            if path_str.contains(pattern) {
                return false;
            }
        }

        // Check extensions if specified
        if !self.extensions.is_empty() {
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if !self.extensions.iter().any(|e| e.to_lowercase() == ext_str) {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }
}

/// The main indexer for context prioritization.
pub struct Indexer {
    /// Project root directory.
    root: PathBuf,
    /// Configuration.
    config: IndexerConfig,
    /// Scorer for retention calculations.
    scorer: Scorer,
    /// Git analyzer (optional, if in git repo).
    git: Option<GitAnalyzer>,
    /// Persisted index.
    index: PersistedIndex,
    /// Index storage.
    store: IndexStore,
    /// Dependency graph.
    graph: DependencyGraph,
    /// Language parser registry.
    parsers: ParserRegistry,
    /// Vector index for semantic search (optional, enabled via config)
    vector_index: Option<VectorIndex>,
}

impl Indexer {
    /// Create a new indexer for a project.
    pub fn new(root: &Path, config: IndexerConfig) -> Result<Self> {
        let root = root
            .canonicalize()
            .map_err(|e| TedError::Config(format!("Failed to canonicalize project root: {}", e)))?;

        let scorer = Scorer::with_config(config.scoring.clone());
        let git = GitAnalyzer::open(&root).ok();
        let store = IndexStore::new()?;
        let index = store.load_or_create(&root)?;
        let graph = DependencyGraph::new(root.clone());
        let parsers = ParserRegistry::new();

        // Initialize vector index if semantic search is enabled
        let vector_index = if config.semantic_search.enabled {
            Some(VectorIndex::new(config.semantic_search.embedding_dimension))
        } else {
            None
        };

        Ok(Self {
            root,
            config,
            scorer,
            git,
            index,
            store,
            graph,
            parsers,
            vector_index,
        })
    }

    /// Get the project root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the configuration.
    pub fn config(&self) -> &IndexerConfig {
        &self.config
    }

    /// Get the scorer.
    pub fn scorer(&self) -> &Scorer {
        &self.scorer
    }

    /// Check if git is available.
    pub fn has_git(&self) -> bool {
        self.git.is_some()
    }

    /// Perform a full scan of the project.
    pub fn full_scan(&mut self) -> Result<ScanStats> {
        let mut stats = ScanStats::default();
        let start = std::time::Instant::now();

        // Get all files
        let files = self.collect_files()?;
        stats.files_scanned = files.len();

        // Get git metrics if available
        let git_metrics = self
            .git
            .as_ref()
            .and_then(|g| g.analyze_all().ok())
            .unwrap_or_default();

        // Update commit hash
        if let Some(ref git) = self.git {
            self.index.git_commit = git.head_commit_hash();
        }

        // Collect file contents for graph building
        let mut file_contents: Vec<(PathBuf, String)> = Vec::new();

        // Process each file
        for path in files {
            let relative = path.strip_prefix(&self.root).unwrap_or(&path).to_path_buf();

            let mut file_memory = self
                .index
                .files
                .remove(&relative)
                .unwrap_or_else(|| FileMemory::new(relative.clone()));

            // Update file metadata
            if let Ok(metadata) = std::fs::metadata(&path) {
                file_memory.byte_size = metadata.len();
            }

            if let Ok(content) = std::fs::read_to_string(&path) {
                file_memory.line_count = content.lines().count() as u32;
                // Store content for graph building
                file_contents.push((path.clone(), content));
            }

            // Detect language
            if let Some(ext) = path.extension() {
                file_memory.language = Language::from_extension(&ext.to_string_lossy());
            }

            // Apply git metrics
            if let Some(metrics) = git_metrics.get(&relative) {
                file_memory.commit_count = metrics.commit_count;
                file_memory.churn_rate = metrics.normalized_churn();
                if let Some(last_mod) = metrics.last_modified {
                    file_memory.last_modified = last_mod;
                }
            }

            // Calculate initial retention score (will be updated with centrality)
            file_memory.retention_score = self.scorer.file_retention_score(&file_memory);

            self.index.upsert_file(file_memory);
            stats.files_indexed += 1;
        }

        // Build dependency graph
        let graph_input: Vec<_> = file_contents
            .iter()
            .map(|(p, c)| (p.as_path(), c.as_str()))
            .collect();

        if let Ok(graph_stats) = self
            .graph
            .build_from_files(graph_input.into_iter(), &self.parsers)
        {
            stats.imports_found = graph_stats.imports_found;
            stats.imports_resolved = graph_stats.imports_resolved;
        }

        // Update file memory with centrality scores and dependencies
        for node in self.graph.nodes() {
            if let Some(file) = self.index.get_file_mut(&node.path) {
                file.centrality_score = node.centrality;
                file.dependencies = node.dependencies.clone();
                file.dependents = node.dependents.clone();
                // Recalculate retention score with centrality
                file.retention_score = self.scorer.file_retention_score(file);
            }
        }

        // Remove stale entries (files that no longer exist)
        let stale: Vec<_> = self
            .index
            .files
            .keys()
            .filter(|p| !self.root.join(p).exists())
            .cloned()
            .collect();

        for path in stale {
            self.index.remove_file(&path);
            self.graph.remove_file(&path);
            stats.files_removed += 1;
        }

        stats.duration_ms = start.elapsed().as_millis() as u64;

        // Persist the index
        self.save()?;

        Ok(stats)
    }

    /// Record an access to a file (updates recency and frequency).
    pub fn record_file_access(&mut self, path: &Path) {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);

        if let Some(file) = self.index.get_file_mut(relative) {
            file.record_access();
            file.retention_score = self.scorer.file_retention_score(file);
        }
    }

    /// Record access to a chunk.
    pub fn record_chunk_access(&mut self, chunk_id: uuid::Uuid) {
        if let Some(memory) = self.index.get_chunk_memory_mut(chunk_id) {
            memory.record_access();

            // Boost referenced chunks (associative memory)
            let refs: Vec<_> = memory.references.clone();
            let boost = self.scorer.associative_boost();

            for ref_id in refs {
                if let Some(ref_memory) = self.index.get_chunk_memory_mut(ref_id) {
                    ref_memory.apply_session_boost(boost);
                }
            }
        }
    }

    /// Get top N files by retention score.
    pub fn top_files(&self, n: usize) -> Vec<&FileMemory> {
        self.scorer
            .select_top_files(&self.index.files.values().cloned().collect::<Vec<_>>(), n)
            .into_iter()
            .map(|f| {
                // Return references from our index
                self.index.get_file(&f.path).unwrap()
            })
            .collect()
    }

    /// Get files within a byte budget.
    pub fn files_within_budget(&self, max_bytes: u64) -> Vec<&FileMemory> {
        let files: Vec<_> = self.index.files.values().cloned().collect();
        self.scorer
            .select_within_budget(&files, max_bytes)
            .into_iter()
            .filter_map(|f| self.index.get_file(&f.path))
            .collect()
    }

    /// Recalculate all retention scores.
    pub fn recalculate_scores(&mut self) {
        let paths: Vec<_> = self.index.files.keys().cloned().collect();

        for path in paths {
            if let Some(file) = self.index.get_file_mut(&path) {
                file.retention_score = self.scorer.file_retention_score(file);
            }
        }
    }

    /// Clear session state (called when session ends).
    pub fn clear_session(&mut self) {
        let chunk_ids: Vec<_> = self.index.chunk_memory.keys().cloned().collect();

        for id in chunk_ids {
            if let Some(memory) = self.index.get_chunk_memory_mut(id) {
                memory.clear_session();
            }
        }
    }

    /// Get statistics about the index.
    pub fn stats(&self) -> IndexStats {
        let files = &self.index.files;

        IndexStats {
            file_count: files.len(),
            chunk_count: self.index.chunks.len(),
            total_bytes: files.values().map(|f| f.byte_size).sum(),
            total_lines: files.values().map(|f| f.line_count as u64).sum(),
            git_available: self.git.is_some(),
            last_updated: self.index.updated_at,
        }
    }

    /// Save the index to disk.
    pub fn save(&self) -> Result<()> {
        self.store.save(&self.index)
    }

    /// Get the persisted index.
    pub fn index(&self) -> &PersistedIndex {
        &self.index
    }

    /// Get a mutable reference to the persisted index.
    pub fn index_mut(&mut self) -> &mut PersistedIndex {
        &mut self.index
    }

    /// Get the dependency graph.
    pub fn graph(&self) -> &DependencyGraph {
        &self.graph
    }

    /// Get the parser registry.
    pub fn parsers(&self) -> &ParserRegistry {
        &self.parsers
    }

    /// Get the most central files in the project.
    pub fn central_files(&self, n: usize) -> Vec<&GraphNode> {
        self.graph.top_central(n)
    }

    /// Get files that depend on a given file (transitive).
    pub fn dependents_of(&self, path: &Path) -> std::collections::HashSet<PathBuf> {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        self.graph.transitive_dependents(relative)
    }

    /// Get files that a given file depends on (transitive).
    pub fn dependencies_of(&self, path: &Path) -> std::collections::HashSet<PathBuf> {
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        self.graph.transitive_dependencies(relative)
    }

    /// Collect all files to index.
    fn collect_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for entry in walkdir::WalkDir::new(&self.root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let path = e.path();
                self.config.should_index(path)
            })
        {
            let entry =
                entry.map_err(|e| TedError::Config(format!("Failed to walk directory: {}", e)))?;

            if entry.file_type().is_file() {
                files.push(entry.path().to_path_buf());
            }
        }

        Ok(files)
    }

    // ===== Semantic Search Methods =====

    /// Check if semantic search is enabled
    pub fn has_semantic_search(&self) -> bool {
        self.vector_index.is_some()
    }

    /// Enable semantic search (creates vector index if not already enabled)
    pub fn enable_semantic_search(&mut self, dimension: usize) {
        if self.vector_index.is_none() {
            self.vector_index = Some(VectorIndex::new(dimension));
        }
    }

    /// Get the vector index (if enabled)
    pub fn vector_index(&self) -> Option<&VectorIndex> {
        self.vector_index.as_ref()
    }

    /// Get mutable access to the vector index (if enabled)
    pub fn vector_index_mut(&mut self) -> Option<&mut VectorIndex> {
        self.vector_index.as_mut()
    }

    /// Add an embedding for a chunk
    ///
    /// Returns true if the embedding was added, false if semantic search is disabled.
    pub fn add_chunk_embedding(&mut self, chunk_id: uuid::Uuid, embedding: Vec<f32>) -> bool {
        if let Some(ref mut vector_index) = self.vector_index {
            vector_index.insert(chunk_id, embedding);
            true
        } else {
            false
        }
    }

    /// Add embeddings for multiple chunks
    pub fn add_chunk_embeddings(
        &mut self,
        embeddings: impl IntoIterator<Item = (uuid::Uuid, Vec<f32>)>,
    ) {
        if let Some(ref mut vector_index) = self.vector_index {
            vector_index.insert_batch(embeddings);
        }
    }

    /// Remove a chunk's embedding
    pub fn remove_chunk_embedding(&mut self, chunk_id: &uuid::Uuid) -> Option<Vec<f32>> {
        self.vector_index.as_mut()?.remove(chunk_id)
    }

    /// Perform semantic search for chunks similar to a query embedding
    ///
    /// Returns chunk IDs with their similarity scores, sorted by descending similarity.
    pub fn semantic_search(&self, query_embedding: &[f32], k: usize) -> Vec<(uuid::Uuid, f32)> {
        if let Some(ref vector_index) = self.vector_index {
            let max_results = k.min(self.config.semantic_search.max_results);
            vector_index.search_with_threshold(
                query_embedding,
                max_results,
                self.config.semantic_search.similarity_threshold,
            )
        } else {
            vec![]
        }
    }

    /// Perform hybrid search combining semantic similarity with importance scores
    ///
    /// Returns chunks with combined scores using Reciprocal Rank Fusion.
    pub fn hybrid_search(
        &self,
        query_embedding: &[f32],
        k: usize,
        semantic_weight: f32,
    ) -> Vec<HybridSearchResult> {
        // Get semantic results
        let semantic_results = self.semantic_search(query_embedding, k * 2);

        // Get top chunks by importance score (based on access patterns and centrality)
        // This serves as a "relevance from usage" signal
        let mut importance_results: Vec<(uuid::Uuid, f32)> = self
            .index
            .chunk_memory
            .iter()
            .map(|(id, memory)| {
                // Calculate importance score from available fields
                let recency_factor = memory
                    .session_last_accessed
                    .or(Some(memory.global_last_accessed))
                    .map(|t| {
                        let hours_ago = (Utc::now() - t).num_hours() as f32;
                        1.0 / (1.0 + hours_ago.max(0.0) / 24.0) // Decay over days
                    })
                    .unwrap_or(0.0);

                let access_factor = (memory.global_access_count as f32
                    + memory.session_access_count as f32 * 2.0)
                    .ln_1p();

                let centrality_factor = memory.centrality_score as f32;

                // Combine factors
                let score = recency_factor * 0.3 + access_factor * 0.4 + centrality_factor * 0.3;

                (*id, score)
            })
            .collect();

        importance_results
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        importance_results.truncate(k * 2);

        // Combine using RRF with configurable constant
        // Higher constant = smoother combination, lower = more emphasis on top ranks
        let k_constant = 60.0 * (1.0 - semantic_weight) + 1.0;

        let mut results = reciprocal_rank_fusion(semantic_results, importance_results, k_constant);
        results.truncate(k);
        results
    }

    /// Get statistics about the vector index
    pub fn vector_index_stats(&self) -> Option<VectorIndexStats> {
        self.vector_index.as_ref().map(|vi| VectorIndexStats {
            vector_count: vi.len(),
            dimension: vi.dimension(),
            memory_usage_bytes: vi.memory_usage(),
        })
    }
}

/// Statistics about the vector index
#[derive(Debug, Clone)]
pub struct VectorIndexStats {
    /// Number of vectors in the index
    pub vector_count: usize,
    /// Embedding dimension
    pub dimension: usize,
    /// Estimated memory usage in bytes
    pub memory_usage_bytes: usize,
}

/// Statistics from a full scan.
#[derive(Debug, Clone, Default)]
pub struct ScanStats {
    /// Number of files scanned.
    pub files_scanned: usize,
    /// Number of files indexed.
    pub files_indexed: usize,
    /// Number of stale files removed.
    pub files_removed: usize,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Number of imports found in all files.
    pub imports_found: usize,
    /// Number of imports resolved to local files.
    pub imports_resolved: usize,
}

impl ScanStats {
    /// Import resolution rate as a percentage.
    pub fn resolution_rate(&self) -> f64 {
        if self.imports_found == 0 {
            100.0
        } else {
            (self.imports_resolved as f64 / self.imports_found as f64) * 100.0
        }
    }
}

/// Index statistics.
#[derive(Debug, Clone)]
pub struct IndexStats {
    /// Number of tracked files.
    pub file_count: usize,
    /// Number of chunks.
    pub chunk_count: usize,
    /// Total size in bytes.
    pub total_bytes: u64,
    /// Total lines of code.
    pub total_lines: u64,
    /// Whether git is available.
    pub git_available: bool,
    /// Last update time.
    pub last_updated: chrono::DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_project() -> TempDir {
        let temp = TempDir::new().unwrap();

        // Create some test files
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(temp.path().join("src/lib.rs"), "pub mod utils;").unwrap();
        std::fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        temp
    }

    #[test]
    fn test_indexer_config_default() {
        let config = IndexerConfig::default();

        assert_eq!(config.max_files, 50);
        assert_eq!(config.max_bytes, 102400);
        assert!(config.extensions.is_empty());
        assert!(!config.ignore_patterns.is_empty());
    }

    #[test]
    fn test_indexer_config_should_index() {
        let config = IndexerConfig::default();

        // Should index normal files
        assert!(config.should_index(Path::new("src/main.rs")));
        assert!(config.should_index(Path::new("lib/utils.js")));

        // Should not index ignored paths
        assert!(!config.should_index(Path::new("node_modules/lib.js")));
        assert!(!config.should_index(Path::new("target/debug/main")));
        assert!(!config.should_index(Path::new(".git/config")));
    }

    #[test]
    fn test_indexer_config_extension_filter() {
        let config = IndexerConfig {
            extensions: vec!["rs".into(), "toml".into()],
            ..Default::default()
        };

        assert!(config.should_index(Path::new("src/main.rs")));
        assert!(config.should_index(Path::new("Cargo.toml")));
        assert!(!config.should_index(Path::new("src/main.js")));
        assert!(!config.should_index(Path::new("README.md")));
    }

    #[test]
    fn test_indexer_creation() {
        let temp = create_test_project();
        let config = IndexerConfig::default();

        let indexer = Indexer::new(temp.path(), config);
        assert!(indexer.is_ok());

        let indexer = indexer.unwrap();
        assert!(indexer.root().exists());
    }

    #[test]
    fn test_indexer_full_scan() {
        let temp = create_test_project();
        let config = IndexerConfig::default();

        let mut indexer = Indexer::new(temp.path(), config).unwrap();
        let stats = indexer.full_scan().unwrap();

        assert!(stats.files_scanned > 0);
        assert!(stats.files_indexed > 0);
        // duration_ms is u64, always >= 0
        let _ = stats.duration_ms;
    }

    #[test]
    fn test_indexer_record_access() {
        let temp = create_test_project();
        let config = IndexerConfig::default();

        let mut indexer = Indexer::new(temp.path(), config).unwrap();
        indexer.full_scan().unwrap();

        let path = Path::new("src/main.rs");

        // Get initial access count
        let initial_count = indexer
            .index()
            .get_file(path)
            .map(|f| f.access_count)
            .unwrap_or(0);

        // Record access
        indexer.record_file_access(path);

        // Access count should increase
        let new_count = indexer
            .index()
            .get_file(path)
            .map(|f| f.access_count)
            .unwrap_or(0);
        assert_eq!(new_count, initial_count + 1);
    }

    #[test]
    fn test_indexer_top_files() {
        let temp = create_test_project();
        let config = IndexerConfig::default();

        let mut indexer = Indexer::new(temp.path(), config).unwrap();
        indexer.full_scan().unwrap();

        let top = indexer.top_files(2);
        assert!(top.len() <= 2);
    }

    #[test]
    fn test_indexer_stats() {
        let temp = create_test_project();
        let config = IndexerConfig::default();

        let mut indexer = Indexer::new(temp.path(), config).unwrap();
        indexer.full_scan().unwrap();

        let stats = indexer.stats();
        assert!(stats.file_count > 0);
        assert!(stats.total_bytes > 0);
    }

    #[test]
    fn test_scan_stats_default() {
        let stats = ScanStats::default();
        assert_eq!(stats.files_scanned, 0);
        assert_eq!(stats.files_indexed, 0);
        assert_eq!(stats.files_removed, 0);
    }
}
