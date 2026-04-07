# Phase 1 — Lexer + Parser + Diagnostics

_Last updated: 2026-04-07_

## Goal

Implement the parse stage and produce useful span-based diagnostics.

## TODO

- [ ] Implement token model with spans + trivia (for diagnostics/tooling)
- [ ] Lexer supports keywords/operators/punctuation per `docs/SYNTAX_DESIGN.md`
- [ ] Parser builds AST close to surface syntax (TS-like)
- [ ] Add error recovery sync points (e.g. `;`, `}`, `)`)
- [ ] Diagnostics type: span + message (+ optional help/note)
- [ ] Add snapshot tests for parser recovery and diagnostics formatting

## Acceptance

- [ ] `aurac check examples/hello/main.aura` parses (or emits structured diagnostics)
- [ ] Snapshot tests cover at least: missing `;`, unmatched `}`, bad token in expression

