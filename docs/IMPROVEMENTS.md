# Improvements and Remaining Work

**Updated**: 2026-02-16  
**Purpose**: live execution checklist for taking Ted/Teddy from strong MVP to production-hard.

---

## Current Snapshot

- Rust test suite is broad and currently passing (`cargo test` green in this workspace).
- Teddy frontend lint and type-check are passing.
- Shared chat execution core is in place (`src/chat/engine.rs`) and used across runtimes.
- `TedError::Internal` conversion for `anyhow::Error` is implemented.
- Local provider migration (llama.cpp flow) is complete across core docs/codepaths, with compatibility shims where needed.
- Shell safety hardening includes detection for `find ... -delete`.
- Teddy file tree now supports recursive expand with backend-powered global search.
- Teddy parser has automated tests (`npm run test:parser`) for tool-call fallback behavior.
- Integration coverage now includes WAL/compaction recovery, embedded JSONL event flow assertions, and streaming tool-use loop regression tests.
- Release workflow now includes host-matching binary startup smoke checks and package-content verification.
- `embedded_runner` decomposition has started by splitting runtime helpers and tests into dedicated submodules/files.
- `chat/session` and `tui/chat/runner` decomposition advanced: large inline test modules were split out, and runner settings/keymap logic moved into focused submodules.
- `tui/chat/runner` runtime/render hot path was further decomposed into `runner/render.rs`, with passing regression tests.
- `tui/chat/runner` decomposition continued by extracting `runner/commands.rs` and `runner/turn.rs`, reducing the main runner orchestration file substantially.
- Large inline test modules were split from `main.rs`, `tui/app.rs`, and `agents/runner.rs` into dedicated `tests.rs` submodules.
- `main.rs` decomposition advanced with command dispatch handlers (including `run_ask`) extracted to `src/main/cli_commands.rs` and chat rendering/session-selection helpers extracted to `src/main/chat_ui.rs`.
- `main.rs` decomposition continued with runtime bootstrap extraction to `src/main/chat_runtime.rs` and CLI observer/agent-loop wrapper extraction to `src/main/agent_loop.rs`.
- Large inline TUI test modules were moved out of `src/tui/chat/app.rs` and `src/tui/chat/ui.rs` into dedicated `tests.rs` files, keeping core runtime modules focused.
- `src/main/tests.rs` cwd-sensitive tests were hardened with a lock + RAII guard to avoid global `set_current_dir` race/flakiness.
- Structured logging now covers shared chat engine turn/tool/retry paths, TUI turn/command paths, and embedded run/tool paths; observability guidance is documented in `docs/OBSERVABILITY.md`.
- Release workflow smoke checks now run through `scripts/release-smoke.sh` for consistent startup/help diagnostics on host-matching artifacts.
- Local validation now includes successful runs of:
  - `cargo test`
  - `scripts/release-smoke.sh target/debug/ted`
  - `scripts/perf-smoke.sh`

---

## 8 Workstreams (Current Status)

1. **Security Hardening of Shell and Tool Execution** (`high`)
   - Move from pattern blocklist toward structured allow/deny rules.
   - Add stricter path and mutation gating for shell commands by default.
   - Add focused adversarial tests for bypass attempts.

2. **Provider Robustness and Local-Model Tool-Call Fallbacks** (`high`)
   - Improve handling when local models emit tool intent as raw text instead of structured tool events.
   - Add deterministic parsing/normalization tests for local provider edge cases.
   - Add clearer runtime diagnostics when tool-call parsing degrades.

3. **Integration Test Expansion for Critical Flows** (`high`) - `baseline complete`
   - Added integration tests around context/WAL recovery and compaction boundaries.
   - Added provider-level streaming/tool-use behavior test coverage.
   - Added end-to-end embedded JSONL flow regression assertions for Teddy interoperability.

4. **Teddy UX Completion for Daily-Driver Use** (`high`)
   - File tree has recursive expand + search; continue with large-project scaling behavior.
   - Add keyboard shortcuts and stronger session ergonomics.
   - Finish polish for review/diff flows and edge-case error display.

5. **Codebase Decomposition of Large Modules** (`medium`) - `complete`
   - Split oversized files (notably large provider and TUI modules) into focused submodules.
   - Reduce cross-module coupling and improve incremental compile ergonomics.

6. **Observability and Diagnostics** (`medium`) - `complete`
   - Standardize structured logs across Rust + Teddy (including log levels and correlation IDs).
   - Expose practical debug toggles for embedded mode and provider calls.

7. **Packaging and Distribution Hardening** (`medium`) - `complete`
   - Validate portability on macOS/Linux/Windows release artifacts.
   - Tighten bundled dependency checks and first-run diagnostics in Teddy.
   - Add CI coverage for release packaging smoke checks.

8. **Contributor and Architecture Documentation** (`medium`) - `complete`
   - Keep architecture docs aligned with implementation changes.
   - Add contributor-focused docs for debugging, testing, and release process.
   - Keep feature/limitation docs synchronized with actual current behavior.

---

## Recommended Execution Order

1. Workstreams 1-3 (safety + correctness gates)  
2. Workstream 4 (UX completion for end users)  
3. Workstreams 5-6 (maintainability + diagnostics)  
4. Workstreams 7-8 (distribution + contributor velocity)

---

## Definition of “Done Enough” for v1

- Security model is explicit and enforced by default.
- Local-provider behavior is reliable, including tool-call edge cases.
- CI covers core regressions (Rust/Teddy + embedded flow).
- Teddy supports smooth everyday workflows without manual recovery steps.
- Documentation matches reality and enables outside contributors to onboard quickly.
