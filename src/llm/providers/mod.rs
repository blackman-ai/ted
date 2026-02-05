// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! LLM provider implementations

pub mod anthropic;
pub mod blackman;
pub mod ollama;
pub mod openrouter;

#[cfg(feature = "local-llm")]
pub mod llama_cpp;

pub use anthropic::AnthropicProvider;
pub use blackman::BlackmanProvider;
pub use ollama::OllamaProvider;
pub use openrouter::OpenRouterProvider;

#[cfg(feature = "local-llm")]
pub use llama_cpp::{LlamaCppConfig, LlamaCppProvider};
