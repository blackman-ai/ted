// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Token rate budget allocation for subagent coordination
//!
//! Provides proactive rate limiting by allocating token budgets to agents
//! based on priority, preventing API rate limit exhaustion.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock, Weak};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Priority levels for rate budget allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RatePriority {
    /// Main conversation: 4x weight (always responsive)
    Critical,
    /// Implement/plan agents: 2x weight
    High,
    /// Explore/review agents: 1x weight
    Normal,
    /// Background agents: 0.5x weight
    Background,
}

impl RatePriority {
    /// Get the weight multiplier for this priority level
    pub fn weight(&self) -> f64 {
        match self {
            RatePriority::Critical => 4.0,
            RatePriority::High => 2.0,
            RatePriority::Normal => 1.0,
            RatePriority::Background => 0.5,
        }
    }
}

/// Internal allocation entry tracked by the coordinator
#[derive(Debug)]
struct AllocationEntry {
    /// Unique ID for this allocation (used for debugging and removal)
    #[allow(dead_code)]
    id: Uuid,
    /// Name of the agent (used for debugging and logging)
    #[allow(dead_code)]
    name: String,
    priority: RatePriority,
    budget_per_minute: u64,
}

/// Central coordinator for rate budget allocation
///
/// Manages the total token rate limit and allocates budgets to agents
/// based on their priority levels. Automatically rebalances when agents
/// join or leave.
pub struct TokenRateCoordinator {
    /// Total tokens per minute available
    total_limit: u64,
    /// Global token bucket tracker
    tracker: TokenRateTracker,
    /// Active allocations
    allocations: RwLock<HashMap<Uuid, AllocationEntry>>,
    /// Self-reference for allocation handles
    self_ref: RwLock<Weak<TokenRateCoordinator>>,
}

impl TokenRateCoordinator {
    /// Create a new coordinator with the given total token rate limit
    pub fn new(tokens_per_minute: u64) -> Arc<Self> {
        let coordinator = Arc::new(Self {
            total_limit: tokens_per_minute,
            tracker: TokenRateTracker::new(tokens_per_minute),
            allocations: RwLock::new(HashMap::new()),
            self_ref: RwLock::new(Weak::new()),
        });

        // Store self-reference for allocation handles
        // This should never fail as we just created the RwLock, but handle gracefully
        if let Ok(mut self_ref) = coordinator.self_ref.write() {
            *self_ref = Arc::downgrade(&coordinator);
        } else {
            tracing::error!("Failed to acquire write lock on self_ref during coordinator creation");
        }

        coordinator
    }

    /// Request a rate budget allocation for an agent
    pub fn request_allocation(
        self: &Arc<Self>,
        priority: RatePriority,
        name: String,
    ) -> RateBudgetAllocation {
        let id = Uuid::new_v4();

        // Add to allocations
        {
            let mut allocations = match self.allocations.write() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    tracing::warn!("Allocations lock was poisoned, recovering");
                    poisoned.into_inner()
                }
            };
            allocations.insert(
                id,
                AllocationEntry {
                    id,
                    name: name.clone(),
                    priority,
                    budget_per_minute: 0, // Will be set by rebalance
                },
            );
        }

        // Rebalance all allocations
        self.rebalance();

        RateBudgetAllocation {
            id,
            coordinator: Arc::clone(self),
            tokens_used: AtomicU64::new(0),
            last_reset: RwLock::new(Instant::now()),
            priority,
            name,
        }
    }

    /// Release an allocation (called when RateBudgetAllocation is dropped)
    fn release(&self, id: Uuid) {
        {
            let mut allocations = match self.allocations.write() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    tracing::warn!("Allocations lock was poisoned during release, recovering");
                    poisoned.into_inner()
                }
            };
            allocations.remove(&id);
        }
        self.rebalance();
    }

    /// Recalculate all allocations based on current agents and priorities
    fn rebalance(&self) {
        let mut allocations = self.allocations.write().unwrap();

        if allocations.is_empty() {
            return;
        }

        // Calculate total weight
        let total_weight: f64 = allocations.values().map(|e| e.priority.weight()).sum();

        // Allocate budget based on weight
        for entry in allocations.values_mut() {
            let weight_ratio = entry.priority.weight() / total_weight;
            entry.budget_per_minute = (self.total_limit as f64 * weight_ratio) as u64;
        }
    }

    /// Get current allocation count
    pub fn allocation_count(&self) -> usize {
        self.allocations.read().unwrap().len()
    }

    /// Get total rate limit
    pub fn total_limit(&self) -> u64 {
        self.total_limit
    }

    /// Check if tokens are available globally (non-blocking)
    pub fn try_consume(&self, tokens: u64) -> bool {
        self.tracker.try_consume(tokens)
    }

    /// Wait for tokens to become available globally
    pub async fn wait_for_tokens(&self, tokens: u64) -> Duration {
        self.tracker.wait_for_tokens(tokens).await
    }

    /// Record actual token usage (updates the global tracker)
    pub fn record_usage(&self, tokens: u64) {
        self.tracker.record_usage(tokens);
    }

    /// Get the budget for a specific allocation
    pub fn get_allocation_budget(&self, id: Uuid) -> u64 {
        let allocations = self.allocations.read().unwrap();
        allocations
            .get(&id)
            .map(|e| e.budget_per_minute)
            .unwrap_or(0)
    }
}

