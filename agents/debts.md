# Technical Debt

Standing log of temporary workarounds, incomplete behavior, and deferred follow-ups.

Keep this file focused on active debt. When an item is resolved, remove it or
replace it with the smaller residual scope. Record implementation history in
commits, RFCs, or release notes instead of appending progress for every change.

## Open

None. Ownership erasure, async protocol boundaries, sanitizer fixtures, and
platform entry points are covered by the generated ABI checks, native fixtures,
and CI matrices described below.

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
  release/tooling matrix; collector and overload-aware LSP contracts are now
  covered by their typed runtime and shared-analysis boundaries.
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
  coverage; ownership is now explicit for mutable closure paths.
- 2026-08-09: opaque aggregate captures and erased async results use the typed
  `AuraTypeErasedOps` clone/drop/mark contract in generated C, including
  spawned-closure escape, task-frame destruction, and GC marking paths.
- 2026-08-09: async file/network adapters, cancellation boundaries, and native
  worker paths are covered by target-neutral MIR emission and executable corpus
  checks; sanitizer coverage is wired for Linux amd64/arm64 and macOS arm64.
- 2026-08-09: the runtime platform contract has POSIX and Windows entry points
  for synchronization, wakeups, polling, file/socket I/O, monotonic time,
  signals, and entropy; `windows-platform` compiles and runs the native
  fixture in CI alongside the target-policy checks.
- 2026-08-09: MIR state-machine metadata now computes per-suspension liveness
  and emits only live locals plus the awaited task/result transfer slots into
  typed frame maps.

For exact evidence, use the relevant commit history, RFC, or test fixture rather
than restoring a per-change progress dump here.
