// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Semantic search functionality using embeddings
//!
//! This module provides semantic search over conversation history and code context
//! using vector similarity and optional hybrid search with keyword matching.

use crate::error::Result;
use super::EmbeddingGenerator;

/// A search result with similarity score
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The text content
    pub content: String,
    /// Similarity score (0.0 to 1.0, higher is more similar)
    pub score: f32,
    /// Optional metadata (e.g., file path, timestamp, conversation ID)
    pub metadata: Option<serde_json::Value>,
}

/// Semantic search engine
pub struct SemanticSearch {
    embedding_generator: EmbeddingGenerator,
}

impl SemanticSearch {
    /// Create a new semantic search engine
    pub fn new(embedding_generator: EmbeddingGenerator) -> Self {
        Self { embedding_generator }
    }

    /// Search for similar texts using cosine similarity
    ///
    /// Returns results sorted by similarity score (highest first)
    pub async fn search(
        &self,
        query: &str,
        candidates: &[(String, Option<serde_json::Value>)],
        top_k: usize,
    ) -> Result<Vec<SearchResult>> {
        // Generate embedding for query
        let query_embedding = self.embedding_generator.embed(query).await?;

        // Generate embeddings for all candidates
        let candidate_texts: Vec<String> = candidates.iter().map(|(text, _)| text.clone()).collect();
        let candidate_embeddings = self.embedding_generator.embed_batch(&candidate_texts).await?;

        // Calculate similarities
        let mut results: Vec<SearchResult> = candidates
            .iter()
            .zip(candidate_embeddings.iter())
            .map(|((text, metadata), embedding)| {
                let score = EmbeddingGenerator::cosine_similarity(&query_embedding, embedding);
                SearchResult {
                    content: text.clone(),
                    score,
                    metadata: metadata.clone(),
                }
            })
            .collect();

        // Sort by score (descending)
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Return top K results
        results.truncate(top_k);

        Ok(results)
    }

    /// Hybrid search combining semantic similarity and keyword matching
    ///
    /// Uses weighted combination of semantic similarity and BM25-like keyword scoring
    pub async fn hybrid_search(
        &self,
        query: &str,
        candidates: &[(String, Option<serde_json::Value>)],
        top_k: usize,
        semantic_weight: f32,
    ) -> Result<Vec<SearchResult>> {
        // Semantic search
        let semantic_results = self.search(query, candidates, candidates.len()).await?;

        // Simple keyword scoring (BM25-like)
        let query_tokens: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();
        let keyword_scores: Vec<f32> = candidates
            .iter()
            .map(|(text, _)| {
                let text_lower = text.to_lowercase();
                let mut score = 0.0;

                for token in &query_tokens {
                    // Count occurrences
                    let occurrences = text_lower.matches(token.as_str()).count() as f32;
                    // Simple TF scoring
                    score += occurrences.ln_1p();
                }

                // Normalize by document length
                let doc_length = text.split_whitespace().count() as f32;
                if doc_length > 0.0 {
                    score / doc_length.sqrt()
                } else {
                    0.0
                }
            })
            .collect();

        // Normalize keyword scores to 0-1 range
        let max_keyword_score = keyword_scores.iter().cloned().fold(0.0f32, f32::max);
        let normalized_keyword_scores: Vec<f32> = if max_keyword_score > 0.0 {
            keyword_scores.iter().map(|s| s / max_keyword_score).collect()
        } else {
            keyword_scores
        };

        // Combine scores
        let keyword_weight = 1.0 - semantic_weight;
        let mut combined_results: Vec<SearchResult> = semantic_results
            .into_iter()
            .zip(normalized_keyword_scores.iter())
            .map(|(mut result, keyword_score)| {
                result.score = semantic_weight * result.score + keyword_weight * keyword_score;
                result
            })
            .collect();

        // Sort by combined score
        combined_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Return top K
        combined_results.truncate(top_k);

        Ok(combined_results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires Ollama running
    async fn test_semantic_search() {
        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        let candidates = vec![
            ("The cat sits on the mat".to_string(), None),
            ("A dog runs in the park".to_string(), None),
            ("Quantum physics is complex".to_string(), None),
            ("The feline rests on the carpet".to_string(), None),
        ];

        let results = search.search("cat on mat", &candidates, 2).await.unwrap();

        assert_eq!(results.len(), 2);
        // First result should be "The cat sits on the mat"
        assert!(results[0].content.contains("cat") || results[0].content.contains("feline"));
        // Scores should be in descending order
        assert!(results[0].score >= results[1].score);
    }

    #[tokio::test]
    #[ignore] // Requires Ollama running
    async fn test_hybrid_search() {
        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        let candidates = vec![
            ("Rust programming language".to_string(), None),
            ("Rust is fast and safe".to_string(), None),
            ("Python is easy to learn".to_string(), None),
            ("JavaScript for web development".to_string(), None),
        ];

        let results = search
            .hybrid_search("Rust programming", &candidates, 2, 0.7)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        // Top results should be about Rust
        assert!(results[0].content.contains("Rust"));
    }
}
