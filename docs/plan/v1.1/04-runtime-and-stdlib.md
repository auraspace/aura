# Phase 4 — Runtime and Standard Library (v1.1)

_Last updated: 2026-04-09_

## Goal

Strengthen the runtime surface and start building out a minimal standard library so common programs need less compiler- or runtime-specific scaffolding.

## Priority

This sits after the core frontend phases because runtime and stdlib choices are easier to stabilize once the language surface they support is clearer.

## Scope

- Clarify which runtime responsibilities remain embedded in `libaura_rt.a`.
- Add or formalize missing helper APIs for common language features.
- Define the first useful standard library layer on top of the runtime.
- Keep object, string, array, and exception behavior aligned with `docs/ARCHITECTURE.md`.

## Implementation Notes

- Separate what is runtime ABI from what is merely a helper used by codegen today.
- Keep the stdlib thin at first: prefer a small number of stable modules with clear ownership.
- Make it explicit which features are runtime-only and which are available to user code.
- Document any object layout or helper assumptions before codegen starts relying on them.
- Add tests for the runtime-facing behavior that future language growth will depend on.

## Runtime Themes

- The runtime should stay small but dependable.
- Common helpers should live in a stable place instead of being reinvented in user code.
- The stdlib should grow conservatively and document what is guaranteed.

## TODO

- [ ] Audit the current runtime helper list and mark each helper as stable, provisional, or internal.
- [ ] Decide the first stdlib modules to ship, and define what each one owns.
- [ ] Document the string, array, object, and exception assumptions that generated code can rely on.
- [ ] Add runtime tests for allocation, string operations, and exception boundary behavior.
- [ ] Add or update architecture docs when the ABI or object layout changes.
- [ ] Record any new stdlib naming or import rules in the relevant docs.

## Acceptance

- [ ] The runtime ABI is explicit enough for backend and lowering work to depend on it safely.
- [ ] The first stdlib additions are documented, minimal, and tested.
- [ ] Runtime changes do not silently widen the language contract.
- [ ] Object, string, array, and exception behavior remain aligned with architecture docs.
