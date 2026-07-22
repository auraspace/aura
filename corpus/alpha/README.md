# Alpha contract corpus

This directory is reserved for v0.1.1-alpha fixtures that are not part of the
legacy language corpus. Existing fixtures remain valid and are referenced by
`docs/plans/v0.1.1-alpha/contract-matrix.tsv`.

## Layout

- `frontend/` — syntax and type-checking fixtures
- `diagnostics/` — expected compiler failures
- `runtime/` and `async/` — native execution fixtures
- `io/` and `http/` — I/O and HTTP contract fixtures
- `build/`, `registry/`, and `ffi/` — toolchain boundary fixtures
- `golden/` — normalized output expectations
- `expected/` — deferred or blocked behavior with an owner and reason

Fixture names should match the contract matrix ID where a new fixture is the
acceptance owner. A fixture must be deterministic and must not require network
access unless its stage is explicitly marked `network`.

Golden output is updated deliberately by the owning workstream. CI never
overwrites golden files; a golden change must be a reviewable diff accompanied
by the matrix ID and the reason for the update.
