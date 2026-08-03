# Aura Alpha Standard API Lock

Status: locked for alpha source compatibility. The package list is recorded in
[`std/api-lock.tsv`](../../std/api-lock.tsv). A locked API may gain a new
versioned package, but existing public names, parameter order, return types,
ownership, error conventions, and async boundaries must not change silently.

The `std/` Aura sources are the canonical declarations. The standard-library
guide documents behavior and limits; this file records the compatibility rule
and the intentionally incomplete shells. `std/api-symbol-digests.tsv` is the
machine-checked snapshot of public declaration lines; intentional API changes
must update it together with this document.

## Completeness matrix

| Area                                                                   | Alpha state                            | Meaning                                                                                                                                                        |
| ---------------------------------------------------------------------- | -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `std.io` through `std.metrics`                                         | Locked and implemented in bounded form | Usable today within the documented target and size limits                                                                                                      |
| `std.crypto`, `std.reflect`                                            | Locked placeholder                     | Names and value shapes exist; calls fail with a placeholder error                                                                                              |
| `std.tls`, `std.udp`, `std.websocket`, `std.compress`, `std.multipart` | Locked placeholder                     | Transport/protocol shapes exist; backends and parsers are not wired                                                                                            |
| `std.json` bounded value core                                          | Bounded implementation                 | Validation, escaping, root classification, clone, byte length/depth, bounded options, and typed parse outcomes are implemented; traversal/mapping remains open |
| `std.json` traversal/mapping                                           | Locked placeholder                     | `Value.get`, `at`, `asString`, `keys`, duplicate-key policy enforcement, and `decode<T>` remain reserved and fail explicitly                                   |
| `std.task.taskScope`, `Select<T>`, `spawnBlocking<T>`                  | Bounded implementation                 | Scope supervision, channel selection, and worker-pool execution are available within the documented MVP limits                                                 |
| `std.sync.Lazy<T>`                                                     | Bounded implementation                 | Exactly-once task-safe initialization is available within the documented MVP limits                                                                            |
| `std.collections.List<T>`                                              | Implemented (initial)                  | Owning Array-backed list with `List.of`, `listOf`, indexed mutation, stack operations, and generic `map<R>`; iterator/clone semantics remain follow-up work    |
| `std.test` core and smoke hooks                                        | Bounded implementation                 | Assertions execute; benchmark/property/snapshot hooks provide deterministic smoke behavior, while runner reports/storage remain open                           |
| Unix sockets, HTTP/2+, HTTP/3/QUIC                                     | Reserved                               | Design/API follow-ons; no package is exposed yet                                                                                                               |

## Placeholder rule

Placeholder APIs must be explicit in their documentation and must fail at the
call boundary. Returning empty data or a successful security result is not an
acceptable implementation. Replacing a placeholder with a real backend is an
implementation change only when the locked signature and ownership contract
remain compatible.

## What is still required for a fully productive language

The API surface is now broad enough for CLI, file, collection, service, and
bounded HTTP applications. The compiler/runtime now cover typed task outcomes,
aggregate ownership across the shipped async CFGs, non-empty generic spawn
captures, owned Array closure captures, token-tree/derive expansion, and typed
async frame GC hooks. Remaining limits are arbitrary method-aware HTTP handler
CFGs, nested/hygienic macro expansion and dependency plugin provenance, a
concurrent tracing collector, cryptographic/TLS backends, full JSON
traversal/mapping, and release evidence on deferred targets. Those gaps remain
tracked as debt rather than hidden behind an unstable API.

See [`deferred-alpha.md`](deferred-alpha.md) for RFC-derived surfaces that are
reserved as contracts but intentionally have no source placeholder yet because
their syntax or ownership model is not frozen.
