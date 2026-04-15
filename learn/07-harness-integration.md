# Harness Integration

.bit can be applied to AI coding harnesses and development tools automatically.

## Watch mode

Monitor a directory for .bit file changes:

```sh
bit watch ./config
```

Output (NDJSON — one JSON object per line):
```json
{"event":"modified","path":"config/rules.bit","timestamp":"2026-04-11T12:00:00Z"}
{"event":"created","path":"config/new-skill.bit","timestamp":"2026-04-11T12:00:01Z"}
{"event":"removed","path":"config/old.bit","timestamp":"2026-04-11T12:00:02Z"}
```

Only `.bit` file changes are emitted. Use this to trigger rebuilds, syncs, or validations.

## Apply

Apply .bit configurations to a detected harness:

```sh
bit apply ./my-bit-config
```

### Auto-detection

`bit apply` walks up from the current directory looking for harness markers:

| Directory found | Harness detected | Action |
|----------------|-----------------|--------|
| `.claude/` | Claude Code | Copies .bit files to `.claude/skills/` |
| (default) | Generic | Copies .bit files to `.bit-applied/` |

### Explicit harness

```sh
bit apply ./config --harness claude
bit apply ./config --harness generic
```

### Claude Code integration

When Claude Code is detected, `bit apply`:
1. Reads all .bit files from the source directory
2. Copies them to `.claude/skills/`
3. Reports what was applied

This lets you manage Claude Code skills as .bit files — version-controlled, structured, and portable.

### Generic apply

For any other tool, `bit apply` copies .bit files to a `.bit-applied/` directory. You can then configure your tool to read from there.

## Combining watch + apply

```sh
# Watch and auto-apply on changes
bit watch ./config | while read -r line; do
    bit apply ./config
    echo "Applied: $line"
done
```

## Tips

- Watch mode runs until killed (Ctrl+C)
- NDJSON output is easy to parse in any language
- Harness detection is convention-based — look for known directories
- New harness adapters can be added to bit-lang-cli as the ecosystem grows
