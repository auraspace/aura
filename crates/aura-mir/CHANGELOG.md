# Changelog
All notable changes to this project will be documented in this file.

## [0.1.0](https://github.com/auraspace/aura/releases/tag/aura-mir-v0.1.0) - 2026-04-09

### Added

- *(parser)* support export wrappers
- *(exceptions)* lower try/catch/finally into MIR cleanup regions
- implement class inheritance and interface support in type checker and MIR lowering
- *(oop)* add direct method calls
- *(oop)* heap allocate class instances
- implement runtime string conversion functions and update LLVM codegen to support implicit type casting for function calls
- *(mir)* implement explicit evaluation order and short-circuiting
- implement Mid-level IR (MIR) generation and add --emit=mir flag to the compiler

### Fixed

- *(exceptions)* add phase 8 coverage and backend fix
- *(exceptions)* complete finally and uncaught coverage
- *(mir)* record throw cleanup edge

### Other

- add smoke coverage across core crates
- *(mir)* refine lowering to handle complex L-values and enforce evaluation order
