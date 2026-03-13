## [unreleased]

### 🚀 Features

- Add x86_64-unknown-linux-gnu placehoder (#4)

### ⚙️ Miscellaneous Tasks

- Enhance release workflow to package binaries with stdlib and c… (#1)
- Complete (#2)
- Remove push trigger for master branch. (#3)
## [0.2.3] - 2026-03-13

### ⚙️ Miscellaneous Tasks

- Update GitHub Actions workflow to add write permissions for contents
- Update changelog
- Release aura version 0.2.3
## [0.2.2] - 2026-03-13

### ⚙️ Miscellaneous Tasks

- Refactor CI workflow to build and release binaries, add release script for changelog generation
- Improve release script to check for cargo-release availability and conditionally commit changelog
- Update changelog generation logic in release script to use git status for changes
- Update release script to execute cargo release command
- Update changelog
- Release aura version 0.2.2
## [0.2.1] - 2026-03-13

### 🚀 Features

- Implement robust error recovery and initialize project
- Implement basic LSP infrastructure and integrate with CLI
- Refactor semantic analyzer to collect diagnostics and integrate AST spans
- Integrate semantic diagnostics and hover support into LSP
- Implement Go to Definition in LSP and finalize Phase 3
- Implement doc comments and advanced LSP
- Implement unary operations, static members, and stdlib support
- Implement Plan 4 Phase 1 - Generational GC (heap + sweep)
- Implement Plan 4 Phase 2 - async executor and work-stealing scheduler
- Implement Plan 4 Phase 3 - FFI & System Hooks
- Implement Plan 4 Phase 4 - Compiler-Runtime Contract
- Implement Plan 4 Phase 5 - Full Codegen
- Implement Plan 5 Phase 1 & 3 - IR Specification and Text Format
- Implement Plan 5 Phase 5 - Platform Expansion (x86_64)
- Implement import and export statement parsing
- Add template literal support to lexer, parser, and interpreter
- Implement async/await and promise support
- Implement string concatenation and comparison in the interpreter, update agent trigger rules to manual, and refine compiler handling for async, template, and error expressions alongside async E2E test adjustments.
- Implement Promise static methods and array literal support in the interpreter.
- Implement intrinsic functions, standard library modules (net, http, fs, array, string, core), try/catch error handling, and logical operators.
- Add `std/date` and `std/timer` modules, introduce `Int64` type, enhance string concatenation, and update function value representation.
- Integrate type information into codegen, expand built-in runtime support, and improve the build process with temporary file management.
- Add VSCode extension for Aura language support and rename project from `aura-rust` to `aura`.
- Enhance AST with detailed span tracking, introduce 'number' type alias, improve stdlib path resolution, and add VSCode extension icons.
- Implement module import/export resolution in semantic analysis and ARM64 code generation.
- Refactor standard library loading to use `core.aura` from a specified path instead of hardcoded class definitions.
- Add `UndefinedFunction` semantic error detection and a corresponding test case.
- And VSCode extension configuration updates (Restart Server).
- Implement multi-file compilation with per-file type tracking and core module integration, and add ARM64 stack helpers.
- Overhaul CLI argument parsing to support explicit commands, options, and a detailed help message, controlling build and run behavior.
- Implement cross-file "go to definition" functionality by adding file path information to symbol definitions and updating import statement parsing.
- Add built-in constants, pre-declare variables during semantic analysis, and normalize import paths to always include the `.aura` extension.
- Enhance semantic analysis with duplicate declaration checks and introduce ARM64 register definitions for code generation.
- Add semantic checks for duplicate field and method declarations, including name conflicts.
- Add keyword and built-in function completion items to the LSP server.
- Add `const` keyword support and enforce immutability for constant declarations in the semantic checker.
- Implement LSP completion for imported symbols, utilizing an `is_exported` flag and adding import test files.
- Introduce a new frontend AST formatter
- Enhance VS Code extension with server restart and logging, and add HTTP client and timer to standard library.
- Broaden nullable type comparison in semantic checker to include Union and Unknown types.
- Improve codegen for string and array methods, including 'read', and refine HTTP listen handler type signature.
- Implement enum declarations with support for numeric and string members, including full compiler pipeline and LSP integration.
- Enhance formatter to preserve manual blank lines and intelligently add blank lines before comments and doc comments.
- Automatically call the `main` function if it exists and is not explicitly invoked by the program.
- Add project metadata including description, license, and README for Aura toolchain
- Add CI and release workflows, configure changelog generation, and update Cargo metadata for versioning
- Update Cargo metadata with repository, homepage, and documentation links for Aura toolchain

### 🐛 Bug Fixes

- Prevent duplicate .aura extension when resolving standard library

### 🚜 Refactor

- Strictly enforce 'function' keyword and remove 'fn' support

### 📚 Documentation

- Update documentation comments to JSDoc style across stdlib modules.

### 🧪 Testing

- Add comprehensive E2E test suite covering syntax.md features
- Add e2e test for async/await
- Add expected output to 22_async_test.aura

### ⚙️ Miscellaneous Tasks

- Release aura version 0.2.0
- Disable package publishing in Cargo.toml
- Release aura version 0.2.1
