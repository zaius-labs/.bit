# How the Bitstore Engine Works

A deep dive into how bit-lang stores data — pages, B-trees, and queries from scratch.

## Why Build a Database?

The .bitstore engine stores data in indexed pages. Query one entity without touching the rest. Same idea as SQLite: one file, zero config, instant queries. You open the file, walk the index to the record you want, read a few kilobytes, done.

Building this is the best way to understand how databases actually work. Every real database — SQLite, Postgres, MySQL — uses the same core ideas: pages, B-trees, and a pager.

## Pages: The Fundamental Unit

Every database organizes data into fixed-size pages. Ours are 4096 bytes (4KB). Why 4KB? Because that's what the OS uses. When you read from disk, the OS fetches data in 4KB chunks — the "page size" of the virtual memory system. Reading 1 byte costs the same as reading 4096 bytes. So we align to it.

```
┌────────────────────── 4096 bytes ──────────────────────┐
│ type(1) │ page_num(4) │ cell_count(2) │ extra(4)       │
│         │             │               │                │
│ [cell 1 data] [cell 2 data] [cell 3 data] ...         │
│                                                        │
│                    (unused space)                       │
└────────────────────────────────────────────────────────┘
         11-byte header ───────────┘
```

Every page starts with an 11-byte header:
- **type** (1 byte): what kind of page is this?
- **page_num** (4 bytes): which page in the file
- **cell_count** (2 bytes): how many records (cells) are stored here
- **extra** (4 bytes): depends on page type — `next_leaf` for leaf pages, `rightmost_child` for interior pages

After the header, cells are packed sequentially. Unused space at the end is zeroed out.

There are 5 page types:

| Type | Byte | Purpose |
|------|------|---------|
| Header | 0x01 | File header (page 0 only) |
| BTreeInterior | 0x02 | B-tree branch node — keys + child pointers |
| BTreeLeaf | 0x03 | B-tree leaf — keys + values (actual data) |
| Overflow | 0x04 | Large values that don't fit in a single page |
| Freelist | 0x05 | Recycled (deleted) pages waiting to be reused |

## The File Header (Page 0)

Page 0 is always the file header. It tells you everything you need to start reading:

```
┌─────────────────── Page 0 ───────────────────┐
│ Offset  Field           Size   Example        │
│ ──────  ─────           ────   ───────        │
│  0      magic           4      "BITS"         │
│  4      version         4      2              │
│  8      page_size       4      4096           │
│ 12      page_count      4      47             │
│ 16      freelist_page   4      0              │
│ 20      entity_root     4      3              │
│ 24      task_root        4      12             │
│ 28      flow_root        4      0              │
│ 32      schema_root      4      8              │
│ 36      blob_root        4      15             │
│ 40      change_counter   8      5              │
│ 48      (unused)        4048   zeros          │
└──────────────────────────────────────────────┘
```

The root page pointers are the key. Each one points to the root of a B-tree for that data type. A value of 0 means "empty tree — no data of this type." When you want all entities, you start at `entity_root` and walk that tree. When you want blobs, start at `blob_root`.

The `change_counter` increments on every flush. Useful for detecting stale caches.

## The Pager: I/O Engine

The Pager sits between the B-tree logic and the raw file. It has three jobs: read pages, write pages, and manage free space. Everything above it thinks in page numbers, never in file offsets.

```
┌──────────────┐
│  B-Tree      │  "find @User:alice"
├──────────────┤
│  Pager       │  read_page(3) → cache hit or seek to 3 * 4096
├──────────────┤
│  File        │  raw bytes on disk
└──────────────┘
```

**Reading:** `read_page(n)` checks the cache first. Cache hit? Return immediately. Cache miss? Seek to `n * 4096` in the file, read 4096 bytes, store in cache, return.

**Writing:** `write_page(n, data)` writes to the cache and marks the page dirty. Nothing hits disk yet.

**Flushing:** `flush()` writes the header to page 0, then writes every dirty page to disk, then `fsync`. This is when changes become permanent.

**Allocating:** Need a new page? Check the freelist first. If there's a free page, pop it off and reuse it. Otherwise, increment `page_count` and return the new page number.

