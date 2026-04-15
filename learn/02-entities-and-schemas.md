# Entities & Schemas

Entities are the data model of .bit. They're like database tables, but defined inline in your documents.

## Defining an entity

```bit
define:@User
    name: ""!
    email: ""!
    role: :admin/:editor/:viewer!
    active: true?
    login_count: 0#
    score: 0.0##
    created_at: ""@
    tags: []
    metadata: {}
    team: ->@Team
```

The `define:` keyword creates a schema. `@User` is the entity name (always PascalCase with `@` prefix).

## Field sigils

Every field has a type sigil:

| Sigil | Type | Default | Example |
|-------|------|---------|---------|
| `!` | Required string | `""` | `name: ""!` |
| `#` | Integer | `0` | `count: 0#` |
| `##` | Float | `0.0` | `price: 0.0##` |
| `?` | Boolean | `true`/`false` | `active: true?` |
| `@` | Timestamp | `""` | `created_at: ""@` |
| `^` | Indexed | `""` | `id: ""^` |
| `[]` | List | `[]` | `tags: []` |
| `{}` | JSON blob | `{}` | `meta: {}` |
| `->` | Relation | `->@Entity` | `owner: ->@User` |

Fields without sigils are plain strings.

## Enum fields

Use the `:value/:value` syntax for enums:

```bit
define:@Ticket
    status: :open/:in_progress/:closed!
    priority: :low/:medium/:high/:critical!
```

## Creating instances (mutations)

```bit
mutate:@User:alice
    name: "Alice Chen"
    email: "alice@example.com"
    role: :admin
    active: true

mutate:@User:bob
    name: "Bob Smith"
    email: "bob@example.com"
    role: :editor
```

`mutate:@User:alice` creates or updates the User instance with id `alice`.

## Validating

```sh
# Validate that mutations match the schema
bit validate users.bit --schema schema.bit
```

The validator checks:
- Required fields are present
- Field types match sigils
- Enum values are in the allowed set
- Relations point to valid entities

## Converting from JSON

Already have data in JSON? Convert it:

```sh
echo '{"User": {"name": "alice", "email": "alice@example.com"}}' | bit convert - --from json
```

Output:
```bit
define:@User
    email: "alice@example.com"
    name: "alice"
```

## Tips

- Put schemas in a dedicated `schema.bit` file
- Use `mutate:` for instance data, keep it separate from `define:`
- Entity names are PascalCase: `@User`, `@TeamMember`, `@ApiEndpoint`
- Instance ids are lowercase: `@User:alice`, `@Team:engineering`
