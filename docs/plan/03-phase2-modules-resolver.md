# Phase 2 — Module Loader + Resolver

_Last updated: 2026-04-07_

## Goal

File-based modules + name resolution consistent with `import ... from "./path"`.

## TODO

- [ ] Build module graph from entrypoint(s)
- [ ] Resolve relative imports, omit extension per MVP rules
- [ ] Per-module symbol table construction
- [ ] Resolve locals and top-level names
- [ ] Resolve imports/exports (surface + diagnostics)
- [ ] Member-name resolution scaffold (structure only; full type-driven lookup later)

## Acceptance

- [ ] `aurac check` resolves across multiple files under `examples/`
- [ ] Diagnostics for: missing import target, unknown identifier, duplicate binding

