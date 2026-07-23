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

**Implementation status (bounded runtime slice):** `runtime/aura_ffi.h` and
`runtime/aura_rt.c` expose an opaque POSIX `AuraFile` handle with explicit
`open`, one-syscall `read`/`write`, `flush`, idempotent `close`, and
`destroy`. Calls borrow buffers only for their duration and return stable
`OK`, `EOF`, `PENDING`, `PERMISSION`, `ERROR`, `CLOSED`, or `UNSUPPORTED`
statuses. Regular-file `O_NONBLOCK` is not a real readiness mechanism on the
supported hosts, so this slice does not claim scheduler suspension. Adapters
may register a borrowed frame waiting token and clear it before waking the
executor.

**Checklist:**

- [x] Implement bounded open, read, write, flush, and close semantics.
- [x] Distinguish pending, ready (`OK`), EOF, permission, and other errors.
- [ ] Preserve buffers and handles across suspension.
- [x] Release bounded file/TCP resources exactly once on cancellation, failure,
      forced executor shutdown, and peer disconnect; GC-rooted async file
      buffers remain open.

**Acceptance:** File operations do not unexpectedly block or leak handles.

**Verification:** Run delayed, empty, large, error, cancellation, and forced-GC
cases.

**Dependencies:** IO1, A4–A8.

Buffer/handle preservation across file suspension and the full acceptance claim
require an async file operation contract and GC frame-root integration; they
remain intentionally open.

## IO3. TCP listener and stream integration

**Objective:** Provide reliable TCP transport for client and server workloads.

**Implementation status:** Partial. `runtime/aura_rt.c` now exposes an opaque,
status-based localhost TCP listener/stream slice on POSIX targets. Bind creates
a listening socket (including ephemeral port selection), accept/connect use
nonblocking descriptors with an explicit millisecond poll bound, and read/write
report byte counts plus `OK`, `PENDING`, `TIMEOUT`, `EOF`, `CLOSED`, or `ERROR`.
Close transitions are idempotent and destroy releases the owning handle. The
API is guarded by `AURA_TCP_POSIX` (`__unix__`/`__APPLE__`); unsupported targets
return `AURA_TCP_UNSUPPORTED`. The task ABI now provides bounded listener and
stream readiness adapters that borrow the owned nonblocking descriptor and
delegate to the executor's inline fd wait. Address parsing, full partial-I/O
readiness coverage, and cross-host evidence remain open.

**Checklist:**

- [x] Implement bind, listen, accept, connect, read, write, and close for the
      bounded POSIX slice.
- [x] Represent partial reads/writes and readiness transitions for the bounded
      POSIX stream API.
- [x] Define ephemeral port selection, address reuse, and deterministic close/
      shutdown behavior for the bounded slice; general address parsing remains
      open.
- [x] Make listener/stream descriptor ownership explicit through idempotent
      close/destroy; task/cancellation transfer remains open.
- [x] Register bounded listener/stream readiness waits through the task frame;
      full operation ownership and cancellation transfer remain open.

**Acceptance:** Loopback client/server exchange data without blocking or losing
bytes.

**Verification:** Run loopback, partial-I/O, disconnect, timeout, cancellation,
and concurrent-connection tests on Linux and macOS.

**Dependencies:** IO1, A4–A8, S1–S5.

## IO4. Cancellation and resource cleanup

**Objective:** Make pending I/O safe under every task lifecycle outcome.

