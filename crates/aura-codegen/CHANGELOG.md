# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/auraspace/aura/releases/tag/aura-codegen-v0.1.0) - 2026-04-09

### Added

- *(ci)* add GitHub workflows for automated release and binary packaging
- *(target)* introduce new target constructors and set default target to host
- *(codegen)* define MVP backend interface
- add infrastructure for pluggable codegen backends and introduce a placeholder Cranelift implementation
- implement LLVM backend and native linking (Phase 6)

### Other

- add smoke coverage across core crates
