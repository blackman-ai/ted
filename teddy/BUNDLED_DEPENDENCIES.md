# Bundled Dependencies in Teddy

Teddy includes a smart dependency management system that automatically handles external binaries like `cloudflared` and `ollama`, providing a true batteries-included experience.

## Philosophy

**Users should never need to install dependencies manually.** Teddy automatically:
1. Checks for bundled binaries in the app package
2. Falls back to user's local `.teddy/bin` directory
3. Checks system installations (Homebrew, winget, etc.)
4. Offers to auto-download missing dependencies
5. Only shows manual installation instructions as a last resort

## Architecture

### Bundled Dependencies Manager

Located at: [`electron/bundled/manager.ts`](electron/bundled/manager.ts)

**Key Functions:**
- `getCloudflaredPath()` - Find cloudflared binary (bundled, local, or system)
- `downloadCloudflared()` - Auto-download from GitHub releases
- `isCloudflaredInstalled()` - Check if available
- `getOllamaPath()` - Find Ollama installation
- `getInstallInstructions()` - Fallback manual instructions

### Search Order

When looking for a binary:

1. **Bundled** - `<app>/resources/bin/cloudflared` (production)
2. **Local** - `~/.teddy/bin/cloudflared` (auto-downloaded)
3. **System** - `/usr/local/bin/cloudflared`, `/opt/homebrew/bin/cloudflared`, etc.

### Storage Locations

**Production (packaged app):**
```
Teddy.app/
  Contents/
    Resources/
      bin/
        cloudflared       # Bundled with app
        ted               # Rust CLI binary
```

**Development:**
```
ted/
  bundled-bin/          # Local development binaries
    cloudflared
```

**User-installed (auto-download):**
```
~/.teddy/
  bin/
    cloudflared         # Auto-downloaded
  settings.json
  conversations/
```

## Cloudflared Integration

### Auto-Download

When user clicks "Share" for the first time:

1. Check if cloudflared is installed
2. If not, prompt: "Would you like Teddy to download and install it automatically?"
3. If yes, download from GitHub releases
4. Install to `~/.teddy/bin/cloudflared`
5. Make executable (Unix)
6. Start tunnel immediately

**Download URLs** (based on platform/arch):
- macOS ARM64: `cloudflared-darwin-arm64.tgz`
- macOS x64: `cloudflared-darwin-amd64.tgz`
- Linux ARM64: `cloudflared-linux-arm64`
- Linux x64: `cloudflared-linux-amd64`
- Windows x64: `cloudflared-windows-amd64.exe`

### User Experience

**Before (manual install required):**
```
User clicks "Share"
→ Alert: "Install cloudflared with: brew install cloudflared"
→ User exits app, runs brew, returns
→ Clicks "Share" again
→ Works
```

**After (batteries included):**
```
User clicks "Share"
→ Confirm: "Download cloudflared automatically?"
→ Status: "Downloading cloudflared..."
→ Status: "✓ cloudflared installed"
→ Status: "✓ Sharing at: https://..."
→ Works immediately
```

## Ollama Integration

### Future Implementation

Ollama is larger (~500MB) and has platform-specific installers, so we:

1. Check if Ollama is already installed (very common on dev machines)
2. If not, offer to:
   - **macOS**: Open Ollama.com download page
   - **Linux**: Run official install script
   - **Windows**: Open Ollama.com download page
3. Provide clear instructions and links

### Why Not Bundle Ollama?

- Size: Ollama is ~500MB, would bloat app significantly
- Updates: Ollama updates frequently, bundling would require constant re-packaging
- Native installers: Ollama provides polished native installers
- Common: Many developers already have Ollama installed

## IPC Communication

### Handlers (main.ts)

```typescript
ipcMain.handle('tunnel:isInstalled', async () => {
  const installed = bundled.isCloudflaredInstalled();
  return { installed };
});

ipcMain.handle('tunnel:autoInstall', async () => {
  const result = await bundled.downloadCloudflared();
  return { success: true, path: result };
});
```

