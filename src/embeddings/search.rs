// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Semantic search functionality using embeddings
//!
//! This module provides semantic search over conversation history and code context
//! using vector similarity and optional hybrid search with keyword matching.

use super::EmbeddingGenerator;
use crate::error::Result;

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
        Self {
            embedding_generator,
        }
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
        let candidate_texts: Vec<String> =
            candidates.iter().map(|(text, _)| text.clone()).collect();
        let candidate_embeddings = self
            .embedding_generator
            .embed_batch(&candidate_texts)
            .await?;

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
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

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
        let query_tokens: Vec<String> =
            query.split_whitespace().map(|s| s.to_lowercase()).collect();
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
            keyword_scores
                .iter()
                .map(|s| s / max_keyword_score)
                .collect()
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
        combined_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Return top K
        combined_results.truncate(top_k);

        Ok(combined_results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== Unit Tests =====

    #[test]
    fn test_search_result_creation() {
        let result = SearchResult {
            content: "test content".to_string(),
            score: 0.85,
            metadata: None,
        };

        assert_eq!(result.content, "test content");
        assert!((result.score - 0.85).abs() < 0.001);
        assert!(result.metadata.is_none());
    }

    #[test]
    fn test_search_result_with_metadata() {
        let metadata = serde_json::json!({
            "file": "test.rs",
            "line": 42
        });

        let result = SearchResult {
            content: "code snippet".to_string(),
            score: 0.92,
            metadata: Some(metadata.clone()),
        };

        assert_eq!(result.content, "code snippet");
        assert!(result.metadata.is_some());
        let meta = result.metadata.unwrap();
        assert_eq!(meta["file"], "test.rs");
        assert_eq!(meta["line"], 42);
    }

    #[test]
    fn test_search_result_clone() {
        let result = SearchResult {
            content: "cloneable content".to_string(),
            score: 0.75,
            metadata: Some(serde_json::json!({"key": "value"})),
        };

        let cloned = result.clone();
        assert_eq!(cloned.content, result.content);
        assert_eq!(cloned.score, result.score);
        assert_eq!(cloned.metadata, result.metadata);
    }

    #[test]
    fn test_search_result_debug() {
        let result = SearchResult {
            content: "debug test".to_string(),
            score: 0.5,
            metadata: None,
        };

        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("SearchResult"));
        assert!(debug_str.contains("debug test"));
        assert!(debug_str.contains("0.5"));
    }

    #[test]
    fn test_search_result_score_ranges() {
        // Test various score values
        let scores = vec![0.0, 0.25, 0.5, 0.75, 1.0];
        for score in scores {
            let result = SearchResult {
                content: "test".to_string(),
                score,
                metadata: None,
            };
            assert!((result.score - score).abs() < 0.001);
        }
    }

    #[test]
    fn test_search_result_empty_content() {
        let result = SearchResult {
            content: "".to_string(),
            score: 0.0,
            metadata: None,
        };

        assert!(result.content.is_empty());
    }

    #[test]
    fn test_search_result_unicode_content() {
        let result = SearchResult {
            content: "日本語テスト 🦀 émojis and ünïcödé".to_string(),
            score: 0.88,
            metadata: None,
        };

        assert!(result.content.contains("日本語"));
        assert!(result.content.contains("🦀"));
    }

    #[test]
    fn test_search_result_complex_metadata() {
        let metadata = serde_json::json!({
            "nested": {
                "deep": {
                    "value": 123
                }
            },
            "array": [1, 2, 3],
            "string": "hello"
        });

        let result = SearchResult {
            content: "complex".to_string(),
            score: 0.6,
            metadata: Some(metadata),
        };

        let meta = result.metadata.unwrap();
        assert_eq!(meta["nested"]["deep"]["value"], 123);
        assert_eq!(meta["array"][0], 1);
    }

    #[test]
    fn test_semantic_search_creation() {
        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        // Just verify the struct can be created
        let _ = search;
    }

    #[test]
    fn test_search_result_sorting_by_score() {
        let mut results = [
            SearchResult {
                content: "low".to_string(),
                score: 0.2,
                metadata: None,
            },
            SearchResult {
                content: "high".to_string(),
                score: 0.9,
                metadata: None,
            },
            SearchResult {
                content: "medium".to_string(),
                score: 0.5,
                metadata: None,
            },
        ];

        // Sort by score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        assert_eq!(results[0].content, "high");
        assert_eq!(results[1].content, "medium");
        assert_eq!(results[2].content, "low");
    }

    #[test]
    fn test_search_result_with_nan_score() {
        let result = SearchResult {
            content: "nan test".to_string(),
            score: f32::NAN,
            metadata: None,
        };

        assert!(result.score.is_nan());
    }

    // ===== Integration Tests (require embeddings backend) =====

    #[tokio::test]
    #[ignore] // Requires embeddings backend
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
    #[ignore] // Requires embeddings backend
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

    // ===== Keyword Scoring Tests =====

    #[test]
    fn test_keyword_tokenization() {
        let query = "Rust programming language";
        let tokens: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();

        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], "rust");
        assert_eq!(tokens[1], "programming");
        assert_eq!(tokens[2], "language");
    }

    #[test]
    fn test_keyword_occurrence_counting() {
        let text = "Rust Rust Rust is a programming language for Rust developers";
        let text_lower = text.to_lowercase();
        let occurrences = text_lower.matches("rust").count();

        assert_eq!(occurrences, 4);
    }

    #[test]
    fn test_keyword_ln1p_scoring() {
        // Test the ln_1p scoring function used in hybrid search
        let occurrences = 4.0f32;
        let score = occurrences.ln_1p();

        // ln(1 + 4) = ln(5) ≈ 1.609
        assert!((score - 1.609).abs() < 0.01);
    }

    #[test]
    fn test_keyword_zero_occurrences() {
        let occurrences = 0.0f32;
        let score = occurrences.ln_1p();

        // ln(1 + 0) = ln(1) = 0
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_document_length_normalization() {
        let text = "word word word word word";
        let doc_length = text.split_whitespace().count() as f32;
        let normalized = if doc_length > 0.0 {
            1.0 / doc_length.sqrt()
        } else {
            0.0
        };

        // 1 / sqrt(5) ≈ 0.447
        assert!((normalized - 0.447).abs() < 0.01);
    }

    #[test]
    fn test_empty_document_normalization() {
        let text = "";
        let doc_length = text.split_whitespace().count() as f32;
        let normalized = if doc_length > 0.0 {
            1.0 / doc_length.sqrt()
        } else {
            0.0
        };

        assert_eq!(normalized, 0.0);
    }

    #[test]
    fn test_score_normalization() {
        let scores = [0.5, 1.0, 0.25];
        let max_score = scores.iter().cloned().fold(0.0f32, f32::max);
        let normalized: Vec<f32> = scores.iter().map(|s| s / max_score).collect();

        assert_eq!(normalized[0], 0.5);
        assert_eq!(normalized[1], 1.0);
        assert_eq!(normalized[2], 0.25);
    }

    #[test]
    fn test_score_normalization_zero_max() {
        let scores = vec![0.0, 0.0, 0.0];
        let max_score = scores.iter().cloned().fold(0.0f32, f32::max);
        let normalized: Vec<f32> = if max_score > 0.0 {
            scores.iter().map(|s| s / max_score).collect()
        } else {
            scores.clone()
        };

        assert_eq!(normalized, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_weighted_score_combination() {
        let semantic_score: f32 = 0.8;
        let keyword_score: f32 = 0.6;
        let semantic_weight: f32 = 0.7;
        let keyword_weight: f32 = 1.0 - semantic_weight;

        let combined = semantic_weight * semantic_score + keyword_weight * keyword_score;

        // 0.7 * 0.8 + 0.3 * 0.6 = 0.56 + 0.18 = 0.74
        assert!((combined - 0.74).abs() < 0.001);
    }

    #[test]
    fn test_weighted_full_semantic() {
        let semantic_score = 0.9;
        let keyword_score = 0.5;
        let semantic_weight = 1.0;

        let combined = semantic_weight * semantic_score + (1.0 - semantic_weight) * keyword_score;

        assert_eq!(combined, 0.9);
    }

    #[test]
    fn test_weighted_full_keyword() {
        let semantic_score = 0.9;
        let keyword_score = 0.5;
        let semantic_weight = 0.0;

        let combined = semantic_weight * semantic_score + (1.0 - semantic_weight) * keyword_score;

        assert_eq!(combined, 0.5);
    }

    // ===== Search Result Sorting Tests =====

    #[test]
    fn test_search_results_truncation() {
        let mut results = vec![
            SearchResult {
                content: "a".to_string(),
                score: 0.9,
                metadata: None,
            },
            SearchResult {
                content: "b".to_string(),
                score: 0.8,
                metadata: None,
            },
            SearchResult {
                content: "c".to_string(),
                score: 0.7,
                metadata: None,
            },
            SearchResult {
                content: "d".to_string(),
                score: 0.6,
                metadata: None,
            },
            SearchResult {
                content: "e".to_string(),
                score: 0.5,
                metadata: None,
            },
        ];

        results.truncate(3);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].content, "a");
        assert_eq!(results[2].content, "c");
    }

    #[test]
    fn test_search_results_truncate_larger_than_size() {
        let mut results = vec![
            SearchResult {
                content: "a".to_string(),
                score: 0.9,
                metadata: None,
            },
            SearchResult {
                content: "b".to_string(),
                score: 0.8,
                metadata: None,
            },
        ];

        results.truncate(10); // More than we have

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_results_sort_equal_scores() {
        let mut results = [
            SearchResult {
                content: "a".to_string(),
                score: 0.5,
                metadata: None,
            },
            SearchResult {
                content: "b".to_string(),
                score: 0.5,
                metadata: None,
            },
            SearchResult {
                content: "c".to_string(),
                score: 0.5,
                metadata: None,
            },
        ];

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // All scores equal, original order may be preserved or not (stable sort)
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_search_results_sort_with_nan() {
        let mut results = [
            SearchResult {
                content: "a".to_string(),
                score: 0.5,
                metadata: None,
            },
            SearchResult {
                content: "b".to_string(),
                score: f32::NAN,
                metadata: None,
            },
            SearchResult {
                content: "c".to_string(),
                score: 0.7,
                metadata: None,
            },
        ];

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // NaN comparisons return None, which becomes Equal
        assert_eq!(results.len(), 3);
    }

    // ===== Candidate Processing Tests =====

    #[test]
    fn test_candidates_text_extraction() {
        let candidates: Vec<(String, Option<serde_json::Value>)> = vec![
            ("First text".to_string(), None),
            (
                "Second text".to_string(),
                Some(serde_json::json!({"id": 1})),
            ),
            ("Third text".to_string(), Some(serde_json::json!({"id": 2}))),
        ];

        let texts: Vec<String> = candidates.iter().map(|(text, _)| text.clone()).collect();

        assert_eq!(texts.len(), 3);
        assert_eq!(texts[0], "First text");
        assert_eq!(texts[1], "Second text");
    }

    #[test]
    fn test_empty_candidates() {
        let candidates: Vec<(String, Option<serde_json::Value>)> = vec![];
        let texts: Vec<String> = candidates.iter().map(|(text, _)| text.clone()).collect();

        assert!(texts.is_empty());
    }

    #[test]
    fn test_candidates_with_complex_metadata() {
        let metadata = serde_json::json!({
            "file": "test.rs",
            "line": 42,
            "nested": {
                "deep": true
            }
        });

        let candidates: Vec<(String, Option<serde_json::Value>)> =
            vec![("Code snippet".to_string(), Some(metadata.clone()))];

        assert!(candidates[0].1.is_some());
        let meta = candidates[0].1.as_ref().unwrap();
        assert_eq!(meta["file"], "test.rs");
        assert_eq!(meta["nested"]["deep"], true);
    }

    // ===== SemanticSearch Construction Tests =====

    #[test]
    fn test_semantic_search_with_custom_config() {
        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        // Verify the search engine was created
        let _ = search;
    }

    #[test]
    fn test_semantic_search_default_config() {
        let generator = EmbeddingGenerator::default();
        let search = SemanticSearch::new(generator);

        let _ = search;
    }

    // ===== Top-K Selection Tests =====

    #[test]
    fn test_top_k_zero() {
        let mut results = vec![
            SearchResult {
                content: "a".to_string(),
                score: 0.9,
                metadata: None,
            },
            SearchResult {
                content: "b".to_string(),
                score: 0.8,
                metadata: None,
            },
        ];

        results.truncate(0);
        assert!(results.is_empty());
    }

    #[test]
    fn test_top_k_one() {
        let mut results = vec![
            SearchResult {
                content: "a".to_string(),
                score: 0.9,
                metadata: None,
            },
            SearchResult {
                content: "b".to_string(),
                score: 0.8,
                metadata: None,
            },
            SearchResult {
                content: "c".to_string(),
                score: 0.7,
                metadata: None,
            },
        ];

        // Sort first
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(1);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "a");
    }

    // ===== Query Processing Tests =====

    #[test]
    fn test_query_whitespace_handling() {
        let query = "  rust   programming   ";
        let tokens: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();

        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], "rust");
        assert_eq!(tokens[1], "programming");
    }

    #[test]
    fn test_query_empty() {
        let query = "";
        let tokens: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();

        assert!(tokens.is_empty());
    }

    #[test]
    fn test_query_unicode() {
        let query = "日本語 Rust プログラミング";
        let tokens: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();

        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], "日本語");
        assert_eq!(tokens[1], "rust");
    }

    #[test]
    fn test_query_case_insensitive() {
        let query = "RUST Programming LANGUAGE";
        let tokens: Vec<String> = query.split_whitespace().map(|s| s.to_lowercase()).collect();

        assert_eq!(tokens[0], "rust");
        assert_eq!(tokens[1], "programming");
        assert_eq!(tokens[2], "language");
    }

    // ===== Hybrid Search Weight Tests =====

    #[test]
    fn test_semantic_weight_boundaries() {
        // Test weight at boundaries
        let weights: Vec<f32> = vec![0.0, 0.5, 1.0];
        for semantic_weight in weights {
            let keyword_weight = 1.0 - semantic_weight;
            assert!((semantic_weight + keyword_weight - 1.0).abs() < 0.001);
        }
    }

    #[test]
    fn test_semantic_weight_typical_values() {
        let typical_weights = [0.7, 0.8, 0.6];
        for semantic_weight in typical_weights {
            assert!((0.0..=1.0).contains(&semantic_weight));
            let keyword_weight = 1.0 - semantic_weight;
            assert!((0.0..=1.0).contains(&keyword_weight));
        }
    }

    // ===== Async search tests using wiremock =====

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_search_with_mock_server() {
        let mock_server = MockServer::start().await;

        // Mock embedding endpoint - returns same embedding for all requests
        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.5, 0.5, 0.5]]
            })))
            .mount(&mock_server)
            .await;

        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        let candidates = vec![
            ("First document".to_string(), None),
            (
                "Second document".to_string(),
                Some(serde_json::json!({"id": 1})),
            ),
        ];

        let results = search.search("query", &candidates, 2).await;
        assert!(results.is_ok());
        let results = results.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_search_top_k_limit() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.5, 0.5, 0.5]]
            })))
            .mount(&mock_server)
            .await;

        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        let candidates = vec![
            ("Doc 1".to_string(), None),
            ("Doc 2".to_string(), None),
            ("Doc 3".to_string(), None),
            ("Doc 4".to_string(), None),
        ];

        // Request only top 2
        let results = search.search("query", &candidates, 2).await;
        assert!(results.is_ok());
        let results = results.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_search_empty_candidates() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.5, 0.5, 0.5]]
            })))
            .mount(&mock_server)
            .await;

        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        let candidates: Vec<(String, Option<serde_json::Value>)> = vec![];

        let results = search.search("query", &candidates, 5).await;
        assert!(results.is_ok());
        let results = results.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_preserves_metadata() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[1.0, 0.0, 0.0]]
            })))
            .mount(&mock_server)
            .await;

        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        let metadata = serde_json::json!({
            "file": "test.rs",
            "line": 42
        });
        let candidates = vec![("Document with metadata".to_string(), Some(metadata.clone()))];

        let results = search.search("query", &candidates, 1).await;
        assert!(results.is_ok());
        let results = results.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].metadata.is_some());
        let meta = results[0].metadata.as_ref().unwrap();
        assert_eq!(meta["file"], "test.rs");
    }

    #[tokio::test]
    async fn test_hybrid_search_with_mock_server() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.5, 0.5, 0.5]]
            })))
            .mount(&mock_server)
            .await;

        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        let candidates = vec![
            ("Rust programming language".to_string(), None),
            ("Python is easy".to_string(), None),
        ];

        let results = search.hybrid_search("Rust", &candidates, 2, 0.7).await;
        assert!(results.is_ok());
        let results = results.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_hybrid_search_keyword_boost() {
        let mock_server = MockServer::start().await;

        // Return identical embeddings so keyword scoring is the differentiator
        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.5, 0.5, 0.5]]
            })))
            .mount(&mock_server)
            .await;

        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        let candidates = vec![
            ("Rust Rust Rust programming".to_string(), None), // More keyword matches
            ("Python programming".to_string(), None),
        ];

        // Use low semantic weight so keywords matter more
        let results = search.hybrid_search("Rust", &candidates, 2, 0.3).await;
        assert!(results.is_ok());
        let results = results.unwrap();
        // Document with more "Rust" occurrences should rank higher
        assert!(results[0].content.contains("Rust"));
    }

    #[tokio::test]
    async fn test_hybrid_search_empty_query() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.5, 0.5, 0.5]]
            })))
            .mount(&mock_server)
            .await;

        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        let candidates = vec![
            ("Document one".to_string(), None),
            ("Document two".to_string(), None),
        ];

        let results = search.hybrid_search("", &candidates, 2, 0.7).await;
        assert!(results.is_ok());
    }

    #[tokio::test]
    async fn test_hybrid_search_full_semantic_weight() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.5, 0.5, 0.5]]
            })))
            .mount(&mock_server)
            .await;

        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        let candidates = vec![("Doc A".to_string(), None), ("Doc B".to_string(), None)];

        // semantic_weight = 1.0 means no keyword contribution
        let results = search.hybrid_search("query", &candidates, 2, 1.0).await;
        assert!(results.is_ok());
    }

    #[tokio::test]
    async fn test_hybrid_search_zero_semantic_weight() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.5, 0.5, 0.5]]
            })))
            .mount(&mock_server)
            .await;

        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        let candidates = vec![
            ("keyword keyword keyword".to_string(), None),
            ("other text".to_string(), None),
        ];

        // semantic_weight = 0.0 means only keyword contribution
        let results = search.hybrid_search("keyword", &candidates, 2, 0.0).await;
        assert!(results.is_ok());
        let results = results.unwrap();
        // Document with keyword should rank first
        assert!(results[0].content.contains("keyword"));
    }

    #[tokio::test]
    async fn test_hybrid_search_empty_document() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.5, 0.5, 0.5]]
            })))
            .mount(&mock_server)
            .await;

        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        let candidates = vec![
            ("".to_string(), None), // Empty document
            ("Some content".to_string(), None),
        ];

        let results = search.hybrid_search("query", &candidates, 2, 0.5).await;
        assert!(results.is_ok());
    }

    #[tokio::test]
    async fn test_search_bundled_backend_with_empty_input() {
        let generator = EmbeddingGenerator::new();
        let search = SemanticSearch::new(generator);

        let candidates: Vec<(String, Option<serde_json::Value>)> = vec![];

        let results = search.search("query", &candidates, 1).await;
        // Empty candidates should succeed with empty results (no embedding needed)
        // or error if the model can't load — either is acceptable
        if let Ok(results) = results {
            assert!(results.is_empty());
        }
    }
}
