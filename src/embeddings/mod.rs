// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Embeddings generation module using Ollama
//!
//! This module provides embedding generation for semantic search and conversation memory.
//! It uses Ollama's embedding models (nomic-embed-text by default) to convert text into
//! high-dimensional vectors for similarity comparison.

use crate::error::{Result, TedError};
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub mod search;

/// Default embedding model (nomic-embed-text is optimized for text search)
pub const DEFAULT_EMBEDDING_MODEL: &str = "nomic-embed-text";

/// Configuration for embedding generation
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Ollama base URL
    pub base_url: String,
    /// Model to use for embeddings
    pub model: String,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            model: DEFAULT_EMBEDDING_MODEL.to_string(),
        }
    }
}

/// Request body for Ollama embeddings API
#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    input: String,
}

/// Response from Ollama embeddings API
#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
}

/// Embedding generator using Ollama
#[derive(Clone)]
pub struct EmbeddingGenerator {
    config: EmbeddingConfig,
    client: Client,
}

impl EmbeddingGenerator {
    /// Create a new embedding generator with default config
    pub fn new() -> Self {
        Self::with_config(EmbeddingConfig::default())
    }

    /// Create a new embedding generator with custom config
    pub fn with_config(config: EmbeddingConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// Generate embedding for a single text (with auto-pull if model not found)
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        match self.embed_impl(text).await {
            Ok(embedding) => Ok(embedding),
            Err(e) => {
                // Check if it's a model not found error
                let error_msg = format!("{:?}", e);
                if error_msg.contains("MODEL_NOT_FOUND") {
                    eprintln!(
                        "[EMBEDDINGS] Model '{}' not found, attempting to pull...",
                        self.config.model
                    );

                    if self.pull_model().await.is_ok() {
                        eprintln!(
                            "[EMBEDDINGS] Successfully pulled '{}', retrying embed",
                            self.config.model
                        );
                        // Retry after pulling
                        return self.embed_impl(text).await;
                    } else {
                        eprintln!(
                            "[EMBEDDINGS] Failed to pull model, embedding will be unavailable"
                        );
                    }
                }
                Err(e)
            }
        }
    }

    /// Internal embed function - the actual implementation
    async fn embed_impl(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/api/embed", self.config.base_url);

        // Truncate text to avoid exceeding model's context length
        // nomic-embed-text has ~8192 token context, roughly 4 chars per token
        // Use 24000 chars (~6000 tokens) to be safe
        let truncated_text = if text.len() > 24000 {
            eprintln!(
                "[EMBEDDINGS] Truncating text from {} to 24000 chars for embedding",
                text.len()
            );
            &text[..24000]
        } else {
            text
        };

        let request = EmbeddingRequest {
            model: self.config.model.clone(),
            input: truncated_text.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| TedError::Api(crate::error::ApiError::Network(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();

            // Check if model not found - caller should handle auto-pull
            if status == 404 && body.contains("not found") {
                return Err(TedError::Api(crate::error::ApiError::ServerError {
                    status,
                    message: format!("MODEL_NOT_FOUND:{}", body),
                }));
            }

            return Err(TedError::Api(crate::error::ApiError::ServerError {
                status,
                message: format!("Ollama embedding API error: {}", body),
            }));
        }

        let embedding_response: EmbeddingResponse = response.json().await.map_err(|e| {
            TedError::Api(crate::error::ApiError::InvalidResponse(format!(
                "Failed to parse embedding response: {}",
                e
            )))
        })?;

        // Ollama returns a single embedding in the array
        embedding_response
            .embeddings
            .into_iter()
            .next()
            .ok_or_else(|| {
                TedError::Api(crate::error::ApiError::InvalidResponse(
                    "No embeddings in response".to_string(),
                ))
            })
    }

    /// Pull the embedding model from Ollama
    async fn pull_model(&self) -> Result<()> {
        let url = format!("{}/api/pull", self.config.base_url);

        #[derive(Serialize)]
        struct PullRequest {
            name: String,
            stream: bool,
        }

        let request = PullRequest {
            name: self.config.model.clone(),
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| TedError::Api(crate::error::ApiError::Network(e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(TedError::Api(crate::error::ApiError::ServerError {
                status,
                message: format!("Failed to pull model: {}", body),
            }));
        }

        Ok(())
    }

    /// Generate embeddings for multiple texts in batch
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut embeddings = Vec::with_capacity(texts.len());

        for text in texts {
            let embedding = self.embed(text).await?;
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }

    /// Calculate cosine similarity between two embeddings
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot_product / (norm_a * norm_b)
    }
}

impl Default for EmbeddingGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_config_default() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
        assert_eq!(config.model, DEFAULT_EMBEDDING_MODEL);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![-1.0, -2.0, -3.0];
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    // Integration tests require Ollama running
    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_embed_integration() {
        let generator = EmbeddingGenerator::new();
        let result = generator.embed("Hello, world!").await;

        assert!(
            result.is_ok(),
            "Failed to generate embedding: {:?}",
            result.err()
        );
        let embedding = result.unwrap();
        assert!(!embedding.is_empty(), "Embedding should not be empty");

        // nomic-embed-text produces 768-dimensional embeddings
        assert_eq!(
            embedding.len(),
            768,
            "Expected 768-dimensional embedding from nomic-embed-text"
        );
    }

    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_embed_batch_integration() {
        let generator = EmbeddingGenerator::new();
        let texts = vec!["Hello, world!".to_string(), "Goodbye, world!".to_string()];

        let result = generator.embed_batch(&texts).await;
        assert!(result.is_ok());

        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 2);

        for embedding in &embeddings {
            assert_eq!(embedding.len(), 768);
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_semantic_similarity_integration() {
        let generator = EmbeddingGenerator::new();

        let text1 = "The cat sits on the mat";
        let text2 = "A feline rests on the carpet";
        let text3 = "Quantum mechanics is fascinating";

        let emb1 = generator.embed(text1).await.unwrap();
        let emb2 = generator.embed(text2).await.unwrap();
        let emb3 = generator.embed(text3).await.unwrap();

        let sim_1_2 = EmbeddingGenerator::cosine_similarity(&emb1, &emb2);
        let sim_1_3 = EmbeddingGenerator::cosine_similarity(&emb1, &emb3);

        // text1 and text2 are semantically similar, should have higher similarity
        // than text1 and text3
        assert!(
            sim_1_2 > sim_1_3,
            "Similar texts should have higher similarity. sim_1_2={}, sim_1_3={}",
            sim_1_2,
            sim_1_3
        );
    }
}
