# Aura

**Aura** is a programming language toolchain written in Rust: a compiler, an interpreter, a small standard library, and a Language Server (LSP) for editor integrations.

## What’s in this repo

- **CLI compiler/interpreter**: `aura` (see `src/main.rs`)
- **Front-end**: lexer + parser + diagnostics
- **Semantic analysis**: type checking, symbol resolution, stdlib loading
- **Execution modes**:
  - **Interpreter** (`--interp`)
  - **Compiler** (native codegen)
  - **IR pipeline** (`--ir`) with optional `--emit-ir`
- **Targets**: `aarch64-apple-darwin` and `x86_64` (via `--target`)
- **LSP server**: `aura lsp` (hover, completion, go-to-definition, formatting, symbols)
- **VS Code extension**: `editors/vscode/`

## Build

Requires a recent Rust toolchain (edition 2021).

```bash
cargo build
```

## Run Aura code

The CLI supports `run` (default), `build`, and `lsp`.

```bash
# Run (default)
cargo run -- tests/e2e/01_basic_types.aura

# Run with interpreter
cargo run -- --interp tests/e2e/23_math.aura

# Compile + run using the IR pipeline
cargo run -- --ir tests/e2e/03_arithmetic.aura

# Compile only (produces <input>_bin)
cargo run -- build tests/e2e/03_arithmetic.aura

# Select backend target
cargo run -- --ir --target x86_64 tests/e2e/03_arithmetic.aura

# Print IR and exit
cargo run -- --emit-ir tests/e2e/03_arithmetic.aura
```

### Stdlib / runtime paths

By default, Aura looks for:

- stdlib: `stdlib/std`
- runtime C: `src/runtime/runtime.c`

Override with:

- `AURA_STDLIB`: path to the stdlib root (e.g. `stdlib/std`)
- `AURA_RUNTIME`: path to the runtime C file (e.g. `src/runtime/runtime.c`)

Example:

```bash
AURA_STDLIB=./stdlib/std cargo run -- tests/e2e/01_basic_types.aura
```

## Language Server (LSP)

Start the LSP server (stdio):

```bash
cargo run -- lsp
```

If you use the VS Code extension, see `editors/vscode/README.md` for how to point it to the server binary.

## Tests

End-to-end tests run Aura programs from `tests/e2e/*.aura` and compare stdout with the `// Expected output:` header blocks.

```bash
cargo test --test e2e
```

You can switch the test execution mode:

```bash
cargo test --test e2e -- interp
cargo test --test e2e -- compiler
cargo test --test e2e -- ir
cargo test --test e2e -- interp compiler
```

(Previously used `AURA_TEST_MODE` environment variable, which is still supported as a fallback.)

## License

MIT. See `LICENSE`.
