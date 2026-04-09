# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/auraspace/aura/releases/tag/aura-rt-v0.1.0) - 2026-04-09

### Added

- *(ci)* add GitHub workflows for automated release and binary packaging
- implement runtime string conversion functions and update LLVM codegen to support implicit type casting for function calls
- *(runtime)* implement minimal runtime ABI and staticlib

### Fixed

- *(e2e)* restore failed tests
- *(exceptions)* complete finally and uncaught coverage
- *(runtime)* wire setjmp/longjmp exception throw
- *(runtime)* add exception handler frame state

### Other

- format longjmp call in aura-rt for improved readability
