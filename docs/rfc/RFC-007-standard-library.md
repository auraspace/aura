# RFC-007: Standard Library

| Field        | Value                              |
| ------------ | ---------------------------------- |
| **RFC**      | 007                                |
| **Title**    | Standard Library                   |
| **Status**   | Accepted                           |
| **Layer**    | Runtime                            |
| **Authors**  |                                    |
| **Created**  | 2026-07-15                         |
| **Updated**  | 2026-07-31                         |
| **Estimate** | 40–80 pages                        |
| **Depends**  | RFC-001, RFC-002, RFC-003, RFC-006 |
| **Blocks**   | RFC-011                            |

---

## 1. Abstract

This RFC outlines the **Aura standard library** for servers and CLIs: prelude, collections, I/O, networking primitives, JSON, logging, synchronization, crypto baseline, testing support types, and FFI helpers. It is **core-only**—no HTTP application framework, ORM, or DI container.

Implementation is primarily **Aura**, with thin runtime/FFI bridges where required.

**Toolchain today (2026-08-01):** the repository ships the package set listed in
the [standard-library guide](../guide/standard-library.md): `std.io`,
`std.assert`, `std.collections`, `std.error`, `std.bytes`, `std.encoding`,
`std.json`, `std.mime`, `std.fs`, `std.os`, `std.net`, `std.dns`, `std.url`,
`std.http`, `std.stream`, `std.time`, `std.task`, `std.sync`, `std.signal`,
`std.log`, `std.metrics`, `std.test`, `std.crypto`, `std.reflect`, `std.tls`,
`std.udp`, `std.websocket`, `std.compress`, and `std.multipart`. The standard
packages expose bounded usable behavior; their portable fallback bodies are
replaced by compiler/runtime intrinsics on the C backend. The bounded APIs include typed
shared outcomes, loopback TCP/HTTP, monotonic timers, cooperative task
cancellation, nonblocking synchronization, encoding and JSON validation, and
structured logging/metrics. Some operations remain intentionally bounded or
runtime-backed: strict file APIs may throw `String`, `std.net` uses endpoint
strings with loopback as the port-only default,
JSON exposes bounded traversal/mapping, including recursive generic class decoding
with primitive leaves, nested class/struct fields, unit-enum fields, and primitive/class/unit-enum arrays, and Unix sockets plus framework-level
HTTP routing remain outside this alpha core.

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

### 6.1 Package map (design and shipped surface)

| Package           | Contents                                                                                                           |
| ----------------- | ------------------------------------------------------------------------------------------------------------------ |
| `std.io`          | Console, files, stdin, argv, owned file handles, and async descriptor I/O                                          |
| `std.assert`      | Runtime assertion primitive                                                                                        |
| `std.test`        | Deterministic Bool/Int/String assertion helpers                                                                    |
| `std.collections` | `Map`, `Set`, generic `HashMap`/`HashSet`, snapshot/live iterators, `Iterable`, and HOFs                           |
| `std.error`       | Shared `ErrorKind`, owned `Error`, and generic `Outcome<T,E>`                                                      |
| `std.bytes`       | Validated `Byte`, binary buffers, slicing/concatenation, and network-order integer helpers                         |
| `std.encoding`    | UTF-8, base64, hex, and RFC 3986 percent encoding                                                                  |
| `std.json`        | Bounded validation, escaping, parsing, root classification, policy, and typed mapping surface                      |
| `std.mime`        | Media-type validation and filename sanitization                                                                    |
| `std.fs`          | Portable paths and bounded filesystem metadata snapshots                                                           |
| `std.os`          | Environment, cwd, pid, and platform helpers                                                                        |
| `std.net`         | Endpoint-aware nonblocking TCP with exact binary I/O, deadlines, and cancellation cleanup                          |
| `std.dns`         | Bounded numeric host resolution                                                                                    |
| `std.url`         | Origin-form and absolute URI component validation                                                                  |
| `std.http`        | Bounded HTTP/1.1 server and loopback client request/response API; routing frameworks, HTTP/2+, TLS remain separate |
| `std.stream`      | Async reader/writer adapters over owned network streams                                                            |
| `std.time`        | Monotonic durations, deadlines, and async sleep                                                                    |
| `std.task`        | Task join, cooperative cancellation, delayed cancellation, and parent/child cancellation linking                   |
| `std.sync`        | Sequentially consistent atomics, nonblocking mutex/RW locks, and one-shot gates                                    |
| `std.signal`      | SIGINT/SIGTERM graceful-shutdown state                                                                             |
| `std.log`         | Level-filtered and structured text logging                                                                         |
| `std.metrics`     | Sequentially consistent counters and Prometheus samples                                                            |
| `std.crypto`      | Bounded runtime-backed MD5/SHA-256, HMAC, PBKDF2, secure random, and TLS foundations                               |
| `std.reflect`     | Bounded compiler-backed opt-in reflection metadata (RFC-009)                                                       |
| `std.tls`         | Bounded OpenSSL-backed certificate/config and String/binary async connection surface                               |
| `std.udp`         | Bounded POSIX endpoint, datagram, and async socket surface                                                         |
| `std.websocket`   | Bounded POSIX messages, ping/pong, close, and async connection surface                                             |
| `std.compress`    | Bounded gzip/deflate codec options and text-safe transforms                                                        |
| `std.multipart`   | Bounded parts, parser, and encoder surface                                                                         |

