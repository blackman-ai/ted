// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Memory structures for the context indexer.
//!
//! These structs model human memory patterns for context prioritization:
//! - Recency + frequency = retention
//! - Decay over time
//! - Recall promotes back to active
//! - Associative memory (connected items get reinforced)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Detected programming language for a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Java,
    C,
    Cpp,
    CSharp,
    Ruby,
    Swift,
    Kotlin,
    Php,
    Shell,
    Markdown,
    Json,
    Yaml,
    Toml,
    #[default]
    Unknown,
}

impl Language {
    /// Detect language from file extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "rs" => Language::Rust,
            "ts" | "tsx" => Language::TypeScript,
            "js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
            "py" | "pyi" => Language::Python,
            "go" => Language::Go,
            "java" => Language::Java,
            "c" | "h" => Language::C,
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Language::Cpp,
            "cs" => Language::CSharp,
            "rb" => Language::Ruby,
            "swift" => Language::Swift,
            "kt" | "kts" => Language::Kotlin,
            "php" => Language::Php,
            "sh" | "bash" | "zsh" => Language::Shell,
            "md" | "markdown" => Language::Markdown,
            "json" => Language::Json,
            "yaml" | "yml" => Language::Yaml,
            "toml" => Language::Toml,
            _ => Language::Unknown,
        }
    }

    /// Check if this language supports syntax-aware parsing.
    pub fn supports_syntax_parsing(&self) -> bool {
        matches!(
            self,
            Language::Rust
                | Language::TypeScript
                | Language::JavaScript
                | Language::Python
                | Language::Go
        )
    }
}

/// Source location tracking for chunks.
///
/// Every chunk references its origin file and line numbers,
/// enabling jumping back to source and detecting invalidation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    /// Path to the source file.
    pub file_path: PathBuf,
    /// Starting line number (1-indexed).
    pub start_line: u32,
    /// Ending line number (1-indexed, inclusive).
    pub end_line: u32,
    /// Optional starting column for precise highlighting.
    pub start_col: Option<u32>,
    /// Optional ending column.
    pub end_col: Option<u32>,
}

impl SourceLocation {
    /// Create a new source location.
    pub fn new(file_path: PathBuf, start_line: u32, end_line: u32) -> Self {
        Self {
            file_path,
            start_line,
            end_line,
            start_col: None,
            end_col: None,
        }
    }

    /// Create a source location with column information.
    pub fn with_columns(
        file_path: PathBuf,
        start_line: u32,
        end_line: u32,
        start_col: u32,
        end_col: u32,
    ) -> Self {
        Self {
            file_path,
            start_line,
            end_line,
            start_col: Some(start_col),
            end_col: Some(end_col),
        }
    }

    /// Number of lines spanned by this location.
    pub fn line_count(&self) -> u32 {
        self.end_line.saturating_sub(self.start_line) + 1
    }
}

/// Type of symbol represented by a code chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SymbolType {
    /// A function or method.
    Function,
    /// A struct, class, or type definition.
    Struct,
    /// An enum definition.
    Enum,
    /// A trait or interface.
    Trait,
    /// An impl block.
    Impl,
    /// A module declaration.
    Module,
    /// A constant or static value.
    Constant,
    /// Import/use statements.
    Import,
    /// A test function or module.
    Test,
    /// Documentation or comments.
    Documentation,
    /// Unknown or generic code block.
    #[default]
    Unknown,
}

/// A code chunk with source tracking.
///
/// Represents an atomic unit of code (function, struct, impl block, etc.)
/// with full source location information for navigation and invalidation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    /// Unique identifier for this chunk.
    pub id: Uuid,
    /// The actual code content.
    pub content: String,
    /// Where this chunk came from.
    pub source: SourceLocation,
    /// Symbol name (e.g., "Config::new" or "main").
    pub symbol_name: Option<String>,
    /// Type of symbol this chunk represents.
    pub symbol_type: SymbolType,
    /// UUIDs of other chunks this one references.
    pub references: Vec<Uuid>,
    /// When this chunk was created.
    pub created_at: DateTime<Utc>,
    /// Content hash for change detection.
    pub content_hash: u64,
}

impl CodeChunk {
    /// Create a new code chunk.
    pub fn new(content: String, source: SourceLocation) -> Self {
        let content_hash = Self::hash_content(&content);
        Self {
            id: Uuid::new_v4(),
            content,
            source,
            symbol_name: None,
            symbol_type: SymbolType::Unknown,
            references: Vec::new(),
            created_at: Utc::now(),
            content_hash,
        }
    }

