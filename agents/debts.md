# Technical Debt

Standing log of temporary workarounds, incomplete behavior, and deferred follow-ups.

Keep this file focused on active debt. When an item is resolved, remove it or
replace it with the smaller residual scope. Record implementation history in
commits, RFCs, or release notes instead of appending progress for every change.

## Open

### GC-001 concurrent tracing collector contract (2026-08-04)

- Area: `runtime/src/gc_ownership.c`, generated heap ownership, task roots
- Symptom: generated heap classes use precise typed trace callbacks, while
  legacy opaque allocations remain conservative; collection is stop-the-world
  mark/sweep and has no explicit concurrent tri-color write-barrier or stack-map
  contract.
- Why deferred: the collector, compiler-generated barriers, and suspension-root
  metadata must be designed and sanitized together; conservative scanning is not
  a safe substitute for that contract.
- Next step: define the gray-work queue and write-barrier ABI, emit stack maps
  for live task roots, then add concurrent collector and race/sanitizer coverage.

### LSP-001 language-server MVP limits (2026-07-29)

- Area: `crates/aura-lsp`, `auralsp`
- Symptom: lifecycle, diagnostics, formatting, navigation, completion, and
  code actions work. References and rename use durable server-lifetime
  binding IDs across span-shifting edits, stdio requests run through a
  cooperative cancellation worker, and diagnostics expose precise structured
  suggestions. Semantic overload candidate resolution remains incomplete.
- Why deferred: the analysis API still exposes name-oriented navigation and
  completion facts rather than resolved overload sets.
- Next step: expose resolved overload candidates through the shared analysis
  boundary and render each candidate without label-based loss.

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
- Symptom: file operations and some native adapters do not suspend through the
  scheduler; richer iterator/protocol shapes, nested aggregate layouts, and
  async cancellation boundaries remain bounded.
- Why deferred: suspension, backpressure, cancellation, and ownership must be
  specified together for each operation family.
- Next step: define one async I/O adapter contract, then migrate file and
  network operations with pending/failure/cancellation tests.

### LAMBDA-001 richer captures and scheduler policy (MVP)

- Area: lambdas and spawned closures
- Symptom: primitive, String, class, Array, interface, and common nested
  aggregate captures have explicit ownership, but opaque aggregate elements and
  scheduler policy remain bounded/rejected.
- Next step: specify clone/mark/drop hooks for the remaining aggregate types and
  document the scheduler guarantees without changing the shared-cell contract.

### SAN-001 host-dependent sanitizer coverage (2026-07-28)

- Area: native/sanitizer fixtures and TCP integration
- Symptom: some TCP fixtures require ephemeral bind/network capabilities that are
  unavailable in the current sandbox; cross-host sanitizer evidence is absent.
- Next step: run the full matrix on supported clean hosts and retain the host,
  target, and sanitizer configuration with the release evidence.

### RELEASE-001 release and registry publication (2026-07-31)

- Area: packaging, registry, signing, and cross-target release
- Symptom: publish/authentication, alternate dependency sources, production
  signing, upload, and clean-host installer rehearsal are not complete.
- Why deferred: they require an external registry contract, credentials, release
  assets, and supported target hosts. The bounded local self-update and rollback
  contract is covered by U7/U8 evidence.
- Next step: define the registry/signing protocol, then run the frozen-release
  installer and checksum matrix on every supported host.

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

For exact evidence, use the relevant commit history, RFC, or test fixture rather
than restoring a per-change progress dump here.
