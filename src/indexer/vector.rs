// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Vector index for semantic search
//!
//! This module provides an in-memory vector index with cosine similarity search.
//! It stores embeddings for code chunks and supports efficient nearest neighbor queries.

use std::collections::HashMap;
use uuid::Uuid;

/// In-memory vector index for semantic search
///
/// This is a simple brute-force implementation suitable for small to medium codebases.
/// For larger codebases (>100k chunks), consider implementing HNSW or using an external
/// vector database.
#[derive(Debug, Clone)]
pub struct VectorIndex {
    /// Mapping from chunk ID to embedding vector
    vectors: HashMap<Uuid, Vec<f32>>,
    /// Expected embedding dimension
    dimension: usize,
}

impl VectorIndex {
    /// Create a new vector index with the specified embedding dimension
    pub fn new(dimension: usize) -> Self {
        Self {
            vectors: HashMap::new(),
            dimension,
        }
    }

    /// Get the embedding dimension
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Get the number of vectors in the index
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// Insert a vector into the index
    ///
    /// Returns the previous vector if the ID already existed.
    pub fn insert(&mut self, id: Uuid, vector: Vec<f32>) -> Option<Vec<f32>> {
        debug_assert_eq!(
            vector.len(),
            self.dimension,
            "Vector dimension mismatch: expected {}, got {}",
            self.dimension,
            vector.len()
        );
        self.vectors.insert(id, vector)
    }

    /// Remove a vector from the index
    pub fn remove(&mut self, id: &Uuid) -> Option<Vec<f32>> {
        self.vectors.remove(id)
    }

    /// Get a vector by ID
    pub fn get(&self, id: &Uuid) -> Option<&Vec<f32>> {
        self.vectors.get(id)
    }

    /// Check if a vector exists in the index
    pub fn contains(&self, id: &Uuid) -> bool {
        self.vectors.contains_key(id)
    }

    /// Search for the k nearest neighbors to a query vector
    ///
    /// Returns a vector of (id, similarity_score) pairs, sorted by descending similarity.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(Uuid, f32)> {
        if query.len() != self.dimension {
            tracing::warn!(
                "Query dimension mismatch: expected {}, got {}",
                self.dimension,
                query.len()
            );
            return vec![];
        }

        let mut scores: Vec<(Uuid, f32)> = self
            .vectors
            .iter()
            .map(|(id, vec)| (*id, cosine_similarity(query, vec)))
            .collect();

        // Sort by descending similarity
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top k
        scores.truncate(k);
        scores
    }

    /// Search with a minimum similarity threshold
    ///
    /// Returns results with similarity >= threshold, up to k results.
    pub fn search_with_threshold(
        &self,
        query: &[f32],
        k: usize,
        threshold: f32,
    ) -> Vec<(Uuid, f32)> {
        self.search(query, k)
            .into_iter()
            .filter(|(_, score)| *score >= threshold)
            .collect()
    }

    /// Get all IDs in the index
    pub fn ids(&self) -> impl Iterator<Item = &Uuid> {
        self.vectors.keys()
    }

    /// Clear all vectors from the index
    pub fn clear(&mut self) {
        self.vectors.clear();
    }

    /// Batch insert multiple vectors
    pub fn insert_batch(&mut self, vectors: impl IntoIterator<Item = (Uuid, Vec<f32>)>) {
        for (id, vec) in vectors {
            self.insert(id, vec);
        }
    }

    /// Get memory usage estimate in bytes
    pub fn memory_usage(&self) -> usize {
        // Approximate: HashMap overhead + (id size + vector size) per entry
        let per_entry = std::mem::size_of::<Uuid>() + self.dimension * std::mem::size_of::<f32>();
        std::mem::size_of::<HashMap<Uuid, Vec<f32>>>() + self.vectors.len() * per_entry
    }
}

/// Calculate cosine similarity between two vectors
///
/// Returns a value in [-1, 1] where:
/// - 1.0 means identical direction
/// - 0.0 means orthogonal (unrelated)
/// - -1.0 means opposite direction
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

/// Result of a hybrid search combining semantic and keyword search
#[derive(Debug, Clone)]
pub struct HybridSearchResult {
    /// Chunk ID
    pub id: Uuid,
    /// Combined score (higher is better)
    pub score: f32,
    /// Semantic similarity score (0-1)
    pub semantic_score: f32,
    /// Keyword match score (0-1)
    pub keyword_score: f32,
}

