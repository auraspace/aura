# RFC-003: Memory Model & Concurrency

| Field        | Value                      |
| ------------ | -------------------------- |
| **RFC**      | 003                        |
| **Title**    | Memory Model & Concurrency |
| **Status**   | Accepted                   |
| **Layer**    | Language                   |
| **Authors**  |                            |
| **Created**  | 2026-07-15                 |
| **Updated**  | 2026-07-16                 |
| **Estimate** | 40–80 pages                |
| **Depends**  | RFC-000, RFC-001, RFC-002  |
| **Blocks**   | RFC-004, RFC-006, RFC-007  |

---

## 1. Abstract

This RFC defines Aura’s **memory and concurrency model**: tracing GC, reference vs value semantics, lightweight **tasks**, bounded channels, `async`/`await`, shared-memory synchronization, happens-before rules, and the policy that **data races are bugs**—detected in development, not licensed as silent undefined behavior. C22 freezes a deterministic single-threaded task/event-loop MVP; the broader multi-worker model remains future work.

Runtime implementation details (scheduler, collector algorithm) are expanded in **RFC-006**; this document is the language-level contract.

**Toolchain today (2026-07-22, S2/C21i):** class instances are GC heap references, `struct` values remain by-value, and execution is single-threaded. The runtime has a stop-the-world mark/sweep collector with registered roots and deep scans, plus ownership handling for Array/String values and captured environments. C20c–e add MVP shared pointer boxes for mutable class, Array, and nested Fun captures; C20g adds read-only collection snapshots. C21b–e add sema-checked, non-owning scoped refs and borrow-safe Array field returns without a new runtime ownership ABI. Class payloads are rooted and nested Fun environments retain/release. Tasks, channels, `async`/`await`, a race detector, and concurrent GC are **not** implemented and remain deferred.

## 2. Motivation

### 2.1 Problem statement

Service authors need cheap concurrency and safe memory without Rust-style ownership. Classic JVM models provide GC and threads but historically weak tools for structured concurrency; Go provides tasks and channels with a simple model. Aura combines **GC + tasks** with a modern async surface and explicit race policy.

### 2.2 Why now

Compiler lowering, stdlib sync primitives, and diagnostics all depend on a single concurrency story.

### 2.3 Success metrics

| Metric           | Target                                                                   |
| ---------------- | ------------------------------------------------------------------------ |
| Data-race policy | Documented; detector in dev; not “optimizable UB”                        |
| Task scalability | Large numbers of blocked tasks with small stacks (stackless/async style) |
| Latency          | GC and scheduler behaviors documented with knobs (RFC-006)               |
| Expressiveness   | Concurrent servers without manual OS thread pools                        |

## 3. Goals

- Clear concurrency story for backends and CLIs.
- Safe-by-default memory via GC (no UAF in safe code).
- Async I/O first-class with structured patterns.
- Shared state possible with explicit locks/atomics/channels.
- Portable memory ordering story for atomics.

## 4. Non-goals

- Full CUDA/GPU model in v1.
- Distributed actor cluster protocol (library later).
- Rust-style ownership as the primary model; C21 may add only a scoped, non-owning `ref` capability.
- Hard real-time GC guarantees in v1.

## 5. Prior art & alternatives

| Model                      | Notes                | Decision                         |
| -------------------------- | -------------------- | -------------------------------- |
| Go GC + goroutines         | Simple, proven       | **Adopt spirit**                 |
| JVM threads + executors    | Mature               | Heavier default                  |
| Rust ownership             | Strong races freedom | Reject for user lang             |
| Actor-only                 | Isolation            | Optional library, not sole model |
| Single-threaded event loop | Simple               | Too limited alone                |

## 6. Design

### 6.1 Overview

```text
┌─────────────────────────────────────────────┐
│  Process                                    │
│  ┌─────────────┐    ┌─────────────────────┐ │
│  │ GC Heap     │    │ Task scheduler (M:N)│ │
│  │  objects    │◄──►│  tasks / async      │ │
│  │  structs*   │    │  I/O reactor        │ │
│  └─────────────┘    └─────────────────────┘ │
│         ▲                    ▲              │
│         │    channels/locks  │              │
└─────────┴────────────────────┴──────────────┘
* structs may live in heap boxes or stack/registers when escaped analysis allows
```

- **Memory:** tracing GC for class instances and boxed values.
- **C22 concurrency:** many cooperative tasks multiplexed by one deterministic ready queue on one OS thread. A task yields only at an `await`/runtime suspension point; no OS thread is created by `spawn`.
- **Communication:** prefer channels & isolation; locks for shared mutability.

The C22 MVP excludes OS-thread scheduling, work stealing, a blocking-I/O
reactor, and concurrent GC. Those facilities may be added by a later RFC or
milestone without changing the source vocabulary below. Release packaging,
signing, notarization, and publication are also outside C22.