### 6.2 Error conventions

- Expected failures: `Result<T, E>` with typed errors (`IoError`, `ParseError`).
- Abnormal: throw hierarchy under `Error`.
- I/O: prefer `Result` for recoverable IO.

#### 6.2a Shared async and platform error contract

All first-party platform and protocol APIs use `Result<T, E>` for a failure
the caller can handle. A `throw` is reserved for violated Aura invariants,
programmer misuse of an unsafe API, and a runtime fault that cannot be
represented by the documented result type. `Bool`, empty strings, null, and a
process-global errno are not failure channels in new public APIs.

Every public error enum has a stable `kind` selected from this common set and
may carry package-specific, non-secret detail:

| Kind               | Meaning                                                                            | Required users     |
| ------------------ | ---------------------------------------------------------------------------------- | ------------------ |
| `InvalidInput`     | The caller supplied malformed or out-of-range data.                                | os, net, dns, http |
| `Unsupported`      | The target or capability does not implement the operation.                         | os, net, dns, http |
| `NotFound`         | A named OS object, host, or route was absent.                                      | os, dns, http      |
| `PermissionDenied` | Platform policy denied the operation.                                              | os, net            |
| `WouldBlock`       | A nonblocking operation needs readiness; async wrappers do not expose it.          | net, http          |
| `TimedOut`         | The documented monotonic deadline elapsed.                                         | net, dns, http     |
| `Cancelled`        | Structured cancellation or shutdown ended the operation.                           | os, net, dns, http |
| `Disconnected`     | A peer closed or reset transport state.                                            | net, http          |
| `LimitExceeded`    | A documented size, count, depth, or backpressure bound was exceeded.               | net, dns, http     |
| `Protocol`         | A syntactically valid transport exchange violated its protocol.                    | dns, http          |
| `System`           | An otherwise unmapped OS error; includes a stable operation name and numeric code. | os, net, dns, http |

`std.os.OsError`, `std.net.NetError`, `std.dns.DnsError`, and
`std.http.HttpError` are package-owned error types with a `kind` in that set; they
do not expose native pointers, native errno storage, DNS resolver text, TLS
keys, request bodies, headers, or authorization values. Adapters map an inner
error to their own error type while preserving the common kind and adding only
redacted context. The error result is terminal: it releases all owned native
resources before it becomes observable, and no borrowed view may cross a
`Result`, `Task`, channel, or spawn boundary.

Synchronous `try*` APIs added before this contract may retain their legacy
boolean/null form until their typed replacement ships, but must be documented
as compatibility shims and may not be copied into new APIs. Existing runtime
status enums remain private adapter input, not the Aura public ABI.

The alpha `std.net` surface now provides additive `listenResult`,
`connectResult`, `closeListenerResult`, and `closeStreamResult` wrappers using
`std.error.Outcome<..., NetError>`. The legacy handle/Bool forms remain only as
documented compatibility shims. `std.http.getResponseResult` and
`postResponseResult` preserve both protocol-framing and transport failures as
`HttpError`; they do not let a failed underlying request escape as an
uncategorized string exception.

The C backend also supports a bounded async exception bridge: a single
`await` inside a `try` may catch a child task's owned `String`, `Int`, or
`Bool` failure, identified by the task-frame error type tag. The catch body
runs after the child frame is released. The same bounded shape runs an async
`finally` body before propagating a child failure. Class catches, multi-await
regions, same-name catches whose types change, and nested failure cleanup
remain compiler follow-ons.

#### 6.2b Bounded HTTP/1.1 contract

