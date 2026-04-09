# Phase 2 — Type System Ergonomics (v1.1)

_Last updated: 2026-04-09_

## Goal

Improve the type system so Aura is easier to write and maintain without losing the language's nominal, statically checked core.

## Priority

This comes after module resolution and diagnostics because the next type-system changes need stable symbol lookup and clear failure modes.

## Scope

- Expand limited inference where it is safe and predictable.
- Tighten rules for class, interface, and function type checking.
- Improve generic handling where the MVP currently relies on simple monomorphization assumptions.
- Keep nominal typing behavior explicit in the language contract.

## Implementation Notes

- Prefer inference only where the expected type is already obvious from context.
- Keep any new inference rule local and predictable, not flow-sensitive across large scopes.
- Write down the assignability matrix for classes, interfaces, primitives, and `void`.
- Decide whether generics stay monomorphized or gain more explicit type argument support in this phase.
- Make sure any new narrowing or inference rule has a corresponding negative test.

## Type System Themes

- Preserve explicit annotations where they improve readability.
- Add ergonomics only when they do not hide important control-flow or ownership assumptions.
- Keep interface and class typing aligned with the existing OOP model.

## TODO

- [ ] Add inference for obvious local bindings when the initializer fully determines the type.
- [ ] Add inference for function returns where all return sites agree and the signature is omitted.
- [ ] Define the generic story for this phase: keep monomorphization, or introduce explicit type arguments in limited form.
- [ ] Document the assignability rules between a class, its base classes, and implemented interfaces.
- [ ] Add tests for invalid overrides, invalid assignments, and missing annotations in ambiguous cases.
- [ ] Add tests for any new narrowing or promotion rule introduced in this phase.

## Acceptance

- [ ] Obvious locals and returns need fewer redundant annotations.
- [ ] The nominal OOP model still behaves consistently for classes and interfaces.
- [ ] Ambiguous code still fails with a clear type error rather than guessing.
- [ ] Type-checking behavior is documented well enough for future compiler passes to rely on it.