    /// Create a chunk with symbol information.
    pub fn with_symbol(
        content: String,
        source: SourceLocation,
        symbol_name: String,
        symbol_type: SymbolType,
    ) -> Self {
        let content_hash = Self::hash_content(&content);
        Self {
            id: Uuid::new_v4(),
            content,
            source,
            symbol_name: Some(symbol_name),
            symbol_type,
            references: Vec::new(),
            created_at: Utc::now(),
            content_hash,
        }
    }

    /// Add a reference to another chunk.
    pub fn add_reference(&mut self, chunk_id: Uuid) {
        if !self.references.contains(&chunk_id) {
            self.references.push(chunk_id);
        }
    }

    /// Check if content has changed by comparing hashes.
    pub fn content_changed(&self, new_content: &str) -> bool {
        Self::hash_content(new_content) != self.content_hash
    }

    /// Simple FNV-1a hash for content.
    fn hash_content(content: &str) -> u64 {
        const FNV_OFFSET: u64 = 14695981039346656037;
        const FNV_PRIME: u64 = 1099511628211;

        let mut hash = FNV_OFFSET;
        for byte in content.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }

    /// Estimated token count (rough heuristic: ~4 chars per token).
    pub fn estimated_tokens(&self) -> usize {
        self.content.len() / 4
    }
}

/// Per-file metadata tracked by the indexer.
///
/// Models the "memory" of a file based on access patterns,
/// git history, and dependency relationships.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMemory {
    /// Path to the file (relative to project root).
    pub path: PathBuf,

    // === Memory factors ===
    /// When this file was last accessed (read/edited).
    pub last_accessed: DateTime<Utc>,
    /// Total number of times this file has been accessed.
    pub access_count: u32,
    /// Computed retention score (higher = more likely to stay in context).
    pub retention_score: f64,

    // === Static analysis ===
    /// Files this one imports/depends on.
    pub dependencies: Vec<PathBuf>,
    /// Files that import/depend on this one.
    pub dependents: Vec<PathBuf>,
    /// PageRank-style centrality score.
    pub centrality_score: f64,

    // === Git metrics ===
    /// Number of commits that touched this file.
    pub commit_count: u32,
    /// Last modification time from git.
    pub last_modified: DateTime<Utc>,
    /// Churn rate (commits per time period, higher = more volatile).
    pub churn_rate: f64,

    // === Metadata ===
    /// Detected programming language.
    pub language: Language,
    /// Number of lines in the file.
    pub line_count: u32,
    /// Size in bytes.
    pub byte_size: u64,

    // === Chunk tracking ===
    /// UUIDs of chunks derived from this file.
    pub chunk_ids: Vec<Uuid>,
}

impl FileMemory {
    /// Create a new file memory entry.
    pub fn new(path: PathBuf) -> Self {
        let now = Utc::now();
        Self {
            path,
            last_accessed: now,
            access_count: 0,
            retention_score: 0.0,
            dependencies: Vec::new(),
            dependents: Vec::new(),
            centrality_score: 0.0,
            commit_count: 0,
            last_modified: now,
            churn_rate: 0.0,
            language: Language::Unknown,
            line_count: 0,
            byte_size: 0,
            chunk_ids: Vec::new(),
        }
    }

    /// Record an access to this file.
    pub fn record_access(&mut self) {
        self.last_accessed = Utc::now();
        self.access_count = self.access_count.saturating_add(1);
    }

    /// Time since last access in seconds.
    pub fn seconds_since_access(&self) -> i64 {
        (Utc::now() - self.last_accessed).num_seconds()
    }

    /// Check if file has dependencies.
    pub fn has_dependencies(&self) -> bool {
        !self.dependencies.is_empty()
    }

    /// Check if file has dependents.
    pub fn has_dependents(&self) -> bool {
        !self.dependents.is_empty()
    }

    /// Total connectivity (dependencies + dependents).
    pub fn connectivity(&self) -> usize {
        self.dependencies.len() + self.dependents.len()
    }
}

