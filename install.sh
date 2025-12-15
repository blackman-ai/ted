#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.
#
# Ted installer script
# Usage:
#   Install:   curl -fsSL https://raw.githubusercontent.com/blackman-ai/ted/master/install.sh | sh
#   Uninstall: curl -fsSL https://raw.githubusercontent.com/blackman-ai/ted/master/install.sh | sh -s -- --uninstall
#
# Environment variables:
#   TED_INSTALL_DIR - Installation directory (default: ~/.local/bin or /usr/local/bin)
#   TED_VERSION     - Specific version to install (default: latest)

set -e

REPO="blackman-ai/ted"
BINARY_NAME="ted"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() {
    printf "${BLUE}==>${NC} %s\n" "$1"
}

success() {
    printf "${GREEN}==>${NC} %s\n" "$1"
}

warn() {
    printf "${YELLOW}Warning:${NC} %s\n" "$1"
}

error() {
    printf "${RED}Error:${NC} %s\n" "$1" >&2
    exit 1
}

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "darwin" ;;
        MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
        *)       error "Unsupported operating system: $(uname -s)" ;;
    esac
}

# Detect architecture
detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *)             error "Unsupported architecture: $(uname -m)" ;;
    esac
}

# Get target triple
get_target() {
    local os="$1"
    local arch="$2"

    case "$os" in
        linux)   echo "${arch}-unknown-linux-gnu" ;;
        darwin)  echo "${arch}-apple-darwin" ;;
        windows) echo "${arch}-pc-windows-msvc" ;;
    esac
}

# Get latest version from GitHub
get_latest_version() {
    local url="https://api.github.com/repos/${REPO}/releases/latest"

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" | grep '"tag_name"' | sed -E 's/.*"v?([^"]+)".*/\1/'
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- "$url" | grep '"tag_name"' | sed -E 's/.*"v?([^"]+)".*/\1/'
    else
        error "Neither curl nor wget found. Please install one of them."
    fi
}

# Download file
download() {
    local url="$1"
    local output="$2"

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$output"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$url" -O "$output"
    else
        error "Neither curl nor wget found. Please install one of them."
    fi
}

# Determine install directory
get_install_dir() {
    # Use TED_INSTALL_DIR if set
    if [ -n "$TED_INSTALL_DIR" ]; then
        echo "$TED_INSTALL_DIR"
        return
    fi

    # Check if user has write access to /usr/local/bin
    if [ -w "/usr/local/bin" ]; then
        echo "/usr/local/bin"
    elif [ -d "$HOME/.local/bin" ]; then
        echo "$HOME/.local/bin"
    else
        # Create ~/.local/bin if it doesn't exist
        mkdir -p "$HOME/.local/bin"
        echo "$HOME/.local/bin"
    fi
}

# Check if directory is in PATH
check_path() {
    local dir="$1"
    case ":$PATH:" in
        *":$dir:"*) return 0 ;;
        *)          return 1 ;;
    esac
}

# Find ted binary location
find_ted() {
    # Check common locations
    for dir in "$TED_INSTALL_DIR" "$HOME/.local/bin" "/usr/local/bin" "/usr/bin"; do
        if [ -n "$dir" ] && [ -x "$dir/ted" ]; then
            echo "$dir/ted"
            return 0
        fi
    done

    # Try which
    if command -v ted >/dev/null 2>&1; then
        which ted
        return 0
    fi

    return 1
}

# Uninstall ted
uninstall() {
    info "Uninstalling Ted..."
    echo

    local ted_path=$(find_ted)

    if [ -z "$ted_path" ]; then
        error "Ted is not installed or could not be found."
    fi

    info "Found ted at: $ted_path"

    # Get version before removing
    local version=$("$ted_path" --version 2>/dev/null | head -1 | awk '{print $2}' || echo "unknown")

    # Remove binary
    if rm "$ted_path" 2>/dev/null; then
        success "Removed $ted_path"
    else
        error "Failed to remove $ted_path. Try running with sudo."
    fi

    # Optionally remove config directory
    local config_dir="${TED_HOME:-$HOME/.ted}"
    if [ -d "$config_dir" ]; then
        echo
        warn "Configuration directory exists at: $config_dir"
        echo "This contains your settings, session history, and custom caps."
        echo "To remove it, run: rm -rf $config_dir"
    fi

    echo
    success "Ted v$version uninstalled successfully!"
}

