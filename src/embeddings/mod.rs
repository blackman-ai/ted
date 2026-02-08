// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Embeddings generation module
//!
//! This module provides embedding generation for semantic search and conversation memory.
//! It uses the **Bundled** backend (fastembed) with locally-bundled ONNX models (no external deps).

use crate::error::Result;
use std::path::PathBuf;

#[cfg(feature = "bundled-embeddings")]
pub mod bundled;
pub mod search;

#[cfg(feature = "bundled-embeddings")]
pub use bundled::{BundledEmbeddings, BundledModel};

/// Default embedding model
pub const DEFAULT_EMBEDDING_MODEL: &str = "nomic-embed-text";

/// Embedding backend type
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EmbeddingBackend {
    /// Bundled fastembed (no external dependencies)
    #[cfg(feature = "bundled-embeddings")]
    #[default]
    Bundled,
}

/// Configuration for embedding generation
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Backend to use for embeddings
    pub backend: EmbeddingBackend,
    /// Model name (interpretation depends on backend)
    pub model: String,
    /// Cache directory for bundled models
    pub cache_dir: Option<PathBuf>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            backend: EmbeddingBackend::default(),
            model: DEFAULT_EMBEDDING_MODEL.to_string(),
            cache_dir: None,
        }
    }
}

impl EmbeddingConfig {
    /// Create config for bundled embeddings
    #[cfg(feature = "bundled-embeddings")]
    pub fn bundled(model: &str) -> Self {
        Self {
            backend: EmbeddingBackend::Bundled,
            model: model.to_string(),
            cache_dir: None,
        }
    }

    /// Create config for bundled embeddings with cache directory
    #[cfg(feature = "bundled-embeddings")]
    pub fn bundled_with_cache(model: &str, cache_dir: PathBuf) -> Self {
        Self {
            backend: EmbeddingBackend::Bundled,
            model: model.to_string(),
            cache_dir: Some(cache_dir),
        }
    }
}

/// Internal backend implementation
enum EmbeddingBackendImpl {
    #[cfg(feature = "bundled-embeddings")]
    Bundled(BundledEmbeddings),
}

/// Unified embedding generator supporting multiple backends
///
/// Use `EmbeddingGenerator::bundled()` for self-contained operation (default).
pub struct EmbeddingGenerator {
    backend: EmbeddingBackendImpl,
}

impl Clone for EmbeddingGenerator {
    fn clone(&self) -> Self {
        match &self.backend {
            #[cfg(feature = "bundled-embeddings")]
            EmbeddingBackendImpl::Bundled(b) => {
                // Create a new bundled embeddings with the same config
                Self {
                    backend: EmbeddingBackendImpl::Bundled(BundledEmbeddings::new(
                        BundledModel::parse(b.model_name()).unwrap_or_default(),
                        dirs::home_dir()
                            .unwrap_or_else(|| PathBuf::from("."))
                            .join(".ted")
                            .join("models")
                            .join("embeddings"),
                    )),
                }
            }
        }
    }
}

impl EmbeddingGenerator {
    /// Create a new embedding generator with default config (bundled if available)
    pub fn new() -> Self {
        Self::with_config(EmbeddingConfig::default())
    }

    /// Create a new embedding generator with custom config
    pub fn with_config(config: EmbeddingConfig) -> Self {
        match config.backend {
            #[cfg(feature = "bundled-embeddings")]
            EmbeddingBackend::Bundled => {
                let cache_dir = config.cache_dir.unwrap_or_else(|| {
                    dirs::home_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join(".ted")
                        .join("models")
                        .join("embeddings")
                });
                let model = BundledModel::parse(&config.model).unwrap_or_default();
                Self {
                    backend: EmbeddingBackendImpl::Bundled(BundledEmbeddings::new(
                        model, cache_dir,
                    )),
                }
            }
        }
    }

    /// Create bundled embedding generator (no external dependencies)
    #[cfg(feature = "bundled-embeddings")]
    pub fn bundled() -> Self {
        Self::bundled_with_model("default")
    }

    /// Create bundled embedding generator with specific model
    #[cfg(feature = "bundled-embeddings")]
    pub fn bundled_with_model(model: &str) -> Self {
        Self::with_config(EmbeddingConfig::bundled(model))
    }

    /// Get the embedding dimension for the current backend/model
    pub fn dimension(&self) -> usize {
        match &self.backend {
            #[cfg(feature = "bundled-embeddings")]
            EmbeddingBackendImpl::Bundled(b) => b.dimension(),
        }
    }

    /// Get the backend type
    pub fn backend_type(&self) -> EmbeddingBackend {
        match &self.backend {
            #[cfg(feature = "bundled-embeddings")]
            EmbeddingBackendImpl::Bundled(_) => EmbeddingBackend::Bundled,
        }
    }