`std.http` is a transport-facing standard-library package, not an application
framework. The initial server contract is HTTP/1.1 on supported POSIX targets
(Linux amd64 and macOS arm64), origin-form targets, `GET`, `HEAD`, `POST`,
`PUT`, `PATCH`, `DELETE`, and `OPTIONS`,
64 headers, an 8 KiB request line, 16 KiB aggregate headers, an 8 MiB body,
and a 16 MiB total request. It maps malformed requests to 400, unsupported
methods to 405, oversized input to 413, handler failure to one bounded 500,
and timeout to 408. Read, write, and idle timeouts default to 30 seconds and
are bounded to 30 seconds; async waits use monotonic deadlines rather than
wall clock time. TLS, HTTP/2, HTTP/3, WebSockets, compression, multipart,
and extended protocols remain capability-gated follow-on API areas; the
bounded HTTP client is part of the shipped alpha surface and is not silently
emulated by the HTTP/1.1 server.

The parser accepts bounded `Content-Length` and inbound `Transfer-Encoding:
chunked` request bodies. Chunked bytes are decoded into the same owned bounded
request snapshot as content-length bytes; chunk extensions and non-empty
trailer sections are rejected until the streaming body API defines their
ownership and visibility rules.

The Aura-level `serve` loop admits at most 64 active connections. It retains
only those task frames, reaps each terminal connection and handler frame, and
leaves excess work in the listener backlog until capacity is available. The
native parser and response builder enforce the stated request/response bounds;
partial writes suspend on readiness rather than buffering unbounded output.
Calling `std.net.closeListener` is the graceful-shutdown signal for an Aura
server: the accept task wakes, stops admitting work, and completes normally;
already accepted connections are allowed to finish or are cancelled during
executor shutdown. Closing a listener is idempotent.

The public handler shape is `Handler = (Request, Response) -> Task<Unit>`;
callers normally create that task with `spawn { ... }`. `Request` is an owned
snapshot: method, target, version, headers, and body remain valid across
`await` and are destroyed with the handler frame. `Response` is a task-owned
mutable builder: status, headers, body, and keep-alive policy may be changed
until the handler returns. It cannot be retained, sent, spawned, or written
after the connection reaches a terminal state. Native request, response, and
socket handles are implementation details and never form part of the public
Aura API. Cancellation, peer close, timeout, and shutdown end the handler
without serializing further writes; cleanup removes readiness registrations
and releases each native resource exactly once.

`std.time.sleep(milliseconds)` uses the runtime monotonic deadline queue,
preserves the task frame while pending, and maps negative or out-of-range
durations to task failure. `std.time.Duration` and `sleepFor` provide the
typed-duration wrapper without changing the monotonic semantics. `nowMillis`,
`Deadline`, `after`, and `sleepUntil` compose relative deadlines on the same
clock. The API does not observe wall-clock adjustments; full `Instant` values
and wall-clock formatting remain separately gated.

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
- Live iterators/views are opt-in and remain attached to the source. The C20j
  cursors traverse the current logical table order from their cursor position;
  structural mutation invalidates the entire cursor rather than exposing
  insertion-position ambiguity. Value replacement remains visible through a
  valid cursor.
- A mutation that invalidates a live cursor must be detected. The permitted
  outcomes are a typed invalidation error or a terminal iterator state;
  silently dereferencing a stale bucket, array slot, or entry is forbidden.
  Rehash, remove, clear, and capacity-changing insertion are invalidating
  mutations by default.
- `HashMap.entry(key)` returns a key-based handle for an existing entry. It
  retains the source map but no bucket or backing-array pointer; `set(value)`
  resolves the key at call time, returns `false` after removal, and is safe
  across rehash and GC. `key` is read-only and the handle cannot structurally
  mutate the map. `HashMap.liveEntry(key)` returns an invalidation-checked
  live entry view for an existing key. Value updates preserve validity;
  insert, remove, clear, and grow/rehash advance the map epoch so `isValid()`
  becomes false and `get()`/`set(value)` fail safely.
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

`std.task` exposes the task operations from RFC-003. The bounded runtime
surface includes `Select<T>`, `select<T>()`, and
`spawnBlocking<T>(() -> T)`, with explicit ownership and cancellation
behavior. `async fun f(...): T`
produces `Task<T>`; `spawn` returns `TaskHandle<T>`. `join` is repeatable and
returns a typed task outcome for primitive, nullable primitive, and aggregate
payloads. `cancel` is cooperative and has no preemptive
or OS-thread behavior. `isCancelled()` reports the current task's cancellation
request at cooperative checkpoints and returns `false` outside an async frame.

`Channel<T>(capacity: Int)` is bounded and requires `capacity > 0`. `send`
suspends when full, `receive` suspends when empty, and both use FIFO wait
queues. `close` is idempotent; queued values remain observable before a final
`Closed` outcome, while sends after close return `Closed`. Task/channel payloads
must be owned values or GC-managed references; scoped `ref` values cannot be
stored, sent, or retained by a task or channel.

