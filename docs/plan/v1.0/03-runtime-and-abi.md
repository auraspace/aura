# Phase 3 — Runtime and ABI Stabilization (v1.0)

_Last updated: 2026-04-09_

## Goal

Harden the embedded runtime boundary so codegen can depend on a stable C ABI for allocations, strings, arrays, object dispatch, and exceptions.

## Scope

- Keep the runtime as a static library embedded into the final executable.
- Document the ABI boundary for generated code.
- Preserve current exception/unwinding behavior.
- Keep object layout and helper calls understandable from docs alone.

## TODO

- [ ] Document the runtime entry points currently used by codegen.
- [ ] Define which runtime APIs are stable and which are provisional.
- [ ] Describe object header fields and dispatch assumptions in one place.
- [ ] Add tests that validate runtime ABI usage from compiler output.
- [ ] Capture any header or layout changes in `runtime/include/`.

## Acceptance

- [ ] The compiler/runtime contract is explicit enough to keep backend and runtime changes decoupled.
- [ ] Runtime ABI changes are reflected in architecture docs before or with implementation.
- [ ] Exception and cleanup semantics remain aligned with the v0 contract.
