# RFC-007: Standard Library

| Field        | Value                              |
| ------------ | ---------------------------------- |
| **RFC**      | 007                                |
| **Title**    | Standard Library                   |
| **Status**   | Accepted                           |
| **Layer**    | Runtime                            |
| **Authors**  |                                    |
| **Created**  | 2026-07-15                         |
| **Updated**  | 2026-07-22                         |
| **Estimate** | 40–80 pages                        |
| **Depends**  | RFC-001, RFC-002, RFC-003, RFC-006 |
| **Blocks**   | RFC-011                            |

---

## 1. Abstract

This RFC outlines the **Aura standard library** for servers and CLIs: prelude, collections, I/O, networking primitives, JSON, logging, synchronization, crypto baseline, testing support types, and FFI helpers. It is **core-only**—no HTTP application framework, ORM, or DI container.

Implementation is primarily **Aura**, with thin runtime/FFI bridges where required.

**Toolchain today (2026-07-22, S2/C21i):** repo packages `std/io` (console, file, process, stdin, exit, and non-throwing `readFileResult`/`writeFileResult`), `std/assert`, and `std/collections` (Map/Set, generic `HashMap<K,V>`/`HashSet<T>`, Iterable, generic map/filter/fold, join, hash-collection snapshots/HOFs, read-only snapshot iterators, and `HashMapEntry` snapshots with direct `for-in`). Package builds auto-prelude `std.io` and resolve `std.*` path deps. Strict file I/O still throws `String`; Result wrappers provide structured failures. Live iterators/views, mutation-through-entry, networking, JSON, logging, crypto, synchronization, and async I/O remain deferred.

## 2. Motivation

### 2.1 Problem statement

A language without a solid stdlib cannot bootstrap an ecosystem. Conversely, a stdlib that includes full web frameworks freezes opinions and bloats core.

### 2.2 Why now

Compiler MVP needs types to lower; users need I/O and collections for non-toy programs.

### 2.3 Success metrics

| Metric   | Target                                              |
| -------- | --------------------------------------------------- |
| Coverage | Build CLI + TCP service with std only               |
| Cohesion | Consistent error (`Result`/`Error`) and null styles |
| Size     | Tree-shaken / linked only used parts as feasible    |

## 3. Goals

- Batteries for collections, text, time, I/O, net primitives, JSON, log, sync, crypto basics.
- Async-friendly APIs matching RFC-003.
- Stable naming and package layout under `std.*`.
- Safe defaults; unsafe isolated.

## 4. Non-goals

- HTTP router/framework, GraphQL, gRPC codegen (ecosystem).
- Database drivers / ORM.
- GUI, mobile, browser DOM.
- Full TLS policy engine beyond practical client/server primitives (detail open).

## 5. Prior art & alternatives

| Library           | Notes            | Take                    |
| ----------------- | ---------------- | ----------------------- |
| Go stdlib         | Pragmatic net/io | Inspiration             |
| Java stdlib       | Large            | Avoid bloat             |
| Rust std + crates | Layered          | Split core vs ecosystem |
| Kotlin stdlib     | Collections DX   | Inspiration             |

## 6. Design

### 6.1 Package map (v1)

| Package                | Contents                                                                                                       |
| ---------------------- | -------------------------------------------------------------------------------------------------------------- |
| `std.prelude`          | Auto-imported essentials: `String` methods surface, `Result`, `Option`/`T?` helpers, `println` maybe re-export |
| `std.core`             | `Any`, `Error`, primitives wrappers if any                                                                     |
| `std.collections`      | `List`, `Map`, `Set`, `Vec`/`ArrayList`, iterators                                                             |
| `std.str` / string ops | Unicode-aware helpers                                                                                          |
| `std.io`               | Reader/Writer, files, stdin/stdout                                                                             |
| `std.fs`               | Path, metadata, directory                                                                                      |
| `std.net`              | TCP/UDP/Unix sockets; **not** full HTTP server framework                                                       |
| `std.http`             | **Deferred** as full client/server; ecosystem packages first. Net primitives stay in `std.net`                 |
| `std.json`             | Parse/serialize                                                                                                |
| `std.log`              | Structured levels                                                                                              |
| `std.sync`             | Mutex, RwLock, bounded `Channel<T>`, Atomic, Once                                                              |
| `std.task`             | `Task<T>`, `TaskHandle<T>`, spawn, join, cancel, scope, sleep (thin over runtime)                              |
| `std.time`             | Instant, Duration, clock                                                                                       |
| `std.crypto`           | Hash (SHA-2), HMAC, random; TLS via well-scoped API                                                            |
| `std.encoding`         | Base64, hex, UTF-8                                                                                             |
| `std.ffi`              | C string/buffer helpers                                                                                        |
| `std.reflect`          | Opt-in reflection (RFC-009)                                                                                    |
| `std.test`             | Assert helpers (RFC-011)                                                                                       |

