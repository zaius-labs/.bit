# Contributing to bit-lang

## Getting started

```sh
git clone https://github.com/zaius-labs/dotbit
cd dotbit
cargo test --workspace
```

## Project structure

```
crates/
  bit-core/          Pure Rust parser, IR, interpreter, renderer, validator
  bit-store/         Page-based document store (B-tree indexed)
  bit-cli/           CLI binary (bit command)
  bit-wasm/          WASM bindings for JavaScript/npm
  bit-python/        Python bindings via PyO3
docs/              Lesson book and guides
tests/             Integration and conformance tests
```

## Development workflow

```sh
# Run all tests
cargo test --workspace

# Check for lint issues
cargo clippy --workspace

# Format code
cargo fmt --all

# Build the CLI
cargo build -p bit-lang-cli

# Test the CLI
echo '# Hello\n[!] Task' | cargo run -p bit-lang-cli -- parse -
```

## Adding a new feature

1. Write tests first (TDD)
2. Implement the feature in the appropriate crate
3. Run `cargo test --workspace` and `cargo clippy --workspace`
4. Update documentation if needed
5. Submit a PR

## Code style

- Follow existing patterns in the codebase
- Keep modules focused — one concept per file
- Use `Result<T, E>` for fallible operations, not panics
- All public types should derive `Serialize, Deserialize` for JSON interop
- Tests go in the same file (`#[cfg(test)] mod tests`) for unit tests, or in `tests/` for integration tests

## Reporting issues

File issues at https://github.com/zaius-labs/dotbit/issues

## License

By contributing, you agree that your contributions will be licensed under MIT.
