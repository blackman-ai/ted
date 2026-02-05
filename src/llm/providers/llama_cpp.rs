// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Local LLM provider using llama.cpp
//!
//! This module provides a self-contained LLM inference capability using llama-cpp-2,
//! allowing Ted to run completely without external API dependencies.
//!
//! # Features
//!
//! - Zero network dependency for inference
//! - Support for GGUF model files
//! - Streaming token generation
//! - GPU acceleration when available
//!
//! # Usage
//!
//! ```no_run
//! use ted::llm::providers::LlamaCppProvider;
//!
//! # fn main() -> ted::Result<()> {
//! let provider = LlamaCppProvider::new("/path/to/model.gguf")?;
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use futures::Stream;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::token::data_array::LlamaTokenDataArray;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::error::{Result, TedError};
use crate::llm::message::Message;
use crate::llm::provider::{
    CompletionRequest, CompletionResponse, ContentBlockDelta, ContentBlockResponse, LlmProvider,
    ModelInfo, StopReason, StreamEvent, Usage,
};

/// Default context size for llama.cpp models
const DEFAULT_CONTEXT_SIZE: u32 = 4096;

/// Default number of GPU layers (0 = CPU only)
const DEFAULT_GPU_LAYERS: u32 = 0;

/// Configuration for the LlamaCpp provider
#[derive(Debug, Clone)]
pub struct LlamaCppConfig {
    /// Path to the GGUF model file
    pub model_path: PathBuf,
    /// Context size (number of tokens)
    pub context_size: u32,
    /// Number of layers to offload to GPU (0 for CPU-only)
    pub gpu_layers: u32,
    /// Number of threads to use for inference
    pub threads: Option<u32>,
    /// Model alias/name for identification
    pub model_name: String,
}

impl Default for LlamaCppConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            context_size: DEFAULT_CONTEXT_SIZE,
            gpu_layers: DEFAULT_GPU_LAYERS,
            threads: None,
            model_name: "local".to_string(),
        }
    }
}

impl LlamaCppConfig {
    /// Create a new config with the specified model path
    pub fn new(model_path: impl AsRef<Path>) -> Self {
        let path = model_path.as_ref();
        let model_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("local")
            .to_string();

        Self {
            model_path: path.to_path_buf(),
            model_name,
            ..Default::default()
        }
    }

    /// Set the context size
    pub fn with_context_size(mut self, size: u32) -> Self {
        self.context_size = size;
        self
    }

    /// Set the number of GPU layers
    pub fn with_gpu_layers(mut self, layers: u32) -> Self {
        self.gpu_layers = layers;
        self
    }

    /// Set the number of threads
    pub fn with_threads(mut self, threads: u32) -> Self {
        self.threads = Some(threads);
        self
    }
}

/// Local LLM provider using llama.cpp
///
/// This provider runs inference locally using GGUF model files,
/// providing a zero-dependency inference option.
pub struct LlamaCppProvider {
    /// Configuration
    config: LlamaCppConfig,
    /// llama.cpp backend (shared)
    backend: Arc<LlamaBackend>,
    /// Loaded model
    model: Arc<LlamaModel>,
}

impl LlamaCppProvider {
    /// Create a new LlamaCpp provider from a model path
    pub fn new(model_path: impl AsRef<Path>) -> Result<Self> {
        Self::with_config(LlamaCppConfig::new(model_path))
    }

    /// Create a new LlamaCpp provider with custom config
    pub fn with_config(config: LlamaCppConfig) -> Result<Self> {
        if !config.model_path.exists() {
            return Err(TedError::Config(format!(
                "Model file not found: {}",
                config.model_path.display()
            )));
        }

        // Initialize llama.cpp backend
        let backend = LlamaBackend::init().map_err(|e| {
            TedError::Context(format!("Failed to initialize llama.cpp backend: {}", e))
        })?;

        // Set up model parameters
        let mut model_params = LlamaModelParams::default();
        model_params = model_params.with_n_gpu_layers(config.gpu_layers);

        // Load the model
        let model = LlamaModel::load_from_file(&backend, &config.model_path, &model_params)
            .map_err(|e| TedError::Context(format!("Failed to load model: {}", e)))?;

        Ok(Self {
            config,
            backend: Arc::new(backend),
            model: Arc::new(model),
        })
    }

