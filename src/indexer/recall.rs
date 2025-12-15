// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Recall event processing for memory-based context prioritization.
//!
//! This module handles events that trigger "recall" - when a file or chunk
//! is accessed, it should be boosted in the retention scoring system.
//!
//! # Event Types
//!
//! - **Explicit**: Direct tool calls (file_read, file_edit, grep results)
//! - **Implicit**: File paths mentioned in LLM responses
//! - **Filesystem**: Changes detected by the background daemon
//! - **Associative**: When a file is recalled, its dependencies get a weaker boost
//!
//! # For Custom Tool Authors
//!
//! If you're implementing a custom tool that accesses files, you can integrate
//! with the memory system by emitting recall events. This ensures files your
//! tool touches are boosted in the context priority system.
//!
//! ## Using ToolContext (Recommended)
//!
//! The [`ToolContext`](crate::tools::ToolContext) passed to your tool's execute method
//! provides convenient emit methods:
//!
//! ```ignore
//! use crate::tools::{Tool, ToolContext, ToolResult};
//!
//! async fn execute(&self, tool_use_id: String, input: Value, context: &ToolContext) -> Result<ToolResult> {
//!     let path = PathBuf::from(input["path"].as_str().unwrap());
//!
//!     // Do your work...
//!     let content = std::fs::read_to_string(&path)?;
//!
//!     // Emit the appropriate recall event
//!     context.emit_file_read(&path);       // For reads
//!     // context.emit_file_edit(&path);    // For edits
//!     // context.emit_file_write(&path);   // For writes
//!     // context.emit_search_match(vec![path1, path2]); // For search results
//!
//!     Ok(ToolResult::success(tool_use_id, content))
//! }
//! ```
//!
//! ## Using RecallSender Directly
//!
//! For more control, you can use [`RecallSender`] directly:
//!
//! ```ignore
//! use crate::indexer::{recall_channel, RecallEvent, FileChangeType};
//!
//! // Create a channel
//! let (sender, receiver) = recall_channel();
//!
//! // Send events
//! sender.file_read("src/main.rs");
//! sender.file_edit("src/lib.rs");
//! sender.search_match(vec![PathBuf::from("src/foo.rs")]);
//! sender.filesystem_change("src/new.rs", FileChangeType::Created);
//!
//! // Or construct events manually
//! sender.send(RecallEvent::file_read_with_chunks("src/main.rs", vec![chunk_id]));
//! ```
//!
//! ## Processing Daemon Events
//!
//! To integrate filesystem changes from the daemon into the recall system:
//!
//! ```ignore
//! use crate::indexer::{recall_channel, DaemonEvent};
//!
//! let (sender, receiver) = recall_channel();
//!
//! // When you receive a daemon event:
//! let daemon_event = DaemonEvent::FileModified(PathBuf::from("src/main.rs"));
//! sender.process_daemon_event(&daemon_event);
//! ```
//!
//! ## Boost Multipliers
//!
//! Different event types have different boost values:
//!
//! | Event Type | Multiplier | Rationale |
//! |------------|------------|-----------|
//! | FileEdit | 1.5x | Active modification = highest relevance |
//! | FileWrite | 1.2x | New file creation = high relevance |
//! | FileRead | 1.0x | Direct read = full relevance |
//! | ChunkAccess | 1.0x | Direct chunk access = full relevance |
//! | FileSystemChange (Modified) | 0.8x | External change = high relevance |
//! | FileSystemChange (Created) | 0.6x | New external file = moderate |
//! | SearchMatch | 0.5x | Search result = partial relevance |
//! | LlmMention | 0.3x | Mentioned in text = weak relevance |
//! | FileSystemChange (Deleted) | 0.2x | Deleted = low but noted |

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use uuid::Uuid;

/// Events that trigger recall/memory boost.
#[derive(Debug, Clone)]
pub enum RecallEvent {
    /// A file was read via tool call.
    FileRead {
        /// Path to the file (relative to project root).
        path: PathBuf,
        /// Optional chunk IDs if the file was chunked.
        chunk_ids: Vec<Uuid>,
    },

