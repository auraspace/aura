## [0.3.0](https://github.com/phamtanminhtien/aura/compare/v0.2.10...v0.3.0) (2026-03-17)

### 🚀 Features

- Add automated release management with Release Please. ([ac75f65](https://github.com/phamtanminhtien/aura/commit/ac75f6590a313399a55b7d729f80457bf1046de2))
- Implement static class members and methods, including access, assignment, and calls. ([cb656a3](https://github.com/phamtanminhtien/aura/commit/cb656a3670cc2ad786cab200a540decdd3063d86))

### 📚 Documentation

- Add installation instructions to the README. ([1006341](https://github.com/phamtanminhtien/aura/commit/10063411433c84584db213f47a814cbd20f7c73d))
- update changelog version headings to include GitHub compare links. ([33b2681](https://github.com/phamtanminhtien/aura/commit/33b2681c0a2333a2420ccb27f7de47596e1e4d0e))

### ⚙️ Miscellaneous Tasks

- Add CHANGELOG.md and .release-please-manifest.json to CI pull request trigger paths. ([6412b41](https://github.com/phamtanminhtien/aura/commit/6412b411cb23a1e695867314200dd6d9861d4bca))
- Add pull request title pattern configuration to release-please. ([7fdb41b](https://github.com/phamtanminhtien/aura/commit/7fdb41baeac436d5a83626afc6871be8129f945b))
- Remove '### ' prefix from changelog section headings in release-please-config. ([f7a65cb](https://github.com/phamtanminhtien/aura/commit/f7a65cb4254bc7011cc449b04bc46df3a5a1fdae))
- Remove git-cliff configuration from Cargo.toml and delete cliff.toml. ([f60ae2f](https://github.com/phamtanminhtien/aura/commit/f60ae2f3ddabd5ffcfd5884d940cdb5149b36741))

## [0.2.10](https://github.com/phamtanminhtien/aura/compare/v0.2.9...v0.2.10) (2026-03-15)

### 🚀 Features

- Implement abstract classes and methods, including lexer support, semantic checks, IR lowering for virtual calls, and new e2e tests for polymorphism and multiple interfaces.
- Implement formatting for `abstract` classes and methods, and update method declaration syntax to remove the `function` keyword.
- Explicitly declare class fields, constructors, and methods as public.
- Implement bitwise operators (`&`, `|`, `^`, `~`, `<<`, `>>`) across the frontend, IR, interpreter, and AArch64 backend.
- Introduce bitwise operator syntax highlighting, configure VS Code language server path, and update bitwise test file syntax.
- Add comprehensive floating-point number support to the language, compiler, and runtime.
- Implement a release script for automated builds and add build artifacts to gitignore.

## [0.2.9](https://github.com/phamtanminhtien/aura/compare/v0.2.8...v0.2.9) (2026-03-15)

### 🚀 Features

- Introduce pre-commit hooks for Rust formatting and clippy, and add a comment clarifying a known error in `core.aura`.
- Remove timer intrinsic functions, interpreter event loop, and associated stdlib and tests.
- Add new binaries for various language constructs and fix AArch64 string literal escaping.
- Add GitHub issue handling skill documentation detailing steps from information gathering to PR creation.
- Implement access modifiers (public, private, protected) and readonly properties for class members.
- Implement inheritance and virtual method dispatch, including `super` calls and vtable generation.
- Implement interfaces across the compiler pipeline, including lexing, parsing, semantic analysis, and IR lowering.
- Add support for `implements`, `extends`, and `override` keywords with corresponding formatter and syntax highlighting updates.

### 🐛 Bug Fixes

- Add missing expected output to 06_http_server.aura and remove unused import (#10)
- Bump missing version to 0.2.8.

### 💼 Other

- Split checker.rs into smaller modules
- Modularize LSP server handlers (formatted)
- Split formatter.rs into smaller modules. Closes #20
- Split parser.rs into smaller modules (#16)

### 🚜 Refactor

- Reorder `use` statements in `lsp/server.rs`.
- Reorganize e2e tests into categorized groups and update their file paths.
- Split interpreter into env and eval modules
- Split codegen.rs into a module and dedicated emit_expr/emit_stmt files

### 📚 Documentation

- Add documentation for union types, JSON handling design, and OOP concepts, and update existing syntax documentation.
- Remove documentation for Aura union types

### ⚙️ Miscellaneous Tasks

- Update changelog
- Release aura version 0.2.9
- Update release configuration and remove obsolete release script
- Update Rust version to 1.94 in Cargo.toml and add rust-toolchain.toml for toolchain management
- Update pre-release commit message format in Cargo.toml
- _(release)_ V0.2.9

### ◀️ Revert

- Rollback to 43f268a296f026057561efde84c301df00cdeccf

## [0.2.8](https://github.com/phamtanminhtien/aura/compare/v0.2.7...v0.2.8) (2026-03-14)

### 🐛 Bug Fixes

- Prevent redundant file processing and refine duplicate declaration checks. (#5)

### ⚙️ Miscellaneous Tasks

- Update changelog
- Release aura version 0.2.8

## [0.2.7](https://github.com/phamtanminhtien/aura/compare/v0.2.6...v0.2.7) (2026-03-13)

### 🚀 Features

- Implement esbuild for extension bundling and enhance activation logging with detailed context and error information.
- Register intrinsic functions with the semantic analyzer in LSP handlers for opened and changed documents.
- Implement network host resolution and update `print` statement syntax.

### 🚜 Refactor

- Standardize `print` statement syntax, class method declarations, and add explicit return types across e2e tests and stdlib.
- Update print statement syntax to remove parentheses.

### ⚙️ Miscellaneous Tasks

- Add path filtering to the pull request CI workflow trigger.
- Update changelog
- Release aura version 0.2.7

## [0.2.6](https://github.com/phamtanminhtien/aura/compare/v0.2.5...v0.2.6) (2026-03-13)

### 🚀 Features

- Implement version flag (`-v`, `--version`) to display the program version.
- Add `aura fmt` command and update Aura syntax for method declarations and print statements.

### 🐛 Bug Fixes

- Multi-line block comment formatting with normalized indentation and add a corresponding test.

### 🚜 Refactor

- Simplify target selection to exclusively support aarch64-apple-darwin by removing other platform-specific code.

### ⚙️ Miscellaneous Tasks

- Update changelog
- Release aura version 0.2.6

## [0.2.5](https://github.com/phamtanminhtien/aura/compare/v0.2.4...v0.2.5) (2026-03-13)

### 🚀 Features

- Add scripts for installing and uninstalling the Aura application.
- Embed C runtime code directly into the binary and update build drivers to compile it from an in-memory string.

### 🚜 Refactor

- Centralize Aura language server path resolution into a new helper function, improving discovery and updating configuration description.

### ⚙️ Miscellaneous Tasks

- Remove legacy raw binary asset creation from the release workflow.
- Update changelog
- Release aura version 0.2.5

## [0.2.4](https://github.com/phamtanminhtien/aura/compare/v0.2.3...v0.2.4) (2026-03-13)

### 🚀 Features

- Add x86_64-unknown-linux-gnu placehoder (#4)

### ⚙️ Miscellaneous Tasks

- Enhance release workflow to package binaries with stdlib and c… (#1)
- Complete (#2)
- Remove push trigger for master branch. (#3)
- Update changelog
- Release aura version 0.2.4

## [0.2.3](https://github.com/phamtanminhtien/aura/compare/v0.2.2...v0.2.3) (2026-03-13)

### ⚙️ Miscellaneous Tasks

- Update GitHub Actions workflow to add write permissions for contents
- Update changelog
- Release aura version 0.2.3

## [0.2.2](https://github.com/phamtanminhtien/aura/compare/v0.2.1...v0.2.2) (2026-03-13)

### ⚙️ Miscellaneous Tasks

- Refactor CI workflow to build and release binaries, add release script for changelog generation
- Improve release script to check for cargo-release availability and conditionally commit changelog
- Update changelog generation logic in release script to use git status for changes
- Update release script to execute cargo release command
- Update changelog
- Release aura version 0.2.2

## [0.2.1] (2026-03-13)

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
