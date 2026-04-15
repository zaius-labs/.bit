#!/bin/bash
# Auto-validate and collapse .bit files after Write/Edit
# PostToolUse hook for Write|Edit on .bit files

INPUT=$(cat)

# Extract file path from tool_input
FILE_PATH=$(echo "$INPUT" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    path = data.get('tool_input', {}).get('file_path', '') or data.get('file_path', '')
    print(path)
except:
    print('')
" 2>/dev/null)

# Only process .bit files
if [ -z "$FILE_PATH" ] || [[ "$FILE_PATH" != *.bit ]]; then
    echo '{"continue":true,"suppressOutput":true}'
    exit 0
fi

# Check bit CLI availability
if ! command -v bit &> /dev/null; then
    echo '{"continue":true,"suppressOutput":true}'
    exit 0
fi

# Validate the file
RESULT=$(bit validate "$FILE_PATH" 2>&1)
EXIT_CODE=$?

MESSAGE=""

if [ $EXIT_CODE -ne 0 ]; then
    RESULT_ESC=$(echo "$RESULT" | sed 's/\\/\\\\/g; s/"/\\"/g; s/\t/\\t/g' | tr '\n' ' ')
    MESSAGE="[bit-lang] Validation error in $(basename "$FILE_PATH"): $RESULT_ESC"
elif echo "$RESULT" | grep -q "^warning:"; then
    RESULT_ESC=$(echo "$RESULT" | sed 's/\\/\\\\/g; s/"/\\"/g; s/\t/\\t/g' | tr '\n' ' ')
    MESSAGE="[bit-lang] Validation warning in $(basename "$FILE_PATH"): $RESULT_ESC"
fi

# Find a .bitstore for collapse
SEARCH_DIR="${CLAUDE_PROJECT_DIR:-.}"
STORE=""
for candidate in "$SEARCH_DIR/project.bitstore" "$SEARCH_DIR/.bitstore"; do
    if [ -f "$candidate" ]; then
        STORE="$candidate"
        break
    fi
done
if [ -z "$STORE" ]; then
    STORE=$(find "$SEARCH_DIR" -maxdepth 2 -name '*.bitstore' -print -quit 2>/dev/null)
fi

# Collapse into store if available
if [ -n "$STORE" ] && [ -f "$STORE" ]; then
    FILE_DIR=$(dirname "$FILE_PATH")
    bit collapse "$FILE_DIR" --output "$STORE" 2>/dev/null
    if [ $? -eq 0 ]; then
        if [ -n "$MESSAGE" ]; then
            MESSAGE="$MESSAGE (store synced)"
        else
            MESSAGE="[bit-lang] Synced $(basename "$FILE_PATH") to store"
        fi
    fi
fi

# Return result
if [ -n "$MESSAGE" ]; then
    MESSAGE_ESC=$(echo "$MESSAGE" | sed 's/\\/\\\\/g; s/"/\\"/g')
    cat <<EOF
{"continue":true,"suppressOutput":false,"systemMessage":"${MESSAGE_ESC}"}
EOF
else
    echo '{"continue":true,"suppressOutput":true}'
fi
exit 0