### 6.5 I/O & net

- The shipped bounded async surface includes `std.io.readFd`/`writeFd`,
  `std.net.accept`/`readStream`/`writeStream`, exact binary
  `readExactly`/`writeAll`, and the `std.http` client/server adapters. These
  operations preserve owned handles and inputs across await.
- `std.net` supports endpoint strings with loopback defaults, monotonic
  operation deadlines, and cancellation cleanup that closes pending streams.
  `std.tls` upgrades TCP streams or creates verified OpenSSL connections and
  exposes the same String/binary async adapter model. UDP, Unix-domain sockets,
  and broad blocking convenience APIs are separate follow-ons.

### 6.6 JSON

```aura
val value = Json.parse(text)
val text = value?.serialize()
val payload = Json.encode<User>(user)
```

- The shipped bounded value model validates complete values, preserves their
  source text, exposes root classification, traversal, independent cloning,
  size/depth metadata, duplicate-key policy, typed failures, and primitive,
  string, recursively nested primitive-array, recursive generic class/struct, unit-enum, and primitive/class/unit-enum-array
  `decode<T>` mappings. `ParseOptions` enforces `maxBytes`, `maxDepth`, and
  `Reject`/`FirstWins`/`LastWins`; payload-carrying enums, arbitrary aggregate
  leaves, and derive-driven mappings remain outside this bounded shape.

`encode<T>` converts supported Aura values to compact JSON; `stringify<T>` is
an equivalent naming alias. The alpha encoder supports primitive values,
nested classes/structs, recursively nested arrays, string-key maps, and
payload-carrying enums. A field may use `@json(name = "...")` to choose its
wire key, while fields without the attribute retain their Aura name. Generic
type bodies remain erased until a concrete monomorph is emitted, and pretty
printing is intentionally outside this compact encoder contract. Nullable
primitive, reference, array, enum, and inline-struct fields map missing/`null`
JSON members to `T?`; present values are decoded and encoded recursively. The
native backend uses tagged scalar optionals, an array null sentinel, an enum
null tag, and an inline-struct presence bit, so `null` is never silently
coerced to an empty or zero aggregate.

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

| #   | Question                                | Options                                | Owner  | Status                                                                                                               |
| --- | --------------------------------------- | -------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------- |
| 1   | Bounded `std.http` server/client in v1? | defer or HTTP/1.1 core                 | Stdlib | **Resolved** — ship the bounded HTTP/1.1 server and loopback client; keep routing, TLS, and extended protocols gated |
| 2   | List naming: List vs Vec                | `List` + growable `Vec` or single type | Stdlib | **Resolved** — single growable `List<T>`; keep builtin `Array<T>`; no `Vec`                                          |
| 3   | Password hash in std?                   | no                                     | Stdlib | **Resolved** — ecosystem                                                                                             |
| 4   | Prelude size                            | small                                  | Stdlib | **Resolved** — minimal prelude                                                                                       |
| 5   | Default collection traversal            | snapshot or live                       | Stdlib | **Resolved** — snapshots by default; live views require a named contract                                             |

## 8. Rationale & trade-offs

Go-like pragmatic breadth without framework lock-in. Async-first net matches runtime. Keeping HTTP frameworks out preserves modularity. Cost: users assemble stacks from packages early—desired for ecosystem health.

## 9. Unresolved / future work

- Full API reference site
- Capability-based FS/net permissions (sandbox)
- SIMD / performance utilities
- Live collection iterators retain their source and use collection epochs:
  structural mutation invalidates the cursor, while value replacement remains
  visible. Snapshot iterators remain the mutation-independent alternative.

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

| Date       | Author | Change                                                                                                         |
| ---------- | ------ | -------------------------------------------------------------------------------------------------------------- |
| 2026-07-31 |        | Synchronize the shipped package map and bounded API status with the standard-library guide                     |
| 2026-07-29 |        | Admit bounded `std.http` HTTP/1.1 server contract; retain framework and extended protocols as gated follow-ons |
| 2026-07-16 |        | Lock `List<T>` naming; Status → **Accepted**                                                                   |
| 2026-07-16 |        | Status → **In Review** — Review: package map locked; most packages still sketch-level                          |
| 2026-07-16 |        | Note shipped std.io / std.assert + Array MVP                                                                   |
| 2026-07-22 |        | Define snapshot/live collection view, entry lifetime, invalidation, aliasing, and GC contract                  |
| 2026-07-15 |        | Initial skeleton                                                                                               |
| 2026-07-15 |        | Solid draft: package map, core-only scope                                                                      |
| 2026-07-15 |        | Defer std.http; lock small prelude, no password hash                                                           |
