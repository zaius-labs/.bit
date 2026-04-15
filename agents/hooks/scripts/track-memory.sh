#!/bin/bash
# Track Claude memory writes and sync to .bitstore
# PostToolUse hook for Write|Edit on memory .md files

INPUT=$(cat)

# Extract file_path and content from tool_input
eval "$(echo "$INPUT" | python3 -c "
import sys, json, shlex
try:
    data = json.load(sys.stdin)
    ti = data.get('tool_input', {})
    path = ti.get('file_path', '')
    content = ti.get('content', '')
    print('FILE_PATH=' + shlex.quote(path))
    print('FILE_CONTENT=' + shlex.quote(content))
except:
    print('FILE_PATH=')
    print('FILE_CONTENT=')
" 2>/dev/null)"

# Only process memory markdown files
if [ -z "$FILE_PATH" ] || [[ "$FILE_PATH" != */memory/*.md ]]; then
    echo '{"continue":true,"suppressOutput":true}'
    exit 0
fi

# Find a .bitstore
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

if [ -z "$STORE" ] || [ ! -f "$STORE" ]; then
    echo '{"continue":true,"suppressOutput":true}'
    exit 0
fi

# Parse frontmatter and generate entity fields
eval "$(echo "$FILE_CONTENT" | python3 -c "
import sys, re, shlex, os, datetime

content = sys.stdin.read()

# If no content from tool_input, bail
if not content.strip():
    print('SKIP=1')
    sys.exit(0)

# Parse YAML frontmatter between --- markers
fm_match = re.match(r'^---\s*\n(.*?)\n---\s*\n?(.*)', content, re.DOTALL)
if fm_match:
    fm_text = fm_match.group(1)
    body = fm_match.group(2).strip()
else:
    body = content.strip()
    fm_text = ''

name = ''
mem_type = ''
description = ''
for line in fm_text.split('\n'):
    line = line.strip()
    if line.startswith('name:'):
        name = line.split(':', 1)[1].strip().strip('\"').strip(\"'\")
    elif line.startswith('type:'):
        mem_type = line.split(':', 1)[1].strip().strip('\"').strip(\"'\")
    elif line.startswith('description:'):
        description = line.split(':', 1)[1].strip().strip('\"').strip(\"'\")

# Generate ID from filename
filename = os.environ.get('FILE_PATH', 'unknown.md')
basename = os.path.basename(filename).replace('.md', '')
# Strip common prefixes and convert to kebab-case
entity_id = basename.replace('_', '-')
for prefix in ['feedback-', 'project-', 'memory-']:
    if entity_id.startswith(prefix):
        entity_id = entity_id[len(prefix):]
        break

# Use name or derive from filename
if not name:
    name = entity_id.replace('-', ' ').title()

# Build text: prefer body, fallback to description
text = body if body else (description if description else name)
# Collapse for single-line field value
text_oneline = ' '.join(text.split())

today = datetime.date.today().isoformat()

print('SKIP=0')
print('ENTITY_ID=' + shlex.quote(entity_id))
print('MEM_NAME=' + shlex.quote(name))
print('MEM_TYPE=' + shlex.quote(mem_type or 'note'))
print('MEM_TEXT=' + shlex.quote(text_oneline))
print('MEM_DATE=' + shlex.quote(today))
" 2>/dev/null)"

if [ "${SKIP:-1}" = "1" ]; then
    echo '{"continue":true,"suppressOutput":true}'
    exit 0
fi

# Insert into bitstore via bit CLI
if command -v bit &> /dev/null; then
    bit insert "@Memory:${ENTITY_ID}" "text=${MEM_TEXT}" "type=${MEM_TYPE}" "created=${MEM_DATE}" --store "$STORE" 2>/dev/null
    if [ $? -eq 0 ]; then
        NAME_ESC=$(echo "$MEM_NAME" | sed 's/\\/\\\\/g; s/"/\\"/g')
        cat <<EOF
{"continue":true,"suppressOutput":false,"systemMessage":"[bit-lang] Memory synced: ${NAME_ESC}"}
EOF
        exit 0
    fi
fi

# Graceful degradation
echo '{"continue":true,"suppressOutput":true}'
exit 0
