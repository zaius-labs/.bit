---
name: bit-rules
description: Manage enforced rules — list, add, toggle, test patterns. The onboarding skill for rule enforcement.
user-invocable: true
argument-hint: "[list|add|toggle|test|remove]"
---

# Manage .bit Rules

Manage @Rule entities in the project's .bit store. Parse the argument to determine which subcommand to run. If no argument is given, default to `list`.

## list

Show all rules with their enforcement status, patterns, and actions.

```bash
bit query "@Rule"
```

Format each rule as:

```
ID: @Rule:abc123
  text: "Always work on dev branch"
  enforced: true
  scope: global
  pattern: git checkout|git switch
  action: block
```

Group by enforcement status — show enforced rules first, then advisory rules.

## add

Ask the user for:
1. **Rule text** — what the rule says in plain language
2. **Pattern** — regex that would match violations (can be empty)
3. **Action** — block, warn, or log
4. **Scope** — global, directory, or file (default: global)
5. **Enforced** — true or false (default: true)

Then write the entity to a .bit file and collapse:

```bit
mutate:@Rule
    text: "<user's rule text>"!
    enforced: <true/false>?
    scope: :<scope>!
    pattern: "<regex>"
    action: :<action>!
```

Append the entity to CLAUDE.bit (or rules.bit if CLAUDE.bit does not exist). Then collapse:

```bash
bit collapse . --output project.bitstore
```

Verify with:

```bash
bit query "@Rule"
```

Confirm the new rule appears in the output.

## toggle

Ask the user which rule to toggle (by ID or text match). Then update the enforced field:

```bash
bit update @Rule:<id> enforced=true
```

or

```bash
bit update @Rule:<id> enforced=false
```

If `bit update` is not available, read the .bit file containing the rule, flip the enforced value manually, and re-collapse.

After toggling, confirm the new state:

```bash
bit query "@Rule"
```

## test

Ask the user for:
1. **Pattern** — the regex to test (can be from an existing rule or a new one)
2. **Sample** — a command, file path, or string to test against

Run the regex match and report:
- Whether the sample matches the pattern
- What the match captures (if any)

Use a simple inline test — no external tools needed:

```bash
echo "<sample>" | grep -P "<pattern>" && echo "MATCH" || echo "NO MATCH"
```

This helps users validate their patterns before adding them as rules.

## remove

Ask the user which rule to remove (by ID or text match).

```bash
bit delete @Rule:<id>
```

If `bit delete` is not available, read the .bit file containing the rule, remove the mutate:@Rule block, and re-collapse.

After removal, confirm:

```bash
bit query "@Rule"
```

Verify the rule no longer appears.
