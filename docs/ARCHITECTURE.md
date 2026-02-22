# Ted Architecture

This document explains how Ted and Teddy are structured today, with emphasis on shared execution, local-first portability, and context compaction.

## Runtime Surfaces

- `ted` CLI (`src/main.rs`)
  - Interactive chat, one-shot ask, settings, history, context commands, caps management.
  - Decomposed runtime modules under `src/main/`:
    - `cli_commands.rs` (command handlers)
    - `chat_ui.rs` (CLI chat presentation helpers)
    - `chat_runtime.rs` (provider/session/bootstrap setup)
    - `agent_loop.rs` (CLI observer and chat engine loop wrappers)
- TUI chat runtime (`src/tui/chat/*`)
  - Interactive terminal UI with streaming, tool events, agent pane, session controls.
  - UI/controller decomposition:
    - `app/controller.rs`, `app/messages.rs`
    - `ui/layout.rs`, `ui/view.rs`, `ui/overlay.rs`
- Embedded runtime (`src/embedded_runner.rs`)
  - JSONL event stream used by Teddy (Electron app) for local GUI workflows.

All three surfaces converge on shared chat/session orchestration in `src/chat/*`.

## Chat Core

- `src/chat/engine.rs`
  - Shared execution loop and tool-use batching.
  - Runtime-specific execution strategies:
    - `SequentialToolExecutionStrategy` (CLI/default)
    - `TuiToolExecutionStrategy`
    - `EmbeddedToolExecutionStrategy`
  - Tool loop detection, cancellation mapping, ordered tool-result reconstruction.
- `src/chat/session.rs`
  - Session lifecycle helpers, persistence, history synchronization, context trimming.
- `src/chat/mod.rs`
  - Public orchestration entry points used by CLI/TUI/embedded callers.

## LLM Layer

- Provider abstraction in `src/llm/*`.
- Implementations:
  - `anthropic`
  - `openrouter`
  - `blackman`
  - `local` (llama.cpp/OpenAI-compatible local server flow)
- Local model process management in `src/llm/providers/local/server.rs`.

The provider layer normalizes model responses into shared message/tool block structures consumed by the chat engine.

## Context and Memory

- `src/context/*` manages durable context with warm/cold tiers.
- Write-ahead log (WAL) under `src/context/wal/*` for append-safe session context updates.
- Background compaction reduces context size and keeps long chats responsive.
- Recall/index features are provided by:
  - `src/indexer/*`
  - `src/embeddings/*` (optionally bundled via feature flags)

## Tools and Safety Model

- Built-in tools under `src/tools/builtin/*` (`file_read`, `file_write`, `file_edit`, `shell`, `glob`, `grep`, and agent/tooling extensions).
- Central execution path in `src/tools/executor.rs`.
- Shell command validation/hardening in `src/tools/builtin/shell.rs`.
- Permissions are mediated by runtime trust mode, cap policy, and explicit approval flows.

## Agents, Plans, and Beads

- Multi-agent and agent orchestration: `src/agents/*`.
- Planning and plan tool: `src/plans/*`, `src/tools/builtin/plan.rs`.
- Bead/task tracking: `src/beads/*`.
- Shared workspace design notes: `DESIGN-shared-workspace.md`.
- Agent execution architecture details: `docs/AGENT_EXECUTION_ARCHITECTURE.md`.

## Hardware-Adaptive Behavior

- Hardware detection and tiering: `src/hardware/*`.
- Thermal monitoring helpers: `src/hardware/thermal.rs`.
- Runtime guardrails can reduce token/context pressure on constrained or throttled systems.

## Teddy (Electron Desktop)

- Desktop app lives under `teddy/`.
- Electron main process bridges local filesystem/process operations and Ted embedded mode.
- `teddy/electron/ted/runner.ts` spawns `ted chat --embedded`.
- `teddy/electron/ted/parser.ts` parses JSONL events and forwards to React UI.

Teddy is designed as the GUI layer; Ted remains the core agent/runtime layer.

## Build and Validation

- Rust validation: `cargo test`
- Teddy validation:
  - `npm run lint`
  - `npm run type-check`
- Smoke script: `scripts/perf-smoke.sh`
  - Fast regression checks for shared chat/tool/context paths.
- Release diagnostics script: `scripts/release-smoke.sh`
  - Startup/help command smoke checks used by release CI for host-matching artifacts.

## Observability

- Logging is structured with `tracing`.
- Runtime targets:
  - `ted.chat.engine`
  - `ted.tui.runner`
  - `ted.embedded`
- Use `RUST_LOG` for fine-grained control and `ted ... -v` for a practical debug preset.
- See `docs/OBSERVABILITY.md` for field-level details and examples.
