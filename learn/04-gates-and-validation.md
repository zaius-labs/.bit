# Gates & Validation

.bit uses ternary logic for validation — not just true/false, but true/false/unknown.

## Ternary logic

| State | Symbol | Meaning |
|-------|--------|---------|
| Positive | `Pos` | Condition met |
| Neutral | `Neutral` | Unknown/not evaluated |
| Negative | `Neg` | Condition failed |

This is Kleene logic. `Unknown AND True = Unknown`. You can't fake certainty.

## Gates

A gate is a set of conditions that must all be positive:

```bit
gate:ready_to_ship
    {tests_passing}
    {docs_updated}
    {changelog_written}
    {security_review_done}
```

Gates evaluate left-to-right. If any condition is Neutral or Negative, the gate blocks.

## Checks

Checks are executable validation suites:

```bit
validate:schema_check
    [!] All required fields present
    [!] No unknown entity references
    [!] Enum values valid
    [!] Relations resolve
```

Run them:

```sh
bit check myfile.bit
```

## Schema validation

```sh
# Define schema
echo 'define:@User
    name: ""!
    email: ""!' > schema.bit

# Validate data against schema
echo 'mutate:@User:alice
    name: "Alice"' > data.bit

bit validate data.bit --schema schema.bit
# Warning: missing required field 'email' on @User:alice
```

## Combining gates with flows

```bit
flow:deployment
    dev --> staging --> production

gate:staging_gate
    {all_tests_green}
    {performance_ok}

gate:production_gate
    {staging_gate}
    {product_approved}
    {rollback_plan_ready}
```

Gates can reference other gates. The production gate requires staging gate to pass first.

## Tips

- Use gates for hard blockers (deployment, releases)
- Use checks/validates for soft verification (linting, best practices)
- Ternary logic means "I don't know yet" is a valid state — better than defaulting to false
- `bit check` runs all validate: blocks in a file and reports pass/fail
