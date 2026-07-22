# Technical Debt

Standing log of temporary workarounds, incomplete behavior, and deferred follow-ups.

When you introduce or discover debt, add an entry here in the same change.
When you resolve debt, update or remove the matching entry.

## Open

### H6 routing is synchronous and exact-match only (2026-07-22)

- Area: HTTP handler API
- Symptom: `aura_http_dispatch_routes` now supports exact method/path matching
  and deterministic 404/405/500 responses, but it does not suspend, spawn, or
  retain request/response buffers across an await.
- Why deferred: those lifetimes depend on H5 async integration and the A5/A6
  frame ownership contract.
- Progress: `runtime/tests/http_routes.c` covers route success, method mismatch,
  not-found, handler failure, and borrowed callback ownership.
- Next step: adapt the route table to async handler frames after H5 is complete.

### IO5 network backpressure remains deferred (2026-07-22)

- Area: async I/O/channel bridge
- Symptom: bounded channels now suspend/wake producers and consumers with FIFO
  ownership and deterministic close/cancel cleanup, but TCP/file completion is
  not connected to this bridge.
- Why deferred: IO3/IO6 still lack scheduler-integrated network operations and
  cross-platform async wake sources.
- Progress: `runtime/tests/task_channel.c` covers full/empty, ordering,
  cancellation, close, and payload destruction.
- Next step: connect readiness/completion events to the existing channel waiter
  contract after async I/O suspension is implemented.

### A7 remains a bounded task-outcome ABI (2026-07-22)

- Area: async exception outcomes
- Symptom: the runtime now exposes and tests success, failure, and cancellation
  ordering with numeric source identity, but has no compiler-generated
  suspension propagation, file/line source spans, or nested exception chains.
- Why deferred: those representations require typed async lowering and a
  defined cancellation-throw policy; the C frame ABI must not imply either.
- Progress: `runtime/tests/task_outcomes.c` proves owned payload cleanup before
  join observation and deterministic cancellation cleanup under the existing
  executor.
- Next step: extend the typed outcome ABI after A6 suspension lowering defines
  source-span and nested-failure ownership.

### Unjoined task failures are bounded runtime diagnostics (S6, 2026-07-22)

- Area: task outcome policy
- Symptom: unjoined terminal failures now report through a borrowed hook or
  default stderr logger, but compiler-generated nested failure chains and
  process-level aggregation are not yet defined.
- Why deferred: those payloads depend on A6/A7 outcome representation and the
  release diagnostics contract.
- Progress: task/source identity, failure bytes, joined suppression, shutdown
  reporting, and cancellation exclusion are covered by the focused fixture.
- Next step: connect the hook to typed async outcomes and release telemetry.

### A8 sanitizer fixture is bounded to the current frame ABI (2026-07-22)

- Area: runtime async ABI sanitizer coverage
- Symptom: the new sanitizer fixture proves root retention and cleanup for
  pending, cancellation, repeated polling, dropped-handle, and failure paths,
  but cannot exercise compiler-generated live-local hoisting or a delayed
  asynchronous wakeup.
- Why deferred: A4–A7 still define only bounded state metadata and the
  single-threaded executor contract; full suspension lowering and wake sources
  are not present in the runtime API.
- Progress: `runtime/tests/task_frame_sanitizer.c` runs the supported paths
  under ASAN/UBSAN, with an LSAN attempt when the host supports leak detection.
- Next step: extend the sanitizer matrix after typed frame roots, resume edges,
  and async wakeup semantics are implemented.

### S5 cancellation boundaries remain bounded (2026-07-22)

- Area: spawn cancellation and executor outcomes
- Symptom: request/acknowledgement ordering and joined/unjoined cleanup are
  defined for the current single-threaded ready/pending frame API, but await,
  I/O, handler, and concurrent completion boundaries are not implemented.
- Why deferred: those boundaries require the async suspension state machine and
  external wake sources; the bounded executor has one deterministic scheduler
  linearization point.
