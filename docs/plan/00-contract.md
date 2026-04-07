# Contract + Invariants (MVP)

_Last updated: 2026-04-07_

## Contract docs (source of truth)

- [ ] Re-check `docs/ARCHITECTURE.md` before major compiler/runtime work
- [ ] Re-check `docs/FOLDER_STRUCTURE.md` before adding/moving crates/dirs
- [ ] Re-check `docs/SYNTAX_DESIGN.md` before changing syntax/semantics
- [ ] If implementation diverges, update the relevant doc in the same diff (no silent divergence)

## MVP invariants

- [ ] AOT single-binary output (`aurac` → one native executable)
- [ ] Embedded runtime shipped as static library, linked into the executable
- [ ] Stage separation: lexer/parser → resolver → typeck → lowering → codegen → link
- [ ] Initial target: `aarch64-apple-darwin`
- [ ] TS-like surface syntax (`function`, `class`, `interface`, `import/export`, `let/const`)
- [ ] Exceptions in MVP are runtime-managed unwinding (not OS zero-cost EH)

