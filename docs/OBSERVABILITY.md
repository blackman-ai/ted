# Ted Observability

This document defines the current logging model for Ted/Teddy runtimes.

## Logging Stack

- Runtime logging uses `tracing` + `tracing_subscriber`.
- Default level is `WARN`.
- `RUST_LOG` can always override defaults.

## Practical Debug Toggles

- `ted ... -v`
  - Enables debug diagnostics for core runtime targets:
    - `ted.chat.engine`
    - `ted.tui.runner`
    - `ted.embedded`
- `RUST_LOG=...` remains the most precise control surface.

Examples:

- `RUST_LOG=ted.chat.engine=debug ted chat`
- `RUST_LOG=ted.tui.runner=debug ted chat`
- `RUST_LOG=ted.embedded=debug ted chat --embedded --prompt "hello"`

## Structured Event Targets

- `ted.chat.engine`
  - Turn lifecycle (`turn`, `conversation_messages`)
  - Retry/rate-limit behavior (`attempt`, `retry_after_secs`)
  - Tool phase summaries (`tool_calls`, `tool_results`, `cancelled_tool_calls`)
- `ted.tui.runner`
  - Slash-command handling (`command`)
  - TUI turn/tool execution summaries (`turn`, `tool_calls`, `tool_results`)
- `ted.embedded`
  - Embedded run lifecycle (`session_id`, `provider`, `model`)
  - Turn and tool batch progression (`turn`, `tool_calls`)

## Current Gaps

- Per-request provider transport timing is not yet emitted as structured spans.
- Cross-process correlation IDs between Teddy Electron and Ted embedded JSONL events are not yet standardized.