- Progress: `runtime/tests/task_cancellation.c` proves request acceptance,
  cancellation acknowledgement after cleanup, completion-wins ordering, and
  identical joined/unjoined cancellation outcomes under ASAN/UBSAN.
- Next step: extend cancellation checks to generated suspension, file/network
  operations, and handler frames once their wake/capture ownership exists.

### File I/O has no scheduler suspension yet (IO2, 2026-07-22)

- Area: runtime file operations
- Symptom: `AuraFile` provides bounded, status-based POSIX descriptor calls,
  but regular-file operations can still block in the host kernel and are not
  registered with an Aura async executor.
- Why deferred: the runtime has no completed A4–A8 suspension state machine,
  cancellation wakeup, or GC frame-root contract to safely park a file
  operation.
- Progress: open/read/write/flush/close/destroy own descriptors explicitly,
  borrow buffers only per call, classify permission/pending/EOF/closed/error,
  and are covered by `runtime/tests/file_io.c` under ASAN/UBSAN.
- Next step: integrate file handles with the async operation frame and define
  cancellation/GC cleanup before checking the remaining IO2 items.

### IO4 native operation adapters remain open (2026-07-22)

- Area: async file/TCP cancellation and cleanup
- Symptom: `AuraTaskFrame` now provides an exactly-once cleanup hook for a
  registered pending resource, but `AuraFile` and `AuraTcpStream` do not yet
  register operations with the executor or expose readiness completion/wake
  sources.
- Why deferred: the current POSIX APIs perform bounded synchronous calls (or
  return `PENDING` for a zero-timeout probe), so claiming disconnect cleanup or
  scheduler-wide failure wakeup would overstate the ABI.
- Progress: `runtime/tests/task_io_cleanup_sanitizer.c` proves file and TCP
  descriptor cleanup on cancellation, failure, and forced executor shutdown
  under ASAN/UBSAN.
- Next step: define an operation handle and readiness/event registration after
  the full A4–A8 suspension contract is available; connect disconnect and
  failure completion to executor wake before closing the remaining IO4 items.

### Foreign symbol lowering deferred to F2 (2026-07-22)

- F1 validates and preserves explicit foreign declarations, but does not add
  foreign symbols to the callable Aura signature table or emit/link them.
- This is intentional: primitive call ABI lowering, ownership, and missing
  symbol behavior belong to F2+ and must not be inferred from declaration
  syntax alone.

> **Last closed batch:** [S2](../docs/plans/2026-07-21-s2-production-toolchain.md) (2026-07-21). Residual open items below.

### HTTP H3 remains transport-independent (2026-07-22)

- Area: runtime HTTP response builder
- Exclusion: H3 does not write sockets, run request/response loops, derive
  keep-alive from parsed requests, emit server diagnostics, or provide chunked
  framing, compression, HTTP/2, or TLS.
- Progress: bounded owned response state, strict header/status validation,
  deterministic HTTP/1.1 serialization, and stable 400/405/413/500 JSON error
  bodies are implemented and covered by `runtime/tests/http_response.c`.
- Next step: H4+ must define lifecycle and diagnostic integration before this
  builder is used by a server; the builder defaults to `Connection: close`.

### HTTP H4 remains synchronous and callback-only (2026-07-22)

- Area: runtime HTTP connection lifecycle
- Symptom: H4 provides a bounded blocking request/response loop and an opaque
  callback, but does not suspend tasks, route methods/paths, or serve multiple
  connections concurrently.
- Why deferred: async suspension and ownership across awaits belong to H5;
  handler/routing API belongs to H6. The callback intentionally avoids making
  either contract implicit.
- Progress: TCP partial reads/writes, idle/read/write timeouts, peer close,
  keep-alive, request limits, active-connection limits, and graceful listener
  shutdown are covered by `runtime/tests/http_connection.c`.
- Next step: implement H5 async integration and H6 handler/routing only after
  their dependency contracts are frozen.

### HTTP hardening remains bounded native coverage (H8, 2026-07-22)

