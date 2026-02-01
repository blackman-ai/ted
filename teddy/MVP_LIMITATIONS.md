# MVP Limitations & Workarounds

This document tracks known limitations in the Teddy MVP and provides practical workarounds.

---

## ✅ Embedded Mode Status

Ted's `--embedded` mode is implemented and emits JSONL events. Teddy can consume these events for streaming chat, file ops, and status updates.

---

## ⚠️ Known Limitations

### Ollama tool-call parsing
**Issue**: Ollama returns tool calls as raw JSON text rather than structured `tool_use` events.  
**Impact**: Tool calls can appear as plain assistant text in Teddy.  
**Workaround**:
1. Use Anthropic/OpenRouter for fully structured tool events
2. Add UI-side parsing for Ollama tool-call JSON in Teddy

### Conversation Memory panel is stubbed
**Issue**: The Memory UI uses placeholder data and does not call Ted APIs.  
**Impact**: No recent memory list or semantic search in Teddy.  
**Workaround**: Use Ted CLI session history for now.

### External file change UX
**Issue**: Teddy detects external changes but does not auto-reload the currently open file.  
**Impact**: Users may need to reselect the file to see updates.  
**Workaround**: Reopen the file or use the Preview refresh.

### LSP file path completions
**Issue**: `ted lsp` lacks file path completions.  
**Impact**: Autocomplete is missing path suggestions in the CLI.  
**Workaround**: Use manual paths or shell completion.

### Toolbar buttons are disabled
**Issue**: The top-level Docker/PostgreSQL/Deploy buttons are disabled.  
**Impact**: Features are accessible in Settings/Preview but not from the toolbar.  
**Workaround**: Use Settings → Database and Preview → Deploy.

---

## Suggested Fix Order

1. Ollama tool-call parsing (unblocks local/offline default)
2. Memory panel API wiring
3. Auto-reload or notify on external file edits
4. Enable toolbar buttons or remove them
5. LSP file path completions
