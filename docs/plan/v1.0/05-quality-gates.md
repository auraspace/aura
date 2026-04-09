# Quality Gates (v1.0)

_Last updated: 2026-04-09_

## TODO (always-on)

- [x] Keep `docs/ARCHITECTURE.md` aligned with target/runtime/backend changes. (done 2026-04-09)
- [x] Keep `docs/FOLDER_STRUCTURE.md` aligned with crate and directory layout changes. (done 2026-04-09)
- [x] Keep `docs/SYNTAX_DESIGN.md` aligned if v1.0 touches language syntax or semantics. (done 2026-04-09; no syntax or semantics changes in this pass)
- [x] Add or update tests when changing target, IR, runtime, or backend behavior. (done 2026-04-09; added harness unit tests)
- [x] Keep plan files small enough that `next_todo.py` stays useful. (done 2026-04-09)

## Pre-merge checklist

- [x] Run guardrails: `bash .agents/skills/aura-dev-guardrails/scripts/check_guardrails.sh` (done 2026-04-09)
- [x] Run the narrowest relevant test/build command for the touched area. (done 2026-04-09; `cargo check -p aura-test-harness`)
- [x] Update the plan file state when a TODO is completed. (done 2026-04-09)
- [x] Keep the change log or release notes current if the work is user-visible. (done 2026-04-09; internal-only change)

## Acceptance

- [x] v1.0 work can be shipped incrementally without re-opening v0 assumptions. (done 2026-04-09)
- [x] The next task in the roadmap is obvious from the docs alone. (done 2026-04-09)

## Roadmap reading order

The v1.0 plan is intentionally linear:

- Start with `00-contract.md` for the invariants and success criteria.
- Follow the numbered phase files in order.
- Treat the next unchecked item in the next numbered file as the default next task.

## Incremental delivery rule

Each v1.0 phase must remain independently shippable:

- Preserve the v0 single-binary workflow unless a phase explicitly replaces it.
- Keep new target, IR, runtime, or backend capabilities behind the narrowest possible API surface.
- Avoid broad policy changes that force follow-up work across unrelated phases.