- Area: HTTP hostile-input and lifecycle acceptance
- Symptom: the hardening fixture covers oversized input, malformed framing,
  partial-client timeout, connection limits, and forced shutdown, but does not
  provide fuzzing, async suspension, cross-host results, or routing coverage.
- Why deferred: those behaviors belong to the fuzz/release matrix, H5 async
  integration, and H6 handler/routing API respectively; H4 is intentionally
  synchronous.
- Progress: `runtime/tests/http_hardening.c` runs the bounded checks over the
  existing parser and connection APIs under strict ASAN/UBSAN (and LSAN when
  supported).
- Next step: keep the H6 routing checklist open and extend H8 with fuzz and
  supported-host acceptance evidence after the dependent work lands.

### Async suspension GC roots and ownership (C22s, 2026-07-22)

- Area: async/task runtime and codegen
- Symptom: the C22 task frame owns opaque `data` bytes but has no GC mark hook; captured heap-class references cannot safely survive a future `await` suspension. C22o channel payloads are currently safe only because class values use a temporary GC-rooted box and `Int`/`String` values transfer their malloc ownership to the receiver.
- Why deferred: C22l state-machine/capture lowering and the corresponding frame-root contract are not implemented; the shipped slice supports no-await tasks and empty `spawn {}` only.
- Progress: frame captures, pending operations, results, and errors now have explicit ownership metadata, GC root registration, borrowed-value rejection, and exactly-once release. The compiler already rejects borrowed values crossing await/spawn/channel boundaries.
- Next step: add an explicit frame-data mark/drop contract for typed locals/captures and enable non-empty async bodies after A4–A6 state lowering.

### Async lowering and task outcome gaps (C22t, 2026-07-22)

- Area: async/task codegen and runtime outcomes
- Symptom: `await` parses and type-checks but has no lowered suspension state machine; only empty `spawn {}` bodies execute; task failure propagation is not complete.
- Progress: no-await code generation now emits deterministic `resume_state` transitions; immediate `await` polls a ready frame and returns its completed result; repeated polling is covered by runtime fixtures; A1–A3 define the frame ABI, ownership classes, roots, and error storage.
- Next step: lower await points with captured-local storage, implement non-empty spawn capture/drop, and define end-to-end success/failure/cancellation propagation before advertising the full C22 contract as executable.

### S4 source locations and nested failures remain bounded (2026-07-22)

- Area: runtime task failure outcomes
- Symptom: the bounded executor retains a numeric source identity with each
  failure, but does not yet carry file/line/column metadata or nested exception
  chains through compiler-generated async frames.
- Why deferred: those fields depend on the A4–A7 suspension/state-machine and
  diagnostic payload contracts; inventing a second source-location format in
  the C-only runtime would not prove end-to-end propagation.
- Progress: failed joins preserve payload and source ID across repeated
  observation; result/error slots clear before GC-root removal and user cleanup,
  and terminal release is covered by `runtime/tests/task_join.c`.
- Next step: extend the typed compiler outcome ABI when async lowering defines
  source spans and nested failure representation.

### Executor-owned non-terminal handle drop (S3, 2026-07-22)

- Area: bounded task executor ownership
- Symptom: `aura_task_executor_release` safely releases only terminal frames;
  a ready or pending dropped handle remains executor-owned until cancellation
  or shutdown.
- Why deferred: releasing a live frame would race its queue/waiter links and
  requires the cancellation and suspension lifecycle from S4/S5.
- Next step: define live-handle drop semantics together with cancellation and
  waiter unlinking; do not infer them from terminal release.

### S3 release rehearsal external blockers

- Area: production release / S3.2 + S3.6
- Symptom: this offline rehearsal can exercise only the current host's native
  target. macOS amd64/arm64 and Linux amd64 each still need a matching clean
  host run; a cross-compiled archive is not treated as a runtime pass.
- Blocker: published installer smoke requires the release URL, CDN availability,
  GitHub release assets, and credentials/permissions outside this repository.
