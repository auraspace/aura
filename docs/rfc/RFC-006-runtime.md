# RFC-006: Runtime

| Field        | Value                     |
| ------------ | ------------------------- |
| **RFC**      | 006                       |
| **Title**    | Runtime                   |
| **Status**   | Accepted                  |
| **Layer**    | Runtime                   |
| **Authors**  |                           |
| **Created**  | 2026-07-15                |
| **Updated**  | 2026-08-02                |
| **Estimate** | 40–60 pages               |
| **Depends**  | RFC-000, RFC-001, RFC-003 |
| **Blocks**   | RFC-007, RFC-008, RFC-013 |

---

## 1. Abstract

This RFC specifies the **Aura runtime** linked into application binaries: tracing **GC**, **M:N task scheduler**, async I/O reactor, exception personality support, panic/abort paths, timers, and **C ABI FFI** bridges. The runtime is shipped as libraries produced by the Rust toolchain and linked by `aura build`, not installed as a separate end-user package.

**Toolchain today (2026-08-02):** embedded C runtime [`runtime/aura_rt.c`](../../runtime/aura_rt.c) linked by the C backend — console/file/process I/O, exception frames with typed causes, recursive aggregate ownership helpers, executor-coordinated stop-the-world mark/sweep GC, opt-in POSIX M:N workers, a versioned `AuraReactor` poll/timer boundary with POSIX default implementation, task-frame ABI, typed frame mark/drop hooks, bounded channels/select, structured scopes, blocking jobs, task-safe lazy cells, bounded FFI pin retention, and a versioned `AuraTypeErasedOps` clone/drop/mark boundary for open payload transfer. General handler CFG lowering, inferred spawn captures, aggregate ownership, procedural macro sandboxing, and HTTP/1.1 async streaming are implemented. Remaining compiler boundaries are descriptor-aware operations in genuinely open generic bodies, unsupported spawn-body shapes, non-POSIX reactor backends, and a concurrent tracing collector.

## 2. Motivation

### 2.1 Problem statement

A GC + task language needs a coherent runtime ABI for codegen (RFC-004) and stdlib (RFC-007). Single-binary deploy requires the runtime to be **linkable and stripable**, with documented knobs for servers.

### 2.2 Why now

Without a runtime contract, compiler lowering and stdlib cannot stabilize.

### 2.3 Success metrics

| Metric       | Target                                                                        |
| ------------ | ----------------------------------------------------------------------------- |
| Hello binary | Links runtime, allocates, prints, exits 0                                     |
| Tasks        | 100k+ sleeping tasks feasible on commodity hardware (order-of-magnitude goal) |
| GC           | Correct collection under concurrent allocation                                |
| FFI          | Call C `write` / libc subset safely with pins                                 |

## 3. Goals

- Provide GC, scheduler, I/O, exceptions as linked libraries.
- Stable **runtime ABI** for compiler-generated code (versioned).
- Observability hooks (metrics, tracing) without mandatory heavy agents.
- Minimal default footprint for CLIs; tunable for servers.

## 4. Non-goals

- Distributed runtime / cluster membership.
- JVM-compatible object model.
- Hot code reload / dynamic agent attach in v1.
- Guaranteed hard real-time latency.

## 5. Prior art & alternatives

| Runtime         | Notes                     | Take                                     |
| --------------- | ------------------------- | ---------------------------------------- |
| Go runtime      | GC + goroutines + netpoll | Primary inspiration                      |
| JVM             | Mature GC, heavy          | Complexity ceiling                       |
| .NET Native/AOT | Single-file trends        | Deploy inspiration                       |
| libuv / tokio   | Reactors                  | I/O patterns (Rust side may use similar) |
| WASM runtimes   | Sandbox                   | Not v1 host                              |

## 6. Design

### 6.1 Overview

