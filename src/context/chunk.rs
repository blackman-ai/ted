// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Chunk data structures
//!
//! Chunks are the fundamental unit of context storage. Each chunk represents
//! a piece of conversation context (message, tool call, summary, etc.).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// A chunk of conversation context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    /// Unique identifier
    pub id: Uuid,
    /// Type of chunk
    pub chunk_type: ChunkType,
    /// The actual content
    pub content: ChunkContent,
    /// Parent chunk ID (for threading)
    pub parent_id: Option<Uuid>,
    /// Child chunk IDs
    pub children: Vec<Uuid>,
    /// Estimated token count
    pub token_count: u32,
    /// Priority for retention during compaction
    pub priority: ChunkPriority,
    /// Sequence number (for ordering)
    pub sequence: u64,
    /// Current storage tier
    pub storage_tier: StorageTier,
    /// When this chunk was created
    pub created_at: DateTime<Utc>,
    /// When this chunk was last accessed
    pub accessed_at: DateTime<Utc>,
    /// File paths referenced by this chunk (for memory integration)
    #[serde(default)]
    pub referenced_files: Vec<PathBuf>,
    /// Related chunk IDs (for associative memory)
    #[serde(default)]
    pub related_chunks: Vec<Uuid>,
    /// Memory retention score (computed from indexer)
    #[serde(default)]
    pub retention_score: f64,
}

impl Chunk {
    /// Create a new chunk
    pub fn new(
        chunk_type: ChunkType,
        content: ChunkContent,
        parent_id: Option<Uuid>,
        sequence: u64,
    ) -> Self {
        let token_count = content.estimate_tokens();
        let priority = chunk_type.default_priority();
        let referenced_files = content.extract_file_paths();
        let now = Utc::now();

        Self {
            id: Uuid::new_v4(),
            chunk_type,
            content,
            parent_id,
            children: Vec::new(),
            token_count,
            priority,
            sequence,
            storage_tier: StorageTier::Hot,
            created_at: now,
            accessed_at: now,
            referenced_files,
            related_chunks: Vec::new(),
            retention_score: 0.0,
        }
    }

    /// Create a new message chunk
    pub fn new_message(role: &str, content: &str, parent_id: Option<Uuid>, sequence: u64) -> Self {
        Self::new(
            ChunkType::Message,
            ChunkContent::Message {
                role: role.to_string(),
                content: content.to_string(),
            },
            parent_id,
            sequence,
        )
    }

    /// Create a new tool call chunk
    pub fn new_tool_call(
        tool_name: &str,
        input: &serde_json::Value,
        output: &str,
        is_error: bool,
        parent_id: Option<Uuid>,
        sequence: u64,
    ) -> Self {
        Self::new(
            ChunkType::ToolCall,
            ChunkContent::ToolCall {
                tool_name: tool_name.to_string(),
                input: input.clone(),
                output: output.to_string(),
                is_error,
            },
            parent_id,
            sequence,
        )
    }

    /// Create a new summary chunk
    pub fn new_summary(
        summary: &str,
        summarized_chunks: Vec<Uuid>,
        parent_id: Option<Uuid>,
        sequence: u64,
    ) -> Self {
        Self::new(
            ChunkType::Summary,
            ChunkContent::Summary {
                text: summary.to_string(),
                summarized_chunks,
            },
            parent_id,
            sequence,
        )
    }

    /// Create a new system context chunk
    pub fn new_system(content: &str, sequence: u64) -> Self {
        Self::new(
            ChunkType::System,
            ChunkContent::System {
                content: content.to_string(),
            },
            None,
            sequence,
        )
    }

    /// Create a new file tree chunk (core memory - never compacted)
    pub fn new_file_tree(
        root_name: &str,
        tree: &str,
        file_count: usize,
        dir_count: usize,
        truncated: bool,
        sequence: u64,
    ) -> Self {
        Self::new(
            ChunkType::FileTree,
            ChunkContent::FileTree {
                root_name: root_name.to_string(),
                tree: tree.to_string(),
                file_count,
                dir_count,
                truncated,
            },
            None,
            sequence,
        )
    }

