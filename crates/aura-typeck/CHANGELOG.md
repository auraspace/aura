# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/auraspace/aura/releases/tag/v0.1.0) - 2026-04-09

### Added

- *(ci)* add GitHub workflows for automated release and binary packaging
- *(parser)* support export wrappers
- *(exceptions)* lower try/catch/finally into MIR cleanup regions
- implement class inheritance and interface support in type checker and MIR lowering
- *(oop)* heap allocate class instances
- implement runtime string conversion functions and update LLVM codegen to support implicit type casting for function calls
- implement LLVM backend and native linking (Phase 6)
- implement Mid-level IR (MIR) generation and add --emit=mir flag to the compiler
- *(typeck)* finalize phase 3 - member access, assignments, and top-level calls
- implement type checking system and environment management for Aura
- implement --print=types and --emit=hir debug output modes
- enforce constructor-only `this` assignment and void return type rules
- implement class declaration parsing and add validation for this keyword usage
- *(parser)* add this/new expressions
- *(typeck)* check returns and return paths
- *(typeck)* check let/const and assignments
- *(typeck)* add builtin types and validation

### Fixed

- *(e2e)* restore failed tests
- *(exceptions)* add phase 8 coverage and backend fix
- *(typeck)* expression type registration and diagnostic de-duplication

### Other

- remove unused lookup_mut method from TypeEnv
- apply consistent code formatting across multiple crates