### 6.2 Memory management strategy

| Option                   | v1                            |
| ------------------------ | ----------------------------- |
| Tracing GC               | **Yes — default**             |
| RC/ARC primary           | No                            |
| Ownership/borrow primary | No — scoped `ref` is additive |
| Hybrid arenas            | Optional later for buffers    |
| Regions                  | Future                        |

**Decision:** Tracing GC. Phased algorithm (RFC-006): free-all MVP (shipped) → precise **stop-the-world mark-sweep** next → concurrent collector later.

Safe Aura guarantees:

- No use-after-free / double-free for GC-managed objects.
- Finalizers: discouraged; prefer explicit `Close`/`using` patterns (stdlib).

### 6.2.1 C21 ref MVP boundary

The selected C21 direction is a checked, non-owning borrow for short-lived
access to an existing owner. It does not change GC ownership: the owner keeps
the object or Array buffer alive, while sema rejects a `ref T` that would
outlive the owner's lexical scope. The first consumers are safe Array field
returns and read-only collection views.

The MVP has no mutable borrow, heap-stored reference, nullable/nested `ref`,
closure/task escape, pinning, or concurrent sharing. Codegen may represent a
valid borrow as a temporary pointer/view; no new runtime retain/release ABI is
required. Async/tasks is explicitly outside this track and deferred until the
borrow contract and single-threaded lifetime checks are stable.

### 6.3 Value semantics & references

| Kind              | Semantics                                |
| ----------------- | ---------------------------------------- |
| `class` instances | Reference identity; GC-managed           |
| primitives        | Value; copy                              |
| `struct`          | Value copy on assign/pass (unless boxed) |
| arrays / strings  | Reference; **`String` is immutable**     |

- **Interior mutability:** fields of classes mutable per `var`/`val`; no separate `Cell` required for GC objects.
- **Pinning:** needed at FFI boundaries for buffers (RFC-006).

### 6.4 Threading model

- **Logical concurrency unit:** **task**, represented by a stackless frame and scheduled cooperatively.
- **MVP execution:** one runtime executor, one ready queue, FIFO enqueue order, and no user-visible parallelism.
- **OS threads:** unavailable to C22 `spawn`; worker pools, `spawnBlocking`, work stealing, and scheduler placement are deferred to RFC-006 follow-up work.

### 6.5 Async model

```aura
async fun loadUser(id: Id): User { ... }

fun main() {
  spawn { backgroundSync() }

  val user = await loadUser(42)
  // structured: prefer scopes
  taskScope {
    val a = async { fetchA() }
    val b = async { fetchB() }
    use(await a, await b)
  }
}
```

| Topic                  | Rule                                                                                                                     |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| Coroutines             | **Stackless** async state machines (C11-backend-friendly)                                                                |
| `await`                | Suspends the current task and returns control to the single-threaded executor; it never blocks an OS thread in the MVP   |
| Cancellation           | Cooperative; a cancelled task observes cancellation at suspension/check points and completes with a cancellation outcome |
| Structured concurrency | **Encouraged** via `taskScope`; global `spawn` is allowed but remains explicitly unstructured and lintable               |

### 6.6 Shared-state concurrency

**Primitives (stdlib):**

| API                      | Role                     |
| ------------------------ | ------------------------ |
| `Mutex<T>` / `RwLock<T>` | Critical sections        |
| `Channel<T>` / `Select`  | Message passing          |
| `Atomic*`                | Lock-free counters/flags |
| `Once` / `Lazy`          | Init                     |

**Default style:** share-memory-by-communicating when practical; locks when needed.

There is **no** borrow-based `Send`/`Sync` enforcement. Documentation and optional attributes may mark thread-hostile types. Race detector covers misuse.

### 6.7 Memory consistency model

- **Sequentially consistent** atomics as default API for simplicity.
- **Acquire/release** variants available for experts.
- **Happens-before** edges: unlock→lock same mutex; channel send→receive; task spawn→start; async resume edges; volatile/atomic ops per their orderings.
- **Data race definition:** concurrent conflicting accesses to the same non-atomic location where at least one is a write, without happens-before.

**Data race policy:**

| Approach                                                                     | Rejected / Accepted                |
| ---------------------------------------------------------------------------- | ---------------------------------- |
| Silent UB (C/C++)                                                            | **Rejected**                       |
| “Catch fire”                                                                 | **Rejected**                       |
| Language-level “race is a bug”; values may be torn/stale; runtime may detect | **Accepted**                       |
| Dev **race detector** (like Go)                                              | **Required for MVP tooling story** |
| Prod detector always-on                                                      | Optional flag / sampling           |

