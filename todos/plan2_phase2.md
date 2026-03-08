# Plan 2 Phase 2: Flow-Sensitive Typing and Narrowing

## 📌 Goals

Enable the compiler to understand type narrowing (e.g., if a variable is `i32 | string`, inside an `if (v is i32)` block, it should be treated as `i32`).

## 📝 Tasks

- [x] Implement `is` operator in Lexer and Parser
- [x] Add `Expr::TypeTest(Box<Expr>, TypeExpr)` to AST
- [x] Enhance `SemanticAnalyzer` to support type narrowing in `if` conditions
- [x] Implement exhaustive checking for union types using flow-sensitive info
- [x] Update `ir/lower.rs` to handle tagged union representation for `i32 | string`
- [x] Add runtime support for type tags in `runtime/`
