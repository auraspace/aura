# Todo: Plan 2 Phase 1 - Advanced AST & Sema

**Focus**: Solidifying the strict, TypeScript-inspired Type System.

- [x] Update `ast/mod.rs` to support type annotations in variable declarations and parameters
- [x] Implement Union Types in `ast/mod.rs` and update Parser
- [x] Implement Generics in `ast/mod.rs` (class/function level) and update Parser
- [x] Enhance `sema/ty.rs` to represent Union and Generic types
- [x] Implement Structural Identity Check in `sema/checker.rs`
- [x] Implement exhaustive type checking for union types in `sema/checker.rs`
- [x] Add unit tests for Sema (tested via `.aura` files in `tests/`)
      to verify valid code passes and invalid code (like `any`) fails
