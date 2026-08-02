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
returned explicitly. The current capability is endpoint-aware TCP on POSIX targets
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
`runtime/runtime.c` expose an opaque POSIX `AuraFile` handle with explicit
`open`, one-syscall `read`/`write`, `flush`, idempotent `close`, and
`destroy`. Calls borrow buffers only for their duration and return stable
`OK`, `EOF`, `PENDING`, `PERMISSION`, `ERROR`, `CLOSED`, or `UNSUPPORTED`
statuses. Regular-file `O_NONBLOCK` is not a real readiness mechanism on the
supported hosts, so this slice does not claim scheduler suspension. Adapters
may register a borrowed frame waiting token and clear it before waking the
executor; `aura_task_frame_wait_file` now covers descriptor-backed file-like
handles. Regular files remain always-ready and do not gain a fake suspension
claim.

**Checklist:**

- [x] Implement bounded open, read, write, flush, and close semantics.
- [x] Distinguish pending, ready (`OK`), EOF, permission, and other errors.
- [x] Preserve descriptor-backed buffers and handles across bounded suspension;
      regular-file syscalls remain explicitly non-suspending.
- [x] Release bounded file/TCP resources exactly once on cancellation, failure,
      forced executor shutdown, and peer disconnect; GC-rooted async file
      buffers remain open.

**Acceptance:** File operations do not unexpectedly block or leak handles.

**Verification:** Run delayed, empty, large, error, cancellation, and forced-GC
cases.

**Dependencies:** IO1, A4–A8.

The bounded descriptor adapter now proves that a GC-rooted capture containing
an `AuraFile` handle and read buffer survives a pending wait, GC collection,
resumption, and terminal release. A true asynchronous regular-file operation
contract remains outside this slice because POSIX regular files do not provide
a portable readiness source; that broader scheduler integration remains open.

The compiler now lowers `std.io.readFd(fd, capacity)` into an explicit async
frame with a descriptor wait, resume state, nonblocking read, owned String
result, failure payload, forced-GC retention, and cancellation cleanup. It also
lowers `std.io.writeFd(fd, content)` into an owned-input frame that waits for
`POLLOUT`, resumes after short writes, returns the byte count, and releases its
buffer on completion, failure, cancellation, or executor shutdown. The
executor join path drives registered fd readiness so these generated operations
do not stop at `PENDING`. The compiler now also emits a bounded
`std.io.readFile(ForeignHandle<Int>, capacity)` frame that pins the opaque
handle, reads its `AuraFile` resource, owns the String buffer, and unpins during
frame teardown. Its `std.io.writeFile(ForeignHandle<Int>, content)` counterpart
pins the same resource, owns the input buffer, resumes short writes, and
returns the transferred byte count. `std.net.readStream` and
`std.net.writeStream` now provide the matching bounded compiler lowering for a
typed `AuraTcpStream`: task-scoped pinning, readiness waits, EOF/error mapping,
short-write continuation, and cancellation cleanup are emitted in the frame.
`std.net.connect(endpoint, timeout)` now lowers to an owned
`ForeignHandle<Int>` around a connected `AuraTcpStream`; the compiler ABI
fixture proves constructor wiring and destroy ownership, while native loopback
execution remains host-gated.
`std.io.openFile(path, mode)` now creates an owned typed handle and releases it
lexically after the enclosing Aura binding leaves scope. Bounded `spawn` now
retains captured typed file handles and drops the frame owner independently,
so `spawn { await writeFile(handle, ...) }` survives outer lexical cleanup;
native coverage now verifies a write/read round trip with forced GC, repeated
typed joins, and queued cancellation cleanup. General CFG async callers now
retain typed-handle parameters across multiple awaits in branch/loop frames;
the versioned `AuraReactor` boundary now owns the POSIX poll policy, with
non-POSIX backends remaining a separate target capability.

## IO3. TCP listener and stream integration

**Objective:** Provide reliable TCP transport for client and server workloads.

**Implementation status:** Complete for the bounded G3 compiler/runtime slice.
`runtime/runtime.c` now exposes an opaque,
status-based endpoint-aware TCP listener/stream slice on POSIX targets. Bind creates
a listening socket (including ephemeral port selection), accept/connect use
nonblocking descriptors with an explicit millisecond poll bound, and read/write
report byte counts plus `OK`, `PENDING`, `TIMEOUT`, `EOF`, `CLOSED`, or `ERROR`.
Close transitions are idempotent and destroy releases the owning handle. The
API is guarded by `AURA_TCP_POSIX` (`__unix__`/`__APPLE__`); unsupported targets
return `AURA_TCP_UNSUPPORTED`. The task ABI now provides bounded listener and
stream readiness adapters that borrow the owned nonblocking descriptor and
delegate to the executor's inline fd wait. Typed operation fixtures cover
partial I/O, EOF, peer failure, backpressure, cancellation, and exactly-once
cleanup. Non-POSIX backends remain a separate target capability.

