# ARM64 Support for Teddy

Teddy now supports ARM64 architecture, enabling it to run on Raspberry Pi 5 and other ARM64 Linux systems.

## Quick Start

### For End Users

Download the ARM64 build from the [releases page](https://github.com/blackman-ai/ted/releases):

**Debian/Ubuntu (including Raspberry Pi OS)**:
```bash
wget https://github.com/blackman-ai/ted/releases/latest/download/teddy_<version>_arm64.deb
sudo dpkg -i teddy_<version>_arm64.deb
```

**AppImage (Universal Linux)**:
```bash
wget https://github.com/blackman-ai/ted/releases/latest/download/teddy-<version>-arm64.AppImage
chmod +x teddy-<version>-arm64.AppImage
./teddy-<version>-arm64.AppImage
```

### For Developers

See [BUILD_ARM64.md](./BUILD_ARM64.md) for detailed build instructions.

Quick build commands:

```bash
# Cross-compile from x64 Linux
npm run build:linux:arm64

# Native build on ARM64
npm run build
```

## What's Included

The ARM64 build includes:

1. **Teddy Electron App** (ARM64)
   - Full Monaco code editor
   - Live preview with dev server
   - Settings management
   - One-click Vercel deployment
   - Cloudflare Tunnel sharing

2. **Ted CLI** (ARM64)
   - Full terminal interface
   - All 11 built-in tools
   - MCP server support
   - Hardware detection
   - Multi-provider AI support

## Hardware Requirements

### Minimum (UltraTiny Tier)
- **Device**: Raspberry Pi 5 (8GB)
- **Storage**: 32GB+ microSD or NVMe SSD
- **OS**: Raspberry Pi OS 64-bit (Bookworm)

### Recommended
- **Device**: Raspberry Pi 5 (8GB)
- **Storage**: 256GB NVMe SSD via M.2 HAT
- **OS**: Raspberry Pi OS 64-bit (latest)
- **Cooling**: Active cooling fan or heatsink

## Performance

On Raspberry Pi 5 (8GB):

| Task | Performance |
|------|-------------|
| UI responsiveness | Smooth |
| Code editing | Instant |
| AI responses (1.5B model) | 15-45 seconds |
| AI responses (3B+ model) | Not recommended |
| Project builds (Vite) | 5-15 seconds |
| Project builds (Next.js) | 15-30 seconds |

**Recommended AI Models for Pi 5**:
- qwen2.5-coder:1.5b (best balance)
- deepseek-coder:1.3b (faster)
- tinyllama:1.1b (fastest)

## Build System

The build system consists of:

### 1. Cross-Compilation Script
[`scripts/build-rust-cli.sh`](./scripts/build-rust-cli.sh)
- Detects target architecture
- Uses `cross` for ARM64 cross-compilation
- Falls back to native builds when possible
- Handles platform-specific binary names

### 2. Electron Builder Config
[`package.json`](./package.json) - `build.linux` section
- Builds both x64 and ARM64 packages
- Generates .deb and AppImage formats
- Includes Rust CLI binary as extraResource

### 3. GitHub Actions Workflow
[`.github/workflows/teddy-release.yml`](../.github/workflows/teddy-release.yml)
- Automated builds for all platforms
- ARM64 cross-compilation on x64 runners
- Uploads release artifacts
- Creates GitHub releases

## CI/CD

### Triggering a Release

```bash
# Tag the release
git tag teddy-v0.1.0

# Push the tag
git push origin teddy-v0.1.0
```

GitHub Actions will automatically:
1. Build ARM64 Linux packages (.deb, AppImage)
2. Build x64 Linux packages
3. Build macOS packages (x64, ARM64)
4. Build Windows packages
5. Generate SHA256 checksums
6. Create GitHub release with all artifacts

### Manual Testing

Before creating a release tag, test locally:

```bash
# Test cross-compilation
npm run build:linux:arm64

# Check the output
ls -lh release/
```

## Architecture Overview

```
┌─────────────────────────────────────────┐
│          Teddy (Electron)               │
│  ┌───────────────────────────────────┐  │
│  │   Renderer Process (React)        │  │
│  │   - Monaco Editor                 │  │
│  │   - Preview with iframe           │  │
│  │   - Settings UI                   │  │
│  └───────────────────────────────────┘  │
│  ┌───────────────────────────────────┐  │
│  │   Main Process (Node.js)          │  │
│  │   - IPC handlers                  │  │
│  │   - File system access            │  │
│  │   - Shell commands                │  │
│  │   - Deployment (Vercel/Tunnel)    │  │
│  │   - Settings storage              │  │
│  └───────────────────────────────────┘  │
│  ┌───────────────────────────────────┐  │
│  │   Ted CLI (Rust binary)           │  │
│  │   - ARM64 native                  │  │
│  │   - Bundled as extraResource      │  │
│  │   - Spawned via Node.js           │  │
│  └───────────────────────────────────┘  │
└─────────────────────────────────────────┘
```

## Troubleshooting

### Build Issues

**Error: "cross not found"**
```bash
cargo install cross --git https://github.com/cross-rs/cross
```

**Error: "Docker daemon not running"**
```bash
sudo systemctl start docker
```

**Error: "Cannot find module 'electron-builder'"**
```bash
cd teddy && npm install
```

### Runtime Issues

**AppImage won't run**
```bash
sudo apt-get install -y fuse libfuse2
```

**Out of memory during builds**
- Close other applications
- Add swap space: `sudo dphys-swapfile swapoff && sudo nano /etc/dphys-swapfile` (set CONF_SWAPSIZE=2048)
- Restart swap: `sudo dphys-swapfile setup && sudo dphys-swapfile swapon`

**Slow AI responses**
- Use smaller models (1.5B or less)
- Ensure Ollama is using CPU: `OLLAMA_NUM_THREAD=4 ollama serve`
- Close background apps to free RAM

## See Also

- [BUILD_ARM64.md](./BUILD_ARM64.md) - Detailed build instructions
- [Main README](../README.md) - Ted project overview
- [ROADMAP](../ROADMAP.md) - Development roadmap
- [Raspberry Pi OS](https://www.raspberrypi.com/software/) - Official OS
- [Ollama](https://ollama.com/) - Local AI runtime

## Support

For issues specific to ARM64 builds:
- GitHub Issues: https://github.com/blackman-ai/ted/issues
- Tag with: `arm64`, `raspberry-pi`

For general Teddy issues, see the main README.
