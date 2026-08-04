# Aura Standard Library Implementation Status

Status: actively implemented. The package inventory is recorded in
[`std/api-status.tsv`](../../std/api-status.tsv). Public APIs may evolve while
the standard library is being completed; every change must update its tests,
documentation, and callers in the same implementation slice.

The `std/` Aura sources are the canonical declarations. The standard-library
guide documents behavior and limits; this file records the compatibility rule
and the remaining implementation boundaries. The status file is an audit
manifest, not an API freeze.

## Completeness matrix

| Area                                                  | Alpha state                 | Meaning                                                                                                                                                                                                                                                                                                                             |
| ----------------------------------------------------- | --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `std.io` through `std.metrics`                        | Implemented in bounded form | Usable today within the documented target and size limits; APIs remain open while completion work continues                                                                                                                                                                                                                         |
| `std.crypto`                                          | Bounded baseline            | Random bytes, SHA-256, HMAC-SHA256, constant-time comparison, and verified TLS client are runtime-backed                                                                                                                                                                                                                            |
| `std.reflect`                                         | Bounded type metadata       | `typeOf`, `typeIdOf`, type-kind classification, and public member metadata are compiler-backed; user declarations opt in with `@reflect`, including closed generic class/interface metadata with substituted member types                                                                                                           |
| `std.tls`                                             | Bounded implementation      | OpenSSL-backed client handshake, hostname verification, certificate loading, and bounded I/O                                                                                                                                                                                                                                        |
| `std.udp`                                             | Bounded implementation      | Endpoint-keyed POSIX datagrams with nonblocking bind/send/receive, bounded payloads, idempotent close, and unsupported-target failure                                                                                                                                                                                               |
| `std.websocket`                                       | Bounded implementation      | POSIX WebSocket client handshake, masked text/binary/control frames, bounded payloads, and idempotent close                                                                                                                                                                                                                         |
| `std.compress`                                        | Bounded implementation      | Gzip/deflate text round-trip; compressed bytes use a reversible hex wire representation due NUL-terminated String ABI                                                                                                                                                                                                               |
| `std.multipart`                                       | Bounded implementation      | Boundary encoder/parser with quoted form-data parameters and malformed-part skipping                                                                                                                                                                                                                                                |
| `std.json` bounded value core                         | Bounded implementation      | Validation, escaping, root classification, clone, byte length/depth, bounded options, typed outcomes, traversal, and primitive/nested-primitive-array/recursive-class `decode<T>` are implemented within documented shape limits                                                                                                    |
| `std.json` traversal/mapping                          | Bounded implementation      | `Value.get`, `at`, `asString`, `keys`, reject-duplicate policy, primitive decode, recursively nested primitive arrays, recursive nested class/struct mapping (including generic class instantiations and nullable class fields), unit-enum fields, and primitive/class/unit-enum arrays are implemented over owned validated slices |
| `std.stream`                                          | Bounded implementation      | Owned TCP reader/writer adapters expose raw operations plus typed `Outcome` read/write/close boundaries                                                                                                                                                                                                                             |
| `std.task.taskScope`, `Select<T>`, `spawnBlocking<T>` | Bounded implementation      | Scope supervision, channel selection, and worker-pool execution are available within the documented MVP limits                                                                                                                                                                                                                      |
| `std.sync.Lazy<T>`                                    | Bounded implementation      | Exactly-once task-safe initialization is available within the documented MVP limits                                                                                                                                                                                                                                                 |
| `std.collections.List<T>`                             | Implemented                 | Owning Array-backed list with `List.of`, `listOf`, indexed mutation, stack operations, generic `map<R>`, independent snapshots, and `ListIterator<T>`                                                                                                                                                                               |
| `std.test` core and smoke hooks                       | Bounded implementation      | Assertions execute; benchmark/property hooks and persistent snapshot create/read/mismatch checks are covered, and `aura test/bench --format json` reports per-case and package durations                                                                                                                                            |
| Unix sockets, HTTP/2+, HTTP/3/QUIC                    | Reserved                    | Design/API follow-ons; no package is exposed yet                                                                                                                                                                                                                                                                                    |

## Implementation rule

No placeholder, no-op, or successful fake security result may remain in a
completed package. Portable Aura declarations may use an explicit compiler or
runtime intrinsic bridge, but that bridge must have a real backend and direct
test coverage.

## What is still required for a fully productive language

The API surface is now broad enough for CLI, file, collection, service, and
bounded HTTP applications. The compiler/runtime now cover typed task outcomes,
aggregate ownership across the shipped async CFGs, non-empty generic spawn
captures, owned Array closure captures, token-tree/derive expansion, and typed
async frame GC hooks. Remaining limits are arbitrary method-aware HTTP handler
CFGs, nested/hygienic macro expansion and dependency plugin provenance, a
concurrent tracing collector, broader cryptographic algorithms and TLS server
support, richer JSON aggregate traversal/mapping (enums and arbitrary aggregate
leaves), and release evidence on deferred targets. Those gaps remain
tracked as debt rather than hidden behind an unstable API.

See [`deferred-alpha.md`](deferred-alpha.md) for RFC-derived surfaces that are
reserved as contracts but intentionally have no source placeholder yet because
their syntax or ownership model is not frozen.