    /// Mark chunk as accessed (updates accessed_at)
    pub fn touch(&mut self) {
        self.accessed_at = Utc::now();
    }

    /// Demote to a lower storage tier
    pub fn demote(&mut self) {
        self.storage_tier = match self.storage_tier {
            StorageTier::Hot => StorageTier::Warm,
            StorageTier::Warm => StorageTier::Cold,
            StorageTier::Cold => StorageTier::Cold, // Already at lowest
        };
    }

    /// Promote to a higher storage tier
    pub fn promote(&mut self) {
        self.storage_tier = match self.storage_tier {
            StorageTier::Hot => StorageTier::Hot, // Already at highest
            StorageTier::Warm => StorageTier::Hot,
            StorageTier::Cold => StorageTier::Warm,
        };
    }

    /// Check if this chunk can be compacted
    pub fn can_compact(&self) -> bool {
        match self.priority {
            ChunkPriority::Critical => false, // Never compact critical chunks
            ChunkPriority::High => self.storage_tier == StorageTier::Cold,
            ChunkPriority::Normal | ChunkPriority::Low => true,
        }
    }

    /// Add a related chunk reference.
    pub fn add_related(&mut self, chunk_id: Uuid) {
        if !self.related_chunks.contains(&chunk_id) {
            self.related_chunks.push(chunk_id);
        }
    }

    /// Add a file reference.
    pub fn add_file_reference(&mut self, path: PathBuf) {
        if !self.referenced_files.contains(&path) {
            self.referenced_files.push(path);
        }
    }

    /// Update retention score from indexer.
    pub fn set_retention_score(&mut self, score: f64) {
        self.retention_score = score;
    }

    /// Get effective priority considering retention score.
    ///
    /// Combines the static priority with the dynamic retention score.
    pub fn effective_priority(&self) -> f64 {
        let base = match self.priority {
            ChunkPriority::Critical => 1.0,
            ChunkPriority::High => 0.75,
            ChunkPriority::Normal => 0.5,
            ChunkPriority::Low => 0.25,
        };

        // Blend static priority with dynamic retention score
        // Weight: 70% static, 30% dynamic
        (base * 0.7) + (self.retention_score * 0.3)
    }

    /// Check if this chunk references a specific file.
    pub fn references_file(&self, path: &std::path::Path) -> bool {
        self.referenced_files.iter().any(|p| p == path)
    }
}

/// Types of chunks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkType {
    /// A conversation message (user or assistant)
    Message,
    /// A tool call and its result
    ToolCall,
    /// A summary of other chunks
    Summary,
    /// System context (project info, caps, etc.)
    System,
    /// File content that was read
    FileContent,
    /// Metadata about the session
    Metadata,
    /// Project file tree structure (core memory - never compacted)
    FileTree,
}

impl ChunkType {
    /// Get the default priority for this chunk type
    pub fn default_priority(&self) -> ChunkPriority {
        match self {
            ChunkType::Message => ChunkPriority::High,
            ChunkType::ToolCall => ChunkPriority::Normal,
            ChunkType::Summary => ChunkPriority::High, // Summaries are valuable
            ChunkType::System => ChunkPriority::Critical,
            ChunkType::FileContent => ChunkPriority::Low,
            ChunkType::Metadata => ChunkPriority::Normal,
            ChunkType::FileTree => ChunkPriority::Critical, // Core memory - never compacted
        }
    }
}

/// The actual content of a chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChunkContent {
    /// A message in the conversation
    Message { role: String, content: String },
    /// A tool call and result
    ToolCall {
        tool_name: String,
        input: serde_json::Value,
        output: String,
        is_error: bool,
    },
    /// A summary of previous chunks
    Summary {
        text: String,
        summarized_chunks: Vec<Uuid>,
    },
    /// System context
    System { content: String },
    /// File content
    FileContent {
        path: String,
        content: String,
        language: Option<String>,
    },
    /// Session metadata
    Metadata {
        key: String,
        value: serde_json::Value,
    },
    /// Project file tree structure
    FileTree {
        /// Root directory name
        root_name: String,
        /// Tree string representation
        tree: String,
        /// Number of files in tree
        file_count: usize,
        /// Number of directories in tree
        dir_count: usize,
        /// Whether tree was truncated
        truncated: bool,
    },
}

