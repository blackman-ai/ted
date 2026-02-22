# MVP Limitations & Workarounds

This document tracks current, user-visible limitations in Teddy.

---

## ✅ Recently Resolved

- Embedded mode JSONL integration is stable.
- Memory panel now uses backend APIs (`memoryGetRecent`, `memorySearch`).
- External file changes trigger editor reload prompts and file-tree refresh.
- Toolbar actions for Docker/PostgreSQL/Deployment are active.
- `ted lsp` now supports import/file path completion flows.
- File tree search now uses backend global file search (not just loaded nodes).
- Automated parser tests now run via `npm run test:parser`.

---

## ⚠️ Current Limitations

### Local-model tool-call parsing variability
**Issue**: Some local models still emit tool intent as plain text/JSON-like content rather than structured tool events. Teddy now attempts fallback parsing for common tool-call JSON shapes, but free-form outputs can still fail.  
**Impact**: Teddy may still show intent in chat without executing file/tool operations when model output is ambiguous.  
**Workaround**:
1. Prefer providers/models with reliable structured tool-call output.
2. Use review mode and verify planned edits before apply.

### Limited automated coverage for Teddy renderer/electron layers
**Issue**: Teddy now has parser-level automated tests, but broader renderer/component/integration coverage is still missing.  
**Impact**: UI regressions outside parser behavior can still slip in.  
**Workaround**: Run manual smoke checks for session flow, file edits, preview, and deploy settings.

---

## Suggested Next Fix Order

1. Robust local-model tool-call fallback parsing/execution strategy
2. Expand automated Teddy tests beyond parser coverage (runner/hooks/components)
3. Add renderer integration smoke tests
