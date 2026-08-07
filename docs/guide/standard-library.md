---
title: Standard library
section: Toolchain
order: 55
summary: In-tree std packages, public API contracts, and prelude resolution.
---

# Standard library

Aura’s **core** stdlib is intentionally small ([RFC-007](/rfc/007), [RFC-000](/rfc/000) batteries-included-but-modular). In this repository, packages live under `std/`.

## Packages today

| Package           | Path              | Role                                                                                                                       |
| ----------------- | ----------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `std.io`          | `std/io`          | Console, file I/O, argv, stdin, exit                                                                                       |
| `std.assert`      | `std/assert`      | Assert helpers for tests                                                                                                   |
| `std.collections` | `std/collections` | Map/Set/List, generic hash collections, snapshot and live iterators, `Iterable`, HOFs, `join`                              |
| `std.error`       | `std/error`       | Shared error categories, owned errors, and generic outcomes                                                                |
| `std.bytes`       | `std/bytes`       | Validated `Byte`, binary `Buffer` operations, and network-order integer helpers                                            |
| `std.encoding`    | `std/encoding`    | UTF-8, hexadecimal, base64, and percent encoding                                                                           |
| `std.json`        | `std/json`        | Bounded JSON validation, parsing, escaping, root classification, and typed generic decoding                                |
| `std.mime`        | `std/mime`        | Media-type validation and upload filename sanitization                                                                     |
| `std.fs`          | `std/fs`          | Portable path and filesystem metadata helpers                                                                              |
| `std.os`          | `std/os`          | Environment, process, platform, and working-directory helpers                                                              |
| `std.net`         | `std/net`         | Nonblocking endpoint-aware TCP listeners, connections, streams, and typed failures                                         |
| `std.dns`         | `std/dns`         | Bounded numeric host resolution                                                                                            |
| `std.url`         | `std/url`         | Origin-form and absolute URI parsing plus component encoding                                                               |
| `std.http`        | `std/http`        | Bounded HTTP/1.1 client/server request and response values                                                                 |
| `std.stream`      | `std/stream`      | Async reader/writer adapters over owned network streams                                                                    |
| `std.time`        | `std/time`        | Monotonic durations, deadlines, and async sleep                                                                            |
| `std.task`        | `std/task`        | Task join, cancellation, and cancellation linking                                                                          |
| `std.sync`        | `std/sync`        | Nonblocking atomics, mutexes, reader/writer locks, and one-shot gates                                                      |
| `std.signal`      | `std/signal`      | Graceful SIGINT/SIGTERM shutdown state                                                                                     |
| `std.log`         | `std/log`         | Bounded level-based and structured text logging                                                                            |
| `std.metrics`     | `std/metrics`     | Sequentially consistent counters and Prometheus samples                                                                    |
| `std.test`        | `std/test`        | Deterministic assertion helpers for native and corpus tests                                                                |
| `std.crypto`      | `std/crypto`      | Runtime-backed MD5/SHA-256, HMAC, PBKDF2, secure randomness, and TLS foundations                                           |
| `std.reflect`     | `std/reflect`     | Bounded compiler-backed type/member metadata                                                                               |
| `std.tls`         | `std/tls`         | OpenSSL-backed verified TLS client with String and binary stream adapters                                                  |
| `std.udp`         | `std/udp`         | Runtime-backed bounded endpoint/datagram transport on POSIX                                                                |
| `std.websocket`   | `std/websocket`   | Runtime-backed bounded WebSocket client framing                                                                            |
| `std.compress`    | `std/compress`    | Bounded gzip/deflate text round-trip with hex-safe compressed output                                                       |
| `std.multipart`   | `std/multipart`   | Bounded multipart parser/encoder with line-delimited boundaries, escaped quoted parameters, and header-injection rejection |

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
| `readLineResult()`       | `Result<String?, String>`; `Ok(null)` is EOF and `Err` is an I/O failure          |
| `readAllStdin(): String` | Remainder of stdin (throws on oversize / I/O / embedded NUL)                      |
| `readAllStdinResult()`   | `Result<String, String>` with an owned stdin failure message                      |
| `exit(code: Int)`        | Terminate with status; flushes stdout/stderr first; does not return (C12e)        |