impl ChunkContent {
    /// Estimate token count for this content
    ///
    /// Uses a simple heuristic of ~4 characters per token.
    /// For more accuracy, use tiktoken-rs.
    pub fn estimate_tokens(&self) -> u32 {
        let text_len = match self {
            ChunkContent::Message { content, .. } => content.len(),
            ChunkContent::ToolCall { input, output, .. } => input.to_string().len() + output.len(),
            ChunkContent::Summary { text, .. } => text.len(),
            ChunkContent::System { content } => content.len(),
            ChunkContent::FileContent { content, .. } => content.len(),
            ChunkContent::Metadata { value, .. } => value.to_string().len(),
            ChunkContent::FileTree {
                tree, root_name, ..
            } => tree.len() + root_name.len() + 50, // overhead for header
        };

        // Rough estimate: 1 token ≈ 4 characters
        (text_len / 4) as u32
    }

    /// Get the text content (for display or search)
    pub fn text(&self) -> String {
        match self {
            ChunkContent::Message { content, .. } => content.clone(),
            ChunkContent::ToolCall {
                tool_name, output, ..
            } => {
                format!("Tool: {}\nOutput: {}", tool_name, output)
            }
            ChunkContent::Summary { text, .. } => text.clone(),
            ChunkContent::System { content } => content.clone(),
            ChunkContent::FileContent { path, content, .. } => {
                format!("File: {}\n{}", path, content)
            }
            ChunkContent::Metadata { key, value } => {
                format!("{}: {}", key, value)
            }
            ChunkContent::FileTree {
                root_name,
                tree,
                file_count,
                dir_count,
                truncated,
            } => {
                let mut result = format!("Project structure ({}):\n{}", root_name, tree);
                if !truncated {
                    result.push_str(&format!(
                        "\n({} files, {} directories)",
                        file_count, dir_count
                    ));
                }
                result
            }
        }
    }

    /// Extract file paths referenced in this content.
    pub fn extract_file_paths(&self) -> Vec<PathBuf> {
        match self {
            ChunkContent::FileContent { path, .. } => {
                vec![PathBuf::from(path)]
            }
            ChunkContent::ToolCall {
                tool_name, input, ..
            } => {
                // Extract paths from tool inputs for file-related tools
                let mut paths = Vec::new();
                if matches!(
                    tool_name.as_str(),
                    "file_read" | "file_edit" | "file_write" | "glob" | "grep"
                ) {
                    if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                        paths.push(PathBuf::from(path));
                    }
                    if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                        // For glob patterns, store the pattern itself
                        paths.push(PathBuf::from(pattern));
                    }
                }
                paths
            }
            _ => Vec::new(),
        }
    }
}

/// Priority levels for chunk retention
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ChunkPriority {
    /// Never delete (system prompts, critical context)
    Critical,
    /// Keep as long as possible (recent messages, important summaries)
    High,
    /// Standard retention
    Normal,
    /// Can be deleted first when space is needed
    Low,
}

/// Storage tier for a chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageTier {
    /// In-memory and WAL (most recent)
    Hot,
    /// On disk as individual files
    Warm,
    /// Compressed archive (oldest)
    Cold,
}

