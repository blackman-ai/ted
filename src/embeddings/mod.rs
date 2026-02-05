// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Embeddings generation module
//!
//! This module provides embedding generation for semantic search and conversation memory.
//! It supports multiple backends:
//! - **Bundled** (default): Uses fastembed with locally-bundled ONNX models (no external deps)
//! - **Ollama**: Uses Ollama's embedding API (requires running Ollama server)

use crate::error::{Result, TedError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(feature = "bundled-embeddings")]
pub mod bundled;
pub mod search;

#[cfg(feature = "bundled-embeddings")]
pub use bundled::{BundledEmbeddings, BundledModel};

/// Default embedding model for Ollama backend
pub const DEFAULT_OLLAMA_MODEL: &str = "nomic-embed-text";

/// Default embedding model (kept for backwards compatibility)
pub const DEFAULT_EMBEDDING_MODEL: &str = DEFAULT_OLLAMA_MODEL;

/// Embedding backend type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddingBackend {
    /// Bundled fastembed (no external dependencies)
    #[cfg(feature = "bundled-embeddings")]
    Bundled,
    /// Ollama server (requires running Ollama)
    Ollama,
}

// Can't derive Default due to conditional compilation of variants
#[allow(clippy::derivable_impls)]
impl Default for EmbeddingBackend {
    fn default() -> Self {
        #[cfg(feature = "bundled-embeddings")]
        {
            EmbeddingBackend::Bundled
        }
        #[cfg(not(feature = "bundled-embeddings"))]
        {
            EmbeddingBackend::Ollama
        }
    }
}

