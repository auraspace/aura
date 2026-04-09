# Quality Gates (ongoing)

_Last updated: 2026-04-09_

## TODO (always-on)

- [x] Keep docs as contract; update in same diff on design changes (done 2026-04-09)
- [x] Add unit tests in each crate (lexer/parser/typeck/IR) (done 2026-04-09)
- [x] Add snapshot tests for diagnostics formatting (done 2026-04-09)
- [x] Add E2E tests: compile fixtures → run binary → assert stdout/stderr/exit code (done 2026-04-09)
- [x] Add debug modes: `--emit=ast|hir|mir|obj|asm`, `--print=types|symbols|imports` (done 2026-04-09)

## Pre-merge checklist

- [x] Syntax changed → update `docs/SYNTAX_DESIGN.md` (done 2026-04-09; aligned module syntax doc with implemented parser surface)
- [x] Pipeline/IR/runtime responsibilities changed → update `docs/ARCHITECTURE.md` (done 2026-04-09; aligned architecture doc with current workspace/backend/runtime shape)
- [x] Repo layout/crates changed → update `docs/FOLDER_STRUCTURE.md` (done 2026-04-09; aligned folder structure doc with actual workspace crates and top-level dirs)
- [x] Run guardrails: `bash .agents/skills/aura-dev-guardrails/scripts/check_guardrails.sh` (done 2026-04-09; contract/doc guardrails pass)