### 6.2 Error conventions

- Expected failures: `Result<T, E>` with typed errors (`IoError`, `ParseError`).
- Abnormal: throw hierarchy under `Error`.
- I/O: prefer `Result` for recoverable IO.

### 6.3 Collections sketch

```aura
let xs = List.of(1, 2, 3)
let ys = xs.map((x) => x * 2)
var m = Map<String, Int>()
m.put("a", 1)
```

- Generics monomorphized per RFC-002.
- Iteration via `Iterable` interface.
- **Naming:** one growable `List<T>` in `std.collections`; language builtin `Array<T>` stays for dense buffers. No separate `Vec` type.

#### 6.3.1 Collection views and iterators

Collection traversal has two explicit families: **snapshots** and **live
views**. APIs must name which family they return; callers must not infer
mutation or lifetime behavior from a generic `Iterable<T>` alone.

- Snapshot APIs copy the logical key/value or element sequence at creation.
  They are stable while the source is mutated, including insertion, removal,
  clear, and hash-table rehash. Snapshot order is the source's documented
  logical order at creation time; it is not a promise about future source
  order.
- Live iterators/views are opt-in and remain attached to the source. They
  observe only mutations made after their documented position; an API must
  specify whether an insertion before, at, or after that position is visible.
  Until such rules are implemented and tested, collection APIs must expose
  snapshots only.
- A mutation that invalidates a live cursor must be detected. The permitted
  outcomes are a typed invalidation error or a terminal iterator state;
  silently dereferencing a stale bucket, array slot, or entry is forbidden.
  Rehash, remove, clear, and capacity-changing insertion are invalidating
  mutations by default.
- An entry view is a handle to one map entry, not an owned key/value pair.
  Its lifetime is bounded by the iterator/view borrow and it must not outlive
  the source collection or the validity epoch of the cursor. `key` is
  read-only. `value` may be read or assigned only through an explicitly
  mutable entry API, and removal invalidates that entry.
- Entry handles and live iterators must not permit aliases that can make the
  collection representation inconsistent. Structural mutation through a live
  entry is disallowed while that entry is borrowed; mutation APIs must either
  require exclusive access or return an invalidation result. Snapshot entries
  have no alias to the source and may be retained freely.
- Snapshots retain their element values according to normal Aura value/GC
  rules. A live view retains the source collection for the duration of its
  handle, and entry values remain GC-visible through the source/view roots.
  Dropping a view releases that retention; it must never free storage still
  owned by the source collection. Public APIs must not expose raw bucket or
  backing-array pointers.
- `for-in` over a collection uses the collection's default traversal mode.
  The default is snapshot traversal for mutation safety and deterministic
  lifetime. A future live traversal must use a distinct constructor or
  explicitly named API, with documented invalidation and ownership rules.

The minimum contract for each future collection view API documents: source
retention, element/entry lifetime, order, visibility of each mutation class,
invalidation behavior, aliasing restrictions, and GC ownership. No API is
considered stable until it has corpus coverage for mutation, rehash, clear,
entry escape, and collection/element reclamation.

### 6.4 Concurrency surface

```aura
val ch = Channel<Int>(capacity: 2)
val handle = spawn { ch.send(1) }
val v = ch.receive()              // FIFO; suspends if empty
val outcome = join(handle)        // Ok(Unit), Failed(error), or Cancelled
cancel(handle)                    // idempotent cooperative request
ch.close()                        // queued values drain; future sends close
Mutex.withLock(mu) { /* ... */ }
```

#### 6.4.1 C22 task/channel API contract

`std.task` exposes the task operations from RFC-003. `async fun f(...): T`
produces `Task<T>`; `spawn` returns `TaskHandle<T>`. `join` is repeatable and
returns a typed task outcome. `cancel` is cooperative and has no preemptive
or OS-thread behavior.

