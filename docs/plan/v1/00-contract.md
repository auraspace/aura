# Contract + Invariants (MVP)

_Last updated: 2026-04-07_

## Contract docs (source of truth)

- [x] Re-check `docs/ARCHITECTURE.md` before major compiler/runtime work (done 2026-04-07; still matches MVP invariants)
- [x] Re-check `docs/FOLDER_STRUCTURE.md` before adding/moving crates/dirs (done 2026-04-07; aligns with planned Rust workspace layout)
- [x] Re-check `docs/SYNTAX_DESIGN.md` before changing syntax/semantics (done 2026-04-07; TS-like syntax scope is consistent)
- [x] If implementation diverges, update the relevant doc in the same diff (no silent divergence) (done 2026-04-07)

## MVP invariants (acceptance, not TODOs)

These are the acceptance constraints for MVP. Track implementation work in the phase plan files (so `next_todo.py` stays actionable).

- AOT single-binary output (`aurac` → one native executable)
- Embedded runtime shipped as static library, linked into the executable
- Stage separation: lexer/parser → resolver → typeck → lowering → codegen → link
- Initial target: `aarch64-apple-darwin`
- `x86_64-unknown-linux-gnu` is placeholder-only and must not be generated or treated as an active target until the plan explicitly promotes it
- TS-like surface syntax (`function`, `class`, `interface`, `import/export`, `let/const`)
- Exceptions in MVP are runtime-managed unwinding (not OS zero-cost EH)
