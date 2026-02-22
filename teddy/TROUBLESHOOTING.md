# Teddy Troubleshooting Guide

Common issues and solutions.

---

## Installation Issues

### ❌ `npm install` fails with EACCES

**Error**:
```
npm ERR! code EACCES
npm ERR! syscall access
```

**Solution**:
```bash
# Fix npm permissions (macOS/Linux)
sudo chown -R $(whoami) ~/.npm
sudo chown -R $(whoami) /usr/local/lib/node_modules

# Or use nvm (recommended)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
nvm install 20
```

### ❌ `cargo build` fails

**Error**:
```
error: linking with `cc` failed
```

**Solution**:
```bash
# macOS: Install Xcode Command Line Tools
xcode-select --install

# Linux: Install build essentials
sudo apt-get install build-essential

# Update Rust
rustup update
```

---

## Development Server Issues

### ❌ Port 5173 already in use

**Error**:
```
Port 5173 is in use, trying another one...
```

**Solution**:
```bash
# Find and kill process
lsof -ti:5173 | xargs kill

# Or change port in vite.config.ts:
server: {
  port: 5174  // Use different port
}
```

### ❌ Vite dev server won't start

**Error**:
```
failed to load config from vite.config.ts
```

**Solution**:
```bash
# Clear cache and reinstall
rm -rf node_modules package-lock.json
npm install

# Check Node version
node --version  # Should be 20+
```

### ❌ Electron window doesn't open

**Error**:
No window appears, or window crashes immediately.

**Solution**:
```bash
# Check terminal for errors
npm run dev 2>&1 | grep -i error

# Try safe mode (disable GPU)
npm run dev -- --disable-gpu

# Check Electron version
npx electron --version
```

---

## Ted Integration Issues

### ❌ Ted binary not found

**Error**:
```
Error: Ted binary not found at: target/release/ted
```

**Solution**:
```bash
# Build Ted first
cd ..  # Go to repo root
cargo build --release

# Verify binary exists
ls -lh target/release/ted

# Test it
./target/release/ted --version
```

### ❌ Ted process fails to start

**Error**:
```
spawn ted ENOENT
```

**Solution**:
```bash
# Check Ted binary path in development
# Should be: ../target/release/ted (relative to teddy/)

# Verify path in runner.ts:
const devBinary = path.join(__dirname, '../../target/release/ted');
console.log('Ted binary path:', devBinary);

# Make sure binary is executable
chmod +x ../target/release/ted
```

### ❌ No JSONL output from Ted

**Error**:
Ted runs but Teddy doesn't receive events.

**Solution**:
```bash
# Test Ted embedded mode manually
../target/release/ted chat --embedded "test" 2>/dev/null

# Should output JSONL lines like:
# {"type":"status",...}

# If no output, check Ted code for --embedded flag handling
```

---

## File Operation Issues

### ❌ File path escapes project root

**Error**:
```
Error: Path escapes project root: ../../etc/passwd
```

**Solution**:
This is a security feature. Ted is trying to access files outside the project.

**Fix**: Ensure Ted only references files within the project directory.

### ❌ Permission denied when writing files

**Error**:
```
EACCES: permission denied, open '/path/to/file'
```

**Solution**:
```bash
# Check folder permissions
ls -la /path/to/project

# Fix permissions (macOS/Linux)
chmod -R u+w /path/to/project

# Or choose a different project folder
```

---

## Git Integration Issues

### ❌ Git not initialized

**Error**:
```
fatal: not a git repository
```

**Solution**:
```bash
# Initialize git in project
cd /path/to/project
git init
git add .
git commit -m "Initial commit"
```

### ❌ Git commit fails

**Error**:
```
Author identity unknown
```

**Solution**:
```bash
# Configure git
git config --global user.name "Your Name"
git config --global user.email "you@example.com"
```

---

## UI Issues

### ❌ Monaco Editor not loading

**Error**:
Blank editor area or "Loading..." stuck.

