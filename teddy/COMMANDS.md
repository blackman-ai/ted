# Teddy Command Reference

Quick reference for all commands and scripts.

---

## Development Commands

### Start Development Server
```bash
npm run dev
```
Starts Vite dev server + Electron with hot reload.

### Type Check
```bash
npm run type-check
```
Run TypeScript compiler without emitting files.

### Lint Code
```bash
npm run lint
```
Run ESLint on all TypeScript files.

---

## Build Commands

### Build for Current Platform
```bash
npm run build
```
Creates distributable installer in `release/`.

### Build Directory Only (No Installer)
```bash
npm run build:dir
```
Creates unpackaged app in `release/` (faster, for testing).

### Platform-Specific Builds

**macOS**:
```bash
npm run build -- --mac
```
Creates: `.dmg` and `.zip`

**Windows**:
```bash
npm run build -- --win
```
Creates: `.exe` installer and portable `.exe`

**Linux**:
```bash
npm run build -- --linux
```
Creates: `.AppImage` and `.deb`

---

## Ted Commands (From Root)

### Build Ted CLI
```bash
cargo build --release
```
Binary at: `target/release/ted`

### Run Ted in Embedded Mode
```bash
./target/release/ted chat --embedded "Your prompt here"
```
Outputs JSONL events to stdout.

### Test Ted JSONL Output
```bash
./target/release/ted chat --embedded "Create a test file" 2>/dev/null | jq
```
Pretty-print JSON events (requires `jq`).

---

## Testing Commands

### Run Unit Tests (Future)
```bash
npm test
```

### Run E2E Tests (Future)
```bash
npm run test:e2e
```

---

## Utility Commands

### Clean Build Artifacts
```bash
rm -rf dist dist-electron release node_modules
npm install
```

### Clean Everything (Including Dependencies)
```bash
rm -rf dist dist-electron release node_modules package-lock.json
npm install
```

### Kill Processes
```bash
# Kill Vite dev server
lsof -ti:5173 | xargs kill

# Kill all Electron processes
killall Electron

# Kill all Ted processes
killall ted
```

### View Ted Logs
```bash
# View Ted's context store
ls -lah ~/.ted/wal/

# View recent sessions
ls -lt ~/.ted/sessions/

# Tail Ted logs (if logging to file)
tail -f ~/.ted/debug.log
```

---

## Package Management

### Install Dependencies
```bash
npm install
```

### Update Dependencies
```bash
npm update
```

### Audit Dependencies
```bash
npm audit
npm audit fix
```

### Add New Dependency
```bash
# Runtime dependency
npm install <package>

# Dev dependency
npm install -D <package>
```

---

## Git Commands

### Initialize Git (If Needed)
```bash
git init
git add .
git commit -m "Initial Teddy implementation"
```

### Commit Changes
```bash
git add teddy/
git commit -m "Add Teddy desktop app"
```

### View Teddy-Related Changes
```bash
git log --oneline -- teddy/
```

---

## Debugging Commands

### Enable Verbose Logging
```bash
export DEBUG=teddy:*
npm run dev
```

### Enable Ted Debug Logging
```bash
export RUST_LOG=debug
npm run dev
```

### Open DevTools Automatically
```bash
# Already enabled in vite.config.ts for dev mode
npm run dev
```

### Check Electron Version
```bash
npx electron --version
```

### Check Build Configuration
```bash
cat package.json | jq '.build'
```

---

## macOS-Specific Commands

### Open Built App
```bash
open release/mac/Teddy.app
```

### Sign App (Future)
```bash
codesign --sign "Developer ID" release/mac/Teddy.app
```

### Notarize App (Future)
```bash
xcrun altool --notarize-app --file release/Teddy.dmg
```

---

## Windows-Specific Commands

### Open Built App
```bash
start release\win-unpacked\Teddy.exe
```

### Sign App (Future)
```powershell
signtool sign /f cert.pfx release\Teddy.exe
```

---

## Linux-Specific Commands

### Run AppImage
```bash
chmod +x release/Teddy-*.AppImage
./release/Teddy-*.AppImage
```

### Install deb Package
```bash
sudo dpkg -i release/teddy_*.deb
```

---

## Environment Variables

### Development
```bash
# Enable debug mode
export DEBUG=teddy:*

# Set Ted binary path (if not using default)
export TED_BINARY_PATH=/path/to/ted

# Use Anthropic instead of local provider
export ANTHROPIC_API_KEY="your-key"
```

### Production Build
```bash
# Skip code signing (faster builds)
export CSC_IDENTITY_AUTO_DISCOVERY=false

# Set app version
export npm_package_version="1.0.0"
```

---

## Troubleshooting Commands

### Reset Everything
```bash
# Kill all processes
killall Electron ted

# Clean all build artifacts
rm -rf dist dist-electron release node_modules

# Reinstall
npm install

# Rebuild Ted
cd .. && cargo build --release && cd teddy

# Restart dev server
npm run dev
```

### Check for Port Conflicts
```bash
# Check what's using port 5173
lsof -i:5173

# Kill process on port 5173
lsof -ti:5173 | xargs kill -9
```

### Verify Ted Binary
```bash
# Check if binary exists
ls -lh ../target/release/ted

# Test Ted
../target/release/ted --version

# Test embedded mode
echo '{"test": "prompt"}' | ../target/release/ted chat --embedded "test"
```

---

## CI/CD Commands (Future)

### GitHub Actions Build
```bash
# Simulate CI build
npm ci
npm run type-check
npm run lint
npm run build
```

### Release Build
```bash
# Clean build for release
rm -rf dist dist-electron release
npm ci --production
npm run build
```

---

## Performance Profiling

### Bundle Analysis
```bash
# Analyze Vite bundle
npm run build -- --report
```

### Electron Performance
```bash
# Enable performance monitoring
export ELECTRON_ENABLE_LOGGING=1
npm run dev
```

---

## Useful Aliases

Add to your `~/.bashrc` or `~/.zshrc`:

```bash
# Quick Teddy commands
alias teddy-dev='cd ~/path/to/ted/teddy && npm run dev'
alias teddy-build='cd ~/path/to/ted/teddy && npm run build'
alias teddy-clean='cd ~/path/to/ted/teddy && rm -rf dist dist-electron release'

# Ted commands
alias ted-build='cd ~/path/to/ted && cargo build --release'
alias ted-test='cd ~/path/to/ted && cargo test'
```

---

## Quick Reference Card

```
┌─────────────────────────────────────────────┐
│  TEDDY QUICK COMMANDS                       │
├─────────────────────────────────────────────┤
│  Start Dev:       npm run dev               │
│  Build App:       npm run build             │
│  Build Ted:       cargo build --release     │
│  Type Check:      npm run type-check        │
│  Lint:            npm run lint              │
│  Clean:           rm -rf dist release       │
│  Kill Port:       lsof -ti:5173 | xargs kill│
└─────────────────────────────────────────────┘
```

---

**Tip**: Bookmark this file for quick command lookup!
