# Teddy

**Offline-first AI coding environment powered by Ted**

Teddy is a cross-platform desktop application that brings AI-assisted coding to everyone - developers and non-coders alike. Built on Electron, it provides a full-featured IDE experience with integrated AI capabilities, all running locally on your machine.

## Features

- 🧸 **Local-first AI** - Works offline with Ollama (no API keys required)
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

- **Ollama** - For local AI models (offline mode)
  - macOS/Linux: `curl -fsSL https://ollama.com/install.sh | sh`
  - Or download from [ollama.ai](https://ollama.ai)
  - Pull a model: `ollama pull qwen2.5-coder:14b`

### Optional

- **Docker Desktop** - For PostgreSQL and container features
- **Anthropic API key** - For Claude models (if not using Ollama)

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

**Option A: Ollama (Offline, Recommended)**

1. Install Ollama: `curl -fsSL https://ollama.com/install.sh | sh`
2. Pull a model: `ollama pull qwen2.5-coder:14b`
3. Start Ollama: `ollama serve` (or launch the desktop app)
4. Teddy will auto-detect Ollama

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

## Troubleshooting

### Ted binary not found

**Error**: `Ted binary not found at: target/release/ted`

**Solution**: Build Ted first from the repository root:
```bash
cd ..
cargo build --release
```

### Ollama connection failed

**Error**: `Failed to connect to Ollama`

**Solution**: Ensure Ollama is running:
```bash
ollama serve
```

Or launch the Ollama desktop app.

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

We welcome contributions! This is an early MVP. Priority areas:

- [ ] Docker runtime detection and management
- [ ] Auto-detect and start dev servers
- [ ] Better file tree (search, multi-select)
- [ ] Diff view for AI changes
- [ ] PostgreSQL integration
- [ ] Deploy integrations (Vercel, Netlify, etc.)

## License

AGPL-3.0-or-later

Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

---

Built with ❤️ using Electron, React, and the power of Ted.
