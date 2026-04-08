# Phase 8 — Exceptions (MVP-friendly unwinding)

_Last updated: 2026-04-07_

## Goal

Implement `throw`, `try/catch/finally` using runtime-managed unwinding (MVP plan in `docs/ARCHITECTURE.md`).

## TODO

- [x] Lower `try/catch/finally` into MIR regions with explicit cleanup edges (done 2026-04-08)
- [ ] Runtime: handler frames + current exception storage
- [ ] Jump to catch entry using `setjmp/longjmp` approach (initial)
- [ ] Ensure `finally` runs on:
  - [ ] normal fallthrough
  - [ ] `return`
  - [ ] `throw`
- [ ] Enforce “exceptions do not cross C boundary” rule (document + guard)
- [ ] Add E2E tests proving `finally` always runs

## Acceptance

- [ ] `finally` executes in all exit paths (tests)
- [ ] Uncaught exception in `main` prints message and exits non-zero
