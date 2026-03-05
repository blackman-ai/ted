# Improvements and Active Roadmap

**Updated**: 2026-03-05  
**Purpose**: canonical active roadmap for Ted/Teddy.

This file is the source of truth for what is still in progress.  
Completed historical execution batches are captured in `docs/GAP_CLOSURE_EXECUTION_PLAN.md`.

---

## Product Vision

Ted/Teddy should operate as one local-first coding-agent system with:
- shared execution behavior across CLI, TUI, and embedded/Teddy surfaces,
- composable caps identity and policy controls (including org governance),
- explicit and auditable tool safety,
- enough reliability and observability for daily production use.

---

## Status Legend

- `complete`: shipped and validated; maintenance only.
- `partial`: major capability shipped, but meaningful gaps remain.
- `active`: currently prioritized implementation work.

---

## Current Validated Snapshot

- Rust suite passes in this workspace (`cargo test -- --test-threads=1`).
- Teddy checks pass (`npm run lint`, `npm run type-check`, `npm run test:parser`).
- Release and perf smoke checks pass:
  - `scripts/release-smoke.sh target/debug/ted`
  - `scripts/perf-smoke.sh`
- Shared chat core remains unified in `src/chat/engine.rs`.
- Policy engine v2 (`allow|ask|deny`, include packs, lock rules, audit log, compliance command) is shipped.
- Caps identity/policy rendering adapter is shipped with legacy `system_prompt` compatibility.

---

## 8 Workstreams (Current Status)

1. **Security Hardening of Shell and Tool Execution** (`high`) - `partial`
   - Structured policy controls are in place and enforced.
   - Remaining work: deeper adversarial/bypass regression coverage and continued hardening.

2. **Provider Robustness and Local-Model Tool-Call Fallbacks** (`high`) - `active`
   - Fallback parsing and regression tests exist across Rust and Teddy parser layers.
   - Remaining work: improve behavior on ambiguous free-form local model output and strengthen diagnostics.

3. **Integration Test Expansion for Critical Flows** (`high`) - `complete`
   - WAL/compaction recovery and embedded streaming/tool-loop regressions are covered.
   - Ongoing work is maintenance-level test additions only.

4. **Teddy UX Completion for Daily-Driver Use** (`high`) - `active`
   - Recursive file-tree expand/search, session controls, and shortcut overlays are in place.
   - Remaining work: renderer/electron integration coverage and final UX polish for edge cases.

5. **Codebase Decomposition of Large Modules** (`medium`) - `partial`
   - Significant decomposition has landed in `main`, TUI, and runner paths.
   - Remaining work: reduce very large core modules (`chat/engine`, `tools/executor`, `embedded_runner`) incrementally.

6. **Observability and Diagnostics** (`medium`) - `partial`
   - Structured logging and practical debug toggles are in place.
   - Remaining work (tracked in `docs/OBSERVABILITY.md`): provider transport timing spans and cross-process correlation IDs.

7. **Packaging and Distribution Hardening** (`medium`) - `complete`
   - CI/release flows include host-matching smoke checks and package-content verification.
   - Ongoing work is routine maintenance and platform drift monitoring.

8. **Contributor and Architecture Documentation** (`medium`) - `partial`
   - Core architecture and policy docs are strong.
   - Remaining work: keep status docs synchronized, remove stale MVP language, and maintain a single active roadmap narrative.

---

## Priority Order (Current)

1. Workstream 1 (safety hardening follow-through)
2. Workstream 2 (provider robustness)
3. Workstream 4 (Teddy UX and integration confidence)
4. Workstream 6 (observability gaps)
5. Workstream 5 (module decomposition cleanup)
6. Workstream 8 (docs coherence maintenance)

---

## Document Map

- `docs/IMPROVEMENTS.md`: active roadmap and current status (this file).
- `docs/GAP_CLOSURE_EXECUTION_PLAN.md`: completed execution batch history.
- `docs/OBSERVABILITY.md`: logging model plus open diagnostics gaps.
- `teddy/MVP_LIMITATIONS.md`: user-visible Teddy limitations and workarounds.

---

## Definition of “Done Enough” for v1

- Security model is explicit, enforced, and resilient against common bypass patterns.
- Local-provider behavior is dependable for tool-call execution, including fallback scenarios.
- CI and smoke checks cover critical regressions across Rust and Teddy.
- Teddy workflows are smooth without manual recovery for common development tasks.
- Docs remain accurate, non-contradictory, and usable for external contributors.
