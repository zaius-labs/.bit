#!/bin/bash
# PreToolUse hook: enforce .bit rules mechanically
# Blocks or warns when tool use matches enforced rule patterns
# Matcher: Bash|Write|Edit

set -euo pipefail

INPUT=$(cat)

# Find the bitstore
STORE=""
for candidate in project.bitstore .bitstore; do
    if [ -f "$candidate" ]; then
        STORE="$candidate"
        break
    fi
done
if [ -z "$STORE" ]; then
    STORE=$(ls *.bitstore 2>/dev/null | head -1 || true)
fi

# No store — allow everything
if [ -z "$STORE" ] || [ ! -f "$STORE" ]; then
    echo '{"continue":true,"suppressOutput":true}'
    exit 0
fi

# Query all rules (filter enforced in Python since where clause is unreliable)
RULES=$(bit query "$STORE" "@Rule" 2>/dev/null || echo "[]")

# Pipe both input and rules to Python via env vars is fragile; use temp files
TMPINPUT=$(mktemp)
TMPRULES=$(mktemp)
trap "rm -f '$TMPINPUT' '$TMPRULES'" EXIT

echo "$INPUT" > "$TMPINPUT"
echo "$RULES" > "$TMPRULES"
export TMPINPUT TMPRULES

python3 << 'PYEOF'
import sys, json, re, os

with open(os.environ["TMPINPUT"]) as f:
    input_data = json.load(f)
with open(os.environ["TMPRULES"]) as f:
    rules = json.load(f)

tool_name = input_data.get("tool_name", "")
tool_input = input_data.get("tool_input", {})

# Extract match target based on tool
if tool_name == "Bash":
    target = tool_input.get("command", "")
elif tool_name in ("Write", "Edit"):
    target = tool_input.get("file_path", "")
else:
    target = ""

if not target:
    print(json.dumps({"continue": True, "suppressOutput": True}))
    sys.exit(0)

for rule in rules:
    fields = rule.get("fields", {})

    # Only enforced rules
    if fields.get("enforced", "false") != "true":
        continue

    pattern = fields.get("pattern", "")
    # Strip surrounding quotes from pattern value (bit query wraps strings)
    if pattern.startswith('"') and pattern.endswith('"'):
        pattern = pattern[1:-1]

    if not pattern:
        continue

    action = fields.get("action", "log")
    text = fields.get("text", rule.get("id", "unnamed rule"))

    try:
        if re.search(pattern, target):
            if action == "block":
                result = {
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "deny",
                        "permissionDecisionReason": f"Blocked by rule: {text}\nPattern: {pattern}\nMatched: {target}"
                    }
                }
                print(json.dumps(result))
                sys.exit(0)
            elif action == "warn":
                result = {
                    "continue": True,
                    "additionalContext": f"WARNING: Rule '{text}' triggered on this action (pattern: {pattern})"
                }
                print(json.dumps(result))
                sys.exit(0)
            # action == "log": silent continue
    except re.error:
        pass

print(json.dumps({"continue": True, "suppressOutput": True}))
PYEOF

exit 0
