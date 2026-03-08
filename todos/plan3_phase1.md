# Plan 3 Phase 1: Robust Error Recovery

## 📌 Goals

Implement a Lexer and Parser that can handle incomplete or broken code, essential for high-quality IDE support and real-time feedback. Instead of stopping at the first error, the frontend will collect diagnostics and attempt to resynchronize.

## 📝 Tasks

- [x] Update `Lexer` to collect lexing errors instead of calling `panic!` or returning early
- [x] Add `Diagnostics` struct to manage a collection of error messages with span information
- [x] Implement error recovery in `Parser` using "Panic Mode" synchronization (e.g., skip to next semicolon or brace)
- [x] Update `Parser` methods to return `Result<Node, Error>` without stopping the entire parsing loop
- [x] Ensure the CLI can report multiple errors from a single run
- [x] Add tests with intentional syntax errors to verify recovery (e.g., missing semicolons, unclosed braces)
- [x] Implement `Parser::synchronize()` function to recover from common syntax errors
