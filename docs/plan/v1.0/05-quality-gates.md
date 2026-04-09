# Quality Gates (v1.0)

_Last updated: 2026-04-09_

## TODO (always-on)

- [x] Keep `docs/ARCHITECTURE.md` aligned with target/runtime/backend changes. (done 2026-04-09)
- [ ] Keep `docs/FOLDER_STRUCTURE.md` aligned with crate and directory layout changes.
- [ ] Keep `docs/SYNTAX_DESIGN.md` aligned if v1.0 touches language syntax or semantics.
- [ ] Add or update tests when changing target, IR, runtime, or backend behavior.
- [ ] Keep plan files small enough that `next_todo.py` stays useful.

## Pre-merge checklist

- [ ] Run guardrails: `bash .agents/skills/aura-dev-guardrails/scripts/check_guardrails.sh`
- [ ] Run the narrowest relevant test/build command for the touched area.
- [ ] Update the plan file state when a TODO is completed.
- [ ] Keep the change log or release notes current if the work is user-visible.

## Acceptance

- [ ] v1.0 work can be shipped incrementally without re-opening v0 assumptions.
- [ ] The next task in the roadmap is obvious from the docs alone.
