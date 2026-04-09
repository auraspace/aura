# Phase 1 — Target Abstraction (v1.0)

_Last updated: 2026-04-09_

## Goal

Move target knowledge into a clearer abstraction so the compiler can reason about triples, object format, and linker behavior without hardcoding those details in multiple places.

## Scope

- Centralize target triple parsing and normalization.
- Represent target capabilities explicitly: pointer size, object format, platform linker, and placeholder status.
- Keep `aarch64-apple-darwin` as the primary active target.
- Keep `x86_64-unknown-linux-gnu` as placeholder-only.

## TODO

- [x] Introduce a target descriptor type that captures triple, format, and support status. (done 2026-04-09)
- [x] Move target-specific helper logic out of ad hoc CLI branches. (done 2026-04-09)
- [x] Add explicit capability checks for codegen and linking. (done 2026-04-09)
- [x] Document how unsupported targets fail and what error text they produce. (done 2026-04-09)
- [x] Add tests for target parsing and placeholder-target rejection. (done 2026-04-09)

## Acceptance

- [x] Supported targets are resolved through one stable API. (done 2026-04-09)
- [x] Unsupported targets fail before code generation with a clear diagnostic. (done 2026-04-09)
- [x] No backend or linker code relies on raw string matching for target policy. (done 2026-04-09)
