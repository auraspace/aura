# Contract + Invariants (v1.1)

_Last updated: 2026-04-09_

## Contract docs (source of truth)

Before changing syntax, resolution, types, diagnostics, runtime behavior, or backend/target policy, re-check:

- `docs/ARCHITECTURE.md`
- `docs/FOLDER_STRUCTURE.md`
- `docs/SYNTAX_DESIGN.md`

If v1.1 work changes those contracts, update the relevant doc in the same diff.

## v1.1 invariants

These are the acceptance constraints for v1.1.

- Keep the MVP single-binary workflow working end-to-end.
- Preserve `aarch64-apple-darwin` as the primary supported target.
- Keep `x86_64-unknown-linux-gnu` placeholder-only until explicitly promoted.
- Keep the runtime embedded as a static library linked into the final executable.
- Keep backend selection explicit and fail fast on unsupported combinations.
- Keep exception semantics runtime-managed and avoid introducing OS zero-cost EH by accident.
- Keep docs in sync with repo layout whenever crates or top-level directories change.

## v1.1 success criteria

- Module and import handling are easier to reason about across multi-file projects.
- The type system becomes more ergonomic without silently weakening static guarantees.
- Diagnostics are more actionable, especially for parse and resolve failures.
- Runtime and standard library boundaries are documented tightly enough to support language growth.
- Backend and target policy remain centralized instead of spreading through CLI branches.
- Each phase can be worked on independently without reopening v1.0 assumptions.