**Solution**:
```bash
# Clear Monaco cache
rm -rf node_modules/@monaco-editor
npm install

# Check console for errors
# Open DevTools: Cmd+Option+I (macOS)
```

### ❌ File tree not showing files

**Error**:
File tree is empty despite files existing.

**Solution**:
```typescript
// Check console for errors
// Debug listFiles IPC call:
window.teddy.listFiles('.').then(console.log);

// Should return array of files
```

### ❌ Chat messages not appearing

**Error**:
Typed message doesn't show up.

**Solution**:
```typescript
// Check Ted events in console
window.teddy.onTedEvent((event) => {
  console.log('Ted event:', event);
});

// Verify events are flowing
```

---

## Build Issues

### ❌ electron-builder fails

**Error**:
```
Error: Application entry file "dist-electron/main.js" does not exist
```

**Solution**:
```bash
# Build Vite first
npm run build

# Check if main.js was created
ls -lh dist-electron/main.js

# If missing, run:
npx vite build
```

### ❌ DMG creation fails (macOS)

**Error**:
```
Error: Cannot create DMG
```

**Solution**:
```bash
# Disable code signing for testing
export CSC_IDENTITY_AUTO_DISCOVERY=false
npm run build

# Or install required tools
npm install -g dmg-creator
```

### ❌ Windows build fails on macOS

**Error**:
```
Error: wine is required
```

**Solution**:
```bash
# Install Wine (required for cross-platform builds)
brew install wine-stable

# Or build on Windows only
# (can't cross-compile Windows from macOS easily)
```

---

## Runtime Issues

### ❌ App crashes on startup

**Error**:
App window opens then immediately closes.

**Solution**:
```bash
# Run from terminal to see errors
./release/mac/Teddy.app/Contents/MacOS/Teddy

# Or check logs
# macOS: ~/Library/Logs/Teddy/
# Windows: %APPDATA%\Teddy\logs\
# Linux: ~/.config/Teddy/logs/
```

### ❌ High CPU usage

**Error**:
Teddy uses 100% CPU.

**Solution**:
```bash
# Check if Ted is stuck
ps aux | grep ted

# Kill stuck Ted processes
killall ted

# Restart Teddy
```

### ❌ Memory leak

**Error**:
Memory usage grows over time.

**Solution**:
```typescript
// Check for event listener leaks
// Make sure to cleanup listeners:
useEffect(() => {
  const unsub = window.teddy.onTedEvent(...);
  return unsub;  // Cleanup!
}, []);
```

---

## Local Provider Issues

### ❌ Failed to connect to local llama.cpp server

**Error**:
```
Error: connect ECONNREFUSED 127.0.0.1:8847
```

**Solution**:
```bash
# Check local provider is selected
./target/release/ted settings get provider

# Ensure a GGUF model exists
ls ~/.ted/models/local/model.gguf

# If missing, place one there or configure model_path in settings
```

### ❌ Model not found

**Error**:
```
Error: No GGUF model files found
```

**Solution**:
```bash
# Ensure model path exists
ls ~/.ted/models/local/model.gguf

# Or set a custom model path in settings (providers.local.model_path)
```

---

## Anthropic Integration Issues

### ❌ Invalid API key

**Error**:
```
Error: Invalid API key
```

**Solution**:
```bash
# Set API key
export ANTHROPIC_API_KEY="your-key-here"

# Verify it's set
echo $ANTHROPIC_API_KEY

# Restart Teddy
npm run dev
```

### ❌ Rate limit exceeded

**Error**:
```
Error: Rate limit exceeded
```

**Solution**:
```
Wait a few minutes, then retry.
Or switch to the local provider for offline usage.
```

---

## Platform-Specific Issues

### macOS

#### ❌ "Teddy.app is damaged and can't be opened"

**Solution**:
```bash
# Remove quarantine attribute
xattr -cr /Applications/Teddy.app

# Or allow in System Preferences
# System Preferences > Security & Privacy > Allow anyway
```

