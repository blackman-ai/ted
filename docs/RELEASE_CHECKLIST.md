# Ted/Teddy Release Checklist (Strict)

Updated: 2026-02-16

Use this as the single "done gate" before broad release. Every item is pass/fail.

## Phase 1: Teddy UX Polish (Priority First)

1. Keyboard workflow and discoverability
   - Pass criteria:
     - Common actions have shortcuts (new chat, settings, tab switch, focus chat).
     - Shortcut reference is available in-app.
     - `Esc` closes overlays/dialogs.
   - Status: Complete

2. Non-blocking error/recovery UX
   - Pass criteria:
     - No critical workflow depends on modal `alert()` interruptions.
     - Failures are visible inline (status/toast) with actionable wording.
     - Session actions report success/failure in UI.
   - Status: Complete

3. Large-project file navigation
   - Pass criteria:
     - File search is global (not limited to expanded nodes).
     - Selecting search result can open/focus the target path.
     - Search remains responsive under large trees.
   - Status: Complete

4. UX regression guard
   - Pass criteria:
     - `npm run lint`, `npm run type-check`, `npm run test:parser` pass.
     - CI executes Teddy checks on every PR/push.
   - Status: Complete

## Phase 2: Security and Safety Hardening

1. Shell policy model
   - Pass criteria:
     - Root-destructive patterns blocked independent of trust mode.
     - Workspace boundary escape patterns blocked for mutating commands.
     - Integration tests verify bypass attempts fail.
   - Status: Complete

2. Tool execution guardrails
   - Pass criteria:
     - Clear mutation classification for risky commands.
     - Permission requests mark destructive actions correctly.
     - New tests added for discovered bypass classes.
   - Status: Complete

## Phase 3: Reliability and Test Expansion

1. Local-model tool-call reliability
   - Pass criteria:
     - Fallback parser handles common raw JSON tool-call formats.
     - Unknown/ambiguous responses fail safely with visible diagnostics.
     - Automated tests cover positive/negative parser cases.
   - Status: Complete

2. Integration flow coverage
   - Pass criteria:
     - WAL/compaction recovery scenarios have integration tests.
     - Embedded mode end-to-end event flow has regression tests.
     - Provider streaming/tool-use behavior has integration coverage.
   - Status: Complete

## Phase 4: Maintainability and Shipping Ops

1. Module decomposition
   - Pass criteria:
     - Oversized files split with stable behavior and passing tests.
   - Status: Complete (CLI chat runtime/bootstrap moved into `src/main/chat_runtime.rs`, CLI agent-loop observer/wrappers moved into `src/main/agent_loop.rs`, and large inline TUI test modules moved into dedicated `src/tui/chat/app/tests.rs` and `src/tui/chat/ui/tests.rs`; full test suite passes)

2. Observability
   - Pass criteria:
     - Structured logging approach documented and applied in hot paths.
   - Status: Complete

3. Packaging and release hardening
   - Pass criteria:
     - Cross-platform build smoke checks pass.
     - Installer/startup diagnostics validated.
   - Status: Complete (release smoke diagnostics are wired in CI, and local artifact checks passed via `scripts/release-smoke.sh target/debug/ted`; perf regression smoke passed via `scripts/perf-smoke.sh`)

4. Final docs consistency
   - Pass criteria:
     - Architecture, limitations, and release docs match current behavior.
   - Status: Complete
