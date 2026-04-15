# Advanced Patterns

## Nested entities with relations

```bit
define:@Team
    name: ""!
    lead: ->@User!

define:@User
    name: ""!
    email: ""!
    team: ->@Team

mutate:@Team:engineering
    name: "Engineering"
    lead: ->@User:alice

mutate:@User:alice
    name: "Alice Chen"
    email: "alice@company.com"
    team: ->@Team:engineering
```

Relations use `->@Entity` syntax. They create typed links between entities.

## Multi-file projects

Organize large projects across files:

```
project/
├── schema.bit          # Entity definitions
├── data/
│   ├── users.bit       # User instances
│   └── teams.bit       # Team instances
├── tasks/
│   ├── sprint-42.bit   # Current sprint
│   └── backlog.bit     # Future work
└── flows/
    └── release.bit     # Release state machine
```

Each file is independent. Use `bit query` to search across all of them:

```sh
bit query '@User' project/**/*.bit
```

## Schema-first design

Define schemas before creating instances:

```bit
# schema.bit

define:@Customer
    id: ""^!
    name: ""!
    email: ""!
    plan: :free/:pro/:enterprise!
    mrr: 0.0##
    signed_up: ""@
    active: true?
    tags: []
    notes: {}
```

Then validate instances against the schema:

```sh
bit validate customers.bit --schema schema.bit
```

## Flows as documentation

Use flows to document processes, not just enforce them:

```bit
# Incident Response

flow:incident
    detected --> triaging --> investigating --> mitigating --> resolved --> postmortem
    investigating --> escalated --> investigating
    mitigating --> investigating

## Runbook
    [!] Check monitoring dashboard
    [!] Identify affected services
    [!] Notify on-call team
    [!] Begin mitigation
    [!] Write postmortem
```

## The expand/collapse workflow for teams

```sh
# Developer A: expand, edit, collapse
bit expand project.bitstore --output ./working
# ... edit .bit files ...
bit collapse ./working --output project.bitstore
git add project.bitstore
git commit -m "update project data"

# Developer B: pull, expand, work
git pull
bit expand project.bitstore --output ./working
```

## Using .bit with CI/CD

```yaml
# .github/workflows/validate.yml
name: Validate .bit files
on: push
jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo install bit-lang-cli
      - run: bit validate schema.bit
      - run: bit check suite.bit
      - run: |
          for f in data/*.bit; do
            bit validate "$f" --schema schema.bit
          done
```

## Tips

- .bit files are just text — use git diff, grep, sed, awk on them freely
- The CLI is designed for pipes — chain commands with `|`
- .bitstore is great for shipping data, .bit files are great for editing it
- Start simple: entities + tasks. Add flows and gates as your project grows.
- Everything in .bit is optional — use only the features you need
