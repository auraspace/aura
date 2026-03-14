# Refactor Issues - phamtanminhtien/aura

List of issues identified with the "refactor" keyword.

## [Issue #15](https://github.com/phamtanminhtien/aura/issues/15): Refactor: Split `src/compiler/sema/checker.rs` into smaller modules
- **Status**: open
- **Description**: The file `src/compiler/sema/checker.rs` is currently 1943 lines long, which makes it difficult to maintain and navigate.
- **Proposed refactor**: Split `checker.rs` into multiple modules under `src/compiler/sema/checker/`:
    - `mod.rs`: Main entry point and `SemanticAnalyzer` struct definition.
    - `expr.rs`: Expression type checking logic.
    - `stmt.rs`: Statement type checking logic.
    - `decl.rs`: Declaration checking logic.

## [Issue #16](https://github.com/phamtanminhtien/aura/issues/16): Refactor: Split `src/compiler/frontend/parser.rs` into smaller modules
- **Status**: open
- **Description**: The file `src/compiler/frontend/parser.rs` is 1390 lines long. Splitting it into smaller modules will improve readability.
- **Proposed refactor**: Split `parser.rs` into modules under `src/compiler/frontend/parser/`:
    - `mod.rs`: Parser struct and high-level parsing logic.
    - `expr.rs`: Expression parsing.
    - `stmt.rs`: Statement and declaration parsing.

## [Issue #17](https://github.com/phamtanminhtien/aura/issues/17): Refactor: Split `src/compiler/backend/aarch64_apple_darwin/codegen.rs`
- **Status**: open
- **Description**: The file `src/compiler/backend/aarch64_apple_darwin/codegen.rs` is 1360 lines long.
- **Proposed refactor**: Split into modules within `src/compiler/backend/aarch64_apple_darwin/`:
    - `emit_expr.rs`: Code generation for expressions.
    - `emit_stmt.rs`: Code generation for statements.

## [Issue #18](https://github.com/phamtanminhtien/aura/issues/18): Refactor: Split `src/compiler/interp/mod.rs` into smaller modules
- **Status**: open
- **Description**: The entry point for the interpreter `src/compiler/interp/mod.rs` has grown to 1102 lines.
- **Proposed refactor**: Split into separate files in `src/compiler/interp/`:
    - `env.rs`: Value and Environment definitions.
    - `eval.rs`: Core evaluation logic.

## [Issue #19](https://github.com/phamtanminhtien/aura/issues/19): Refactor: Split `src/compiler/ir/lower.rs` into smaller modules
- **Status**: open
- **Description**: The file `src/compiler/ir/lower.rs` is 995 lines long.
- **Proposed refactor**: Split into modules under `src/compiler/ir/lower/`:
    - `expr.rs`: Expression lowering.
    - `stmt.rs`: Statement lowering.

## [Issue #20](https://github.com/phamtanminhtien/aura/issues/20): Refactor: Split `src/compiler/frontend/formatter.rs` into smaller modules
- **Status**: open
- **Description**: The formatter logic in `src/compiler/frontend/formatter.rs` (938 lines) should be split into smaller, more manageable pieces.

## [Issue #21](https://github.com/phamtanminhtien/aura/issues/21): Refactor: Modularize `src/lsp/server.rs` and expand `src/lsp/handler/`
- **Status**: open
- **Description**: The LSP server logic in `src/lsp/server.rs` (925 lines) can be further modularized.
- **Proposed refactor**: Move feature-specific handlers (hover, completion, definition, etc.) into separate files within `src/lsp/handler/`.
