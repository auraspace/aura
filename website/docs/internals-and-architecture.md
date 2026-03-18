---
title: 9. Internals & Architecture
sidebar_position: 9
---

# Internals & Architecture (For Contributors)

If you wish to contribute to the language's development, understanding its architecture is critical.

## Compiler Architecture
1. **Frontend:** Generates a stream of Tokens, constructing the Abstract Syntax Tree (AST).
2. **Semantic Analysis:** Walks the AST to enforce the type system and scoping rules.
3. **IR Generation:** Yields a high-level representation (Intermediate Representation).
4. **Backend:** Emits machine code. AArch64 (Apple Silicon) is the primary targeted backend before x86_64.

## Runtime System
All compilation outputs embed a lightweight statically linked runtime. This runs our generational GC and OS platform abstraction layers.

## Contributing Guide
Please read `CONTRIBUTING.md` at the project roots for setting up your local Rust environment and sending Pull Requests to the compiler monorepo.
