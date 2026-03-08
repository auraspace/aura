---
trigger: always_on
glob: "**/*.{rs,aura,md}"
description: "Rules for maintaining the Aura Project Structure"
---

# Project Structure Rules

All development within the `aura-rust` project must adhere to the following directory structure. Any new modules or files must be placed according to these definitions.

## 🛠 Directory Layout

```text
aura-rust/
├── Cargo.toml             # Project manifest and dependencies
├── src/
│   ├── main.rs            # CLI entry point (driver for compiler, lsp, etc.)
│   ├── lib.rs             # Core library exporting compiler and runtime
│   ├── compiler/          # High-level compiler orchestration
│   │   ├── ast/           # Abstract Syntax Tree definitions
│   │   ├── frontend/      # Lexical analysis and parsing
│   │   │   ├── lexer.rs   # Scanner: Source text -> Tokens
│   │   │   ├── parser.rs  # Parser: Tokens -> AST
│   │   │   └── token.rs   # Token definitions and kinds
│   │   ├── sema/          # Semantic analysis and Validation
│   │   │   ├── checker.rs # Type checking and inference
│   │   │   ├── scope.rs   # Symbol tables and scoping
│   │   │   └── ty.rs      # Aura type system representation
│   │   ├── ir/            # Intermediate Representation (SSA style)
│   │   │   ├── builder.rs # IR construction utilities
│   │   │   └── instr.rs   # Instruction set architecture-agnostic
│   │   └── backend/       # Native code generators
│   │       ├── codegen.rs # Common backend traits and logic
│   │       ├── arm64/     # Primary: AArch64 registers & instructions (Priority)
│   │       │   ├── reg.rs # Register allocator for ARM64
│   │       │   └── asm.rs # Assembler/Emitter for ARM64
│   │       └── x86_64/    # Secondary: Intel/AMD backend
│   ├── runtime/           # Language Runtime (statically linked)
│   │   ├── gc/            # Generational Garbage Collector
│   │   │   ├── heap.rs    # Allocation and management
│   │   │   └── sweep.rs   # Garbage identification logic
│   │   ├── scheduler/     # Async executor for Promises/Tasks
│   │   └── ffi/           # Platform-specific system calls (Linux, macOS)
│   └── lsp/               # IDE Support (Language Server)
│       ├── server.rs      # LSP message handler
│       └── handler/       # Hover, Completion, Definition logic
├── stdlib/                # Core library written in Aura (.aura files)
├── docs/                  # Architecture and syntax specs
└── tests/                 # Integration tests (Aura source -> binary execution)
```

## ⚠️ Important Constraints

1.  **Strict Typing**: No `any` or `unknown` types are permitted in the Aura language design.
2.  **ARM64 First**: The `backend/arm64` module is the primary focus and must be implemented before `x86_64`.
3.  **Self-Contained**: The compiler must generate a single binary including the `runtime` (statically linked).
4.  **Custom Backend**: Avoid heavy external frameworks; use a custom backend infrastructure similar to Go's approach.
5.  **LSP Support**: All core language features must be exposed via the `lsp/` module for IDE support.
