# Changelog
All notable changes to this project will be documented in this file.

## [0.1.0](https://github.com/auraspace/aura/releases/tag/aura-codegen-llvm-v0.1.0) - 2026-04-09

### Added

- implement class inheritance and interface support in type checker and MIR lowering
- *(oop)* add direct method calls
- *(oop)* heap allocate class instances
- *(codegen)* define MVP backend interface
- implement runtime string conversion functions and update LLVM codegen to support implicit type casting for function calls
- implement LLVM backend and native linking (Phase 6)

### Fixed

- *(e2e)* restore failed tests
- *(exceptions)* add phase 8 coverage and backend fix
- *(exceptions)* complete finally and uncaught coverage

### Other

- add smoke coverage across core crates