```text
┌──────────────────────────────────────────┐
│              Application code            │
├──────────────────────────────────────────┤
│  Stdlib (Aura)                           │
├──────────────────────────────────────────┤
│  Runtime ABI (calls from codegen)        │
│  ┌────────┐ ┌──────────┐ ┌────────────┐  │
│  │  GC    │ │ Scheduler│ │  I/O poll  │  │
│  └────────┘ └──────────┘ └────────────┘  │
│  ┌────────┐ ┌──────────┐ ┌────────────┐  │
│  │Except. │ │  Timers  │ │ FFI / stubs│  │
│  └────────┘ └──────────┘ └────────────┘  │
├──────────────────────────────────────────┤
│  OS: threads, sockets, files, virtual mem│
└──────────────────────────────────────────┘
```

Runtime components may be implemented in **Rust** (and/or C for tiny stubs), exposed to LLVM codegen via known symbols (`aura_rt_*`).

### 6.2 GC

| Topic        | Direction                                                                   |
| ------------ | --------------------------------------------------------------------------- |
| Model        | Tracing GC, precise preferred                                               |
| Concurrency  | Current: executor-safe precise **STW mark-sweep**; concurrent tracing later |
| Roots        | Generated frame mark/drop hooks; typed Array roots; global roots registry    |
| Finalization | Weak; prefer explicit resource management                                   |
| Tuning       | Env/`AURA_GC_*` or runtime flags: heap size, pacing                         |

**Safepoints:** compiler inserts polls at back-edges and calls (policy with RFC-004).

Generated aggregate frames may register `aura_task_frame_set_gc_mark` and
`aura_task_frame_set_data_drop` callbacks. Array buffers containing tagged or
by-value aggregate elements use `aura_gc_add_array_root_typed`; its callback
scans the generated element layout rather than interpreting every element as a
raw pointer. Registration is owner-scoped and must be removed before stack
storage or frame data is destroyed.

The open-generic `AuraTypeErasedValue` result accessor is clone-out: a caller
receives an independently owned descriptor payload and may release the child
frame immediately. It must not expose the terminal frame's borrowed result
storage across a forwarding or suspension boundary.

### 6.3 Scheduler

- **M:N** tasks on an opt-in POSIX worker pool (shared ready queue).
- Worker pool and reactor coordination use mutex/condition safepoints.
- **Cooperative** yield at await points; optional preemption via safepoint time slices (open).
- `spawn`, `join`, cancellation propagation for scopes. Async CFG frames with
  synchronous `finally` blocks install a typed cancel hook; direct frame
  cancellation re-enters the CFG so nested cleanup runs before the runtime
  publishes `AURA_TASK_CANCELLED`.
- `spawn_blocking` for sync OS calls that would stall workers.

### 6.4 Async I/O

- `AuraReactor` is a versioned policy boundary (`AURA_REACTOR_ABI_VERSION`)
  owned by the executor. `aura_reactor_posix_new()` supplies the default
  implementation; `aura_task_executor_set_reactor()` can replace it before
  tasks or workers are active. Reactor polling never owns task frames and must
  return after waking or leaving registrations pending.
- The POSIX reactor uses `poll` plus a wake pipe and monotonic timers.
- Stdlib net/fs async APIs can park tasks rather than blocking workers; epoll,
  kqueue, and IOCP backends remain platform follow-ups.
- Timers: min-heap / time wheel wheel in runtime.

### 6.5 Exceptions & panics

- Exceptions are language-level; runtime provides raise/unwind landing pads with LLVM EH.
- Uncaught exception in `main` → print diagnostic, exit non-zero.
- Uncaught in task → surface on join; if detached, log + default handler.
- **Abort path** for fatal OOM / corrupted runtime.

### 6.6 Allocation API (compiler ABI)

Illustrative symbols (names TBD):

```text
aura_rt_alloc(size, type_id) -> ptr
aura_rt_alloc_array(elem_size, len, type_id) -> ptr
aura_rt_write_barrier(obj, field, new_value)  // if concurrent GC needs it
aura_rt_safepoint()
aura_rt_throw(exc_ptr) -> !
aura_rt_spawn(fn_ptr, env_ptr)
aura_rt_await(...)
```

Versioning: `AURA_RT_ABI_VERSION` checked at startup.

### 6.7 FFI