/// Memory metadata attached to a context chunk.
///
/// Combines global (persisted) and session (ephemeral) memory
/// for smart context prioritization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMemory {
    /// The chunk this memory is for.
    pub chunk_id: Uuid,

    // === Associative memory ===
    /// Chunks this one mentions/references.
    pub references: Vec<Uuid>,
    /// Chunks that mention/reference this one.
    pub referenced_by: Vec<Uuid>,

    // === Global memory (persisted) ===
    /// Total access count across all sessions.
    pub global_access_count: u32,
    /// Last access time (global).
    pub global_last_accessed: DateTime<Utc>,
    /// Centrality score from dependency graph.
    pub centrality_score: f64,
    /// Git churn rate for the source file.
    pub churn_rate: f64,

    // === Session memory (ephemeral) ===
    /// Access count in current session.
    pub session_access_count: u32,
    /// Last access in current session.
    pub session_last_accessed: Option<DateTime<Utc>>,
    /// Session-specific boost factor.
    pub session_boost: f64,
}

impl ChunkMemory {
    /// Create a new chunk memory.
    pub fn new(chunk_id: Uuid) -> Self {
        Self {
            chunk_id,
            references: Vec::new(),
            referenced_by: Vec::new(),
            global_access_count: 0,
            global_last_accessed: Utc::now(),
            centrality_score: 0.0,
            churn_rate: 0.0,
            session_access_count: 0,
            session_last_accessed: None,
            session_boost: 0.0,
        }
    }

    /// Record an access to this chunk.
    pub fn record_access(&mut self) {
        let now = Utc::now();
        self.global_access_count = self.global_access_count.saturating_add(1);
        self.global_last_accessed = now;
        self.session_access_count = self.session_access_count.saturating_add(1);
        self.session_last_accessed = Some(now);
    }

    /// Apply session boost (decays over time).
    pub fn apply_session_boost(&mut self, boost: f64) {
        self.session_boost = (self.session_boost + boost).min(1.0);
    }

    /// Clear session-specific state (called when session ends).
    pub fn clear_session(&mut self) {
        self.session_access_count = 0;
        self.session_last_accessed = None;
        self.session_boost = 0.0;
    }

    /// Add a reference to another chunk.
    pub fn add_reference(&mut self, chunk_id: Uuid) {
        if !self.references.contains(&chunk_id) {
            self.references.push(chunk_id);
        }
    }

