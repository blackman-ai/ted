# Unwrap Audit Tracking

This document tracks progress on converting unsafe `.unwrap()` and `.expect()` calls to proper error handling.

**Total instances:** ~1,898 across 93 files
**Target:** Reduce to safe patterns, prioritizing production code paths

---

## Phase 1: Critical Risk (Database/External Data Parsing)

### src/context/memory.rs
- [x] Line 189: `Uuid::parse_str(&id).unwrap()` in `search_keywords`
- [x] Line 190-191: `DateTime::parse_from_rfc3339(&timestamp).unwrap()` in `search_keywords`
- [x] Line 230: `Uuid::parse_str(&id).unwrap()` in `get_recent`
- [x] Line 231-232: `DateTime::parse_from_rfc3339(&timestamp).unwrap()` in `get_recent`
- [x] Line 269: `Uuid::parse_str(&id).unwrap()` in `load_all`
- [x] Line 270-271: `DateTime::parse_from_rfc3339(&timestamp).unwrap()` in `load_all`
- [x] Line 311: `Uuid::parse_str(&id).unwrap()` in `get`
- [x] Line 312-313: `DateTime::parse_from_rfc3339(&timestamp).unwrap()` in `get`

### src/tui/input.rs
- [x] Line 198: `app.editor.as_mut().unwrap()` in `handle_editor_normal_mode`
- [x] Line 273: `app.editor.as_mut().unwrap()` in `handle_editor_insert_mode`
- [x] Line 292: `app.editor.as_mut().unwrap()` in `handle_editor_command_mode`

### src/skills/loader.rs
- [x] Line 126: `skill_file.parent().unwrap_or(skill_file)` - has fallback, verified safe

---

## Phase 2: Medium Risk (RwLock Poisoning)

### src/beads/storage.rs (~17 instances) - COMPLETE
Added helper functions `read_index()` and `write_index()` for safe lock acquisition.

Methods with proper error propagation (return `Result`):
- [x] `load()` - write lock
- [x] `create()` - read + write locks
- [x] `update()` - read + write locks
- [x] `delete()` - read + write locks
- [x] `compact()` - read lock

Methods with descriptive `.expect()` (read-only, return data):
- [x] `get()` - read lock
- [x] `all()` - read lock
- [x] `by_status()` - read lock
- [x] `get_actionable()` - read lock
- [x] `by_tag()` - read lock
- [x] `by_priority()` - read lock
- [x] `children_of()` - read lock
- [x] `count()` - read lock
- [x] `stats()` - read lock

### src/skills/loader.rs (3 instances) - COMPLETE
- [x] Line 149: `self.loaded.read()` → proper error propagation
- [x] Line 166: `self.loaded.write()` → proper error propagation
- [x] Line 244: `self.loaded.write()` → descriptive `.expect()`

### src/context/memory.rs (cache locks)
- [ ] Audit remaining RwLock usage (future phase)

---

## Phase 3: File-by-File Audit (Future Sessions)

| File | Count | Status |
|------|-------|--------|
| src/skills/loader.rs | 115 | Partial |
| src/context/memory.rs | 92 | Partial |
| src/context/store.rs | 59 | Pending |
| src/tui/input.rs | 58 | Partial |
| src/context/wal/reader.rs | 56 | Pending |
| src/context/mod.rs | 55 | Pending |
| src/commands/mod.rs | 54 | Pending |
| src/beads/storage.rs | 54 | Pending |
| src/update.rs | 52 | Pending |
| src/mcp/protocol.rs | 52 | Pending |
| src/history/store.rs | 47 | Pending |
| src/context/filetree.rs | 46 | Pending |
| src/tools/builtin/plan.rs | 46 | Pending |
| src/context/cold.rs | 41 | Pending |
| src/tools/builtin/file_edit.rs | 40 | Pending |
| src/caps/loader.rs | 38 | Pending |
| src/tools/builtin/database.rs | 35 | Pending |
| src/tui/ui.rs | 34 | Pending |
| src/tools/builtin/grep.rs | 31 | Pending |
| src/tools/builtin/glob.rs | 31 | Pending |
| src/mcp/server.rs | 29 | Pending |
| src/context/wal/writer.rs | 29 | Pending |
| src/utils.rs | 28 | Pending |
| src/embedded/mod.rs | 27 | Pending |
| src/tools/external/mod.rs | 26 | Pending |
| src/llm/providers/anthropic.rs | 25 | Pending |
| src/tools/builtin/file_changeset.rs | 25 | Pending |
| src/llm/providers/ollama.rs | 24 | Pending |
| src/agents/runner.rs | 23 | Pending |
| src/tools/builtin/shell.rs | 23 | Pending |
| src/caps/builtin/defaults.rs | 23 | Pending |
| src/tools/mod.rs | 22 | Pending |
| src/mcp/transport.rs | 21 | Pending |

---

## Progress Summary

- **Phase 1 Critical:** 12/12 complete
- **Phase 2 Medium:** 20/20 complete (beads/storage.rs + skills/loader.rs)
- **Phase 3 Full Audit:** 0/33 files

Last updated: 2026-02-02