- **C ABI** extern declarations in Aura (`extern "C" fun ...`) lowered to LLVM.
- Libc linking as needed per target.
- Pin APIs for byte buffers across calls.
- Callbacks from C into Aura require runtime re-entry rules (documented).

### 6.8 Startup / shutdown

1. Runtime init (GC, scheduler, main thread).
2. Run static initializers (order rules: within package defined; across packages by dependency topo).
3. Call user `main`.
4. Drain tasks or cancel on exit policy.
5. Flush stdio; GC teardown optional.

### 6.9 Configuration

| Knob          | Example                 |
| ------------- | ----------------------- |
| Workers       | `AURA_GOMAXPROCS`-like  |
| Heap          | max heap, soft limits   |
| Race detector | on/off (dev builds)     |
| Logging       | runtime debug log level |

### 6.10 Examples

```text
# build links libaura_rt
aura build -o app
./app
AURA_WORKERS=4 AURA_GC_MAX_HEAP=512m ./app
```

### 6.11 Error model / edge cases

| Case           | Behavior                                                                      |
| -------------- | ----------------------------------------------------------------------------- |
| OOM            | **Abort** in MVP (optional `OutOfMemoryError` later)                          |
| Stack overflow | Guard pages / async state machines reduce risk; hard abort if native overflow |
| Dead scheduler | Fatal diagnostic                                                              |
| ABI mismatch   | Fail fast at startup                                                          |

### 6.12 Compatibility & migration

- Runtime ABI major bumps with toolchain major.
- Older binaries not guaranteed to load newer shared RT (static link default avoids this).

## 7. Open questions

| #   | Question                                   | Options        | Owner   | Status                                                                   |
| --- | ------------------------------------------ | -------------- | ------- | ------------------------------------------------------------------------ |
| 1   | Exact GC algorithm                         |                | Runtime | **Resolved** — free-all MVP → precise STW mark-sweep → concurrent later  |
| 2   | Static linking only vs optional dynamic RT | static default | Dist    | **Resolved** — static default                                            |
| 3   | OOM: abort vs throw                        | abort MVP      | Runtime | **Resolved**                                                             |
| 4   | Preemption                                 |                | Runtime | **Resolved** — cooperative await + safepoint polls (hybrid with RFC-003) |

## 8. Rationale & trade-offs

Linking the runtime into each binary matches single-file deploy and avoids “install runtime first.” Go-like scheduler matches language concurrency. Implementing RT in Rust aligns with the toolchain monorepo. Cost: larger binaries than freestanding C; mitigated by LTO/strip and feature flags (CLI vs server profiles).

## 9. Unresolved / future work

- Continuous profiling integration
- GC visualization tools
- Optional arena APIs for buffers
- Windows IOCP maturity checklist

## 10. Security & safety considerations

- Runtime is trusted computing base for all apps.
- FFI re-entry and callbacks are high-risk; document threat model.
- Allocator integrity checks in debug.
- Race detector memory overhead only when enabled.

## 11. Implementation plan (optional)

| Phase | Scope                  | Exit criteria    | Status                                                                                        |
| ----- | ---------------------- | ---------------- | --------------------------------------------------------------------------------------------- |
| R0    | Alloc + print + exit   | Hello            | **Done** (C1 + C3x path)                                                                      |
| R1    | GC MVP single-thread   | Class heap refs  | **Partial** — alloc + free-all (C3x/C3y); not full tracing                                    |
| R2    | Scheduler + channels   | Concurrent tests | **Partial** — C22 single-threaded FIFO executor/channels landed; parallel scheduling deferred |
| R3    | Async net + exceptions | Echo server      | Exceptions partial (C3c/C3g/C3s); await state machines and async net deferred                 |

### Task result and channel ownership invariant

`AuraTaskFrame` owns result and failure storage until the frame is released.
Each storage slot is rooted while live and is cleared before its optional
destructor runs. A null destructor denotes externally managed storage: frame
release still removes the GC root and clears the slot, but does not guess how
to free the payload. `AuraTaskOutcome` is borrowed; callers that need a result
after frame release must use `aura_task_outcome_clone` with matching clone and
destroy callbacks.

