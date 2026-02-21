#!/bin/bash
set -e

# Build script for CraftNet iOS
# Generates Swift bindings and builds the Rust library for iOS targets

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
UNIFFI_CRATE="$PROJECT_ROOT/crates/uniffi"
OUTPUT_DIR="$SCRIPT_DIR/CraftNet/Generated"

echo "Building CraftNet for iOS..."
echo "Project root: $PROJECT_ROOT"

# Ensure output directory exists
mkdir -p "$OUTPUT_DIR"

# Check for required tools
if ! command -v cargo &> /dev/null; then
    echo "Error: cargo is not installed"
    exit 1
fi

# Install iOS targets if needed
echo "Checking iOS targets..."
rustup target add aarch64-apple-ios || true
rustup target add aarch64-apple-ios-sim || true
rustup target add x86_64-apple-ios || true

# Build for iOS device (arm64)
echo "Building for iOS device (aarch64-apple-ios)..."
cargo build -p craftnet-uniffi --release --target aarch64-apple-ios

# Build for iOS simulator (arm64 - Apple Silicon)
echo "Building for iOS simulator (aarch64-apple-ios-sim)..."
cargo build -p craftnet-uniffi --release --target aarch64-apple-ios-sim

# Build for iOS simulator (x86_64 - Intel)
echo "Building for iOS simulator (x86_64-apple-ios)..."
cargo build -p craftnet-uniffi --release --target x86_64-apple-ios

# Generate Swift bindings
echo "Generating Swift bindings..."
cargo run -p uniffi-bindgen -- generate \
    --library "$PROJECT_ROOT/target/aarch64-apple-ios/release/libcraftnet_uniffi.dylib" \
    --language swift \
    --out-dir "$OUTPUT_DIR" \
    2>/dev/null || {
    # Fallback: use uniffi-bindgen-cli if available
    echo "Attempting alternative binding generation..."
    cd "$UNIFFI_CRATE"
    cargo build --release
    uniffi-bindgen generate \
        src/lib.rs \
        --language swift \
        --out-dir "$OUTPUT_DIR" \
        2>/dev/null || echo "Warning: Swift binding generation requires uniffi-bindgen-cli"
}

# Create XCFramework
echo "Creating XCFramework..."
XCFRAMEWORK_DIR="$SCRIPT_DIR/Frameworks"
mkdir -p "$XCFRAMEWORK_DIR"

# Create fat library for simulator (arm64 + x86_64)
echo "Creating simulator fat library..."
mkdir -p "$PROJECT_ROOT/target/ios-sim-universal/release"
lipo -create \
    "$PROJECT_ROOT/target/aarch64-apple-ios-sim/release/libcraftnet_uniffi.a" \
    "$PROJECT_ROOT/target/x86_64-apple-ios/release/libcraftnet_uniffi.a" \
    -output "$PROJECT_ROOT/target/ios-sim-universal/release/libcraftnet_uniffi.a" \
    2>/dev/null || {
    echo "Note: Using single-arch simulator library (arm64 only)"
    cp "$PROJECT_ROOT/target/aarch64-apple-ios-sim/release/libcraftnet_uniffi.a" \
       "$PROJECT_ROOT/target/ios-sim-universal/release/libcraftnet_uniffi.a"
}

# Create XCFramework
rm -rf "$XCFRAMEWORK_DIR/CraftNetUniFFI.xcframework"
xcodebuild -create-xcframework \
    -library "$PROJECT_ROOT/target/aarch64-apple-ios/release/libcraftnet_uniffi.a" \
    -library "$PROJECT_ROOT/target/ios-sim-universal/release/libcraftnet_uniffi.a" \
    -output "$XCFRAMEWORK_DIR/CraftNetUniFFI.xcframework" \
    2>/dev/null || {
    echo "Warning: XCFramework creation requires Xcode command line tools"
    echo "Static libraries are available in target/<arch>/release/"
}

echo ""
echo "Build complete!"
echo ""
echo "Generated files:"
echo "  - Swift bindings: $OUTPUT_DIR/"
echo "  - Static libraries:"
echo "    - Device: $PROJECT_ROOT/target/aarch64-apple-ios/release/libcraftnet_uniffi.a"
echo "    - Simulator: $PROJECT_ROOT/target/ios-sim-universal/release/libcraftnet_uniffi.a"
echo ""
echo "To use in Xcode:"
echo "  1. Add the Generated/*.swift files to your project"
echo "  2. Add the XCFramework or static library to your project"
echo "  3. Import 'craftnet_uniffi' in your Swift code"
