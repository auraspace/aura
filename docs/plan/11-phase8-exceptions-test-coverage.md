# Phase 8b — Exceptions Test Coverage

_Last updated: 2026-04-09_

## Goal

Add missing tests for `throw`, `try/catch/finally`, and cleanup lowering so the exception pipeline is covered across parser, type checking, MIR, and end-to-end execution.

## TODO

- [x] Parser: reject `try` blocks without `catch` or `finally` (done 2026-04-09)
- [x] Parser: accept `try/catch` only and `try/finally` only (done 2026-04-09)
- [x] Parser: cover `catch` binding with optional type annotation (done 2026-04-09)
- [x] Typeck: ensure `catch` binding scope does not leak outside the clause (done 2026-04-09)
- [x] Typeck: verify return-guarantee behavior for `try`, `catch`, and `finally` combinations (done 2026-04-09)
- [x] MIR: cover `catch` cleanup edges, nested `try`, and `throw` inside `catch` (done 2026-04-09)
- [x] MIR: cover `return` inside `catch` and `finally` (done 2026-04-09)
- [x] E2E: verify `throw` is caught and program continues (done 2026-04-09)
- [x] E2E: verify `finally` runs after `throw`, including when `catch` returns (done 2026-04-09)
- [x] E2E: verify nested `try/catch/finally` execution order (done 2026-04-09)

## Acceptance

- [x] Parser, typeck, MIR, and e2e layers each have at least one regression test for exception control flow (done 2026-04-09)
- [x] `finally` behavior is covered for normal fallthrough, `return`, and `throw` (done 2026-04-09)
- [x] `catch` behavior is covered for successful handling and nested unwind (done 2026-04-09)