The FFI callback ABI follows the same rule for payloads. The ordinary callback
entry point receives a synchronous borrow. `aura_ffi_callback_invoke_owned`
creates an explicit owned snapshot using caller-supplied clone/destroy hooks,
returns it only on `OK`, caps it at 16 MiB, and releases it through the paired
idempotent destroy helper. No callback payload ownership is inferred from a
raw pointer or allocator convention.

`AuraTaskChannelValue` is transferred to the channel on `AURA_CHANNEL_OK` or
`AURA_CHANNEL_PENDING`. On `AURA_CHANNEL_CLOSED` the channel destroys it. On
`AURA_CHANNEL_ERROR` ownership stays with the caller, which must destroy the
value. This distinction is required for allocation-failure paths and prevents
double destruction in generated send code. Runtime-owned `Task<T>`,
`TaskHandle<T>`, and `Channel<T>` values use explicit scheduler references:
task payloads retain the executor-owned frame, while channel payloads retain
the channel. The wrapper transfers that reference on receive and releases it
exactly once on close, failed send, or lexical cleanup.

Async exception payloads use the same generated ownership ABI as task results:
scalar values, Arrays, classes, enums, value structs, interfaces, function
values, and tagged foreign handles are cloned into the frame error payload when
their typed hooks exist.
The error type name and source span travel with that clone, allowing nested
await propagation and typed catch extraction without borrowing child-frame
storage. Each async catch binding is assigned a distinct generated frame slot;
source-level names are aliases scoped to the handler, so sequential catches may
reuse a name across different payload types without ABI or C-layout collisions.
Owning `join` materializes the failure's type name and source span into the
public `TaskError` value; raw typed payload bytes remain frame-owned and are
never exposed as an untracked borrowed pointer.

Executor shutdown invalidates the scheduler before freeing it. Frames that
still have scheduler-owned payload references are detached from the executor
and remain alive until the final payload destructor releases them; payload
destruction after shutdown never dereferences the freed executor. A shutdown
also invalidates lexical task handles, so applications must not resume or
release a handle after its executor has been shut down.

The generated channel ABI currently covers `Int`, `Bool`, `String`, typed
foreign handles, heap classes, functions, interfaces, enums, value structs,
recursively typed Arrays, and scheduler-owned task/channel wrappers. Primitive
`Int` and `Bool` receives lower to
`Opt_Int` and `Opt_Bool`, so closed-channel state cannot be confused with a
zero value. `Unit` and nested nullable primitives remain compile-time
unsupported because their value representation is still ambiguous.

The public `runtime/aura_ffi.h` scheduler ABI exposes executor/frame creation,
submission, run/release/shutdown, and the same retained task payload and
channel-value transfer operations to foreign callers. A foreign
`TaskHandle<T>` crossing must retain/release through the executor payload API;
a foreign `Channel<T>` crossing must use the typed `AuraTaskChannelValue`
constructors/take functions so ownership transfers exactly once. Raw
frame/channel pointers remain opaque.

## 12. References

- Go runtime design talks/docs
- LLVM statepoints / EH docs
- RFC-003, RFC-004, RFC-007

---

## Changelog

| Date       | Author | Change                                                                                |
| ---------- | ------ | ------------------------------------------------------------------------------------- |
| 2026-07-16 |        | Lock GC/preemption; Status → **Accepted**                                             |
| 2026-07-16 |        | Status → **In Review** — Review: solid runtime design; GC algo + scheduler still open |
| 2026-07-16 |        | Note C runtime MVP status vs full RFC                                                 |
| 2026-07-15 |        | Initial skeleton                                                                      |
| 2026-07-15 |        | Solid draft: GC, M:N, FFI, ABI sketch                                                 |
| 2026-07-15 |        | Lock static link default, OOM abort MVP                                               |
| 2026-08-01 |        | Generated inferred-return wrappers use typed terminal fallbacks; HTTP smoke is warning-free |
| 2026-08-02 |        | Chunked request trailers are validated, retained in full snapshots, and consumed before streaming EOF |