    /// A file was edited via tool call.
    FileEdit {
        /// Path to the file.
        path: PathBuf,
        /// Optional chunk IDs affected.
        chunk_ids: Vec<Uuid>,
    },

    /// A file was written via tool call.
    FileWrite {
        /// Path to the file.
        path: PathBuf,
    },

    /// Files were matched by grep/glob.
    SearchMatch {
        /// Paths that matched the search.
        paths: Vec<PathBuf>,
    },

    /// File paths were mentioned in LLM response (implicit recall).
    LlmMention {
        /// Paths mentioned in the response.
        paths: Vec<PathBuf>,
    },

    /// Multiple chunks were accessed together.
    ChunkAccess {
        /// Chunk IDs that were accessed.
        chunk_ids: Vec<Uuid>,
    },

    /// A file was modified by an external process (from daemon).
    FileSystemChange {
        /// Path to the file.
        path: PathBuf,
        /// Type of change.
        change_type: FileChangeType,
    },
}

/// Type of filesystem change detected by the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeType {
    /// File was created.
    Created,
    /// File was modified.
    Modified,
    /// File was deleted.
    Deleted,
}

impl RecallEvent {
    /// Create a file read event.
    pub fn file_read(path: impl Into<PathBuf>) -> Self {
        Self::FileRead {
            path: path.into(),
            chunk_ids: Vec::new(),
        }
    }

    /// Create a file read event with chunk IDs.
    pub fn file_read_with_chunks(path: impl Into<PathBuf>, chunk_ids: Vec<Uuid>) -> Self {
        Self::FileRead {
            path: path.into(),
            chunk_ids,
        }
    }

    /// Create a file edit event.
    pub fn file_edit(path: impl Into<PathBuf>) -> Self {
        Self::FileEdit {
            path: path.into(),
            chunk_ids: Vec::new(),
        }
    }

    /// Create a file edit event with chunk IDs.
    pub fn file_edit_with_chunks(path: impl Into<PathBuf>, chunk_ids: Vec<Uuid>) -> Self {
        Self::FileEdit {
            path: path.into(),
            chunk_ids,
        }
    }

    /// Create a file write event.
    pub fn file_write(path: impl Into<PathBuf>) -> Self {
        Self::FileWrite { path: path.into() }
    }

    /// Create a search match event.
    pub fn search_match(paths: Vec<PathBuf>) -> Self {
        Self::SearchMatch { paths }
    }

    /// Create an LLM mention event.
    pub fn llm_mention(paths: Vec<PathBuf>) -> Self {
        Self::LlmMention { paths }
    }

    /// Create a chunk access event.
    pub fn chunk_access(chunk_ids: Vec<Uuid>) -> Self {
        Self::ChunkAccess { chunk_ids }
    }

    /// Create a filesystem change event.
    pub fn filesystem_change(path: impl Into<PathBuf>, change_type: FileChangeType) -> Self {
        Self::FileSystemChange {
            path: path.into(),
            change_type,
        }
    }

    /// Get all file paths affected by this event.
    pub fn affected_paths(&self) -> Vec<&PathBuf> {
        match self {
            RecallEvent::FileRead { path, .. } => vec![path],
            RecallEvent::FileEdit { path, .. } => vec![path],
            RecallEvent::FileWrite { path } => vec![path],
            RecallEvent::SearchMatch { paths } => paths.iter().collect(),
            RecallEvent::LlmMention { paths } => paths.iter().collect(),
            RecallEvent::ChunkAccess { .. } => vec![],
            RecallEvent::FileSystemChange { path, .. } => vec![path],
        }
    }

    /// Get all chunk IDs affected by this event.
    pub fn affected_chunks(&self) -> Vec<&Uuid> {
        match self {
            RecallEvent::FileRead { chunk_ids, .. } => chunk_ids.iter().collect(),
            RecallEvent::FileEdit { chunk_ids, .. } => chunk_ids.iter().collect(),
            RecallEvent::ChunkAccess { chunk_ids } => chunk_ids.iter().collect(),
            _ => vec![],
        }
    }

