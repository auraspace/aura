---
title: Standard library
section: Toolchain
order: 55
summary: In-tree std packages, public API contracts, and prelude resolution.
---

# Standard library

Aura’s **core** stdlib is intentionally small ([RFC-007](/rfc/007), [RFC-000](/rfc/000) batteries-included-but-modular). In this repository, packages live under `std/`.

## Packages today

| Package           | Path              | Role                                                                                     |
| ----------------- | ----------------- | ---------------------------------------------------------------------------------------- |
| `std.io`          | `std/io`          | Console, file I/O, argv, stdin, exit                                                     |
| `std.assert`      | `std/assert`      | Assert helpers for tests                                                                 |
| `std.collections` | `std/collections` | Map/Set, generic hash collections, snapshot and live iterators, `Iterable`, HOFs, `join` |
| `std.error`       | `std/error`       | Shared error categories, owned errors, and generic outcomes                              |
| `std.bytes`       | `std/bytes`       | Owned byte strings and bounded mutable byte buffers                                      |
| `std.encoding`    | `std/encoding`    | UTF-8, hexadecimal, base64, and percent encoding                                         |
| `std.json`        | `std/json`        | Bounded JSON validation, parsing, escaping, and root classification                      |
| `std.mime`        | `std/mime`        | Media-type validation and upload filename sanitization                                   |
| `std.fs`          | `std/fs`          | Portable path and filesystem metadata helpers                                            |
| `std.os`          | `std/os`          | Environment, process, platform, and working-directory helpers                            |
| `std.net`         | `std/net`         | Nonblocking endpoint-aware TCP listeners, connections, streams, and typed failures       |
| `std.dns`         | `std/dns`         | Bounded numeric host resolution                                                          |
| `std.url`         | `std/url`         | Origin-form and absolute URI parsing plus component encoding                             |
| `std.http`        | `std/http`        | Bounded HTTP/1.1 client/server request and response values                               |
| `std.stream`      | `std/stream`      | Async reader/writer adapters over owned network streams                                  |
| `std.time`        | `std/time`        | Monotonic durations, deadlines, and async sleep                                          |
| `std.task`        | `std/task`        | Task join, cancellation, and cancellation linking                                        |
| `std.sync`        | `std/sync`        | Nonblocking atomics, mutexes, reader/writer locks, and one-shot gates                    |
| `std.signal`      | `std/signal`      | Graceful SIGINT/SIGTERM shutdown state                                                   |
| `std.log`         | `std/log`         | Bounded level-based and structured text logging                                          |
| `std.metrics`     | `std/metrics`     | Sequentially consistent counters and Prometheus samples                                  |
| `std.test`        | `std/test`        | Deterministic assertion helpers for native and corpus tests                              |

Builtins such as `Array<T>` and core scalars are part of the **language**, not a separate import. String methods (`indexOf`, `split`, `trim`, `toInt`, …) are language surface — see [Types](./types-and-nullability.md) and the [cheatsheet](./syntax-cheatsheet.md).

## `std.io`

Console, process, and file helpers (runtime `aura_*` intrinsics). Strict file APIs throw a `String` message on failure (missing path, I/O error, oversized file, embedded NUL). Soft `tryReadFile` returns `null` instead. Text is treated as a regular-file UTF-8 byte sequence (no embedded NUL); max size **256 MiB**.

### Console

| API                   | Role                               |
| --------------------- | ---------------------------------- |
| `print` / `println`   | stdout (no newline / with newline) |
| `eprint` / `eprintln` | stderr                             |

### Process (C12b–e)

| API                      | Role                                                                              |
| ------------------------ | --------------------------------------------------------------------------------- |
| `args(): Array<String>`  | Process argv; `[0]` = program name; user flags from index 1 (C12b)                |
| `readLine(): String?`    | One line without trailing `\n` / `\r\n`; `null` on EOF; empty line is `""` (C12d) |
| `readAllStdin(): String` | Remainder of stdin (throws on oversize / I/O / embedded NUL)                      |
| `exit(code: Int)`        | Terminate with status; flushes stdout/stderr first; does not return (C12e)        |

