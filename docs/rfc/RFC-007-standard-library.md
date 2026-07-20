# RFC-007: Standard Library

| Field        | Value                              |
| ------------ | ---------------------------------- |
| **RFC**      | 007                                |
| **Title**    | Standard Library                   |
| **Status**   | Accepted                           |
| **Layer**    | Runtime                            |
| **Authors**  |                                    |
| **Created**  | 2026-07-15                         |
| **Updated**  | 2026-07-16                         |
| **Estimate** | 40–80 pages                        |
| **Depends**  | RFC-001, RFC-002, RFC-003, RFC-006 |
| **Blocks**   | RFC-011                            |

---

## 1. Abstract

This RFC outlines the **Aura standard library** for servers and CLIs: prelude, collections, I/O, networking primitives, JSON, logging, synchronization, crypto baseline, testing support types, and FFI helpers. It is **core-only**—no HTTP application framework, ORM, or DI container.

Implementation is primarily **Aura**, with thin runtime/FFI bridges where required.

**Toolchain today (2026-07-20):** repo packages `std/io` (console + file: `print`/`println`/`eprint`/`eprintln`, `readFile`/`writeFile`/`appendFile`/`fileExists`/`fileSize`), `std/assert` (`assert`), and `std/collections` (Map/Set/HashMap/Iterable + Int HOF). Package builds auto-prelude `std.io` and resolve `std.*` path deps. File I/O throws `String` on error (Result wrappers deferred). No net/json/async I/O yet.

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
| `std.sync`             | Mutex, RwLock, Channel, Atomic, Once                                                                           |
| `std.task`             | spawn, scope, sleep (thin over runtime)                                                                        |
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

### 6.4 Concurrency surface

```aura
val ch = Channel<Int>()
spawn { ch.send(1) }
val v = ch.receive()
Mutex.withLock(mu) { /* ... */ }
```

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

## 8. Rationale & trade-offs

Go-like pragmatic breadth without framework lock-in. Async-first net matches runtime. Keeping HTTP frameworks out preserves modularity. Cost: users assemble stacks from packages early—desired for ecosystem health.

## 9. Unresolved / future work

- Full API reference site
- Capability-based FS/net permissions (sandbox)
- SIMD / performance utilities

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

| Date       | Author | Change                                                                                |
| ---------- | ------ | ------------------------------------------------------------------------------------- |
| 2026-07-16 |        | Lock `List<T>` naming; Status → **Accepted**                                          |
| 2026-07-16 |        | Status → **In Review** — Review: package map locked; most packages still sketch-level |
| 2026-07-16 |        | Note shipped std.io / std.assert + Array MVP                                          |
| 2026-07-15 |        | Initial skeleton                                                                      |
| 2026-07-15 |        | Solid draft: package map, core-only scope                                             |
| 2026-07-15 |        | Defer std.http; lock small prelude, no password hash                                  |