    /// Get the boost multiplier for this event type.
    ///
    /// Different event types have different impact on retention scores.
    pub fn boost_multiplier(&self) -> f64 {
        match self {
            RecallEvent::FileRead { .. } => 1.0,  // Direct read = full boost
            RecallEvent::FileEdit { .. } => 1.5,  // Edit is even more important
            RecallEvent::FileWrite { .. } => 1.2, // Write is important
            RecallEvent::SearchMatch { .. } => 0.5, // Search result = partial boost
            RecallEvent::LlmMention { .. } => 0.3, // Mention = weaker boost
            RecallEvent::ChunkAccess { .. } => 1.0, // Direct chunk access = full boost
            RecallEvent::FileSystemChange { change_type, .. } => {
                // Filesystem changes get moderate boost
                match change_type {
                    FileChangeType::Modified => 0.8, // Modification = high relevance
                    FileChangeType::Created => 0.6,  // New file = moderate relevance
                    FileChangeType::Deleted => 0.2,  // Deleted = low (but note it)
                }
            }
        }
    }
}

/// Channel for sending recall events.
#[derive(Clone)]
pub struct RecallSender {
    tx: Sender<RecallEvent>,
}

impl RecallSender {
    /// Send a recall event.
    pub fn send(&self, event: RecallEvent) -> bool {
        self.tx.send(event).is_ok()
    }

    /// Send a file read event.
    pub fn file_read(&self, path: impl Into<PathBuf>) -> bool {
        self.send(RecallEvent::file_read(path))
    }

    /// Send a file edit event.
    pub fn file_edit(&self, path: impl Into<PathBuf>) -> bool {
        self.send(RecallEvent::file_edit(path))
    }

    /// Send a file write event.
    pub fn file_write(&self, path: impl Into<PathBuf>) -> bool {
        self.send(RecallEvent::file_write(path))
    }

    /// Send a search match event.
    pub fn search_match(&self, paths: Vec<PathBuf>) -> bool {
        self.send(RecallEvent::search_match(paths))
    }

    /// Send an LLM mention event.
    pub fn llm_mention(&self, paths: Vec<PathBuf>) -> bool {
        self.send(RecallEvent::llm_mention(paths))
    }

    /// Send a filesystem change event.
    pub fn filesystem_change(&self, path: impl Into<PathBuf>, change_type: FileChangeType) -> bool {
        self.send(RecallEvent::filesystem_change(path, change_type))
    }

    /// Process a daemon event and send the appropriate recall event.
    ///
    /// This bridges the gap between the filesystem watcher daemon and the
    /// recall system, allowing filesystem changes to trigger memory boosts.
    pub fn process_daemon_event(&self, event: &super::config::DaemonEvent) -> bool {
        use super::config::DaemonEvent;

        match event {
            DaemonEvent::FileCreated(path) => {
                self.filesystem_change(path.clone(), FileChangeType::Created)
            }
            DaemonEvent::FileModified(path) => {
                self.filesystem_change(path.clone(), FileChangeType::Modified)
            }
            DaemonEvent::FileDeleted(path) => {
                self.filesystem_change(path.clone(), FileChangeType::Deleted)
            }
            DaemonEvent::FileRenamed { from, to } => {
                // Treat rename as delete + create
                self.filesystem_change(from.clone(), FileChangeType::Deleted);
                self.filesystem_change(to.clone(), FileChangeType::Created)
            }
            // Other daemon events don't trigger recall
            DaemonEvent::IndexPersisted | DaemonEvent::Error(_) | DaemonEvent::Stopped => true,
        }
    }
}

/// Receiver for recall events.
pub struct RecallReceiver {
    rx: Receiver<RecallEvent>,
}

impl RecallReceiver {
    /// Try to receive an event without blocking.
    pub fn try_recv(&self) -> Option<RecallEvent> {
        match self.rx.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => None,
        }
    }

    /// Receive an event, blocking until one is available.
    pub fn recv(&self) -> Option<RecallEvent> {
        self.rx.recv().ok()
    }

    /// Drain all pending events.
    pub fn drain(&self) -> Vec<RecallEvent> {
        let mut events = Vec::new();
        while let Some(event) = self.try_recv() {
            events.push(event);
        }
        events
    }
}

/// Create a new recall event channel.
pub fn recall_channel() -> (RecallSender, RecallReceiver) {
    let (tx, rx) = mpsc::channel();
    (RecallSender { tx }, RecallReceiver { rx })
}

