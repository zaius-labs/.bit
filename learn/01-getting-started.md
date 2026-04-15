# Getting Started with .bit

## Install the CLI

```sh
cargo install bit-lang-cli
```

This gives you the `bit` command.

## Your first .bit file

Create `hello.bit`:

```bit
# Hello World

[!] Learn .bit syntax
[!] Try the CLI
[x] Install bit-lang
```

## Parse it

```sh
bit parse hello.bit
```

This outputs a JSON AST. Every .bit file is a tree of nodes — groups, tasks, entities, flows.

## Format it

```sh
bit fmt hello.bit --write
```

Like `rustfmt` or `prettier`, but for .bit files. Enforces consistent indentation and style.

## Initialize a project

```sh
bit init my-project
cd my-project
ls
# schema.bit
```

`schema.bit` is the language reference (embedded in every bitstore as `@_system:schema`). Define your entities here or in separate .bit files.

## What's next?

- [Entities & Schemas](02-entities-and-schemas.md) — define typed data structures
- [Tasks & Flows](03-tasks-and-flows.md) — track work and state machines
- [CLI Reference](../../README.md#cli-reference) — all commands