### Task outcomes

`join` returns `Result<T, TaskError>`. `taskErrorTypeName` exposes the typed
failure name when available; `taskErrorSpanStart` and `taskErrorSpanEnd`
expose the retained source span, and `taskErrorSourceId` exposes its stable
throw-origin identity without borrowing the child task frame.

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
| `fileExistsResult(path): Result<Bool, String>`         | non-throwing regular-file existence check                     |
| `fileSize(path): Int`                                  | byte size (throws if missing)                                 |
| `fileSizeResult(path): Result<Int, String>`            | non-throwing regular-file size query                          |
| `openFile(path, mode): ForeignHandle<Int>`             | owned handle; mode 0 read, 1 truncate, 2 read/write, 3 append |
| `readFd(fd, capacity): String`                         | async bounded descriptor read                                 |
| `readFdResult(fd, capacity): Result<String, String>`   | async descriptor read with an owned failure message           |
| `writeFd(fd, content): Int`                            | async descriptor write; returns bytes                         |
| `writeFdResult(fd, content): Result<Int, String>`      | async descriptor write with an owned failure message          |

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

The RFC-011 names are `assertTrue`, generic `assertEqual`, generic
`assertNotNull`, and `assertFails`. The typed `assertEqInt`, `assertEqString`,
and `assertEqBool` helpers remain alpha compatibility aliases; the
language-level `assert_eq` helpers are separate builtins.

`benchmark`, `snapshot`, and `property` provide deterministic advanced testing
hooks. Benchmarks use `AURA_BENCH_ITERATIONS`; snapshots read from
`AURA_SNAPSHOT_DIR` and can be created or updated with
`AURA_UPDATE_SNAPSHOTS=1`; property checks execute the requested number of
cases. The CLI runner and richer generator/report protocols remain separate
follow-up work.

## `std.collections`

| Type / helper                                                            | Notes                                                                               |
| ------------------------------------------------------------------------ | ----------------------------------------------------------------------------------- |
| `Map<K, V>`                                                              | Linear map; `get` → `V?`; `put` / `remove` / `clear`                                |
| `Set<T>`                                                                 | Generic set (linear)                                                                |
| `map_string_int()` / `set()`                                             | Empty concrete compatibility factories                                              |
| `HashMap<K,V>`                                                           | Generic open addressing with `K: Hashable`; `containsValue` (C19a)                  |
| `hashMap<K,V>()`                                                         | Generic factory for typed named-parameter maps (compatibility factories remain)     |
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

`List<T>`, `List.of(...)`, `listOf(...)`, and `list<T>()` provide the growable
list API backed by owning `Array<T>` storage. `map<R>` supports element-type
transforms. `iterator()` and `toArray()` return independent snapshots, so later
list mutation cannot invalidate or alias the returned values.

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