Aura does **not** promise that racy programs have sequential semantics; it promises races are **not a free optimization license** to delete safety checks elsewhere, and tools help find them.

### 6.8 FFI & foreign memory

- Foreign memory is **not** GC-managed unless copied/bridged.
- Buffers passed to C must remain valid for the call (pin / explicit lifetime scope).
- Allocator hooks for custom native buffers → RFC-006.
- `unsafe` required for raw pointer dereference.

### 6.9 Examples

```aura
fun worker(c: Channel<Int>) {
  for (n in c) {
    println(n)
  }
}

fun main() {
  val c = Channel<Int>()
  spawn { worker(c) }
  c.send(1)
  c.send(2)
  c.close()
}
```

```aura
async fun handle(conn: Conn) {
  val req = await conn.readRequest()
  val res = await route(req)
  await conn.writeResponse(res)
}
```

### 6.10 Error model / edge cases

| Topic                        | Policy                                                                     |
| ---------------------------- | -------------------------------------------------------------------------- |
| Deadlock                     | Not prevented statically; timeouts in stdlib; detector optional later      |
| Panic/exception across tasks | Isolated by default; join surfaces error; `spawn` needs supervision policy |
| Cancellation leaks           | Scopes + `finally` / `defer` (if introduced)                               |
| GC during FFI                | Documented; pin buffers                                                    |

### 6.11 Compatibility & migration

- Scheduler tuning flags may change performance but not language semantics without edition.
- Strengthening race detection must not break race-free programs.

## 7. Open questions

| #   | Question                              | Options                              | Owner   | Status                                                                      |
| --- | ------------------------------------- | ------------------------------------ | ------- | --------------------------------------------------------------------------- |
| 1   | GC algorithm                          | Immix / CMS / Go-like                | Runtime | **Resolved** — phased STW mark-sweep next; concurrent later (RFC-006)       |
| 2   | Structured concurrency mandatory?     | encourage                            | Lang    | **Resolved** — encourage, not require                                       |
| 3   | Preemptive vs cooperative task switch | cooperative await + safepoint hybrid | Runtime | **Resolved for C22** — cooperative single-threaded MVP; preemption deferred |
| 4   | String mutability                     | immutable                            | Lang    | **Resolved**                                                                |
| 5   | `spawn` supervision defaults          | log + join surfaces error            | Lang    | **Resolved** — log + `join` surfaces error; no auto-restart                 |

## 8. Rationale & trade-offs

Go-like tasks + GC maximize concurrency productivity for servers. Stackless async integrates cleanly with LLVM and avoids huge per-task stacks. Rejecting ownership keeps the type system focused on nullability and classes. Rejecting silent UB for races aligns with “safety as a product value” while remaining implementable without a borrow checker. Cost: GC pauses and the need for discipline (and a detector) around shared mutation.

## 9. Unresolved / future work

- Formal memory model appendix (axiomatic)
- Profiler/tracing integration
- Optional ownership annotations for buffers
- Actor library design

## 10. Security & safety considerations

- UAF/double-free in safe code: mitigated by GC.
- Races as security bugs (TOCTOU, torn reads of pointers—mitigated if references are atomic-sized and GC-safe; still logical bugs).
- FFI is the primary memory-safety escape hatch.
- Side channels (Spectre) out of scope unless noted.

## 11. Implementation plan (optional)

| Phase | Scope                         | Exit criteria                |
| ----- | ----------------------------- | ---------------------------- |
| M0    | Single-threaded + async I/O   | HTTP echo / CLI              |
| M1    | Multi-task scheduler          | Concurrent load test         |
| M2    | Race detector + atomics/locks | Detector finds planted races |

## 12. References

- Go memory model; Go race detector
- Java Memory Model (happens-before concepts)
- Kotlin coroutines / structured concurrency (Trio, Swift TaskGroup inspiration)
- RFC-000, RFC-001, RFC-006

---

## Changelog

| Date       | Author | Change                                                                                                                 |
| ---------- | ------ | ---------------------------------------------------------------------------------------------------------------------- |
| 2026-07-16 |        | Lock GC phased path + spawn supervision defaults                                                                       |
| 2026-07-16 |        | Status → **Accepted** — Review: GC + tasks language contract locked; algo/scheduler detail in 006                      |
| 2026-07-16 |        | Note GC MVP vs full concurrency model                                                                                  |
| 2026-07-15 |        | Initial skeleton                                                                                                       |
| 2026-07-15 |        | Solid draft: GC, M:N tasks, race policy, async                                                                         |
| 2026-07-15 |        | Lock string immutability, structured concurrency encourage                                                             |
| 2026-07-22 |        | C22a: freeze single-threaded cooperative task vocabulary and defer OS-thread, reactor, concurrent-GC, and release work |
