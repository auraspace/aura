---
sidebar_position: 1
---

# Introduction

**Aura** is a programming language toolchain written in Rust: a compiler, an interpreter, a small standard library, and a Language Server (LSP) for editor integrations.

## What’s in Aura

- **CLI compiler/interpreter**: `aura` (see `src/main.rs`)
- **Front-end**: lexer + parser + diagnostics
- **Semantic analysis**: type checking, symbol resolution, stdlib loading
- **Execution modes**:
  - **Interpreter** (`--interp`)
  - **Compiler** (native codegen)
  - **IR pipeline** (`--ir`) with optional `--emit-ir`
- **Targets**: `aarch64-apple-darwin` and `x86_64` (via `--target`)
- **LSP server**: `aura lsp` (hover, completion, go-to-definition, formatting, symbols)
- **VS Code extension**: available in `editors/vscode/`

## Installation

Install the latest version of Aura:

```bash
curl -fsSL https://raw.githubusercontent.com/auraspace/aura/master/scripts/install.sh | bash
```

To install a specific version (e.g., `v0.2.9`):

```bash
curl -fsSL https://raw.githubusercontent.com/auraspace/aura/master/scripts/install.sh | bash -s -- v0.2.9
```

## Build

Requires a recent Rust toolchain (edition 2021).

```bash
cargo build
```

## Running Aura Code

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
```
