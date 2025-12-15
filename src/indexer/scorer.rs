// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Score calculation for context prioritization.
//!
//! Implements the memory-based scoring formula:
//! ```text
//! retention_score = (recency * 0.4) + (frequency * 0.3) + (centrality * 0.3)
//! decay_rate = base_decay * (1 + churn_factor * 0.2)
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::memory::{ChunkMemory, FileMemory};

/// Configuration for scoring weights and decay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConfig {
    /// Weight for recency factor (default: 0.4).
    pub recency_weight: f64,
    /// Weight for frequency factor (default: 0.3).
    pub frequency_weight: f64,
    /// Weight for centrality factor (default: 0.3).
    pub centrality_weight: f64,
    /// Churn decay modifier (default: 0.2).
    pub churn_decay_factor: f64,
    /// Half-life for recency decay in hours (default: 24).
    pub decay_half_life_hours: f64,
    /// Maximum frequency before normalization (default: 100).
    pub max_frequency: u32,
    /// Session boost multiplier (default: 0.5).
    pub session_boost_multiplier: f64,
    /// Associative boost for referenced chunks (default: 0.3).
    pub associative_boost: f64,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            recency_weight: 0.4,
            frequency_weight: 0.3,
            centrality_weight: 0.3,
            churn_decay_factor: 0.2,
            decay_half_life_hours: 24.0,
            max_frequency: 100,
            session_boost_multiplier: 0.5,
            associative_boost: 0.3,
        }
    }
}

impl ScoringConfig {
    /// Validate that weights sum to 1.0.
    pub fn validate(&self) -> Result<(), &'static str> {
        let sum = self.recency_weight + self.frequency_weight + self.centrality_weight;
        if (sum - 1.0).abs() > 0.001 {
            return Err("Scoring weights must sum to 1.0");
        }
        if self.decay_half_life_hours <= 0.0 {
            return Err("Decay half-life must be positive");
        }
        Ok(())
    }
}

/// Scorer for computing retention scores.
#[derive(Debug, Clone)]
pub struct Scorer {
    config: ScoringConfig,
}

impl Scorer {
    /// Create a new scorer with default configuration.
    pub fn new() -> Self {
        Self {
            config: ScoringConfig::default(),
        }
    }

    /// Create a scorer with custom configuration.
    pub fn with_config(config: ScoringConfig) -> Self {
        Self { config }
    }

    /// Get the current configuration.
    pub fn config(&self) -> &ScoringConfig {
        &self.config
    }

    /// Calculate the recency score (0.0 to 1.0).
    ///
    /// Uses exponential decay based on time since last access.
    /// Score of 1.0 means just accessed, decays to 0.5 after half-life.
    pub fn recency_score(&self, last_accessed: DateTime<Utc>) -> f64 {
        let now = Utc::now();
        let hours_since = (now - last_accessed).num_seconds() as f64 / 3600.0;

        if hours_since <= 0.0 {
            return 1.0;
        }

        // Exponential decay: score = 0.5^(hours/half_life)
        let decay_factor = hours_since / self.config.decay_half_life_hours;
        0.5_f64.powf(decay_factor)
    }

    /// Calculate the frequency score (0.0 to 1.0).
    ///
    /// Normalized access count with logarithmic scaling.
    pub fn frequency_score(&self, access_count: u32) -> f64 {
        if access_count == 0 {
            return 0.0;
        }

        // Logarithmic scaling: log(1 + count) / log(1 + max)
        let count_log = (1.0 + access_count as f64).ln();
        let max_log = (1.0 + self.config.max_frequency as f64).ln();

        (count_log / max_log).min(1.0)
    }

    /// Calculate the centrality score (0.0 to 1.0).
    ///
    /// Already normalized from PageRank-style calculation.
    pub fn centrality_score(&self, centrality: f64) -> f64 {
        centrality.clamp(0.0, 1.0)
    }

    /// Calculate decay rate modifier based on churn.
    ///
    /// High-churn files decay faster (they're more volatile).
    pub fn churn_decay_modifier(&self, churn_rate: f64) -> f64 {
        1.0 + churn_rate.min(1.0) * self.config.churn_decay_factor
    }

