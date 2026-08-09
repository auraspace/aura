# Technical Debt

Standing log of temporary workarounds, incomplete behavior, and deferred follow-ups.

Keep this file focused on active debt. When an item is resolved, remove it or
replace it with the smaller residual scope. Record implementation history in
commits, RFCs, or release notes instead of appending progress for every change.

## Open

### GC-001 concurrent tracing collector contract (2026-08-04)

- Area: `runtime/src/memory/gc.c`, generated heap ownership, task roots
- Symptom: generated heap classes use precise typed trace callbacks, while
  legacy opaque allocations remain conservative; collection is stop-the-world
  mark/sweep and has no explicit concurrent tri-color write-barrier or stack-map
  contract.
- Why deferred: the collector, compiler-generated barriers, and suspension-root
  metadata must be designed and sanitized together; conservative scanning is not
  a safe substitute for that contract.
- Next step: define the gray-work queue and write-barrier ABI, emit stack maps
  for live task roots, then add concurrent collector and race/sanitizer coverage.

### ASYNC-001 remaining aggregate/runtime ownership cases (2026-08-03)

- Area: async frames, task outcomes, channels, and generated ownership hooks
- Symptom: the main general CFG, typed failure/cancellation, suspension roots,
  and common aggregate paths are covered, but opaque aggregates without
  generated clone/drop/mark hooks remain rejected. Specialized lowerers and
  some legacy outcome paths still use compatibility behavior.
- Why deferred: every remaining layout needs an explicit backend-neutral
  ownership descriptor and sanitizer coverage; conservative scanning is not a
  safe substitute.
- Next step: migrate the remaining specialized paths to typed callbacks and
  expose raw typed failure payloads through the public join contract.

### ASYNC-002 richer async protocol shapes (2026-08-03)

- Area: async lowering, file I/O, channels, and native operation adapters
- Symptom: file operations and some native adapters do not yet suspend through
  the LLVM scheduler; richer iterator/protocol shapes, nested aggregate
  layouts, and async cancellation boundaries remain bounded.
- Why deferred: suspension, backpressure, cancellation, and ownership must be
  specified together for each operation family.
- Next step: define one async I/O adapter contract, then migrate file and
  network operations with pending/failure/cancellation tests.

### LAMBDA-001 remaining opaque aggregate captures (2026-08-09)

- Area: lambdas and spawned closures
- Symptom: primitive, String, ForeignHandle, Task/Channel, class, Array,
  interface, and common nested aggregate captures have explicit ownership, but
  opaque aggregate elements remain bounded or rejected.
- Why deferred: the remaining layouts need backend-neutral clone/drop/mark
  descriptors rather than another type-specific ownership branch.
- Next step: add typed ownership descriptors for opaque aggregate elements and
  sanitizer coverage for their closure escape paths.

### SAN-001 host-dependent sanitizer coverage (2026-07-28)

- Area: native/sanitizer fixtures and TCP integration
- Symptom: some TCP fixtures require ephemeral bind/network capabilities that are
  unavailable in the current local sandbox.
- Mitigation: `.github/workflows/ci.yml` now runs the complete sanitizer smoke
  matrix on Linux amd64, Linux arm64, and macOS arm64; CI artifacts retain the
  host/target matrix as release evidence.
- Next step: remove this entry after the first successful three-host CI run.

## Resolved History

The detailed progress log was intentionally removed from this file on
2026-08-03. The following milestones are retained as pointers, not active debt:

- 2026-07-20 to 2026-07-31: generic collections, ownership, lambdas, FFI,
  registry consumption, HTTP alpha paths, and bounded runtime fixtures.
- 2026-08-01 to 2026-08-02: general async CFG lowering, typed outcomes,
  suspension GC roots, aggregate clone/drop/mark hooks, channels, and spawn
  captures.
- 2026-08-03: backend-neutral Checked IR/MIR, state-machine validation,
  generic closure, ownership actions, and explicit C alpha fallback boundaries.
- 2026-08-04: API-003 compiler/runtime/tooling inventory closed for the
  Linux amd64/arm64 and macOS release/tooling matrix; Windows remains outside
  scope. The remaining collector work moved to GC-001, while overload-aware LSP
  results remain tracked by LSP-001.
- 2026-08-06: public package origin contract closed: publication is an
  immutable Git tag push, direct Git origins pin version/source/revision/checksum,
  warm-cache verification is fail-closed, and the versioned proxy read boundary
  is prepared without serving a proxy.
- 2026-08-09: LLVM specialized async operations are covered by target-neutral
  MIR emission and executable corpus checks for blocking workers, UDP, FD I/O,
  exact binary streams, TLS adapters, and cancellation/worker sanitizer paths.
- 2026-08-09: LSP navigation now consumes sema-selected `declaration_span`
  call facts for type-directed overloads, including same-arity overloads;
  hover/definition/reference/rename coverage is tested through the shared
  analysis boundary.
- 2026-08-09: LLVM mutable task/channel captures use shared pointer boxes,
  versioned task/channel ownership callbacks, and executable regression
  coverage; the residual lambda debt is now limited to opaque aggregate elements.

For exact evidence, use the relevant commit history, RFC, or test fixture rather
than restoring a per-change progress dump here.