impl StorageTier {
    /// Get the tier as a string (for directory names)
    pub fn as_str(&self) -> &'static str {
        match self {
            StorageTier::Hot => "wal",
            StorageTier::Warm => "chunks",
            StorageTier::Cold => "cold",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_message_chunk() {
        let chunk = Chunk::new_message("user", "Hello, world!", None, 0);
        assert_eq!(chunk.chunk_type, ChunkType::Message);
        assert_eq!(chunk.storage_tier, StorageTier::Hot);
        assert!(chunk.token_count > 0);
    }

    #[test]
    fn test_chunk_demote() {
        let mut chunk = Chunk::new_message("user", "test", None, 0);
        assert_eq!(chunk.storage_tier, StorageTier::Hot);

        chunk.demote();
        assert_eq!(chunk.storage_tier, StorageTier::Warm);

        chunk.demote();
        assert_eq!(chunk.storage_tier, StorageTier::Cold);

        chunk.demote();
        assert_eq!(chunk.storage_tier, StorageTier::Cold); // Stays at cold
    }

    #[test]
    fn test_token_estimation() {
        let content = ChunkContent::Message {
            role: "user".to_string(),
            content: "Hello, this is a test message!".to_string(),
        };
        // ~30 chars / 4 = ~7 tokens
        let tokens = content.estimate_tokens();
        assert!((5..=10).contains(&tokens));
    }

    #[test]
    fn test_chunk_promote() {
        let mut chunk = Chunk::new_message("user", "test", None, 0);
        chunk.storage_tier = StorageTier::Cold;

        chunk.promote();
        assert_eq!(chunk.storage_tier, StorageTier::Warm);

        chunk.promote();
        assert_eq!(chunk.storage_tier, StorageTier::Hot);

        chunk.promote();
        assert_eq!(chunk.storage_tier, StorageTier::Hot); // Stays at hot
    }

    #[test]
    fn test_chunk_touch() {
        let mut chunk = Chunk::new_message("user", "test", None, 0);
        let original_time = chunk.accessed_at;

        std::thread::sleep(std::time::Duration::from_millis(10));
        chunk.touch();

        assert!(chunk.accessed_at > original_time);
    }

    #[test]
    fn test_new_tool_call_chunk() {
        let input = serde_json::json!({"path": "/test"});
        let chunk = Chunk::new_tool_call("file_read", &input, "file contents", false, None, 1);

        assert_eq!(chunk.chunk_type, ChunkType::ToolCall);
        assert_eq!(chunk.sequence, 1);

        if let ChunkContent::ToolCall {
            tool_name,
            is_error,
            ..
        } = &chunk.content
        {
            assert_eq!(tool_name, "file_read");
            assert!(!is_error);
        } else {
            panic!("Expected ToolCall content");
        }
    }

    #[test]
    fn test_new_tool_call_with_error() {
        let input = serde_json::json!({});
        let chunk = Chunk::new_tool_call("shell", &input, "error occurred", true, None, 1);

        if let ChunkContent::ToolCall { is_error, .. } = &chunk.content {
            assert!(is_error);
        } else {
            panic!("Expected ToolCall content");
        }
    }

    #[test]
    fn test_new_summary_chunk() {
        let summarized = vec![Uuid::new_v4(), Uuid::new_v4()];
        let chunk = Chunk::new_summary("This is a summary", summarized.clone(), None, 2);

        assert_eq!(chunk.chunk_type, ChunkType::Summary);
        assert_eq!(chunk.priority, ChunkPriority::High);

        if let ChunkContent::Summary {
            text,
            summarized_chunks,
        } = &chunk.content
        {
            assert_eq!(text, "This is a summary");
            assert_eq!(summarized_chunks.len(), 2);
        } else {
            panic!("Expected Summary content");
        }
    }

    #[test]
    fn test_new_system_chunk() {
        let chunk = Chunk::new_system("System context here", 0);

        assert_eq!(chunk.chunk_type, ChunkType::System);
        assert_eq!(chunk.priority, ChunkPriority::Critical);
        assert!(chunk.parent_id.is_none());

        if let ChunkContent::System { content } = &chunk.content {
            assert_eq!(content, "System context here");
        } else {
            panic!("Expected System content");
        }
    }

    #[test]
    fn test_chunk_with_parent() {
        let parent_id = Uuid::new_v4();
        let chunk = Chunk::new_message("assistant", "Response", Some(parent_id), 1);

        assert_eq!(chunk.parent_id, Some(parent_id));
    }

    #[test]
    fn test_can_compact_critical() {
        let chunk = Chunk::new_system("critical", 0);
        assert!(!chunk.can_compact());
    }

    #[test]
    fn test_can_compact_high_priority() {
        let chunk = Chunk::new_message("user", "test", None, 0);
        // High priority can compact only when cold
        assert!(!chunk.can_compact()); // Still hot

        let mut chunk = chunk;
        chunk.storage_tier = StorageTier::Cold;
        assert!(chunk.can_compact());
    }

    #[test]
    fn test_can_compact_normal_priority() {
        let input = serde_json::json!({});
        let chunk = Chunk::new_tool_call("test", &input, "out", false, None, 0);
        assert!(chunk.can_compact()); // Normal priority can always compact
    }

    #[test]
    fn test_chunk_type_default_priority() {
        assert_eq!(ChunkType::Message.default_priority(), ChunkPriority::High);
        assert_eq!(
            ChunkType::ToolCall.default_priority(),
            ChunkPriority::Normal
        );
        assert_eq!(ChunkType::Summary.default_priority(), ChunkPriority::High);
        assert_eq!(
            ChunkType::System.default_priority(),
            ChunkPriority::Critical
        );
        assert_eq!(
            ChunkType::FileContent.default_priority(),
            ChunkPriority::Low
        );
        assert_eq!(
            ChunkType::Metadata.default_priority(),
            ChunkPriority::Normal
        );
        assert_eq!(
            ChunkType::FileTree.default_priority(),
            ChunkPriority::Critical
        );
    }

    #[test]
    fn test_storage_tier_as_str() {
        assert_eq!(StorageTier::Hot.as_str(), "wal");
        assert_eq!(StorageTier::Warm.as_str(), "chunks");
        assert_eq!(StorageTier::Cold.as_str(), "cold");
    }

    #[test]
    fn test_chunk_content_text_message() {
        let content = ChunkContent::Message {
            role: "user".to_string(),
            content: "Hello world".to_string(),
        };
        assert_eq!(content.text(), "Hello world");
    }

    #[test]
    fn test_chunk_content_text_tool_call() {
        let content = ChunkContent::ToolCall {
            tool_name: "file_read".to_string(),
            input: serde_json::json!({}),
            output: "file contents".to_string(),
            is_error: false,
        };
        let text = content.text();
        assert!(text.contains("file_read"));
        assert!(text.contains("file contents"));
    }

    #[test]
    fn test_chunk_content_text_summary() {
        let content = ChunkContent::Summary {
            text: "Summary text".to_string(),
            summarized_chunks: vec![],
        };
        assert_eq!(content.text(), "Summary text");
    }

    #[test]
    fn test_chunk_content_text_system() {
        let content = ChunkContent::System {
            content: "System content".to_string(),
        };
        assert_eq!(content.text(), "System content");
    }

    #[test]
    fn test_chunk_content_text_file() {
        let content = ChunkContent::FileContent {
            path: "/path/to/file.rs".to_string(),
            content: "fn main() {}".to_string(),
            language: Some("rust".to_string()),
        };
        let text = content.text();
        assert!(text.contains("/path/to/file.rs"));
        assert!(text.contains("fn main()"));
    }

    #[test]
    fn test_chunk_content_text_metadata() {
        let content = ChunkContent::Metadata {
            key: "version".to_string(),
            value: serde_json::json!("1.0.0"),
        };
        let text = content.text();
        assert!(text.contains("version"));
        assert!(text.contains("1.0.0"));
    }

    #[test]
    fn test_token_estimation_tool_call() {
        let content = ChunkContent::ToolCall {
            tool_name: "test".to_string(),
            input: serde_json::json!({"key": "value"}),
            output: "output result".to_string(),
            is_error: false,
        };
        let tokens = content.estimate_tokens();
        assert!(tokens > 0);
    }

    #[test]
    fn test_token_estimation_file_content() {
        let content = ChunkContent::FileContent {
            path: "/test.rs".to_string(),
            content: "fn main() { println!(\"Hello\"); }".to_string(),
            language: Some("rust".to_string()),
        };
        let tokens = content.estimate_tokens();
        assert!(tokens > 0);
    }

    #[test]
    fn test_chunk_serialization() {
        let chunk = Chunk::new_message("user", "test message", None, 0);
        let json = serde_json::to_string(&chunk).unwrap();
        let deserialized: Chunk = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, chunk.id);
        assert_eq!(deserialized.chunk_type, chunk.chunk_type);
        assert_eq!(deserialized.sequence, chunk.sequence);
    }

