# Phase 2 — IR and Lowering Boundaries (v1.0)

_Last updated: 2026-04-09_

## Goal

Make the AST/HIR/MIR boundary easier to reason about so future language features can be lowered consistently without leaking frontend concerns into the backend.

## Scope

- Clarify what belongs in AST versus MIR.
- Preserve explicit control flow in MIR.
- Keep evaluation order, temporaries, and cleanup edges explicit.
- Treat lowering as the place where language sugar gets normalized.

## TODO

- [ ] Write a compact lowering contract for expressions, statements, and control flow.
- [ ] Document how temporaries and evaluation order are preserved.
- [ ] Add a clear rule for cleanup insertion around `return`, `throw`, and `finally`.
- [ ] Add MIR dump examples that show the lowering boundary.
- [ ] Add regression tests for any new lowering edge cases.

## Acceptance

- [ ] MIR remains the source of truth for backend consumption.
- [ ] Lowering rules are documented well enough that new passes do not duplicate frontend logic.
- [ ] Explicit cleanup and control-flow edges remain visible in debug output.
