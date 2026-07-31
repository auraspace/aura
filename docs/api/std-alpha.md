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

| Area                                                                   | Alpha state                            | Meaning                                                                                                                                                     |
| ---------------------------------------------------------------------- | -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `std.io` through `std.metrics`                                         | Locked and implemented in bounded form | Usable today within the documented target and size limits                                                                                                   |
| `std.crypto`, `std.reflect`                                            | Locked placeholder                     | Names and value shapes exist; calls fail with a placeholder error                                                                                           |
| `std.tls`, `std.udp`, `std.websocket`, `std.compress`, `std.multipart` | Locked placeholder                     | Transport/protocol shapes exist; backends and parsers are not wired                                                                                         |
| `std.json` traversal/mapping                                           | Locked placeholder                     | `Value.get`, `at`, `asString`, `keys`, `clone`, `byteLength`, `depth`, `parseWithOptions`, `parseResult`, and `decode<T>` are reserved and fail explicitly  |
| `std.json` parse policy                                                | Locked placeholder                     | `ParseOptions`, `DuplicateKeyPolicy`, and `ParseError` reserve bounds/duplicate-key semantics                                                               |
| `std.task.taskScope`, `Select<T>`, `spawnBlocking<T>`                  | Locked placeholder                     | Structured concurrency and worker-pool names/signatures are reserved; calls fail explicitly                                                                 |
| `std.sync.Lazy<T>`                                                     | Locked placeholder                     | Exactly-once lazy initialization shape is reserved; calls fail explicitly                                                                                   |
| `std.collections.List<T>`                                              | Implemented (initial)                  | Owning Array-backed list with `List.of`, `listOf`, indexed mutation, stack operations, and generic `map<R>`; iterator/clone semantics remain follow-up work |
| `std.test.benchmark`, `snapshot`, `property`                           | Locked placeholder                     | RFC-011 advanced test hooks fail explicitly; runner protocols are not wired                                                                                 |
| Unix sockets, HTTP/2+, HTTP/3/QUIC                                     | Reserved                               | Design/API follow-ons; no package is exposed yet                                                                                                            |

## Placeholder rule

Placeholder APIs must be explicit in their documentation and must fail at the
call boundary. Returning empty data or a successful security result is not an
acceptable implementation. Replacing a placeholder with a real backend is an
implementation change only when the locked signature and ownership contract
remain compatible.

## What is still required for a fully productive language

The API surface is now broad enough for CLI, file, collection, service, and
bounded HTTP applications. “Complete” still requires separate implementation
work for general async lowering and aggregate ownership, non-empty spawn
capture transfer, macro/derive expansion, thread scheduling and concurrent GC,
cryptographic/TLS backends, full JSON traversal/mapping, and release evidence
on deferred targets. Those gaps are tracked as debt rather than hidden behind
an unstable API.

See [`deferred-alpha.md`](deferred-alpha.md) for RFC-derived surfaces that are
reserved as contracts but intentionally have no source placeholder yet because
their syntax or ownership model is not frozen.