- Next step: on each supported clean host, run `bash scripts/install-smoke.sh
--from-release` against the frozen release, then record the URL, target, and
  checksum result. Keep failed/interrupted-install evidence with the release
  ticket; the offline script only proves failed archive verification preserves
  the active `current` link.

### C22 release work deferred (C22t, 2026-07-22)

- Area: release / publication
- Symptom: C22t records implementation status only; no new release rehearsal, signing, publication, or cross-target artifact work was performed.
- Why deferred: release execution is outside the C22 scope and requires an explicit release request plus external hosts, credentials, and distribution services.
- Next step: create a separate release task after await/capture/failure gaps are resolved and run the supported-target acceptance matrix.

### Lambda capture limits (MVP)

- Area: language / lambdas (C10h/C12k/C12l/C12m/C13e/C13f/C13g)
- Symptom: `var` class/Array/Fun capture has only an MVP box/lowering contract; Array views do not have borrow-checked lifetime safety (val Fun + var Int/Bool/String remain supported)
- Why deferred: full Array ownership needs a borrow/lifetime contract; owner movement, escaping live views, and mutation invalidation are not yet specified
- Progress: C20c–e add shared pointer boxes and codegen lowering for mutable class/Array/Fun captures; class payloads are GC-rooted, nested Fun environments retain/release, and corpus covers mutation, rebinding, escaping closures, and GC churn. Existing env `__drop` still unregisters class roots / releases boxes / nested Fun envs then frees (never frees Array buffers)
- Note (C12l): Array capture is a non-owning `{data,len,cap}` view (like field bind). Freeing/moving the outer Array owner while Fun is still live is **undefined**
- Note (C12m/C13f): `var` Int/Bool/String uses `aura_box_*` (refcount); String box owns heap copy (`set` frees previous); outer + each capturing env retain; multiple lambdas share mutations; escaping Fun keeps the box alive
- Note (C13g): Fun param transfer moves env (caller must not call after pass); nested retain via capture keeps both live — stress corpus documents both
- Next step: define true borrow/lifetime rules for Array capture and owner movement before strengthening the MVP
- Note: C12 batch closed (C12t); C13e Fun + C13f var String + C13g stress audit shipped; C20c–e mutable class/Array/Fun MVP shipped — residual is the Array ownership contract
- Introduced: narrowed after C10h; env free 2026-07-20; class C12k 2026-07-21; Array view C12l 2026-07-21; var Int/Bool C12m 2026-07-21; Fun C13e 2026-07-21; var String C13f 2026-07-21; stress C13g 2026-07-21; mutable class/Array/Fun MVP C20c–e 2026-07-22

### Array field return still moves (no true borrow type)

- Area: builtin Array (C7c/C8j)
- Symptom: `return this.items` still moves buffer out of the object; bind/assign from field is non-owning view (C8j)
- Why deferred: no `ref`/`borrow` type in the language; shallow view is enough for field reads
- Progress: C9c `Array.clone()` owning copy as escape hatch for field returns
- Next step: true borrow type if needed
- Introduced: narrowed after C8j; clone C9c

### Registry publishing and alternate dependency sources

- Area: toolchain / RFC-005
- Symptom: `aura publish`, registry authentication, and `github=`/`git=` dependency sources are not implemented.
- Why deferred: the S2 release contract covers consuming locked packages; hosting,
  accounts, and publishing require an external registry API decision.
- Progress: lock schema, semver pinning, SHA-256 verification, HTTPS metadata and
  archive downloads, nested registry resolution, atomic cache publication, and
  production acceptance coverage are complete.
- Next step: define the registry API/authentication contract before implementing
  `aura publish` or alternate dependency sources.
- Introduced: narrowed after C3p; HTTPS/nested registry work completed in S2

### Publish signing and dependency resolution in dry-run (U4, 2026-07-22)

- Area: package publication preview
- Symptom: `aura publish --dry-run` previews an unsigned archive and validates
  registry dependencies only from an existing local lock pin; it does not sign,
  resolve, fetch, or upload.