**Freeing:** When a page is no longer needed (e.g., after a B-tree node merge), push it onto the freelist. Next allocation will reuse it instead of growing the file.

## B-Trees: How We Find Things Fast

B-trees are the core of every database index. They're like binary search trees, but wider — each node holds many keys, not just one. This matters because each node is one page, and each page is one disk read. Wider nodes = fewer disk reads.

```
            ┌─────────────────────────────┐
            │  Interior: [grape] [mango]  │
            │  ↙         ↓         ↘     │
      ┌─────┴───┐  ┌────┴────┐  ┌──┴─────┐
      │  Leaf    │  │  Leaf   │  │  Leaf   │
      │ a, b, c  │→│ d, e, f  │→│ g, h, i │
      └──────────┘  └─────────┘  └─────────┘
           leaf chain (next_leaf →)
```

**Interior nodes** hold keys and child pointers. Each cell says "keys less than me are in my left child." The `rightmost_child` pointer handles keys greater than or equal to the last key.

**Leaf nodes** hold the actual data: key-value pairs. Keys are sorted. Leaves are linked via `next_leaf` pointers so you can scan sequentially without going back up the tree.

**Search** = walk from root to leaf. At each interior node, compare your key to the cells to pick the right child pointer. At the leaf, binary-search the cells. For 10,000 entities, that's about 3 page reads total — root, one interior, one leaf. O(log n).

### Cell Encoding

Cells are encoded differently depending on the page type:

**Leaf cell:** `[key_len: u16][key bytes][value_len: u32][value bytes]`

**Interior cell:** `[key_len: u16][key bytes][child_page: u32]`

Interior cells don't carry values — they just route you to the right child.

## Inserting Data

Walk through what happens when you insert `@User:alice`:

1. **Find the right leaf.** Walk the tree from root, comparing keys at each interior node, until you reach a leaf.
2. **Insert in sorted position.** Cells in a leaf are kept sorted by key. Find the insertion point, shift everything after it.
3. **Check if the leaf fits.** If `header_size + total_cell_bytes <= 4096`, done.
4. **If full, split.**

```
Before split:                  After split:
┌──────────────────────┐       ┌──────────┐   ┌──────────┐
│ a  b  c  d  e  f     │       │ a  b  c  │ → │ d  e  f  │
│        FULL!         │       └──────────┘   └──────────┘
└──────────────────────┘              ↑
                                 median "d" promoted to parent
```

Splitting works by taking the median key and pushing it up to the parent interior node. The left half stays in the original page, the right half goes into a newly allocated page. If the parent is also full, it splits too — this can cascade all the way to the root, which is how the tree grows taller.

If the root splits, we create a new root with one key and two children. This is the only way the tree gets a new level.

## Range Scans: The Leaf Chain Trick

Leaves are linked via `next_leaf` pointers. This is what makes prefix scans fast.

To find all `@User:*` entities:

1. Search for the first key starting with `@User:` — walk root to leaf.
2. Scan forward through that leaf's cells, collecting matches.
3. When you hit the end of the leaf, follow `next_leaf` to the next one.
4. Stop when you hit a key that no longer starts with `@User:`.

No need to go back to interior nodes. The leaf chain turns a tree walk into a sequential scan — and sequential reads are what disks (and SSDs) are best at.

## Tables: Typed Layers

The B-tree is generic — it stores `[key bytes] → [value bytes]`. The table layer adds meaning:

```
Entity tree:  @User:alice        → {"name":"Alice","role":"admin"}
Task tree:    sprint.bit:12:0    → {"text":"Add auth","marker":"!"}
Flow tree:    release             → {"states":[...],"edges":[...]}
Schema tree:  @User              → {"fields":{"name":"string!",...}}
Blob tree:    users.bit          → [hash_len][hash][raw file bytes]
```

Each table formats keys differently so prefix scans work for the right grouping:

- **EntityTable**: `@{Type}:{id}` — scan by type with prefix `@User:`
- **TaskTable**: `{file}:{line}:{idx}` — scan by file with prefix `sprint.bit:`
- **FlowTable**: flow name — simple key lookup
- **SchemaTable**: `@{Type}` — one schema per entity type
- **BlobTable**: relative file path — stores raw .bit file content plus a blake3 hash