| API                                                                | Contract                                                                                                                                                                                          |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ErrorKind`                                                        | RFC names: `InvalidInput`, `Unsupported`, `NotFound`, `PermissionDenied`, `WouldBlock`, `TimedOut`, `Cancelled`, `Disconnected`, `LimitExceeded`, `Protocol`, `System` plus legacy alpha variants |
| `Error(kind, message, code)`                                       | Owned error payload; `isRetryable()` identifies transient I/O/network/timeout failures                                                                                                            |
| `protocol` / `network` / `transport` / `invalidInput` / `notFound` | Error constructors; `transport` classifies timeout, cancellation, and peer-close diagnostics                                                                                                      |
| `kindCode(code)`                                                   | Map a native status code to the stable category number                                                                                                                                            |
| `Outcome<T,E>`                                                     | `OutcomeOk(value)` or `OutcomeErr(error)`                                                                                                                                                         |
| `success` / `failure` / `isSuccess`                                | Import-safe outcome constructors and inspection                                                                                                                                                   |

## `std.bytes`

| API                                     | Contract                                                               |
| --------------------------------------- | ---------------------------------------------------------------------- |
| `Byte` / `byte(value)`                  | Nominal unsigned byte; construction rejects values outside 0..255      |
| `copy` / `concat` / `equals`            | Owned byte-string copy, concatenation, and exact comparison            |
| `slice(value, start, length)`           | Bounded owned slice; returns null for invalid bounds                   |
| `Buffer` / `newBuffer()`                | Mutable owned binary buffer and empty-buffer factory                   |
| `Buffer.length` / `get` / `clone`       | Length, nullable integer inspection, and deep copy                     |
| `Buffer.appendByte` / `readByte`        | Append/read validated `Byte` values without UTF-8 conversion           |
| `Buffer.writeByte` / `slice` / `concat` | In-place write, independent bounded slice, and owned concatenation     |
| `readInt16BE` / `readInt32BE`           | Read unsigned network-order integers; return null for invalid bounds   |
| `writeInt16BE` / `writeInt32BE`         | Write network-order integers; return false for invalid value or bounds |

The legacy `Buffer.push` and `Buffer.get` integer operations remain available
for compatibility. Binary protocol code should use `Byte` and the explicit
buffer methods so payloads never pass through UTF-8 `String` conversion.

## `std.encoding`

| API                               | Contract                                                   |
| --------------------------------- | ---------------------------------------------------------- |
| `isValidUtf8`                     | Validate a complete UTF-8 byte sequence                    |
| `hexEncode` / `hexDecode`         | Lowercase hexadecimal encoding and bounded decoding        |
| `base64Encode` / `base64Decode`   | RFC 4648 base64 without line wrapping                      |
| `percentEncode` / `percentDecode` | RFC 3986 component escaping; malformed escapes return null |

## `std.json`

| API                                                                          | Contract                                                                                             |
| ---------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `isValid`                                                                    | Validate one complete bounded JSON value                                                             |
| `errorOffset`                                                                | First invalid byte offset, or -1 for valid input                                                     |
| `escapeString`                                                               | Encode a JSON string literal including quotes                                                        |
| `parse`                                                                      | Return a validated `Value`, or null                                                                  |
| `Value.raw` / `serialize`                                                    | Return the preserved validated JSON text                                                             |
| `Value.kind`                                                                 | Return `object`, `array`, `string`, `number`, `bool`, `null`, or `invalid`                           |
| `Value.isObject` / `isArray` / `isString` / `isNumber` / `isBool` / `isNull` | Root-kind predicates                                                                                 |
| `Value.get` / `at` / `asString` / `keys`                                     | Bounded source-backed traversal and string access                                                    |
| `ParseOptions` / `DuplicateKeyPolicy` / `ParseError`                         | Locked bounds, duplicate-key, and typed-failure contract                                             |
| `parseWithOptions` / `parseResult` / `decode<T>`                             | Bounded parser outcomes plus primitive, nested primitive-array, and recursive generic class decoding |
| `Value.clone` / `byteLength` / `depth`                                       | Independent clone and bounded source/tree metadata                                                   |

`Value` exposes bounded object-member and array traversal. `ParseOptions`
supports `maxBytes`, `maxDepth`, and explicit duplicate-key behavior (`Reject`,
`FirstWins`, `LastWins`). Typed decoding supports primitives, `Array<Int>`,
`Array<Bool>`, `Array<String>`, recursively nested primitive arrays, and
recursive application classes with generic, nullable, nested-class, struct,
unit-enum, and primitive/class-array fields. Payload-bearing enum mapping
remains outside this bounded contract.

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

| API                                                 | Contract                                                                           |
| --------------------------------------------------- | ---------------------------------------------------------------------------------- |
| `listen(endpoint)` / `connect(endpoint, timeoutMs)` | Legacy throwing handles; `endpoint` is `PORT`, `HOST:PORT`, or `[IPv6]:PORT`       |
| `listenResult` / `connectResult`                    | Typed `Outcome` wrappers returning owned handles or `NetError`                     |
| `accept(listener)`                                  | Async accepted-stream operation                                                    |
| `closeListener` / `closeStream`                     | Idempotent terminal close operations                                               |
| `closeListenerResult` / `closeStreamResult`         | Typed `Outcome<Bool, NetError>` compatibility wrappers                             |
| `readStream(stream, capacity)`                      | Async single-chunk read; empty string means EOF                                    |
| `readAllStream(stream, capacity)`                   | Async read-until-EOF bounded by aggregate capacity                                 |
| `writeStream(stream, content)`                      | Async complete write; returns transferred byte count                               |
| `readExactly(stream, length)`                       | Async exact binary read into `std.bytes.Buffer`; distinguishes EOF and partial EOF |
| `readExactlyWithTimeout` / `writeAllWithTimeout`    | Binary exact read/write with an operation deadline in milliseconds                 |
| `writeAll(stream, bytes)`                           | Async complete binary write from `std.bytes.Buffer`; returns byte count            |
| `readStreamResult` / `writeStreamResult`            | Shared `std.error.Outcome` wrappers                                                |

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

The bounded server accepts origin-form `GET`, `HEAD`, `POST`, `PUT`, `PATCH`,
`DELETE`, and `OPTIONS` requests. Other methods receive a bounded 405 response.

| API                                                             | Contract                                                                                           |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `Handler`                                                       | `(Request, Response) -> Task<Unit>` handler type                                                   |
| `serveConnection` / `serve`                                     | Async bounded HTTP server entry points                                                             |
| `get` / `post`                                                  | Async raw response helpers for loopback servers                                                    |
| `ClientResponse(status, body)`                                  | Parsed bounded response value                                                                      |
| `getResponse` / `postResponse`                                  | Raw client helpers returning `ClientResponse`                                                      |
| `getResponseResult` / `postResponseResult`                      | Typed response helpers returning `std.error.Outcome`, including transport failures                 |
| `Request.method` / `target` / `version`                         | Request-line fields                                                                                |
| `Request.headerCount` / `headerName` / `headerValue`            | Bounded header snapshot access                                                                     |
| `Request.body` / `bodyReader`                                   | Body snapshot or single-reader body adapter                                                        |
| `RequestBody.readChunk`                                         | Async bounded single-reader chunk read; claim is held across suspension and empty string means EOF |
| `RequestBody.readChunkResult`                                   | Typed `Outcome<String, HttpError>` body-read boundary                                              |
| `Response.status` / `keepAlive`                                 | Inspect response state                                                                             |
| `Response.setStatus` / `setKeepAlive` / `setBody` / `addHeader` | Configure response before commit                                                                   |
| `Response.writeChunk`                                           | Async chunked response write; commits on first call                                                |
| `Response.writeChunkResult`                                     | Typed `Outcome<Bool, HttpError>` response-write boundary                                           |

## `std.stream`

| API                                | Contract                                                 |
| ---------------------------------- | -------------------------------------------------------- |
| `Reader(stream)`                   | Handler-scoped async reader over an owned network stream |
| `Reader.read(capacity)`            | Read one bounded String chunk                            |
| `Reader.readBytes` / `readExactly` | Exact binary reads into `std.bytes.Buffer`               |
| `Writer.writeBytes` / `writeAll`   | Complete binary writes from `std.bytes.Buffer`           |
| `Reader.close()`                   | Idempotently close the underlying stream                 |
| `Writer(stream)`                   | Handler-scoped async writer over an owned network stream |
| `Writer.write(content)`            | Write all content and return transferred bytes           |
| `Writer.close()`                   | Idempotently close the underlying stream                 |

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

| API                               | Contract                                                    |
| --------------------------------- | ----------------------------------------------------------- |
| `joinTask(task)`                  | Observe completion as `std.io.Result<T, TaskError>`         |
| `cancelTask(task)`                | Request cooperative cancellation; idempotent                |
| `cancelAfter(task, milliseconds)` | Arm delayed cancellation; false for invalid/terminal tasks  |
| `linkCancellation(parent, child)` | Propagate cancellation between live tasks                   |
| `isCancelled()`                   | Inspect cancellation of the current async task              |
| `taskScope(body)`                 | Structured scope with child adoption and cancellation drain |
| `Select<T>` / `select<T>()`       | Scheduler-backed channel selection with fair wakeups        |
| `spawnBlocking<T>(body)`          | OS-worker execution with cooperative cancellation           |

## `std.sync`

These primitives are nonblocking. `tryLock`, `tryRead`, and `tryWrite` return
false instead of blocking an async worker.

| API                     | Contract                                                                                         |
| ----------------------- | ------------------------------------------------------------------------------------------------ |
| `AtomicInt`             | Sequentially consistent `load`, `store`, `fetchAdd`, and `compareExchange`                       |
| `Mutex`                 | `tryLock`, `unlock`, and `isLocked` cooperative mutex state                                      |
| `RwLock`                | Nonblocking `tryRead`/`tryWrite`, `unlockRead`/`unlockWrite`, `readerCount`, and `isWriteLocked` |
| `Once`                  | One-shot `tryEnter` gate and `isDone` inspection                                                 |
| `Lazy<T>` / `lazy<T>()` | Exactly-once task-safe initialization cell                                                       |

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

| API                                                            | Contract                          |
| -------------------------------------------------------------- | --------------------------------- |
| `assert(condition)`                                            | Fail the current test when false  |
| `assertTrue` / `assertEqual` / `assertNotNull` / `assertFails` | RFC-011 canonical test assertions |

## `std.crypto`

The alpha contract provides `Digest`, `TlsConfig`, `TlsConnection`,
`randomBytes`, `randomBytesBuffer`, `md5Bytes`, `sha256`, `sha256Bytes`,
`hmacSha256`, `hmacSha256Bytes`, `pbkdf2Sha256`, and
`constantTimeEquals`. Binary crypto APIs accept and return
`std.bytes.Buffer`, so NUL bytes are preserved. PBKDF2-HMAC-SHA-256 supports
SCRAM-SHA-256 key derivation; randomness, hashes, HMAC, PBKDF2, and constant
time comparison are backed by the native runtime.

## `std.tls`

| API                                     | Contract                                                             |
| --------------------------------------- | -------------------------------------------------------------------- |
| `config(serverName, verifyPeer)`        | Verified TLS configuration                                           |
| `connect(endpoint, options)`            | Async OpenSSL client with hostname verification                      |
| `wrapStream(stream, endpoint, options)` | Upgrade an existing `std.net` TCP stream without String conversion   |
| `Connection.read` / `write`             | String compatibility stream operations                               |
| `Connection.readBytes` / `writeBytes`   | Binary stream operations over `std.bytes.Buffer`                     |
| `*WithTimeout` methods                  | Binary operations with a monotonic deadline in milliseconds          |
| `Connection.close()`                    | Idempotent close; pending TLS waits are woken and resources released |
| `loadCertificate(path)`                 | Load bounded certificate subject and issuer metadata                 |

TLS async operations use the underlying TCP readiness scheduler. A cancelled
read or write closes the TLS session and wakes the socket wait, preventing a
cancelled task from retaining a blocked descriptor.

## `std.reflect`

The package provides compiler-backed `typeOf<T>`, `typeIdOf<T>`, type-kind
classification, and declaration metadata. Primitive types are always
reflectable; user classes, structs, enums, and interfaces opt in with
`@reflect`. Only public fields and methods are exposed. Closed generic class
and interface metadata uses the concrete monomorph name and substitutes type
parameters in exposed field and method return types.

## Bounded protocol packages

`std.tls`, `std.websocket`, `std.compress`, and `std.multipart` provide
runtime-backed bounded implementations with explicit size and platform limits.
Unix sockets, HTTP/2/3, and QUIC remain reserved without a public package until
their ownership and capability contracts are settled.

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
