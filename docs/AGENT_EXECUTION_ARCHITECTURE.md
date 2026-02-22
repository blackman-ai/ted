# Agent Execution Architecture

## Goal
Ted now uses a shared execution core for CLI, TUI, and embedded flows so tool loops, cancellation behavior, and tool-result message shape stay consistent across runtimes.

## Core Path
The shared core lives in `src/chat/engine.rs`:

1. `get_response_with_context_retry(...)`
2. `execute_tool_uses_with_strategy(...)`
3. `run_agent_loop_inner(...)` (CLI/default path)

`execute_tool_uses_with_strategy(...)` is the key convergence point.
It provides:

- tool loop detection via `ToolCallTracker`
- standardized cancellation handling
- ordered `tool_result` block reconstruction
- observer callbacks for UI/runtime-specific output
- pluggable execution strategy per frontend

## Strategy Interface
`ToolExecutionStrategy` is the runtime boundary:

- `SequentialToolExecutionStrategy`
  - default CLI behavior (serial tool execution)
- `TuiToolExecutionStrategy`
  - TUI-specific behavior (interactive polling, split-pane agent progress, cancellation while tools run)
- `EmbeddedToolExecutionStrategy`
  - embedded JSONL behavior, including review-mode file-modification mocking

All strategies return `ToolExecutionBatch`:

- `results`
- `cancelled_tool_use_ids`

The engine merges these into one canonical `ToolExecutionOutcome`.

## Conversation Invariants
Across runtimes, each tool turn follows the same structure:

1. assistant message with normalized `tool_use` blocks
2. tool execution with loop detection and cancellation mapping
3. single user message with ordered `tool_result` blocks

This keeps tool pairing stable for replay/history.

## Persistence Invariants
- Assistant text is persisted once per completed response.
- Tool calls are persisted from executed calls mapped by `tool_use_id`.
- Cancelled calls are not persisted as successful executions.
- Interrupted/error turns roll conversation back to pre-turn length where applicable.

## Shared Session Helpers
`src/chat/session.rs` provides reusable orchestration helpers:

- `record_message_and_persist(...)`
  - increments message count
  - optionally sets first-message summary
  - updates `SessionInfo` and history
- `trim_conversation_if_needed(...)`
  - proactive context trimming based on model context window

Used by CLI and TUI paths to avoid duplicated lifecycle logic.

## Quick Verification
Run the smoke checks from repo root:

```bash
./scripts/perf-smoke.sh
```

This validates the main latency-sensitive paths:

- agent loop execution
- shared tool execution strategy flow
- context compaction
- background compaction startup path