Pass user args after `--` with the CLI ([CLI](./cli.md)):

```bash
aura run my_pkg -- --flag value
cargo run -p aura-cli -- run corpus/std_io/args -- hello
printf 'line\n' | cargo run -p aura-cli -- run corpus/std_io/stdin
```

### Files (C11a / C12p)

| API                                                    | Role                                                          |
| ------------------------------------------------------ | ------------------------------------------------------------- |
| `readFile(path): String`                               | read entire regular file (throws on error)                    |
| `tryReadFile(path): String?`                           | soft read; `null` on missing/error (C12p)                     |
| `writeFile(path, content)`                             | create/truncate and write                                     |
| `tryWriteFile(path, content): Bool`                    | soft write; `false` on failure                                |
| `readFileResult(path): Result<String, String>`         | non-throwing read with error payload                          |
| `writeFileResult(path, content): Result<Bool, String>` | non-throwing write with error payload                         |
| `appendFile(path, content)`                            | append (create if needed)                                     |
| `fileExists(path): Bool`                               | regular file present                                          |
| `fileSize(path): Int`                                  | byte size (throws if missing)                                 |
| `openFile(path, mode): ForeignHandle<Int>`             | owned handle; mode 0 read, 1 truncate, 2 read/write, 3 append |
| `readFd(fd, capacity): String`                         | async bounded descriptor read                                 |
| `writeFd(fd, content): Int`                            | async descriptor write; returns bytes                         |

Typical use (explicit import or auto-prelude on package builds):

```aura
package main

import std.io as Io

fun main() {
  Io.println("Hello, Aura")
  val argv = Io.args()
  if (argv.len > 1) {
    Io.println(argv.get(1))
  }
  Io.writeFile("out.txt", "hi")
  val s = Io.tryReadFile("out.txt")
  if (s != null) {
    Io.println(s)
  }
}
```

Corpus:

```bash
aura run corpus/std_io/app
aura run corpus/std_io/prelude
aura run corpus/std_io/files
aura run corpus/std_io/try_read_file
aura run corpus/std_io/args -- hello
aura run corpus/std_io/stdin
aura run corpus/std_io/exit
# monorepo: cargo run -p aura-cli -- run corpus/std_io/files
```

