# Phase 8 — Exceptions (MVP-friendly unwinding)

_Last updated: 2026-04-08_

## Goal

Implement `throw`, `try/catch/finally` using runtime-managed unwinding (MVP plan in `docs/ARCHITECTURE.md`).

## TODO

- [x] Lower `try/catch/finally` into MIR regions with explicit cleanup edges (done 2026-04-08)
- [x] Runtime: handler frames + current exception storage (done 2026-04-08)
- [x] Jump to catch entry using `setjmp/longjmp` approach (initial) (done 2026-04-08)
- [x] Ensure `finally` runs on: (done 2026-04-08)
  - [x] normal fallthrough
  - [x] `return`
  - [x] `throw`
- [ ] Enforce “exceptions do not cross C boundary” rule (document + guard)
- [ ] Add E2E tests proving `finally` always runs

## Acceptance

- [ ] `finally` executes in all exit paths (tests)
- [ ] Uncaught exception in `main` prints message and exits non-zero