- Why deferred: signing keys/policy and the U5 registry upload protocol are not
  yet defined; keeping the preview read-only prevents false release claims.
- Next step: add a signing primitive/key policy and U5 upload orchestration,
  then extend the preview to verify the exact publish metadata end to end.

### Array of interface elements

- Area: builtin Array
- Symptom: interface elements rejected (C4x/C7h message); enum/class/struct/prim/Array OK
- Decision (C7h): **reject for MVP** — no `Array<I>` until a stable elem layout exists
- Why: interface values are closed-world fat/tag unions; Array mono needs fixed elem size today
- Next step (post-MVP): erase to fat pointer `(tag, data*)` or box each element as a class
- Introduced: narrowed after C6g; decision locked C7h

### Stdlib collections polish

- Area: stdlib / RFC-007
- Symptom: no live iterator/entry view or mutation-through-entry API; `Map`/`Set` remain linear alternatives.
- Why deferred: C20 defines and ships deterministic read-only snapshots; borrowed/live aliases need lifetime checking and mutation invalidation rules.
- Progress: C9b auto-resize; C12n String→String; C12o String HOF; **C14** generic `HashMap<K,V>`; **C15** generic `HashSet<T>`; **C16** generic `map`/`filter`/`fold`; **C17** user-defined class HOF coverage; **C18** hash snapshots/HOFs; **C19a–d** accessors/entry snapshots/entry `for-in`; **C20f–g** snapshot contract and read-only iterator snapshots; **C20i** explicitly defers mutation-through-entry. Compiler prerequisites **C19x** generic constructor substitution and **C19y** nested generic return/local substitution are resolved.
- Limitation: `entries()` is a fresh shallow structural snapshot in logical table order. It preserves key/value pairing and never mutates the source map, but it is not live and entries cannot mutate the map.
- Next step: borrow/lifetime design before any live iterator, entry view, or mutation API.
- Note: C14/C15 resolved the generic hash-collection residual; C19 resolved the nested generic codegen blockers exposed by entry snapshots.
- Introduced: narrowed after C8i; resize C9b; String→String C12n; String HOF C12o

## Resolved

### Generic class construction inside generic bodies (C19x, 2026-07-22)

- Resolved: codegen now substitutes generic function/method type arguments
  before emitting generic class constructor symbols, including alias-qualified
  constructors. Corpus `generic/constructor_subst.aura` covers both a generic
  function and a generic-class method returning concrete `Pair` monomorphs.

### C16 generic HOF compiler support (2026-07-21)

- Resolved: sema accepts generic function parameters such as `(T) -> R`; codegen skips open generic `Fun<T, R>` typedefs and emits only concrete monomorphs, allowing generic `map`/`filter`/`fold` implementations to compile.

### C16 generic HOF stdlib coverage (2026-07-21)

- Resolved: `std.collections` generic `map<T,R>`, `filter<T>`, and `fold<T,A>` are exercised end-to-end by corpus packages for `Array<Int>` and `Array<String>`.
- Extended by C17: generic HOF codegen is exercised end-to-end with `Box<Int>` as both element and accumulator.

### C17 generic HOF user-defined class coverage (2026-07-21)

- Resolved: generic `map<T,R>`, `filter<T>`, and `fold<T,A>` compile and run with a generic heap class `Box<Int>` in `Array<Box<Int>>`.
- Coverage remains focused on closed monomorphs; interface elements and richer nested generic layouts remain separate work.

### C14 generic HashMap (2026-07-21)

- Resolved: compiler-backed `Hashable` for `Int`/`String`, generic open-addressing `HashMap<K,V>`, compatibility factories, Int-key corpus, and collection docs.

### C15 generic HashSet (2026-07-21)

- Resolved: generic open-addressing `HashSet<T : Hashable>` backed by `HashMap<T, Bool>`, String factory, Int-key corpus, iteration API, and collection docs.

### C18 generic hash-collection HOFs (2026-07-21)

