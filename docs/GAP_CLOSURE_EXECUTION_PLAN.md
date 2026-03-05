# Ted Gap-Closure Execution Plan

> Historical status note (2026-03-05): this execution plan is complete for the cycle it defined.
> Active remaining work now lives in `docs/IMPROVEMENTS.md`.

## Objective
Close the highest-impact product gaps versus mature coding-agent tools while preserving Ted's strengths:
- local-first runtime
- composable caps identity/policy
- portable CLI/TUI/embedded architecture

This document is the implementation backlog and sequencing source of truth.

## Scope
In scope:
- Permission System V2 (`allow|ask|deny`, policy files, audit trail)
- Cross-surface continuity (CLI/TUI/VS Code session continuity)
- Team/org governance (managed policy packs, enforceable guardrails, compliance reporting)

Out of scope for this plan:
- replacing existing cap model
- hosted cloud synchronization as a hard dependency
- unrelated refactors

## Milestones
Timeline assumes focused implementation by one core maintainer with review support.

1. Milestone A (Weeks 1-4): Permission System V2 GA
2. Milestone B (Weeks 5-9): IDE + session continuity parity
3. Milestone C (Weeks 10-14): Team governance and compliance reporting

## Milestone A: Permission System V2

### A1. Policy engine foundation (in progress)
Ticket: `PERM-001`  
Status: Completed  
Files:
- `src/tools/policy.rs`
- `src/tools/permission.rs`
- `src/tools/executor.rs`
- `src/config/settings/io.rs`

Deliverables:
- Policy loader for user + project policy files
- Deterministic merge order: user rules then project rules (project can override)
- Rule matching for tool/command/path/destructive dimensions
- Executor integration for policy allow/deny/ask behavior

Acceptance:
- Policy deny blocks matching tool requests before interactive prompt
- Policy allow auto-approves matching requests without prompt
- Policy ask forces prompt even if default heuristic would auto-approve
- Existing behavior unchanged when no policy file exists

### A2. Policy schema and documentation
Ticket: `PERM-002`  
Status: Completed  
Files:
- `docs/PERMISSIONS_POLICY_V2.md`
- `README.md`
- `src/main/cli_commands.rs` (help text updates)

Deliverables:
- Stable `permissions.toml` schema docs
- Examples for common org scenarios (safe commands, blocked shell ops, path restrictions)
- Migration notes from trust-mode-only workflows

Acceptance:
- Users can configure first policy from docs only
- Policy examples are copy-paste runnable

### A3. CLI policy management commands
Ticket: `PERM-003`  
Status: Completed  
Files:
- `src/cli/args.rs`
- `src/main/cli_commands.rs`
- `src/main/tests.rs`

Deliverables:
- `ted permissions show`
- `ted permissions init`
- `ted permissions check --tool <name> --action "..."`

Acceptance:
- Users can inspect active merged policy without reading files manually
- Users can dry-run policy decisions from terminal

### A4. Runtime policy diagnostics
Ticket: `PERM-004`  
Status: Completed  
Files:
- `src/tools/executor.rs`
- `src/embedded_runner.rs`
- `src/tui/chat/*`

Deliverables:
- explainable deny message (rule source + optional reason)
- status feedback when policy file fails to parse
- optional verbose logs for policy matches

Acceptance:
- Every deny includes "why" and "from where"
- parse failures are visible and non-fatal

### A5. Permission audit trail
Ticket: `PERM-005`  
Status: Completed  
Files:
- `src/history/*` or new `src/audit/*`
- `src/tools/executor.rs`
- `src/context/*` (if reusing storage)

Deliverables:
- append-only local record of allow/deny/prompt decisions
- event shape: timestamp, tool, action summary, decision, policy source/user response

Acceptance:
- simple query command can show recent permission decisions
- records are durable across sessions

## Milestone B: IDE + Session Continuity

### B1. Shared session resume contract
Ticket: `CONT-001`  
Status: Completed  
Files:
- `src/chat/session.rs`
- `src/history/*`
- `src/embedded_runner.rs`

Deliverables:
- stable session attach API for external clients
- capability metadata: caps, model, policy state, context window stats

Acceptance:
- session started in CLI can be resumed by embedded/IDE flow without manual copy

### B2. VS Code extension continuity integration
Ticket: `CONT-002`  
Status: Completed  
Files:
- `vscode-extension/*`
- `src/embedded/*`

Deliverables:
- attach-to-session command in VS Code
- resume-from-IDE command in CLI

Acceptance:
- users can move active work between surfaces with one command each direction

### B3. Surface parity tests
Ticket: `CONT-003`  
Status: Completed  
Files:
- `src/embedded_runner/tests.rs`
- `src/main/tests.rs`
- `src/tui/chat/runner/tests.rs`

Deliverables:
- shared parity assertions for caps/policy/tool outcomes across CLI/TUI/embedded

Acceptance:
- no divergence in permission behavior by surface

## Milestone C: Team Governance and Compliance

### C1. Managed policy packs
Ticket: `GOV-001`  
Status: Completed  
Files:
- `src/config/*`
- `src/tools/policy.rs`
- `src/main/cli_commands.rs`

Deliverables:
- policy pack include mechanism
- lock mode for required denies/mandatory rules

Acceptance:
- org can distribute a pinned policy pack and enforce it project-wide

### C2. Managed caps packs and required caps
Ticket: `GOV-002`  
Status: Completed  
Files:
- `src/caps/*`
- `src/config/*`
- `src/main.rs`

Deliverables:
- optional config for required caps
- deny-list for disallowed caps in managed mode

Acceptance:
- org-level policy can prevent bypass via ad-hoc cap stack

### C3. Compliance report command
Ticket: `GOV-003`  
Status: Completed  
Files:
- `src/main/cli_commands.rs`
- `src/history/*` or `src/audit/*`

Deliverables:
- `ted compliance report --since <date>`
- summarize policy denies, prompt overrides, trust usage

Acceptance:
- leads can produce a machine-readable local report for audits

## Dependency order
1. `PERM-001` -> `PERM-002` -> `PERM-003` -> `PERM-004` -> `PERM-005`
2. `CONT-*` starts after `PERM-003` stabilizes
3. `GOV-*` starts after `PERM-005` storage format is stable

## Risks and mitigations
Risk: policy complexity increases user confusion  
Mitigation: ship `permissions init` templates + `permissions check` dry-run command.

Risk: behavior drift between runtime surfaces  
Mitigation: parity tests in `CONT-003` and shared execution entrypoints only.

Risk: breaking existing workflows  
Mitigation: no policy file means legacy behavior; trust mode remains explicit override.

## Current kickoff status
Completed in this cycle:
- `PERM-001` through `PERM-005`
- `CONT-001` through `CONT-003`
- `GOV-001` through `GOV-003`
