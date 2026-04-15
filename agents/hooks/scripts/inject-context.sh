#!/bin/bash
# inject-context.sh — UserPromptSubmit hook
# Queries .bitstore for rules, tasks, memories, conventions
# Injects as additionalContext before Claude processes each prompt

set -euo pipefail

BIT="${HOME}/.cargo/bin/bit"
PASSTHROUGH='{"continue":true,"suppressOutput":true}'

# Read stdin
INPUT=$(cat)

# Extract CWD and prompt from input JSON
eval "$(echo "$INPUT" | python3 -c "
import json, sys, shlex
d = json.load(sys.stdin)
print(f'CWD={shlex.quote(d.get(\"cwd\", \".\"))}')
print(f'PROMPT={shlex.quote(d.get(\"prompt\", \"\"))}')
" 2>/dev/null)" || { echo "$PASSTHROUGH"; exit 0; }

# Find store
STORE=""
for candidate in "$CWD/project.bitstore" "$CWD/.bitstore"; do
    if [ -f "$candidate" ]; then
        STORE="$candidate"
        break
    fi
done
if [ -z "$STORE" ]; then
    for candidate in "$CWD"/*.bitstore; do
        if [ -f "$candidate" ]; then
            STORE="$candidate"
            break
        fi
    done
fi

# No store — silent passthrough
if [ -z "$STORE" ] || ! command -v "$BIT" &>/dev/null; then
    echo "$PASSTHROUGH"
    exit 0
fi

CONTEXT=""

# --- RULES (query all, filter enforced in Python — store saves booleans as strings) ---
RULES=$("$BIT" query "$STORE" '@Rule' 2>/dev/null || true)
if [ -n "$RULES" ] && [ "$RULES" != "[]" ]; then
    RULES_TEXT=$(echo "$RULES" | python3 -c "
import json, sys
try:
    items = json.load(sys.stdin)
    for r in items[:10]:
        f = r.get('fields', {})
        if f.get('enforced', 'false') != 'true': continue
        text = f.get('text', r.get('id', ''))
        pattern = f.get('pattern', '').strip('\"')
        action = f.get('action', 'block')
        parts = [text]
        if pattern: parts.append(f'pattern: {pattern}')
        parts.append(f'action: {action}')
        print(f'- {\" | \".join(parts)}')
except: pass
" 2>/dev/null || true)
    if [ -n "$RULES_TEXT" ]; then
        CONTEXT="${CONTEXT}## Enforced Rules
${RULES_TEXT}
"
    fi
fi

# --- TASKS (query all, filter status in Python) ---
TASKS=$("$BIT" query "$STORE" '@Task' 2>/dev/null || true)
if [ -n "$TASKS" ] && [ "$TASKS" != "[]" ]; then
    TASKS_TEXT=$(echo "$TASKS" | python3 -c "
import json, sys
try:
    items = json.load(sys.stdin)
    for t in items[:10]:
        f = t.get('fields', {})
        status = f.get('status', '')
        if status not in ('pending', 'in_progress'): continue
        text = f.get('text', t.get('id', ''))
        priority = f.get('priority', '')
        marker = 'o' if status == 'in_progress' else '!'
        line = f'- [{marker}] {text}'
        if priority and priority != '0': line += f' (priority: {priority})'
        print(line)
except: pass
" 2>/dev/null || true)
    if [ -n "$TASKS_TEXT" ]; then
        CONTEXT="${CONTEXT}
## Active Tasks
${TASKS_TEXT}
"
    fi
fi

# --- MEMORIES (BM25 relevance search) ---
if [ -n "$PROMPT" ]; then
    MEMORIES=$("$BIT" search "$STORE" "$PROMPT" 2>/dev/null || true)
    if [ -n "$MEMORIES" ] && [ "$MEMORIES" != "[]" ]; then
        MEMORIES_TEXT=$(echo "$MEMORIES" | python3 -c "
import json, sys
try:
    items = json.load(sys.stdin)
    if isinstance(items, list):
        for m in items[:5]:
            content = m.get('content', m.get('text', m.get('summary', '')))
            kind = m.get('kind', m.get('type', ''))
            if content:
                line = f'- {content[:120]}'
                if kind: line += f' ({kind})'
                print(line)
except: pass
" 2>/dev/null || true)
        if [ -n "$MEMORIES_TEXT" ]; then
            CONTEXT="${CONTEXT}
## Relevant Memory
${MEMORIES_TEXT}
"
        fi
    fi
fi

# --- CONVENTIONS (only if prompt mentions code-related keywords) ---
if echo "$PROMPT" | grep -qiE '(code|implement|fix|build|test|refactor|create|add|write|update|change|modify)'; then
    CONVENTIONS=$("$BIT" query "$STORE" '@Convention' 2>/dev/null || true)
    if [ -n "$CONVENTIONS" ] && [ "$CONVENTIONS" != "[]" ]; then
        CONV_TEXT=$(echo "$CONVENTIONS" | python3 -c "
import json, sys
try:
    items = json.load(sys.stdin)
    for c in items[:8]:
        f = c.get('fields', {})
        name = f.get('name', c.get('id', ''))
        desc = f.get('description', '')
        if name:
            line = f'- {name}'
            if desc: line += f': {desc[:80]}'
            print(line)
except: pass
" 2>/dev/null || true)
        if [ -n "$CONV_TEXT" ]; then
            CONTEXT="${CONTEXT}
## Conventions
${CONV_TEXT}
"
        fi
    fi
fi

# No context gathered — silent passthrough
if [ -z "$CONTEXT" ]; then
    echo "$PASSTHROUGH"
    exit 0
fi

# Truncate to ~500 tokens (~2000 chars)
CONTEXT=$(echo "$CONTEXT" | head -c 2000)

# Format final context block
FULL_CONTEXT="[bit-lang context]

${CONTEXT}"

# Output JSON with additionalContext
python3 -c "
import json, sys
ctx = sys.stdin.read()
print(json.dumps({
    'continue': True,
    'suppressOutput': True,
    'additionalContext': ctx
}))
" <<< "$FULL_CONTEXT"

exit 0
