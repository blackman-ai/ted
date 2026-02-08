// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Bundled embeddings using fastembed-rs
//!
//! This module provides embedding generation using locally-bundled ONNX models
//! via fastembed. No external server is required.

use crate::error::{Result, TedError};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Supported embedding models
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BundledModel {
    /// all-MiniLM-L6-v2: Fast, lightweight (384 dimensions, ~25MB)
    #[default]
    AllMiniLmL6V2,
    /// nomic-embed-text-v1.5: Higher quality (768 dimensions, ~130MB)
    NomicEmbedTextV15,
    /// BGE Small EN v1.5: Good balance (384 dimensions, ~45MB)
    BgeSmallEnV15,
}

impl BundledModel {
    /// Get the fastembed model type
    fn to_fastembed(self) -> EmbeddingModel {
        match self {
            BundledModel::AllMiniLmL6V2 => EmbeddingModel::AllMiniLML6V2,
            BundledModel::NomicEmbedTextV15 => EmbeddingModel::NomicEmbedTextV15,
            BundledModel::BgeSmallEnV15 => EmbeddingModel::BGESmallENV15,
        }
    }

    /// Get the embedding dimension for this model
    pub fn dimension(&self) -> usize {
        match self {
            BundledModel::AllMiniLmL6V2 => 384,
            BundledModel::NomicEmbedTextV15 => 768,
            BundledModel::BgeSmallEnV15 => 384,
        }
    }

    /// Get the approximate model size in bytes
    pub fn size_bytes(&self) -> u64 {
        match self {
            BundledModel::AllMiniLmL6V2 => 25 * 1024 * 1024, // ~25MB
            BundledModel::NomicEmbedTextV15 => 130 * 1024 * 1024, // ~130MB
            BundledModel::BgeSmallEnV15 => 45 * 1024 * 1024, // ~45MB
        }
    }

    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            BundledModel::AllMiniLmL6V2 => "all-MiniLM-L6-v2",
            BundledModel::NomicEmbedTextV15 => "nomic-embed-text-v1.5",
            BundledModel::BgeSmallEnV15 => "bge-small-en-v1.5",
        }
    }

    /// Parse model name from string
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "all-minilm-l6-v2" | "minilm" | "default" => Some(BundledModel::AllMiniLmL6V2),
            "nomic-embed-text-v1.5" | "nomic" | "nomic-embed-text" => {
                Some(BundledModel::NomicEmbedTextV15)
            }
            "bge-small-en-v1.5" | "bge" | "bge-small" => Some(BundledModel::BgeSmallEnV15),
            _ => None,
        }
    }
}

/// Bundled embedding generator using fastembed
///
/// This is a synchronous wrapper around fastembed's TextEmbedding.
/// The model is lazy-loaded on first use and cached for subsequent calls.
pub struct BundledEmbeddings {
    model_type: BundledModel,
    cache_dir: PathBuf,
    /// Lazy-loaded model (fastembed downloads on first use)
    model: Arc<RwLock<Option<TextEmbedding>>>,
}

impl BundledEmbeddings {
    /// Create a new bundled embeddings generator
    ///
    /// The model will be downloaded to `cache_dir` on first use if not already present.
    pub fn new(model_type: BundledModel, cache_dir: PathBuf) -> Self {
        Self {
            model_type,
            cache_dir,
            model: Arc::new(RwLock::new(None)),
        }
    }

    /// Create with default model (all-MiniLM-L6-v2) and default cache dir
    pub fn default_with_cache(cache_dir: PathBuf) -> Self {
        Self::new(BundledModel::default(), cache_dir)
    }

    /// Get the embedding dimension
    pub fn dimension(&self) -> usize {
        self.model_type.dimension()
    }

