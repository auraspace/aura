# Plan 2 Phase 3: Tree-Walk Interpreter

## 📌 Goals

Build a simple Tree-Walk Interpreter to run Aura code directly from the AST/Sema state. This will allow for rapid testing of semantic rules without needing the full backend pipeline.

## 📝 Tasks

- [x] Define `Value` enum for runtime representation (Int, String, Bool, Object, Function)
- [x] Implement `Environment` for variable scoping (mapping names to `Value`)
- [x] Create `Interpreter` struct in `src/compiler/interp/mod.rs`
- [x] Implement `eval_expr` for all `Expr` variants
- [x] Implement `execute_stmt` for all `Statement` variants
- [x] Support Function Calls and Closures/Scoping
- [x] Support Class instantiation and Method calls
- [x] Update `main.rs` to add an `--interp` flag
- [x] Add integration tests for the Interpreter