- Resolved: `HashMap.keyArray`/`valueArray`, `HashSet.toArray`, and generic free functions `map_hash_map_values`, `filter_hash_set`, and `map_hash_set`; Int and String runtime corpus coverage added.
- Limitation: Aura methods cannot declare their own type parameters (C2b), so HOFs use explicit free-function names and return arrays rather than entry tuples or new collections.

### C13 batch (2026-07-21)

- Resolved C13a–t: method-on-temp; `Int.toString` + String↔Int `+`; Array\<String\> elem free; Fun + `var` String captures + stress; capture reject diags; registry K1 offline (index/semver/fetch/build); `toLower`/`toUpper`; eprint corpus; `tryWriteFile`; Hashable spike; `examples/wc` polish; signing design note; docs close.
- Residual: registry publishing/authentication; stdlib generic HOF API; true borrow;
  `var` class/Array/Fun.

### Process argv string ownership (`Io.args`) — S1.1

- Resolved: `aura_args_get` now returns a heap-allocated copy for each process argument, matching `Array<String>` element destruction.
- Regression: `aura-cli` builds and executes `corpus/std_io/args` with forwarded arguments and verifies successful teardown.
- Resolved: 2026-07-21

### Chained method on `Array.get` temporary (codegen) — C13b / C13q

- Resolved: method-on-temp for call-result receivers; `examples/wc` uses `segs.get(j).trim()` and `argv.get(i).trim().toInt()` without intermediate binds.

### No std Int→String (CLI print) — C13c / C13q

- Resolved: builtin `Int.toString()` (+ String/Int `+`); `examples/wc` prints counts with `.toString()` (local `u64ToString` removed).

### Array element drop for String (C13d)

- Resolved: free owned `const char *` elems on Array\<String\> drop/clear/set; push/set heap-copy. Residual: process argv arrays (see open debt).

### C12 post-alpha batch (2026-07-21)

- Resolved C12a–t: process argv/stdin/exit; String `indexOf`/`split`/`trim*`/`toInt`; `join`; lambda class/Array/`var` Int·Bool captures; HashMapStr; String HOF; `tryReadFile`; `examples/wc`; guide/corpus/install smoke; batch close. Residual open debts (Fun capture, generic HashMap, String free, method-on-temp, Int→String, registry, borrow, Array&lt;I&gt;) unchanged in scope.

### Higher-order Int array helpers (2026-07-20)

- Resolved in C10i: `std.collections` `map_ints` / `filter_ints` / `fold_ints`; corpus `fun/lambda_hof.aura`, `std_collections/hof`.

### Higher-order String array helpers (2026-07-21)

- Resolved in C12o: `std.collections` `map_strings` / `filter_strings` / `fold_strings`; corpus `std_collections/hof_str`.

### Generic collection higher-order helpers (2026-07-21)

- Resolved in C16: `std.collections` now exposes generic `map<T,R>`, `filter<T>`, and `fold<T,A>`; the Int/String helpers remain compatibility wrappers. The old zero-argument `map()` factory was renamed to `map_string_int()` because Aura does not yet support overloads by arity. Generic HOFs over arbitrary user-defined element types still depend on broader generic codegen coverage.

### Soft file read `tryReadFile` (2026-07-21)

- Resolved in C12p: `std.io.tryReadFile(path): String?` (null on missing/error); throwing `readFile` kept; runtime `aura_try_read_file`; corpus `std_io/try_read_file`. Full `Result` I/O still deferred.

### C10 first-class funs batch (2026-07-20)

- Resolved C10a–j: diagnostics polish, lambdas (expr/block), fun types, val captures (MVP), HOF helpers. Remaining: richer captures / env GC (see open debt).

### Generic class implements interface (2026-07-20)

- Resolved in C9a: `class Box<T> : Boxable<T>`; open implements type args; class mono subst for assignability; codegen tags/upcast/dispatch for mono implementors. Corpus `iface/generic_class_impl.aura`.

### Generic `Iterable<E>` implements (2026-07-20)

- Resolved in C8c/C8d: `implements TypeRef` with args; `Ty::InterfaceApp`; method subst; mono iface codegen; `std.collections.Iterable<E>`; for-in.

