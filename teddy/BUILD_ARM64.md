# Building Teddy for ARM64 (Raspberry Pi 5)

This guide covers building Teddy for ARM64 architecture, specifically targeting the Raspberry Pi 5 (8GB model) for the UltraTiny tier.

## Prerequisites

### On Linux (x64)

For cross-compiling to ARM64:

```bash
# Install cross-compilation toolchain
cargo install cross --git https://github.com/cross-rs/cross

# Install Docker (required by cross)
# Follow instructions at: https://docs.docker.com/engine/install/
```

### On Raspberry Pi 5 (Native Build)

For building natively on the Pi 5:

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Node.js 20.x
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt-get install -y nodejs

# Install build dependencies
sudo apt-get install -y build-essential libssl-dev pkg-config
```

## Building

### Cross-Compilation (from x64 Linux)

```bash
# Clone the repository
git clone https://github.com/blackman-ai/ted.git
cd ted/teddy

# Install dependencies
npm install

# Build for ARM64
npm run build:linux:arm64
```

The built packages will be in `teddy/release/`:
- `teddy-<version>-arm64.AppImage` - Portable AppImage
- `teddy_<version>_arm64.deb` - Debian package

### Native Build (on Raspberry Pi 5)

```bash
# Clone the repository
git clone https://github.com/blackman-ai/ted.git
cd ted/teddy

# Install dependencies
npm install

# Build (will auto-detect ARM64)
npm run build
```

## Installation on Raspberry Pi 5

### Using .deb Package (Recommended)

```bash
# Download the latest release
wget https://github.com/blackman-ai/ted/releases/latest/download/teddy_<version>_arm64.deb

# Install
sudo dpkg -i teddy_<version>_arm64.deb

# Fix dependencies if needed
sudo apt-get install -f
```

### Using AppImage

```bash
# Download the latest release
wget https://github.com/blackman-ai/ted/releases/latest/download/teddy-<version>-arm64.AppImage

# Make executable
chmod +x teddy-<version>-arm64.AppImage

# Run
./teddy-<version>-arm64.AppImage
```

## Performance on Raspberry Pi 5

The Raspberry Pi 5 (8GB) is classified as **UltraTiny tier** hardware:

- **Recommended Models**: qwen2.5-coder:1.5b, qwen2.5-coder:3b, qwen2.5-coder:7b (optional)
- **Expected Response Time**: 15-45 seconds
- **Capabilities**:
  - Simple web apps (HTML/CSS/JS)
  - Basic Python scripts
  - Configuration file editing
  - Learning projects

- **Limitations**:
  - Cannot run models >3B parameters
  - Limited to very simple projects
  - Slow for complex operations

## Recommended Setup

For optimal performance on Raspberry Pi 5:

1. **Prepare a local GGUF model** for local AI:
   ```bash
   mkdir -p ~/.ted/models/local
   # Place model.gguf in ~/.ted/models/local/model.gguf
   ```

2. **Configure Teddy**:
   - Open Settings → AI Providers
   - Select "Local (llama.cpp)"
   - Set Model to: `local`
   - Set Port to: `8847`

3. **Use lightweight projects**:
   - Avoid large frameworks
   - Keep dependencies minimal
   - Use static site generators when possible

## Troubleshooting

### Build Errors

**"cross not found"**: Install cross with `cargo install cross --git https://github.com/cross-rs/cross`

**Docker errors**: Ensure Docker is running: `sudo systemctl start docker`

**Out of memory**: The Pi 5 has 8GB RAM. Close other applications during build.

### Runtime Issues

**Slow responses**: This is expected. See performance notes above.

**Out of memory during AI inference**: Use smaller models (1.5B or less).

**AppImage won't run**: Install FUSE: `sudo apt-get install -y fuse libfuse2`

## CI/CD

ARM64 builds are automatically created for every release via GitHub Actions. See `.github/workflows/teddy-release.yml`.

To trigger a release:

```bash
git tag teddy-v0.1.0
git push origin teddy-v0.1.0
```

## See Also

- [Raspberry Pi OS Installation](https://www.raspberrypi.com/software/)
- [llama.cpp](https://github.com/ggml-org/llama.cpp)
- [Ted Hardware Detection](../src/tools/builtin/hardware.rs)