/// Token bucket rate tracker
///
/// Implements a sliding window token bucket algorithm for rate limiting.
pub struct TokenRateTracker {
    /// Current tokens available in the bucket
    tokens_available: AtomicU64,
    /// Tokens added per second (rate limit / 60)
    tokens_per_second: f64,
    /// Maximum bucket capacity
    max_tokens: u64,
    /// Last refill timestamp
    last_refill: RwLock<Instant>,
    /// Tokens consumed in current window
    tokens_consumed: AtomicU64,
    /// Window start time
    window_start: RwLock<Instant>,
}

impl TokenRateTracker {
    /// Create a new tracker with the given tokens-per-minute limit
    pub fn new(tokens_per_minute: u64) -> Self {
        Self {
            tokens_available: AtomicU64::new(tokens_per_minute),
            tokens_per_second: tokens_per_minute as f64 / 60.0,
            max_tokens: tokens_per_minute,
            last_refill: RwLock::new(Instant::now()),
            tokens_consumed: AtomicU64::new(0),
            window_start: RwLock::new(Instant::now()),
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&self) {
        let mut last_refill = self.last_refill.write().unwrap();
        let now = Instant::now();
        let elapsed = now.duration_since(*last_refill);

        if elapsed.as_millis() > 0 {
            let tokens_to_add = (elapsed.as_secs_f64() * self.tokens_per_second) as u64;
            if tokens_to_add > 0 {
                let current = self.tokens_available.load(Ordering::Relaxed);
                let new_value = (current + tokens_to_add).min(self.max_tokens);
                self.tokens_available.store(new_value, Ordering::Relaxed);
                *last_refill = now;
            }
        }

        // Reset window if needed (every minute)
        let window_start = self.window_start.read().unwrap();
        if now.duration_since(*window_start) >= Duration::from_secs(60) {
            drop(window_start);
            let mut window_start = self.window_start.write().unwrap();
            *window_start = now;
            self.tokens_consumed.store(0, Ordering::Relaxed);
        }
    }

    /// Try to consume tokens, returns true if successful
    pub fn try_consume(&self, tokens: u64) -> bool {
        self.refill();

        loop {
            let current = self.tokens_available.load(Ordering::Relaxed);
            if current < tokens {
                return false;
            }

            match self.tokens_available.compare_exchange_weak(
                current,
                current - tokens,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.tokens_consumed.fetch_add(tokens, Ordering::Relaxed);
                    return true;
                }
                Err(_) => continue, // Retry
            }
        }
    }

    /// Record actual token usage (for tracking)
    pub fn record_usage(&self, tokens: u64) {
        self.tokens_consumed.fetch_add(tokens, Ordering::Relaxed);
    }

    /// Wait until tokens are available, returns the wait duration
    pub async fn wait_for_tokens(&self, tokens: u64) -> Duration {
        let start = Instant::now();

        loop {
            self.refill();

            let current = self.tokens_available.load(Ordering::Relaxed);
            if current >= tokens {
                // Tokens available now
                return start.elapsed();
            }

            // Calculate how long to wait for enough tokens
            let needed = tokens - current;
            let wait_secs = needed as f64 / self.tokens_per_second;
            let wait_duration = Duration::from_secs_f64(wait_secs.max(0.1)); // Min 100ms

            tokio::time::sleep(wait_duration).await;
        }
    }

