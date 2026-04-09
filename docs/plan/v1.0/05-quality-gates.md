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

- [ ] v1.0 work can be shipped incrementally without re-opening v0 assumptions.
- [ ] The next task in the roadmap is obvious from the docs alone.
