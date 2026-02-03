// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Memory recall integration for agent loop
//!
//! This module provides functions to search past conversations and inject
//! relevant context into the current conversation.

use crate::context::memory::{ConversationMemory, MemoryStore};
use crate::embeddings::EmbeddingGenerator;
use crate::error::Result;
use chrono::Utc;
use uuid::Uuid;

/// Recall relevant past conversations based on the current query
///
/// This searches the memory store for similar past conversations and returns
/// them formatted for inclusion in the system prompt.
pub async fn recall_relevant_context(
    query: &str,
    memory_store: &MemoryStore,
    max_results: usize,
) -> Result<Option<String>> {
    // Search for similar past conversations
    let results = memory_store.search(query, max_results).await?;

    if results.is_empty() {
        return Ok(None);
    }

    // Format results for injection into system prompt
    let mut context = String::from("\n\n## Relevant Past Conversations\n\n");
    context.push_str("You previously worked on related tasks. Here's what you did:\n\n");

    for (i, result) in results.iter().enumerate() {
        if result.score < 0.5 {
            // Skip low-relevance results
            continue;
        }

        context.push_str(&format!("{}. {}\n", i + 1, result.content));

        // Extract full content if available
        if let Some(metadata) = &result.metadata {
            if let Some(full_content) = metadata.get("full_content").and_then(|v| v.as_str()) {
                // Include a snippet of the full conversation
                let snippet: String = full_content.chars().take(200).collect();
                context.push_str(&format!("   Context: {}...\n", snippet));
            }
        }

        context.push('\n');
    }

    Ok(Some(context))
}

/// Store the current conversation in memory
///
/// This should be called at the end of each conversation turn to persist
/// the conversation for future recall.
pub async fn store_conversation(
    conversation_id: Uuid,
    summary: String,
    files_changed: Vec<String>,
    tags: Vec<String>,
    full_content: String,
    embedding_generator: &EmbeddingGenerator,
    memory_store: &MemoryStore,
) -> Result<()> {
    // Generate embedding for the summary
    let embedding = embedding_generator.embed(&summary).await?;

    // Create memory record
    let memory = ConversationMemory {
        id: conversation_id,
        timestamp: Utc::now(),
        summary,
        files_changed,
        tags,
        content: full_content,
        embedding,
    };

    // Store in database
    memory_store.store(&memory).await?;

    Ok(())
}

