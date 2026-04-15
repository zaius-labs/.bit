#!/bin/bash
# Validate .bit files after Write/Edit
# Receives tool use info on stdin

INPUT=$(cat)

# Extract file path from the tool input
# PostToolUse provides tool_input as JSON
FILE_PATH=$(echo "$INPUT" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    # Try different field names
    path = data.get('tool_input', {}).get('file_path', '') or data.get('file_path', '')
    print(path)
except:
    print('')
" 2>/dev/null)

# Only validate .bit files
if [ -z "$FILE_PATH" ] || [[ "$FILE_PATH" != *.bit ]]; then
    exit 0
fi

# Validate if CLI is available
if command -v bit &> /dev/null; then
    RESULT=$(bit validate "$FILE_PATH" 2>&1)
    EXIT_CODE=$?
    if [ $EXIT_CODE -ne 0 ]; then
        echo "[bit-lang] Validation issue in $FILE_PATH:" >&2
        echo "$RESULT" >&2
    fi
fi

exit 0