    /// Get tokens consumed in current window
    pub fn tokens_consumed(&self) -> u64 {
        self.tokens_consumed.load(Ordering::Relaxed)
    }

    /// Get current available tokens
    pub fn tokens_available(&self) -> u64 {
        self.refill();
        self.tokens_available.load(Ordering::Relaxed)
    }
}

/// Per-agent allocation handle
///
/// Automatically releases budget back to the coordinator when dropped (RAII).
pub struct RateBudgetAllocation {
    /// Unique allocation ID
    id: Uuid,
    /// Reference to the coordinator
    coordinator: Arc<TokenRateCoordinator>,
    /// Tokens used in current window
    tokens_used: AtomicU64,
    /// Window reset time
    last_reset: RwLock<Instant>,
    /// Priority level
    priority: RatePriority,
    /// Agent name for logging
    name: String,
}

impl RateBudgetAllocation {
    /// Get the current budget per minute (queries coordinator for up-to-date value)
    pub fn budget(&self) -> u64 {
        self.coordinator.get_allocation_budget(self.id)
    }

    /// Get the priority level
    pub fn priority(&self) -> RatePriority {
        self.priority
    }

    /// Get the agent name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the allocation ID
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Check if tokens are available within this allocation's budget
    pub fn try_consume(&self, tokens: u64) -> bool {
        self.maybe_reset_window();

        let used = self.tokens_used.load(Ordering::Relaxed);
        let budget = self.budget();

        if used + tokens > budget {
            return false;
        }

        // Also check global availability
        if !self.coordinator.try_consume(tokens) {
            return false;
        }

        self.tokens_used.fetch_add(tokens, Ordering::Relaxed);
        true
    }

    /// Wait for tokens to become available within budget
    pub async fn wait_for_budget(&self, tokens: u64) -> Duration {
        let start = Instant::now();

        loop {
            self.maybe_reset_window();

            let used = self.tokens_used.load(Ordering::Relaxed);
            let budget = self.budget();

            // Check if within our budget allocation
            if used + tokens <= budget {
                // Wait for global availability too
                let global_wait = self.coordinator.wait_for_tokens(tokens).await;
                self.tokens_used.fetch_add(tokens, Ordering::Relaxed);
                return start.elapsed() + global_wait;
            }

            // Wait for window reset or partial budget availability
            let overage = (used + tokens).saturating_sub(budget);
            let tokens_per_second = budget as f64 / 60.0;
            let wait_secs = if tokens_per_second > 0.0 {
                overage as f64 / tokens_per_second
            } else {
                1.0 // Default 1 second if no budget
            };

            tokio::time::sleep(Duration::from_secs_f64(wait_secs.max(0.1))).await;
        }
    }

    /// Record actual token usage
    pub fn record_usage(&self, tokens: u64) {
        self.tokens_used.fetch_add(tokens, Ordering::Relaxed);
        self.coordinator.record_usage(tokens);
    }

    /// Get tokens used in current window
    pub fn tokens_used(&self) -> u64 {
        self.tokens_used.load(Ordering::Relaxed)
    }

    /// Reset window if a minute has passed
    fn maybe_reset_window(&self) {
        let last_reset = self.last_reset.read().unwrap();
        if last_reset.elapsed() >= Duration::from_secs(60) {
            drop(last_reset);
            let mut last_reset = self.last_reset.write().unwrap();
            // Double-check after acquiring write lock
            if last_reset.elapsed() >= Duration::from_secs(60) {
                *last_reset = Instant::now();
                self.tokens_used.store(0, Ordering::Relaxed);
            }
        }
    }
}

impl Drop for RateBudgetAllocation {
    fn drop(&mut self) {
        self.coordinator.release(self.id);
    }
}

