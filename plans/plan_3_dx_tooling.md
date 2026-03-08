# Plan 3: The "DX & Tooling" (LSP-First)

**Focus**: _Developer Experience and Early Adoption._

Aura is designed for modern developers. This plan builds the LSP alongside the compiler to ensure the language is usable and helpful from day one.

## Phases

1.  **Phase 1: Robust Error Recovery**: Implement a Lexer and Parser that can handle incomplete or broken code (essential for IDE support).
2.  **Phase 2: LSP Infrastructure**: Build the `lsp/server.rs` to handle "Hover", "Go to Definition", and "Diagnostics".
3.  **Phase 3: Real-time Feedback**: Connect the Sema module to the LSP so users get instant red-squiggles for type mismatches (especially important for strict typing).
4.  **Phase 4: Self-Hosted Stdlib Docs**: Write the `stdlib` in Aura and use the LSP to generate documentation and autocomplete.
5.  **Phase 5: Performance Backend**: Optimize the backend after the toolchain is already providing a great experience.

> [!NOTE]
> High-quality tooling (LSP) makes development faster for the compiler writers themselves!