Values are JSON (via serde_json), except for blobs which pack hash + raw bytes.

## Collapse: Turning Files into a Database

Here's what `bit collapse ./project` does:

1. **Walk the directory.** Find all `.bit` files recursively. Sort for determinism.
2. **For each file:**
   - Read raw bytes. Hash with blake3.
   - Insert into the **blob tree** (path → hash + content). This is the source of truth — `expand` reconstructs files from this.
   - Parse with `bit_core::parse_source`. If parsing fails, the blob is still stored — you don't lose data.
3. **Index the parsed document.** Walk every AST node:
   - `define:` → insert into **schema tree**
   - `mutate:` → insert into **entity tree**
   - `[!]`/`[x]` → insert into **task tree**
   - `flow:` → insert into **flow tree**
4. **Flush.** Write all dirty pages to disk. Increment the change counter.

The result is a single `.bitstore` file that you can query instantly.

## Queries: From Text to Results

Walk through: `bit query store.bitstore "@User where role=admin"`

1. **Parse the query.** Target = Entity("User"), filter = "role=admin".
2. **Prefix scan the entity tree.** Find all keys starting with `@User:`. This walks to the first matching leaf, then follows the leaf chain.
3. **For each match:** Deserialize the JSON value. Check if `role == "admin"`.
4. **Return matches.**

For a store with 50 files and 500 entities, the engine reads maybe 3-5 pages to find what you need.

## The Freelist: Recycling Space

When you delete a record and a B-tree node becomes empty, that page gets freed. The freelist is a simple linked list stored in the pages themselves:

```
Freelist chain:
Header.freelist_page → page 7 → page 3 → page 12 → 0 (end)
```

Each freelist page stores the next pointer at bytes 1-4 (byte 0 is the page type `0x05`). It's a stack:

- **free(page 5):** Write `next=7` into page 5, set `header.freelist_page = 5`. Chain: `5 → 7 → 3 → 12 → 0`
- **allocate():** Pop page 5, set `header.freelist_page = 7`. Return page 5 for reuse.

This means the file never grows when you delete-then-insert. Pages get recycled.

## What We Didn't Build (Yet)

| Feature | What It Does | Why SQLite Needs It |
|---------|-------------|-------------------|
| WAL (Write-Ahead Log) | Crash recovery | We flush everything at end; SQLite needs concurrent readers |
| MVCC | Multiple readers without blocking | We have single-threaded access |
| Transactions | Atomic multi-row changes | We do one operation at a time |
| Vacuum | Reclaim fragmented space | Our freelist handles simple cases |
| Prepared statements | Compiled query plans | Our queries are simple enough |

These are all solvable — they just aren't needed for the current use case. If you wanted to add crash safety, a WAL is the first thing you'd build: write changes to a separate log file first, then apply them to the main database. If the process crashes mid-write, replay the log on startup.

## The Full Stack

Putting it all together, here's every layer from "query a user" down to bytes on disk:

```
bit query store.bitstore "@User:alice"
         │
         ▼
┌─────────────────────┐
│  BitStore            │  High-level API: get_entity("User", "alice")
├─────────────────────┤
│  EntityTable         │  Formats key as "@User:alice", deserializes JSON
├─────────────────────┤
│  BTree               │  search(key) → walk root → interior → leaf
├─────────────────────┤
│  Pager               │  read_page(3) → cache / seek to 12288
├─────────────────────┤
│  File                │  4096 bytes at offset 12288
└─────────────────────┘
```

Five layers. Each one does exactly one thing. That's the whole database.

## Try It Yourself

```sh
# Create a project
mkdir mydata
cat > mydata/items.bit << 'EOF'
define:@Item
    name: ""!
    price: 0#

mutate:@Item:widget
    name: Widget
    price: 9
EOF

# Collapse into a database
bit collapse mydata

# Query it
bit query mydata.bitstore "@Item"

# Insert directly
bit insert mydata.bitstore @Item:gadget name=Gadget price=19

# See the page structure
bit info mydata.bitstore
bit pages mydata.bitstore
```