    /// Record being referenced by another chunk.
    pub fn add_referenced_by(&mut self, chunk_id: Uuid) {
        if !self.referenced_by.contains(&chunk_id) {
            self.referenced_by.push(chunk_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        assert_eq!(Language::from_extension("rs"), Language::Rust);
        assert_eq!(Language::from_extension("ts"), Language::TypeScript);
        assert_eq!(Language::from_extension("tsx"), Language::TypeScript);
        assert_eq!(Language::from_extension("py"), Language::Python);
        assert_eq!(Language::from_extension("go"), Language::Go);
        assert_eq!(Language::from_extension("unknown"), Language::Unknown);
    }

    #[test]
    fn test_language_case_insensitive() {
        assert_eq!(Language::from_extension("RS"), Language::Rust);
        assert_eq!(Language::from_extension("Py"), Language::Python);
    }

    #[test]
    fn test_source_location() {
        let loc = SourceLocation::new(PathBuf::from("src/main.rs"), 10, 50);
        assert_eq!(loc.line_count(), 41);

        let loc_with_cols =
            SourceLocation::with_columns(PathBuf::from("src/main.rs"), 10, 50, 1, 80);
        assert_eq!(loc_with_cols.start_col, Some(1));
        assert_eq!(loc_with_cols.end_col, Some(80));
    }

    #[test]
    fn test_code_chunk_creation() {
        let source = SourceLocation::new(PathBuf::from("src/lib.rs"), 1, 10);
        let chunk = CodeChunk::new("fn main() {}".to_string(), source);

        assert!(!chunk.id.is_nil());
        assert_eq!(chunk.symbol_type, SymbolType::Unknown);
        assert!(chunk.references.is_empty());
    }

    #[test]
    fn test_code_chunk_with_symbol() {
        let source = SourceLocation::new(PathBuf::from("src/lib.rs"), 1, 10);
        let chunk = CodeChunk::with_symbol(
            "fn main() {}".to_string(),
            source,
            "main".to_string(),
            SymbolType::Function,
        );

        assert_eq!(chunk.symbol_name, Some("main".to_string()));
        assert_eq!(chunk.symbol_type, SymbolType::Function);
    }

    #[test]
    fn test_code_chunk_content_change_detection() {
        let source = SourceLocation::new(PathBuf::from("src/lib.rs"), 1, 10);
        let chunk = CodeChunk::new("fn main() {}".to_string(), source);

        assert!(!chunk.content_changed("fn main() {}"));
        assert!(chunk.content_changed("fn main() { println!(\"hello\"); }"));
    }

    #[test]
    fn test_code_chunk_references() {
        let source = SourceLocation::new(PathBuf::from("src/lib.rs"), 1, 10);
        let mut chunk = CodeChunk::new("fn main() {}".to_string(), source);

        let ref_id = Uuid::new_v4();
        chunk.add_reference(ref_id);
        chunk.add_reference(ref_id); // Duplicate should not be added

        assert_eq!(chunk.references.len(), 1);
        assert_eq!(chunk.references[0], ref_id);
    }

    #[test]
    fn test_file_memory_creation() {
        let memory = FileMemory::new(PathBuf::from("src/main.rs"));

        assert_eq!(memory.path, PathBuf::from("src/main.rs"));
        assert_eq!(memory.access_count, 0);
        assert_eq!(memory.language, Language::Unknown);
    }

    #[test]
    fn test_file_memory_access_recording() {
        let mut memory = FileMemory::new(PathBuf::from("src/main.rs"));

        memory.record_access();
        assert_eq!(memory.access_count, 1);

        memory.record_access();
        assert_eq!(memory.access_count, 2);
    }

    #[test]
    fn test_file_memory_connectivity() {
        let mut memory = FileMemory::new(PathBuf::from("src/main.rs"));

        memory.dependencies.push(PathBuf::from("src/lib.rs"));
        memory.dependents.push(PathBuf::from("src/app.rs"));
        memory.dependents.push(PathBuf::from("src/cli.rs"));

        assert!(memory.has_dependencies());
        assert!(memory.has_dependents());
        assert_eq!(memory.connectivity(), 3);
    }

    #[test]
    fn test_chunk_memory_creation() {
        let chunk_id = Uuid::new_v4();
        let memory = ChunkMemory::new(chunk_id);

        assert_eq!(memory.chunk_id, chunk_id);
        assert_eq!(memory.global_access_count, 0);
        assert_eq!(memory.session_access_count, 0);
    }

    #[test]
    fn test_chunk_memory_access_recording() {
        let chunk_id = Uuid::new_v4();
        let mut memory = ChunkMemory::new(chunk_id);

        memory.record_access();

        assert_eq!(memory.global_access_count, 1);
        assert_eq!(memory.session_access_count, 1);
        assert!(memory.session_last_accessed.is_some());
    }

    #[test]
    fn test_chunk_memory_session_boost() {
        let chunk_id = Uuid::new_v4();
        let mut memory = ChunkMemory::new(chunk_id);

        memory.apply_session_boost(0.5);
        assert_eq!(memory.session_boost, 0.5);

        memory.apply_session_boost(0.7);
        assert_eq!(memory.session_boost, 1.0); // Capped at 1.0
    }

    #[test]
    fn test_chunk_memory_clear_session() {
        let chunk_id = Uuid::new_v4();
        let mut memory = ChunkMemory::new(chunk_id);

        memory.record_access();
        memory.apply_session_boost(0.5);
        memory.clear_session();

        assert_eq!(memory.global_access_count, 1); // Global preserved
        assert_eq!(memory.session_access_count, 0);
        assert!(memory.session_last_accessed.is_none());
        assert_eq!(memory.session_boost, 0.0);
    }

    #[test]
    fn test_chunk_memory_references() {
        let chunk_id = Uuid::new_v4();
        let mut memory = ChunkMemory::new(chunk_id);

        let ref1 = Uuid::new_v4();
        let ref2 = Uuid::new_v4();

        memory.add_reference(ref1);
        memory.add_referenced_by(ref2);

        assert_eq!(memory.references.len(), 1);
        assert_eq!(memory.referenced_by.len(), 1);
    }

    #[test]
    fn test_estimated_tokens() {
        let source = SourceLocation::new(PathBuf::from("test.rs"), 1, 1);
        let chunk = CodeChunk::new("fn main() { let x = 42; }".to_string(), source);

        // 25 chars / 4 = 6 tokens (rough estimate)
        assert_eq!(chunk.estimated_tokens(), 6);
    }
}
