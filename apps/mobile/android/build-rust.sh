#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

echo "Building CraftNet Rust library for Android..."

# Ensure cargo-ndk is installed
if ! command -v cargo-ndk &> /dev/null; then
    echo "Installing cargo-ndk..."
    cargo install cargo-ndk
fi

# Add Android targets if not present
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android

cd "$PROJECT_ROOT"

# Build for all Android architectures
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 \
    -o "$SCRIPT_DIR/app/src/main/jniLibs" \
    build -p craftnet-uniffi --release

echo "Android Rust library built successfully!"