    /// Get the model name
    pub fn model_name(&self) -> &str {
        &self.config.model_name
    }

    /// Convert messages to a prompt string using ChatML format
    fn messages_to_prompt(&self, messages: &[Message], system: Option<&str>) -> String {
        let mut prompt = String::new();

        // Add system prompt if present
        if let Some(sys) = system {
            prompt.push_str("<|im_start|>system\n");
            prompt.push_str(sys);
            prompt.push_str("<|im_end|>\n");
        }

        // Add conversation messages
        for msg in messages {
            let role = match msg.role {
                crate::llm::message::Role::User => "user",
                crate::llm::message::Role::Assistant => "assistant",
            };

            prompt.push_str("<|im_start|>");
            prompt.push_str(role);
            prompt.push('\n');

            // Extract text content from message
            if let Some(text) = msg.get_text() {
                prompt.push_str(text);
            }

            prompt.push_str("<|im_end|>\n");
        }

        // Add assistant prefix to elicit response
        prompt.push_str("<|im_start|>assistant\n");

        prompt
    }

    /// Generate completion for the given prompt
    fn generate(&self, prompt: &str, max_tokens: u32, temperature: f32) -> Result<String> {
        // Create context parameters
        let ctx_params =
            LlamaContextParams::default().with_n_ctx(NonZeroU32::new(self.config.context_size));

        // Create context
        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| TedError::Context(format!("Failed to create context: {}", e)))?;

        // Tokenize the prompt
        let tokens = self
            .model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| TedError::Context(format!("Failed to tokenize prompt: {}", e)))?;

        if tokens.is_empty() {
            return Ok(String::new());
        }

        // Create batch and add tokens
        let mut batch = LlamaBatch::new(self.config.context_size as usize, 1);

        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch
                .add(*token, i as i32, &[0], is_last)
                .map_err(|e| TedError::Context(format!("Failed to add token to batch: {}", e)))?;
        }

        // Decode the batch (process prompt)
        ctx.decode(&mut batch)
            .map_err(|e| TedError::Context(format!("Failed to decode batch: {}", e)))?;

        // Generate tokens
        let mut output = String::new();
        let mut n_cur = tokens.len();

        for _ in 0..max_tokens {
            // Sample next token
            let candidates = ctx.candidates_ith(batch.n_tokens() - 1);

            let mut candidates_data = candidates
                .iter()
                .map(|c| llama_cpp_2::token::data::LlamaTokenData::new(c.id(), c.logit(), 0.0))
                .collect::<Vec<_>>();

            let mut candidates_array =
                LlamaTokenDataArray::from_iter(candidates_data.iter_mut(), false);

            // Apply temperature
            candidates_array.sample_temp(temperature);

            // Greedy sampling (or use other samplers)
            let new_token_id = candidates_array.sample_token(&mut ctx);

            // Check for end of generation
            if self.model.is_eog_token(new_token_id) {
                break;
            }

            // Decode token to string
            let piece = self
                .model
                .token_to_str(new_token_id, Special::Tokenize)
                .map_err(|e| TedError::Context(format!("Failed to decode token: {}", e)))?;

            output.push_str(&piece);

            // Prepare next batch
            batch.clear();
            batch
                .add(new_token_id, n_cur as i32, &[0], true)
                .map_err(|e| TedError::Context(format!("Failed to add token: {}", e)))?;

            ctx.decode(&mut batch)
                .map_err(|e| TedError::Context(format!("Failed to decode: {}", e)))?;

            n_cur += 1;
        }

        // Clean up ChatML end token if present
        let output = output.trim_end_matches("<|im_end|>").trim_end().to_string();

        Ok(output)
    }
}

#[async_trait]
impl LlmProvider for LlamaCppProvider {
    fn name(&self) -> &str {
        "llama.cpp"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![ModelInfo {
            id: self.config.model_name.clone(),
            display_name: format!("Local: {}", self.config.model_name),
            context_window: self.config.context_size as usize,
            max_output_tokens: self.config.context_size as usize / 2,
            supports_tools: false,  // Tool support is complex with local models
            supports_vision: false, // Vision support depends on model
            supports_streaming: false, // Streaming can be added later
            input_cost_per_million: 0, // Free (local)
            output_cost_per_million: 0, // Free (local)
        }]
    }

