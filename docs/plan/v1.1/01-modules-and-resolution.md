# Phase 1 — Modules and Resolution (v1.1)

_Last updated: 2026-04-09_

## Goal

Make multi-file Aura projects easier to load, resolve, and reason about by tightening module boundaries and import/export behavior.

## Priority

This is the first implementation phase after the contract because every later frontend or type-system change depends on reliable module loading and symbol resolution.

## Scope

- Clarify module loading rules for file-based imports.
- Reduce ambiguity around export visibility and symbol lookup.
- Improve handling of circular or partially loaded module graphs.
- Keep resolution behavior explicit enough that future package support can build on it.

## Implementation Notes

- Start from the current file-based loader and write down the exact path resolution order.
- Decide whether import roots are searched before or after module-relative paths, then make that rule testable.
- Make `export` visibility rules explicit for functions, classes, interfaces, and values.
- Define one cycle policy: reject, partially load with diagnostics, or allow only certain forms.
- Keep resolver state transitions observable enough for tests to assert on them.

## TODO

- [ ] Write the module resolution order as a step-by-step algorithm.
- [ ] Define which declarations are exported by default, and which require `export`.
- [ ] Specify the error behavior for missing files, duplicate exports, and import cycles.
- [ ] Add tests for same-named symbols in different modules.
- [ ] Add tests for relative imports, module-root imports, and extensionless paths.
- [ ] Add tests for re-export or nested import scenarios if they are allowed.

## Acceptance

- [ ] A reader can resolve an import path from the docs alone.
- [ ] Exported symbols are discoverable without guessing hidden visibility rules.
- [ ] Cycle and missing-file behavior are documented and covered by tests.
- [ ] Multi-file resolution failures produce stable, actionable diagnostics.
