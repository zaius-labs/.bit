#!/bin/bash
# Load .bitstore context at session start
# Reads from stdin for session info

CWD="${CLAUDE_PROJECT_DIR:-.}"

# Check for CLAUDE.bit
if [ -f "$CWD/CLAUDE.bit" ]; then
    echo "[bit-lang] Found CLAUDE.bit — native .bit project" >&2
fi

# Check for .bitstore
STORE=""
for candidate in "$CWD/project.bitstore" "$CWD/.bitstore" "$CWD/*.bitstore"; do
    if [ -f "$candidate" ]; then
        STORE="$candidate"
        break
    fi
done

if [ -n "$STORE" ] && command -v bit &> /dev/null; then
    INFO=$(bit info "$STORE" 2>/dev/null)
    if [ $? -eq 0 ]; then
        echo "[bit-lang] Store loaded: $STORE" >&2
        echo "$INFO" >&2
    fi
fi

exit 0