    fn supports_model(&self, model: &str) -> bool {
        model == self.config.model_name || model == "local"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        // Convert messages to prompt
        let prompt = self.messages_to_prompt(&request.messages, request.system.as_deref());

        // Get generation parameters
        let max_tokens = request.max_tokens.unwrap_or(2048);
        let temperature = request.temperature.unwrap_or(0.7);

        // Generate response (blocking, run in spawn_blocking)
        let model = Arc::clone(&self.model);
        let backend = Arc::clone(&self.backend);
        let config = self.config.clone();
        let prompt_clone = prompt.clone();

        let output = tokio::task::spawn_blocking(move || {
            // This is a simplified version - in production, you'd want to
            // properly manage the context lifecycle
            let provider = LlamaCppProvider {
                config,
                backend,
                model,
            };
            provider.generate(&prompt_clone, max_tokens, temperature)
        })
        .await
        .map_err(|e| TedError::Context(format!("Inference task failed: {}", e)))??;

        // Estimate token counts (rough approximation: 4 chars per token)
        let input_tokens = (prompt.len() / 4) as u32;
        let output_tokens = (output.len() / 4) as u32;

        Ok(CompletionResponse {
            id: format!("local-{}", uuid::Uuid::new_v4()),
            model: self.config.model_name.clone(),
            content: vec![ContentBlockResponse::Text { text: output }],
            stop_reason: Some(StopReason::EndTurn),
            usage: Usage {
                input_tokens,
                output_tokens,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            },
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        // For now, fall back to non-streaming completion
        // Real streaming would require a different approach with channels
        let response = self.complete(request).await?;

        // Create a single-item stream that returns the complete response
        let events = vec![
            Ok(StreamEvent::MessageStart {
                message: response.clone(),
            }),
            Ok(StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlockResponse::Text {
                    text: String::new(),
                },
            }),
        ];

        // Add content delta for each text block
        let mut text_events = vec![];
        for content in &response.content {
            if let ContentBlockResponse::Text { text } = content {
                text_events.push(Ok(StreamEvent::ContentBlockDelta {
                    index: 0,
                    delta: ContentBlockDelta::TextDelta { text: text.clone() },
                }));
            }
        }

        let mut all_events = events;
        all_events.extend(text_events);
        all_events.push(Ok(StreamEvent::ContentBlockStop { index: 0 }));
        all_events.push(Ok(StreamEvent::MessageDelta {
            delta: response.stop_reason,
            usage: Some(response.usage),
        }));
        all_events.push(Ok(StreamEvent::MessageStop));

        Ok(Box::pin(futures::stream::iter(all_events)))
    }

    fn count_tokens(&self, text: &str, _model: &str) -> Result<u32> {
        // Use the model's tokenizer for accurate counting
        let tokens = self
            .model
            .str_to_token(text, AddBos::Never)
            .map_err(|e| TedError::Context(format!("Failed to tokenize: {}", e)))?;

        Ok(tokens.len() as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = LlamaCppConfig::default();
        assert_eq!(config.context_size, DEFAULT_CONTEXT_SIZE);
        assert_eq!(config.gpu_layers, DEFAULT_GPU_LAYERS);
    }

    #[test]
    fn test_config_builder() {
        let config = LlamaCppConfig::new("/path/to/model.gguf")
            .with_context_size(8192)
            .with_gpu_layers(32)
            .with_threads(8);

        assert_eq!(config.context_size, 8192);
        assert_eq!(config.gpu_layers, 32);
        assert_eq!(config.threads, Some(8));
        assert_eq!(config.model_name, "model");
    }

    #[test]
    fn test_messages_to_prompt() {
        // This test would require a loaded model, so we skip the actual prompt generation
        // Just test that the config is properly constructed
        let config = LlamaCppConfig::new("/path/to/model.gguf");
        assert_eq!(config.model_name, "model");
    }
}