    #[test]
    fn test_chunk_priority_ordering() {
        // Derive Ord uses declaration order: Critical (0) < High (1) < Normal (2) < Low (3)
        // This means Critical is "smallest" and Low is "largest"
        assert!(ChunkPriority::Critical < ChunkPriority::High);
        assert!(ChunkPriority::High < ChunkPriority::Normal);
        assert!(ChunkPriority::Normal < ChunkPriority::Low);
    }

    #[test]
    fn test_chunk_referenced_files() {
        let chunk = Chunk::new(
            ChunkType::FileContent,
            ChunkContent::FileContent {
                path: "/test/file.rs".to_string(),
                content: "fn main() {}".to_string(),
                language: Some("rust".to_string()),
            },
            None,
            0,
        );

        assert_eq!(chunk.referenced_files.len(), 1);
        assert!(chunk.references_file(std::path::Path::new("/test/file.rs")));
    }

    #[test]
    fn test_chunk_add_related() {
        let mut chunk = Chunk::new_message("user", "test", None, 0);
        let related_id = Uuid::new_v4();

        chunk.add_related(related_id);
        assert_eq!(chunk.related_chunks.len(), 1);

        // Adding same ID should not duplicate
        chunk.add_related(related_id);
        assert_eq!(chunk.related_chunks.len(), 1);
    }