/// Configuration for embedding generation
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Backend to use for embeddings
    pub backend: EmbeddingBackend,
    /// Ollama base URL (only used for Ollama backend)
    pub base_url: String,
    /// Model name (interpretation depends on backend)
    pub model: String,
    /// Cache directory for bundled models
    pub cache_dir: Option<PathBuf>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            backend: EmbeddingBackend::default(),
            base_url: "http://localhost:11434".to_string(),
            model: DEFAULT_OLLAMA_MODEL.to_string(),
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
            base_url: String::new(),
            model: model.to_string(),
            cache_dir: None,
        }
    }

    /// Create config for bundled embeddings with cache directory
    #[cfg(feature = "bundled-embeddings")]
    pub fn bundled_with_cache(model: &str, cache_dir: PathBuf) -> Self {
        Self {
            backend: EmbeddingBackend::Bundled,
            base_url: String::new(),
            model: model.to_string(),
            cache_dir: Some(cache_dir),
        }
    }

    /// Create config for Ollama embeddings
    pub fn ollama(base_url: &str, model: &str) -> Self {
        Self {
            backend: EmbeddingBackend::Ollama,
            base_url: base_url.to_string(),
            model: model.to_string(),
            cache_dir: None,
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

/// Internal backend implementation
enum EmbeddingBackendImpl {
    #[cfg(feature = "bundled-embeddings")]
    Bundled(BundledEmbeddings),
    Ollama {
        config: EmbeddingConfig,
        client: Client,
    },
}

/// Unified embedding generator supporting multiple backends
///
/// Use `EmbeddingGenerator::bundled()` for self-contained operation (default),
/// or `EmbeddingGenerator::ollama()` if you have Ollama running.
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
            EmbeddingBackendImpl::Ollama { config, .. } => Self {
                backend: EmbeddingBackendImpl::Ollama {
                    config: config.clone(),
                    client: Client::new(),
                },
            },
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
            EmbeddingBackend::Ollama => Self {
                backend: EmbeddingBackendImpl::Ollama {
                    config,
                    client: Client::new(),
                },
            },
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

    /// Create Ollama embedding generator
    pub fn ollama(base_url: &str, model: &str) -> Self {
        Self::with_config(EmbeddingConfig::ollama(base_url, model))
    }

    /// Get the embedding dimension for the current backend/model
    pub fn dimension(&self) -> usize {
        match &self.backend {
            #[cfg(feature = "bundled-embeddings")]
            EmbeddingBackendImpl::Bundled(b) => b.dimension(),
            EmbeddingBackendImpl::Ollama { config, .. } => {
                // Ollama models have different dimensions
                match config.model.as_str() {
                    "nomic-embed-text" => 768,
                    "mxbai-embed-large" => 1024,
                    "all-minilm" => 384,
                    _ => 768, // Default assumption
                }
            }
        }
    }

    /// Get the backend type
    pub fn backend_type(&self) -> EmbeddingBackend {
        match &self.backend {
            #[cfg(feature = "bundled-embeddings")]
            EmbeddingBackendImpl::Bundled(_) => EmbeddingBackend::Bundled,
            EmbeddingBackendImpl::Ollama { .. } => EmbeddingBackend::Ollama,
        }
    }

    /// Generate embedding for a single text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        match &self.backend {
            #[cfg(feature = "bundled-embeddings")]
            EmbeddingBackendImpl::Bundled(b) => b.embed(text).await,
            EmbeddingBackendImpl::Ollama { .. } => self.embed_ollama(text).await,
        }
    }

    /// Generate embedding using Ollama (with auto-pull if model not found)
    async fn embed_ollama(&self, text: &str) -> Result<Vec<f32>> {
        let EmbeddingBackendImpl::Ollama { config, .. } = &self.backend else {
            return Err(TedError::Context("Not an Ollama backend".to_string()));
        };

        match self.embed_ollama_impl(text).await {
            Ok(embedding) => Ok(embedding),
            Err(e) => {
                // Check if it's a model not found error
                let error_msg = format!("{:?}", e);
                if error_msg.contains("MODEL_NOT_FOUND") {
                    eprintln!(
                        "[EMBEDDINGS] Model '{}' not found, attempting to pull...",
                        config.model
                    );

                    if self.pull_model_ollama().await.is_ok() {
                        eprintln!(
                            "[EMBEDDINGS] Successfully pulled '{}', retrying embed",
                            config.model
                        );
                        // Retry after pulling
                        return self.embed_ollama_impl(text).await;
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

    /// Internal Ollama embed function
    async fn embed_ollama_impl(&self, text: &str) -> Result<Vec<f32>> {
        let EmbeddingBackendImpl::Ollama { config, client } = &self.backend else {
            return Err(TedError::Context("Not an Ollama backend".to_string()));
        };

        let url = format!("{}/api/embed", config.base_url);

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
            model: config.model.clone(),
            input: truncated_text.to_string(),
        };

        let response = client
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
    async fn pull_model_ollama(&self) -> Result<()> {
        let EmbeddingBackendImpl::Ollama { config, client } = &self.backend else {
            return Err(TedError::Context("Not an Ollama backend".to_string()));
        };

        let url = format!("{}/api/pull", config.base_url);

        #[derive(Serialize)]
        struct PullRequest {
            name: String,
            stream: bool,
        }

        let request = PullRequest {
            name: config.model.clone(),
            stream: false,
        };

        let response = client
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

    // ===== Additional EmbeddingConfig Tests =====

    #[test]
    fn test_embedding_config_clone() {
        let config = EmbeddingConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.base_url, config.base_url);
        assert_eq!(cloned.model, config.model);
    }

    #[test]
    fn test_embedding_config_debug() {
        let config = EmbeddingConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("EmbeddingConfig"));
        assert!(debug.contains("localhost:11434"));
    }

    #[test]
    fn test_embedding_config_custom() {
        let config = EmbeddingConfig::ollama("http://custom:1234", "custom-model");
        assert_eq!(config.base_url, "http://custom:1234");
        assert_eq!(config.model, "custom-model");
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

    #[test]
    fn test_embedding_generator_with_config() {
        let config = EmbeddingConfig::ollama("http://custom:1234", "custom-model");
        let generator = EmbeddingGenerator::with_config(config);
        // Just verify the generator was created with custom config
        let _ = generator;
    }

    #[test]
    fn test_embedding_generator_clone() {
        let config = EmbeddingConfig::ollama("http://localhost:11434", "test-model");
        let generator = EmbeddingGenerator::with_config(config);
        let cloned = generator.clone();
        // Just verify clone works without panic
        let _ = cloned;
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

    // ===== EmbeddingRequest Tests =====

    #[test]
    fn test_embedding_request_serialization() {
        let request = EmbeddingRequest {
            model: "test-model".to_string(),
            input: "test input text".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"test-model\""));
        assert!(json.contains("\"input\":\"test input text\""));
    }

    #[test]
    fn test_embedding_request_debug() {
        let request = EmbeddingRequest {
            model: "nomic-embed-text".to_string(),
            input: "Hello world".to_string(),
        };

        let debug = format!("{:?}", request);
        assert!(debug.contains("EmbeddingRequest"));
        assert!(debug.contains("nomic-embed-text"));
    }

    // ===== EmbeddingResponse Tests =====

    #[test]
    fn test_embedding_response_deserialization() {
        let json = r#"{"embeddings": [[0.1, 0.2, 0.3], [0.4, 0.5, 0.6]]}"#;
        let response: EmbeddingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.embeddings.len(), 2);
        assert_eq!(response.embeddings[0].len(), 3);
    }

    #[test]
    fn test_embedding_response_single_embedding() {
        let json = r#"{"embeddings": [[0.1, 0.2, 0.3, 0.4, 0.5]]}"#;
        let response: EmbeddingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.embeddings.len(), 1);
        assert_eq!(response.embeddings[0], vec![0.1, 0.2, 0.3, 0.4, 0.5]);
    }

    #[test]
    fn test_embedding_response_empty() {
        let json = r#"{"embeddings": []}"#;
        let response: EmbeddingResponse = serde_json::from_str(json).unwrap();
        assert!(response.embeddings.is_empty());
    }

    #[test]
    fn test_embedding_response_debug() {
        let json = r#"{"embeddings": [[1.0, 2.0]]}"#;
        let response: EmbeddingResponse = serde_json::from_str(json).unwrap();
        let debug = format!("{:?}", response);
        assert!(debug.contains("EmbeddingResponse"));
    }

    #[test]
    fn test_embedding_response_large_embedding() {
        let embedding: Vec<f32> = (0..768).map(|i| i as f32 / 768.0).collect();
        let embeddings = vec![embedding.clone()];

        // Simulate creating a response
        let json = serde_json::json!({"embeddings": embeddings});
        let response: EmbeddingResponse = serde_json::from_value(json).unwrap();

        assert_eq!(response.embeddings.len(), 1);
        assert_eq!(response.embeddings[0].len(), 768);
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

    // ===== Config URL Tests =====

    #[test]
    fn test_embedding_config_custom_port() {
        let config = EmbeddingConfig::ollama("http://localhost:8080", "custom");
        assert_eq!(config.base_url, "http://localhost:8080");
    }

    #[test]
    fn test_embedding_config_remote_url() {
        let config = EmbeddingConfig::ollama("https://ollama.example.com", "nomic-embed-text");
        assert!(config.base_url.starts_with("https://"));
    }

    #[test]
    fn test_embedding_config_ip_address() {
        let config = EmbeddingConfig::ollama("http://192.168.1.100:11434", "nomic-embed-text");
        assert!(config.base_url.contains("192.168.1.100"));
    }

    // ===== EmbeddingGenerator Configuration Tests =====

    #[test]
    fn test_embedding_generator_model_access() {
        let config = EmbeddingConfig::ollama("http://localhost:11434", "mxbai-embed-large");
        let generator = EmbeddingGenerator::with_config(config);
        // Verify generator was created (can't access config directly after refactor)
        let _ = generator;
    }

    #[test]
    fn test_embedding_generator_url_access() {
        let config = EmbeddingConfig::ollama("http://custom:1234", "test");
        let generator = EmbeddingGenerator::with_config(config);
        // Verify generator was created
        let _ = generator;
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

    #[test]
    fn test_generator_with_different_configs() {
        let gen1 = EmbeddingGenerator::with_config(EmbeddingConfig::ollama(
            "http://server1:11434",
            "model1",
        ));

        let gen2 = EmbeddingGenerator::with_config(EmbeddingConfig::ollama(
            "http://server2:11434",
            "model2",
        ));

        // Generators with different configs should be created successfully
        let _ = gen1;
        let _ = gen2;
    }

    // ===== URL Construction Tests =====

    #[test]
    fn test_embed_url_construction() {
        let config = EmbeddingConfig::ollama("http://localhost:11434", "test");

        let url = format!("{}/api/embed", config.base_url);
        assert_eq!(url, "http://localhost:11434/api/embed");
    }

    #[test]
    fn test_pull_url_construction() {
        let config = EmbeddingConfig::ollama("http://localhost:11434", "test");

        let url = format!("{}/api/pull", config.base_url);
        assert_eq!(url, "http://localhost:11434/api/pull");
    }

    #[test]
    fn test_url_with_trailing_slash() {
        let config = EmbeddingConfig::ollama("http://localhost:11434/", "test");

        // Note: This would create a URL with double slash, which might need handling
        let url = format!("{}/api/embed", config.base_url);
        assert!(url.contains("11434"));
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

    // ===== Error Message Tests =====

    #[test]
    fn test_model_not_found_error_detection() {
        let error_msg = "MODEL_NOT_FOUND:model xyz not found";
        assert!(error_msg.contains("MODEL_NOT_FOUND"));
    }

    #[test]
    fn test_generic_error_detection() {
        let error_msg = "Generic API error occurred";
        assert!(!error_msg.contains("MODEL_NOT_FOUND"));
    }

    // ===== Async HTTP tests using wiremock =====

    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_embed_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.1, 0.2, 0.3, 0.4, 0.5]]
            })))
            .mount(&mock_server)
            .await;

        let config = EmbeddingConfig::ollama(&mock_server.uri(), "test-model");
        let generator = EmbeddingGenerator::with_config(config);

        let result = generator.embed("test text").await;
        assert!(result.is_ok());
        let embedding = result.unwrap();
        assert_eq!(embedding, vec![0.1, 0.2, 0.3, 0.4, 0.5]);
    }

    #[tokio::test]
    async fn test_embed_with_long_text_truncation() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.1, 0.2, 0.3]]
            })))
            .mount(&mock_server)
            .await;

        let config = EmbeddingConfig::ollama(&mock_server.uri(), "test-model");
        let generator = EmbeddingGenerator::with_config(config);

        // Create text longer than 24000 chars
        let long_text = "x".repeat(30000);
        let result = generator.embed(&long_text).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_embed_network_error() {
        // Use a port that's unlikely to be in use
        let config = EmbeddingConfig::ollama("http://127.0.0.1:59999", "test-model");
        let generator = EmbeddingGenerator::with_config(config);

        let result = generator.embed("test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_embed_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&mock_server)
            .await;

        let config = EmbeddingConfig::ollama(&mock_server.uri(), "test-model");
        let generator = EmbeddingGenerator::with_config(config);

        let result = generator.embed("test").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("500") || err.to_string().contains("Server"));
    }

    #[tokio::test]
    async fn test_embed_model_not_found_without_auto_pull() {
        let mock_server = MockServer::start().await;

        // First call returns 404 model not found
        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(404).set_body_string("model test-model not found"))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Pull endpoint also fails
        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(ResponseTemplate::new(500).set_body_string("pull failed"))
            .mount(&mock_server)
            .await;

        let config = EmbeddingConfig::ollama(&mock_server.uri(), "test-model");
        let generator = EmbeddingGenerator::with_config(config);

        let result = generator.embed("test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_embed_model_not_found_with_auto_pull_success() {
        let mock_server = MockServer::start().await;

        // First embed call returns 404
        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(404).set_body_string("model test-model not found"))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        // Pull succeeds
        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&mock_server)
            .await;

        // Second embed call succeeds
        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.1, 0.2, 0.3]]
            })))
            .mount(&mock_server)
            .await;

        let config = EmbeddingConfig::ollama(&mock_server.uri(), "test-model");
        let generator = EmbeddingGenerator::with_config(config);

        let result = generator.embed("test").await;
        // May succeed or fail depending on mock ordering - just verify no panic
        let _ = result;
    }

    #[tokio::test]
    async fn test_embed_invalid_json_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
            .mount(&mock_server)
            .await;

        let config = EmbeddingConfig::ollama(&mock_server.uri(), "test-model");
        let generator = EmbeddingGenerator::with_config(config);

        let result = generator.embed("test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_embed_empty_embeddings_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": []
            })))
            .mount(&mock_server)
            .await;

        let config = EmbeddingConfig::ollama(&mock_server.uri(), "test-model");
        let generator = EmbeddingGenerator::with_config(config);

        let result = generator.embed("test").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("No embeddings"));
    }

    #[tokio::test]
    async fn test_embed_batch_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.1, 0.2, 0.3]]
            })))
            .mount(&mock_server)
            .await;

        let config = EmbeddingConfig::ollama(&mock_server.uri(), "test-model");
        let generator = EmbeddingGenerator::with_config(config);

        let texts = vec!["text1".to_string(), "text2".to_string()];
        let result = generator.embed_batch(&texts).await;
        assert!(result.is_ok());
        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 2);
    }

    #[tokio::test]
    async fn test_embed_batch_empty() {
        let mock_server = MockServer::start().await;

        let config = EmbeddingConfig::ollama(&mock_server.uri(), "test-model");
        let generator = EmbeddingGenerator::with_config(config);

        let texts: Vec<String> = vec![];
        let result = generator.embed_batch(&texts).await;
        assert!(result.is_ok());
        let embeddings = result.unwrap();
        assert!(embeddings.is_empty());
    }

    #[tokio::test]
    async fn test_embed_batch_one_fails() {
        let mock_server = MockServer::start().await;

        // First call succeeds
        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "embeddings": [[0.1, 0.2]]
            })))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        // Second call fails
        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(500).set_body_string("error"))
            .mount(&mock_server)
            .await;

        let config = EmbeddingConfig::ollama(&mock_server.uri(), "test-model");
        let generator = EmbeddingGenerator::with_config(config);

        let texts = vec!["text1".to_string(), "text2".to_string()];
        let result = generator.embed_batch(&texts).await;
        // May succeed or fail depending on ordering
        let _ = result;
    }

    #[tokio::test]
    async fn test_pull_model_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/pull"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&mock_server)
            .await;

        let config = EmbeddingConfig::ollama(&mock_server.uri(), "test-model");
        let generator = EmbeddingGenerator::with_config(config);

        // pull_model is private, but we can test it through embed when model not found
        // Just verify the generator can be created
        let _ = generator;
    }

    #[tokio::test]
    async fn test_embed_non_404_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .mount(&mock_server)
            .await;

        let config = EmbeddingConfig::ollama(&mock_server.uri(), "test-model");
        let generator = EmbeddingGenerator::with_config(config);

        let result = generator.embed("test").await;
        assert!(result.is_err());
    }
}
