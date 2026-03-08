# Plan 3 Phase 4: Self-Hosted Stdlib Docs

## 📌 Goals

Write the core library in Aura itself and upgrade the LSP to support autocomplete, documentation generation, and cross-file navigation.

## 📝 Tasks

- [ ] Initialize `stdlib/` directory and create `io.aura` and `math.aura`
- [ ] Add support for "Doc Comments" (e.g., `///`) in the Lexer and AST
- [ ] Implement `textDocument/completion` for basic identifier and member access
- [ ] Implement `textDocument/documentSymbol` to provide an outline of the current file
- [ ] Update `SemanticAnalyzer` to load and analyze `stdlib` files automatically