**Checklist:**

- [x] Implement bind, listen, accept, connect, read, write, and close for the
      bounded POSIX slice.
- [x] Represent partial reads/writes and readiness transitions for the bounded
      POSIX stream API.
- [x] Define ephemeral port selection, address reuse, and deterministic close/
      shutdown behavior for the bounded slice; general address parsing remains
      open.
- [x] Make listener/stream descriptor ownership explicit through idempotent
      close/destroy; task/cancellation transfer is covered by typed operation
      handles.
- [x] Register bounded listener/stream readiness waits through the task frame;
      typed operation ownership and cancellation transfer are covered by the
      G3 operation-handle ABI.

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
exactly once. Typed `AuraFile`/`AuraTcpStream` operations now register with the
bounded POSIX readiness source and scheduler. A bounded POSIX `fd/events` wait is now
stored inline in the frame; `aura_task_executor_poll_waiting` polls all
registered descriptors in one bounded turn and wakes each ready frame, with
timeout, multi-wait, and cancellation coverage. The compiler-generated
`std.io.readFd` and `std.io.writeFd` slices now consume this wake path, and the
typed file/TCP operation handles use the same registration and cleanup path.

**Checklist:**

- [x] Cancel pending file and TCP operations without double-close for
      frame-registered adapter resources.
- [x] Wake suspended tasks when operations fail or cancel through the bounded
      adapter wake protocol; typed file/TCP registration and ownership are
      covered by the G3 operation-handle ABI.
- [x] Poll a bounded POSIX fd wait and wake its pending frame exactly once;
      timeout and cancellation clear the registration before resumption.
- [x] Reclaim buffers and descriptors after bounded native disconnect; peer
      close/EOF and typed failure paths connect to frame terminal cleanup.
- [x] Drain or cancel frame-registered outstanding operations deterministically
      at shutdown.

**Acceptance:** No frame-registered operation survives its owning task or
executor shutdown. The bounded G3 server-shutdown acceptance is complete;
regular-file readiness portability and non-POSIX backends remain outside it.

**Verification:** `runtime/tests/task_io_cleanup_sanitizer.c` covers real file
and TCP descriptors under cancellation, failure, forced executor shutdown, and
peer disconnect with ASAN/UBSAN; the typed operation fixture additionally
covers file EOF and TCP peer-write failure.

**Dependencies:** IO2, IO3, S5.

## IO5. Backpressure and channel bridge

**Objective:** Connect I/O completion to bounded channels safely.

**Implementation status:** Bounded executor/channel bridge and network response
backpressure are complete. The
capacity-limited channel wakes pending consumers when a producer sends and
wakes pending producers when a consumer removes a value. FIFO payload order,
owned-value destruction, cancellation, and close behavior are covered by
`runtime/tests/task_channel.c`. HTTP response writes and typed TCP writes use
the same readiness/backpressure path; richer streaming adapters remain outside
this bounded slice.

**Checklist:**

- [x] Suspend producers when bounded channels are full.
- [x] Suspend consumers when bounded channels have no data.
- [x] Preserve FIFO ordering and payload ownership.
- [x] Define bounded close and cancellation propagation, including peer-failure
      and network-operation propagation for the typed G3/G4 paths.

**Acceptance:** Backpressure never loses, duplicates, or leaks a message.

**Verification:** `runtime/tests/task_channel.c` runs producer/consumer,
full/empty, FIFO, close, cancellation, and cleanup cases under the runtime
fixture. Slow-peer and network completion are covered by the typed TCP
and HTTP readiness fixtures; richer streaming remains outside this slice.

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
- [x] Run the bounded primitive-FFI example from the CLI on Linux;
      macOS execution remains open.
- [x] Capture native logs, exit status, and cleanup result in the smoke script
      and `examples/http-health/README.md`; installed-release and macOS data
      remain open.

**Acceptance:** The example is reproducible on every supported native host.

**Verification:** Execute it from a clean checkout and installed release.

**Dependencies:** IO2–IO5.