Dogfood CLI that ties args + soft read + String tools: `examples/wc` ([README](https://github.com/auraspace/aura/blob/main/examples/wc/README.md)).

## `std.assert`

`std.assert.assert(condition)` is the runtime assertion primitive. Use it with
`aura test` and `@test` functions:

```bash
aura run corpus/std_assert/app
```

The typed `assertEqInt`, `assertEqString`, and `assertEqBool` helpers belong to
`std.test`; the language-level `assert_eq` helpers are separate builtins.

## `std.collections`

| Type / helper                                                            | Notes                                                                               |
| ------------------------------------------------------------------------ | ----------------------------------------------------------------------------------- |
| `Map<K, V>`                                                              | Linear map; `get` → `V?`; `put` / `remove` / `clear`                                |
| `Set<T>`                                                                 | Generic set (linear)                                                                |
| `map_string_int()` / `set()`                                             | Empty concrete compatibility factories                                              |
| `HashMap<K,V>`                                                           | Generic open addressing with `K: Hashable`; `containsValue` (C19a)                  |
| `hash_map()` / `hash_map_str()` / `hash_set()`                           | Empty generic collection factories                                                  |
| `HashSet<T>`                                                             | Generic open addressing backed by `HashMap<T,Bool>`; `containsAll(Array<T>)` (C19a) |
| `HashMapEntryHandle` / `HashMapLiveEntry`                                | Key-based mutation handle and epoch-checked live entry view                         |
| `get` / `getOr` / `contains` / `replace`                                 | Nullable lookup, default lookup, membership, and existing-value replacement         |
| `grow` / `capacity` / `len` / `isEmpty` / `clear`                        | Table sizing and collection state operations                                        |
| `Hashable`                                                               | `hash(): Int`; built-in for `Int` and `String` (C14)                                |
| `keyArray()` / `valueArray()`                                            | `HashMap` snapshots in logical table order (C18)                                    |
| `HashMapEntry<K,V>` / `entries()`                                        | Key/value snapshot pairs in logical table order (C19b)                              |
| `toArray()`                                                              | `HashSet` snapshots in logical table order (C18)                                    |
| `map_hash_map_values`                                                    | Generic `(K,V) -> R` map-entry HOF (C18)                                            |
| `filter_hash_set` / `map_hash_set`                                       | Generic set HOFs returning arrays (C18)                                             |
| `Iterable<E>`                                                            | `len` + `get` protocol for `for-in`, including entry snapshots (C19c)               |
| `keyIterator()` / `entryIterator()` / `iterator()`                       | Read-only deterministic snapshots for HashMap/HashSet (C20g)                        |
| `liveKeyIterator()` / `liveEntryIterator()` / `liveIterator()`           | Invalidation-checked live HashMap/HashSet cursors (C20j)                            |
| `HashMapKeyIterator` / `HashMapEntryIterator` / `HashSetIterator`        | Snapshot values exposing `len()` and `get(i)`                                       |
| `HashMapLiveKeyIterator` / `HashMapLiveIterator` / `HashSetLiveIterator` | Live cursors exposing `isValid()`, `hasNext()`, and `next()`                        |
| `map<T,R>` / `filter<T>` / `fold<T,A>`                                   | Generic array HOFs; verified for `Int` and `String` (C16)                           |
| `map_ints` / `filter_ints` / `fold_ints`                                 | Int compatibility wrappers                                                          |
| `map_strings` / `filter_strings` / `fold_strings`                        | String compatibility wrappers (C12o)                                                |
| `join(parts, sep)`                                                       | `Array<String>` → `String` with separator (C12j)                                    |

See [Arrays](./arrays.md) for HOF usage and capture limits.

```bash
aura run corpus/std_collections/app
aura run corpus/std_collections/hashmap
aura run corpus/std_collections/hashmap_str
aura run corpus/std_collections/hashmap_int
aura run corpus/std_collections/hashset_int
aura run corpus/std_collections/hof
aura run corpus/std_collections/hof_str
aura run corpus/std_collections/join
```

Hash collection HOFs are free functions because methods cannot declare their own
type parameters yet (C2b). They return arrays in logical table order and skip
empty/tombstone slots; they do not mutate the source collection.

`HashMap.entries()` likewise returns a fresh, shallow structural snapshot of
`HashMapEntry<K,V>` pairs. It preserves key/value pairing and can be consumed
directly with `for-in`, but it is not a live iterator or entry view: changing an
entry cannot mutate the source map.

`HashMap.entry(key)` returns a key-based mutation handle when the key exists.
Calling `handle.set(value)` replaces only that existing value and returns
`false` if it was removed. The handle retains the map and re-resolves its key
on every update, so rehash and GC cannot make it stale.

`HashMap.liveEntry(key)` returns an invalidation-checked live view when the
key exists. Its `get()` and `set(value)` operate only while `isValid()` is
true; inserting, removing, clearing, or growing/rehashing the map invalidates
the view. Updating its own value preserves validity.

Snapshot iterators are safe across source mutation, rehash, and clear. Live
iterators retain their source and become terminal after structural mutation;
`isValid()` reports the epoch check and `next()` returns `null` once invalid.
Value replacement remains visible while a cursor is valid. Map live entry
iterators yield `HashMapLiveEntry` views, whose `get()` and `set(value)` are
also invalidation-checked.

## `std.error`

Shared non-throwing error surface used by filesystem, OS, DNS, network, and
HTTP adapters.

| API                                                  | Contract                                                                                                                                 |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `ErrorKind`                                          | Stable categories: invalid input, not found, permission, I/O, network, timeout, cancelled, protocol, limit, closed, unsupported, unknown |
| `Error(kind, message, code)`                         | Owned error payload; `isRetryable()` identifies transient I/O/network/timeout failures                                                   |
| `protocol` / `network` / `invalidInput` / `notFound` | Error constructors                                                                                                                       |
| `kindCode(code)`                                     | Map a native status code to the stable category number                                                                                   |
| `Outcome<T,E>`                                       | `OutcomeOk(value)` or `OutcomeErr(error)`                                                                                                |
| `success` / `failure` / `isSuccess`                  | Import-safe outcome constructors and inspection                                                                                          |

## `std.bytes`

| API                           | Contract                                                        |
| ----------------------------- | --------------------------------------------------------------- |
| `copy` / `concat` / `equals`  | Owned byte-string copy, concatenation, and exact comparison     |
| `slice(value, start, length)` | Bounded owned slice; returns null for invalid bounds            |
| `Buffer`                      | Mutable owned byte buffer; `length`, `get`, `push`, and `clone` |
| `newBuffer()`                 | Empty `Buffer` factory                                          |

`Buffer.push` accepts only values from 0 through 255. String operations are
byte-oriented and do not perform Unicode normalization.

## `std.encoding`

| API                               | Contract                                                   |
| --------------------------------- | ---------------------------------------------------------- |
| `isValidUtf8`                     | Validate a complete UTF-8 byte sequence                    |
| `hexEncode` / `hexDecode`         | Lowercase hexadecimal encoding and bounded decoding        |
| `base64Encode` / `base64Decode`   | RFC 4648 base64 without line wrapping                      |
| `percentEncode` / `percentDecode` | RFC 3986 component escaping; malformed escapes return null |

## `std.json`

| API                                                                          | Contract                                                                   |
| ---------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `isValid`                                                                    | Validate one complete bounded JSON value                                   |
| `errorOffset`                                                                | First invalid byte offset, or -1 for valid input                           |
| `escapeString`                                                               | Encode a JSON string literal including quotes                              |
| `parse`                                                                      | Return a validated `Value`, or null                                        |
| `Value.raw` / `serialize`                                                    | Return the preserved validated JSON text                                   |
| `Value.kind`                                                                 | Return `object`, `array`, `string`, `number`, `bool`, `null`, or `invalid` |
| `Value.isObject` / `isArray` / `isString` / `isNumber` / `isBool` / `isNull` | Root-kind predicates                                                       |

The current `Value` model intentionally does not expose object-member or array
index access yet.

## `std.mime`

| API                   | Contract                                                 |
| --------------------- | -------------------------------------------------------- |
| `isValidType`         | Validate a media type and semicolon-delimited parameters |
| `sanitizeFilename`    | Remove path separators and reject unsafe or empty names  |
| `dispositionFilename` | Extract and sanitize a `filename` parameter              |

## `std.fs`

| API                                           | Contract                                                   |
| --------------------------------------------- | ---------------------------------------------------------- |
| `join` / `basename` / `dirname` / `extension` | Portable path composition and components                   |
| `isAbsolute`                                  | Check host-specific absolute path syntax                   |
| `isDirectory` / `isSymlink`                   | Inspect filesystem node kind without throwing              |
| `fileMode`                                    | 0 missing/error, 1 regular file, 2 directory, 3 other node |
| `permissions`                                 | Low nine POSIX permission bits, or 0 when unavailable      |
| `modifiedMillis`                              | Unix epoch modification time, or -1 on error               |
| `listNames`                                   | Bounded newline-delimited directory-entry snapshot         |
| `readTextResult` / `writeTextResult`          | Shared `std.error.Outcome` wrappers over text file I/O     |

## `std.os`

| API                                                | Contract                                               |
| -------------------------------------------------- | ------------------------------------------------------ |
| `getEnv` / `setEnv` / `unsetEnv`                   | Read, update, or remove environment variables          |
| `cwd` / `pid` / `platform`                         | Current directory, process ID, and platform identifier |
| `getEnvResult` / `setEnvResult` / `unsetEnvResult` | Non-throwing shared error wrappers                     |

## `std.net`

`std.net` accepts endpoint strings on POSIX targets. A numeric endpoint such as
`"8080"` binds/connects to loopback; use `"0.0.0.0:8080"` for all IPv4
interfaces or `"[::]:8080"` for IPv6. Handles are owned
`ForeignHandle<Int>` resources and async operations preserve them across
suspension.

| API                                                 | Contract                                                                                         |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `listen(endpoint)` / `connect(endpoint, timeoutMs)` | Create a listener or connect to an endpoint; `endpoint` is `PORT`, `HOST:PORT`, or `[IPv6]:PORT` |
| `accept(listener)`                                  | Async accepted-stream operation                                                                  |
| `closeListener` / `closeStream`                     | Idempotent terminal close operations                                                             |
| `readStream(stream, capacity)`                      | Async single-chunk read; empty string means EOF                                                  |
| `readAllStream(stream, capacity)`                   | Async read-until-EOF bounded by aggregate capacity                                               |
| `writeStream(stream, content)`                      | Async complete write; returns transferred byte count                                             |
| `readStreamResult` / `writeStreamResult`            | Shared `std.error.Outcome` wrappers                                                              |

## `std.dns`

| API                                 | Contract                                          |
| ----------------------------------- | ------------------------------------------------- |
| `resolveHost(host, preferIpv6)`     | One numeric IPv4/IPv6 address, or null            |
| `resolveHostList(host, preferIpv6)` | Preference-ordered newline-delimited address list |
| `resolveHostResult`                 | Shared network error outcome for lookup failure   |

## `std.url`

| API                                       | Contract                                         |
| ----------------------------------------- | ------------------------------------------------ |
| `isOriginForm` / `path` / `normalizePath` | Validate and process HTTP origin-form targets    |
| `query` / `queryValue`                    | Read raw query text or one exact key value       |
| `isAbsolute` / `authority`                | Validate and extract absolute-URI authority      |
| `authorityHost` / `authorityPort`         | Extract host and explicit decimal port           |
| `encodeComponent` / `decodeComponent`     | RFC 3986 component encoding and bounded decoding |

## `std.http`

Bounded HTTP/1.1 values and loopback client/server helpers built on
`std.net`. Server handlers receive scoped `Request` and `Response` objects;
raw foreign handles remain package-private.

| API                                                             | Contract                                              |
| --------------------------------------------------------------- | ----------------------------------------------------- |
| `Handler`                                                       | `(Request, Response) -> Task<Unit>` handler type      |
| `serveConnection` / `serve`                                     | Async bounded HTTP server entry points                |
| `get` / `post`                                                  | Async raw response helpers for loopback servers       |
| `ClientResponse(status, body)`                                  | Parsed bounded response value                         |
| `getResponse` / `postResponse`                                  | Raw client helpers returning `ClientResponse`         |
| `getResponseResult` / `postResponseResult`                      | Typed response helpers returning `std.error.Outcome`  |
| `Request.method` / `target` / `version`                         | Request-line fields                                   |
| `Request.headerCount` / `headerName` / `headerValue`            | Bounded header snapshot access                        |
| `Request.body` / `bodyReader`                                   | Body snapshot or single-reader body adapter           |
| `RequestBody.readChunk`                                         | Async bounded body chunk read; empty string means EOF |
| `Response.status` / `keepAlive`                                 | Inspect response state                                |
| `Response.setStatus` / `setKeepAlive` / `setBody` / `addHeader` | Configure response before commit                      |
| `Response.writeChunk`                                           | Async chunked response write; commits on first call   |

## `std.stream`

| API                     | Contract                                                 |
| ----------------------- | -------------------------------------------------------- |
| `Reader(stream)`        | Handler-scoped async reader over an owned network stream |
| `Reader.read(capacity)` | Read one bounded chunk                                   |
| `Reader.close()`        | Idempotently close the underlying stream                 |
| `Writer(stream)`        | Handler-scoped async writer over an owned network stream |
| `Writer.write(content)` | Write all content and return transferred bytes           |
| `Writer.close()`        | Idempotently close the underlying stream                 |

## `std.time`

All clocks and deadlines are monotonic; wall-clock changes do not affect them.

| API                                       | Contract                                                      |
| ----------------------------------------- | ------------------------------------------------------------- |
| `Duration(milliseconds)` / `milliseconds` | Typed duration; negative values are representable but invalid |
| `Duration.isValid`                        | Check for a non-negative duration                             |
| `nowMillis`                               | Current monotonic timestamp                                   |
| `Deadline(atMillis)`                      | Absolute monotonic expiry point                               |
| `Deadline.isExpired` / `remaining`        | Inspect expiry and get a non-negative remainder               |
| `after(duration)`                         | Create a deadline relative to now                             |
| `sleep` / `sleepFor` / `sleepUntil`       | Async monotonic suspension helpers                            |

## `std.task`

| API                               | Contract                                                   |
| --------------------------------- | ---------------------------------------------------------- |
| `joinTask(task)`                  | Observe completion as `std.io.Result<T, TaskError>`        |
| `cancelTask(task)`                | Request cooperative cancellation; idempotent               |
| `cancelAfter(task, milliseconds)` | Arm delayed cancellation; false for invalid/terminal tasks |
| `linkCancellation(parent, child)` | Propagate cancellation between live tasks                  |
| `isCancelled()`                   | Inspect cancellation of the current async task             |

## `std.sync`

These primitives are nonblocking. `tryLock`, `tryRead`, and `tryWrite` return
false instead of blocking an async worker.

| API         | Contract                                                                                         |
| ----------- | ------------------------------------------------------------------------------------------------ |
| `AtomicInt` | Sequentially consistent `load`, `store`, `fetchAdd`, and `compareExchange`                       |
| `Mutex`     | `tryLock`, `unlock`, and `isLocked` cooperative mutex state                                      |
| `RwLock`    | Nonblocking `tryRead`/`tryWrite`, `unlockRead`/`unlockWrite`, `readerCount`, and `isWriteLocked` |
| `Once`      | One-shot `tryEnter` gate and `isDone` inspection                                                 |

## `std.signal`

| API                   | Contract                                             |
| --------------------- | ---------------------------------------------------- |
| `installShutdown()`   | Install SIGINT/SIGTERM handling on supported targets |
| `shutdownRequested()` | Read the in-process graceful-shutdown flag           |
| `clearShutdown()`     | Clear the flag after the application drains work     |

## `std.log`

| API                                 | Contract                                                                   |
| ----------------------------------- | -------------------------------------------------------------------------- |
| `debug` / `info` / `warn` / `error` | Emit level-filtered text events                                            |
| `setMinLevel(level)`                | Set 0=debug, 1=info, 2=warn, 3=error threshold                             |
| `minLevel()`                        | Read the current threshold                                                 |
| `infoFields` / `errorFields`        | Emit alternating key/value context fields; odd trailing fields are ignored |

## `std.metrics`

| API                                   | Contract                                        |
| ------------------------------------- | ----------------------------------------------- |
| `Counter`                             | Mutable sequentially consistent integer counter |
| `add` / `increment` / `get` / `reset` | Counter mutation and inspection                 |
| `prometheus(name)`                    | Render one Prometheus text exposition sample    |

## `std.test`

| API                                               | Contract                          |
| ------------------------------------------------- | --------------------------------- |
| `assert(condition)`                               | Fail the current test when false  |
| `assertEqInt` / `assertEqString` / `assertEqBool` | Type-specific equality assertions |

## How the CLI finds `std.*`

- Auto-prelude **`std.io`** for package builds
- Path resolution for any `std.*` package:
  1. `AURA_STD` (directory that contains package directories)
  2. Walk-up from the package looking for monorepo `std/<pkg>`
  3. Release install: `share/aura/std/<pkg>` next to the toolchain
  4. Embedded copy materialized under `~/.cache/aura/<version>/std/`

After a normal install (or `cargo install` of a recent CLI), you should **not** need to declare `std.io = { path = "..." }` in app `aura.toml`.

## What is _not_ in core (by design)

Application frameworks, DI containers, ORM/HTTP stacks stay **out of core** RFCs. Expect those as ecosystem packages later, not as stdlib defaults.

## Next

- [Packages](./packages.md)
- [CLI](./cli.md)
- [Testing](./testing.md)
- [RFC-007](/rfc/007)