    /// Get the model name
    pub fn model_name(&self) -> &'static str {
        self.model_type.name()
    }

    /// Initialize the model (downloads if needed)
    async fn ensure_model(&self) -> Result<()> {
        // Check if already loaded
        {
            let guard = self.model.read().await;
            if guard.is_some() {
                return Ok(());
            }
        }

        // Need to load - take write lock
        let mut guard = self.model.write().await;

        // Double-check after acquiring write lock
        if guard.is_some() {
            return Ok(());
        }

        tracing::info!(
            "Loading embedding model '{}' (this may download ~{}MB on first run)",
            self.model_type.name(),
            self.model_type.size_bytes() / (1024 * 1024)
        );

        // Create cache directory if it doesn't exist
        std::fs::create_dir_all(&self.cache_dir).map_err(|e| {
            TedError::Io(std::io::Error::other(format!(
                "Failed to create cache dir: {}",
                e
            )))
        })?;

        // Initialize fastembed - this is a blocking operation
        let model_type = self.model_type;
        let cache_dir = self.cache_dir.clone();

        let model = tokio::task::spawn_blocking(move || {
            TextEmbedding::try_new(
                InitOptions::new(model_type.to_fastembed())
                    .with_cache_dir(cache_dir)
                    .with_show_download_progress(true),
            )
        })
        .await
        .map_err(|e| TedError::Context(format!("Task join error: {}", e)))?
        .map_err(|e| TedError::Context(format!("Failed to load embedding model: {}", e)))?;

        *guard = Some(model);
        tracing::info!(
            "Embedding model '{}' loaded successfully",
            self.model_type.name()
        );

        Ok(())
    }

    /// Generate embedding for a single text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.ensure_model().await?;

        let guard = self.model.read().await;
        let model = guard
            .as_ref()
            .ok_or_else(|| TedError::Context("Model not loaded after ensure_model".to_string()))?;

        // Truncate text to avoid exceeding model limits
        // Most models handle ~512 tokens well, roughly 2000 chars
        let truncated = if text.len() > 8000 {
            tracing::debug!(
                "Truncating text from {} to 8000 chars for embedding",
                text.len()
            );
            &text[..8000]
        } else {
            text
        };

        // fastembed embed is synchronous, run in blocking task
        let text_owned = truncated.to_string();

        // Clone the model Arc for the blocking task
        // Note: We need to clone the actual model, but TextEmbedding is not Clone
        // So we need a different approach - get the result directly

        // Actually, we can't easily move the model into spawn_blocking
        // Let's just call it synchronously since it's CPU-bound anyway
        let embeddings = model
            .embed(vec![text_owned], None)
            .map_err(|e| TedError::Context(format!("Embedding generation failed: {}", e)))?;

        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| TedError::Context("No embedding returned from model".to_string()))
    }

    /// Generate embeddings for multiple texts in batch
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        self.ensure_model().await?;

        let guard = self.model.read().await;
        let model = guard
            .as_ref()
            .ok_or_else(|| TedError::Context("Model not loaded after ensure_model".to_string()))?;

        // Truncate texts
        let truncated: Vec<String> = texts
            .iter()
            .map(|t| {
                if t.len() > 8000 {
                    t[..8000].to_string()
                } else {
                    t.clone()
                }
            })
            .collect();

        model
            .embed(truncated, None)
            .map_err(|e| TedError::Context(format!("Batch embedding failed: {}", e)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundled_model_default() {
        let model = BundledModel::default();
        assert_eq!(model, BundledModel::AllMiniLmL6V2);
    }

    #[test]
    fn test_bundled_model_dimension() {
        assert_eq!(BundledModel::AllMiniLmL6V2.dimension(), 384);
        assert_eq!(BundledModel::NomicEmbedTextV15.dimension(), 768);
        assert_eq!(BundledModel::BgeSmallEnV15.dimension(), 384);
    }

    #[test]
    fn test_bundled_model_name() {
        assert_eq!(BundledModel::AllMiniLmL6V2.name(), "all-MiniLM-L6-v2");
        assert_eq!(
            BundledModel::NomicEmbedTextV15.name(),
            "nomic-embed-text-v1.5"
        );
        assert_eq!(BundledModel::BgeSmallEnV15.name(), "bge-small-en-v1.5");
    }

    #[test]
    fn test_bundled_model_from_str() {
        assert_eq!(
            BundledModel::parse("all-minilm-l6-v2"),
            Some(BundledModel::AllMiniLmL6V2)
        );
        assert_eq!(
            BundledModel::parse("minilm"),
            Some(BundledModel::AllMiniLmL6V2)
        );
        assert_eq!(
            BundledModel::parse("default"),
            Some(BundledModel::AllMiniLmL6V2)
        );
        assert_eq!(
            BundledModel::parse("nomic"),
            Some(BundledModel::NomicEmbedTextV15)
        );
        assert_eq!(
            BundledModel::parse("bge"),
            Some(BundledModel::BgeSmallEnV15)
        );
        assert_eq!(BundledModel::parse("unknown"), None);
    }

    #[test]
    fn test_bundled_model_size() {
        // Just verify sizes are reasonable
        assert!(BundledModel::AllMiniLmL6V2.size_bytes() > 10 * 1024 * 1024);
        assert!(BundledModel::NomicEmbedTextV15.size_bytes() > 100 * 1024 * 1024);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = BundledEmbeddings::cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = BundledEmbeddings::cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![-1.0, -2.0, -3.0];
        let sim = BundledEmbeddings::cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = BundledEmbeddings::cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = BundledEmbeddings::cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_bundled_embeddings_new() {
        let cache_dir = std::env::temp_dir().join("ted-test-embeddings");
        let embeddings = BundledEmbeddings::new(BundledModel::AllMiniLmL6V2, cache_dir);
        assert_eq!(embeddings.dimension(), 384);
        assert_eq!(embeddings.model_name(), "all-MiniLM-L6-v2");
    }

    #[test]
    fn test_bundled_embeddings_default_with_cache() {
        let cache_dir = std::env::temp_dir().join("ted-test-embeddings");
        let embeddings = BundledEmbeddings::default_with_cache(cache_dir);
        assert_eq!(embeddings.dimension(), 384);
    }

    // Integration test - requires downloading the model
    #[tokio::test]
    #[ignore] // Run with: cargo test -- --ignored
    async fn test_bundled_embed_integration() {
        let cache_dir = dirs::home_dir()
            .unwrap()
            .join(".ted")
            .join("models")
            .join("embeddings");

        let embeddings = BundledEmbeddings::new(BundledModel::AllMiniLmL6V2, cache_dir);

        let result = embeddings.embed("Hello, world!").await;
        assert!(
            result.is_ok(),
            "Failed to generate embedding: {:?}",
            result.err()
        );

        let embedding = result.unwrap();
        assert_eq!(embedding.len(), 384, "Expected 384-dimensional embedding");
    }

    #[tokio::test]
    #[ignore]
    async fn test_bundled_semantic_similarity() {
        let cache_dir = dirs::home_dir()
            .unwrap()
            .join(".ted")
            .join("models")
            .join("embeddings");

        let embeddings = BundledEmbeddings::new(BundledModel::AllMiniLmL6V2, cache_dir);

        let text1 = "The cat sits on the mat";
        let text2 = "A feline rests on the carpet";
        let text3 = "Quantum mechanics is fascinating";

        let emb1 = embeddings.embed(text1).await.unwrap();
        let emb2 = embeddings.embed(text2).await.unwrap();
        let emb3 = embeddings.embed(text3).await.unwrap();

        let sim_1_2 = BundledEmbeddings::cosine_similarity(&emb1, &emb2);
        let sim_1_3 = BundledEmbeddings::cosine_similarity(&emb1, &emb3);

        assert!(
            sim_1_2 > sim_1_3,
            "Similar texts should have higher similarity. sim_1_2={}, sim_1_3={}",
            sim_1_2,
            sim_1_3
        );
    }
}
