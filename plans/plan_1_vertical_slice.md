# Plan 1: The "Vertical Slice" (End-to-End MVP)

**Focus**: _Proving the pipeline from Source to ARM64 Binary._

This plan focuses on getting a "Hello World" or basic arithmetic script compiled into a native ARM64 binary as quickly as possible.

## Phases

1.  **Phase 1: Minimalist Frontend**: Implement a basic Lexer and Parser for a subset of the language (Variables, `i32`, `+`, `-`, `print`).
2.  **Phase 2: Direct Codegen**: Skip complex IR; generate ARM64 assembly directly from the AST for simple expressions.
3.  **Phase 3: The Static Linker**: Implement the logic to bundle a tiny "dummy" runtime with the generated assembly into an executable (using `ld` or a custom linker).
4.  **Phase 4: Incremental Features**: Once "Hello World" works, add loops, then functions, then objects.
5.  **Phase 5: Refinement**: Introduce the SSA IR and full Semantic Analysis once the binary pipeline is stable.

> [!TIP]
> Use this plan if you want to see immediate results on your hardware and verify that the ARM64 emitter works correctly.