/// Processor for recall events that updates the indexer.
pub struct RecallProcessor {
    /// Receiver for events.
    receiver: RecallReceiver,
    /// Associative boost factor for dependencies.
    associative_boost: f64,
}

impl RecallProcessor {
    /// Create a new recall processor.
    pub fn new(receiver: RecallReceiver) -> Self {
        Self {
            receiver,
            associative_boost: 0.3, // Dependencies get 30% of the boost
        }
    }

    /// Set the associative boost factor.
    pub fn with_associative_boost(mut self, boost: f64) -> Self {
        self.associative_boost = boost;
        self
    }

    /// Process all pending events and return affected files.
    pub fn process_pending(&self) -> ProcessedRecalls {
        let events = self.receiver.drain();
        self.process_events(events)
    }

    /// Process a batch of events.
    pub fn process_events(&self, events: Vec<RecallEvent>) -> ProcessedRecalls {
        let mut result = ProcessedRecalls::default();

        for event in events {
            let multiplier = event.boost_multiplier();

            // Collect affected paths with boost
            for path in event.affected_paths() {
                let current = result.file_boosts.get(path).copied().unwrap_or(0.0);
                result
                    .file_boosts
                    .insert(path.clone(), current + multiplier);
            }

            // Collect affected chunks with boost
            for chunk_id in event.affected_chunks() {
                let current = result.chunk_boosts.get(chunk_id).copied().unwrap_or(0.0);
                result.chunk_boosts.insert(*chunk_id, current + multiplier);
            }

            result.event_count += 1;
        }

        result
    }

    /// Get the associative boost factor.
    pub fn associative_boost(&self) -> f64 {
        self.associative_boost
    }
}

/// Result of processing recall events.
#[derive(Debug, Default)]
pub struct ProcessedRecalls {
    /// Files to boost and their cumulative boost amounts.
    pub file_boosts: std::collections::HashMap<PathBuf, f64>,
    /// Chunks to boost and their cumulative boost amounts.
    pub chunk_boosts: std::collections::HashMap<Uuid, f64>,
    /// Number of events processed.
    pub event_count: usize,
}

impl ProcessedRecalls {
    /// Check if any boosts need to be applied.
    pub fn has_boosts(&self) -> bool {
        !self.file_boosts.is_empty() || !self.chunk_boosts.is_empty()
    }

    /// Get unique file paths that were affected.
    pub fn affected_files(&self) -> HashSet<&PathBuf> {
        self.file_boosts.keys().collect()
    }

    /// Get unique chunk IDs that were affected.
    pub fn affected_chunks(&self) -> HashSet<&Uuid> {
        self.chunk_boosts.keys().collect()
    }
}

