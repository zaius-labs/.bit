---
name: bit-status
description: Show store health — entity counts, drift status, enforced rules, active tasks. Quick health check for bit-powered projects.
user-invocable: true
---

# .bit Status Report

Run a health check on the project's .bit store and report the results.

## Step 1: Entity counts and store size

```bash
bit info
```

Report the total entity count and store size. If `bit info` fails, the project may not have a .bitstore — suggest running `/bit-init` first.

## Step 2: Drift check

```bash
bit drift
```

Compare .bit source files against the collapsed .bitstore. Report whether they are in sync or if re-collapse is needed. If drift is detected, list which files have changed.

## Step 3: Enforced rules

```bash
bit query "@Rule"
```

From the results, filter for rules where `enforced: true`. List each enforced rule with its text, scope, pattern, and action. If no rules exist, note that.

## Step 4: Active tasks

```bash
bit query "@Task"
```

From the results, filter for tasks where status is `:in_progress` or `:pending`. List each with its text, status, and priority. If the `where` clause is supported, use:

```bash
bit query "@Task where status=in_progress"
bit query "@Task where status=pending"
```

## Step 5: Memory summary

```bash
bit query "@Memory"
```

Count memories by type (:user, :feedback, :project, :reference). Report the totals.

## Step 6: Format the report

Present a clean status report like this:

```
## .bit Status

Store: project.bitstore (X entities, Y KB)
Drift: in sync / X files changed

### Enforced Rules (N)
- [block] "rule text" (scope: global, pattern: regex)
- [warn] "rule text" (scope: directory, pattern: regex)

### Active Tasks (N)
- [in_progress] "task text" (priority: 3)
- [pending] "task text" (priority: 1)

### Memory (N total)
- project: X
- user: X
- feedback: X
- reference: X
```

If any section has zero entities, still show it with a count of 0 so the user knows the category exists but is empty.
