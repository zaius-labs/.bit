#!/bin/bash
# init-store.sh — Load .bitstore context at session start
# Discovers or creates a .bitstore, queries enforced @Rule entities,
# and injects them as additionalContext for Claude.

set -euo pipefail

BIT="${HOME}/.cargo/bin/bit"
CWD="${CLAUDE_PROJECT_DIR:-$(pwd)}"

# Silent exit helper — no store, no .bit files, nothing to do
silent_exit() {
    echo '{"continue":true,"suppressOutput":true}'
    exit 0
}

# --- 1. Find a .bitstore ---
STORE=""
for candidate in "$CWD/project.bitstore" "$CWD/.bitstore"; do
    if [ -f "$candidate" ]; then
        STORE="$candidate"
        break
    fi
done

# Glob fallback
if [ -z "$STORE" ]; then
    for candidate in "$CWD"/*.bitstore; do
        if [ -f "$candidate" ]; then
            STORE="$candidate"
            break
        fi
    done
fi

# --- 2. No store found — try to create one from .bit files ---
if [ -z "$STORE" ]; then
    # Check if any .bit files exist
    shopt -s nullglob
    bit_files=("$CWD"/*.bit "$CWD"/**/*.bit)
    shopt -u nullglob

    if [ ${#bit_files[@]} -eq 0 ]; then
        silent_exit
    fi

    # bit CLI required from here
    if ! command -v "$BIT" &>/dev/null; then
        silent_exit
    fi

    # Collapse .bit files into a store
    if "$BIT" collapse "$CWD" --output "$CWD/project.bitstore" 2>/dev/null; then
        STORE="$CWD/project.bitstore"
    else
        silent_exit
    fi
fi

# --- 3. Query enforced @Rule entities ---
if ! command -v "$BIT" &>/dev/null; then
    # Have a store but no CLI — report store exists, skip rules
    echo '{"continue":true,"suppressOutput":false,"systemMessage":"[bit-lang] Store found but bit CLI unavailable — rules not loaded"}'
    exit 0
fi

# Get store info for the system message
ENTITY_COUNT=0
RULE_COUNT=0
RULES_TEXT=""

QUERY_OUTPUT=$("$BIT" query "$STORE" '@Rule' 2>/dev/null || echo "[]")

# Parse query output with python3 — filter enforced=true in Python (store saves booleans as strings)
PARSED=$(python3 -c "
import json, sys

raw = sys.argv[1]
try:
    rules = json.loads(raw)
except (json.JSONDecodeError, ValueError):
    rules = []

if not isinstance(rules, list):
    rules = []

lines = []
enforced_rules = [r for r in rules if r.get('fields', {}).get('enforced', 'false') == 'true']
for r in enforced_rules:
    fields = r.get('fields', {})
    text = fields.get('text', r.get('id', 'unnamed'))
    pattern = fields.get('pattern', '').strip('\"')
    action = fields.get('action', 'block')
    parts = [text]
    if pattern:
        parts.append(f'pattern: {pattern}')
    parts.append(f'action: {action}')
    lines.append('- ' + ' | '.join(parts))

rule_count = len(enforced_rules)
rules_text = chr(10).join(lines) if lines else ''
print(json.dumps({'rule_count': rule_count, 'rules_text': rules_text}))
" "$QUERY_OUTPUT" 2>/dev/null || echo '{"rule_count":0,"rules_text":""}')

RULE_COUNT=$(python3 -c "import json,sys; print(json.loads(sys.argv[1])['rule_count'])" "$PARSED" 2>/dev/null || echo "0")
RULES_TEXT=$(python3 -c "import json,sys; print(json.loads(sys.argv[1])['rules_text'])" "$PARSED" 2>/dev/null || echo "")

# Get total entity count
INFO_OUTPUT=$("$BIT" info "$STORE" 2>/dev/null || echo "")
ENTITY_COUNT=$(python3 -c "
import sys, re
info = sys.argv[1]
# Try to extract entity count from info output
m = re.search(r'(\d+)\s*entit', info)
print(m.group(1) if m else '?')
" "$INFO_OUTPUT" 2>/dev/null || echo "?")

# --- 4. Build JSON response ---
SYS_MSG="[bit-lang] Store loaded: ${ENTITY_COUNT} entities, ${RULE_COUNT} rules enforced"

if [ -n "$RULES_TEXT" ]; then
    # Build response with additionalContext
    python3 -c "
import json, sys
msg = sys.argv[1]
rules = sys.argv[2]
ctx = '[bit-lang rules]\n' + rules
print(json.dumps({
    'continue': True,
    'suppressOutput': False,
    'systemMessage': msg,
    'additionalContext': ctx
}))
" "$SYS_MSG" "$RULES_TEXT"
else
    python3 -c "
import json, sys
msg = sys.argv[1]
print(json.dumps({
    'continue': True,
    'suppressOutput': False,
    'systemMessage': msg
}))
" "$SYS_MSG"
fi

exit 0