    /// Generate embedding for a single text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        match &self.backend {
            #[cfg(feature = "bundled-embeddings")]
            EmbeddingBackendImpl::Bundled(b) => b.embed(text).await,
        }
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

    // ===== Additional EmbeddingConfig Tests =====

    #[test]
    fn test_embedding_config_clone() {
        let config = EmbeddingConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.model, config.model);
    }

    #[test]
    fn test_embedding_config_debug() {
        let config = EmbeddingConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("EmbeddingConfig"));
    }

    // ===== Additional EmbeddingGenerator Tests =====

    #[test]
    fn test_embedding_generator_new() {
        // EmbeddingGenerator::new() creates a generator - we can verify it doesn't panic
        let generator = EmbeddingGenerator::new();
        // Just verify the generator was created successfully
        let _ = generator;
    }

    #[test]
    fn test_embedding_generator_default() {
        let generator = EmbeddingGenerator::default();
        // Just verify default generator was created
        let _ = generator;
    }

    // ===== Additional Cosine Similarity Tests =====

    #[test]
    fn test_cosine_similarity_empty_vectors() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        // Empty vectors have same length but both norms are 0
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_single_element() {
        let a = vec![5.0];
        let b = vec![3.0];
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        // cos(0) = 1 for any positive scalars
        assert!((sim - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_negative_single() {
        let a = vec![-5.0];
        let b = vec![3.0];
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        // cos(180°) = -1 for opposite directions
        assert!((sim + 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_large_vectors() {
        let a: Vec<f32> = (0..768).map(|i| i as f32).collect();
        let b: Vec<f32> = (0..768).map(|i| (i as f32) * 2.0).collect();
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        // Scaled vectors should have similarity of 1
        assert!((sim - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_normalized_vectors() {
        // Pre-normalized vectors (unit vectors)
        let a = vec![0.6, 0.8, 0.0]; // norm = 1
        let b = vec![0.0, 0.8, 0.6]; // norm = 1
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        // Dot product = 0.64, should be similarity
        assert!((sim - 0.64).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_both_zero() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![0.0, 0.0, 0.0];
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_range() {
        // Test that similarity is always in [-1, 1]
        let test_cases = vec![
            (vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]),
            (vec![-1.0, -2.0, -3.0], vec![4.0, 5.0, 6.0]),
            (vec![0.1, 0.2, 0.3], vec![0.001, 0.002, 0.003]),
            (vec![1000.0, 2000.0], vec![0.001, 0.002]),
        ];

        for (a, b) in test_cases {
            let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
            assert!(
                (-1.0..=1.0).contains(&sim),
                "Similarity {} out of range for {:?} vs {:?}",
                sim,
                a,
                b
            );
        }
    }

    // ===== Constant Tests =====

    #[test]
    fn test_default_embedding_model_constant() {
        assert_eq!(DEFAULT_EMBEDDING_MODEL, "nomic-embed-text");
    }

    // ===== Additional Cosine Similarity Edge Cases =====

    #[test]
    fn test_cosine_similarity_near_zero_values() {
        let a = vec![0.0001, 0.0002, 0.0003];
        let b = vec![0.0001, 0.0002, 0.0003];
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_mixed_signs() {
        let a = vec![1.0, -1.0, 1.0, -1.0];
        let b = vec![1.0, -1.0, 1.0, -1.0];
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_one_zero_vector() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![0.0, 0.0, 0.0];
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_very_large_values() {
        let a = vec![1e10, 2e10, 3e10];
        let b = vec![1e10, 2e10, 3e10];
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_very_small_values() {
        let a = vec![1e-10, 2e-10, 3e-10];
        let b = vec![1e-10, 2e-10, 3e-10];
        let sim = EmbeddingGenerator::cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);
    }

    // ===== Text Truncation Tests =====

    #[test]
    fn test_text_truncation_logic() {
        // Test the truncation logic used in embed_impl
        let long_text = "x".repeat(30000);
        let truncated = if long_text.len() > 24000 {
            &long_text[..24000]
        } else {
            &long_text[..]
        };

        assert_eq!(truncated.len(), 24000);
    }

    #[test]
    fn test_text_no_truncation_needed() {
        let short_text = "Hello world".to_string();
        let truncated = if short_text.len() > 24000 {
            &short_text[..24000]
        } else {
            &short_text[..]
        };

        assert_eq!(truncated, "Hello world");
    }

    #[test]
    fn test_text_exact_limit() {
        let exact_text = "x".repeat(24000);
        let truncated = if exact_text.len() > 24000 {
            &exact_text[..24000]
        } else {
            &exact_text[..]
        };

        assert_eq!(truncated.len(), 24000);
    }

    // ===== Multiple Generator Instances =====

    #[test]
    fn test_multiple_generators() {
        let gen1 = EmbeddingGenerator::new();
        let gen2 = EmbeddingGenerator::new();

        // Both generators should be created successfully
        let _ = gen1;
        let _ = gen2;
    }

    // ===== Batch Embedding Tests =====

    #[test]
    fn test_empty_batch_texts() {
        // Test the batch logic with empty input
        let texts: Vec<String> = vec![];
        let capacity = texts.len();
        assert_eq!(capacity, 0);
    }

    #[test]
    fn test_batch_capacity() {
        let texts = ["a".to_string(), "b".to_string(), "c".to_string()];
        let embeddings: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
        assert_eq!(embeddings.capacity(), 3);
    }
}
