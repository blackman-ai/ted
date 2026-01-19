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
        let content = "User: Add authentication\nAssistant: I'll add JWT authentication...".to_string();

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
        let recalled = recall_relevant_context(
            "How do I add login to my app?",
            &store,
            3,
        )
        .await
        .unwrap();

        assert!(recalled.is_some());
        let context = recalled.unwrap();
        assert!(context.contains("authentication") || context.contains("Relevant"));
    }
}
