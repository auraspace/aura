# Quality Gates (ongoing)

_Last updated: 2026-04-07_

## TODO (always-on)

- [ ] Keep docs as contract; update in same diff on design changes
- [ ] Add unit tests in each crate (lexer/parser/typeck/IR)
- [ ] Add snapshot tests for diagnostics formatting
- [ ] Add E2E tests: compile fixtures → run binary → assert stdout/stderr/exit code
- [ ] Add debug modes: `--emit=ast|hir|mir|obj|asm`, `--print=types|symbols|imports`

## Pre-merge checklist

- [ ] Syntax changed → update `docs/SYNTAX_DESIGN.md`
- [ ] Pipeline/IR/runtime responsibilities changed → update `docs/ARCHITECTURE.md`
- [ ] Repo layout/crates changed → update `docs/FOLDER_STRUCTURE.md`
- [ ] Run guardrails: `bash .agents/skills/aura-dev-guardrails/scripts/check_guardrails.sh`

