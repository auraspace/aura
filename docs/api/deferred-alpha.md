# Aura Alpha Deferred Contract Inventory

This inventory separates a locked placeholder from a future surface whose
syntax is not yet stable enough to expose in `std/`. It is derived from RFC-003,
RFC-009, RFC-010, and RFC-011.

| RFC             | Surface                                                                    | Alpha status                                                     | Blocking boundary                                                                 |
| --------------- | -------------------------------------------------------------------------- | ---------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| RFC-003         | `std.task.taskScope(() -> Unit)`                                           | Placeholder in source; calls fail                                | Async closures, child supervision, cancellation cleanup                           |
| RFC-003         | `std.task.Select<T>` / `select<T>()`                                       | Placeholder in source; calls fail                                | Channel readiness/fairness, closed-channel outcome, scheduler integration         |
| RFC-003         | `std.sync.Lazy<T>` / `lazy<T>()`                                           | Placeholder in source; calls fail                                | Task-safe ownership and exactly-once initialization                               |
| RFC-003/RFC-006 | `std.task.spawnBlocking<T>`                                                | Placeholder in source; calls fail                                | Runtime workers, scheduler placement, cancellation semantics                      |
| RFC-009         | `@attribute`, retention `Source/Binary/Runtime`                            | Attribute contract documented; emission deferred                 | Attribute declarations, package metadata, runtime retention                       |
| RFC-009/RFC-010 | User derive/macro expansion                                                | Compiler names locked in `compiler-alpha.md`; expansion deferred | Macro ABI, sandbox, expansion ordering and diagnostics                            |
| RFC-011         | `@bench`, `std.test.benchmark`, `snapshot`, `property`                     | Placeholder functions in source; runner/reporting deferred       | Generator/shrinker protocol, snapshot storage, benchmark execution and reports    |
| RFC-011         | `aura test --coverage`, LCOV/HTML instrumentation                          | CLI contract reserved; no std source                             | Backend instrumentation, source maps, report format and reproducibility           |
| RFC-007         | JSON tree ownership, policy, limits, typed mapping                         | Placeholder surface locked in `std.json`                         | Owned tree ABI, duplicate keys, depth/byte enforcement, reflection mapping        |
| RFC-007         | Unix sockets, HTTP/2+, HTTP/3/QUIC, password hashing                       | Reserved contract; no source package                             | Transport capability matrix, crypto provider, async ownership and security review |
| RFC-004         | Compiler metadata/derive expansion boundary                                | Contract-only; not a std/builtin wrapper                         | AST expansion order, name resolution, diagnostics, metadata ABI                   |
| RFC-006         | Worker scheduler, async I/O, concurrent GC                                 | Contract-only runtime work                                       | Runtime ABI, OS reactor, task ownership, root maps                                |
| RFC-008         | Custom profiles, cross-target sysroot, build scripts                       | Contract-only build surface                                      | Manifest schema, target toolchains, sandbox and reproducibility                   |
| RFC-001/002     | Overloads, structural typing, mutable/nested refs, general async ownership | Language contract deferred; no std wrapper                       | Parser/type checker, borrow escape rules, capture cloning and codegen             |
| RFC-005         | `git=`/`github=` sources, workspaces, live publish/auth                    | Package-manager contract deferred                                | Resolver identity, lockfile pins, credentials, registry publication               |
| RFC-012/013     | `aura fix/doc/add/remove/update/tree/publish/clean/toolchain`              | CLI/distribution contract; not std source                        | CLI compatibility, registry credentials, signing and release artifacts            |
| RFC-014         | Full LSP cancellation, semantic tokens, binding IDs, cache eviction        | Tooling contract; no Aura source shell                           | Analysis query API, protocol capabilities, bounded workspace resources            |

Rules:

- A source placeholder exists only when its signature and ownership boundary
  are stable enough to lock.
- A deferred contract row must not be treated as implemented or callable.
- Each row needs corpus/compiler/runtime coverage before promotion to locked
  implementation status.
