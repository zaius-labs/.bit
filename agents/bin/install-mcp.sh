#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CRATE_DIR="$(cd "$SCRIPT_DIR/../../crates/bit-mcp" && pwd)"

echo "Building bit-mcp server..."

if ! command -v cargo &> /dev/null; then
    echo "ERROR: cargo not found. Install Rust first: https://rustup.rs"
    exit 1
fi

# Build release binary
cargo build --release --manifest-path "$CRATE_DIR/Cargo.toml" -p bit-lang-mcp

# Copy binary to plugin bin directory
BIN_SRC="$(cd "$CRATE_DIR/../.." && pwd)/target/release/bit-mcp"
if [ ! -f "$BIN_SRC" ]; then
    echo "ERROR: build succeeded but binary not found at $BIN_SRC"
    exit 1
fi

cp "$BIN_SRC" "$SCRIPT_DIR/bit-mcp"
chmod +x "$SCRIPT_DIR/bit-mcp"

echo "Installed bit-mcp to $SCRIPT_DIR/bit-mcp"
echo "MCP server ready for Claude Code plugin use."
