# Tasks & Flows

.bit has first-class support for task tracking and state machines.

## Task markers

```bit
# Sprint 42

[!] Implement user authentication
[!] Write API tests :@alice
[o] Design database schema
[x] Set up CI pipeline
[x] Create project structure
[~] Migrate legacy data (blocked)
```

| Marker | Meaning | When to use |
|--------|---------|-------------|
| `[!]` | Pending | Not started yet |
| `[o]` | In progress | Actively being worked on |
| `[x]` | Completed | Done |
| `[~]` | Blocked/partial | Waiting on something |

## Labels and assignments

```bit
[A!] Critical: Fix production bug        # "A" is the label
[!] Review PR #42 :@alice                 # assigned to @alice
[B!] Nice-to-have: Add dark mode :@bob   # labeled "B", assigned to @bob
```

## Groups organize tasks

```bit
# Backend
    [x] Set up database
    [!] Implement REST API
    [!] Add authentication

## API Endpoints
    [!] GET /users
    [!] POST /users
    [x] GET /health

# Frontend
    [!] Create login page
    [!] Build dashboard
```

Groups are `#` (depth 1), `##` (depth 2), etc. Tasks inherit their group's context.

## Flows (state machines)

```bit
flow:ticket_lifecycle
    open --> in_progress --> review --> closed
    review --> in_progress
    open --> closed
```

This defines a state machine:
```
open ──→ in_progress ──→ review ──→ closed
  │                        │
  └────────────────────────┘
          (reopen)
```

Flows are first-class — they can be validated, visualized, and enforced.

## Gates (conditions)

```bit
gate:deploy_ready
    {all_tests_pass}
    {code_reviewed}
    {no_critical_bugs}
```

Gates use ternary logic (true/false/unknown). A gate blocks progress until all conditions are met.

## Combining everything

```bit
# Release v2.0

define:@Release
    version: ""!
    status: :planning/:development/:testing/:released!

mutate:@Release:v2
    version: "2.0.0"
    status: :development

## Development
    [x] Feature A: User profiles
    [o] Feature B: Search
    [!] Feature C: Notifications

## Quality
    gate:release_gate
        {all_features_complete}
        {qa_approved}
    [!] Run regression tests
    [!] Performance benchmarks

    flow:release
        development --> testing --> staging --> released
```

## Tips

- Use groups to organize tasks by area (backend, frontend, etc.)
- Flows enforce valid state transitions — you can't go from `released` back to `development` unless you define that transition
- Gates are powerful for checklists that must ALL pass before proceeding
- Combine entities + tasks + flows for full project tracking in plain text