/// Combine semantic and keyword search results using Reciprocal Rank Fusion (RRF)
///
/// RRF is a robust method for combining ranked lists that doesn't require
/// score normalization and handles different score distributions well.
pub fn reciprocal_rank_fusion(
    semantic_results: Vec<(Uuid, f32)>,
    keyword_results: Vec<(Uuid, f32)>,
    k_constant: f32,
) -> Vec<HybridSearchResult> {
    let mut scores: HashMap<Uuid, (f32, f32, f32)> = HashMap::new();

    // Process semantic results
    for (rank, (id, sim_score)) in semantic_results.iter().enumerate() {
        let rrf_score = 1.0 / (k_constant + rank as f32);
        let entry = scores.entry(*id).or_insert((0.0, 0.0, 0.0));
        entry.0 += rrf_score;
        entry.1 = *sim_score;
    }

    // Process keyword results
    for (rank, (id, kw_score)) in keyword_results.iter().enumerate() {
        let rrf_score = 1.0 / (k_constant + rank as f32);
        let entry = scores.entry(*id).or_insert((0.0, 0.0, 0.0));
        entry.0 += rrf_score;
        entry.2 = *kw_score;
    }

    // Convert to results
    let mut results: Vec<HybridSearchResult> = scores
        .into_iter()
        .map(|(id, (score, semantic, keyword))| HybridSearchResult {
            id,
            score,
            semantic_score: semantic,
            keyword_score: keyword,
        })
        .collect();

    // Sort by combined score descending
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_index_new() {
        let index = VectorIndex::new(384);
        assert_eq!(index.dimension(), 384);
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_vector_index_insert_and_get() {
        let mut index = VectorIndex::new(3);
        let id = Uuid::new_v4();
        let vec = vec![1.0, 2.0, 3.0];

        let prev = index.insert(id, vec.clone());
        assert!(prev.is_none());
        assert_eq!(index.len(), 1);
        assert!(index.contains(&id));

        let retrieved = index.get(&id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), &vec);
    }

    #[test]
    fn test_vector_index_insert_replaces() {
        let mut index = VectorIndex::new(3);
        let id = Uuid::new_v4();

        index.insert(id, vec![1.0, 2.0, 3.0]);
        let prev = index.insert(id, vec![4.0, 5.0, 6.0]);

        assert!(prev.is_some());
        assert_eq!(prev.unwrap(), vec![1.0, 2.0, 3.0]);
        assert_eq!(index.get(&id), Some(&vec![4.0, 5.0, 6.0]));
    }

    #[test]
    fn test_vector_index_remove() {
        let mut index = VectorIndex::new(3);
        let id = Uuid::new_v4();

        index.insert(id, vec![1.0, 2.0, 3.0]);
        let removed = index.remove(&id);

        assert!(removed.is_some());
        assert!(index.is_empty());
        assert!(!index.contains(&id));
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![-1.0, -2.0, -3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_vector_index_search() {
        let mut index = VectorIndex::new(3);

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        // Insert vectors with varying similarity to query
        index.insert(id1, vec![1.0, 0.0, 0.0]); // Most similar to query
        index.insert(id2, vec![0.7, 0.7, 0.0]); // Medium similarity
        index.insert(id3, vec![0.0, 0.0, 1.0]); // Least similar

        let query = vec![1.0, 0.0, 0.0];
        let results = index.search(&query, 3);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, id1); // Most similar first
        assert!((results[0].1 - 1.0).abs() < 0.0001); // Similarity should be ~1.0
    }

    #[test]
    fn test_vector_index_search_with_threshold() {
        let mut index = VectorIndex::new(3);

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        index.insert(id1, vec![1.0, 0.0, 0.0]); // Similarity 1.0
        index.insert(id2, vec![0.0, 1.0, 0.0]); // Similarity 0.0

        let query = vec![1.0, 0.0, 0.0];
        let results = index.search_with_threshold(&query, 10, 0.5);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, id1);
    }

    #[test]
    fn test_vector_index_search_empty() {
        let index = VectorIndex::new(3);
        let query = vec![1.0, 0.0, 0.0];
        let results = index.search(&query, 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_vector_index_search_wrong_dimension() {
        let mut index = VectorIndex::new(3);
        index.insert(Uuid::new_v4(), vec![1.0, 0.0, 0.0]);

        let query = vec![1.0, 0.0]; // Wrong dimension
        let results = index.search(&query, 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_vector_index_clear() {
        let mut index = VectorIndex::new(3);
        index.insert(Uuid::new_v4(), vec![1.0, 0.0, 0.0]);
        index.insert(Uuid::new_v4(), vec![0.0, 1.0, 0.0]);

        assert_eq!(index.len(), 2);
        index.clear();
        assert!(index.is_empty());
    }

    #[test]
    fn test_vector_index_batch_insert() {
        let mut index = VectorIndex::new(3);

        let vectors: Vec<(Uuid, Vec<f32>)> = vec![
            (Uuid::new_v4(), vec![1.0, 0.0, 0.0]),
            (Uuid::new_v4(), vec![0.0, 1.0, 0.0]),
            (Uuid::new_v4(), vec![0.0, 0.0, 1.0]),
        ];

        index.insert_batch(vectors);
        assert_eq!(index.len(), 3);
    }

    #[test]
    fn test_reciprocal_rank_fusion() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        // id1 ranked high in both
        // id2 ranked high in semantic only
        // id3 ranked high in keyword only
        let semantic = vec![(id1, 0.9), (id2, 0.8), (id3, 0.3)];
        let keyword = vec![(id1, 0.9), (id3, 0.7), (id2, 0.2)];

        let results = reciprocal_rank_fusion(semantic, keyword, 60.0);

        // id1 should be first (high in both)
        assert_eq!(results[0].id, id1);
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn test_memory_usage() {
        let mut index = VectorIndex::new(384);

        // Empty index should have minimal memory
        let empty_usage = index.memory_usage();

        // Add some vectors
        for _ in 0..100 {
            index.insert(Uuid::new_v4(), vec![0.0; 384]);
        }

        let filled_usage = index.memory_usage();
        assert!(filled_usage > empty_usage);
    }

    #[test]
    fn test_cosine_similarity_zero_vectors() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_scaled() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![2.0, 4.0, 6.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.0001); // Scaled vectors should have similarity 1.0
    }

    #[test]
    fn test_vector_index_ids() {
        let mut index = VectorIndex::new(3);
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        index.insert(id1, vec![1.0, 0.0, 0.0]);
        index.insert(id2, vec![0.0, 1.0, 0.0]);

        let ids: Vec<&Uuid> = index.ids().collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&&id1));
        assert!(ids.contains(&&id2));
    }
}
