#!/bin/bash
# PreToolUse hook: validate .bit files before writing
# Matcher: Write|Edit

INPUT=$(cat)

# Extract file_path and tool_name via python3
PARSED=$(python3 -c "
import sys, json
data = json.loads(sys.stdin.read())
tool_name = data.get('tool_name', '')
tool_input = data.get('tool_input', {})
file_path = tool_input.get('file_path', '')
print(tool_name)
print(file_path)
" <<< "$INPUT" 2>/dev/null) || true

TOOL_NAME=$(echo "$PARSED" | head -1)
FILE_PATH=$(echo "$PARSED" | tail -1)

# Only validate .bit files
if [ -z "$FILE_PATH" ] || [[ "$FILE_PATH" != *.bit ]]; then
    echo '{"continue":true,"suppressOutput":true}'
    exit 0
fi

# For Edit: can't validate pre-edit content easily, allow (PostToolUse catches it)
if [ "$TOOL_NAME" = "Edit" ]; then
    echo '{"continue":true,"suppressOutput":true}'
    exit 0
fi

# For Write: extract content, write to temp, validate
if ! command -v bit &> /dev/null; then
    echo '{"continue":true,"suppressOutput":true}'
    exit 0
fi

TMPBIT="$(mktemp).bit"
trap "rm -f '$TMPBIT'" EXIT

# Extract content and write to temp file
python3 -c "
import sys, json
data = json.loads(sys.stdin.read())
content = data.get('tool_input', {}).get('content', '')
sys.stdout.write(content)
" <<< "$INPUT" > "$TMPBIT" 2>/dev/null

RESULT=$(bit validate "$TMPBIT" 2>&1) || true
EXIT_CODE=$?
# Get actual exit code from bit validate
bit validate "$TMPBIT" > /dev/null 2>&1
EXIT_CODE=$?

if [ $EXIT_CODE -ne 0 ]; then
    # Validation errors — block the write
    python3 -c "
import sys, json
errors = sys.argv[1]
result = {
    'hookSpecificOutput': {
        'hookEventName': 'PreToolUse',
        'permissionDecision': 'deny',
        'permissionDecisionReason': 'Validation errors in .bit file:\n' + errors
    }
}
print(json.dumps(result))
" "$RESULT"
    exit 0
fi

if echo "$RESULT" | grep -q "warning:"; then
    # Warnings — allow with context
    python3 -c "
import sys, json
warnings = sys.argv[1]
result = {
    'continue': True,
    'additionalContext': '[bit-lang] Validation warnings: ' + warnings
}
print(json.dumps(result))
" "$RESULT"
    exit 0
fi

# Clean — allow silently
echo '{"continue":true,"suppressOutput":true}'
exit 0