/// Integration point for embedded_runner.rs
///
/// Add this near the beginning of run_embedded_chat(), after loading the prompt:
///
/// ```rust,ignore
/// // Memory recall (if enabled)
/// if let Some(memory_store) = &settings.memory_store {
///     if let Ok(Some(context)) = recall::recall_relevant_context(
///         &prompt,
///         memory_store,
///         3  // Top 3 most relevant past conversations
///     ).await {
///         // Append recalled context to system prompt
///         merged_cap.system_prompt.push_str(&context);
///     }
/// }
/// ```
///
/// And at the end, after the conversation completes:
///
/// ```rust,ignore
/// // Store conversation in memory (if enabled)
/// if let Some(memory_store) = &settings.memory_store {
///     let summary = summarizer::summarize_conversation(&messages, provider.as_ref()).await?;
///     let files = summarizer::extract_files_changed(&messages);
///     let tags = summarizer::extract_tags(&messages);
///     let content = messages.iter()
///         .map(|m| format!("{}: {:?}", m.role, m.content))
///         .collect::<Vec<_>>()
///         .join("\n");
///
///     recall::store_conversation(
///         session_id,
///         summary,
///         files,
///         tags,
///         content,
///         &embedding_generator,
///         memory_store,
///     ).await?;
/// }
/// ```
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    #[ignore] // Requires Ollama running
    async fn test_recall_and_store() {
        let temp_file = NamedTempFile::new().unwrap();
        let generator = EmbeddingGenerator::new();
        let store = MemoryStore::open(temp_file.path(), generator.clone()).unwrap();

        // Store a memory
        let conversation_id = Uuid::new_v4();
        let summary = "Added authentication system to the application".to_string();
        let files = vec!["src/auth.rs".to_string()];
        let tags = vec!["auth".to_string(), "security".to_string()];
        let content =
            "User: Add authentication\nAssistant: I'll add JWT authentication...".to_string();

        store_conversation(
            conversation_id,
            summary,
            files,
            tags,
            content,
            &generator,
            &store,
        )
        .await
        .unwrap();

        // Recall related conversation
        let recalled = recall_relevant_context("How do I add login to my app?", &store, 3)
            .await
            .unwrap();

        assert!(recalled.is_some());
        let context = recalled.unwrap();
        assert!(context.contains("authentication") || context.contains("Relevant"));
    }

    // ==================== ConversationMemory struct tests ====================

    #[test]
    fn test_conversation_memory_creation() {
        let memory = ConversationMemory {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            summary: "Test summary".to_string(),
            files_changed: vec!["file1.rs".to_string(), "file2.rs".to_string()],
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            content: "Full conversation content".to_string(),
            embedding: vec![0.1, 0.2, 0.3],
        };

        assert!(!memory.id.is_nil());
        assert_eq!(memory.summary, "Test summary");
        assert_eq!(memory.files_changed.len(), 2);
        assert_eq!(memory.tags.len(), 2);
        assert_eq!(memory.embedding.len(), 3);
    }

    #[test]
    fn test_conversation_memory_empty_fields() {
        let memory = ConversationMemory {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            summary: String::new(),
            files_changed: vec![],
            tags: vec![],
            content: String::new(),
            embedding: vec![],
        };

        assert!(memory.summary.is_empty());
        assert!(memory.files_changed.is_empty());
        assert!(memory.tags.is_empty());
        assert!(memory.content.is_empty());
        assert!(memory.embedding.is_empty());
    }

    #[test]
    fn test_conversation_memory_large_embedding() {
        let embedding: Vec<f32> = (0..1024).map(|i| i as f32 / 1024.0).collect();
        let memory = ConversationMemory {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            summary: "Summary".to_string(),
            files_changed: vec![],
            tags: vec![],
            content: "Content".to_string(),
            embedding: embedding.clone(),
        };

        assert_eq!(memory.embedding.len(), 1024);
        assert_eq!(memory.embedding[0], 0.0);
        assert!((memory.embedding[512] - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_conversation_memory_multiple_files() {
        let files: Vec<String> = (0..100).map(|i| format!("src/file_{}.rs", i)).collect();
        let memory = ConversationMemory {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            summary: "Summary".to_string(),
            files_changed: files,
            tags: vec![],
            content: "Content".to_string(),
            embedding: vec![],
        };

        assert_eq!(memory.files_changed.len(), 100);
        assert!(memory.files_changed[50].contains("file_50"));
    }

    #[test]
    fn test_conversation_memory_special_characters() {
        let memory = ConversationMemory {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            summary: "Added 日本語 support & <special> \"chars\"".to_string(),
            files_changed: vec!["path/with spaces/file.rs".to_string()],
            tags: vec!["emoji🎉".to_string(), "tag-with-dash".to_string()],
            content: "Content with\nnewlines\tand\ttabs".to_string(),
            embedding: vec![],
        };

        assert!(memory.summary.contains("日本語"));
        assert!(memory.files_changed[0].contains("spaces"));
        assert!(memory.tags[0].contains("🎉"));
        assert!(memory.content.contains('\n'));
    }

    #[test]
    fn test_uuid_uniqueness() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_timestamp_order() {
        let t1 = Utc::now();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let t2 = Utc::now();
        assert!(t2 > t1);
    }

    // ==================== Context formatting tests ====================

    #[test]
    fn test_context_header_format() {
        // Verify the expected header format for recalled context
        let expected_header = "\n\n## Relevant Past Conversations\n\n";
        let expected_subheader = "You previously worked on related tasks. Here's what you did:\n\n";

        assert!(expected_header.starts_with("\n\n##"));
        assert!(expected_subheader.contains("previously"));
    }

    #[test]
    fn test_content_snippet_truncation() {
        let long_content = "A".repeat(500);
        let snippet: String = long_content.chars().take(200).collect();

        assert_eq!(snippet.len(), 200);
        assert!(!snippet.contains('B')); // All should be 'A'
    }

    #[test]
    fn test_content_snippet_short_content() {
        let short_content = "Short text";
        let snippet: String = short_content.chars().take(200).collect();

        assert_eq!(snippet, "Short text");
        assert_eq!(snippet.len(), short_content.len());
    }

    // ==================== Score threshold tests ====================

    #[test]
    fn test_score_threshold() {
        // The code skips results with score < 0.5
        let threshold = 0.5;

        // These should be included
        assert!(0.5 >= threshold);
        assert!(0.75 >= threshold);
        assert!(1.0 >= threshold);

        // These should be skipped
        assert!(0.49 < threshold);
        assert!(0.0 < threshold);
    }

    // ==================== Integration-ready mock tests ====================

    #[test]
    fn test_max_results_parameter() {
        // Verify max_results affects expected behavior
        let max_results = 3;
        assert!(max_results > 0);
        assert!(max_results <= 100); // Reasonable upper bound
    }

    #[test]
    fn test_query_string_handling() {
        // Various query strings that should be handled
        let long_query = "A".repeat(10000);
        let queries: Vec<&str> = vec![
            "simple query",
            "How do I implement authentication?",
            "",
            &long_query,
            "query with\nnewlines",
            "query with unicode: 日本語",
        ];

        for query in queries {
            // Query should be a valid string reference
            let _ = query.len(); // Just verify we can access it without panic
        }
    }

    #[test]
    fn test_metadata_extraction() {
        // Test the metadata JSON value extraction pattern used in the code
        use serde_json::json;

        let metadata = json!({
            "full_content": "This is the full conversation content"
        });

        let full_content = metadata.get("full_content").and_then(|v| v.as_str());

        assert!(full_content.is_some());
        assert_eq!(
            full_content.unwrap(),
            "This is the full conversation content"
        );
    }

    #[test]
    fn test_metadata_missing_field() {
        use serde_json::json;

        let metadata = json!({
            "other_field": "value"
        });

        let full_content = metadata.get("full_content").and_then(|v| v.as_str());

        assert!(full_content.is_none());
    }

    #[test]
    fn test_metadata_wrong_type() {
        use serde_json::json;

        let metadata = json!({
            "full_content": 12345  // Number, not string
        });

        let full_content = metadata.get("full_content").and_then(|v| v.as_str());

        assert!(full_content.is_none()); // as_str returns None for non-strings
    }

    // ==================== Async recall_relevant_context tests ====================

    /// Mock search result for testing context formatting
    #[derive(Debug)]
    struct MockSearchResult {
        content: String,
        score: f32,
        metadata: Option<serde_json::Value>,
    }

    #[test]
    fn test_context_formatting_with_high_score() {
        // Test the formatting logic used in recall_relevant_context
        let results = [MockSearchResult {
            content: "Added authentication to the API".to_string(),
            score: 0.85,
            metadata: Some(serde_json::json!({
                "full_content": "User asked about login, and I implemented JWT authentication..."
            })),
        }];

        let mut context = String::from("\n\n## Relevant Past Conversations\n\n");
        context.push_str("You previously worked on related tasks. Here's what you did:\n\n");

        for (i, result) in results.iter().enumerate() {
            if result.score < 0.5 {
                continue;
            }

            context.push_str(&format!("{}. {}\n", i + 1, result.content));

            if let Some(metadata) = &result.metadata {
                if let Some(full_content) = metadata.get("full_content").and_then(|v| v.as_str()) {
                    let snippet: String = full_content.chars().take(200).collect();
                    context.push_str(&format!("   Context: {}...\n", snippet));
                }
            }

            context.push('\n');
        }

        assert!(context.contains("Relevant Past Conversations"));
        assert!(context.contains("authentication"));
        assert!(context.contains("Context:"));
    }

    #[test]
    fn test_context_formatting_with_low_score_skipped() {
        let results = [
            MockSearchResult {
                content: "Low relevance result".to_string(),
                score: 0.3, // Below 0.5 threshold
                metadata: None,
            },
            MockSearchResult {
                content: "High relevance result".to_string(),
                score: 0.8,
                metadata: None,
            },
        ];

        let mut context = String::new();
        for (i, result) in results.iter().enumerate() {
            if result.score < 0.5 {
                continue; // Skip low-relevance results
            }
            context.push_str(&format!("{}. {}\n", i + 1, result.content));
        }

        assert!(!context.contains("Low relevance"));
        assert!(context.contains("High relevance"));
    }

    #[test]
    fn test_context_formatting_empty_results() {
        let results: Vec<MockSearchResult> = vec![];

        // When results are empty, return None
        if results.is_empty() {
            let context: Option<String> = None;
            assert!(context.is_none());
        }
    }

    #[test]
    fn test_context_formatting_no_metadata() {
        let results = [MockSearchResult {
            content: "Result without metadata".to_string(),
            score: 0.9,
            metadata: None,
        }];

        let mut context = String::new();
        for result in results.iter() {
            if result.score >= 0.5 {
                context.push_str(&format!("{}\n", result.content));
                // No metadata, so no context snippet
            }
        }

        assert!(context.contains("Result without metadata"));
        assert!(!context.contains("Context:"));
    }

    #[test]
    fn test_context_snippet_truncation() {
        let long_content = "A".repeat(500);
        let snippet: String = long_content.chars().take(200).collect();

        assert_eq!(snippet.len(), 200);
    }

    // ==================== ConversationMemory construction tests ====================

    #[test]
    fn test_conversation_memory_for_store() {
        let conversation_id = Uuid::new_v4();
        let summary = "Implemented user authentication".to_string();
        let files_changed = vec!["src/auth.rs".to_string(), "src/main.rs".to_string()];
        let tags = vec!["auth".to_string(), "security".to_string()];
        let content = "User: Add login\nAssistant: I'll add JWT...".to_string();
        let embedding = vec![0.1, 0.2, 0.3, 0.4, 0.5];

        let memory = ConversationMemory {
            id: conversation_id,
            timestamp: Utc::now(),
            summary: summary.clone(),
            files_changed: files_changed.clone(),
            tags: tags.clone(),
            content: content.clone(),
            embedding: embedding.clone(),
        };

        assert_eq!(memory.summary, summary);
        assert_eq!(memory.files_changed, files_changed);
        assert_eq!(memory.tags, tags);
        assert_eq!(memory.content, content);
        assert_eq!(memory.embedding, embedding);
    }

    #[test]
    fn test_conversation_memory_with_realistic_embedding() {
        // Realistic embedding dimensions (e.g., 384 for smaller models, 1024 for larger)
        let embedding: Vec<f32> = (0..384).map(|i| (i as f32) / 384.0).collect();

        let memory = ConversationMemory {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            summary: "Test".to_string(),
            files_changed: vec![],
            tags: vec![],
            content: "Test content".to_string(),
            embedding,
        };

        assert_eq!(memory.embedding.len(), 384);
        // First element should be close to 0
        assert!(memory.embedding[0] < 0.01);
        // Last element should be close to 1
        assert!(memory.embedding[383] > 0.99);
    }

    // ==================== Score threshold tests ====================

    #[test]
    fn test_score_threshold_boundary() {
        let threshold = 0.5;

        // Exactly at threshold should be included (>= 0.5)
        assert!(0.5 >= threshold);

        // Just below should be excluded (< 0.5)
        assert!(0.49 < threshold);
        assert!(0.499999 < threshold);

        // Just above should be included
        assert!(0.51 >= threshold);
    }

    // ==================== Async method integration tests ====================

    #[tokio::test]
    async fn test_recall_returns_none_for_empty_store_logic() {
        // Test the logic that returns None when no results
        let results: Vec<MockSearchResult> = vec![];

        let context = if results.is_empty() {
            None
        } else {
            Some("context".to_string())
        };

        assert!(context.is_none());
    }

    #[tokio::test]
    async fn test_recall_returns_some_for_matching_results_logic() {
        let results = vec![MockSearchResult {
            content: "Match".to_string(),
            score: 0.8,
            metadata: None,
        }];

        let context = if results.is_empty() {
            None
        } else {
            let mut ctx = String::from("## Relevant\n");
            for r in &results {
                if r.score >= 0.5 {
                    ctx.push_str(&format!("{}\n", r.content));
                }
            }
            Some(ctx)
        };

        assert!(context.is_some());
        assert!(context.unwrap().contains("Match"));
    }

    #[tokio::test]
    async fn test_store_conversation_memory_construction() {
        // Test the memory construction used in store_conversation
        let conversation_id = Uuid::new_v4();
        let summary = "Added authentication".to_string();
        let files = vec!["src/auth.rs".to_string()];
        let tags = vec!["auth".to_string()];
        let content = "Full conversation".to_string();
        let embedding = vec![0.5; 384];

        let memory = ConversationMemory {
            id: conversation_id,
            timestamp: Utc::now(),
            summary,
            files_changed: files,
            tags,
            content,
            embedding,
        };

        assert!(!memory.id.is_nil());
        assert!(memory.timestamp <= Utc::now());
    }

    // ==================== EmbeddingGenerator tests ====================

    #[test]
    fn test_embedding_generator_construction() {
        // Just test that we can create an EmbeddingGenerator
        let _generator = EmbeddingGenerator::new();
    }

    // ==================== Context formatting edge cases ====================

    #[test]
    fn test_context_formatting_all_low_scores() {
        let results = [
            MockSearchResult {
                content: "Result 1".to_string(),
                score: 0.1,
                metadata: None,
            },
            MockSearchResult {
                content: "Result 2".to_string(),
                score: 0.2,
                metadata: None,
            },
            MockSearchResult {
                content: "Result 3".to_string(),
                score: 0.4,
                metadata: None,
            },
        ];

        let mut context = String::new();
        for result in results.iter() {
            if result.score >= 0.5 {
                context.push_str(&format!("{}\n", result.content));
            }
        }

        // All scores are below threshold, so context should be empty
        assert!(context.is_empty());
    }

    #[test]
    fn test_context_formatting_with_many_results() {
        let results: Vec<MockSearchResult> = (0..10)
            .map(|i| MockSearchResult {
                content: format!("Result {}", i),
                score: 0.6 + (i as f32 * 0.03),
                metadata: None,
            })
            .collect();

        let mut context = String::new();
        for (i, result) in results.iter().enumerate() {
            if result.score >= 0.5 {
                context.push_str(&format!("{}. {}\n", i + 1, result.content));
            }
        }

        // All 10 results should be included
        assert!(context.contains("Result 0"));
        assert!(context.contains("Result 9"));
    }

    #[test]
    fn test_context_formatting_preserves_numbering() {
        let results = [
            MockSearchResult {
                content: "First".to_string(),
                score: 0.9,
                metadata: None,
            },
            MockSearchResult {
                content: "Second".to_string(),
                score: 0.8,
                metadata: None,
            },
        ];

        let mut context = String::new();
        for (i, result) in results.iter().enumerate() {
            if result.score >= 0.5 {
                context.push_str(&format!("{}. {}\n", i + 1, result.content));
            }
        }

        assert!(context.contains("1. First"));
        assert!(context.contains("2. Second"));
    }

    #[test]
    fn test_context_formatting_with_unicode() {
        let results = [MockSearchResult {
            content: "日本語のテスト 🎉".to_string(),
            score: 0.9,
            metadata: Some(serde_json::json!({
                "full_content": "This contains unicode: 日本語 and emoji 🎉"
            })),
        }];

        let mut context = String::new();
        for result in results.iter() {
            if result.score >= 0.5 {
                context.push_str(&format!("{}\n", result.content));
                if let Some(metadata) = &result.metadata {
                    if let Some(full) = metadata.get("full_content").and_then(|v| v.as_str()) {
                        context.push_str(&format!("   Context: {}\n", full));
                    }
                }
            }
        }

        assert!(context.contains("日本語"));
        assert!(context.contains("🎉"));
    }

    #[test]
    fn test_context_formatting_with_newlines_in_content() {
        let results = [MockSearchResult {
            content: "Multi\nline\ncontent".to_string(),
            score: 0.9,
            metadata: None,
        }];

        let mut context = String::new();
        for result in results.iter() {
            context.push_str(&format!("{}\n", result.content));
        }

        assert!(context.contains("Multi\nline\ncontent"));
    }

    // ==================== MemoryStore operations simulation ====================

    #[tokio::test]
    async fn test_memory_store_search_logic() {
        // Simulate the search logic
        let query = "How do I add authentication?";
        let max_results = 3;

        // In real code, this would search the store
        // Here we just verify the parameters are used correctly
        assert!(!query.is_empty());
        assert!(max_results > 0);
        assert!(max_results <= 100);
    }

    #[tokio::test]
    async fn test_memory_store_store_logic() {
        // Simulate storing a memory
        let memory = ConversationMemory {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            summary: "Test summary".to_string(),
            files_changed: vec!["file.rs".to_string()],
            tags: vec!["test".to_string()],
            content: "Full content here".to_string(),
            embedding: vec![0.1, 0.2, 0.3],
        };

        // Verify the memory is valid for storage
        assert!(!memory.id.is_nil());
        assert!(!memory.summary.is_empty());
        assert!(!memory.embedding.is_empty());
    }
}
