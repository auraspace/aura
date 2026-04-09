# Phase 2 — IR and Lowering Boundaries (v1.0)

_Last updated: 2026-04-09_

## Goal

Make the AST/HIR/MIR boundary easier to reason about so future language features can be lowered consistently without leaking frontend concerns into the backend.

## Scope

- Clarify what belongs in AST versus MIR.
- Preserve explicit control flow in MIR.
- Keep evaluation order, temporaries, and cleanup edges explicit.
- Treat lowering as the place where language sugar gets normalized.

## Lowering Contract

Lowering should produce MIR that is small, explicit, and free of frontend-only syntax:

- Expressions lower to values plus any required temporaries; subexpressions are evaluated left-to-right unless the operator definition says otherwise.
- Statements lower to explicit MIR blocks and terminators instead of relying on implicit fallthrough.
- Control-flow sugar such as `for`, short-circuiting, `return`, `break`, `continue`, `throw`, and `finally` is normalized into explicit branches and cleanup edges.
- Temporaries exist only to preserve evaluation order or lifetime; they must not reintroduce AST-shaped nesting into MIR.
- Cleanup insertion happens on every exit path, including normal returns and exceptional unwinds, before control transfers to the next block or handler.

### Evaluation Order

- Evaluate operands left-to-right unless a specific operator or call convention requires another order.
- Introduce a temporary only when a later subexpression would otherwise observe the wrong value or lifetime.
- Keep short-circuiting visible in MIR as an explicit branch, not as hidden expression nesting.

### Cleanup Semantics

- `return` first runs any pending cleanups for the current scope, then transfers to the function exit block.
- `throw` first runs any pending cleanups, then transfers to the nearest matching handler.
- `finally` is modeled as an explicit cleanup region that runs on normal and exceptional exits.
- Cleanup order is innermost to outermost so nested scopes behave predictably.

### MIR Example

Source:

```aura
function f(): i32 {
  try {
    return g(h())
  } finally {
    cleanup()
  }
}
```

Lowered shape:

```text
block0:
  t0 = call h()
  t1 = call g(t0)
  branch finally_exit(t1)

finally_exit(value):
  call cleanup()
  return value
```

## TODO

- [x] Write a compact lowering contract for expressions, statements, and control flow. (done 2026-04-09)
- [x] Document how temporaries and evaluation order are preserved. (done 2026-04-09)
- [x] Add a clear rule for cleanup insertion around `return`, `throw`, and `finally`. (done 2026-04-09)
- [x] Add MIR dump examples that show the lowering boundary. (done 2026-04-09)
- [x] Add regression tests for any new lowering edge cases. (done 2026-04-09)

## Acceptance

- [x] MIR remains the source of truth for backend consumption. (done 2026-04-09)
- [x] Lowering rules are documented well enough that new passes do not duplicate frontend logic. (done 2026-04-09)
- [x] Explicit cleanup and control-flow edges remain visible in debug output. (done 2026-04-09)