### API (preload.ts)

```typescript
interface TeddyAPI {
  tunnelIsInstalled: () => Promise<{ installed: boolean }>;
  tunnelAutoInstall: () => Promise<{ success: boolean; path?: string; error?: string }>;
  tunnelGetInstallInstructions: () => Promise<{ instructions: string }>;
}
```

### Usage (Preview.tsx)

```typescript
const { installed } = await window.teddy.tunnelIsInstalled();

if (!installed) {
  const shouldInstall = confirm('Download cloudflared automatically?');
  if (shouldInstall) {
    await window.teddy.tunnelAutoInstall();
  }
}
```

## Build System Integration

### Electron Builder Config

```json
{
  "extraResources": [
    {
      "from": "bundled-bin/",
      "to": "bin/",
      "filter": ["cloudflared", "cloudflared.exe", "ted", "ted.exe"]
    }
  ]
}
```

### Pre-Build Script

To bundle cloudflared with the app (optional):

```bash
#!/bin/bash
# Download cloudflared for all platforms

mkdir -p bundled-bin

# macOS ARM64
curl -L https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-darwin-arm64.tgz \
  | tar xz -C bundled-bin

# Linux x64
curl -L -o bundled-bin/cloudflared-linux \
  https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64
chmod +x bundled-bin/cloudflared-linux
```

## Error Handling

### Download Failures

If auto-download fails:
1. Show specific error message
2. Offer manual installation instructions
3. Log error for debugging
4. Don't crash the app

```typescript
try {
  const result = await window.teddy.tunnelAutoInstall();
  if (!result.success) {
    alert(`Failed to install: ${result.error}\n\nPlease install manually.`);
  }
} catch (err) {
  console.error('Auto-install error:', err);
  // Fall back to manual instructions
}
```

### Permission Issues

If user doesn't have write access to `~/.teddy/bin`:
1. Detect the error
2. Suggest alternative: system installation with `sudo`
3. Provide clear error message

## Testing

### Manual Testing

```bash
# Remove installed binaries
rm -rf ~/.teddy/bin

# Start Teddy in dev mode
cd teddy && pnpm dev

# Try sharing a preview
# Should prompt for auto-download
```

### Verify Search Order

```typescript
// In browser console (Teddy dev tools)
await window.teddy.tunnelIsInstalled()
// Should check: bundled → local → system
```

## Security Considerations

1. **Official Sources Only**: Download only from official GitHub releases
2. **HTTPS**: Use HTTPS for all downloads
3. **Verify**: Could add SHA256 checksum verification (future enhancement)
4. **User Consent**: Always ask before downloading
5. **Sandboxing**: Downloaded binaries run in same sandbox as app

## Future Enhancements

### Planned

- [ ] SHA256 checksum verification for downloads
- [ ] Version checking and auto-updates
- [ ] Ollama auto-install for Linux
- [ ] Bundle common model files (1.5B models for offline use)
- [ ] Dependency status UI in Settings

### Ideas

- Pre-download popular models on first launch (with consent)
- Smart model recommendations based on hardware tier
- P2P model distribution (IPFS/BitTorrent) for faster downloads
- Offline model marketplace

## Troubleshooting

**Auto-download not working:**
1. Check internet connection
2. Check ~/.teddy/bin exists and is writable
3. Try manual download: `curl -L <url> -o ~/.teddy/bin/cloudflared`
4. Check console for error messages

**Bundled binary not found:**
1. Verify `extraResources` in package.json
2. Check bundled-bin/ has files before build
3. Inspect built app: `Teddy.app/Contents/Resources/bin/`

**Permission denied:**
```bash
chmod +x ~/.teddy/bin/cloudflared
```

## See Also

- [Cloudflare Tunnel Integration](../electron/deploy/cloudflare-tunnel.ts)
- [Bundled Manager](../electron/bundled/manager.ts)
- [Preview Component](../src/components/Preview.tsx)
- [ARM64 Build Guide](./BUILD_ARM64.md)