    /// Calculate the full retention score for a file.
    pub fn file_retention_score(&self, file: &FileMemory) -> f64 {
        let recency = self.recency_score(file.last_accessed);
        let frequency = self.frequency_score(file.access_count);
        let centrality = self.centrality_score(file.centrality_score);

        let base_score = (recency * self.config.recency_weight)
            + (frequency * self.config.frequency_weight)
            + (centrality * self.config.centrality_weight);

        // Apply churn decay
        let decay_modifier = self.churn_decay_modifier(file.churn_rate);
        base_score / decay_modifier
    }

    /// Calculate the full retention score for a chunk.
    pub fn chunk_retention_score(&self, chunk: &ChunkMemory) -> f64 {
        let recency = self.recency_score(chunk.global_last_accessed);
        let frequency = self.frequency_score(chunk.global_access_count);
        let centrality = self.centrality_score(chunk.centrality_score);

        let global_score = (recency * self.config.recency_weight)
            + (frequency * self.config.frequency_weight)
            + (centrality * self.config.centrality_weight);

        // Apply churn decay
        let decay_modifier = self.churn_decay_modifier(chunk.churn_rate);
        let decayed_global = global_score / decay_modifier;

        // Add session boost
        let session_bonus = chunk.session_boost * self.config.session_boost_multiplier;

        (decayed_global + session_bonus).min(1.0)
    }

    /// Calculate boost for associatively referenced chunks.
    ///
    /// When a chunk is accessed, its references get a smaller boost.
    pub fn associative_boost(&self) -> f64 {
        self.config.associative_boost
    }

    /// Rank files by retention score (highest first).
    pub fn rank_files(&self, files: &mut [FileMemory]) {
        for file in files.iter_mut() {
            file.retention_score = self.file_retention_score(file);
        }
        files.sort_by(|a, b| {
            b.retention_score
                .partial_cmp(&a.retention_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Select top N files by retention score.
    pub fn select_top_files<'a>(&self, files: &'a [FileMemory], n: usize) -> Vec<&'a FileMemory> {
        let mut scored: Vec<_> = files
            .iter()
            .map(|f| (self.file_retention_score(f), f))
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        scored.into_iter().take(n).map(|(_, f)| f).collect()
    }

    /// Select files within a byte budget.
    pub fn select_within_budget<'a>(
        &self,
        files: &'a [FileMemory],
        max_bytes: u64,
    ) -> Vec<&'a FileMemory> {
        let mut scored: Vec<_> = files
            .iter()
            .map(|f| (self.file_retention_score(f), f))
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let mut selected = Vec::new();
        let mut total_bytes = 0u64;

        for (_, file) in scored {
            if total_bytes + file.byte_size <= max_bytes {
                total_bytes += file.byte_size;
                selected.push(file);
            }
        }

        selected
    }
}

