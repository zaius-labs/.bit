# Store & Expand

.bit files are designed for filesystem editing. The `.bitstore` format adds portable compression.

## The workflow

```
┌─────────────┐    collapse    ┌──────────────┐
│  .bit files  │ ────────────→ │  .bitstore   │
│  (editable)  │               │ (compressed)  │
│              │ ←──────────── │              │
└─────────────┘    expand      └──────────────┘
```

## Collapse (pack)

Collect all .bit files in a directory into a single compressed archive:

```sh
bit collapse ./my-project
# Collapsed 12 files into my-project.bitstore

bit collapse ./my-project --output backup.bitstore
```

Only `.bit` files are collected. Other files are ignored.

## Expand (unpack)

Extract .bit files from a store:

```sh
bit expand my-project.bitstore
# Expanded 12 files to ./my-project/

bit expand my-project.bitstore --output ./working-copy
```

Directory structure is preserved. If you had `schemas/user.bit` and `tasks/sprint.bit`, they expand to the same paths.

## Status (diff)

Check if expanded files have drifted from the store:

```sh
bit status my-project.bitstore ./working-copy

# Example output:
# Modified: schemas/user.bit
# Added: tasks/new-sprint.bit
# Deleted: tasks/old-sprint.bit

# If no changes:
# No changes
```

Uses blake3 checksums — fast and accurate.

## When to use stores

**Ship in repos:** Commit the `.bitstore` to git. Lightweight, compressed. Developers expand when they need to edit.

**Backup:** Pack before major refactors.

**Transport:** Share a project as a single file.

**CI/CD:** Check in the store, expand in CI, validate, collapse back.

## .bitstore format

```
[BITS]     4-byte magic
[version]  u32, little-endian
[zstd]     compressed payload containing:
           - JSON manifest (paths, sizes, blake3 checksums)
           - concatenated file contents
```

Single file. No runtime dependencies. ~3GB/s decompression.

## Tips

- `.bitstore` files are binary — add `*.bitstore` to `.gitignore` if you prefer expanded files in version control
- Or commit the `.bitstore` and `.gitignore` the expanded directory — either pattern works
- `bit status` is fast — just checksums, no full decompression
- The format is versioned — future versions can add query indexes without breaking old stores
