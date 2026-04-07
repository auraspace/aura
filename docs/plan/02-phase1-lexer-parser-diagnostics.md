# Phase 1 — Lexer + Parser + Diagnostics

_Last updated: 2026-04-07_

## Goal

Implement the parse stage and produce useful span-based diagnostics.

## TODO

- [x] Implement token model with spans + trivia (for diagnostics/tooling) (done 2026-04-07; `aura-span` + `aura-lexer` types)
- [x] Lexer supports keywords/operators/punctuation per `docs/SYNTAX_DESIGN.md` (done 2026-04-07; `aura_lexer::lex`)
- [x] Parser builds AST close to surface syntax (TS-like) (done 2026-04-07; `aura_parser::parse_program`)
- [ ] Add error recovery sync points (e.g. `;`, `}`, `)`)
- [ ] Diagnostics type: span + message (+ optional help/note)
- [ ] Add snapshot tests for parser recovery and diagnostics formatting

## Acceptance

- [ ] `aurac check examples/hello/main.aura` parses (or emits structured diagnostics)
- [ ] Snapshot tests cover at least: missing `;`, unmatched `}`, bad token in expression
