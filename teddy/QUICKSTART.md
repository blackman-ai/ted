# Teddy Quick Start Guide

Get Teddy running in under 5 minutes.

## Prerequisites Check

Before starting, verify you have:

```bash
# Node.js 20+
node --version  # Should be v20.x.x or higher

# npm
npm --version

# Rust & Cargo
cargo --version  # Should be 1.70+

# Ollama (optional but recommended)
ollama --version
```

If missing, install:
- **Node.js**: https://nodejs.org/
- **Rust**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Ollama**: `curl -fsSL https://ollama.com/install.sh | sh`

---

## Step 1: Install Dependencies

```bash
cd teddy
npm install
```

This will install:
- Electron
- Vite
- React
- Monaco Editor
- TypeScript
- And all other dependencies

**Expected output**: `added XXX packages` (takes ~1-2 minutes)

---

## Step 2: Build Ted CLI

Teddy needs the Ted binary to function. Build it:

```bash
# From the teddy directory, go up to repo root
cd ..

# Build Ted in release mode
cargo build --release
```

**Expected output**:
```
   Compiling ted v0.1.1
    Finished release [optimized] target(s) in 2m 34s
```

The binary will be at: `target/release/ted`

Verify it works:
```bash
./target/release/ted --version
# Should output: ted 0.1.1
```

---

## Step 3: Setup Ollama (Recommended)

For offline AI, use Ollama:

```bash
# Start Ollama server (if not already running)
ollama serve &

# Pull a coding model (14B parameters, ~8GB download)
ollama pull qwen2.5-coder:14b
```

**Alternative models**:
- `qwen2.5-coder:7b` - Smaller, faster (4GB)
- `deepseek-coder-v2:latest` - Strong coding model (16GB)

**Skip this step** if you have an Anthropic API key:
```bash
export ANTHROPIC_API_KEY="your-key-here"
```

---

## Step 4: Run Teddy

```bash
cd teddy
npm run dev
```

**Expected behavior**:
1. Terminal shows: `VITE vx.x.x ready in XXX ms`
2. Electron window opens with Teddy
3. You see the project picker screen

**Troubleshooting**:

❌ **Port 5173 already in use**
```bash
lsof -ti:5173 | xargs kill
npm run dev
```

❌ **Electron doesn't launch**
- Check for errors in terminal
- Try: `rm -rf node_modules && npm install`

---

## Step 5: Open a Project

1. Click "Open Project Folder"
2. Select a folder (or create a new one for testing)
3. Teddy loads the file tree

**Test Project** (if you don't have one):
```bash
mkdir ~/teddy-test
cd ~/teddy-test
echo "console.log('Hello');" > index.js
```

Then open `~/teddy-test` in Teddy.

---

## Step 6: Chat with Ted

1. Type in the chat panel (bottom right):
   ```
   Add a README file explaining this project
   ```

2. Press **Send** (or Enter)

3. Watch Ted:
   - Show a plan
   - Create `README.md`
   - Update the file tree
   - Auto-commit to Git

4. Click on `README.md` in the file tree to see it in the editor

---

## Step 7: Edit and Preview

### Edit a File

1. Select `index.js` from file tree
2. Edit in Monaco editor
3. Click **Save**

### Preview (for web projects)

1. Click the **Preview** tab
2. Start your dev server manually:
   ```bash
   cd ~/teddy-test
   npx vite
   ```
3. Enter URL in preview toolbar: `http://localhost:5173`
4. Click refresh to load

---

## Common Tasks

### Change AI Provider

**To Ollama**:
```typescript
// In ChatPanel, modify options:
onSendMessage(prompt, {
  provider: 'ollama',
  model: 'qwen2.5-coder:14b'
});
```

**To Anthropic**:
```bash
export ANTHROPIC_API_KEY="your-key"
```
```typescript
onSendMessage(prompt, {
  provider: 'anthropic',
  model: 'claude-sonnet-4-20250514'
});
```

### View Console Logs

Look at the bottom panel (Console tab) to see:
- Ted's stderr output
- File change notifications
- Git commit messages

### Stop Ted

If Ted is stuck:
1. Close Teddy
2. Terminal: `killall ted`
3. Restart Teddy

---

## Development Workflow

### Make Changes to UI

1. Edit files in `src/`:
   - Components: `src/components/*.tsx`
   - Hooks: `src/hooks/*.ts`
   - Styles: `src/**/*.css`

2. Hot reload happens automatically
   - Changes appear in ~100ms

### Make Changes to Main Process

1. Edit files in `electron/`:
   - Main: `electron/main.ts`
   - Ted integration: `electron/ted/*.ts`

2. Restart Teddy:
   - Ctrl+C in terminal
   - `npm run dev` again

### Make Changes to Ted

1. Edit Ted source: `src/` (repo root)
2. Rebuild Ted: `cargo build --release`
3. Restart Teddy

---

## Building for Distribution

### Test the packaged app

```bash
npm run build:dir
```

This creates an unpackaged app in `release/`:
- **macOS**: `release/mac/Teddy.app`
- **Windows**: `release/win-unpacked/Teddy.exe`
- **Linux**: `release/linux-unpacked/teddy`

Run it to test:
```bash
# macOS
open release/mac/Teddy.app

# Linux
./release/linux-unpacked/teddy
```

### Create installer

```bash
npm run build
```

Installers in `release/`:
- **macOS**: `Teddy-0.1.0.dmg`
- **Windows**: `Teddy Setup 0.1.0.exe`
- **Linux**: `Teddy-0.1.0.AppImage`

---

## Debugging

### Enable DevTools

DevTools are auto-opened in dev mode. To open manually:

**macOS**: `Cmd+Option+I`
**Windows/Linux**: `Ctrl+Shift+I`

### Check Ted Output

1. Open Console panel in Teddy
2. Or check terminal where you ran `npm run dev`

### Verbose Ted Logging

```bash
# In terminal before running Teddy:
export RUST_LOG=debug
npm run dev
```

### Check File Operations

```bash
# Watch Ted's actions in real-time:
tail -f ~/.ted/wal/*.wal
```

---

## Next Steps

Now that Teddy is running:

1. **Read the Architecture** - [ARCHITECTURE.md](./ARCHITECTURE.md)
2. **Explore the Code** - Start with `src/App.tsx`
3. **Customize the UI** - Edit components and styles
4. **Add Features** - See `ROADMAP.md` and `MVP_LIMITATIONS.md` for ideas
5. **Contribute** - Open a PR!

---

## Getting Help

**Issues**: Open a GitHub issue with:
- OS and version
- Node/npm versions
- Error messages
- Steps to reproduce

**Questions**: Start a discussion on GitHub

**Logs**: Attach console output when reporting issues

---

Happy coding! 🧸
