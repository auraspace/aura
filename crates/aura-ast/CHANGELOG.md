# Changelog
All notable changes to this project will be documented in this file.

## [0.1.0](https://github.com/auraspace/aura/releases/tag/aura-ast-v0.1.0) - 2026-04-09

### Added

- *(parser)* support export wrappers
- *(exceptions)* lower try/catch/finally into MIR cleanup regions
- implement class inheritance and interface support in type checker and MIR lowering
- implement --print=types and --emit=hir debug output modes
- implement class declaration parsing and add validation for this keyword usage
- *(parser)* add this/new expressions
- *(modules)* parse imports and scaffold module graph
- *(parser)* build AST and parse TS-like surface
- initialize project workspace with crates, CLI skeleton, and directory structure

### Fixed

- *(e2e)* restore failed tests

### Other

- add smoke coverage across core crates
