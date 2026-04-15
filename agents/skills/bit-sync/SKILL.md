---
name: bit-sync
description: Sync between .bit files and .bitstore. Collapse files into the store or expand the store back to files. Use after editing .bit files or when you need fresh context from the store.
user-invocable: true
argument-hint: "[collapse|expand|status]"
---

# Sync .bit ↔ .bitstore

Keep .bit files and the .bitstore database in sync.

## Commands

### Collapse (files → store)

Pack all .bit files into the queryable .bitstore:

```bash
bit collapse . --output project.bitstore
```

Use after editing .bit files directly.

### Expand (store → files)

Unpack the .bitstore back to editable .bit files:

```bash
bit expand project.bitstore --output .
```

Use when you need to hand-edit content.

### Status (check drift)

See what's changed between files and store:

```bash
bit status project.bitstore .
```

Shows added, modified, and deleted files.

## When to sync

- After `/bit-init` — collapse to create the initial store
- After manual .bit edits — collapse to update the store
- After store mutations (bit insert/update) — expand to update files
- Before committing — ensure store and files match

## Quick reference

```bash
bit collapse .                    # Pack files → store
bit expand project.bitstore       # Unpack store → files
bit status project.bitstore .     # Check drift
bit query project.bitstore "@X"   # Query without expanding
bit info project.bitstore         # Store stats
```