impl Default for Scorer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_file(
        access_count: u32,
        hours_ago: i64,
        centrality: f64,
        churn: f64,
    ) -> FileMemory {
        let mut file = FileMemory::new(PathBuf::from("test.rs"));
        file.access_count = access_count;
        file.last_accessed = Utc::now() - chrono::Duration::hours(hours_ago);
        file.centrality_score = centrality;
        file.churn_rate = churn;
        file.byte_size = 1000;
        file
    }

    #[test]
    fn test_config_validation() {
        let config = ScoringConfig::default();
        assert!(config.validate().is_ok());

        let bad_weights = ScoringConfig {
            recency_weight: 0.5,
            frequency_weight: 0.5,
            centrality_weight: 0.5,
            ..Default::default()
        };
        assert!(bad_weights.validate().is_err());

        let bad_decay = ScoringConfig {
            decay_half_life_hours: -1.0,
            ..Default::default()
        };
        assert!(bad_decay.validate().is_err());
    }

    #[test]
    fn test_recency_score() {
        let scorer = Scorer::new();

        // Just accessed = 1.0
        let recent = Utc::now();
        assert!((scorer.recency_score(recent) - 1.0).abs() < 0.01);

        // 24 hours ago = 0.5 (half-life)
        let day_ago = Utc::now() - chrono::Duration::hours(24);
        assert!((scorer.recency_score(day_ago) - 0.5).abs() < 0.01);

        // 48 hours ago = 0.25
        let two_days = Utc::now() - chrono::Duration::hours(48);
        assert!((scorer.recency_score(two_days) - 0.25).abs() < 0.01);
    }

    #[test]
    fn test_frequency_score() {
        let scorer = Scorer::new();

        assert_eq!(scorer.frequency_score(0), 0.0);

        // Some access should give partial score
        let score_10 = scorer.frequency_score(10);
        assert!(score_10 > 0.0 && score_10 < 1.0);

        // More access = higher score
        let score_50 = scorer.frequency_score(50);
        assert!(score_50 > score_10);

        // Max frequency = 1.0
        let score_max = scorer.frequency_score(100);
        assert!((score_max - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_centrality_score_clamping() {
        let scorer = Scorer::new();

        assert_eq!(scorer.centrality_score(-0.5), 0.0);
        assert_eq!(scorer.centrality_score(0.5), 0.5);
        assert_eq!(scorer.centrality_score(1.5), 1.0);
    }

    #[test]
    fn test_churn_decay_modifier() {
        let scorer = Scorer::new();

        // No churn = no extra decay
        assert_eq!(scorer.churn_decay_modifier(0.0), 1.0);

        // Max churn = 1.2x decay
        assert!((scorer.churn_decay_modifier(1.0) - 1.2).abs() < 0.001);

        // Over 1.0 churn is clamped
        assert!((scorer.churn_decay_modifier(2.0) - 1.2).abs() < 0.001);
    }

    #[test]
    fn test_file_retention_score() {
        let scorer = Scorer::new();

        // Recent, frequently accessed, central file
        let hot_file = create_test_file(50, 1, 0.8, 0.1);
        let hot_score = scorer.file_retention_score(&hot_file);

        // Old, rarely accessed, peripheral file
        let cold_file = create_test_file(2, 72, 0.1, 0.5);
        let cold_score = scorer.file_retention_score(&cold_file);

        assert!(hot_score > cold_score);
    }

    #[test]
    fn test_chunk_retention_score_with_session() {
        let scorer = Scorer::new();

        let chunk_id = uuid::Uuid::new_v4();
        let mut chunk = ChunkMemory::new(chunk_id);
        chunk.global_access_count = 10;
        chunk.centrality_score = 0.5;

        let base_score = scorer.chunk_retention_score(&chunk);

        // Add session boost
        chunk.session_boost = 1.0;
        let boosted_score = scorer.chunk_retention_score(&chunk);

        assert!(boosted_score > base_score);
    }

    #[test]
    fn test_rank_files() {
        let scorer = Scorer::new();

        let mut files = vec![
            create_test_file(5, 48, 0.2, 0.3),  // Low score
            create_test_file(50, 1, 0.8, 0.1),  // High score
            create_test_file(20, 12, 0.5, 0.2), // Medium score
        ];

        scorer.rank_files(&mut files);

        // Should be sorted highest to lowest
        assert!(files[0].retention_score >= files[1].retention_score);
        assert!(files[1].retention_score >= files[2].retention_score);
    }

    #[test]
    fn test_select_top_files() {
        let scorer = Scorer::new();

        let files = vec![
            create_test_file(5, 48, 0.2, 0.3),
            create_test_file(50, 1, 0.8, 0.1),
            create_test_file(20, 12, 0.5, 0.2),
        ];

        let top = scorer.select_top_files(&files, 2);
        assert_eq!(top.len(), 2);

        // Verify highest scored are selected
        let top_score = scorer.file_retention_score(top[0]);
        let second_score = scorer.file_retention_score(top[1]);
        assert!(top_score >= second_score);
    }

    #[test]
    fn test_select_within_budget() {
        let scorer = Scorer::new();

        let mut files = vec![
            create_test_file(5, 48, 0.2, 0.3),
            create_test_file(50, 1, 0.8, 0.1),
            create_test_file(20, 12, 0.5, 0.2),
        ];

        // Each file is 1000 bytes
        files[0].byte_size = 1000;
        files[1].byte_size = 1000;
        files[2].byte_size = 1000;

        // Budget for 2 files
        let selected = scorer.select_within_budget(&files, 2000);
        assert_eq!(selected.len(), 2);

        // Budget for all
        let all = scorer.select_within_budget(&files, 10000);
        assert_eq!(all.len(), 3);

        // Budget for none
        let none = scorer.select_within_budget(&files, 500);
        assert!(none.is_empty());
    }
}
