# Teddy Architecture

**Version**: 0.1.0 MVP
**Status**: Initial Implementation
**Last Updated**: 2026-01-12

## Overview

Teddy is an offline-first AI coding environment built as an Electron desktop app. It embeds the Ted CLI as its AI engine while maintaining Ted's independence as a standalone tool.

---

## Design Decisions

### 1. Integration Strategy: Subprocess (Not Library)

**Decision**: Spawn Ted as a child process with `--embedded` flag

**Rationale**:
- ✅ Zero coupling - Ted remains 100% independent
- ✅ Crash isolation - Ted crashes don't kill Teddy
- ✅ Independent releases - Teddy can pin to specific Ted versions
- ✅ Simple IPC - JSONL over stdout is proven and reliable
- ✅ Easy testing - Ted can be tested independently

**Tradeoffs**:
- ❌ ~50-200ms startup overhead per Ted invocation
- ❌ Serialization cost (JSONL encode/decode)
- ❌ Harder to debug across process boundary

**Migration Path** (V2):
1. Extract `ted_core` crate from Ted
2. Create FFI bindings or Rust native Node module
3. Embed `ted_core` for hot paths
4. Keep subprocess option for stability

### 2. Repository Structure: Monorepo

**Decision**: Teddy lives in `teddy/` subdirectory of Ted repo

**Rationale**:
- ✅ Easier coordination during rapid iteration
- ✅ Shared CI/CD for both projects
- ✅ Single source of truth for protocol changes
- ✅ Atomic commits for cross-project changes

**Alternative Considered**: Separate repos with Teddy consuming Ted binaries via releases
- Would be better for stable, slower-moving projects
- Too much overhead during MVP phase

### 3. Protocol: JSONL Streaming

**Decision**: Line-delimited JSON events on stdout

**Format**:
```jsonl
{"type":"event_name","timestamp":1234567890,"session_id":"abc","data":{...}}
```

**Rationale**:
- ✅ Streaming-friendly - emit events as they happen
- ✅ Language-agnostic - works with any consumer
- ✅ Human-readable - easy to debug with `tail -f`
- ✅ Backwards compatible - only active with `--embedded`
- ✅ Proven pattern - used by LSP, DAP, etc.

**Security Considerations**:
- Events never contain sensitive data (API keys, secrets)
- File paths are validated against project root
- No arbitrary code execution from events

---

## System Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Teddy (Electron)                     │
│                                                         │
│  ┌───────────────────────────────────────────────────┐ │
│  │          Renderer Process (React)                 │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────────┐   │ │
│  │  │ FileTree │  │  Editor  │  │  ChatPanel   │   │ │
│  │  └──────────┘  └──────────┘  └──────────────┘   │ │
│  │  ┌──────────┐  ┌──────────┐                      │ │
│  │  │ Preview  │  │ Console  │                      │ │
│  │  └──────────┘  └──────────┘                      │ │
│  └─────────────────────┬─────────────────────────────┘ │
│                        │ IPC (contextBridge)           │
│  ┌─────────────────────┴─────────────────────────────┐ │
│  │         Main Process (Node.js)                    │ │
│  │                                                    │ │
│  │  ┌─────────────┐  ┌──────────────┐               │ │
│  │  │ TedRunner   │  │ FileApplier  │               │ │
│  │  └──────┬──────┘  └──────────────┘               │ │
│  │         │                                         │ │
│  │  ┌──────┴─────────┐  ┌──────────────┐            │ │
│  │  │  TedParser     │  │ AutoCommit   │            │ │
│  │  │  (JSONL)       │  │ (Git)        │            │ │
│  │  └────────────────┘  └──────────────┘            │ │
│  └─────────────────────┬─────────────────────────────┘ │
└────────────────────────┼───────────────────────────────┘
                         │ child_process.spawn()
                         │ stdin/stdout/stderr
            ┌────────────┴────────────┐
            │   Ted CLI (Rust)        │
            │   --embedded mode       │
            │   JSONL → stdout        │
            └─────────────────────────┘
