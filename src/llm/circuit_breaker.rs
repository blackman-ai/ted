// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Circuit breaker pattern for LLM provider resilience

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation, requests allowed
    Closed,
    /// Too many failures, requests blocked
    Open,
    /// Testing if service recovered, limited requests allowed
    HalfOpen,
}

/// Circuit breaker for tracking provider health
pub struct CircuitBreaker {
    /// Consecutive failure count
    failure_count: AtomicU32,
    /// Timestamp when circuit opened (seconds since epoch)
    opened_at: AtomicU64,
    /// Maximum consecutive failures before opening
    max_failures: u32,
    /// Cooldown period in seconds before half-open
    cooldown_secs: u64,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(max_failures: u32, cooldown_secs: u64) -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            opened_at: AtomicU64::new(0),
            max_failures,
            cooldown_secs,
        }
    }

    /// Create with default settings (5 failures, 10 second cooldown)
    pub fn default() -> Self {
        Self::new(5, 10)
    }

    /// Get current circuit state
    pub fn state(&self) -> CircuitState {
        let failures = self.failure_count.load(Ordering::Relaxed);
        let opened_at = self.opened_at.load(Ordering::Relaxed);

        if failures < self.max_failures {
            return CircuitState::Closed;
        }

        // Circuit is open, check if cooldown expired
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now - opened_at >= self.cooldown_secs {
            CircuitState::HalfOpen
        } else {
            CircuitState::Open
        }
    }

    /// Check if request should be allowed
    pub fn allow_request(&self) -> bool {
        match self.state() {
            CircuitState::Closed => true,
            CircuitState::Open => false,
            CircuitState::HalfOpen => true, // Allow one test request
        }
    }

    /// Record a successful request
    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.opened_at.store(0, Ordering::Relaxed);
    }

    /// Record a failed request
    pub fn record_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;

        if failures >= self.max_failures {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            self.opened_at.store(now, Ordering::Relaxed);
            eprintln!("[CIRCUIT_BREAKER] Circuit opened after {} failures. Cooldown: {}s", failures, self.cooldown_secs);
        }
    }

    /// Get current failure count
    pub fn failure_count(&self) -> u32 {
        self.failure_count.load(Ordering::Relaxed)
    }

    /// Reset the circuit breaker
    pub fn reset(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.opened_at.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_circuit_breaker_initial_state() {
        let cb = CircuitBreaker::new(3, 5);
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
        assert_eq!(cb.failure_count(), 0);
    }

    #[test]
    fn test_circuit_breaker_record_success() {
        let cb = CircuitBreaker::new(3, 5);
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.failure_count(), 2);

        cb.record_success();
        assert_eq!(cb.failure_count(), 0);
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_opens_after_max_failures() {
        let cb = CircuitBreaker::new(3, 5);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_half_open_after_cooldown() {
        let cb = CircuitBreaker::new(2, 1); // 1 second cooldown for fast test

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        sleep(Duration::from_secs(2));
        assert_eq!(cb.state(), CircuitState::HalfOpen);
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_closes_on_half_open_success() {
        let cb = CircuitBreaker::new(2, 1);

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        sleep(Duration::from_secs(2));
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_reopens_on_half_open_failure() {
        let cb = CircuitBreaker::new(2, 1);

        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        sleep(Duration::from_secs(2));
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let cb = CircuitBreaker::new(3, 5);

        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);

        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert_eq!(cb.failure_count(), 0);
    }

    #[test]
    fn test_circuit_breaker_default() {
        let cb = CircuitBreaker::default();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_multiple_failures() {
        let cb = CircuitBreaker::new(5, 10);

        for i in 1..=4 {
            cb.record_failure();
            assert_eq!(cb.failure_count(), i);
            assert_eq!(cb.state(), CircuitState::Closed);
        }

        cb.record_failure();
        assert_eq!(cb.failure_count(), 5);
        assert_eq!(cb.state(), CircuitState::Open);
    }
}
