# Technical Debt

Standing log of temporary workarounds, incomplete behavior, and deferred follow-ups.

Keep this file focused on active debt. When an item is resolved, remove it or
replace it with the smaller residual scope. Record implementation history in
commits, RFCs, or release notes instead of appending progress for every change.

## Open

### API-001 platform and protocol placeholders (2026-07-31)

- Area: `std.crypto` TLS, `std.reflect`, `std.tls`, `std.udp`, `std.websocket`
- Symptom: public package contracts exist, but TLS, reflection,
  datagram and framing operations still fail
  explicitly or remain metadata-only.
- Why deferred: each surface needs a capability policy, ownership contract,
  parser hardening, and platform-backed tests.
- Next step: implement the TLS/provider and UDP foundations first, then
  add bounded streaming protocols without changing locked signatures.

### API-002 data and contract placeholders (2026-07-31)

- Area: `std.json.Value`, `std.collections.List<T>`, shared errors
- Symptom: generic JSON mapping, List iterator semantics, package-specific
  errors, and reflection metadata are incomplete. Bounded JSON traversal,
  duplicate-key rejection, and the std.test assertion/smoke helpers are
  implemented.
- Why deferred: the ownership, duplicate-key, depth/size, and metadata rules
  need one coherent RFC/runtime pass.
- Next step: define generic mapping ownership, then implement List iterators
  before expanding reflection and package-specific error APIs.

### API-003 compiler/runtime/tooling boundary inventory (2026-07-31)

- Area: RFC-001/002/004/005/006/008/010/012/013/014 surfaces
- Symptom: concurrent tracing write barriers and precise stack maps,
  cross-target sysroot delivery, self-update, unhygienic macro spans, and full
  LSP protocol behavior are not complete.
- Why deferred: these are separate distribution, GC, macro, and tooling
  contracts rather than prerequisites for the current alpha compiler.
- Next step: specify each boundary independently and track acceptance on the
  relevant host/tooling matrix.

### NET-001 bounded synchronous networking (2026-07-31)

- Area: `std.net`, `std.http`, POSIX TCP runtime
- Symptom: endpoint parsing and DNS resolution are synchronous and string-based;
  HTTP remains loopback-oriented with bounded framing, no pooling, timeout
  result, TLS, HTTP/2, or HTTP/3 support.
- Why deferred: the alpha transport needs a small usable surface first; typed
  endpoints, cancellation, resolver caching, and richer errors need a broader
  async networking design.
- Next step: add a validated endpoint/error model and move blocking resolution
  behind the async transport boundary.

### LSP-001 language-server MVP limits (2026-07-29)

- Area: `crates/aura-lsp`, `auralsp`
- Symptom: lifecycle, diagnostics, formatting, navigation, completion, and
  code actions work, but stable binding IDs, overload-aware results, precise
  suggestions, and preemptive cancellation are missing.
- Why deferred: the analysis API and serial stdio loop do not yet provide the
  required identity and scheduling contracts.
- Next step: share the package cache with all semantic queries, add binding IDs
  and structured suggestions, then move long queries to a cancellable scheduler.

### ANALYSIS-001 analysis cache eviction (2026-07-29)

- Area: `aura-analysis` snapshot query cache
- Symptom: every distinct document snapshot remains cached for the lifetime of
  the host.
- Next step: add bounded LRU/size-based eviction and hit/eviction metrics before
  enabling long-lived workspaces by default.

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
- Symptom: publish/authentication, alternate dependency sources, signing,
  upload, self-update, and clean-host installer rehearsal are not complete.
- Why deferred: they require an external registry contract, credentials, release
  assets, and supported target hosts.
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

For exact evidence, use the relevant commit history, RFC, or test fixture rather
than restoring a per-change progress dump here.
