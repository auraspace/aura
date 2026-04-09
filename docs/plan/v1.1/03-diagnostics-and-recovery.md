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

## Minimum Diagnostic Payload

Frontend diagnostics should consistently carry:

- `severity`
- `span`
- `message`
- optional `help`
- optional `note`

Keep the field order stable in the `Diagnostic` struct and in any constructors or formatting code that mirror it.

## Parser Sync Points

Current parser recovery should gracefully stop at these boundaries:

- `;` to resume after malformed statements and expression statements.
- `}` to resume at the end of a block or member list.
- `)` to resume after malformed parenthesized expressions, calls, and condition heads.
- Block-entry `{` after `if`, `while`, `new`, and parenthesized control-flow heads that can recover by skipping to the opening block.

## Diagnostics Themes

- Errors should point at the real source of failure when possible.
- Recovery should keep later errors visible instead of stopping at the first bad token.
- Diagnostics should stay compact, specific, and predictable.

## TODO

- [x] List the parser sync points for `;`, `}`, `)`, and block boundaries that should recover gracefully. (done 2026-04-09)
- [x] Define the minimum diagnostic payload for frontend errors and stabilize its field order or naming. (done 2026-04-09)
- [x] Add resolver diagnostics for missing imports, duplicate symbols, and unresolved exports. (done 2026-04-09)
- [x] Add type-check diagnostics for the most common assignability and return-type failures. (done 2026-04-09)
- [x] Add fix-it hints for errors with an obvious single edit. (done 2026-04-09)
- [x] Add regression tests that cover both recovered parsing and the final diagnostic output. (done 2026-04-09)

## Acceptance

- [x] One syntax mistake does not explode into unreadable follow-up errors. (done 2026-04-09)
- [x] Frontend failures include a stable code, span, and message shape. (done 2026-04-09)
- [x] Common mistakes produce actionable diagnostics with at least one clear hint when appropriate. (done 2026-04-09)
- [x] Recovery keeps enough of the tree intact for later phases to continue testing. (done 2026-04-09)
