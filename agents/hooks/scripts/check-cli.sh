#!/bin/bash
# Check if bit CLI is installed, warn if not
if ! command -v bit &> /dev/null; then
    echo "WARN: bit-lang CLI not installed." >&2
    echo "Install with: cargo install bit-lang-cli" >&2
    echo "Or: pip install bit-lang (Python bindings)" >&2
    exit 0
fi
echo "bit-lang CLI: $(bit --version 2>/dev/null || echo 'installed')" >&2
exit 0
