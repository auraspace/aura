# Phase 2 — Module Loader + Resolver

_Last updated: 2026-04-07_

## Goal

File-based modules + name resolution consistent with `import ... from "./path"`.

## TODO

- [x] Build module graph from entrypoint(s) (done 2026-04-07; import AST + driver graph scaffold)
- [x] Resolve relative imports, omit extension per MVP rules (done 2026-04-07; resolve `./`/`../` to `.aura`/`.ar`)
- [x] Per-module symbol table construction (done 2026-04-07; collect top-level + import bindings)
- [x] Resolve locals and top-level names (done 2026-04-07; unknown identifier diagnostics)
- [x] Resolve imports/exports (surface + diagnostics) (done 2026-04-07; missing import target + multi-file `check`)
- [x] Member-name resolution scaffold (structure only; full type-driven lookup later) (done 2026-04-07; collect member accesses)

## Acceptance

- [x] `aurac check` resolves across multiple files under `examples/` (done 2026-04-07; added `examples/modules/`)
- [x] Diagnostics for: missing import target, unknown identifier, duplicate binding (done 2026-04-07)
