# Phase 4 — IR + Lowering

_Last updated: 2026-04-07_

## Goal

Create a typed IR suitable for backend codegen.

## TODO

- [x] Decide IR staging: introduce HIR (optional) and MIR (recommended) (done 2026-04-08)
- [ ] HIR: desugar `for` → `while` (if `for` implemented)
- [ ] MIR: typed temporaries + explicit CFG blocks + terminators
- [ ] Lower expressions + statements into MIR with explicit evaluation order
- [ ] Provide `--emit=mir` output for fixtures

## Acceptance

- [ ] `aurac --emit=mir` produces stable, readable output for fixtures
- [ ] MIR has explicit blocks/branches/returns (no hidden control flow)

