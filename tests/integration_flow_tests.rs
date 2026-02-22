// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use std::collections::HashSet;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::{self, Stream};
use tempfile::TempDir;

use ted::chat::engine::{run_agent_loop, NoopAgentLoopObserver};
use ted::config::Settings;
use ted::context::chunk::{Chunk, ChunkContent, ChunkType};
use ted::context::store::{ContextStore, StoreConfig};
use ted::context::{ContextManager, SessionId};
use ted::llm::message::{Conversation, Message};
use ted::llm::provider::{
    CompletionRequest, CompletionResponse, ContentBlockDelta, ContentBlockResponse, LlmProvider,
    ModelInfo, StopReason, StreamEvent, Usage,
};
use ted::tools::{ToolContext, ToolExecutor};

#[derive(Default)]
struct TwoTurnStreamToolProvider {
    turn: AtomicUsize,
}

impl TwoTurnStreamToolProvider {
    fn model_info() -> ModelInfo {
        ModelInfo {
            id: "test-model".to_string(),
            display_name: "Test Model".to_string(),
            context_window: 128_000,
            max_output_tokens: 8_192,
            supports_tools: true,
            supports_vision: false,
            input_cost_per_1k: 0.0,
            output_cost_per_1k: 0.0,
        }
    }

    fn next_turn(&self) -> usize {
        self.turn.fetch_add(1, Ordering::SeqCst)
    }
}

#[async_trait]
impl LlmProvider for TwoTurnStreamToolProvider {
    fn name(&self) -> &str {
        "two-turn-stream-tool"
    }

    fn available_models(&self) -> Vec<ModelInfo> {
        vec![Self::model_info()]
    }

    fn supports_model(&self, model: &str) -> bool {
        model == "test-model"
    }

