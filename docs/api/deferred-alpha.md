# Aura Alpha Deferred Contract Inventory

This inventory separates a locked placeholder from a future surface whose
syntax is not yet stable enough to expose in `std/`. It is derived from RFC-003,
RFC-009, RFC-010, and RFC-011.

| RFC             | Surface                                                                    | Alpha status                                                                                                 | Blocking boundary                                                                   |
| --------------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------- |
| RFC-003         | `std.task.taskScope(() -> Unit)`                                           | Bounded compiler/runtime implementation                                                                      | Aggregate child failure reporting                                                   |
| RFC-003         | `std.task.Select<T>` / `select<T>()`                                       | Runtime/compiler implementation                                                                              | Broader async CFG integration beyond the intrinsic selector                         |
| RFC-003         | `std.sync.Lazy<T>` / `lazy<T>()`                                           | Runtime/compiler implementation                                                                              | Broader cross-platform stress coverage                                              |
| RFC-003/RFC-006 | `std.task.spawnBlocking<T>`                                                | Worker-pool implementation                                                                                   | Full cancellation semantics and unrestricted M:N placement                          |
| RFC-009         | `@attribute`, retention `Source/Binary/Runtime`                            | Typed retention metadata and versioned C emission implemented                                                | Package side tables and full runtime reflection                                     |
| RFC-009/RFC-010 | User derive/macro expansion                                                | Built-in derives, registered AST macros, token-tree expansion, and package-local exported macros implemented | Nested/hygienic expansion, dependency discovery, and plugin build integration       |
| RFC-011         | `@bench`, `std.test.benchmark`, `snapshot`, `property`                     | Placeholder functions in source; runner/reporting deferred                                                   | Generator/shrinker protocol, snapshot storage, benchmark execution and reports      |
| RFC-011         | `aura test --coverage`, LCOV/HTML instrumentation                          | CLI contract reserved; no std source                                                                         | Backend instrumentation, source maps, report format and reproducibility             |
| RFC-007         | JSON tree ownership, policy, limits, typed mapping                         | Placeholder surface locked in `std.json`                                                                     | Owned tree ABI, duplicate keys, depth/byte enforcement, reflection mapping          |
| RFC-007         | Unix sockets, HTTP/2+, HTTP/3/QUIC, password hashing                       | Reserved contract; no source package                                                                         | Transport capability matrix, crypto provider, async ownership and security review   |
| RFC-004         | Compiler metadata/derive expansion boundary                                | Built-in derive phase and metadata ABI implemented                                                           | Declarative macro phase, user proc sandbox, full expansion diagnostics              |
| RFC-006         | Worker scheduler, async I/O, concurrent GC                                 | OS-worker M:N pool, versioned `AuraReactor` boundary, POSIX backend, and executor-safe GC implemented        | Concurrent tracing collector, non-POSIX backends, and scheduler policy beyond POSIX |
| RFC-008         | Custom profiles, cross-target sysroot, build scripts                       | Contract-only build surface                                                                                  | Manifest schema, target toolchains, sandbox and reproducibility                     |
| RFC-001/002     | Overloads, structural typing, mutable/nested refs, general async ownership | Language contract deferred; no std wrapper                                                                   | Parser/type checker, borrow escape rules, capture cloning and codegen               |
| RFC-005         | `git=`/`github=` sources, workspaces, live publish/auth                    | Package-manager contract deferred                                                                            | Resolver identity, lockfile pins, credentials, registry publication                 |
| RFC-012/013     | `aura fix/doc/add/remove/update/tree/publish/clean/toolchain`              | CLI/distribution contract; not std source                                                                    | CLI compatibility, registry credentials, signing and release artifacts              |
| RFC-014         | Full LSP cancellation, semantic tokens, binding IDs, cache eviction        | Tooling contract; no Aura source shell                                                                       | Analysis query API, protocol capabilities, bounded workspace resources              |

Rules:

- A source placeholder exists only when its signature and ownership boundary
  are stable enough to lock.
- The implemented RFC-003 declarations retain a failing fallback body in the
  portable source shell; the C backend replaces those bodies with the runtime
  intrinsics listed in this table.
- A deferred contract row must not be treated as complete beyond the status
  and scope stated in this inventory.
- Each row needs corpus/compiler/runtime coverage before promotion to locked
  implementation status.
