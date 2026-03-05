# Teddy

**Offline-first AI coding environment powered by Ted**

Teddy is a cross-platform desktop application that brings AI-assisted coding to everyone - developers and non-coders alike. Built on Electron, it provides a full-featured IDE experience with integrated AI capabilities, all running locally on your machine.

## Project Status

- Active roadmap and remaining work: `../docs/IMPROVEMENTS.md`
- Current user-visible limitations: `./MVP_LIMITATIONS.md`
- Architecture reference: `./ARCHITECTURE.md`

## Features

- 🧸 **Local-first AI** - Works offline with Ted's `local` llama.cpp provider (no API keys required)
- 📁 **File Management** - Browse, edit, and manage your project files
- ✍️ **Monaco Editor** - Professional code editor with syntax highlighting
- 💬 **AI Chat** - Natural language interface to generate and modify code
- 🔄 **Live Preview** - See your web apps running in real-time
- 📊 **Git Integration** - Automatic commits for every AI-generated change
- 🐳 **Docker + PostgreSQL** - Optional container management via Settings → Database
- 🚀 **One-Click Deploy** - Vercel/Netlify deploy from the Preview tab (tokens required)

## Prerequisites

### Required

- **Node.js 20+** - [Download](https://nodejs.org/)
- **Rust 1.70+** - [Install via rustup](https://rustup.rs/)

### Recommended

- **GGUF model file** - For local AI models (offline mode)
  - Place a model at `~/.ted/models/local/model.gguf`, or point Ted to one with `--model-path`
  - Ted manages the `llama-server` runtime automatically for the `local` provider

### Optional

- **Docker Desktop** - For PostgreSQL and container features
- **Anthropic API key** - For Claude models (if not using the local provider)

## Development Setup

### 1. Clone and Setup

```bash
# From the ted repository root
cd teddy
npm install
```

### 2. Build Ted CLI

Teddy depends on the Ted binary. Build it first:

```bash
# From the ted repository root (parent directory)
cd ..
cargo build --release
```

The binary will be at `target/release/ted`.

### 3. Run Teddy in Development

```bash
cd teddy
npm run dev
```

This will:
- Start the Vite dev server (React app)
- Launch Electron with hot-reload
- Open the Teddy window

### 4. Configure AI Provider

**Option A: Local llama.cpp (Offline, Recommended)**

1. Open Teddy Settings → Hardware
2. Click **Setup Local AI**
3. Teddy picks the best model for your hardware, downloads it, and configures everything automatically
4. Start chatting - Ted launches the local runtime when needed

**Option B: Anthropic (Cloud)**

1. Get an API key from [console.anthropic.com](https://console.anthropic.com/)
2. Set environment variable:
   ```bash
   export ANTHROPIC_API_KEY="your-key-here"
   ```

## Building for Production

### Build for your platform

```bash
npm run build
```

This creates a distributable package in `release/`:
- **macOS**: `Teddy-x.x.x.dmg`
- **Windows**: `Teddy Setup x.x.x.exe`
- **Linux**: `Teddy-x.x.x.AppImage`

### Build for specific platform

```bash
# macOS
npm run build -- --mac

# Windows (from macOS/Linux requires Wine)
npm run build -- --win

# Linux
npm run build -- --linux
```

## Project Structure

```
teddy/
├── electron/              # Electron main process (Node.js)
│   ├── main.ts           # App entry point, IPC handlers
│   ├── preload.ts        # Context bridge for renderer
│   ├── ted/              # Ted integration layer
│   │   ├── runner.ts     # Subprocess spawner
│   │   ├── parser.ts     # JSONL event parser
│   │   └── protocol.ts   # Type definitions
│   ├── operations/       # File operations
│   │   └── file-applier.ts
│   └── git/              # Git integration
│       └── auto-commit.ts
│
├── src/                  # React renderer (UI)
│   ├── App.tsx           # Main app component
│   ├── components/       # UI components
│   │   ├── FileTree.tsx
│   │   ├── Editor.tsx
│   │   ├── ChatPanel.tsx
│   │   ├── Preview.tsx
│   │   └── Console.tsx
│   ├── hooks/            # React hooks
│   │   ├── useTed.ts
│   │   └── useProject.ts
│   └── types/
│
├── package.json          # Dependencies and scripts
├── vite.config.ts        # Vite + Electron config
└── tsconfig.json         # TypeScript config
```

## How It Works

### Ted ↔ Teddy Integration

Teddy spawns Ted as a subprocess with the `--embedded` flag:

```
User types prompt in Teddy UI
      ↓
Teddy spawns: ted chat --embedded "Create a login page"
      ↓
Ted outputs JSONL events to stdout:
  {"type":"plan","data":{"steps":[...]}}
  {"type":"file_create","data":{"path":"login.tsx","content":"..."}}
  {"type":"completion","data":{"success":true,"summary":"Created login page"}}
      ↓
Teddy parses events and applies file operations
      ↓
File tree updates, editor opens new file
      ↓
Git auto-commits changes
```

### JSONL Protocol

Ted emits JSON events (one per line) when run with `--embedded`:

```jsonl
{"type":"status","timestamp":1705420800,"session_id":"abc","data":{"state":"thinking","message":"Planning..."}}
{"type":"plan","timestamp":1705420801,"session_id":"abc","data":{"steps":[{"id":"1","description":"Create component"}]}}
{"type":"file_create","timestamp":1705420802,"session_id":"abc","data":{"path":"src/App.tsx","content":"..."}}
{"type":"completion","timestamp":1705420803,"session_id":"abc","data":{"success":true,"summary":"Done","files_changed":["src/App.tsx"]}}
```

See [electron/types/protocol.ts](electron/types/protocol.ts) for the complete protocol specification.

## Hooks

Teddy supports lightweight hook rules around Ted file application and Share/Deploy actions.

### Hook events

- `BeforeApplyChanges` (blocking): runs before Teddy applies Ted-emitted file ops
- `AfterApplyChanges` (non-blocking): runs after file ops are applied
- `OnShare` (non-blocking): runs when user clicks Share
- `OnDeploy` (non-blocking): runs when user clicks Deploy

### Config locations

Hooks are loaded from these files if present:

- User-level: `~/.teddy/hooks.json`, `~/.ted/hooks.json`
- Project-level: `<project>/.teddy/hooks.json`, `<project>/.ted/hooks.json`

All matching rules are executed in file load order.

### Schema

```json
{
  "hooks": {
    "BeforeApplyChanges": [
      {
        "name": "optional label",
        "enabled": true,
        "matcher": {
          "op": "file_edit|file_create",
          "path": "src/.*",
          "target": "vercel"
        },
        "actions": [
          {
            "type": "command",
            "command": "node .teddy/hooks/check.js",
            "commands": {
              "darwin": "node .teddy/hooks/check.js",
              "linux": "node .teddy/hooks/check.js",
              "win32": "node .teddy/hooks/check.js"
            },
            "timeoutMs": 30000,
            "cwd": ".",
            "env": {
              "CI": "1"
            }
          }
        ]
      }
    ],
    "AfterApplyChanges": [],
    "OnShare": [],
    "OnDeploy": []
  }
}
```

Matcher fields support exact string or regex. Regex can be given as:

- `"re:^src/.*\\.ts$"`
- `"/^src\\/.*\\.ts$/"` (slash-delimited)
- `"src/.*"` (auto-treated as regex when regex meta chars are present)

`matcher.target` values:

- `vercel` / `netlify` for `OnDeploy`
- `custom` for current built-in `OnShare` flow

### Action contract

For `type=command`, Teddy sends JSON payload to stdin. Hook scripts should print JSON to stdout.

`type=node` is also supported for local module handlers:

```json
{
  "type": "node",
  "module": ".teddy/hooks/handler.js",
  "exportName": "default",
  "timeoutMs": 30000
}
```

Blocking response (`BeforeApplyChanges`):

```json
{ "decision": "allow" }
```

```json
{ "decision": "deny", "reason": "Blocked: protected path" }
```

```json
{
  "decision": "ask",
  "reason": "Potential secret file edit",
  "updatedOps": []
}
```

Non-blocking response (`AfterApplyChanges`, `OnShare`, `OnDeploy`):

```json
{
  "message": "Lint/test completed",
  "url": "https://example.com",
  "artifacts": { "buildId": "123" }
}
```

### Examples

1. Block or ask before touching `.env`/secrets paths

```json
{
  "hooks": {
    "BeforeApplyChanges": [
      {
        "name": "protect-secrets",
        "matcher": {
          "op": "file_(create|edit|delete)",
          "path": "(^|/)(\\.env(\\.|$)|secrets?/|credentials?)"
        },
        "actions": [
          {
            "type": "command",
            "command": "node .teddy/hooks/confirm-secrets.js"
          }
        ]
      }
    ]
  }
}
```

2. Post-apply formatting/tests/autocommit

```json
{
  "hooks": {
    "AfterApplyChanges": [
      {
        "name": "quality-gate",
        "matcher": {
          "path": "src/.*"
        },
        "actions": [
          {
            "type": "command",
            "command": "npm run format && npm run lint && npm test && git add -A && git commit -m \"chore: teddy hook auto-commit\""
          }
        ]
      }
    ]
  }
}
```

3. Deploy hook (build + provider CLI)

```json
{
  "hooks": {
    "OnDeploy": [
      {
        "name": "vercel-prod",
        "matcher": {
          "target": "vercel"
        },
        "actions": [
          {
            "type": "command",
            "command": "node .teddy/hooks/deploy-vercel.js"
          }
        ]
      }
    ]
  }
}
```

Inside `deploy-vercel.js`, run `npm run build` and `vercel --prod`, then emit JSON including `url`.

### Security notes

- Teddy executes only hook commands from local config files, never from Ted model output.
- Review and version-control your hook scripts/config before enabling them.
- Keep hook commands least-privilege; avoid broad shell scripts when a narrow command works.

## Troubleshooting

### Ted binary not found

**Error**: `Ted binary not found at: target/release/ted`

**Solution**: Build Ted first from the repository root:
```bash
cd ..
cargo build --release
```

### Local provider connection failed

**Error**: `Failed to connect to local provider`

**Solution**: Ensure local provider is selected and a GGUF model is available:
```bash
./target/release/ted settings get provider
ls ~/.ted/models/local/model.gguf
```

### Port already in use

**Error**: `Port 5173 already in use`

**Solution**: Kill the process using that port:
```bash
lsof -ti:5173 | xargs kill
```

Or change the port in `vite.config.ts`.

## Development Tips

### Hot Reload

Changes to React components (`src/`) hot-reload instantly. Changes to Electron main process (`electron/`) require restarting the app (Ctrl+C and `npm run dev` again).

### Debugging

**Renderer (React)**:
- Open DevTools: `Cmd+Option+I` (macOS) or `Ctrl+Shift+I` (Windows/Linux)
- Or automatically opened in dev mode

**Main Process (Electron)**:
- Add `console.log()` in `electron/main.ts`
- Logs appear in the terminal where you ran `npm run dev`

**Ted Integration**:
- Ted stderr is forwarded to the Console panel
- Check the Console tab in Teddy to see Ted's output

### Adding New Event Types

1. Add type to `electron/types/protocol.ts`
2. Update parser in `electron/ted/parser.ts`
3. Handle event in `electron/main.ts`
4. Display in React components

## Contributing

We welcome contributions. Current priority areas:

- [ ] Local-model tool-call fallback robustness for ambiguous model output
- [ ] Renderer/electron automated coverage beyond parser tests
- [ ] UX polish for review/apply and error-edge handling
- [ ] Embedded diagnostics and correlation improvements across Ted/Teddy events

## License

AGPL-3.0-or-later

Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

---

Built with ❤️ using Electron, React, and the power of Ted.
