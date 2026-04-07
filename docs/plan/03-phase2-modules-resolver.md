# Phase 2 — Module Loader + Resolver

_Last updated: 2026-04-07_

## Goal

File-based modules + name resolution consistent with `import ... from "./path"`.

## TODO

- [x] Build module graph from entrypoint(s) (done 2026-04-07; import AST + driver graph scaffold)
- [x] Resolve relative imports, omit extension per MVP rules (done 2026-04-07; resolve `./`/`../` to `.aura`/`.ar`)
- [x] Per-module symbol table construction (done 2026-04-07; collect top-level + import bindings)
- [ ] Resolve locals and top-level names
- [ ] Resolve imports/exports (surface + diagnostics)
- [ ] Member-name resolution scaffold (structure only; full type-driven lookup later)

## Acceptance

- [ ] `aurac check` resolves across multiple files under `examples/`
- [ ] Diagnostics for: missing import target, unknown identifier, duplicate binding
