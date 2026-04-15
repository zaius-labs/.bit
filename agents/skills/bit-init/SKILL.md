---
name: bit-init
description: Migrate a project from markdown to .bit. Converts CLAUDE.md, memory files, task lists, and plans to native .bit format. Use when setting up bit-lang in a project for the first time.
user-invocable: true
argument-hint: "[directory]"
---

# Migrate to .bit

Convert this project from scattered markdown to native .bit format. Follow these steps exactly.

## Entity Schemas

Write these schemas at the top of CLAUDE.bit. These are the canonical definitions — include them verbatim:

```bit
define:@Rule
    text: ""!
    enforced: true?
    scope: :global/:directory/:file!
    pattern: ""
    action: :block/:warn/:log!

define:@Memory
    text: ""!
    type: :user/:feedback/:project/:reference!
    created: ""@
    ttl: 0#
    tags: []

define:@Task
    text: ""!
    status: :pending/:in_progress/:done/:blocked!
    priority: 0#
    assigned: ""
    blocks: ->@Task
    blocked_by: ->@Task
    created: ""@
    completed: ""@
    tags: []

define:@Sprint
    name: ""!
    goal: ""
    status: :planning/:active/:review/:done!

define:@Convention
    name: ""!
    description: ""!
    example: ""

define:@Component
    name: ""!
    path: ""!
    language: ""!
    description: ""
    depends_on: ->@Component
```

## Step 1: Read CLAUDE.md and extract entities

Read the project's CLAUDE.md (or equivalent project instructions file). Extract three categories:

### Rules

Find anything that says "always", "never", "must", "don't", or describes enforced behavior. For each rule, write a `mutate:@Rule` entity:

```bit
mutate:@Rule
    text: "Always work on dev branch"!
    enforced: true?
    scope: :global!
    pattern: "git checkout|git switch"
    action: :block!
```

- `text` — the rule in plain language
- `enforced` — true if it should be actively checked, false if advisory
- `scope` — :global (whole project), :directory (specific path), or :file (single file)
- `pattern` — regex that would match violations (empty string if not pattern-matchable)
- `action` — :block (prevent), :warn (alert but allow), :log (record silently)

### Architecture / Components

Find references to directories, packages, services, or modules. For each, write a `mutate:@Component` entity:

```bit
mutate:@Component
    name: "Agent Engine"!
    path: "packages/server/lib/zaius/engine/"!
    language: "elixir"!
    description: "Agent harness, experience system, session collection"
    depends_on: ->@Component:canopy_nif
```

### Conventions

Find coding conventions, naming patterns, workflow preferences. For each, write a `mutate:@Convention` entity:

```bit
mutate:@Convention
    name: "Branch policy"!
    description: "Always work on dev branch. Pre-commit hook enforces."!
    example: "git checkout dev"
```

## Step 2: Convert memory files to memory.bit

Read `.claude/memory/*.md` or any equivalent memory/context files. For each distinct memory entry, write a `mutate:@Memory` entity:

```bit
mutate:@Memory
    text: "SWE-bench Docker eval: 10/10 pytest resolved (100%, avg 13.9t)"!
    type: :project!
    created: "2026-04-11"@
    tags: [swebench, results, proven]
```

- `type` — :user (user preferences), :feedback (user corrections), :project (project facts), :reference (external info)
- `created` — date the memory was recorded
- `ttl` — 0 means permanent. Set a positive number for ephemeral memories.
- `tags` — list of relevant keywords for retrieval

## Step 3: Convert task tracking to tasks.bit

Find any TODO lists, task trackers, checklists, or plan files. For each task, write a `mutate:@Task` entity:

```bit
mutate:@Task
    text: "Expand SWE-bench to django/sympy repos"!
    status: :pending!
    priority: 2#
    tags: [swebench, phase2]
```

- `status` — :pending, :in_progress, :done, :blocked
- `priority` — 0 (lowest) to 5 (highest)
- `blocks` / `blocked_by` — reference other @Task entities by ID if dependencies exist

## Step 4: Write the .bit files

Create three files in the project root (or the specified directory):

1. **CLAUDE.bit** — schemas (from above) + all @Rule, @Component, @Convention entities
2. **memory.bit** — all @Memory entities
3. **tasks.bit** — all @Task and @Sprint entities

Do NOT delete the original markdown files. Create .bit files alongside them.

## Step 5: Collapse into .bitstore

```bash
bit collapse . --output project.bitstore
```

## Step 6: Verify

Run these commands and check the output:

```bash
bit info project.bitstore
bit query "@Rule"
bit query "@Memory"
bit query "@Task"
```

Confirm that entity counts match what you extracted. If any counts are zero or missing, check the .bit files for syntax errors and re-collapse.

## Step 7: Report

Tell the user:
- How many entities were migrated per type (@Rule, @Memory, @Task, @Component, @Convention)
- Which source files were read
- Where the .bit files were written
- Any items that could not be cleanly converted (ambiguous rules, unclear task status, etc.)
- Remind them: original markdown files are preserved, .bit files sit alongside them
