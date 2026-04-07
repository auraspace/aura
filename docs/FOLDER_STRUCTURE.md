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
├─ .github/
│  └─ workflows/            # GitHub Actions CI workflows
├─ crates/
│  ├─ aurac/                 # CLI + orchestration (build/check/run)
│  ├─ aura-driver/           # High-level "compile this project" API (no CLI)
│  ├─ aura-lexer/            # Tokenizer
│  ├─ aura-parser/           # AST + parser + error recovery
│  ├─ aura-ast/              # AST data structures (shared)
│  ├─ aura-span/             # Source spans, files, line/col mapping
│  ├─ aura-diagnostics/      # Errors, warnings, formatting
│  ├─ aura-hir/              # (Optional) lowered syntax tree
│  ├─ aura-typeck/           # Type checking + inference (minimal)
│  ├─ aura-mir/              # Typed IR (CFG) + utilities
│  ├─ aura-lower/            # HIR/AST -> MIR lowering
│  ├─ aura-codegen/          # Backend-agnostic codegen interface
│  ├─ aura-codegen-clif/     # Cranelift backend (optional)
│  ├─ aura-codegen-llvm/     # LLVM backend (optional)
│  ├─ aura-link/             # Linker abstraction + platform implementations
│  ├─ aura-target/           # Target triples, data layouts, feature flags
│  └─ aura-stdlib/           # (Later) language-level standard library sources
├─ runtime/
│  ├─ aura-rt/               # Runtime crate (builds staticlib)
│  ├─ include/               # C ABI headers (generated or hand-written)
│  └─ tests/                 # Runtime-level tests
├─ examples/
│  ├─ hello/
│  └─ oop/
├─ tests/
│  ├─ e2e/                   # Compile+run tests
│  ├─ fixtures/              # Small Aura programs
│  └─ snapshots/             # Parser/typeck diagnostics snapshots
├─ tools/
│  ├─ golden/                # Golden-file harness utilities
│  └─ ci/                    # CI scripts
├─ Cargo.toml                # Rust workspace root
└─ README.md
```

## Key Principles

- **One crate per responsibility**: small crates avoid cyclic dependencies.
- **Backends are plugins**: `aura-codegen` defines traits; backend crates implement them.
- **Targets are data**: `aura-target` provides target descriptions and normalization.
- **Driver is stable**: `aura-driver` is a library API usable by CLI, tests, and future editor tooling.

## Target Support Model

### `aura-target`

Centralize target logic:

- target triple parsing/normalization
- pointer size, endianness, OS/ABI
- CPU/features configuration
- data layout strings (if using LLVM)

Examples:

- `aarch64-apple-darwin` (MVP)
- `x86_64-apple-darwin` (next)
- `x86_64-unknown-linux-gnu` (later)

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
- (optional) `cdylib` for experimentation or embedding into other hosts

`runtime/include` contains the C ABI contract:

- `aura_rt.h` (or similar): runtime function signatures and structs used by codegen

This avoids "stringly typed" runtime calls in the compiler.

## Tests

Recommended testing layers:

- **Unit tests** in each crate (lexer, parser, type checker, MIR passes).
- **Snapshot tests** for diagnostics formatting and parser recovery.
- **E2E tests**: compile fixtures to a temp directory and run the produced binary.

Suggested conventions:

- `tests/fixtures/*.aura` are small programs with expected output.
- E2E harness compares stdout/stderr and exit codes.

## Multi-Target Extensibility Checklist

When adding a new target:

- Add a `TargetSpec` entry in `aura-target`.
- Ensure codegen can emit objects for that target (backend capability check).
- Implement or configure a linker strategy in `aura-link`.
- Add at least one E2E test that compiles and runs (or compiles-only for cross targets).