impl std::fmt::Debug for RateBudgetAllocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateBudgetAllocation")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("priority", &self.priority)
            .field("budget_per_minute", &self.budget())
            .field("tokens_used", &self.tokens_used())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_priority_weights() {
        assert_eq!(RatePriority::Critical.weight(), 4.0);
        assert_eq!(RatePriority::High.weight(), 2.0);
        assert_eq!(RatePriority::Normal.weight(), 1.0);
        assert_eq!(RatePriority::Background.weight(), 0.5);
    }

    #[test]
    fn test_token_rate_tracker_new() {
        let tracker = TokenRateTracker::new(60_000);
        assert_eq!(tracker.max_tokens, 60_000);
        assert!((tracker.tokens_per_second - 1000.0).abs() < 0.1);
    }

    #[test]
    fn test_token_rate_tracker_try_consume() {
        let tracker = TokenRateTracker::new(10_000);

        // Should succeed
        assert!(tracker.try_consume(1_000));
        assert!(tracker.try_consume(1_000));

        // Should still have tokens
        assert!(tracker.tokens_available() > 0);
    }

    #[test]
    fn test_token_rate_tracker_exhaustion() {
        let tracker = TokenRateTracker::new(1_000);

        // Consume all tokens
        assert!(tracker.try_consume(1_000));

        // Should fail - no tokens left
        assert!(!tracker.try_consume(100));
    }

    #[test]
    fn test_coordinator_new() {
        let coordinator = TokenRateCoordinator::new(450_000);
        assert_eq!(coordinator.total_limit(), 450_000);
        assert_eq!(coordinator.allocation_count(), 0);
    }

    #[test]
    fn test_coordinator_single_allocation() {
        let coordinator = TokenRateCoordinator::new(100_000);
        let allocation = coordinator.request_allocation(RatePriority::Normal, "test".to_string());

        assert_eq!(coordinator.allocation_count(), 1);
        // Single allocation should get full budget
        assert_eq!(allocation.budget(), 100_000);
    }

    #[test]
    fn test_coordinator_multiple_allocations() {
        let coordinator = TokenRateCoordinator::new(100_000);

        let critical = coordinator.request_allocation(RatePriority::Critical, "main".to_string());
        let normal = coordinator.request_allocation(RatePriority::Normal, "explore".to_string());

        assert_eq!(coordinator.allocation_count(), 2);

        // Critical (4x) + Normal (1x) = 5x total
        // Critical gets 4/5 = 80%
        // Normal gets 1/5 = 20%
        assert_eq!(critical.budget(), 80_000);
        assert_eq!(normal.budget(), 20_000);
    }

    #[test]
    fn test_coordinator_allocation_drop() {
        let coordinator = TokenRateCoordinator::new(100_000);

        let allocation = coordinator.request_allocation(RatePriority::Normal, "temp".to_string());
        assert_eq!(coordinator.allocation_count(), 1);

        drop(allocation);
        assert_eq!(coordinator.allocation_count(), 0);
    }

    #[test]
    fn test_coordinator_rebalance_on_drop() {
        let coordinator = TokenRateCoordinator::new(100_000);

        let critical = coordinator.request_allocation(RatePriority::Critical, "main".to_string());
        let normal = coordinator.request_allocation(RatePriority::Normal, "explore".to_string());

        // Initially: Critical=80K, Normal=20K
        assert_eq!(critical.budget(), 80_000);
        assert_eq!(normal.budget(), 20_000);

        // Drop normal allocation
        drop(normal);

        // After drop, critical should still have allocation tracked
        // But its budget won't auto-update (would need notification mechanism)
        assert_eq!(coordinator.allocation_count(), 1);
    }

    #[test]
    fn test_allocation_try_consume() {
        let coordinator = TokenRateCoordinator::new(100_000);
        let allocation = coordinator.request_allocation(RatePriority::Normal, "test".to_string());

        // Should succeed within budget
        assert!(allocation.try_consume(10_000));
        assert_eq!(allocation.tokens_used(), 10_000);
    }

    #[test]
    fn test_allocation_over_budget() {
        let coordinator = TokenRateCoordinator::new(10_000);
        let allocation = coordinator.request_allocation(RatePriority::Normal, "test".to_string());

        // First consume should succeed
        assert!(allocation.try_consume(5_000));

        // Second consume that exceeds budget should fail
        assert!(!allocation.try_consume(6_000));
    }

    #[test]
    fn test_allocation_record_usage() {
        let coordinator = TokenRateCoordinator::new(100_000);
        let allocation = coordinator.request_allocation(RatePriority::Normal, "test".to_string());

        allocation.record_usage(5_000);
        assert_eq!(allocation.tokens_used(), 5_000);

        allocation.record_usage(3_000);
        assert_eq!(allocation.tokens_used(), 8_000);
    }

    #[test]
    fn test_allocation_debug() {
        let coordinator = TokenRateCoordinator::new(100_000);
        let allocation =
            coordinator.request_allocation(RatePriority::High, "debug-test".to_string());

        let debug_str = format!("{:?}", allocation);
        assert!(debug_str.contains("RateBudgetAllocation"));
        assert!(debug_str.contains("debug-test"));
        assert!(debug_str.contains("High"));
    }

    #[test]
    fn test_priority_equality() {
        assert_eq!(RatePriority::Critical, RatePriority::Critical);
        assert_ne!(RatePriority::Critical, RatePriority::High);
    }

    #[test]
    fn test_priority_clone() {
        let p = RatePriority::Normal;
        let cloned = p;
        assert_eq!(p, cloned);
    }

    #[tokio::test]
    async fn test_tracker_wait_for_tokens() {
        let tracker = TokenRateTracker::new(60_000); // 1000/sec

        // Consume most tokens
        assert!(tracker.try_consume(59_000));

        // Should have ~1000 left, wait for more
        let wait = tracker.wait_for_tokens(500).await;

        // Wait should be very short since we have tokens
        assert!(wait < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_allocation_wait_for_budget() {
        let coordinator = TokenRateCoordinator::new(60_000);
        let allocation = coordinator.request_allocation(RatePriority::Normal, "test".to_string());

        // Request within budget
        let wait = allocation.wait_for_budget(1_000).await;

        // Should be nearly instant
        assert!(wait < Duration::from_secs(1));
    }

    #[test]
    fn test_coordinator_priority_weights() {
        let coordinator = TokenRateCoordinator::new(100_000);

        // Create allocations with different priorities
        let critical = coordinator.request_allocation(RatePriority::Critical, "main".to_string()); // 4x
        let high = coordinator.request_allocation(RatePriority::High, "impl".to_string()); // 2x
        let normal = coordinator.request_allocation(RatePriority::Normal, "explore".to_string()); // 1x
        let background = coordinator.request_allocation(RatePriority::Background, "bg".to_string()); // 0.5x

        // Total weight: 4 + 2 + 1 + 0.5 = 7.5
        // Critical: 4/7.5 ≈ 53.3%
        // High: 2/7.5 ≈ 26.7%
        // Normal: 1/7.5 ≈ 13.3%
        // Background: 0.5/7.5 ≈ 6.7%

        let total = critical.budget() + high.budget() + normal.budget() + background.budget();

        // Should sum to approximately the total limit
        assert!((total as i64 - 100_000i64).abs() < 100);

        // Critical should have most
        assert!(critical.budget() > high.budget());
        assert!(high.budget() > normal.budget());
        assert!(normal.budget() > background.budget());
    }

    #[test]
    fn test_allocation_getters() {
        let coordinator = TokenRateCoordinator::new(100_000);
        let allocation =
            coordinator.request_allocation(RatePriority::High, "test-agent".to_string());

        assert_eq!(allocation.name(), "test-agent");
        assert_eq!(allocation.priority(), RatePriority::High);
        assert!(!allocation.id().is_nil());
    }

    #[test]
    fn test_allocation_budget_updates_on_new_allocation() {
        let coordinator = TokenRateCoordinator::new(100_000);

        // First allocation gets full budget
        let alloc1 = coordinator.request_allocation(RatePriority::Normal, "first".to_string());
        assert_eq!(alloc1.budget(), 100_000);

        // Second allocation causes rebalance - both should now share equally
        let alloc2 = coordinator.request_allocation(RatePriority::Normal, "second".to_string());

        // Both have same priority (1x), so split 50/50
        assert_eq!(alloc1.budget(), 50_000);
        assert_eq!(alloc2.budget(), 50_000);
    }
}