### Nested Array mono + element free (2026-07-20)

- Resolved in C8e/C8f: nested `Array<Array<T>>` mono order; free nested buffers on drop/clear/set.

### Generic Set + for-in collections (2026-07-20)

- Resolved in C8g/C8h: `Set<T>`; `Set.get(i)` duck for-in; `for (k in map.keys)`.

### HashMap String→Int (2026-07-20)

- Resolved in C8i: open addressing + `hash_string`; `hash_map()` capacity 16.

### Array field non-destructive bind (2026-07-20)

- Resolved in C8j: bind/assign from field is view; return still moves (C7c).

### Lock registry schema v0 (2026-07-20)

- Resolved in C8k: parse `version`/`source`/`checksum` inline tables; no fetch yet.

### Nullable primitive `Int?` / `Bool?` C emit (2026-07-20)

- Resolved in C7a: `aura_opt_i64` / `aura_opt_bool` tagged structs; null/wrap/coerce; `== null` via `.has`; `!!` / `?:`; Map.get returns `Int?`. Corpus `types/opt_prim.aura`.

### GC mark / free Array fields (2026-07-20)

- Resolved in C7b: `aura_gc_alloc_full` + per-class `dtor` (free Array buffers on sweep/shutdown) and `mark_extras` (mark Array-of-class field elems via `aura_gc_mark_ptr`). Corpus `class/gc_array_field.aura`.

### Multi-error collect deferred (2026-07-20)

- Resolved in C6h: body statements keep typechecking after an error; `SemaErrors` + CLI prints all. Corpus `diag/multi_error.aura`.
- C7g: declaration phase also collects (continue next decl); corpus `diag/multi_decl.aura`.

### Array fields shallow-copy on ctor/assign (2026-07-20)

- Resolved in C6i (partial): constructor and `var` field assign move from owner locals/params (zero source); reassign frees prior field buffer. Corpus `generic/array_field_move.aura`.

### GC mark does not walk Array-of-class locals (2026-07-20)

- Resolved in C6e (partial): `aura_gc_add_array_root` on Array-of-class locals/params; collect marks `data[0..len)`. Corpus `class/gc_array.aura`.

### Shallow GC mark only (2026-07-20)

- Resolved in C6a: store alloc size; worklist deep scan of pointer-sized slots in marked objects. Corpus `class/gc_deep.aura`.

### Array params not owners (2026-07-20)

- Resolved in C6b (partial): Array params own buffer; call site moves from owner idents. Corpus `generic/array_param_move.aura`.

### Array return binding not owner (2026-07-20)

- Resolved in C6d: `val b = f()` / assign from call that yields Array marks binding owner; free old on reassignment. Corpus `generic/array_return_own.aura`.

### No std.collections Map (2026-07-20)

- Resolved in C6f (partial): `Map` String→Int linear + `map()`; later C8a generic Map.

### `for-in` has no Iterable protocol (duck only) (2026-07-20)

- Resolved in C6c (partial): `for-in` on interface with `len(): Int` + `get(Int): E`; duck class path kept. Generic Iterable: C8d.

### Alpha target capability probing (2026-07-22)

- P6 now rejects native builds outside the published Linux/macOS target matrix
  and reports supported alternatives. Explicit cross targets, sysroot
  discovery, and system-library/linker probing remain deferred until target
  descriptors are modeled in `CompileOptions`.

### Alpha race instrumentation (2026-07-22)

- R3 now emits source-IDed local read/write hooks and source-tagged task,
  await, join, and channel boundaries in detector-enabled profiles. The
  runtime still records events without conflict suppression or stable report
  formatting; vector-clock refinement and actionable diagnostics remain R4.

### Async I/O suspension (2026-07-22)

- Await now resumes pending frames that are not blocked on a channel/I/O waiter
  by re-queueing them through the deterministic executor. Waiter-driven wakeup,
  live-local hoisting, and full async I/O continuation remain deferred.