`Channel<T>(capacity: Int)` is bounded and requires `capacity > 0`. `send`
suspends when full, `receive` suspends when empty, and both use FIFO wait
queues. `close` is idempotent; queued values remain observable before a final
`Closed` outcome, while sends after close return `Closed`. Task/channel payloads
must be owned values or GC-managed references; scoped `ref` values cannot be
stored, sent, or retained by a task or channel.

### 6.5 I/O & net

- Async methods: `await file.readToEnd()`, `await tcp.accept()`.
- Blocking variants available for scripts; prefer async in servers.

### 6.6 JSON

```aura
val user = Json.parse<User>(text)?  // or Result
val text = Json.stringify(user)
```

- Uses attributes for field names (RFC-009) when available; MVP may require manual serializers.

### 6.7 Crypto baseline

- Secure random, SHA-256, HMAC-SHA256.
- Password hashing: recommend ecosystem or carefully chosen one (open).
- TLS: client/server streams—implementation may wrap OS/backend libraries.

### 6.8 Versioning

- Stdlib versioned with toolchain.
- `std` is not published as a normal registry package users replace casually; ship with compiler.

### 6.9 Examples

```aura
import std.io.println
import std.net.TcpListener
import std.task.spawn

fun main() {
  // conceptual
  println("ok")
}
```

### 6.10 Error model / edge cases

| Topic           | Policy                                      |
| --------------- | ------------------------------------------- |
| Partial Unicode | Document UTF-8 errors                       |
| Time zones      | Explicit API; avoid implicit local footguns |
| Cancelled IO    | Map to cancellation errors                  |

### 6.11 Compatibility & migration

- Deprecations via `@deprecated`.
- Major toolchain bumps may remove deprecated APIs.

## 7. Open questions

| #   | Question                      | Options                                | Owner  | Status                                                                      |
| --- | ----------------------------- | -------------------------------------- | ------ | --------------------------------------------------------------------------- |
| 1   | Thin `std.http` client in v1? | defer                                  | Stdlib | **Resolved** — defer                                                        |
| 2   | List naming: List vs Vec      | `List` + growable `Vec` or single type | Stdlib | **Resolved** — single growable `List<T>`; keep builtin `Array<T>`; no `Vec` |
| 3   | Password hash in std?         | no                                     | Stdlib | **Resolved** — ecosystem                                                    |
| 4   | Prelude size                  | small                                  | Stdlib | **Resolved** — minimal prelude                                              |
| 5   | Default collection traversal  | snapshot or live                       | Stdlib | **Resolved** — snapshots by default; live views require a named contract    |

## 8. Rationale & trade-offs

Go-like pragmatic breadth without framework lock-in. Async-first net matches runtime. Keeping HTTP frameworks out preserves modularity. Cost: users assemble stacks from packages early—desired for ecosystem health.

## 9. Unresolved / future work

- Full API reference site
- Capability-based FS/net permissions (sandbox)
- SIMD / performance utilities
- Live collection iterators and mutable entry views remain deferred: C20 ships
  deterministic read-only snapshots, while borrow/lifetime rules and
  compiler/runtime support for live aliases are not yet available.

## 10. Security & safety considerations

- Crypto APIs hard to misuse (no ECB footguns in public API).
- TLS defaults modern.
- Path traversal helpers safe-by-default.
- `std.ffi` clearly unsafe-adjacent.

## 11. Implementation plan (optional)

| Phase | Scope                            | Exit criteria         |
| ----- | -------------------------------- | --------------------- |
| S0    | Prelude + collections + io print | Hello                 |
| S1    | fs + sync + task                 | Concurrent CLI        |
| S2    | net + json + log                 | Tiny TCP JSON service |
| S3    | crypto baseline                  | Secure random + hash  |

## 12. References

- Go standard library overview
- RFC-001–003, RFC-006, RFC-009, RFC-011

---

## Changelog

| Date       | Author | Change                                                                                        |
| ---------- | ------ | --------------------------------------------------------------------------------------------- |
| 2026-07-16 |        | Lock `List<T>` naming; Status → **Accepted**                                                  |
| 2026-07-16 |        | Status → **In Review** — Review: package map locked; most packages still sketch-level         |
| 2026-07-16 |        | Note shipped std.io / std.assert + Array MVP                                                  |
| 2026-07-22 |        | Define snapshot/live collection view, entry lifetime, invalidation, aliasing, and GC contract |
| 2026-07-15 |        | Initial skeleton                                                                              |
| 2026-07-15 |        | Solid draft: package map, core-only scope                                                     |
| 2026-07-15 |        | Defer std.http; lock small prelude, no password hash                                          |