**Bounded implementation status:** `AuraTaskFrame` now exposes a frame-scoped
cleanup hook for an adapter-owned pending file/socket operation. The hook is
cleared before its callback runs, runs before cancellation/failure becomes
observable, and also runs when executor shutdown destroys a live frame.
Cancellation already wakes a pending frame through the bounded executor. The
`aura_task_executor_wake_waiting` helper now clears an adapter-owned wait token
and queues the frame exactly once, so completion, failure, and cancellation
callbacks share the same wake protocol. The native disconnect fixture closes
the peer, observes `AURA_TCP_EOF`, publishes a terminal task failure, and
verifies registered file/socket cleanup releases descriptors and buffers
exactly once. This still does not register `AuraFile`/`AuraTcpStream` operations
with a readiness source or scheduler. A bounded POSIX `fd/events` wait is now
stored inline in the frame; `aura_task_executor_poll_waiting` polls all
registered descriptors in one bounded turn and wakes each ready frame, with
timeout, multi-wait, and cancellation coverage. Adapter-specific file/TCP
operation registration remains open.

**Checklist:**

- [x] Cancel pending file and TCP operations without double-close for
      frame-registered adapter resources.
- [x] Wake suspended tasks when operations fail or cancel through the bounded
      adapter wake protocol; generic POSIX fd readiness is covered, while
      file operation registration and full TCP operation ownership remain open.
- [x] Poll a bounded POSIX fd wait and wake its pending frame exactly once;
      timeout and cancellation clear the registration before resumption.
- [x] Reclaim buffers and descriptors after bounded native disconnect; the
      peer-close/EOF path is connected to frame terminal cleanup, while
      scheduler-wide failure completion remains open.
- [x] Drain or cancel frame-registered outstanding operations deterministically
      at shutdown.

**Acceptance:** No frame-registered operation survives its owning task or
executor shutdown. The full server-shutdown acceptance remains open until
native file/TCP adapters provide operation registration and wake sources.

**Verification:** `runtime/tests/task_io_cleanup_sanitizer.c` covers real file
and TCP descriptors under cancellation, failure, forced executor shutdown, and
peer disconnect with ASAN/UBSAN. Native disconnect races and scheduler-wide
wakeup remain deferred.

**Dependencies:** IO2, IO3, S5.

## IO5. Backpressure and channel bridge

**Objective:** Connect I/O completion to bounded channels safely.

**Implementation status:** Bounded executor/channel bridge complete. The
capacity-limited channel wakes pending consumers when a producer sends and
wakes pending producers when a consumer removes a value. FIFO payload order,
owned-value destruction, cancellation, and close behavior are covered by
`runtime/tests/task_channel.c`; network completion and scheduler-wide
backpressure remain open.

**Checklist:**

- [x] Suspend producers when bounded channels are full.
- [x] Suspend consumers when bounded channels have no data.
- [x] Preserve FIFO ordering and payload ownership.
- [x] Define bounded close and cancellation propagation; peer-failure and
      network-operation propagation remain open.

**Acceptance:** Backpressure never loses, duplicates, or leaks a message.

**Verification:** `runtime/tests/task_channel.c` runs producer/consumer,
full/empty, FIFO, close, cancellation, and cleanup cases under the runtime
fixture. Slow-peer and network completion remain deferred to IO3/IO6.

**Dependencies:** IO3, S1–S6.

## IO6. End-to-end async I/O example

**Objective:** Prove a user can use async I/O from a clean installation.
**Bounded native companion:** `examples/http-health/http_health.c` now uses
the task executor and bounded async HTTP bridge to bind localhost, exchange a
health response, reject malformed input with 400, and shut down
deterministically. `scripts/http-health-smoke.sh` records the listening address,
success/error output, and exit status under ASAN/UBSAN. The Aura CLI and
installed-release path remain open.

**Checklist:**

- [x] Add a bounded native client/server example using documented runtime APIs;
      the Aura-level example remains open.
- [x] Exercise bind/connect, exchange, malformed-request error, and shutdown
      paths in the native companion.
- [ ] Run the example from the CLI on Linux and macOS.
- [x] Capture native logs, exit status, and cleanup result in the smoke script
      and `examples/http-health/README.md`; installed-release and macOS data
      remain open.

**Acceptance:** The example is reproducible on every supported native host.

**Verification:** Execute it from a clean checkout and installed release.

**Dependencies:** IO2–IO5.