### Registry archive publication wiring (2026-07-22)

- U1 now provides a deterministic gzip/tar archive primitive and SHA-256 helper,
  but no `publish`/dry-run CLI command consumes it yet. Next step: wire manifest
  and dependency validation plus upload preview/orchestration before claiming U4.

### String-return ownership metadata (2026-07-22)

- Codegen now frees only known allocating `String` call results and treats
  unknown/user/generic `String` returns as borrowed to avoid invalid frees. This
  can retain allocations longer than necessary. Next step: propagate explicit
  return ownership metadata through sema and call instantiations.

### Registry upload production compatibility (2026-07-22)

- U5 uses the frozen `/api/v1/publish` fixture contract and does not claim
  compatibility with an external production registry. Next step: standardize
  a signed, server-defined publish protocol before replacing this endpoint.

### Registry update activation deferred (U6, 2026-07-22)

- U6 performs metadata-only compatibility discovery and never downloads or
  activates a candidate. Signature verification, atomic replacement, rollback,
  and executable handoff remain U7 by dependency design.

### U8 cross-host release acceptance (2026-07-22)

- The deterministic release-integration fixture now covers publish, install,
  checksum verification, discovery, activation, rollback, and execution on
  native Linux. A native macOS run is still required before claiming macOS
  execution evidence; the fixture intentionally does not emulate another host.

### F2 foreign failure and search-path integration (2026-07-22)

- Primitive foreign calls now lower and link against explicit C libraries on
  the native Linux/macOS matrix. Missing symbols are still reported by the C
  linker rather than mapped to an Aura Result/error outcome, and package
  manifests do not yet expose foreign library search paths; the next step is
  F5 failure mapping plus a target-aware package linker configuration.

### F3 structured FFI values remain bounded (2026-07-22)

- The F3 ABI supports only malloc-backed String values and primitive byte,
  `int64_t`, and one-byte boolean arrays. String-element deep copy, arbitrary
  element destructors, pointers, callbacks, and foreign failure mapping remain
  deferred to F4/F5; the root guard is synchronous-only by contract.

### F3 structured foreign values (2026-07-22)

- F3 freezes an allocation-only C surface for borrowed/copied/transferred
  strings and primitive arrays, with synchronous GC root guards. String-element
  arrays, arbitrary destructors, pointers, callbacks, and async retention are
  intentionally deferred to F4/F5; extend the declaration model only after
  those lifetimes have a complete contract.

### F4 opaque foreign handles (2026-07-22)

- F4 provides a synchronous, tombstoned opaque-handle ABI with deferred
  destruction while pinned. Task, await, channel, and callback crossings are
  rejected. Aura-level pointer types and automatic handle rooting across an
  asynchronous boundary remain deferred; callbacks and foreign error mapping
  belong to F5.

### F5 callback portability (2026-07-22)

- F5 provides a single-threaded synchronous callback ABI with explicit
  environment ownership, frame retention, affinity checks, and bounded integer
  error mapping. Concurrent foreign-thread delivery, host-specific callback
  trampolines, cancellation resumption, and exception-object translation remain
  deferred to cross-host acceptance work; next step is to extend the ABI only
  after scheduler and target descriptors define those semantics.

### Race report command integration (2026-07-22)

- R4 now has deterministic runtime reports and planted-race/suppression/
  synchronization fixtures; the user-facing race command, exit policy, and
  release/profile acceptance remain R5.

### Race CLI bounded evidence (2026-07-22)

- R5 adds `aura race` and a deterministic regression script over the bounded
  single-threaded report fixture. The command reports detector-enabled test
  outcomes; it does not yet stream runtime reports from arbitrary application
  binaries or provide concurrent vector-clock diagnostics. Those remain
  deferred until the runtime exposes a process-level report handoff.

### A4 async lowering boundary (2026-07-22)

- The compiler now exposes deterministic `await` suspension-point IDs and
  source-span metadata, but does not hoist live locals or generate executable
  resume edges. Those require the A5/A6 frame and runtime dependencies.