/// Extract file paths mentioned in text (for implicit recall).
///
/// Looks for common file path patterns in the text.
pub fn extract_paths_from_text(text: &str, project_root: Option<&std::path::Path>) -> Vec<PathBuf> {
    use regex::Regex;
    use std::sync::OnceLock;

    static PATH_REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = PATH_REGEX.get_or_init(|| {
        // Match file paths with common code extensions
        // Lookahead/behind chars: start of line, whitespace, quotes, backticks, parens
        Regex::new(
            r#"(?:^|[`"'\s(])([a-zA-Z0-9_./\\-]+\.(?:rs|ts|tsx|js|jsx|py|go|java|c|cpp|h|hpp|rb|swift|kt|php|toml|yaml|yml|json|md))(?:[`"'\s):,.]|$)"#
        ).unwrap()
    });

    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    for cap in regex.captures_iter(text) {
        if let Some(path_match) = cap.get(1) {
            let path_str = path_match.as_str();

            // Skip if already seen
            if seen.contains(path_str) {
                continue;
            }
            seen.insert(path_str.to_string());

            let path = PathBuf::from(path_str);

            // If project root is given, check if file exists
            if let Some(root) = project_root {
                let full_path = root.join(&path);
                if full_path.exists() {
                    paths.push(path);
                }
            } else {
                // No root given, include all matches
                paths.push(path);
            }
        }
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recall_event_file_read() {
        let event = RecallEvent::file_read("src/main.rs");
        assert_eq!(event.affected_paths().len(), 1);
        assert_eq!(event.boost_multiplier(), 1.0);
    }

    #[test]
    fn test_recall_event_file_edit() {
        let event = RecallEvent::file_edit("src/lib.rs");
        assert_eq!(event.boost_multiplier(), 1.5);
    }

    #[test]
    fn test_recall_event_search_match() {
        let paths = vec![PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")];
        let event = RecallEvent::search_match(paths);
        assert_eq!(event.affected_paths().len(), 2);
        assert_eq!(event.boost_multiplier(), 0.5);
    }

    #[test]
    fn test_recall_event_llm_mention() {
        let paths = vec![PathBuf::from("test.rs")];
        let event = RecallEvent::llm_mention(paths);
        assert_eq!(event.boost_multiplier(), 0.3);
    }

    #[test]
    fn test_recall_event_chunk_access() {
        let chunk_ids = vec![Uuid::new_v4(), Uuid::new_v4()];
        let event = RecallEvent::chunk_access(chunk_ids.clone());
        assert_eq!(event.affected_chunks().len(), 2);
        assert!(event.affected_paths().is_empty());
    }

    #[test]
    fn test_recall_channel() {
        let (sender, receiver) = recall_channel();

        sender.file_read("test.rs");
        sender.file_edit("lib.rs");

        let events = receiver.drain();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_recall_sender_methods() {
        let (sender, receiver) = recall_channel();

        assert!(sender.file_read("a.rs"));
        assert!(sender.file_edit("b.rs"));
        assert!(sender.file_write("c.rs"));
        assert!(sender.search_match(vec![PathBuf::from("d.rs")]));
        assert!(sender.llm_mention(vec![PathBuf::from("e.rs")]));

        let events = receiver.drain();
        assert_eq!(events.len(), 5);
    }

    #[test]
    fn test_recall_processor() {
        let (sender, receiver) = recall_channel();
        let processor = RecallProcessor::new(receiver);

        sender.file_read("test.rs");
        sender.file_read("test.rs"); // Duplicate
        sender.file_edit("lib.rs");

        let result = processor.process_pending();

        assert_eq!(result.event_count, 3);
        assert_eq!(result.file_boosts.len(), 2);

        // test.rs should have boost of 2.0 (1.0 + 1.0)
        assert_eq!(
            *result.file_boosts.get(&PathBuf::from("test.rs")).unwrap(),
            2.0
        );

        // lib.rs should have boost of 1.5 (edit multiplier)
        assert_eq!(
            *result.file_boosts.get(&PathBuf::from("lib.rs")).unwrap(),
            1.5
        );
    }

    #[test]
    fn test_processed_recalls() {
        let mut result = ProcessedRecalls::default();
        assert!(!result.has_boosts());

        result.file_boosts.insert(PathBuf::from("test.rs"), 1.0);
        assert!(result.has_boosts());
        assert_eq!(result.affected_files().len(), 1);
    }

    #[test]
    fn test_extract_paths_from_text() {
        let text = r#"
            I modified `src/main.rs` and also updated src/lib.rs.
            The test is in "tests/test.rs".
        "#;

        let paths = extract_paths_from_text(text, None);

        assert!(paths.contains(&PathBuf::from("src/main.rs")));
        assert!(paths.contains(&PathBuf::from("src/lib.rs")));
        assert!(paths.contains(&PathBuf::from("tests/test.rs")));
    }

    #[test]
    fn test_extract_paths_deduplication() {
        let text = "src/main.rs and src/main.rs again";
        let paths = extract_paths_from_text(text, None);

        // Should only appear once
        let main_count = paths
            .iter()
            .filter(|p| *p == &PathBuf::from("src/main.rs"))
            .count();
        assert_eq!(main_count, 1);
    }

    #[test]
    fn test_extract_paths_with_project_root() {
        let temp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/main.rs"), "").unwrap();

        let text = "Check src/main.rs and src/nonexistent.rs";
        let paths = extract_paths_from_text(text, Some(temp.path()));

        // Only existing file should be included
        assert!(paths.contains(&PathBuf::from("src/main.rs")));
        assert!(!paths.contains(&PathBuf::from("src/nonexistent.rs")));
    }

    #[test]
    fn test_recall_event_with_chunks() {
        let chunk_id = Uuid::new_v4();
        let event = RecallEvent::file_read_with_chunks("test.rs", vec![chunk_id]);

        assert_eq!(event.affected_paths().len(), 1);
        assert_eq!(event.affected_chunks().len(), 1);
    }

    #[test]
    fn test_recall_processor_associative_boost() {
        let (_, receiver) = recall_channel();
        let processor = RecallProcessor::new(receiver).with_associative_boost(0.5);

        assert_eq!(processor.associative_boost(), 0.5);
    }

    #[test]
    fn test_recall_receiver_recv_empty() {
        let (_, receiver) = recall_channel();
        assert!(receiver.try_recv().is_none());
    }

    #[test]
    fn test_filesystem_change_event() {
        let event = RecallEvent::filesystem_change("src/main.rs", FileChangeType::Modified);
        assert_eq!(event.affected_paths().len(), 1);
        assert_eq!(event.boost_multiplier(), 0.8);
    }

    #[test]
    fn test_filesystem_change_types() {
        // Created gets 0.6
        let created = RecallEvent::filesystem_change("new.rs", FileChangeType::Created);
        assert_eq!(created.boost_multiplier(), 0.6);

        // Modified gets 0.8
        let modified = RecallEvent::filesystem_change("existing.rs", FileChangeType::Modified);
        assert_eq!(modified.boost_multiplier(), 0.8);

        // Deleted gets 0.2
        let deleted = RecallEvent::filesystem_change("old.rs", FileChangeType::Deleted);
        assert_eq!(deleted.boost_multiplier(), 0.2);
    }

    #[test]
    fn test_recall_sender_filesystem_change() {
        let (sender, receiver) = recall_channel();

        assert!(sender.filesystem_change("test.rs", FileChangeType::Modified));

        let events = receiver.drain();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_process_daemon_event_file_created() {
        use super::super::config::DaemonEvent;

        let (sender, receiver) = recall_channel();

        let daemon_event = DaemonEvent::FileCreated(PathBuf::from("src/new.rs"));
        assert!(sender.process_daemon_event(&daemon_event));

        let events = receiver.drain();
        assert_eq!(events.len(), 1);

        if let RecallEvent::FileSystemChange { path, change_type } = &events[0] {
            assert_eq!(path, &PathBuf::from("src/new.rs"));
            assert_eq!(*change_type, FileChangeType::Created);
        } else {
            panic!("Expected FileSystemChange event");
        }
    }

    #[test]
    fn test_process_daemon_event_file_modified() {
        use super::super::config::DaemonEvent;

        let (sender, receiver) = recall_channel();

        let daemon_event = DaemonEvent::FileModified(PathBuf::from("src/main.rs"));
        assert!(sender.process_daemon_event(&daemon_event));

        let events = receiver.drain();
        assert_eq!(events.len(), 1);

        if let RecallEvent::FileSystemChange { change_type, .. } = &events[0] {
            assert_eq!(*change_type, FileChangeType::Modified);
        } else {
            panic!("Expected FileSystemChange event");
        }
    }

    #[test]
    fn test_process_daemon_event_file_renamed() {
        use super::super::config::DaemonEvent;

        let (sender, receiver) = recall_channel();

        let daemon_event = DaemonEvent::FileRenamed {
            from: PathBuf::from("old.rs"),
            to: PathBuf::from("new.rs"),
        };
        assert!(sender.process_daemon_event(&daemon_event));

        // Rename produces two events: delete + create
        let events = receiver.drain();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_process_daemon_event_non_file_events() {
        use super::super::config::DaemonEvent;

        let (sender, receiver) = recall_channel();

        // These should not produce recall events
        assert!(sender.process_daemon_event(&DaemonEvent::IndexPersisted));
        assert!(sender.process_daemon_event(&DaemonEvent::Error("test".to_string())));
        assert!(sender.process_daemon_event(&DaemonEvent::Stopped));

        let events = receiver.drain();
        assert_eq!(events.len(), 0);
    }
}
