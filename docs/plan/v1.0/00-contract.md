# Contract + Invariants (v1.0)

_Last updated: 2026-04-09_

## Contract docs (source of truth)

Before changing target handling, IR/lowering, runtime ABI, or backend/linking behavior, re-check:

- `docs/ARCHITECTURE.md`
- `docs/FOLDER_STRUCTURE.md`
- `docs/SYNTAX_DESIGN.md`

If v1.0 work changes those contracts, update the relevant doc in the same diff.

## v1.0 invariants

These are the acceptance constraints for v1.0.

- Keep the MVP single-binary workflow working end-to-end.
- Preserve `aarch64-apple-darwin` as the primary supported target.
- Keep `x86_64-unknown-linux-gnu` placeholder-only until explicitly promoted.
- Keep the runtime embedded as a static library linked into the final executable.
- Keep backend selection explicit and fail fast on unsupported combinations.
- Keep exception semantics runtime-managed and avoid introducing OS zero-cost EH by accident.
- Keep docs in sync with repo layout whenever crates or top-level directories change.

## v1.0 success criteria

- The compiler gains a cleaner place to describe targets without scattering triple logic through the CLI and backend crates.
- Lowering and runtime ABI responsibilities are documented tightly enough to support future language growth.
- Backend/linking behavior is explicit enough that a second backend can be added without redesigning the whole pipeline.
- Quality gates cover the new surface area and keep the docs honest.
