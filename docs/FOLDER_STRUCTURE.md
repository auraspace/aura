# Aura Folder Structure (Scalable, Multi-Target)

This document proposes a scalable repository layout for Aura. It assumes a Rust workspace with multiple crates, clear separation of concerns, and explicit support for multiple compilation targets.

Current focus target: `aarch64-apple-darwin`.

## Top-Level Layout

```
.
├─ docs/
│  ├─ ARCHITECTURE.md
│  ├─ FOLDER_STRUCTURE.md
│  └─ SYNTAX_DESIGN.md
├─ .agents/
│  ├─ skills/
│  └─ ...
├─ crates/
│  ├─ aura-ast/              # AST data structures (shared)
│  ├─ aura-diagnostics/      # Errors, warnings, formatting
│  ├─ aura-driver/           # High-level "compile this project" API (no CLI)
│  ├─ aura-lexer/            # Tokenizer
│  ├─ aura-parser/           # AST + parser + error recovery
│  ├─ aura-span/             # Source spans, files, line/col mapping
│  ├─ aura-typeck/           # Type checking + inference (minimal)
│  ├─ aura-mir/              # Typed IR (CFG) + utilities
│  ├─ aura-codegen/          # Backend-agnostic codegen interface
│  ├─ aura-codegen-clif/     # Cranelift backend placeholder
│  ├─ aura-codegen-llvm/     # LLVM backend implementation
│  ├─ aura-link/             # Linker abstraction + platform implementations
│  ├─ aurac/                 # CLI + orchestration (build/check/run)
│  └─ aura-test-harness/     # Shared end-to-end test helpers
├─ runtime/
│  ├─ aura-rt/               # Runtime crate (builds staticlib + rlib)
│  └─ include/               # C ABI headers (generated or hand-written)
├─ examples/
│  ├─ complex_hir.aura
│  ├─ hello/
│  ├─ modules/
│  ├─ exceptions/
│  ├─ oop/
│  └─ string_conversion/
├─ tests/
│  └─ fixtures/              # Parser/typeck/MIR/E2E fixtures
├─ scripts/
├─ cliff.toml
├─ INSTALL.md
├─ Cargo.toml                # Rust workspace root
├─ Cargo.lock
├─ README.md
├─ CHANGELOG.md
└─ LICENSE
```

## Key Principles

- **One crate per responsibility**: small crates avoid cyclic dependencies.
- **Backends are plugins**: `aura-codegen` defines traits; backend crates implement them.
- **Driver is stable**: `aura-driver` is a library API usable by CLI, tests, and future editor tooling.
- **Test harness stays reusable**: `aura-test-harness` provides shared E2E helpers without coupling tests to the CLI internals.
- **Current workspace stays lean**: planned crates like `aura-hir`, `aura-lower`, `aura-target`, and `aura-stdlib` are not part of the repository yet.

## Target Support Model

### `aura-target`

Target logic currently lives in `aura-codegen` and the CLI/backend crates.

- target triple parsing/normalization
- target descriptors that capture triple, object format, and support status
- pointer size, endianness, OS/ABI
- CPU/features configuration
- data layout strings (if using LLVM)

Examples:

- `aarch64-apple-darwin` (MVP)
- `x86_64-apple-darwin` (next)
- `x86_64-unknown-linux-gnu` (placeholder only; do not generate yet)

### `aura-link`

Provide a small abstraction over platform linking:

- On macOS, default to `clang` as the linker frontend (simplifies SDK integration).
- Accept explicit SDK/toolchain overrides through env vars and CLI flags.

Keep the interface simple:

- inputs: objects, static libs (runtime), system libs
- outputs: executable path + link map (optional)

## Runtime Layout

`runtime/aura-rt` should build:

- `staticlib` for embedding into executables
- `rlib` for intra-workspace reuse
- (optional later) `cdylib` for experimentation or embedding into other hosts

`runtime/include` contains the C ABI contract:

- `aura_rt.h` (or similar): runtime function signatures and structs used by codegen

This avoids "stringly typed" runtime calls in the compiler.

## Tests

Recommended testing layers:

- **Unit tests** in each crate (lexer, parser, type checker, MIR passes).
- **Snapshot tests** for diagnostics formatting and parser recovery.
- **E2E tests**: compile fixtures to a temp directory and run the produced binary.
- **Shared harness**: `aura-test-harness` provides reusable runner helpers for CLI-backed integration tests.

Suggested conventions:

- `tests/fixtures/*.aura` are small programs with expected output.
- E2E harness compares stdout/stderr and exit codes.

## Multi-Target Extensibility Checklist

When adding a new target:

- Add a `TargetSpec` entry in `aura-target`.
- Ensure codegen can emit objects for that target (backend capability check).
- Implement or configure a linker strategy in `aura-link`.
- Add at least one E2E test that compiles and runs (or compiles-only for cross targets).