    #[test]
    fn test_chunk_add_file_reference() {
        let mut chunk = Chunk::new_message("user", "test", None, 0);
        let path = PathBuf::from("src/main.rs");

        chunk.add_file_reference(path.clone());
        assert_eq!(chunk.referenced_files.len(), 1);

        // Adding same path should not duplicate
        chunk.add_file_reference(path);
        assert_eq!(chunk.referenced_files.len(), 1);
    }

    #[test]
    fn test_chunk_retention_score() {
        let mut chunk = Chunk::new_message("user", "test", None, 0);

        assert_eq!(chunk.retention_score, 0.0);
        chunk.set_retention_score(0.8);
        assert_eq!(chunk.retention_score, 0.8);
    }

    #[test]
    fn test_chunk_effective_priority() {
        let mut chunk = Chunk::new_message("user", "test", None, 0);
        // High priority = 0.75 base

        // With retention_score = 0.0
        // effective = 0.75 * 0.7 + 0.0 * 0.3 = 0.525
        let effective_low = chunk.effective_priority();

        chunk.set_retention_score(1.0);
        // effective = 0.75 * 0.7 + 1.0 * 0.3 = 0.825
        let effective_high = chunk.effective_priority();

        assert!(effective_high > effective_low);
        assert!((effective_low - 0.525).abs() < 0.001);
        assert!((effective_high - 0.825).abs() < 0.001);
    }

