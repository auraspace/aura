# Workstream 05: Async I/O

Owner: Runtime + Stdlib. Scope: 6 tasks.

This workstream provides the transport layer required by the HTTP server. It
must preserve async ownership and cancellation rules rather than introduce a
second blocking execution model.

## IO1. Async I/O API contract

**Objective:** Define the minimum filesystem and TCP API needed by alpha.

**Contract (bounded transport slice):** TCP handles are opaque, own one
descriptor, and transition from open to closed exactly once; `destroy` releases
the handle and repeated `close` is harmless. Operations are nonblocking and
return `OK`, `PENDING`, `TIMEOUT`, `EOF`, `ERROR`, `CLOSED`, or
`UNSUPPORTED`; positive timeout values bound one readiness wait in milliseconds.
Buffers are borrowed only for the duration of a call and byte counts are
returned explicitly. The current capability is localhost TCP on POSIX targets
(`__linux__`/`__APPLE__`); scheduler suspension, task cancellation wakeups, and
filesystem async operations are not part of this slice yet.
**Checklist:**

- [x] Define handles, readiness, pending, EOF, error, and close states.
- [x] Define nonblocking behavior and bounded readiness timeouts; scheduler
      interaction and cancellation remain open.
- [x] Define POSIX capability differences and unsupported-target behavior.
- [x] Define handle and borrowed-buffer ownership for the bounded runtime API;
      GC/task crossing rules remain open.

**Acceptance:** Runtime and library implementers share one API contract.

**Verification:** Compile API fixtures and validate cases on both hosts.

**Dependencies:** C1–C3, A1–A3.

## IO2. File operation integration

**Objective:** Make file operations suspend safely in async code.

**Checklist:**

- [ ] Implement async open, read, write, flush, and close semantics.
- [ ] Distinguish pending, ready, EOF, permission, and other errors.
- [ ] Preserve buffers and handles across suspension.
- [ ] Release resources on cancellation, failure, and GC.

**Acceptance:** File operations do not unexpectedly block or leak handles.

**Verification:** Run delayed, empty, large, error, cancellation, and forced-GC
cases.

**Dependencies:** IO1, A4–A8.

## IO3. TCP listener and stream integration

**Objective:** Provide reliable TCP transport for client and server workloads.

**Implementation status:** Partial. `runtime/aura_rt.c` now exposes an opaque,
status-based localhost TCP listener/stream slice on POSIX targets. Bind creates
a listening socket (including ephemeral port selection), accept/connect use
nonblocking descriptors with an explicit millisecond poll bound, and read/write
report byte counts plus `OK`, `PENDING`, `TIMEOUT`, `EOF`, `CLOSED`, or `ERROR`.
Close transitions are idempotent and destroy releases the owning handle. The
API is guarded by `AURA_TCP_POSIX` (`__unix__`/`__APPLE__`); unsupported targets
return `AURA_TCP_UNSUPPORTED`. Async scheduler integration, address parsing,
full partial-I/O readiness coverage, and cross-host evidence remain open.

**Checklist:**

- [x] Implement bind, listen, accept, connect, read, write, and close for the
      bounded POSIX slice.
- [ ] Represent partial reads/writes and readiness transitions.
- [x] Define ephemeral port selection, address reuse, and deterministic close/
      shutdown behavior for the bounded slice; general address parsing remains
      open.
- [ ] Make descriptor ownership explicit across tasks and cancellation.

**Acceptance:** Loopback client/server exchange data without blocking or losing
bytes.

**Verification:** Run loopback, partial-I/O, disconnect, timeout, cancellation,
and concurrent-connection tests on Linux and macOS.

**Dependencies:** IO1, A4–A8, S1–S5.

## IO4. Cancellation and resource cleanup

**Objective:** Make pending I/O safe under every task lifecycle outcome.

**Checklist:**

- [ ] Cancel pending file and TCP operations without double-close.
- [ ] Wake suspended tasks when operations fail or cancel.
- [ ] Reclaim buffers and descriptors after disconnect.
- [ ] Drain or cancel outstanding operations deterministically at shutdown.

**Acceptance:** No pending operation survives its owning task or server shutdown.

**Verification:** Run leak checks, sanitizer tests, cancellation races, and
forced-shutdown cases.

**Dependencies:** IO2, IO3, S5.

## IO5. Backpressure and channel bridge

**Objective:** Connect I/O completion to bounded channels safely.

**Checklist:**

- [ ] Suspend producers when buffers/channels are full.
- [ ] Suspend consumers when no data is available.
- [ ] Preserve ordering and payload ownership.
- [ ] Define close, cancellation, and peer-failure propagation.

**Acceptance:** Backpressure never loses, duplicates, or leaks a message.

**Verification:** Run producer/consumer, slow-peer, full/empty, close, and GC
under-load cases.

**Dependencies:** IO3, S1–S6.

## IO6. End-to-end async I/O example

**Objective:** Prove a user can use async I/O from a clean installation.

**Checklist:**

- [ ] Add a small client/server example using only documented APIs.
- [ ] Exercise bind/connect, exchange, error, and shutdown paths.
- [ ] Run the example from the CLI on Linux and macOS.
- [ ] Capture logs, exit status, and cleanup result in acceptance data.

**Acceptance:** The example is reproducible on every supported native host.

**Verification:** Execute it from a clean checkout and installed release.

**Dependencies:** IO2–IO5.
