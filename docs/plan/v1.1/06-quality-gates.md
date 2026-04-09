# Quality Gates (v1.1)

_Last updated: 2026-04-09_

## Priority

This phase is always active, but it should be read as the enforcement layer that closes each prior phase rather than as a standalone feature slice.

## TODO (always-on)

- [ ] Keep `docs/ARCHITECTURE.md` aligned with target/runtime/backend changes.
- [ ] Keep `docs/FOLDER_STRUCTURE.md` aligned with crate and directory layout changes.
- [ ] Keep `docs/SYNTAX_DESIGN.md` aligned if v1.1 touches language syntax or semantics.
- [ ] Add or update tests when changing resolution, types, diagnostics, runtime, or backend behavior.
- [ ] Keep plan files small enough that `next_todo.py` stays useful.
- [ ] When a phase finishes, update its `TODO` and `Acceptance` checks in the same diff.
- [ ] Keep the next incomplete item obvious from the plan index.

## Pre-merge checklist

- [ ] Run guardrails: `bash .agents/skills/aura-dev-guardrails/scripts/check_guardrails.sh`
- [ ] Run the narrowest relevant test/build command for the touched area.
- [ ] Update the plan file state when a TODO is completed.
- [ ] Keep the change log or release notes current if the work is user-visible.
- [ ] Verify the updated phase still reads as a discrete implementation slice.

## Acceptance

- [ ] v1.1 work can be shipped incrementally without re-opening v1.0 assumptions.
- [ ] The next task in the roadmap is obvious from the docs alone.

## Roadmap reading order

The v1.1 plan is intentionally linear:

- Start with `00-contract.md` for the invariants and success criteria.
- Follow the numbered phase files in order.
- Treat the next unchecked item in the next numbered file as the default next task.

## Incremental delivery rule

Each v1.1 phase must remain independently shippable:

- Preserve the v1.0 single-binary workflow unless a phase explicitly replaces it.
- Keep new module, type, runtime, diagnostics, target, or backend capabilities behind the narrowest possible API surface.
- Avoid broad policy changes that force follow-up work across unrelated phases.