    #[test]
    fn test_extract_file_paths_from_file_content() {
        let content = ChunkContent::FileContent {
            path: "/test/file.rs".to_string(),
            content: "fn main() {}".to_string(),
            language: Some("rust".to_string()),
        };

        let paths = content.extract_file_paths();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], PathBuf::from("/test/file.rs"));
    }

    #[test]
    fn test_extract_file_paths_from_tool_call() {
        let content = ChunkContent::ToolCall {
            tool_name: "file_read".to_string(),
            input: serde_json::json!({"path": "src/main.rs"}),
            output: "file contents".to_string(),
            is_error: false,
        };

        let paths = content.extract_file_paths();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], PathBuf::from("src/main.rs"));
    }

    #[test]
    fn test_extract_file_paths_from_glob_tool() {
        let content = ChunkContent::ToolCall {
            tool_name: "glob".to_string(),
            input: serde_json::json!({"pattern": "src/**/*.rs"}),
            output: "matches".to_string(),
            is_error: false,
        };

        let paths = content.extract_file_paths();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], PathBuf::from("src/**/*.rs"));
    }

    #[test]
    fn test_extract_file_paths_from_message() {
        let content = ChunkContent::Message {
            role: "user".to_string(),
            content: "Hello world".to_string(),
        };

        let paths = content.extract_file_paths();
        assert!(paths.is_empty());
    }

    #[test]
    fn test_chunk_serialization_with_new_fields() {
        let mut chunk = Chunk::new_message("user", "test", None, 0);
        chunk.add_file_reference(PathBuf::from("src/main.rs"));
        chunk.add_related(Uuid::new_v4());
        chunk.set_retention_score(0.5);

        let json = serde_json::to_string(&chunk).unwrap();
        let deserialized: Chunk = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.referenced_files.len(), 1);
        assert_eq!(deserialized.related_chunks.len(), 1);
        assert_eq!(deserialized.retention_score, 0.5);
    }

    #[test]
    fn test_new_file_tree_chunk() {
        let chunk = Chunk::new_file_tree(
            "my-project",
            "├── src/\n│   └── main.rs\n└── Cargo.toml",
            2,
            1,
            false,
            0,
        );

        assert_eq!(chunk.chunk_type, ChunkType::FileTree);
        assert_eq!(chunk.priority, ChunkPriority::Critical);
        assert!(chunk.parent_id.is_none());

        if let ChunkContent::FileTree {
            root_name,
            tree,
            file_count,
            dir_count,
            truncated,
        } = &chunk.content
        {
            assert_eq!(root_name, "my-project");
            assert!(tree.contains("src/"));
            assert_eq!(*file_count, 2);
            assert_eq!(*dir_count, 1);
            assert!(!truncated);
        } else {
            panic!("Expected FileTree content");
        }
    }

    #[test]
    fn test_file_tree_chunk_cannot_compact() {
        let chunk = Chunk::new_file_tree("project", "tree", 1, 1, false, 0);

        // FileTree chunks have Critical priority and should never compact
        assert!(!chunk.can_compact());

        // Even when cold, should not compact
        let mut chunk = chunk;
        chunk.storage_tier = StorageTier::Cold;
        assert!(!chunk.can_compact());
    }

    #[test]
    fn test_file_tree_content_text() {
        let content = ChunkContent::FileTree {
            root_name: "my-project".to_string(),
            tree: "├── src/\n└── Cargo.toml".to_string(),
            file_count: 2,
            dir_count: 1,
            truncated: false,
        };

        let text = content.text();
        assert!(text.contains("Project structure (my-project)"));
        assert!(text.contains("src/"));
        assert!(text.contains("Cargo.toml"));
        assert!(text.contains("2 files"));
        assert!(text.contains("1 directories"));
    }

    #[test]
    fn test_file_tree_content_text_truncated() {
        let content = ChunkContent::FileTree {
            root_name: "big-project".to_string(),
            tree: "├── file1\n... (truncated)\n".to_string(),
            file_count: 500,
            dir_count: 50,
            truncated: true,
        };

        let text = content.text();
        assert!(text.contains("Project structure (big-project)"));
        // Truncated trees don't show the file/dir count summary
        assert!(!text.contains("500 files"));
    }

    #[test]
    fn test_file_tree_token_estimation() {
        let content = ChunkContent::FileTree {
            root_name: "project".to_string(),
            tree: "├── a\n└── b".to_string(),
            file_count: 2,
            dir_count: 0,
            truncated: false,
        };

        let tokens = content.estimate_tokens();
        // Should account for tree + root_name + overhead
        assert!(tokens > 0);
    }
}
