# Plan 3 Phase 3: Real-time Feedback

## 📌 Goals

Connect the `SemanticAnalyzer` to the LSP to provide instant feedback for type errors, name resolution issues, and other semantic violations.

## 📝 Tasks

- [x] Update `SemanticAnalyzer` to collect multiple diagnostics instead of returning a single `Err`
- [x] Integrate `SemanticAnalyzer` into `src/lsp/server.rs`'s `on_change` loop
- [x] Map semantic errors to LSP `Diagnostic` objects
- [x] Implement `textDocument/hover` to show type information from the symbol table
- [x] Verify real-time feedback with type mismatch examples
