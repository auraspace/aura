# Phase 3 — Type Checker (Minimal)

_Last updated: 2026-04-07_

## Goal

Minimal static typing for primitives + functions + classes/interfaces (nominal).

## TODO

- [x] Implement built-in types: `i32`, `i64`, `f32`, `f64`, `bool`, `string`, `void` (done 2026-04-07, validate `TypeRef` against built-ins)
- [x] Type-check variable declarations (`let/const`) and assignments (done 2026-04-07, infer from init + enforce const/assign types)
- [x] Type-check function params/returns + return-path checking (done 2026-04-07, check `return` types + missing return paths)
- [x] Parse/AST: `this` and `new` expressions (prereq for OOP typing) (done 2026-04-07)
- [x] Parse/AST: `class` declarations (fields + methods; no export yet) (done 2026-04-07)
- [x] Resolver: allow `this` inside methods (no field validation yet) (done 2026-04-07)
- [x] Type-check: `this.<field>` access and assignment (done 2026-04-07)
- [x] Type-check: direct instance method calls + `new` result typing (no vtables yet) (done 2026-04-07)
- [x] Type-check: constructor rules (`constructor` returns `void`, assigns fields) (done 2026-04-07; enforce constructor-only `this` assignments and void constructors)
- [x] Interface typing: nominal + `implements` checks (done 2026-04-08)
- [x] Add `--emit=hir` or `--print=types` debug output mode (done 2026-04-08)

## Acceptance

- [x] Reject: wrong argument types, wrong return type, missing return on non-`void`
- [x] Accept: small well-typed `examples/` programs with classes + methods
