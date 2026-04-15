#!/bin/bash
set -e

if command -v bit &> /dev/null; then
    echo "bit-lang CLI already installed"
    bit --version 2>/dev/null || true
    exit 0
fi

echo "Installing bit-lang CLI..."

if command -v cargo &> /dev/null; then
    cargo install bit-lang-cli
    echo "Installed via cargo"
elif command -v pip3 &> /dev/null; then
    pip3 install bit-lang
    echo "Installed Python bindings (full CLI requires: cargo install bit-lang-cli)"
elif command -v npm &> /dev/null; then
    npm install -g bit-lang
    echo "Installed npm bindings (full CLI requires: cargo install bit-lang-cli)"
else
    echo "ERROR: No package manager found. Install Rust first: https://rustup.rs"
    echo "Then run: cargo install bit-lang-cli"
    exit 1
fi
