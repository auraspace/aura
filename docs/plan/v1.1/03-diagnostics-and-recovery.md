# Phase 3 — Diagnostics and Recovery (v1.1)

_Last updated: 2026-04-09_

## Goal

Make parse and type errors easier to recover from and easier to understand, so the compiler is friendlier during iterative development.

## Priority

This phase is intentionally early because better diagnostics reduce friction across every later phase and make the resolution/type-system work safer to iterate on.

## Scope

- Improve parser recovery around common syntax mistakes.
- Make diagnostics more consistent across lexer, parser, resolver, and type checker.
- Keep stable error codes and spans as a first-class goal.
- Improve fix-it hints where the correct repair is obvious.

## Implementation Notes

- Treat diagnostics as a structured API, not just formatted strings.
- Standardize what every diagnostic should include: code, message, primary span, and optional notes or hints.
- Expand parser recovery at the statement and block boundaries that users hit most often.
- Keep recovery conservative enough that it does not invent syntax the user never wrote.
- Add tests that assert on the shape of the diagnostic, not only the text.

## Diagnostics Themes

- Errors should point at the real source of failure when possible.
- Recovery should keep later errors visible instead of stopping at the first bad token.
- Diagnostics should stay compact, specific, and predictable.

## TODO

- [ ] List the parser sync points for `;`, `}`, `)`, and block boundaries that should recover gracefully.
- [ ] Define the minimum diagnostic payload for frontend errors and stabilize its field order or naming.
- [ ] Add resolver diagnostics for missing imports, duplicate symbols, and unresolved exports.
- [ ] Add type-check diagnostics for the most common assignability and return-type failures.
- [ ] Add fix-it hints for errors with an obvious single edit.
- [ ] Add regression tests that cover both recovered parsing and the final diagnostic output.

## Acceptance

- [ ] One syntax mistake does not explode into unreadable follow-up errors.
- [ ] Frontend failures include a stable code, span, and message shape.
- [ ] Common mistakes produce actionable diagnostics with at least one clear hint when appropriate.
- [ ] Recovery keeps enough of the tree intact for later phases to continue testing.