# Main installation
main() {
    info "Installing Ted - AI coding assistant for your terminal"
    echo

    # Detect platform
    local os=$(detect_os)
    local arch=$(detect_arch)
    local target=$(get_target "$os" "$arch")

    info "Detected platform: $target"

    # Get version
    local version="${TED_VERSION:-}"
    if [ -z "$version" ]; then
        info "Fetching latest version..."
        version=$(get_latest_version)
        if [ -z "$version" ]; then
            error "Failed to fetch latest version. Check your internet connection."
        fi
    fi

    # Remove 'v' prefix if present
    version="${version#v}"
    info "Installing version: $version"

    # Determine archive extension
    local ext="tar.gz"
    if [ "$os" = "windows" ]; then
        ext="zip"
    fi

    # Download URL
    local filename="ted-${target}.${ext}"
    local url="https://github.com/${REPO}/releases/download/v${version}/${filename}"

    # Create temp directory
    local tmpdir=$(mktemp -d)
    trap "rm -rf '$tmpdir'" EXIT

    # Download
    info "Downloading $filename..."
    download "$url" "$tmpdir/$filename"

    # Extract
    info "Extracting..."
    cd "$tmpdir"
    if [ "$ext" = "tar.gz" ]; then
        tar xzf "$filename"
    else
        unzip -q "$filename"
    fi

    # Install
    local install_dir=$(get_install_dir)
    info "Installing to $install_dir..."

    local binary_path="$install_dir/$BINARY_NAME"
    if [ "$os" = "windows" ]; then
        binary_path="${binary_path}.exe"
    fi

    # Check if ted already exists
    if [ -f "$binary_path" ]; then
        local existing_version=$("$binary_path" --version 2>/dev/null | head -1 | awk '{print $2}' || echo "unknown")
        warn "Replacing existing installation (version: $existing_version)"
    fi

    # Copy binary
    if [ "$os" = "windows" ]; then
        cp "ted.exe" "$binary_path"
    else
        cp "ted" "$binary_path"
        chmod +x "$binary_path"
    fi

    echo
    success "Ted v$version installed successfully!"
    echo

    # Check PATH
    if ! check_path "$install_dir"; then
        warn "$install_dir is not in your PATH"
        echo
        echo "Add it to your shell configuration:"
        echo
        case "$SHELL" in
            */zsh)
                echo "  echo 'export PATH=\"$install_dir:\$PATH\"' >> ~/.zshrc"
                echo "  source ~/.zshrc"
                ;;
            */bash)
                echo "  echo 'export PATH=\"$install_dir:\$PATH\"' >> ~/.bashrc"
                echo "  source ~/.bashrc"
                ;;
            */fish)
                echo "  fish_add_path $install_dir"
                ;;
            *)
                echo "  export PATH=\"$install_dir:\$PATH\""
                ;;
        esac
        echo
    fi

    # Quick start guide
    echo "Quick start:"
    echo "  1. Set your API key:  export ANTHROPIC_API_KEY=\"your-key\""
    echo "  2. Start chatting:    ted"
    echo
    echo "To uninstall: curl -fsSL https://raw.githubusercontent.com/${REPO}/master/install.sh | sh -s -- --uninstall"
    echo "For more info, visit: https://github.com/${REPO}"
}

# Parse arguments
case "${1:-}" in
    --uninstall|-u|uninstall)
        uninstall
        ;;
    --help|-h|help)
        echo "Ted Installer"
        echo ""
        echo "Usage:"
        echo "  install.sh [options]"
        echo ""
        echo "Options:"
        echo "  --uninstall, -u    Uninstall ted"
        echo "  --help, -h         Show this help"
        echo ""
        echo "Environment variables:"
        echo "  TED_INSTALL_DIR    Installation directory"
        echo "  TED_VERSION        Specific version to install"
        ;;
    *)
        main "$@"
        ;;
esac