#### ❌ Command not found: npm

**Solution**:
```bash
# Install nvm
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash

# Install Node
nvm install 20
```

### Windows

#### ❌ "Windows protected your PC"

**Solution**:
1. Click "More info"
2. Click "Run anyway"

(App isn't code signed yet)

#### ❌ npm not recognized

**Solution**:
Download Node.js from https://nodejs.org/ and install.

### Linux

#### ❌ AppImage won't run

**Solution**:
```bash
# Make executable
chmod +x Teddy-*.AppImage

# Install FUSE (if missing)
sudo apt-get install libfuse2

# Run
./Teddy-*.AppImage
```

---

## Performance Issues

### ❌ Slow file tree with large projects

**Problem**:
File tree takes 10+ seconds to load.

**Solution**:
```typescript
// Future: Add pagination or virtual scrolling
// For now: Exclude large directories

// In FileTree.tsx, filter out:
const filtered = entries.filter(e => {
  return !['node_modules', 'dist', '.git'].includes(e.name);
});
```

### ❌ Editor lag with large files

**Problem**:
Monaco is slow with files > 1MB.

**Solution**:
```typescript
// Add file size limit in Editor.tsx:
if (file.size > 1024 * 1024) {
  setContent('File too large to edit');
  return;
}
```

---

## Debugging Tips

### Enable Verbose Logging

```bash
# Electron main process
export DEBUG=teddy:*

# Ted CLI
export RUST_LOG=debug

# Run with both
DEBUG=teddy:* RUST_LOG=debug npm run dev
```

### Inspect IPC Messages

```typescript
// In renderer (DevTools console)
window.teddy.runTed('test').then(console.log);

// Watch all Ted events
window.teddy.onTedEvent(console.log);
```

### Check File System

```bash
# See what files Ted created
find . -type f -newermt '5 minutes ago'

# Watch file changes in real-time
fswatch -r /path/to/project
```

### Network Issues

```bash
# Check if local server is reachable (during an active local session)
curl http://127.0.0.1:8847/health

# Check Anthropic API
curl https://api.anthropic.com/v1/messages \
  -H "x-api-key: $ANTHROPIC_API_KEY"
```

---

## Getting Help

### Before Asking

1. **Check this guide** for your issue
2. **Search GitHub issues** for similar problems
3. **Check console logs** (DevTools + terminal)
4. **Try clean reinstall**:
   ```bash
   rm -rf node_modules dist dist-electron
   npm install
   ```

### When Reporting Bugs

Include:
1. **OS and version** (e.g., macOS 14.0)
2. **Node version**: `node --version`
3. **npm version**: `npm --version`
4. **Rust version**: `cargo --version`
5. **Error messages** (full output)
6. **Steps to reproduce**

### Where to Get Help

- **GitHub Issues**: https://github.com/blackman-ai/ted/issues
- **Discussions**: https://github.com/blackman-ai/ted/discussions
- **Documentation**: [README.md](README.md), [ARCHITECTURE.md](ARCHITECTURE.md)

---

## Common Solutions Summary

### The "Turn It Off and On Again" Approach

```bash
# Nuclear option: reset everything
killall Electron ted
rm -rf node_modules dist dist-electron release
cd .. && cargo clean && cargo build --release
cd teddy && npm install && npm run dev
```

### The "It Works On My Machine" Checklist

- [ ] Node.js 20+ installed
- [ ] Rust 1.70+ installed
- [ ] Ted binary built (`target/release/ted` exists)
- [ ] Dependencies installed (`node_modules` exists)
- [ ] No port conflicts (5173 is free)
- [ ] GGUF model present (if using local AI)
- [ ] API key set (if using Anthropic)
- [ ] Project folder has write permissions
- [ ] Git initialized (if using auto-commit)

---

**Pro Tip**: When in doubt, read the error message slowly and carefully. Most issues are self-explanatory once you know where to look.

**Remember**: This is an MVP. If you encounter a bug not listed here, it's probably new. Please report it!
