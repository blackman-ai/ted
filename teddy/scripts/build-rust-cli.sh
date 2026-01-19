#!/bin/bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

# Build the Rust CLI binary for the target architecture
# Usage: ./build-rust-cli.sh [target-arch]
# target-arch: x64 (default), arm64

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
TARGET_ARCH="${1:-x64}"

echo "Building Rust CLI for architecture: $TARGET_ARCH"

# Map electron-builder arch names to Rust target triples
case "$TARGET_ARCH" in
  x64)
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
      RUST_TARGET="x86_64-unknown-linux-gnu"
    elif [[ "$OSTYPE" == "darwin"* ]]; then
      RUST_TARGET="x86_64-apple-darwin"
    elif [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "win32" ]]; then
      RUST_TARGET="x86_64-pc-windows-msvc"
    else
      echo "Unsupported OS: $OSTYPE"
      exit 1
    fi
    ;;
  arm64)
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
      RUST_TARGET="aarch64-unknown-linux-gnu"
    elif [[ "$OSTYPE" == "darwin"* ]]; then
      RUST_TARGET="aarch64-apple-darwin"
    else
      echo "Unsupported OS for ARM64: $OSTYPE"
      exit 1
    fi
    ;;
  *)
    echo "Unknown architecture: $TARGET_ARCH"
    exit 1
    ;;
esac

echo "Rust target triple: $RUST_TARGET"

# Install target if not already installed
rustup target add "$RUST_TARGET" || true

cd "$ROOT_DIR"

# Check if cross-compilation is needed
NEED_CROSS=false
CURRENT_ARCH=$(uname -m)

if [[ "$RUST_TARGET" == *"aarch64"* ]] && [[ "$CURRENT_ARCH" != "aarch64" ]] && [[ "$CURRENT_ARCH" != "arm64" ]]; then
  NEED_CROSS=true
elif [[ "$RUST_TARGET" == *"x86_64"* ]] && [[ "$CURRENT_ARCH" != "x86_64" ]] && [[ "$CURRENT_ARCH" != "x64" ]]; then
  NEED_CROSS=true
fi

# Build the binary
if [ "$NEED_CROSS" = true ] && [[ "$OSTYPE" == "linux-gnu"* ]]; then
  echo "Cross-compiling with cross..."
  # Check if cross is installed
  if ! command -v cross &> /dev/null; then
    echo "Installing cross..."
    cargo install cross --git https://github.com/cross-rs/cross
  fi
  cross build --release --target "$RUST_TARGET"
else
  echo "Building natively..."
  cargo build --release --target "$RUST_TARGET"
fi

# Copy binary to expected location for electron-builder
BINARY_NAME="ted"
if [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "win32" ]]; then
  BINARY_NAME="ted.exe"
fi

SOURCE_PATH="$ROOT_DIR/target/$RUST_TARGET/release/$BINARY_NAME"
DEST_DIR="$ROOT_DIR/target/release"

mkdir -p "$DEST_DIR"
cp "$SOURCE_PATH" "$DEST_DIR/$BINARY_NAME"

echo "Binary copied to: $DEST_DIR/$BINARY_NAME"
echo "Build complete!"
