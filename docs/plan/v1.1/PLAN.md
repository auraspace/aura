# Aura v1.1 Roadmap (Draft)

_Last updated: 2026-04-09_

v1.1 focuses on making Aura easier to use in real projects without changing the v1.0 single-binary contract.

Each phase is written as a concrete implementation slice: goal, scope, detailed TODOs, and acceptance checks.

## Priority Order

Work through v1.1 in this order:

1. `00-contract.md` - lock the invariants before changing anything else.
2. `01-modules-and-resolution.md` - module loading and resolution are the base for multi-file work.
3. `03-diagnostics-and-recovery.md` - diagnostics improve every later phase and make failures easier to trust.
4. `02-type-system.md` - type ergonomics depend on stable resolution and better frontend errors.
5. `04-runtime-and-stdlib.md` - runtime and stdlib can grow once frontend behavior is clearer.
6. `05-backend-and-target-policy.md` - backend policy cleanup benefits from the stabilized frontend/runtime surface.
7. `06-quality-gates.md` - keep this active throughout, but use it as the final consistency pass for each slice.

## Draft Themes

1. Module/import resolution and multi-file project behavior
2. Diagnostics, recovery, and error consistency
3. Type system ergonomics and safer inference
4. Runtime ABI and standard library foundation
5. Backend/target policy and capability handling
6. Quality gates that keep docs and implementation aligned

## Draft Plan Files

1. `docs/plan/v1.1/00-contract.md`
2. `docs/plan/v1.1/01-modules-and-resolution.md`
3. `docs/plan/v1.1/03-diagnostics-and-recovery.md`
4. `docs/plan/v1.1/02-type-system.md`
5. `docs/plan/v1.1/04-runtime-and-stdlib.md`
6. `docs/plan/v1.1/05-backend-and-target-policy.md`
7. `docs/plan/v1.1/06-quality-gates.md`
