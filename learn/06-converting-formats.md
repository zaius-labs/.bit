# Converting Formats

.bit can convert to and from JSON and Markdown. This makes it easy to adopt .bit in existing projects.

## JSON to .bit

```sh
echo '{"User": {"name": "Alice", "email": "alice@co.com", "age": 30}}' | bit convert - --from json
```

Output:
```bit
define:@User
    age: 30
    email: "alice@co.com"
    name: "Alice"
```

**Arrays become multiple entities:**

```sh
echo '{"Users": [{"name": "Alice"}, {"name": "Bob"}]}' | bit convert - --from json
```

Output:
```bit
define:@Users
    name: "Alice"

define:@Users
    name: "Bob"
```

## Markdown to .bit

```sh
echo '# Sprint 1
- [ ] Build login page
- [x] Set up database
- [ ] Write tests

## Notes
Some context about the sprint.' | bit convert - --from md
```

Output:
```bit
# Sprint 1
    [!] Build login page
    [x] Set up database
    [!] Write tests

## Notes
Some context about the sprint.
```

Markdown `- [ ]` becomes `[!]` (pending), `- [x]` becomes `[x]` (completed).

## .bit to JSON

```sh
bit parse myfile.bit
```

The `parse` command outputs a JSON AST. For a simpler JSON export of entities:

```sh
echo 'define:@User
    name: "Alice"
    age: 30' | bit convert - --from bit --to json
```

## File-based conversion

```sh
bit convert data.json           # auto-detects format from extension
bit convert notes.md            # markdown → .bit
bit convert config.toml         # TOML → .bit (planned)
```

## Piping and composition

```sh
# Convert JSON API response to .bit
curl -s https://api.example.com/users | bit convert - --from json > users.bit

# Convert .bit to JSON for processing
bit parse users.bit | jq '.nodes[] | select(.kind == "Define")'

# Chain conversions
cat data.json | bit convert - --from json | bit fmt - | bit validate -
```

## Tips

- Use `--from` when piping from stdin (format can't be detected from extension)
- JSON objects map to `define:@Entity` blocks — keys become entity names
- Markdown headers map to groups, lists map to tasks
- `.bit` preserves more structure than either JSON or Markdown — conversion may lose some .bit-specific features when round-tripping through JSON
