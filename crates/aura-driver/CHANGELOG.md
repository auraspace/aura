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
- implement LLVM backend and native linking (Phase 6)
- implement Mid-level IR (MIR) generation and add --emit=mir flag to the compiler
- *(typeck)* finalize phase 3 - member access, assignments, and top-level calls
- implement --print=types and --emit=hir debug output modes
- implement class declaration parsing and add validation for this keyword usage
- *(parser)* add this/new expressions
- *(typeck)* add builtin types and validation
- *(resolver)* report duplicate bindings
- *(modules)* diagnose missing imports and check across files
- *(resolver)* resolve locals and report unknown identifiers
- *(resolver)* add per-module symbol table
- *(modules)* resolve relative imports by extension
- *(modules)* parse imports and scaffold module graph
- *(cli)* implement aurac check via aura-driver
- initialize project workspace with crates, CLI skeleton, and directory structure

### Fixed

- *(e2e)* restore failed tests
- *(typeck)* expression type registration and diagnostic de-duplication

### Other

- apply consistent code formatting across multiple crates
- *(resolver)* scaffold member access collection
