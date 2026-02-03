// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! LLM module for Ted
//!
//! Provides abstraction over different LLM providers.

pub mod circuit_breaker;
pub mod factory;
pub mod message;
pub mod provider;
pub mod providers;
pub mod rate_budget;
pub mod retry;

#[cfg(test)]
pub mod mock_provider;

pub use circuit_breaker::*;
pub use factory::ProviderFactory;
pub use message::*;
pub use provider::*;
pub use rate_budget::*;
pub use retry::*;

#[cfg(test)]
pub use mock_provider::MockProvider;
