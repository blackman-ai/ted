// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! LLM provider implementations

pub mod anthropic;
pub mod blackman;
pub mod local;
pub mod openrouter;

pub use anthropic::AnthropicProvider;
pub use blackman::BlackmanProvider;
pub use local::LocalProvider;
pub use openrouter::OpenRouterProvider;
