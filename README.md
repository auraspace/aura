# Aura

Aura is an OOP, statically typed programming language with a TypeScript-like surface syntax and a Go-like distribution model: `aurac` compiles `.aura` / `.ar` sources into a single native executable that can embed a small runtime (memory management, strings, arrays, panic, etc.).

Current focus target: `aarch64-apple-darwin`.

## Status

This repository is in early bring-up.

- `aurac check <FILE>` is implemented (parse + diagnostics).
- `aurac build` / `aurac run` are stubbed (not implemented yet).

## Quick start

Prerequisites: a working Rust toolchain.

Build:

```sh
cargo build -p aurac
```

Parse/check a file:

```sh
cargo run -p aurac -- check examples/hello/main.aura
```

## Docs

- Architecture: `docs/ARCHITECTURE.md`
- Syntax design: `docs/SYNTAX_DESIGN.md`
- Folder structure: `docs/FOLDER_STRUCTURE.md`
- MVP plan index: `docs/PLAN.md`

## Repo layout (high level)

- `crates/aurac`: CLI entrypoint
- `crates/aura-driver`: high-level API used by CLI/tests
- `crates/aura-lexer`, `crates/aura-parser`: frontend
- `runtime/`: embedded runtime (in progress)
- `examples/`: small Aura programs
- `tests/`: test harnesses/fixtures (in progress)

## License

MIT. See `LICENSE`.