```

---

## Component Responsibilities

### Renderer Process (React)

**Purpose**: User interface and local state management

**Components**:
- `App.tsx` - Main app shell, layout orchestration
- `FileTree.tsx` - Project file browser
- `Editor.tsx` - Monaco-based code editor
- `ChatPanel.tsx` - AI chat interface
- `Preview.tsx` - Embedded webview for live preview
- `Console.tsx` - Logs and command output

**Hooks**:
- `useProject()` - Current project state
- `useTed()` - Ted integration, event handling

**State**:
- Current project path
- Selected file
- Ted events (plan, file ops, status)
- Console logs

### Main Process (Node.js)

**Purpose**: System integration, Ted orchestration, file I/O

**Modules**:

#### `TedRunner` (ted/runner.ts)
- Spawns Ted process with appropriate flags
- Manages lifecycle (start, stop, restart)
- Pipes stdout to parser, stderr to console

#### `TedParser` (ted/parser.ts)
- Parses JSONL events from stdout
- Emits typed events via EventEmitter
- Handles incomplete lines, buffering

#### `FileApplier` (operations/file-applier.ts)
- Applies file operations to disk
- Validates paths (no directory traversal)
- Handles create/edit/delete operations

#### `AutoCommit` (git/auto-commit.ts)
- Initializes Git repo if needed
- Stages and commits file changes
- Generates commit messages from AI summaries

**IPC Handlers**:
- `dialog:openFolder` - Folder picker
- `project:set` - Set active project
- `ted:run` - Execute Ted with prompt
- `file:read/write/list` - File operations

### Ted CLI (Rust)

**Purpose**: AI agent logic, LLM integration, tool execution

**Embedded Mode** (`--embedded` flag):
- Outputs JSONL events instead of TUI
- Auto-approves tool uses if `--trust` is set
- Streams events as they occur
- Still works as normal CLI without flag

**Event Flow**:
1. Parse prompt
2. Emit `plan` event with steps
3. Emit `status` events during execution
4. Emit file operation events (`file_create`, etc.)
5. Emit `completion` or `error` event

---

## JSONL Protocol Specification

### Event Structure

All events follow this base schema:

```typescript
interface BaseEvent {
  type: string;           // Event type identifier
  timestamp: number;      // Unix timestamp (milliseconds)
  session_id: string;     // Ted session ID
  data: object;           // Event-specific payload
}
```

### Event Types

#### 1. `plan`
Emitted when Ted formulates a plan before execution.

```json
{
  "type": "plan",
  "timestamp": 1705420800000,
  "session_id": "abc123",
  "data": {
    "steps": [
      {
        "id": "1",
        "description": "Read existing authentication logic",
        "estimated_files": ["src/auth.ts"]
      },
      {
        "id": "2",
        "description": "Add password reset endpoint"
      }
    ]
  }
}
```

#### 2. `file_create`
Create a new file.

```json
{
  "type": "file_create",
  "timestamp": 1705420801000,
  "session_id": "abc123",
  "data": {
    "path": "src/routes/reset-password.ts",
    "content": "import express from 'express';\n\n...",
    "mode": 420  // Optional: Unix permissions (0o644)
  }
}
```

#### 3. `file_edit`
Modify an existing file.

```json
{
  "type": "file_edit",
  "timestamp": 1705420802000,
  "session_id": "abc123",
  "data": {
    "path": "src/auth.ts",
    "operation": "replace",
    "old_text": "export { login, logout };",
    "new_text": "export { login, logout, resetPassword };"
  }
}
```

Operations:
- `replace` - Find and replace text (requires `old_text`, `new_text`)
- `insert` - Insert at line (requires `line`, `text`)
- `delete` - Delete line (requires `line`)

#### 4. `file_delete`
Remove a file.

```json
{
  "type": "file_delete",
  "timestamp": 1705420803000,
  "session_id": "abc123",
  "data": {
    "path": "src/deprecated.ts"
  }
}
```

#### 5. `command`
Execute a shell command.

```json
{
  "type": "command",
  "timestamp": 1705420804000,
  "session_id": "abc123",
  "data": {
    "command": "npm install bcrypt",
    "cwd": ".",
    "env": {
      "NODE_ENV": "development"
    }
  }
}
```

#### 6. `status`
Progress updates.

```json
{
  "type": "status",
  "timestamp": 1705420805000,
  "session_id": "abc123",
  "data": {
    "state": "writing",  // thinking | reading | writing | running
    "message": "Creating authentication routes",
    "progress": 60  // Optional: 0-100
  }
}
```

#### 7. `error`
Something went wrong.

```json
{
  "type": "error",
  "timestamp": 1705420806000,
  "session_id": "abc123",
  "data": {
    "code": "FILE_NOT_FOUND",
    "message": "Cannot edit src/auth.ts: file does not exist",
    "suggested_fix": "Create the file first or check the path",
    "context": {
      "attempted_path": "src/auth.ts"
    }
  }
}
```

#### 8. `completion`
Task finished.

```json
{
  "type": "completion",
  "timestamp": 1705420807000,
  "session_id": "abc123",
  "data": {
    "success": true,
    "summary": "Added password reset functionality",
    "files_changed": [
      "src/routes/reset-password.ts",
      "src/auth.ts",
      "package.json"
    ]
  }
}
```

#### 9. `message`
AI assistant message (streamed text).

```json
{
  "type": "message",
  "timestamp": 1705420808000,
  "session_id": "abc123",
  "data": {
    "role": "assistant",
    "content": "I'll help you add authentication...",
    "delta": false  // true for streaming chunks
  }
}
```

---

## Data Flow: User Prompt → File Changes

### 1. User Input
```
User types in ChatPanel: "Add a login button to the navbar"
```

### 2. Renderer → Main Process
```typescript
// ChatPanel.tsx
const handleSend = () => {
  window.teddy.runTed(prompt, { trust: false });
};
```

### 3. Main Process Spawns Ted
```typescript
// main.ts
ipcMain.handle('ted:run', async (_, prompt, options) => {
  tedRunner = new TedRunner({
    workingDirectory: currentProjectRoot,
    ...options
  });

  tedRunner.on('event', (event) => {
    mainWindow.webContents.send('ted:event', event);
  });

  await tedRunner.run(prompt);
});
```

### 4. Ted Outputs JSONL
```bash
$ ted chat --embedded "Add a login button to the navbar"

