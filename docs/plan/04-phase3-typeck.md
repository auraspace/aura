# Phase 3 — Type Checker (Minimal)

_Last updated: 2026-04-07_

## Goal

Minimal static typing for primitives + functions + classes/interfaces (nominal).

## TODO

- [x] Implement built-in types: `i32`, `i64`, `f32`, `f64`, `bool`, `string`, `void` (done 2026-04-07, validate `TypeRef` against built-ins)
- [x] Type-check variable declarations (`let/const`) and assignments (done 2026-04-07, infer from init + enforce const/assign types)
- [x] Type-check function params/returns + return-path checking (done 2026-04-07, check `return` types + missing return paths)
- [ ] Class typing: fields, methods, `this` typing, constructor rules
- [ ] Interface typing: nominal + `implements` checks
- [ ] Add `--emit=hir` or `--print=types` debug output mode

## Acceptance

- [ ] Reject: wrong argument types, wrong return type, missing return on non-`void`
- [ ] Accept: small well-typed `examples/` programs with classes + methods