    async fn complete(&self, request: CompletionRequest) -> ted::Result<CompletionResponse> {
        let turn = self.next_turn();
        let content = if turn == 0 {
            vec![ContentBlockResponse::ToolUse {
                id: "toolu_read_notes".to_string(),
                name: "file_read".to_string(),
                input: serde_json::json!({"path": "notes.txt"}),
            }]
        } else {
            vec![ContentBlockResponse::Text {
                text: "Summary complete after reading notes.".to_string(),
            }]
        };

        Ok(CompletionResponse {
            id: format!("msg_{}", turn),
            model: request.model,
            content,
            stop_reason: Some(if turn == 0 {
                StopReason::ToolUse
            } else {
                StopReason::EndTurn
            }),
            usage: Usage {
                input_tokens: 20,
                output_tokens: 15,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            },
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> ted::Result<Pin<Box<dyn Stream<Item = ted::Result<StreamEvent>> + Send>>> {
        let turn = self.next_turn();

        let events = if turn == 0 {
            vec![
                StreamEvent::MessageStart {
                    id: "msg_0".to_string(),
                    model: request.model.clone(),
                },
                StreamEvent::ContentBlockStart {
                    index: 0,
                    content_block: ContentBlockResponse::ToolUse {
                        id: "toolu_read_notes".to_string(),
                        name: "file_read".to_string(),
                        input: serde_json::Value::Object(serde_json::Map::new()),
                    },
                },
                StreamEvent::ContentBlockDelta {
                    index: 0,
                    delta: ContentBlockDelta::InputJsonDelta {
                        partial_json: "{\"path\":\"notes.txt\"}".to_string(),
                    },
                },
                StreamEvent::ContentBlockStop { index: 0 },
                StreamEvent::MessageDelta {
                    stop_reason: Some(StopReason::ToolUse),
                    usage: Some(Usage {
                        input_tokens: 20,
                        output_tokens: 10,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                    }),
                },
                StreamEvent::MessageStop,
            ]
        } else {
            vec![
                StreamEvent::MessageStart {
                    id: "msg_1".to_string(),
                    model: request.model.clone(),
                },
                StreamEvent::ContentBlockStart {
                    index: 0,
                    content_block: ContentBlockResponse::Text {
                        text: String::new(),
                    },
                },
                StreamEvent::ContentBlockDelta {
                    index: 0,
                    delta: ContentBlockDelta::TextDelta {
                        text: "Summary complete after reading notes.".to_string(),
                    },
                },
                StreamEvent::ContentBlockStop { index: 0 },
                StreamEvent::MessageDelta {
                    stop_reason: Some(StopReason::EndTurn),
                    usage: Some(Usage {
                        input_tokens: 20,
                        output_tokens: 14,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                    }),
                },
                StreamEvent::MessageStop,
            ]
        };

        Ok(Box::pin(stream::iter(events.into_iter().map(Ok))))
    }

    fn count_tokens(&self, text: &str, _model: &str) -> ted::Result<u32> {
        Ok((text.len() as u32 / 4).max(1))
    }
}

#[tokio::test]
async fn test_context_store_recovers_compacted_chunks_after_restart() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let store_path = temp_dir.path().join("context-store");
    let config = StoreConfig {
        max_warm_chunks: 12,
        cold_threshold_secs: 0,
        enable_compression: false,
    };

    let mut inserted_ids = HashSet::new();
    {
        let mut store = ContextStore::open_with_config(store_path.clone(), config.clone())
            .await
            .expect("store should open");

        for i in 0..80 {
            let chunk = Chunk::new_tool_call(
                "shell",
                &serde_json::json!({"command": format!("echo {}", i)}),
                &format!("output-{}", i),
                false,
                None,
                0,
            );
            let chunk_id = store.append(chunk).await.expect("append should succeed");
            inserted_ids.insert(chunk_id);
        }

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        store.compact().await.expect("compaction should succeed");

        let stats = store.stats().await;
        assert_eq!(stats.total_chunks, inserted_ids.len());
        assert!(
            stats.warm_chunks + stats.cold_chunks > 0,
            "expected compaction to move at least some chunks out of hot tier"
        );
    }

    let recovered_store = ContextStore::open_with_config(store_path, config)
        .await
        .expect("store should reopen");
    let recovered_chunks = recovered_store
        .get_all()
        .await
        .expect("recovered chunks should be readable");

    let recovered_ids: HashSet<_> = recovered_chunks.iter().map(|chunk| chunk.id).collect();
    for expected_id in &inserted_ids {
        assert!(
            recovered_ids.contains(expected_id),
            "missing recovered chunk id {}",
            expected_id
        );
    }

    assert!(
        recovered_store.next_sequence() >= inserted_ids.len() as u64,
        "next sequence should advance after replay"
    );
}

#[tokio::test]
async fn test_run_agent_loop_streaming_tool_use_persists_tool_context() {
    let temp_dir = TempDir::new().expect("temp dir should be created");
    let project_root = temp_dir.path().to_path_buf();
    std::fs::write(
        project_root.join("notes.txt"),
        "Important notes from project planning.",
    )
    .expect("notes file should be written");

    let provider = TwoTurnStreamToolProvider::default();
    let mut conversation = Conversation::new();
    conversation.push(Message::user("Read notes.txt and summarize."));

    let session_id = SessionId::new();
    let context_storage = temp_dir.path().join("context");
    let context_manager = ContextManager::new(context_storage, session_id.clone())
        .await
        .expect("context manager should initialize");

    let tool_context = ToolContext::new(
        project_root.clone(),
        Some(project_root.clone()),
        session_id.0,
        true,
    );
    let mut tool_executor = ToolExecutor::new(tool_context, true);

    let mut settings = Settings::default();
    settings.defaults.max_tokens = 1024;
    settings.defaults.temperature = 0.0;

    let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut observer = NoopAgentLoopObserver;
    let completed = run_agent_loop(
        &provider,
        "test-model",
        &mut conversation,
        &mut tool_executor,
        &settings,
        &context_manager,
        true,
        &[],
        interrupted,
        &mut observer,
    )
    .await
    .expect("agent loop should run successfully");

    assert!(completed, "agent loop should complete");
    assert!(
        conversation.messages.len() >= 4,
        "expected user -> assistant(tool) -> user(tool_result) -> assistant(final)"
    );

    let tool_call_chunks = context_manager
        .get_chunks_by_type(ChunkType::ToolCall)
        .await
        .expect("tool chunks should be retrievable");
    assert_eq!(
        tool_call_chunks.len(),
        1,
        "expected one executed tool call to be persisted"
    );

    match &tool_call_chunks[0].content {
        ChunkContent::ToolCall {
            tool_name,
            input,
            output,
            is_error,
        } => {
            assert_eq!(tool_name, "file_read");
            assert_eq!(input["path"], "notes.txt");
            assert!(
                output.contains("Important notes from project planning."),
                "tool output should include file content"
            );
            assert!(!is_error);
        }
        other => panic!("expected tool call chunk, got {:?}", other),
    }

    let message_chunks = context_manager
        .get_chunks_by_type(ChunkType::Message)
        .await
        .expect("message chunks should be retrievable");
    assert!(
        message_chunks.iter().any(|chunk| {
            matches!(
                &chunk.content,
                ChunkContent::Message { role, content }
                    if role == "assistant" && content.contains("Summary complete after reading notes.")
            )
        }),
        "assistant summary message should be persisted to context"
    );
}