{"type":"plan","timestamp":...,"data":{"steps":[...]}}
{"type":"status","timestamp":...,"data":{"state":"reading","message":"Reading navbar component"}}
{"type":"file_edit","timestamp":...,"data":{"path":"src/Navbar.tsx","operation":"replace",...}}
{"type":"completion","timestamp":...,"data":{"success":true,"summary":"Added login button"}}
```

### 5. Parser Emits Events
```typescript
// parser.ts
this.parseLine(line);  // Parse each JSONL line
this.emit('file_edit', event);  // Emit typed event
```

### 6. FileApplier Applies Changes
```typescript
// main.ts
tedRunner.on('file_edit', async (event) => {
  await fileApplier.applyEdit(event);
  mainWindow.webContents.send('file:changed', { path: event.data.path });
});
```

### 7. AutoCommit Saves to Git
```typescript
// main.ts
tedRunner.on('completion', async (event) => {
  if (event.data.success) {
    await autoCommit.commitChanges(
      event.data.files_changed,
      event.data.summary
    );
  }
});
```

### 8. Renderer Updates UI
```typescript
// Editor.tsx
useEffect(() => {
  const unsub = window.teddy.onFileChanged((info) => {
    if (info.path === selectedFile) {
      loadFile(info.path);  // Reload file in editor
    }
  });
  return unsub;
}, [selectedFile]);
```

---

## Security Model

### Sandboxing

**Renderer Process**:
- `contextIsolation: true` - Separate JS contexts
- `nodeIntegration: false` - No Node.js in renderer
- Only IPC API exposed via `contextBridge`

**Main Process**:
- Validates all file paths against project root
- No arbitrary code execution from events
- Git commits are safe (no shell injection)

### File Operations

All file paths are validated:

```typescript
private resolvePath(relativePath: string): string {
  const resolved = path.resolve(this.projectRoot, relativePath);

  // Security check: ensure within project root
  if (!resolved.startsWith(this.projectRoot)) {
    throw new Error(`Path escapes project root: ${relativePath}`);
  }

  return resolved;
}
```

### Ted Process

- Runs with same permissions as Teddy
- No elevated privileges
- Can only access files in project directory

---

## Future Enhancements

### V2 Features (Post-MVP)

1. **Container Runtime Manager**
   - Detect Docker Desktop, Podman, Colima
   - Start/stop containers
   - Database connection strings
   - `docker-compose` integration

2. **Smart Preview**
   - Auto-detect dev servers (Vite, Next.js, etc.)
   - Auto-start on project open
   - Hot reload on file changes

3. **Context Selection**
   - Send focused file + related imports
   - Tree-sitter for AST parsing
   - Summarization for large files

4. **Deployment**
   - One-click deploy to Vercel, Netlify
   - Cloudflare Tunnels integration
   - Environment variable management

5. **Diff View**
   - Show before/after for AI changes
   - Review mode before applying
   - Undo/redo for file operations

6. **Multi-agent**
   - Run multiple Ted instances
   - Background indexing agent
   - Test runner agent

### Performance Optimizations

1. **Ted Core Library**
   - Extract `ted_core` Rust crate
   - FFI bindings or native Node module
   - Eliminate startup overhead

2. **Incremental File Updates**
   - Delta encoding for large files
   - Binary diffs instead of full rewrites

3. **Virtual File System**
   - In-memory staging area
   - Batch file operations
   - Transactional commits

---

## Testing Strategy

### Unit Tests
- `TedParser` - JSONL parsing edge cases
- `FileApplier` - Path validation, operations
- `AutoCommit` - Git command generation

### Integration Tests
- End-to-end: Prompt → Ted → File changes
- Mock Ted process with fixture events
- Verify file system changes

### E2E Tests
- Electron spectron tests
- Full user flows (open project → chat → edit)
- Cross-platform compatibility

---

## Build & Packaging

### Development
```bash
npm run dev
```
- Vite dev server (React)
- Electron with hot reload
- Ted binary from `target/release/ted`

### Production
```bash
npm run build
```
- Vite production build
- electron-builder packaging
- Ted binary bundled in `extraResources`

### Packaging Details

**macOS**:
- DMG installer
- Code signing (future)
- Notarization (future)

**Windows**:
- NSIS installer
- Portable EXE
- Code signing (future)

**Linux**:
- AppImage (self-contained)
- deb package (Debian/Ubuntu)
- rpm package (future)

---

## Lessons Learned

### What Worked Well

1. **Subprocess approach** - Clean separation, easier to debug
2. **JSONL protocol** - Simple, proven, easy to extend
3. **Monorepo** - Faster iteration on protocol changes
4. **TypeScript** - Caught many bugs early
5. **Monaco Editor** - Professional experience out of the box

### Challenges

1. **Electron complexity** - Main/renderer split takes time to grok
2. **IPC boilerplate** - Lots of handler/listener code
3. **Type safety** - Hard to enforce across IPC boundary
4. **File watching** - Need to refresh file tree manually for now

### Would Do Differently

1. **State management** - Should use Zustand or Jotai
2. **Component library** - Build reusable design system first
3. **E2E tests** - Set up earlier in development
4. **Error boundaries** - Add React error boundaries from start

---

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for guidelines.

Priority areas:
- [ ] Container runtime detection
- [ ] Postgres integration
- [ ] Deploy adapters
- [ ] Diff view UI
- [ ] Context selection strategy

---

Built with ❤️ by the Ted team
