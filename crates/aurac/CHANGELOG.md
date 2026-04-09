# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/auraspace/aura/releases/tag/v0.1.0) - 2026-04-09

### Added

- *(ci)* add GitHub workflows for automated release and binary packaging
- *(tests)* introduce aura-test-harness for enhanced end-to-end testing
- *(parser)* support export wrappers
- *(target)* introduce new target constructors and set default target to host
- implement class inheritance and interface support in type checker and MIR lowering
- *(oop)* add direct method calls
- *(oop)* heap allocate class instances
- *(codegen)* define MVP backend interface
- add infrastructure for pluggable codegen backends and introduce a placeholder Cranelift implementation
- implement runtime string conversion functions and update LLVM codegen to support implicit type casting for function calls
- implement LLVM backend and native linking (Phase 6)
- implement Mid-level IR (MIR) generation and add --emit=mir flag to the compiler
- *(typeck)* finalize phase 3 - member access, assignments, and top-level calls
- implement type checking system and environment management for Aura
- implement --print=types and --emit=hir debug output modes
- *(cli)* implement aurac check via aura-driver
- initialize project workspace with crates, CLI skeleton, and directory structure

### Fixed

- *(exceptions)* add phase 8 coverage and backend fix
- *(exceptions)* complete finally and uncaught coverage

### Other

- *(quality-gates)* add diagnostics snapshots and debug e2e coverage
- apply consistent code formatting across multiple crates
