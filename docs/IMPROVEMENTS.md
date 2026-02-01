# Areas for Improvement

**Date**: 2025-01-20  
**Status**: Assessment of codebase at ~98% MVP completion

---

## 1. Test Coverage - Needs Significant Expansion 🔴

- Only **6 test files** for ~29,000 lines of Rust code
- Missing tests for critical modules:
  - `src/llm/providers/` - No provider integration tests
  - `src/context/` - No context/WAL storage tests  
  - `src/mcp/` - No MCP protocol tests
  - `src/indexer/` - No indexer/language parser tests
  - `src/embeddings/` - No embedding/search tests
- The existing tests are mostly unit tests for basic types, not behavior tests

### Current Test Files
```
tests/cli_tests.rs
tests/config_tests.rs
tests/error_tests.rs
tests/llm_tests.rs
tests/tool_tests.rs
tests/tui_integration_tests.rs
```

---

## 2. Documentation Gaps 🟡

- **No `src/` module-level documentation** - `lib.rs` just lists modules without explaining architecture
- Missing **architecture documentation** - How do the context tiers work? How does the caps resolver work?
- **API documentation sparse** - Many public functions lack doc comments
- The 3 doc-tests in `indexer/recall.rs` are **ignored** (not running)

### Recommended Actions
- Add `docs/ARCHITECTURE.md` explaining major subsystems
- Add module-level doc comments to `lib.rs`
- Enable and fix ignored doc-tests

---

## 3. Error Handling Inconsistencies 🟡

`From<anyhow::Error>` converts everything to `TedError::Lsp` which is semantically wrong:

```rust
impl From<anyhow::Error> for TedError {
    fn from(err: anyhow::Error) -> Self {
        TedError::Lsp(err.to_string())  // Why LSP?
    }
}
```

Many error variants use `String` instead of structured data (loses context for debugging).

### Recommended Actions
- Add a generic `TedError::Internal(String)` variant for anyhow conversions
- Consider adding structured error data where helpful for debugging

---

## 4. Teddy TypeScript - Missing Type Safety 🟡

- `TedRunner` uses `require('fs')` inside a method instead of proper imports
- Heavy use of `console.log` for debugging - should use a proper logging framework
- No tests visible for the Electron/TypeScript layer

### Recommended Actions
- Replace inline `require()` with top-level imports
- Add a logging abstraction (e.g., electron-log)
- Add Jest/Vitest tests for parser, runner, and file applier

---

## 5. Technical Debt from ROADMAP 🟡

Per the roadmap, these are acknowledged gaps:

| Feature | Status | Priority |
|---------|--------|----------|
| Raspberry Pi thermal monitoring | Not started | P1 |
| Minimal Electron mode for low-RAM | Not started | P1 |
| Undo/redo for file operations | Not started | P1 |
| File tree search | Not started | P1 |
| Dark/light theme toggle | Not started | P2 |
| Keyboard shortcuts | Not started | P1 |
| Shareable sessions | Not started | P2 |

---

## 6. Security Considerations 🟡

Shell command blocking is pattern-based and easily bypassed:

```rust
blocked_patterns.insert("rm -rf /".to_string());
// Can bypass with: rm -rf --no-preserve-root /
// Or: rm -r -f /
// Or: find / -delete
```

MCP documentation warns about full filesystem access but there's no sandboxing.

### Recommended Actions
- Consider allowlisting safe commands instead of blocklisting dangerous ones
- Add configurable sandboxing options (e.g., restrict to project directory)
- Document security model more explicitly

---

## 7. Code Organization 🟢

Some files are very large and could benefit from splitting:

| File | Lines | Recommendation |
|------|-------|----------------|
| `src/llm/providers/anthropic.rs` | 1,541 | Split into request/response/streaming modules |
| `src/tui/ui.rs` | 1,314 | Split by screen/component |
| `src/tools/builtin/shell.rs` | 591 | Consider extracting command validation |

---

## 8. Missing Features from Competitors 🟢

Per the roadmap comparison with Lovable, v0, Bolt.new:

| Feature | Status | Effort |
|---------|--------|--------|
| Visual editing (click-to-edit) | Not started | 2 weeks |
| GitHub Actions bot | Not started | 3 days |
| Figma import | Not started | 1 week |
| Shareable sessions | Not started | 4 days |

---

## Recommendations (Priority Order)

1. **Add integration tests** for LLM providers, MCP protocol, and context storage
2. **Fix the `anyhow::Error` conversion** - add a generic `TedError::Internal` variant
3. **Add architecture documentation** - a `docs/ARCHITECTURE.md` explaining the major subsystems
4. **Replace console.log debugging** in Teddy with a proper logger
5. **Improve shell command safety** - consider allowlisting instead of blocklisting
6. **Split large files** - especially the 1500+ line provider files

---

## Summary

The codebase is generally well-structured with good error types and clean module separation. The main gaps are **testing** and **documentation**, which is common for fast-moving projects at MVP completion stage.

### Strengths
- Clean module organization
- Good error type hierarchy with `thiserror`
- No `unwrap()` calls in production code (safe error handling)
- Comprehensive feature set
- Well-documented README and ROADMAP

### Areas Needing Attention
- Test coverage is the biggest gap
- Documentation for internals/architecture
- Some large files need splitting
- Security model could be more robust
